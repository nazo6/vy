//! Main editor view coordinating panes, input, RPC, and rendering.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use crate::focus::FocusStack;
use crate::theme::Theme;
use crate::ui;

mod commands;
mod input;
mod plugin;
mod render;
mod rpc;

use gpui::{
    Context, FocusHandle, KeyDownEvent, Modifiers, Pixels, Point, ScrollWheelEvent, SharedString,
    Window, px,
};
use gpui_base::VirtualListScrollHandle;
use vy_app::commands::{CommandRegistry, Invocation};
use vy_app::config::Config;
use vy_app::connection::{ConnectionSpec, Startup};
use vy_app::editor::{Editor, Key, Mode, ViewState, VisualRow};
use vy_app::ex::ExHistory;
use vy_app::keymap::{KeyContext, Keymap, KeymapOutcome, OverlayKind, PaneKind};
use vy_app::overlay::{ApprovalState, Overlay};
use vy_app::paths::Paths;
use vy_app::plugin::PluginManager;
use vy_app::plugin_host::{self, PluginHost};
use vy_app::rpc::Client;
use vy_app::slots::{GutterRegistry, HoverState};
use vy_app::state::{DocumentState, PaneState};
use vy_app::statusline::Statusline;
use vy_app::text_layout::scroll_lines;
use vy_app::workspace::{document_uri, workspace_info_for};
use vy_client_api::widget::UiRole;
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
    Resize,
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

/// How the open overlay handles a key before the editor sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayTarget {
    /// `:` in an empty palette switches to the Ex line.
    OpenEx,
    /// Printable text reaches the input handler.
    Text,
    /// The overlay handles the key itself.
    Key,
}

/// Classifies an overlay keystroke without touching overlay state.
///
/// `opens_ex` is computed by the caller from `opens_ex_from_palette` and
/// takes precedence over text: `:` in an empty palette must switch to the
/// Ex line instead of being inserted as text.
///
/// A converted `Key::Char` is text. A keystroke that `convert_key` drops
/// (notably Option-modified keys on macOS) is still text when it carries a
/// `key_char` and neither Control nor Command is held: that is how
/// Option-composed characters and dead keys reach the input handler.
/// Control and Command keystrokes are never text here; `Cmd+V` paste is a
/// separate, unimplemented feature.
fn classify_overlay_keystroke(
    key: Option<Key>,
    key_char: Option<&str>,
    modifiers: &Modifiers,
    opens_ex: bool,
) -> OverlayTarget {
    if opens_ex {
        return OverlayTarget::OpenEx;
    }
    if matches!(key, Some(Key::Char(_))) {
        return OverlayTarget::Text;
    }
    if key.is_none()
        && !modifiers.control
        && !modifiers.platform
        && let Some(character) = key_char
        && !character.is_empty()
    {
        return OverlayTarget::Text;
    }
    OverlayTarget::Key
}

/// Widget focus and list selection of one pane.
///
/// Comparing this before and after a key press is what tells the view that a
/// `vy.ui.*` command changed something worth drawing. Without it, `Tab` moved
/// focus but the frame was not requested, so the ring appeared on the next
/// cursor blink instead.
type PaneUiSnapshot = (
    vy_app::focus_path::FocusPath,
    std::collections::BTreeMap<String, vy_app::ui_list::ListState>,
);

/// Active drag operation resizing a split.
#[derive(Debug, Clone)]
pub(super) struct SplitDragState {
    pub(super) first_pane: PaneId,
    pub(super) second_pane: PaneId,
    pub(super) direction: vy_core::SplitDirection,
    pub(super) start_mouse: Point<Pixels>,
    pub(super) start_ratio: f32,
    pub(super) total_pixels: f32,
    pub(super) current_ratio: f32,
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
    /// Set when runtime settings changed; the next render refreshes caches.
    settings_dirty: bool,
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
    /// Modal overlay currently open (palette, Ex line, prompt, or approval).
    pub(crate) overlay: Option<Overlay>,
    /// Commands run from the palette, most recent first. Session-local.
    recent_commands: Vec<String>,
    ex_history: ExHistory,
    /// Commands waiting to run. Only one command runs at a time.
    command_queue: VecDeque<Invocation>,
    /// The script command that is currently running, if any.
    running_command: Option<String>,
    pub focus_handle: FocusHandle,
    pub overlay_focus_handle: FocusHandle,
    pub palette_scroll_handle: VirtualListScrollHandle,
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
    /// Rows marked in the open list surface, by item id.
    pub(super) marks: std::collections::BTreeSet<String>,
    /// Where the pointer went down on the titlebar. A window move starts only
    /// once the pointer leaves this point, so that double clicks still reach
    /// the titlebar instead of being swallowed by the compositor's move grab.
    titlebar_press: Rc<Cell<Option<Point<Pixels>>>>,
    last_reported_document_id: Option<u64>,
    last_reported_selections: Option<Vec<vy_core::Selection>>,
    pub(super) pending_view_for_next_open: Option<String>,
    pub(super) view_widget_trees: HashMap<String, vy_app::widget::WidgetNode>,
    /// Action name to command id, per view id. Set by `views.register`.
    pub(super) view_actions: HashMap<String, HashMap<String, String>>,
    /// Which runtime spawned each process, so stream events go to its owner.
    pub(super) stream_owners: HashMap<u32, Option<String>>,
    pub(super) cursor_visible: bool,
    pub(super) status_generation: usize,
    pub(crate) config_arc: std::sync::Arc<Config>,
    pub(crate) theme_arc: std::sync::Arc<Theme>,
    pub(super) resizing_split: Option<SplitDragState>,
}

impl EditorView {
    pub fn new(cx: &mut Context<Self>, startup: Startup) -> Self {
        let paths = Paths::from_env();
        let workspace = startup.local_workspace.clone();
        let (config, warnings) = Config::load(&paths, workspace.as_deref());
        let theme = Theme::from_config(&config);
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

        let initial_overlay = plugin_manager.next_approval_request().map(|req| {
            Overlay::Approval(ApprovalState::new(
                req.plugin_id.clone(),
                req.plugin_name.clone(),
                req.plugin_version.clone(),
                req.missing_capabilities.clone(),
            ))
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
            settings_dirty: false,
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
            overlay: initial_overlay,
            recent_commands: Vec::new(),
            ex_history: ExHistory::new(),
            command_queue: VecDeque::new(),
            running_command: None,
            focus_handle: cx.focus_handle(),
            overlay_focus_handle: cx.focus_handle(),
            palette_scroll_handle: VirtualListScrollHandle::new(),
            focus_stack: FocusStack::new(),
            status: "starting vy-server".into(),
            frame_log: std::env::var_os("VY_FRAME_LOG").is_some(),
            last_frame: None,
            char_width,
            statusline: Statusline::new(),
            gutter: GutterRegistry::new(),
            hover: None,
            marks: std::collections::BTreeSet::new(),
            titlebar_press: Rc::new(Cell::new(None)),
            last_reported_document_id: None,
            last_reported_selections: None,
            pending_view_for_next_open: None,
            view_widget_trees: HashMap::new(),
            view_actions: HashMap::new(),
            stream_owners: HashMap::new(),
            cursor_visible: true,
            status_generation: 0,
            config_arc,
            theme_arc,
            resizing_split: None,
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
        let is_select = view.mode().is_select();

        let is_wrap = pane.is_wrap(&self.config);
        let wrap_width = if is_wrap {
            Some(pane.viewport_cols.max(1))
        } else {
            None
        };

        let Some(document) = self.documents.get_mut(&document_id) else {
            return;
        };
        document.editor.set_wrap_width(wrap_width);
        if is_select {
            let range = document
                .editor
                .range_for(view.mode(), view.anchor(), view.head());
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
        // Switching to a UI pane puts keyboard focus on its first widget, so
        // `j` scrolls that panel instead of reaching the editor behind it.
        if let Some(pane) = pane.and_then(|id| self.panes.get_mut(&id)) {
            pane.normalize_list_states();
            pane.ensure_focus();
        }
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
                        let theme = Theme::from_config(&this.config);
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

    /// Refreshes cached config values after a runtime settings change.
    pub(super) fn apply_config_change(&mut self, cx: &mut Context<Self>) {
        if !self.settings_dirty {
            return;
        }
        self.settings_dirty = false;

        let idle = Duration::from_millis(self.config.transaction_idle_ms());
        let timeout = Duration::from_millis(self.config.sequence_timeout_ms());
        self.set_transaction_idle(idle);
        self.keymap.set_sequence_timeout(timeout);
        self.config_arc = std::sync::Arc::new(self.config.clone());

        let theme = Theme::from_config(&self.config);
        self.update_theme(theme, cx);
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

    /// Synchronizes window focus with the modal overlay via the focus stack.
    pub(super) fn sync_modal_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.is_some() {
            let target = self.overlay_focus_handle.clone();
            if self.focus_stack.is_empty() {
                let previous = self.focus_handle.clone();
                self.focus_stack.push(previous, &target, window, cx);
            } else if !target.is_focused(window) {
                target.focus(window, cx);
            }
        } else if !self.focus_stack.is_empty() {
            self.focus_stack.pop(window, cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        let key = ui::key::convert_key(&event.keystroke);

        if self.overlay.is_some() {
            let opens_ex = key
                .map(|key| self.opens_ex_from_palette(key))
                .unwrap_or(false);
            let target = classify_overlay_keystroke(
                key,
                event.keystroke.key_char.as_deref(),
                &event.keystroke.modifiers,
                opens_ex,
            );
            match target {
                OverlayTarget::OpenEx => self.open_ex(),
                // Printable text reaches the input handler via `replace_text_in_range`.
                // Do not stop propagation here: on macOS, stopping marks the key
                // as handled and GPUI never calls the input method (`insertText`),
                // so palette/Ex text input breaks with an ABC layout.
                OverlayTarget::Text => {}
                OverlayTarget::Key => {
                    if let Some(key) = key {
                        // Overlay keys go through the keymap, so `overlay == 'palette'`
                        // and the `vy.ui.*` defaults decide what a key does
                        // (ADR 0024 §8, ADR 0032 §6).
                        self.apply_overlay_key(key);
                    }
                }
            }
            self.sync_modal_focus(window, cx);
            if target != OverlayTarget::Text {
                cx.stop_propagation();
            }
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
        if matches!(
            key,
            Key::Char(_) | Key::Enter | Key::Tab | Key::BackTab | Key::Backspace
        ) {
            cx.stop_propagation();
        }

        if self.apply_key(key) {
            self.sync_modal_focus(window, cx);
            cx.notify();
        }
    }

    /// Passes one key of an open overlay to the keymap, then runs its command.
    ///
    /// A key the keymap does not resolve falls back to the overlay's built-in
    /// handling, so editing keys that have no default binding (`Backspace`,
    /// arrows in the Ex line) keep working. The keymap wins where it has a
    /// binding, which is what makes `overlay == 'palette'` rebindable
    /// (ADR 0024 §8).
    fn apply_overlay_key(&mut self, key: Key) {
        let mut context = KeyContext {
            mode: self.overlay_mode(),
            ..KeyContext::default()
        };
        self.apply_key_context(&mut context);

        match self.keymap.feed(key, &context) {
            KeymapOutcome::Command { id, args, values } => {
                let invocation = Invocation::keymap_with_args(id, args, values);
                if let Some(command) = vy_app::commands::UiActionCommand::parse(&invocation.id) {
                    self.run_ui_action_command(command, &invocation);
                } else {
                    self.enqueue_command(invocation);
                }
            }
            KeymapOutcome::Pending => {}
            KeymapOutcome::Ignored => self.handle_overlay_key(key),
        }
    }

    /// The mode an open surface reports to the keymap.
    ///
    /// The palette, picker, prompt, approval, and settings list start in insert
    /// mode so typing filters; a text-only surface stays in insert. Ex edits its
    /// line the same way.
    pub(super) fn overlay_mode(&self) -> Mode {
        Mode::Insert
    }

    /// Key hints for the open surface, in the order the palette shows them.
    ///
    /// The labels come from the keymap, so rebinding `vy.ui.activate` changes
    /// the hint instead of leaving a stale `Enter` in the text
    /// (ADR 0024 §7).
    pub(super) fn overlay_hints(&self) -> Vec<String> {
        let mut context = KeyContext {
            mode: self.overlay_mode(),
            ..KeyContext::default()
        };
        self.apply_key_context(&mut context);

        [
            ("vy.ui.next", "move down"),
            ("vy.ui.prev", "move up"),
            ("vy.ui.activate", "confirm"),
            ("vy.ui.cancel", "close"),
        ]
        .into_iter()
        .filter_map(|(command, label)| {
            let keys = self.keymap.hint_for(&[command], &context)?;
            Some(format!("{keys} {label}"))
        })
        .collect()
    }

    /// `:` in an empty command palette closes it and opens the Ex command line.
    fn opens_ex_from_palette(&self, key: Key) -> bool {
        if !matches!(key, Key::Char(':')) {
            return false;
        }
        self.overlay.as_ref().is_some_and(|overlay| {
            overlay.palette().is_some_and(|palette| {
                palette.input.is_empty()
                    && matches!(palette.mode, vy_app::palette::PaletteMode::Commands)
            })
        })
    }

    /// Passes the key to the keymap and runs the command it resolves to.
    fn apply_key(&mut self, key: Key) -> bool {
        self.activate_active_view();
        let before = self.active_view();
        let before_scroll = self.active_pane_state().map(scroll_position);
        let before_version = self.editor().version();
        let before_overlay = self.overlay.is_some();
        // Widget focus and list state live outside the editor, so a command that
        // only moves focus (`Tab`) or the cursor of a list must still repaint.
        let before_ui = self.active_pane_ui_snapshot();

        let selection = self.editor().selection();
        let mut context = KeyContext {
            mode: self.editor().mode(),
            has_selection: selection.start != selection.end,
            operator_pending: self.editor().operator_pending(),
            text_object_pending: self.editor().text_object_pending(),
            ..KeyContext::default()
        };
        self.apply_key_context(&mut context);

        match self.keymap.feed(key, &context) {
            KeymapOutcome::Command { id, args, values } => {
                // `vy.ui.*` runs here, not through the command queue: movement
                // and selection must not wait for a queue turn and a timeout
                // (ADR 0024 §5). The host defaults live in `ui_action`.
                if let Some(command) = vy_app::commands::UiActionCommand::parse(&id) {
                    let invocation = Invocation::keymap_with_args(id, args, values);
                    self.run_ui_action_command(command, &invocation);
                } else {
                    self.enqueue_command(Invocation::keymap_with_args(id, args, values));
                }
            }
            KeymapOutcome::Pending | KeymapOutcome::Ignored => {}
        }

        // A preedit only makes sense while Insert mode keeps the caret.
        if self.editor().mode() != Mode::Insert {
            self.clear_preedit();
        }
        self.store_active_view();
        self.clamp_active_views();

        self.ensure_cursor_visible();

        // Do not repaint for keys that change nothing, such as a motion at a boundary.
        // Scrolling commands (`zz`, `zt`, `zb`, `zh`, `zl`) move the view without
        // touching the cursor, so the scroll offsets have to be compared as well.
        before != self.active_view()
            || before_scroll != self.active_pane_state().map(scroll_position)
            || before_version != self.editor().version()
            || before_overlay != self.overlay.is_some()
            || before_ui != self.active_pane_ui_snapshot()
    }

    /// The widget focus and list state of the active pane, for change detection.
    ///
    /// Cloned on every key press. The maps hold one entry per list-like widget
    /// and a short id path per pane, so the copy is small next to a frame.
    fn active_pane_ui_snapshot(&self) -> PaneUiSnapshot {
        self.active_pane_state()
            .map(PaneState::ui_snapshot)
            .unwrap_or_default()
    }

    /// Fills the host-owned keymap context values from the active pane or overlay.
    ///
    /// When a modal surface is open, it owns keyboard focus exclusively (ADR 0024 §3,
    /// ADR 0032 §5, ADR 0034 §1). The background pane is masked (`paneKind = 'overlay'`),
    /// and the surface reports its own widget role.
    pub(super) fn apply_key_context(&self, context: &mut KeyContext) {
        let overlay = self.overlay_kind();
        context.overlay = overlay;
        context.surface_id = self.overlay_surface_id();
        context.remote = self.connection.is_remote();
        if overlay.is_modal() {
            context.editor_focus = false;
            context.pane_kind = PaneKind::Overlay;
            context.view_type.clear();
            context.ui_focus = None;
            context.ui_selection_count = 0;
            context.ui_role = self.overlay_ui_role();
        } else {
            match self.active_pane_state() {
                Some(pane) => {
                    pane.apply_key_context(context);
                    context.editor_focus = pane.context_editor_focus(overlay);
                }
                None => {
                    context.editor_focus = overlay == OverlayKind::None;
                }
            }
        }
    }

    /// The widget role reported by the topmost modal surface.
    pub(super) fn overlay_ui_role(&self) -> Option<UiRole> {
        match self.overlay.as_ref() {
            Some(Overlay::Palette(_)) => Some(UiRole::List),
            Some(Overlay::Ex(_)) | Some(Overlay::Prompt(_)) | Some(Overlay::SettingPrompt(_)) => {
                Some(UiRole::TextInput)
            }
            Some(Overlay::Approval(_)) => Some(UiRole::Button),
            Some(Overlay::Settings(_)) | Some(Overlay::SettingChoice(_)) => Some(UiRole::List),
            None => None,
        }
    }

    /// `id` of the topmost surface, when the surface has a stable id.
    ///
    /// Plugin pickers carry the id the plugin passed to `ui.picker.show`; the
    /// host-owned surfaces (palette, Ex line, prompt, approval, settings) have
    /// none (`surfaceId` stays absent for them).
    pub(super) fn overlay_surface_id(&self) -> Option<String> {
        match self.overlay.as_ref() {
            Some(Overlay::Palette(palette)) => match &palette.mode {
                vy_app::palette::PaletteMode::Custom { id, .. } => Some(id.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// The surface kind the keymap context reports.
    pub(super) fn overlay_kind(&self) -> OverlayKind {
        match self.overlay.as_ref() {
            None => OverlayKind::None,
            Some(Overlay::Palette(palette)) => match palette.mode {
                vy_app::palette::PaletteMode::Custom { .. } => OverlayKind::Picker,
                _ => OverlayKind::Palette,
            },
            Some(Overlay::Ex(_)) => OverlayKind::Ex,
            Some(Overlay::Prompt(_)) => OverlayKind::Prompt,
            Some(Overlay::Approval(_)) => OverlayKind::Approval,
            Some(Overlay::Settings(_))
            | Some(Overlay::SettingChoice(_))
            | Some(Overlay::SettingPrompt(_)) => OverlayKind::Settings,
        }
    }

    /// Moves widget focus inside the active pane in depth-first order.
    ///
    /// Returns whether the focused pane has a widget tree to walk. `Tab` and
    /// `Shift-Tab` resolve to `vy.ui.focus_next` / `vy.ui.focus_prev` in a UI
    /// pane, so this runs from the command rather than from `on_key_down`
    /// (ADR 0024 §3).
    ///
    /// A pane with no focus yet starts from the first focusable widget, so the
    /// movement keys work without a click first.
    pub(super) fn step_widget_focus(&mut self, forward: bool) -> bool {
        let Some(pane) = self.active_pane_state_mut() else {
            return false;
        };
        if pane.widget_tree.is_none() {
            return false;
        }
        if pane.focus_path.is_empty() {
            pane.ensure_focus();
            return true;
        }
        pane.step_focus(forward);
        true
    }

    /// Focuses one widget after a click.
    ///
    /// Clicking a widget also makes its pane the active one, so the keymap
    /// context and the focus ring agree.
    pub(super) fn focus_widget(&mut self, pane_id: PaneId, path: &vy_app::focus_path::FocusPath) {
        if self.active_pane != Some(pane_id) {
            self.switch_active_pane(Some(pane_id));
        }
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.set_focus_path(path.clone());
        }
    }

    /// Focuses the first or last focusable widget of the active pane.
    ///
    /// Used by `g g` and `G` when the focused widget is not a list: a panel
    /// built from buttons still needs a way to jump to either end.
    pub(super) fn focus_edge_widget(&mut self, first: bool) -> bool {
        let Some(pane) = self.active_pane_state_mut() else {
            return false;
        };
        let Some(tree) = pane.widget_tree.as_ref() else {
            return false;
        };
        let paths = vy_app::focus_path::focusable_paths(tree);
        let Some(path) = (if first { paths.first() } else { paths.last() }) else {
            return false;
        };
        let path = path.clone();
        pane.set_focus_path(path);
        true
    }

    /// The focused widget of the active pane, for `ui.focused` (ADR 0024 §5).
    ///
    /// Returns `None` for an editor pane, matching the spec: the editor's
    /// cursor and selection are read from `editor.*`, not from a widget.
    pub(super) fn current_ui_focus(&self) -> Option<plugin_host::UiFocus> {
        let pane = self.active_pane_state()?;
        if pane.pane_kind() != PaneKind::Ui {
            return None;
        }
        let focus_id = pane.ui_focus()?;
        Some(plugin_host::UiFocus {
            focus_id: focus_id.to_string(),
            role: pane.ui_role()?,
            pane_id: pane.id,
            view_id: pane.view_type.clone(),
        })
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
                let index = document
                    .editor
                    .visual_index_with(Some(viewport_cols.max(1)));
                let row = VisualRow {
                    line: scroll_top as u32,
                    chunk: scroll_sub_line,
                };
                let target = index.step(row, rows_delta as i64);

                if target.line as usize != scroll_top || target.chunk != scroll_sub_line {
                    if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                        pane.scroll_top = target.line as usize;
                        pane.scroll_sub_line = target.chunk;
                    }
                    changed = true;
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

/// Scroll offsets that decide which part of the document a pane draws.
fn scroll_position(pane: &PaneState) -> (usize, usize, usize) {
    (pane.scroll_top, pane.scroll_left, pane.scroll_sub_line)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Modifiers {
        Modifiers::default()
    }

    #[test]
    fn classify_overlay_keystroke_routes_text_and_keys() {
        // Plain printable text goes to the input handler.
        assert_eq!(
            classify_overlay_keystroke(Some(Key::Char('a')), Some("a"), &plain(), false),
            OverlayTarget::Text
        );
        // Shifted text is still text.
        assert_eq!(
            classify_overlay_keystroke(
                Some(Key::Char('A')),
                Some("A"),
                &Modifiers {
                    shift: true,
                    ..plain()
                },
                false,
            ),
            OverlayTarget::Text
        );
        // `:` in an empty palette switches to Ex instead of inserting text.
        assert_eq!(
            classify_overlay_keystroke(Some(Key::Char(':')), Some(":"), &plain(), true),
            OverlayTarget::OpenEx
        );
        // `:` elsewhere is ordinary text.
        assert_eq!(
            classify_overlay_keystroke(Some(Key::Char(':')), Some(":"), &plain(), false),
            OverlayTarget::Text
        );
        // Option-modified input (e.g. Option-s -> ß on macOS) is dropped by
        // `convert_key`, so it arrives with no `Key` but a `key_char`.
        assert_eq!(
            classify_overlay_keystroke(
                None,
                Some("ß"),
                &Modifiers {
                    alt: true,
                    ..plain()
                },
                false,
            ),
            OverlayTarget::Text
        );
        // Navigation and editing keys are handled by the overlay itself.
        assert_eq!(
            classify_overlay_keystroke(Some(Key::Enter), None, &plain(), false),
            OverlayTarget::Key
        );
        assert_eq!(
            classify_overlay_keystroke(Some(Key::Backspace), None, &plain(), false),
            OverlayTarget::Key
        );
        assert_eq!(
            classify_overlay_keystroke(Some(Key::Tab), None, &plain(), false),
            OverlayTarget::Key
        );
        // Control keys stay with the overlay (e.g. Ctrl-p / Ctrl-n selection).
        assert_eq!(
            classify_overlay_keystroke(
                Some(Key::Ctrl('u')),
                None,
                &Modifiers {
                    control: true,
                    ..plain()
                },
                false,
            ),
            OverlayTarget::Key
        );
        // Cmd+V is not text here; paste is out of scope for this fix.
        assert_eq!(
            classify_overlay_keystroke(
                None,
                None,
                &Modifiers {
                    platform: true,
                    ..plain()
                },
                false,
            ),
            OverlayTarget::Key
        );
        // An empty `key_char` with no converted key is not text.
        assert_eq!(
            classify_overlay_keystroke(None, Some(""), &plain(), false),
            OverlayTarget::Key
        );
    }
}
