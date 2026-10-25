//! Main editor view coordinating panes, input, RPC, and rendering.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use crate::focus::FocusStack;
use crate::theme::Theme;
use crate::ui;

mod commands;
mod input;
mod plugin;
mod render;
mod rpc;

use gpui::{Context, FocusHandle, KeyDownEvent, ScrollWheelEvent, SharedString, Window, px};
use gpui_base::VirtualListScrollHandle;
use vy_app::commands::{CommandRegistry, Invocation};
use vy_app::config::Config;
use vy_app::connection::{ConnectionSpec, Startup};
use vy_app::editor::{Editor, Key, Mode, ViewState};
use vy_app::ex::{ExHistory, ExState};
use vy_app::keymap::{KeyContext, Keymap, KeymapOutcome};
use vy_app::palette::{PaletteMode, PaletteState};
use vy_app::paths::Paths;
use vy_app::plugin::PluginManager;
use vy_app::plugin_host::{self, PluginHost};
use vy_app::rpc::Client;
use vy_app::slots::{GutterRegistry, HoverState};
use vy_app::state::{DocumentState, PaneState};
use vy_app::statusline::Statusline;
use vy_app::text_layout::scroll_lines;
use vy_app::workspace::{document_uri, workspace_info_for};
use vy_core::{LayoutNode, PaneId};
use vy_js::RemoteEnv;

/// The kind of request we are waiting for a response to.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Pending {
    Initialize,
    Layout,
    Sync,
    Open,
    Snapshot,
    Undo,
    Redo,
    Close,
    Focus,
    EditorOpen,
    Save,
    HostCall {
        request: vy_js::HostRequest,
        runtime: Option<String>,
    },
}

/// One plugin runtime and the generation used to stop its old pumps.
pub(super) struct PluginRuntime {
    pub(super) host: PluginHost,
    pub(super) generation: u64,
    pub(super) started: bool,
}

pub struct EditorView {
    client: Option<Client>,
    /// Bumped for every connection attempt. Old reader tasks ignore messages.
    connection_generation: u64,
    /// Whether a reconnect loop is already running.
    reconnecting: bool,
    /// Set by `vy.reconnect_server`; the connection watcher performs it.
    reconnect_requested: bool,
    pending: HashMap<u64, Pending>,
    config: Config,
    paths: Paths,
    connection: ConnectionSpec,
    connection_name: String,
    /// URI of the workspace on the core. For remote connections this is not a
    /// path on this machine.
    workspace_uri: String,
    /// Local path used for project settings. `None` for remote connections.
    local_workspace: Option<PathBuf>,
    settings_stamp: Option<SystemTime>,
    keymap: Keymap,
    commands: CommandRegistry,
    /// Manages discovered external plugins and capability approval.
    plugin_manager: PluginManager,
    /// The script thread and `init.ts` bookkeeping.
    plugin: Option<PluginHost>,
    /// Script runtimes for active plugins, keyed by plugin id.
    plugin_runtimes: HashMap<String, PluginRuntime>,
    /// Bumped for every plugin runtime so old pumps stop.
    next_plugin_generation: u64,
    /// Bumped whenever the script thread is replaced, so old pumps stop.
    script_generation: u64,
    /// Whether `init.ts` was evaluated since the last document load.
    init_evaluated: bool,
    /// URI of the document opened at startup.
    initial_uri: String,
    /// Whether the startup file still needs to be opened after a restored layout.
    open_initial_after_restore: bool,
    window_id: u64,
    layout: Option<LayoutNode>,
    layout_revision: u64,
    active_pane: Option<PaneId>,
    panes: HashMap<PaneId, PaneState>,
    documents: HashMap<u64, DocumentState>,
    /// Used when no pane has a document yet, so drawing has a valid editor.
    fallback: DocumentState,
    /// Open command palette, if any.
    pub(crate) palette: Option<PaletteState>,
    /// Commands run from the palette, most recent first. Session-local.
    recent_commands: Vec<String>,
    /// Open Ex command line, if any.
    ex: Option<ExState>,
    ex_history: ExHistory,
    /// Commands waiting to run. Only one command runs at a time.
    command_queue: VecDeque<Invocation>,
    /// The script command that is currently running, if any.
    running_command: Option<String>,
    pub focus_handle: FocusHandle,
    pub palette_focus_handle: FocusHandle,
    pub palette_scroll_handle: VirtualListScrollHandle,
    pub ex_focus_handle: FocusHandle,
    pub focus_stack: FocusStack,
    status: SharedString,
    frame_log: bool,
    last_frame: Option<Instant>,
    /// Statusline slots registered by scripts or default.
    statusline: Statusline,
    /// Gutter marks registered by scripts/plugins.
    gutter: GutterRegistry,
    /// Width of one character measured from font or approximate default.
    pub(crate) char_width: f32,
    /// Hover popup currently displayed, if any.
    hover: Option<HoverState>,
    last_reported_document_id: Option<u64>,
    last_reported_selections: Option<Vec<vy_core::Selection>>,
    pub(super) pending_view_for_next_open: Option<String>,
    pub(super) view_widget_trees: HashMap<String, vy_app::widget::WidgetNode>,
    /// Which runtime spawned each process, so stream events go to its owner.
    pub(super) stream_owners: HashMap<u32, Option<String>>,
    pub(super) cursor_visible: bool,
    pub(super) status_generation: usize,
    pub(crate) config_arc: std::sync::Arc<Config>,
    pub(crate) theme_arc: std::sync::Arc<Theme>,
}

impl EditorView {
    pub fn new(cx: &mut Context<Self>, startup: Startup) -> Self {
        let paths = Paths::from_env();
        let workspace = startup.local_workspace.clone();
        let (config, warnings) = Config::load(&paths, workspace.as_deref());
        let theme = if config.theme() == "light" {
            Theme::light()
        } else {
            Theme::dark()
        };
        cx.set_global(theme.clone());
        let config_arc = std::sync::Arc::new(config.clone());
        let theme_arc = std::sync::Arc::new(theme);
        let settings_stamp = paths.settings_stamp(workspace.as_deref());
        for warning in &warnings {
            eprintln!("vy: settings: {warning}");
        }

        let keymap = match Keymap::load_default(Duration::from_millis(config.sequence_timeout_ms()))
        {
            Ok(keymap) => keymap,
            Err(error) => {
                eprintln!("vy: {error}");
                Keymap::new(Duration::from_millis(config.sequence_timeout_ms()))
            }
        };

        let roots =
            vy_capabilities::Roots::new(startup.workspace_path.clone(), startup.home.clone());
        let mut plugin_manager = PluginManager::new(paths.clone(), roots);
        let plugin_warnings = plugin_manager.scan();
        for warning in &plugin_warnings {
            eprintln!("vy: plugins: {warning}");
        }

        let initial_palette = plugin_manager.next_approval_request().map(|req| {
            PaletteState::with_mode(vy_app::palette::PaletteMode::Approval {
                plugin_id: req.plugin_id.clone(),
                plugin_name: req.plugin_name.clone(),
                plugin_version: req.plugin_version.clone(),
                missing: req.missing_capabilities.clone(),
            })
        });

        let uri = document_uri(
            startup.initial_file.as_deref(),
            Some(&startup.workspace_uri),
            !startup.connection.is_remote(),
        );
        let workspace_info = workspace_info_for(Some(startup.workspace_uri.clone()));
        let remote = remote_env(&startup);
        let (env, trusted_warnings) =
            plugin_host::script_env_with_remote(&workspace_info, &paths, remote);
        for warning in &trusted_warnings {
            eprintln!("vy: trusted.json: {warning}");
        }
        let plugin = Some(PluginHost::spawn(env, paths.init_ts()));

        let mut fallback = DocumentState::new(0, "vy:empty".to_string(), "", 0);
        fallback
            .editor
            .set_transaction_idle(Duration::from_millis(config.transaction_idle_ms()));

        let char_width = ui::pane::character_width(&config);

        let mut view = Self {
            client: None,
            connection_generation: 0,
            reconnecting: false,
            reconnect_requested: false,
            pending: HashMap::new(),
            config,
            paths,
            connection: startup.connection.clone(),
            connection_name: startup.connection_name.clone(),
            workspace_uri: startup.workspace_uri.clone(),
            local_workspace: startup.local_workspace.clone(),
            settings_stamp,
            keymap,
            commands: CommandRegistry::new(),
            plugin_manager,
            plugin,
            plugin_runtimes: HashMap::new(),
            next_plugin_generation: 0,
            script_generation: 0,
            init_evaluated: false,
            initial_uri: uri,
            open_initial_after_restore: startup.initial_file.is_some(),
            window_id: 1,
            layout: None,
            layout_revision: 0,
            active_pane: None,
            panes: HashMap::new(),
            documents: HashMap::new(),
            fallback,
            palette: initial_palette,
            recent_commands: Vec::new(),
            ex: None,
            ex_history: ExHistory::new(),
            command_queue: VecDeque::new(),
            running_command: None,
            focus_handle: cx.focus_handle(),
            palette_focus_handle: cx.focus_handle(),
            palette_scroll_handle: VirtualListScrollHandle::new(),
            ex_focus_handle: cx.focus_handle(),
            focus_stack: FocusStack::new(),
            status: "starting vy-server".into(),
            frame_log: std::env::var_os("VY_FRAME_LOG").is_some(),
            last_frame: None,
            char_width,
            statusline: Statusline::new(),
            gutter: GutterRegistry::new(),
            hover: None,
            last_reported_document_id: None,
            last_reported_selections: None,
            pending_view_for_next_open: None,
            view_widget_trees: HashMap::new(),
            stream_owners: HashMap::new(),
            cursor_visible: true,
            status_generation: 0,
            config_arc,
            theme_arc,
        };

        if let Some(warning) = warnings.last() {
            view.status = format!("settings: {warning}").into();
        }
        view.connect(cx);
        view.start_config_watch(cx);
        view.start_script(cx);
        view.start_plugin_supervisor(cx);
        view.start_cursor_blink(cx);
        view
    }

    fn workspace_info(&self) -> vy_app::plugin_host::WorkspaceInfo {
        workspace_info_for(Some(self.workspace_uri.clone()))
    }

    fn remote_env(&self) -> RemoteEnv {
        remote_env_for(&self.connection, &self.connection_name)
    }

    fn active_pane_state(&self) -> Option<&PaneState> {
        self.active_pane.and_then(|id| self.panes.get(&id))
    }

    fn active_pane_state_mut(&mut self) -> Option<&mut PaneState> {
        let id = self.active_pane?;
        self.panes.get_mut(&id)
    }

    fn active_document_id(&self) -> Option<u64> {
        self.active_pane_state()?.document_id
    }

    fn active_document(&self) -> Option<&DocumentState> {
        self.active_document_id()
            .and_then(|id| self.documents.get(&id))
    }

    fn active_document_mut(&mut self) -> Option<&mut DocumentState> {
        let id = self.active_document_id()?;
        self.documents.get_mut(&id)
    }

    /// The active editor, or a blank one when no pane is ready yet.
    fn editor(&self) -> &Editor {
        self.active_document()
            .map(|document| &document.editor)
            .unwrap_or(&self.fallback.editor)
    }

    fn editor_mut(&mut self) -> &mut Editor {
        let id = self.active_document_id();
        if let Some(id) = id
            && let Some(document) = self.documents.get_mut(&id)
        {
            return &mut document.editor;
        }
        &mut self.fallback.editor
    }

    fn set_transaction_idle(&mut self, idle: Duration) {
        for document in self.documents.values_mut() {
            document.editor.set_transaction_idle(idle);
        }
        self.fallback.editor.set_transaction_idle(idle);
    }

    fn active_view(&self) -> Option<ViewState> {
        self.active_pane_state().map(|pane| pane.view)
    }

    /// Copies the active pane's cursor and mode into the document editor.
    ///
    /// The editor is the implementation used by commands, so only the active pane
    /// needs to be synchronized. Other panes keep drawing their stored state.
    fn activate_active_view(&mut self) {
        let Some(pane_id) = self.active_pane else {
            return;
        };
        let Some(pane) = self.panes.get(&pane_id) else {
            return;
        };
        let view = pane.view;
        let Some(document_id) = pane.document_id else {
            return;
        };
        let last_visual = view.mode().is_select().then(|| view.selection());

        let Some(document) = self.documents.get_mut(&document_id) else {
            return;
        };
        if let Some(range) = last_visual {
            document.editor.set_last_visual(range);
        }
        document.editor.set_view_state(view);
    }

    /// Copies the document editor's cursor and mode back into the active pane.
    fn store_active_view(&mut self) {
        let Some(pane_id) = self.active_pane else {
            return;
        };
        let Some(document_id) = self.panes.get(&pane_id).and_then(|pane| pane.document_id) else {
            return;
        };
        let Some(document) = self.documents.get(&document_id) else {
            return;
        };
        let view = document.editor.view_state();
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.view = view;
        }
    }

    fn switch_active_pane(&mut self, pane: Option<PaneId>) {
        if self.active_pane == pane {
            return;
        }
        self.store_active_view();
        self.active_pane = pane;
        self.activate_active_view();
    }

    fn clamp_views_for_document(&mut self, document_id: u64) {
        let Some(editor) = self
            .documents
            .get(&document_id)
            .map(|document| &document.editor)
        else {
            return;
        };
        let clamped: Vec<(PaneId, ViewState)> = self
            .panes
            .iter()
            .filter(|(_, pane)| pane.document_id == Some(document_id))
            .map(|(pane_id, pane)| (*pane_id, editor.clamp_view_state(pane.view)))
            .collect();

        for (pane_id, view) in clamped {
            if let Some(pane) = self.panes.get_mut(&pane_id) {
                pane.view = view;
            }
        }
    }

    /// Keeps other panes of the active document inside the text after a local edit.
    fn clamp_active_views(&mut self) {
        if let Some(document_id) = self.active_document_id() {
            self.clamp_views_for_document(document_id);
            self.activate_active_view();
        }
    }

    fn reset_views_for_document(&mut self, document_id: u64) {
        let view = self
            .documents
            .get(&document_id)
            .map(|document| document.editor.view_state())
            .unwrap_or_default();
        for pane in self.panes.values_mut() {
            if pane.document_id == Some(document_id) {
                pane.view = view;
                pane.preedit.clear();
                pane.preedit_selection = None;
                pane.scroll_top = 0;
                pane.scroll_left = 0;
                pane.scroll_sub_line = 0;
            }
        }
    }

    /// Starts listeners for the script thread. Events and host requests arrive on
    /// separate channels; neither blocks the UI.
    /// Re-reads settings when a file changes. `init.ts` reloads with the script host.
    fn start_config_watch(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;

                let updated = this.update(cx, |this, cx| {
                    if this.reconnect_requested {
                        this.reconnect_requested = false;
                        this.reconnect_now(cx);
                    }
                    let config_changed = this.reload_config_if_changed();
                    // Editing `init.ts` starts a fresh runtime, so registrations
                    // from the previous evaluation cannot pile up.
                    if this.plugin.as_ref().is_some_and(PluginHost::changed) {
                        this.reload_init(cx);
                    }
                    let reload_plugins: Vec<String> = this
                        .plugin_runtimes
                        .iter()
                        .filter(|(_, runtime)| runtime.host.changed())
                        .map(|(id, _)| id.clone())
                        .collect();
                    for plugin_id in reload_plugins {
                        this.reload_plugin(&plugin_id);
                    }
                    if config_changed {
                        let theme = if this.config.theme() == "light" {
                            Theme::light()
                        } else {
                            Theme::dark()
                        };
                        this.update_theme(theme, cx);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Updates the theme, storing an `Arc` for frame rendering and notifying GPUI.
    pub(super) fn update_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        if cx.global::<Theme>() != &theme {
            self.theme_arc = std::sync::Arc::new(theme.clone());
            cx.set_global(theme);
            cx.notify();
        }
    }

    /// Starts the cursor blinking loop.
    pub(super) fn start_cursor_blink(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(530))
                    .await;
                let updated = this.update(cx, |this, cx| {
                    this.cursor_visible = !this.cursor_visible;
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    /// Displays a transient notification status message and clears it after 4 seconds.
    pub fn show_notification(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = message.into();
        self.status_generation = self.status_generation.wrapping_add(1);
        let target_generation = self.status_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            let _ = this.update(cx, |this, cx| {
                if this.status_generation == target_generation {
                    this.status = "".into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Reloads the settings layers when the file timestamps changed.
    fn reload_config_if_changed(&mut self) -> bool {
        let stamp = self.paths.settings_stamp(self.local_workspace.as_deref());
        if stamp == self.settings_stamp {
            return false;
        }
        self.settings_stamp = stamp;

        let previous = self.config.clone();
        let warnings = self
            .config
            .reload(&self.paths, self.local_workspace.as_deref());
        for warning in &warnings {
            eprintln!("vy: settings: {warning}");
        }

        let idle = Duration::from_millis(self.config.transaction_idle_ms());
        let timeout = Duration::from_millis(self.config.sequence_timeout_ms());
        self.set_transaction_idle(idle);
        self.keymap.set_sequence_timeout(timeout);
        self.config_arc = std::sync::Arc::new(self.config.clone());

        let changed = self.config.changed_keys(&previous);
        if let Some(key) = changed.last().cloned() {
            self.status = format!("settings: {key} changed").into();
        }
        if !changed.is_empty() {
            self.emit_config_change_to_scripts(changed);
        }
        true
    }

    /// Synchronizes window focus with active modal overlays via the focus stack.
    pub(super) fn sync_modal_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = if self.palette.is_some() {
            Some(self.palette_focus_handle.clone())
        } else if self.ex.is_some() {
            Some(self.ex_focus_handle.clone())
        } else {
            None
        };
        match target {
            Some(target) if self.focus_stack.is_empty() => {
                let previous = self.focus_handle.clone();
                self.focus_stack.push(previous, &target, window, cx);
            }
            Some(target) => {
                if !target.is_focused(window) {
                    target.focus(window, cx);
                }
            }
            None if !self.focus_stack.is_empty() => {
                self.focus_stack.pop(window, cx);
            }
            None => {}
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        let key = ui::key::convert_key(&event.keystroke);

        if self.ex.is_some() {
            if let Some(key) = key {
                if matches!(key, Key::Char(_)) {
                    // Printable text reaches input handler via replace_text_in_range
                    return;
                }
                self.handle_ex_key(key);
            }
            self.sync_modal_focus(window, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if self.palette.is_some() {
            if let Some(key) = key {
                if self.opens_ex_from_palette(key) {
                    self.open_ex();
                    self.sync_modal_focus(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if matches!(key, Key::Char(_)) {
                    // Printable text reaches input handler via replace_text_in_range
                    return;
                }
                self.handle_palette_key(key);
            }
            self.sync_modal_focus(window, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }

        let Some(key) = key else {
            return;
        };

        // Printable text reaches Insert mode through `replace_text_in_range`, which every
        // platform calls for the same key press. Inserting here would duplicate it.
        self.activate_active_view();
        if self.editor().mode() == Mode::Insert && matches!(key, Key::Char(_)) {
            return;
        }

        // A modal command that starts with a letter (`i`, `a`, `p`) must not reach the
        // input handler, or the same letter would also be inserted after the mode change.
        // `Enter`, `Tab`, and `Backspace` are editable keys too: macOS sends them to the
        // input method as text, so they are handled here only.
        if matches!(key, Key::Char(_) | Key::Enter | Key::Tab | Key::Backspace) {
            cx.stop_propagation();
        }

        if self.apply_key(key) {
            self.sync_modal_focus(window, cx);
            cx.notify();
        }
    }

    /// `:` in an empty command palette closes it and opens the Ex command line.
    fn opens_ex_from_palette(&self, key: Key) -> bool {
        if !matches!(key, Key::Char(':')) {
            return false;
        }
        self.palette.as_ref().is_some_and(|palette| {
            palette.query.is_empty()
                && palette.preedit.is_empty()
                && matches!(palette.mode, PaletteMode::Commands)
        })
    }

    /// Passes the key to the keymap and runs the command it resolves to.
    fn apply_key(&mut self, key: Key) -> bool {
        self.activate_active_view();
        let before = self.active_view();
        let before_version = self.editor().version();
        let before_palette = self.palette.is_some();
        let before_ex = self.ex.is_some();

        let selection = self.editor().selection();
        let context = KeyContext {
            mode: self.editor().mode(),
            has_selection: selection.start != selection.end,
            operator_pending: self.editor().operator_pending(),
            text_object_pending: self.editor().text_object_pending(),
            ..KeyContext::default()
        };

        match self.keymap.feed(key, &context) {
            KeymapOutcome::Command { id, args, values } => {
                self.enqueue_command(Invocation::keymap_with_args(id, args, values));
            }
            KeymapOutcome::Pending | KeymapOutcome::Ignored => {}
        }

        // A preedit only makes sense while Insert mode keeps the caret.
        if self.editor().mode() != Mode::Insert {
            self.clear_preedit();
        }
        self.store_active_view();
        self.clamp_active_views();

        // Do not repaint for keys that change nothing, such as a motion at a boundary.
        let changed = before != self.active_view()
            || before_version != self.editor().version()
            || before_palette != self.palette.is_some()
            || before_ex != self.ex.is_some();

        self.ensure_cursor_visible();
        changed
    }

    /// Scrolls by whole lines or columns. The wheel moves the view, not the cursor.
    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = self.line_height();
        let pixel_delta = event.delta.pixel_delta(px(line_height));
        let delta_y = f32::from(pixel_delta.y);
        let delta_x = f32::from(pixel_delta.x);
        if delta_y == 0.0 && delta_x == 0.0 {
            return;
        }

        let Some(active_pane_id) = self.active_pane else {
            return;
        };
        let Some(pane) = self.panes.get(&active_pane_id) else {
            return;
        };
        let is_wrap = pane.is_wrap(&self.config);
        let viewport_rows = pane.viewport_rows;
        let viewport_cols = pane.viewport_cols;
        let scroll_top = pane.scroll_top;
        let scroll_left = pane.scroll_left;
        let scroll_sub_line = pane.scroll_sub_line;
        let Some(doc_id) = pane.document_id else {
            return;
        };
        let Some(document) = self.documents.get(&doc_id) else {
            return;
        };
        let line_count = document.editor.line_count();

        let mut changed = false;

        if delta_x != 0.0 && !is_wrap {
            let char_width = self.char_width;
            let mut cols = (delta_x / char_width).round() as isize;
            if cols == 0 {
                cols = delta_x.signum() as isize;
            }
            let next = vy_app::text_layout::scroll_cols(scroll_left, -cols);
            if next != scroll_left {
                if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                    pane.scroll_left = next;
                }
                changed = true;
            }
        }

        if delta_y != 0.0 {
            let mut lines = (delta_y / line_height).round() as isize;
            if lines == 0 {
                lines = delta_y.signum() as isize;
            }

            if !is_wrap {
                let next = scroll_lines(scroll_top, -lines, viewport_rows, line_count);
                if next != scroll_top {
                    if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                        pane.scroll_top = next;
                    }
                    changed = true;
                }
            } else {
                let rows_delta = -lines;
                let cols = viewport_cols.max(1);
                let mut cur_top = scroll_top;
                let mut cur_sub = scroll_sub_line;

                if rows_delta > 0 {
                    let mut remaining = rows_delta as usize;
                    while remaining > 0 && cur_top < line_count {
                        let text = document.editor.line(cur_top).unwrap_or_default();
                        let num_chunks = vy_app::text_layout::chunk_line(&text, cols).len();
                        if cur_sub + 1 < num_chunks {
                            cur_sub += 1;
                            remaining -= 1;
                        } else if cur_top + 1 < line_count {
                            cur_top += 1;
                            cur_sub = 0;
                            remaining -= 1;
                        } else {
                            break;
                        }
                    }
                    if cur_top != scroll_top || cur_sub != scroll_sub_line {
                        if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                            pane.scroll_top = cur_top;
                            pane.scroll_sub_line = cur_sub;
                        }
                        changed = true;
                    }
                } else if rows_delta < 0 {
                    let mut remaining = rows_delta.unsigned_abs();
                    while remaining > 0 {
                        if cur_sub > 0 {
                            cur_sub -= 1;
                            remaining -= 1;
                        } else if cur_top > 0 {
                            cur_top -= 1;
                            let text = document.editor.line(cur_top).unwrap_or_default();
                            let num_chunks = vy_app::text_layout::chunk_line(&text, cols).len();
                            cur_sub = num_chunks.saturating_sub(1);
                            remaining -= 1;
                        } else {
                            break;
                        }
                    }
                    if cur_top != scroll_top || cur_sub != scroll_sub_line {
                        if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                            pane.scroll_top = cur_top;
                            pane.scroll_sub_line = cur_sub;
                        }
                        changed = true;
                    }
                }
            }
        }

        if changed {
            cx.notify();
        }
    }
}

fn remote_env(startup: &Startup) -> RemoteEnv {
    remote_env_for(&startup.connection, &startup.connection_name)
}

fn remote_env_for(connection: &ConnectionSpec, name: &str) -> RemoteEnv {
    let remote = connection.is_remote();
    RemoteEnv {
        is_remote: remote,
        name: name.to_string(),
        os: if remote {
            "unknown".to_string()
        } else {
            std::env::consts::OS.to_string()
        },
        path_separator: if remote {
            "/".to_string()
        } else {
            std::path::MAIN_SEPARATOR.to_string()
        },
    }
}
