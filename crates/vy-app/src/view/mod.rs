//! Main editor view coordinating panes, input, RPC, and rendering.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use crate::focus::FocusStack;
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
use vy_app::editor::{Editor, Key, Mode, ViewState};
use vy_app::ex::{ExHistory, ExState};
use vy_app::keymap::{KeyContext, Keymap, KeymapOutcome};
use vy_app::palette::PaletteState;
use vy_app::paths::Paths;
use vy_app::plugin_host::{self, PluginHost};
use vy_app::rpc::Client;
use vy_app::state::{DocumentState, PaneState};
use vy_app::statusline::Statusline;
use vy_app::text_layout::scroll_lines;
use vy_app::workspace::{document_uri, workspace_info, workspace_uri};
use vy_core::{LayoutNode, PaneId};
use vy_proto::{InitializeParams, PROTOCOL_VERSION, to_value};

/// The kind of request we are waiting for a response to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pending {
    Initialize,
    Open,
    Snapshot,
    Undo,
    Redo,
    Close,
    Focus,
    EditorOpen,
    Save,
}

pub struct EditorView {
    client: Option<Client>,
    pending: HashMap<u64, Pending>,
    config: Config,
    paths: Paths,
    workspace: Option<PathBuf>,
    settings_stamp: Option<SystemTime>,
    keymap: Keymap,
    commands: CommandRegistry,
    /// The script thread and `init.ts` bookkeeping.
    plugin: Option<PluginHost>,
    /// Bumped whenever the script thread is replaced, so old pumps stop.
    script_generation: u64,
    /// Whether `init.ts` was evaluated since the last document load.
    init_evaluated: bool,
    /// URI of the document opened at startup.
    initial_uri: String,
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
    /// Set once the input handler has been registered with the platform.
    input_registered: bool,
    /// Statusline slots registered by scripts or default.
    statusline: Statusline,
}

impl EditorView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let paths = Paths::from_env();
        let workspace = std::env::current_dir().ok();
        let (config, warnings) = Config::load(&paths, workspace.as_deref());
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

        let uri = document_uri();
        let (env, trusted_warnings) = plugin_host::script_env(&workspace_info(), &paths);
        for warning in &trusted_warnings {
            eprintln!("vy: trusted.json: {warning}");
        }
        let plugin = Some(PluginHost::spawn(env, paths.init_ts()));

        let mut fallback = DocumentState::new(0, "vy:empty".to_string(), "", 0);
        fallback
            .editor
            .set_transaction_idle(Duration::from_millis(config.transaction_idle_ms()));

        let mut view = Self {
            client: None,
            pending: HashMap::new(),
            config,
            paths,
            workspace,
            settings_stamp,
            keymap,
            commands: CommandRegistry::new(),
            plugin,
            script_generation: 0,
            init_evaluated: false,
            initial_uri: uri,
            window_id: 1,
            layout: None,
            layout_revision: 0,
            active_pane: None,
            panes: HashMap::new(),
            documents: HashMap::new(),
            fallback,
            palette: None,
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
            input_registered: false,
            statusline: Statusline::new(),
        };

        match Client::spawn() {
            Ok(mut client) => {
                let id = client.request(
                    "initialize",
                    to_value(&InitializeParams {
                        protocol_version: PROTOCOL_VERSION,
                        client_version: env!("CARGO_PKG_VERSION").into(),
                        client_capabilities: Vec::new(),
                        workspace: Some(workspace_uri()),
                        resume_session: None,
                    }),
                );
                view.pending.insert(id, Pending::Initialize);
                view.client = Some(client);
                view.status = match warnings.last() {
                    Some(warning) => format!("settings: {warning}").into(),
                    None => "waiting for the initialize response".into(),
                };
            }
            Err(error) => {
                view.status = format!("could not start vy-server: {error}").into();
            }
        }

        view.start_receiving(cx);
        view.start_config_watch(cx);
        view.start_script(cx);
        view
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
        let last_visual = (view.mode() == Mode::Select).then(|| view.selection());

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
                    let config_changed = this.reload_config_if_changed();
                    // Editing `init.ts` starts a fresh runtime, so registrations
                    // from the previous evaluation cannot pile up.
                    if this.plugin.as_ref().is_some_and(PluginHost::changed) {
                        this.reload_init(cx);
                    }
                    if config_changed {
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

    /// Reloads the settings layers when the file timestamps changed.
    fn reload_config_if_changed(&mut self) -> bool {
        let stamp = self.paths.settings_stamp(self.workspace.as_deref());
        if stamp == self.settings_stamp {
            return false;
        }
        self.settings_stamp = stamp;

        let previous = self.config.clone();
        let warnings = self.config.reload(&self.paths, self.workspace.as_deref());
        for warning in &warnings {
            eprintln!("vy: settings: {warning}");
        }

        let idle = Duration::from_millis(self.config.transaction_idle_ms());
        let timeout = Duration::from_millis(self.config.sequence_timeout_ms());
        self.set_transaction_idle(idle);
        self.keymap.set_sequence_timeout(timeout);

        let changed = self.config.changed_keys(&previous);
        if let Some(key) = changed.last().cloned() {
            self.status = format!("settings: {key} changed").into();
        }
        if !changed.is_empty()
            && let Some(plugin) = self.plugin.as_ref()
        {
            plugin.emit_config_change(changed);
        }
        true
    }

    /// Synchronizes window focus with active modal overlays via the focus stack.
    pub(super) fn sync_modal_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_modal_open = self.palette.is_some() || self.ex.is_some();
        if is_modal_open && self.focus_stack.is_empty() {
            let target = if self.palette.is_some() {
                &self.palette_focus_handle
            } else {
                &self.ex_focus_handle
            };
            self.focus_stack
                .push(self.focus_handle.clone(), target, window, cx);
        } else if !is_modal_open && !self.focus_stack.is_empty() {
            self.focus_stack.pop(window, cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ui::key::convert_key(&event.keystroke);

        if self.ex.is_some() {
            if let Some(key) = key {
                self.handle_ex_key(key);
            }
            self.sync_modal_focus(window, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if self.palette.is_some() {
            if let Some(key) = key {
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
            KeymapOutcome::Command { id, args } => {
                self.enqueue_command(Invocation::keymap(id, args));
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

    /// Scrolls by whole lines. The wheel moves the view, not the cursor.
    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = self.line_height();
        let delta = f32::from(event.delta.pixel_delta(px(line_height)).y);
        if delta == 0.0 {
            return;
        }

        let Some(pane) = self.active_pane_state() else {
            return;
        };
        let Some(document) = self.active_document() else {
            return;
        };
        let (viewport_rows, scroll_top, line_count) = (
            pane.viewport_rows,
            pane.scroll_top,
            document.editor.line_count(),
        );

        // Positive deltas scroll up, so they move towards earlier lines.
        let mut lines = (delta / line_height).round() as isize;
        if lines == 0 {
            lines = delta.signum() as isize;
        }

        let next = scroll_lines(scroll_top, -lines, viewport_rows, line_count);

        if next != scroll_top {
            if let Some(pane) = self.active_pane_state_mut() {
                pane.scroll_top = next;
            }
            cx.notify();
        }
    }
}
