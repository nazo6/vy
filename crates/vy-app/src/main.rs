//! Skeleton of `vy-app`.
//!
//! On startup it launches `vy-server` as a child process, sends `initialize`,
//! `pane/open`, and `buffer/snapshot` in that order, and draws the returned text in
//! one pane. The modal input in `vy_app::editor` handles keys and sends
//! `buffer/did_change` to the core. The layout tree lands in Phase 3.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use gpui::{
    App, Application, Bounds, Context, DefiniteLength, Div, ElementInputHandler,
    EntityInputHandler, FocusHandle, Focusable, HighlightStyle, KeyDownEvent, Keystroke, Pixels,
    Render, ScrollWheelEvent, SharedString, StyledText, TextRun, UTF16Selection, Window,
    WindowBounds, WindowOptions, canvas, div, point, prelude::*, px, rgb, size,
};
use rmpv::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;
use vy_app::commands::{
    BuiltinCommand, CommandDescriptor, CommandRegistry, DocumentCommand, Invocation,
    InvocationArgs, PaneCommand, UiCommand, parse_ex_line, resolve_ex_name,
};
use vy_app::config::{Config, LineNumbers};
use vy_app::editor::{Action, Editor, Key, Mode, Transaction, ViewState};
use vy_app::keymap::{KeyContext, Keymap, KeymapOutcome};
use vy_app::paths::Paths;
use vy_app::plugin_host::{
    self, DocumentInfo, HostAction, InitLoad, PluginHost, RequestContext, WorkspaceInfo,
};
use vy_app::rpc::Client;
use vy_core::{LayoutNode, PaneId, Range, Selection, SplitDirection, TextEdit};
use vy_js::{HostRequest, ScriptEvent};
use vy_proto::{PROTOCOL_VERSION, RpcMessage, RpcResult, get, object};

/// Lines drawn before the window has been measured.
const VISIBLE_LINES: usize = 30;
/// Space taken by the header, the footer, and the padding around them.
const CHROME_HEIGHT: f32 = 100.0;
/// Width of one character at the default font size. Used to size the gutter.
const CHARACTER_WIDTH: f32 = 10.0;
/// Width of the bar that marks the insertion point.
const CARET_WIDTH: f32 = 2.0;

/// The kind of request we are waiting for a response to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
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

/// A buffer the client draws. The core owns the text; this copy is for rendering.
struct DocumentState {
    id: u64,
    uri: String,
    editor: Editor,
    /// Whether a snapshot has filled this document.
    loaded: bool,
}

impl DocumentState {
    fn new(id: u64, uri: String, text: &str, version: u64) -> Self {
        let mut editor = Editor::with_text(text, version);
        editor.set_transaction_idle(Duration::from_millis(500));
        Self {
            id,
            uri,
            editor,
            loaded: false,
        }
    }
}

/// One pane in the layout tree.
struct PaneState {
    id: PaneId,
    document_id: Option<u64>,
    /// Cursor, selection, and mode of this pane.
    view: ViewState,
    /// Text the input method is still converting. It never reaches the core.
    preedit: String,
    /// Range the input method selected inside the preedit, in UTF-16 code units.
    preedit_selection: Option<std::ops::Range<usize>>,
    /// First line drawn in this pane.
    scroll_top: usize,
    /// Rows that fit in this pane. Updated on every render.
    viewport_rows: usize,
}

impl PaneState {
    fn new(id: PaneId, document_id: Option<u64>) -> Self {
        Self {
            id,
            document_id,
            view: ViewState::default(),
            preedit: String::new(),
            preedit_selection: None,
            scroll_top: 0,
            viewport_rows: VISIBLE_LINES,
        }
    }
}

#[derive(Debug, Default)]
struct PaletteState {
    query: String,
    selected: usize,
}

#[derive(Debug, Default)]
struct ExState {
    input: String,
    history_index: Option<usize>,
}

struct EditorView {
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
    palette: Option<PaletteState>,
    /// Open Ex command line, if any.
    ex: Option<ExState>,
    ex_history: Vec<String>,
    /// Commands waiting to run. Only one command runs at a time.
    command_queue: VecDeque<Invocation>,
    /// The script command that is currently running, if any.
    running_command: Option<String>,
    focus_handle: FocusHandle,
    status: SharedString,
    frame_log: bool,
    last_frame: Option<Instant>,
    /// Set once the input handler has been registered with the platform.
    input_registered: bool,
}

impl EditorView {
    fn new(cx: &mut Context<Self>) -> Self {
        let paths = Paths::from_env();
        let workspace = std::env::current_dir().ok();
        let (config, warnings) = Config::load(&paths, workspace.as_deref());
        let settings_stamp = settings_stamp(&paths, workspace.as_deref());
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
            ex_history: Vec::new(),
            command_queue: VecDeque::new(),
            running_command: None,
            focus_handle: cx.focus_handle(),
            status: "starting vy-server".into(),
            frame_log: std::env::var_os("VY_FRAME_LOG").is_some(),
            last_frame: None,
            input_registered: false,
        };

        match Client::spawn() {
            Ok(mut client) => {
                let id = client.request(
                    "initialize",
                    object([
                        ("protocol_version", Value::from(PROTOCOL_VERSION)),
                        ("client_version", Value::from(env!("CARGO_PKG_VERSION"))),
                        ("client_capabilities", Value::Array(Vec::new())),
                        ("workspace", Value::from(workspace_uri())),
                        ("resume_session", Value::Nil),
                    ]),
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
    fn start_script(&mut self, cx: &mut Context<Self>) {
        let Some(plugin) = self.plugin.as_ref() else {
            return;
        };
        let events = plugin.events();
        let requests = plugin.requests();
        let generation = self.script_generation;

        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let updated = this.update(cx, |this, cx| {
                    if this.script_generation == generation {
                        this.handle_script_event(event);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            while let Ok(request) = requests.recv().await {
                let updated = this.update(cx, |this, cx| {
                    if this.script_generation == generation {
                        this.handle_host_request(request);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    /// Starts a fresh runtime and evaluates `init.ts`. Registrations from the
    /// previous runtime are dropped first.
    fn reload_init(&mut self, cx: &mut Context<Self>) {
        self.command_queue.clear();
        self.running_command = None;
        self.commands
            .unregister_owner(&vy_app::commands::Owner::Init);
        self.keymap.unbind_owner(&vy_app::commands::Owner::Init);
        self.config.clear_memory();

        let (env, warnings) = plugin_host::script_env(&workspace_info(), &self.paths);
        for warning in &warnings {
            eprintln!("vy: trusted.json: {warning}");
        }
        self.plugin = Some(PluginHost::spawn(env, self.paths.init_ts()));
        self.script_generation += 1;
        self.start_script(cx);
        self.evaluate_init();
    }

    /// Hands `init.ts` to the script thread. Missing files are not an error.
    fn evaluate_init(&mut self) {
        let Some(plugin) = self.plugin.as_mut() else {
            return;
        };
        match plugin.evaluate_init() {
            InitLoad::Missing => {
                self.status = "no init.ts".into();
            }
            InitLoad::Loaded => {
                self.status = "loading init.ts".into();
            }
            InitLoad::Failed(message) => {
                eprintln!("vy: {message}");
                self.status = message.into();
            }
        }
    }

    fn handle_script_event(&mut self, event: ScriptEvent) {
        match event {
            ScriptEvent::Evaluated => {
                self.status = "init.ts loaded".into();
            }
            ScriptEvent::CommandFinished { id } => {
                if self.frame_log {
                    eprintln!("vy: command {id} finished");
                }
                if self.running_command.as_deref() == Some(id.as_str()) {
                    self.running_command = None;
                    self.drain_command_queue();
                }
            }
            ScriptEvent::Error { message } => {
                // The log file already has the message; the status line is the
                // shortest path to the user.
                eprintln!("vy: init.ts: {message}");
                self.status = format!("init.ts: {message}").into();
                // A failed command will not send CommandFinished. Do not leave the
                // queue blocked.
                if self.running_command.take().is_some() {
                    self.drain_command_queue();
                }
            }
            ScriptEvent::Stopped => {
                self.running_command = None;
                self.command_queue.clear();
                self.status = "the script host stopped".into();
            }
        }
    }

    /// Answers a call from the shim with the current client state.
    fn handle_host_request(&mut self, request: HostRequest) {
        self.activate_active_view();
        let document = self.document_info();
        let workspace = workspace_info();
        let Some(plugin) = self.plugin.as_ref() else {
            request.respond_error("RemoteDisconnected", "the script host is not running");
            return;
        };

        let editor = {
            let id = self.active_document_id();
            match id.and_then(|id| self.documents.get_mut(&id)) {
                Some(document) => &mut document.editor,
                None => &mut self.fallback.editor,
            }
        };
        let context = RequestContext {
            editor,
            config: &mut self.config,
            keymap: &mut self.keymap,
            commands: &mut self.commands,
            document,
            workspace: &workspace,
            paths: &self.paths,
            script: Some(plugin.script()),
        };
        let outcome = plugin_host::handle_request(context, request);

        if let Some(status) = outcome.status.clone() {
            self.status = status.into();
        }

        if !outcome.config_changed.is_empty() {
            plugin.emit_config_change(outcome.config_changed.clone());
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let timeout = Duration::from_millis(self.config.sequence_timeout_ms());
            self.set_transaction_idle(idle);
            self.keymap.set_sequence_timeout(timeout);
        }

        self.store_active_view();

        match outcome.action {
            HostAction::None => {}
            HostAction::Edit(transaction) => {
                self.send_transaction(transaction);
                self.clamp_active_views();
            }
            HostAction::Undo => self.send_history("buffer/undo", Pending::Undo),
            HostAction::Redo => self.send_history("buffer/redo", Pending::Redo),
            HostAction::Pane(command) => self.run_pane_command(command),
            HostAction::Document(command) => {
                self.run_document_command(command, &Invocation::palette(command.id()));
            }
            HostAction::Ui(command) => self.run_ui_command(command),
        }

        self.ensure_cursor_visible();
    }

    fn document_info(&self) -> Option<DocumentInfo> {
        let document = self.active_document()?;
        Some(DocumentInfo {
            id: document.id,
            uri: document.uri.clone(),
            language_id: "text".to_string(),
        })
    }

    /// Receives messages as they arrive. The reader thread pushes into an async
    /// channel, so the UI only wakes when there is something to handle.
    fn start_receiving(&self, cx: &mut Context<Self>) {
        let Some(receiver) = self.client.as_ref().map(Client::receiver) else {
            return;
        };

        cx.spawn(async move |this, cx| {
            while let Ok(message) = receiver.recv().await {
                let updated = this.update(cx, |this, cx| {
                    this.handle(message);
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }

            // The reader stopped: the server closed the connection.
            let _ = this.update(cx, |this, cx| {
                if this.client.is_some() {
                    this.status = "lost the connection to vy-server; restart it manually".into();
                    this.client = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

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
        let stamp = settings_stamp(&self.paths, self.workspace.as_deref());
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

    fn handle(&mut self, message: RpcMessage) {
        match message {
            RpcMessage::Response { id, result } => self.handle_response(id, result),
            RpcMessage::Notification { method, params } => {
                self.handle_notification(&method, params)
            }
            other => {
                eprintln!("vy: unsupported message: {other:?}");
            }
        }
    }

    fn handle_response(&mut self, id: u64, result: RpcResult) {
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };

        match result {
            RpcResult::Err { code, message, .. } => {
                self.status = format!("{code}: {message}").into();
            }
            RpcResult::Ok { value } => match pending {
                Pending::Initialize => {
                    if let Some(session_id) = get(&value, "session_id").and_then(Value::as_u64)
                        && let Some(client) = self.client.as_mut()
                    {
                        client.set_session_id(session_id);
                    }
                }
                Pending::Open => self.opened(&value),
                Pending::Snapshot => self.received_snapshot(&value),
                Pending::Undo | Pending::Redo => self.received_history(&value),
                Pending::Close | Pending::Focus => {}
                Pending::EditorOpen => self.editor_opened(&value),
                Pending::Save => self.saved(&value),
            },
        }
    }

    fn handle_notification(&mut self, method: &str, params: Value) {
        match method {
            "initialized" => {
                self.status = "connected; opening the document".into();
                self.open_document();
            }
            "buffer/resync" => self.received_snapshot(&params),
            "layout/did_change" => self.received_layout(&params),
            other => {
                eprintln!("vy: unsupported notification: {other}");
            }
        }
    }

    /// Applies the layout the core confirmed. The client never draws a tree the core
    /// has not accepted, so this is the only place panes are added or removed.
    fn received_layout(&mut self, params: &Value) {
        let revision = get(params, "revision")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if revision < self.layout_revision {
            return;
        }

        self.layout_revision = revision;
        self.window_id = get(params, "window_id")
            .and_then(Value::as_u64)
            .unwrap_or(self.window_id);
        self.layout = get(params, "layout").and_then(decode_value::<LayoutNode>);
        let next_active = get(params, "active_pane").and_then(Value::as_u64);
        self.switch_active_pane(next_active);

        let mut pane_ids = Vec::new();
        if let Some(layout) = &self.layout {
            collect_pane_ids(layout, &mut pane_ids);
        }
        for pane_id in &pane_ids {
            self.panes
                .entry(*pane_id)
                .or_insert_with(|| PaneState::new(*pane_id, None));
        }
        self.panes.retain(|pane_id, _| pane_ids.contains(pane_id));

        let used: Vec<u64> = self
            .panes
            .values()
            .filter_map(|pane| pane.document_id)
            .collect();
        self.documents.retain(|id, _| used.contains(id));
        self.activate_active_view();
    }

    fn open_document(&mut self) {
        let uri = self.initial_uri.clone();
        self.request(
            "pane/open",
            object([("uri", Value::from(uri))]),
            Pending::Open,
        );
    }

    fn opened(&mut self, value: &Value) {
        let (Some(pane_id), Some(document_id)) = (
            get(value, "pane_id").and_then(Value::as_u64),
            get(value, "document_id").and_then(Value::as_u64),
        ) else {
            self.status = "pane/open response has no pane or document".into();
            return;
        };

        let inherited = if self.documents.contains_key(&document_id) {
            self.active_view().unwrap_or_default()
        } else {
            ViewState::default()
        };
        let inherited_scroll = if self.documents.contains_key(&document_id) {
            self.active_pane_state()
                .map(|pane| pane.scroll_top)
                .unwrap_or_default()
        } else {
            0
        };
        match self.panes.entry(pane_id) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().document_id = Some(document_id);
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut pane = PaneState::new(pane_id, Some(document_id));
                pane.view = inherited;
                pane.scroll_top = inherited_scroll;
                entry.insert(pane);
            }
        }
        if self.active_pane.is_none() {
            self.active_pane = Some(pane_id);
            self.activate_active_view();
        }

        if !self.documents.contains_key(&document_id) {
            let uri = get(value, "uri")
                .and_then(Value::as_str)
                .unwrap_or("vy:untitled")
                .to_string();
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let mut document = DocumentState::new(document_id, uri, "", 0);
            document.editor.set_transaction_idle(idle);
            self.documents.insert(document_id, document);
        }

        self.request(
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
            Pending::Snapshot,
        );
    }

    fn editor_opened(&mut self, value: &Value) {
        let (Some(pane_id), Some(document_id)) = (
            get(value, "pane_id").and_then(Value::as_u64),
            get(value, "document_id").and_then(Value::as_u64),
        ) else {
            self.status = "editor/open response has no pane or document".into();
            return;
        };
        let replaced = get(value, "replaced")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        // The old pane state belongs to the old document.
        self.store_active_view();

        if !self.documents.contains_key(&document_id) {
            let uri = get(value, "uri")
                .and_then(Value::as_str)
                .unwrap_or("vy:untitled")
                .to_string();
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let mut document = DocumentState::new(document_id, uri, "", 0);
            document.editor.set_transaction_idle(idle);
            self.documents.insert(document_id, document);
        }

        let pane = self
            .panes
            .entry(pane_id)
            .or_insert_with(|| PaneState::new(pane_id, None));
        pane.document_id = Some(document_id);
        if replaced {
            pane.view = ViewState::default();
            pane.preedit.clear();
            pane.preedit_selection = None;
            pane.scroll_top = 0;
        }

        self.active_pane = Some(pane_id);
        self.activate_active_view();
        self.request(
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
            Pending::Snapshot,
        );
    }

    fn saved(&mut self, value: &Value) {
        let path = get(value, "path").and_then(Value::as_str).unwrap_or("-");
        self.status = format!("saved {path}").into();
    }

    fn received_snapshot(&mut self, value: &Value) {
        let started = Instant::now();
        let Some(document_id) = get(value, "document_id").and_then(Value::as_u64) else {
            self.status = "snapshot has no document_id".into();
            return;
        };
        let text = get(value, "text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let version = get(value, "version")
            .and_then(Value::as_u64)
            .unwrap_or_default();

        let mut reset = false;
        if let Some(document) = self.documents.get_mut(&document_id) {
            // A second pane on the same buffer asks for a snapshot too. Keep the
            // per-pane cursor when the copy is already at that version.
            if !document.loaded || document.editor.version() != version {
                document.editor.reset(text, version);
                reset = true;
            }
            document.loaded = true;
            if self.frame_log {
                eprintln!(
                    "vy: snapshot {} lines in {:.1}ms",
                    document.editor.line_count(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
        }
        if reset {
            self.reset_views_for_document(document_id);
            self.activate_active_view();
        }

        let (line_count, version) = self
            .documents
            .get(&document_id)
            .map(|document| (document.editor.line_count(), document.editor.version()))
            .unwrap_or_default();
        self.status = format!(
            "{} lines / version {} / pane {}",
            line_count,
            version,
            self.active_pane
                .map(|pane| pane.to_string())
                .unwrap_or_else(|| "-".into())
        )
        .into();

        // `init.ts` reads documents and settings, so it runs after the first
        // snapshot and before any user command.
        if !self.init_evaluated {
            self.init_evaluated = true;
            self.evaluate_init();
        }
    }

    fn received_history(&mut self, value: &Value) {
        self.activate_active_view();
        let version = get(value, "version")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| self.editor().version());
        let batches = get(value, "edits")
            .and_then(decode_value::<Vec<Vec<TextEdit>>>)
            .unwrap_or_default();
        let selection = get(value, "selection")
            .and_then(decode_value::<Vec<Selection>>)
            .unwrap_or_default();

        let result = {
            let Some(document) = self.active_document_mut() else {
                return;
            };
            document.editor.apply_history(&batches, version, &selection)
        };
        if let Err(error) = result {
            self.status = format!("could not apply the history: {error}").into();
        }

        self.store_active_view();
        if let Some(document_id) = self.active_document_id() {
            self.clamp_views_for_document(document_id);
            self.activate_active_view();
        }
    }

    fn request(&mut self, method: &str, params: Value, pending: Pending) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let id = client.request(method, params);
        self.pending.insert(id, pending);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key = convert_key(&event.keystroke);

        if self.ex.is_some() {
            if let Some(key) = key {
                self.handle_ex_key(key);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if self.palette.is_some() {
            if let Some(key) = key {
                self.handle_palette_key(key);
            }
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

    fn send_transaction(&mut self, transaction: Transaction) {
        let Some(document_id) = self.active_document_id() else {
            return;
        };
        let Some(client) = self.client.as_mut() else {
            return;
        };

        client.notify(
            "buffer/did_change",
            object([
                ("document_id", Value::from(document_id)),
                ("base_version", Value::from(transaction.base_version)),
                ("version", Value::from(transaction.base_version + 1)),
                ("transaction_id", Value::from(transaction.transaction_id)),
                ("edits", value_of(&transaction.edits)),
                ("selection_before", value_of(&transaction.selection_before)),
                ("selection_after", value_of(&transaction.selection_after)),
            ]),
        );
    }

    fn send_history(&mut self, method: &str, pending: Pending) {
        let Some(document_id) = self.active_document_id() else {
            return;
        };
        let Some(client) = self.client.as_mut() else {
            return;
        };

        let id = client.request(method, object([("document_id", Value::from(document_id))]));
        self.pending.insert(id, pending);
    }

    fn run_pane_command(&mut self, command: PaneCommand) {
        match command {
            PaneCommand::SplitRight => self.open_split("right"),
            PaneCommand::SplitDown => self.open_split("below"),
            PaneCommand::Close => self.close_active_pane(),
            PaneCommand::FocusLeft => self.focus_direction("left"),
            PaneCommand::FocusDown => self.focus_direction("down"),
            PaneCommand::FocusUp => self.focus_direction("up"),
            PaneCommand::FocusRight => self.focus_direction("right"),
            PaneCommand::Next => self.focus_next_pane(),
        }
    }

    fn run_document_command(&mut self, command: DocumentCommand, invocation: &Invocation) {
        match command {
            DocumentCommand::Open => {
                let Some(raw) = invocation.ex_raw() else {
                    self.status = "editor.open requires a path".into();
                    return;
                };
                let raw = raw.trim();
                if raw.is_empty() {
                    self.status = "editor.open requires a path".into();
                    return;
                }
                let uri = uri_from_argument(raw, self.workspace.as_deref());
                self.request(
                    "editor/open",
                    object([("uri", Value::from(uri))]),
                    Pending::EditorOpen,
                );
            }
            DocumentCommand::Save => {
                if invocation
                    .ex_raw()
                    .is_some_and(|raw| !raw.trim().is_empty())
                {
                    self.status = "saving to another path is not supported yet".into();
                    return;
                }
                let Some(document_id) = self.active_document_id() else {
                    self.status = "there is no active document".into();
                    return;
                };
                self.request(
                    "document/save",
                    object([("document_id", Value::from(document_id))]),
                    Pending::Save,
                );
            }
        }
    }

    /// Splits the active pane and opens the same document in the new one.
    fn open_split(&mut self, direction: &str) {
        let Some(target) = self.active_pane else {
            return;
        };
        let Some(uri) = self.active_document().map(|document| document.uri.clone()) else {
            self.status = "cannot split a pane without a document".into();
            return;
        };

        self.request(
            "pane/open",
            object([
                ("uri", Value::from(uri)),
                (
                    "placement",
                    object([
                        ("target", Value::from(target)),
                        ("direction", Value::from(direction)),
                    ]),
                ),
            ]),
            Pending::Open,
        );
        self.status = format!("splitting pane {target} {direction}").into();
    }

    fn close_active_pane(&mut self) {
        let Some(pane_id) = self.active_pane else {
            return;
        };
        self.request(
            "pane/close",
            object([("pane_id", Value::from(pane_id))]),
            Pending::Close,
        );
    }

    fn focus_direction(&mut self, direction: &str) {
        let (Some(layout), Some(active)) = (self.layout.as_ref(), self.active_pane) else {
            return;
        };
        let Some(pane_id) = focus_neighbor(layout, active, direction) else {
            return;
        };
        self.request(
            "pane/focus",
            object([("pane_id", Value::from(pane_id))]),
            Pending::Focus,
        );
    }

    fn focus_next_pane(&mut self) {
        let (Some(layout), Some(active)) = (self.layout.as_ref(), self.active_pane) else {
            return;
        };
        let mut pane_ids = Vec::new();
        collect_pane_ids(layout, &mut pane_ids);
        let Some(index) = pane_ids.iter().position(|pane| *pane == active) else {
            return;
        };
        let next = pane_ids[(index + 1) % pane_ids.len()];
        self.request(
            "pane/focus",
            object([("pane_id", Value::from(next))]),
            Pending::Focus,
        );
    }

    fn palette_items(&self) -> Vec<CommandDescriptor> {
        self.commands
            .descriptors()
            .into_iter()
            .filter(|command| !command.hidden)
            .collect()
    }

    fn palette_filtered_len(&self) -> usize {
        let Some(palette) = self.palette.as_ref() else {
            return 0;
        };
        filtered_palette_indices(&self.palette_items(), &palette.query).len()
    }

    fn palette_selected_id(&self) -> Option<String> {
        let palette = self.palette.as_ref()?;
        let items = self.palette_items();
        let filtered = filtered_palette_indices(&items, &palette.query);
        let index = palette.selected.min(filtered.len().saturating_sub(1));
        filtered.get(index).map(|index| items[*index].id.clone())
    }

    fn open_palette(&mut self) {
        self.clear_preedit();
        self.ex = None;
        self.palette = Some(PaletteState::default());
        self.status = "command palette".into();
    }

    fn open_ex(&mut self) {
        self.clear_preedit();
        self.palette = None;
        self.ex = Some(ExState::default());
        self.status = "ex command".into();
    }

    fn handle_ex_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.ex = None;
                return;
            }
            Key::Enter => {
                let Some(state) = self.ex.take() else {
                    return;
                };
                let line = state.input.trim().to_string();
                if line.is_empty() {
                    return;
                }
                self.ex_history.retain(|entry| entry != &line);
                self.ex_history.push(line.clone());
                self.execute_ex_line(&line);
                return;
            }
            Key::Up | Key::Ctrl('p') => {
                self.ex_history_move(-1);
                return;
            }
            Key::Down | Key::Ctrl('n') => {
                self.ex_history_move(1);
                return;
            }
            _ => {}
        }

        let Some(state) = self.ex.as_mut() else {
            return;
        };
        match key {
            Key::Backspace => {
                state.input.pop();
                state.history_index = None;
            }
            Key::Ctrl('u') => {
                state.input.clear();
                state.history_index = None;
            }
            Key::Char(character) => {
                state.input.push(character);
                state.history_index = None;
            }
            _ => {}
        }
    }

    fn ex_history_move(&mut self, delta: isize) {
        if self.ex_history.is_empty() {
            return;
        }
        let Some(state) = self.ex.as_mut() else {
            return;
        };
        let len = self.ex_history.len() as isize;
        let index = match state.history_index {
            Some(index) => (index as isize + delta).clamp(0, len - 1) as usize,
            None if delta < 0 => self.ex_history.len() - 1,
            None => return,
        };
        state.history_index = Some(index);
        state.input = self.ex_history[index].clone();
    }

    fn execute_ex_line(&mut self, line: &str) {
        let parsed = match parse_ex_line(line) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.status = format!("{error}").into();
                return;
            }
        };
        if parsed.bang {
            self.status = format!("{}: `!` is not supported yet", parsed.name).into();
            return;
        }
        let id = match resolve_ex_name(&parsed.name) {
            Ok(id) => id,
            Err(error) => {
                self.status = format!("{error}").into();
                return;
            }
        };

        self.status = format!(":{}", parsed.name).into();
        self.enqueue_command(Invocation::ex(id, parsed.args));
    }

    fn handle_palette_key(&mut self, key: Key) -> bool {
        match key {
            Key::Esc => {
                self.palette = None;
                return true;
            }
            Key::Enter => {
                let Some(id) = self.palette_selected_id() else {
                    return true;
                };
                self.palette = None;
                self.enqueue_command(Invocation::palette(id));
                return true;
            }
            _ => {}
        }

        let len = self.palette_filtered_len();
        let Some(palette) = self.palette.as_mut() else {
            return false;
        };

        match key {
            Key::Up | Key::Ctrl('p') => {
                palette.selected = move_palette_selection(palette.selected, -1, len);
            }
            Key::Down | Key::Ctrl('n') => {
                palette.selected = move_palette_selection(palette.selected, 1, len);
            }
            Key::Backspace => {
                palette.query.pop();
                palette.selected = 0;
            }
            Key::Char(character) => {
                palette.query.push(character);
                palette.selected = 0;
            }
            _ => {}
        }
        true
    }

    /// Queues a command and starts it when the previous command is done.
    fn enqueue_command(&mut self, invocation: Invocation) {
        self.command_queue.push_back(invocation);
        self.drain_command_queue();
    }

    fn drain_command_queue(&mut self) {
        while self.running_command.is_none() {
            let Some(invocation) = self.command_queue.pop_front() else {
                break;
            };
            if self.execute_invocation(invocation) {
                break;
            }
        }
    }

    /// Runs one command. Returns true when it continues on the script thread.
    fn execute_invocation(&mut self, invocation: Invocation) -> bool {
        let id = invocation.id.clone();

        if let Some(command) = UiCommand::parse(&id) {
            self.run_ui_command(command);
            return false;
        }
        if let Some(command) = PaneCommand::parse(&id) {
            self.run_pane_command(command);
            return false;
        }
        if let Some(command) = DocumentCommand::parse(&id) {
            self.run_document_command(command, &invocation);
            return false;
        }
        if let Some(command) = BuiltinCommand::parse(&id) {
            let args = invocation.keymap_args();
            self.activate_active_view();
            let action = self.editor_mut().execute(command, args);
            self.store_active_view();
            self.clamp_active_views();
            match action {
                Action::None => {}
                Action::Edit(transaction) => self.send_transaction(transaction),
                Action::Undo => self.send_history("buffer/undo", Pending::Undo),
                Action::Redo => self.send_history("buffer/redo", Pending::Redo),
            }
            self.ensure_cursor_visible();
            return false;
        }
        if self.commands.is_registered(&id) {
            // Script commands run on the script thread. Any edits they make come
            // back as host requests.
            if let Some(plugin) = self.plugin.as_ref() {
                let args = match &invocation.args {
                    InvocationArgs::Script(value) => value.clone(),
                    InvocationArgs::Keymap(args) => serde_json::json!({
                        "count": args.count,
                        "register": args.register.map(|register| register.to_string()),
                    }),
                    InvocationArgs::Ex { raw } => serde_json::json!({ "raw": raw }),
                    InvocationArgs::None => serde_json::json!({}),
                };
                self.running_command = Some(id.clone());
                plugin.execute(id.clone(), args);
                self.status = format!("running {id}").into();
            } else {
                self.status = format!("{id}: the script host is not running").into();
            }
            return false;
        }

        self.status = format!("unknown command: {id}").into();
        false
    }

    fn run_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::CommandPalette => self.open_palette(),
            UiCommand::ExCommandLine => self.open_ex(),
        }
    }

    /// Collects the lines visible in one pane.
    fn visible_lines(
        &self,
        pane: &PaneState,
        document: &DocumentState,
        window: &mut Window,
    ) -> Vec<Div> {
        let start = pane.scroll_top;
        let end = (start + pane.viewport_rows.max(1)).min(document.editor.line_count());

        (start..end)
            .map(|line| self.render_line(pane, document, line, window))
            .collect()
    }

    fn render_line(
        &self,
        pane: &PaneState,
        document: &DocumentState,
        line_number: usize,
        window: &mut Window,
    ) -> Div {
        let cursor_line = pane.view.cursor().line as usize;
        let mut row = div()
            .relative()
            .flex()
            .h(px(self.line_height()))
            .items_center();

        if let Some(label) = self.config.line_numbers().label(line_number, cursor_line) {
            row = row.child(
                div()
                    .w(px(self.gutter_width(document.editor.line_count())))
                    .flex_shrink_0()
                    .text_color(rgb(0x5a5a5a))
                    .child(format!(
                        "{label:>width$} ",
                        width = self.line_number_width(document.editor.line_count())
                    )),
            );
        }

        let text = document.editor.line(line_number).unwrap_or_default();
        let cursor = pane.view.cursor();

        if line_number as u32 != cursor.line {
            return row.child(text);
        }

        if pane.view.mode() == Mode::Insert {
            // Insert draws a thin bar between characters instead of the block that
            // marks the selection in the other modes.
            let (display, caret, preedit) = self.insert_marker(pane, &text);
            let width = self.text_width(&display, caret, window);
            let gutter = self.gutter_width(document.editor.line_count());

            let mut line = StyledText::new(display);
            if let Some(range) = preedit {
                line = line.with_highlights(vec![(range, cursor_highlight())]);
            }

            return row.child(line).child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(gutter + f32::from(width)))
                    .w(px(CARET_WIDTH))
                    .h(px(self.line_height()))
                    .bg(rgb(0xd4d4d4)),
            );
        }

        let (display, highlight) = self.cursor_marker(pane, &text);

        row.child(StyledText::new(display).with_highlights(vec![(highlight, cursor_highlight())]))
    }

    /// Returns the text to draw on the Insert cursor line, the caret's byte offset in
    /// it, and the range the input method is composing.
    fn insert_marker(
        &self,
        pane: &PaneState,
        text: &str,
    ) -> (String, usize, Option<std::ops::Range<usize>>) {
        let mut display = text.to_string();
        let caret = utf16_to_byte(&display, pane.view.cursor().character);

        if pane.preedit.is_empty() {
            return (display, caret, None);
        }

        // The preedit is drawn on the cursor line without entering the buffer.
        display.insert_str(caret, &pane.preedit);
        let end = caret + pane.preedit.len();
        (display, end, Some(caret..end))
    }

    /// Width in pixels from the start of a line to the byte offset `index`.
    fn text_width(&self, text: &str, index: usize, window: &mut Window) -> Pixels {
        if index == 0 {
            return px(0.);
        }

        let run = TextRun {
            len: text.len(),
            font: gpui::font(self.config.font_family()),
            color: rgb(0xd4d4d4).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(text.to_string()),
            px(self.config.font_size()),
            &[run],
            None,
        );
        shaped.x_for_index(index)
    }

    /// Returns the text to draw on the cursor line and the byte range to mark.
    ///
    /// This covers Normal and Select. Insert draws a bar instead (see
    /// [`Self::insert_marker`]).
    /// The line is shaped once as a whole. Splitting it into several elements would
    /// change the glyphs and widths under the cursor.
    fn cursor_marker(&self, pane: &PaneState, text: &str) -> (String, std::ops::Range<usize>) {
        let mut display = text.to_string();

        let length = display
            .chars()
            .map(|character| character.len_utf16() as u32)
            .sum();
        let Some((from, to)) =
            selection_on_line(pane.view.selection(), pane.view.cursor().line, length)
        else {
            // An empty selection at the end of the document still needs a marker.
            display.push(' ');
            let start = display.len() - 1;
            let end = display.len();
            return (display, start..end);
        };

        let from = utf16_to_byte(&display, from);
        let mut to = utf16_to_byte(&display, to);

        if to <= from {
            // Selecting only the line break: mark the end of the line.
            display.push(' ');
            to = display.len();
        }

        (display, from..to)
    }
}

/// The background used for the selection and for the preedit text.
fn cursor_highlight() -> HighlightStyle {
    HighlightStyle {
        background_color: Some(rgb(0x264f78).into()),
        ..Default::default()
    }
}

impl EditorView {
    /// Length of the preedit text in UTF-16 code units.
    fn preedit_len_utf16(&self) -> usize {
        self.active_pane_state()
            .map(|pane| pane.preedit.encode_utf16().count())
            .unwrap_or_default()
    }

    /// Flat UTF-16 offset of the caret in the text sent to the core.
    fn caret_utf16(&self) -> usize {
        self.editor().utf16_offset(self.editor().cursor())
    }

    fn clear_preedit(&mut self) {
        if let Some(pane) = self.active_pane_state_mut() {
            pane.preedit.clear();
            pane.preedit_selection = None;
        }
    }

    /// Returns text in flat UTF-16 offsets, with the preedit spliced in at the caret.
    ///
    /// The preedit never enters the document, so the input method only sees it through
    /// this view.
    fn document_text(&self, range: std::ops::Range<usize>) -> String {
        let Some(document) = self.active_document() else {
            return String::new();
        };
        let Some(pane) = self.active_pane_state() else {
            return String::new();
        };
        if pane.preedit.is_empty() {
            return document.editor.text_utf16(range);
        }

        let preedit = &pane.preedit;
        let caret = self.caret_utf16();
        let preedit_end = caret + preedit.encode_utf16().count();
        let mut text = String::new();

        if range.start < caret {
            text.push_str(
                &document
                    .editor
                    .text_utf16(range.start..range.end.min(caret)),
            );
        }

        if range.start <= preedit_end && range.end > caret {
            let from = range.start.saturating_sub(caret).min(preedit_end - caret);
            let to = range.end.saturating_sub(caret).min(preedit_end - caret);
            let from = utf16_to_byte(preedit, from as u32);
            let to = utf16_to_byte(preedit, to as u32);
            text.push_str(&preedit[from..to]);
        }

        if range.end > preedit_end {
            let start = range.start.max(preedit_end) - preedit_end;
            text.push_str(&document.editor.text_utf16(start..range.end - preedit_end));
        }

        text
    }

    fn commit_text(&mut self, text: &str) {
        self.activate_active_view();
        let action = self.editor_mut().insert_commit(text);
        self.store_active_view();
        self.clamp_active_views();
        if let Action::Edit(transaction) = action {
            self.send_transaction(transaction);
            self.ensure_cursor_visible();
        }
    }

    /// Digits used for line numbers. At least four, so small files do not shift.
    fn line_number_width(&self, line_count: usize) -> usize {
        line_count.max(1).to_string().len().max(4)
    }

    /// Height of one text row from the settings.
    fn line_height(&self) -> f32 {
        self.config.line_height()
    }

    /// Width of one character at the current font size.
    fn character_width(&self) -> f32 {
        CHARACTER_WIDTH * self.config.font_size() / 14.0
    }

    /// Width of the line number gutter. It grows with the number of lines and is
    /// zero when line numbers are turned off.
    fn gutter_width(&self, line_count: usize) -> f32 {
        if self.config.line_numbers() == LineNumbers::Off {
            return 0.0;
        }
        let width = self.character_width();
        let digits = self.line_number_width(line_count);
        digits as f32 * width + width * 1.2
    }

    /// Moves the active viewport just enough to show the cursor.
    fn ensure_cursor_visible(&mut self) {
        let (line, rows, scroll_top, total) = {
            let Some(pane) = self.active_pane_state() else {
                return;
            };
            let Some(document) = self.active_document() else {
                return;
            };
            (
                pane.view.cursor().line as usize,
                pane.viewport_rows,
                pane.scroll_top,
                document.editor.line_count(),
            )
        };

        let next = scroll_to_show(line, scroll_top, rows, total);
        if let Some(pane) = self.active_pane_state_mut() {
            pane.scroll_top = next;
        }
    }

    fn clamp_scroll(&mut self) {
        let (rows, scroll_top, total) = {
            let Some(pane) = self.active_pane_state() else {
                return;
            };
            let Some(document) = self.active_document() else {
                return;
            };
            (
                pane.viewport_rows,
                pane.scroll_top,
                document.editor.line_count(),
            )
        };

        let next = clamp_scroll_top(scroll_top, rows, total);
        if let Some(pane) = self.active_pane_state_mut() {
            pane.scroll_top = next;
        }
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if self.palette.is_some() {
            return None;
        }
        self.activate_active_view();
        let length = self.editor().len_utf16() + self.preedit_len_utf16();
        let end = range_utf16.end.min(length);
        let start = range_utf16.start.min(end);
        adjusted_range.replace(start..end);
        Some(self.document_text(start..end))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.palette.is_some() {
            return None;
        }
        self.activate_active_view();
        let document = self.active_document()?;
        let pane = self.active_pane_state()?;
        if pane.preedit.is_empty() {
            let selection = document.editor.selection();
            let start = document.editor.utf16_offset(selection.start);
            let end = document.editor.utf16_offset(selection.end);
            return Some(UTF16Selection {
                range: start..end.max(start),
                reversed: false,
            });
        }

        let caret = self.caret_utf16();
        let range = pane
            .preedit_selection
            .clone()
            .unwrap_or_else(|| 0..self.preedit_len_utf16());

        Some(UTF16Selection {
            range: caret + range.start..caret + range.end,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        if self.palette.is_some() || self.preedit_len_utf16() == 0 {
            return None;
        }

        let caret = self.caret_utf16();
        Some(caret..caret + self.preedit_len_utf16())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() || self.preedit_len_utf16() == 0 {
            return;
        }

        self.clear_preedit();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette.is_some() {
            return;
        }
        if self.frame_log {
            eprintln!("vy: commit {text:?}");
        }

        // The marked text is kept outside the document, so the range only tells us what
        // the platform believes it replaces. The commit always lands at the caret.
        let had_preedit = self.preedit_len_utf16() > 0;
        self.clear_preedit();

        if text.is_empty() {
            if had_preedit {
                cx.notify();
            }
            return;
        }

        self.commit_text(text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette.is_some() {
            return;
        }
        self.activate_active_view();
        if self.editor().mode() != Mode::Insert {
            // Typing in Normal or Select is a command, not text. Drop any preedit left
            // over from a mode change during composition.
            let had_preedit = self.preedit_len_utf16() > 0;
            self.clear_preedit();
            if had_preedit {
                cx.notify();
            }
            return;
        }

        if let Some(pane) = self.active_pane_state_mut() {
            pane.preedit.clear();
            pane.preedit.push_str(new_text);
            pane.preedit_selection = new_selected_range_utf16;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.palette.is_some() {
            return None;
        }
        self.activate_active_view();
        let document = self.active_document()?;
        let scroll_top = self.active_pane_state().map(|pane| pane.scroll_top)?;
        let position = document.editor.position_at_utf16(range_utf16.start);
        let line = document.editor.line(position.line as usize)?;
        let prefix = line.get(..utf16_to_byte(&line, position.character))?;

        let mut width = px(0.);
        if !prefix.is_empty() {
            let run = TextRun {
                len: prefix.len(),
                font: gpui::font(self.config.font_family()),
                color: rgb(0xd4d4d4).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(prefix.to_string()),
                window.rem_size(),
                &[run],
                None,
            );
            width = shaped.x_for_index(prefix.len());
        }

        let row = (position.line as usize).saturating_sub(scroll_top);
        let origin = element_bounds.origin
            + point(
                px(self.gutter_width(document.editor.line_count())) + width,
                px(row as f32 * self.line_height()),
            );
        Some(Bounds::new(origin, size(px(2.), px(self.line_height()))))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = Instant::now();

        // Fit the viewport to the window before choosing which lines to draw.
        let height = f32::from(window.viewport_size().height) - CHROME_HEIGHT;
        let rows = ((height / self.line_height()).floor().max(1.0)) as usize;
        if let Some(pane) = self.active_pane_state_mut() {
            pane.viewport_rows = rows;
        }
        self.clamp_scroll();

        let header = div()
            .flex()
            .justify_between()
            .text_sm()
            .text_color(rgb(0x9a9a9a))
            .child(format!(
                "vy {} / {} pane{}",
                env!("CARGO_PKG_VERSION"),
                self.panes.len(),
                if self.panes.len() == 1 { "" } else { "s" }
            ))
            .child(self.status.clone());

        let active_view = self.active_view().unwrap_or_default();
        let cursor = active_view.cursor();
        let footer = div()
            .flex()
            .justify_between()
            .text_sm()
            .text_color(rgb(0x9a9a9a))
            .child(format!(
                "{}  {}:{}",
                active_view.mode().label(),
                cursor.line + 1,
                cursor.character + 1
            ))
            .child(format!(
                "pane {} / version {}",
                self.active_pane
                    .map(|pane| pane.to_string())
                    .unwrap_or_else(|| "-".into()),
                self.editor().version()
            ));

        let content = match self.layout.clone() {
            Some(layout) => self.render_node(&layout, cx, window),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0x6a6a6a))
                .child("waiting for vy-server"),
        };

        let element = div()
            .id("vy-editor")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xd4d4d4))
            .font_family(self.config.font_family())
            .text_size(px(self.config.font_size()))
            .p_4()
            .gap_2()
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .child(content)
                    .children(self.palette.as_ref().map(|_| self.render_palette())),
            )
            .children(self.ex.as_ref().map(|_| self.render_ex()))
            .child(footer);

        self.log_frame(started);
        element
    }
}

impl EditorView {
    fn render_node(&self, node: &LayoutNode, cx: &mut Context<Self>, window: &mut Window) -> Div {
        match node {
            LayoutNode::Leaf { pane_id } => self.render_pane(*pane_id, cx, window),
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let row = match direction {
                    SplitDirection::Horizontal => div().flex().flex_row(),
                    SplitDirection::Vertical => div().flex().flex_col(),
                };

                // The wrappers own the ratio. The pane itself uses size_full, so
                // letting it size the flex item would force 100% width (or height)
                // and push its sibling out of view.
                let first_wrapper = match direction {
                    SplitDirection::Horizontal => div().h_full(),
                    SplitDirection::Vertical => div().w_full(),
                }
                .flex_basis(DefiniteLength::Fraction(*ratio))
                .flex_shrink_0()
                .overflow_hidden()
                .child(self.render_node(first, cx, window));

                let second_wrapper = match direction {
                    SplitDirection::Horizontal => div().h_full(),
                    SplitDirection::Vertical => div().w_full(),
                }
                .flex_basis(DefiniteLength::Fraction(1.0 - *ratio))
                .flex_shrink_0()
                .overflow_hidden()
                .child(self.render_node(second, cx, window));

                row.size_full().child(first_wrapper).child(second_wrapper)
            }
        }
    }

    fn render_ex(&self) -> Div {
        let Some(ex) = self.ex.as_ref() else {
            return div();
        };

        div()
            .flex()
            .gap_1()
            .items_center()
            .px_2()
            .py_1()
            .bg(rgb(0x252526))
            .text_sm()
            .child(div().text_color(rgb(0x9a9a9a)).child(":"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(ex.input.clone())
                    .child(self.input_caret()),
            )
    }

    /// A thin bar drawn at the end of a client-side input, such as the Ex line.
    fn input_caret(&self) -> Div {
        div()
            .w(px(CARET_WIDTH))
            .h(px(self.config.font_size()))
            .flex_shrink_0()
            .bg(rgb(0xd4d4d4))
    }

    fn render_palette(&self) -> Div {
        let Some(palette) = self.palette.as_ref() else {
            return div();
        };

        let items = self.palette_items();
        let filtered = filtered_palette_indices(&items, &palette.query);
        let selected = palette.selected.min(filtered.len().saturating_sub(1));

        let window = 12usize;
        let start = if filtered.len() <= window {
            0
        } else {
            selected
                .saturating_sub(window / 2)
                .min(filtered.len() - window)
        };
        let end = (start + window).min(filtered.len());

        let query = div()
            .flex()
            .justify_between()
            .items_center()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(rgb(0x3c3c3c))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(format!("> {}", palette.query))
                    .child(self.input_caret()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x8a8a8a))
                    .child(format!("{} commands", filtered.len())),
            );

        let mut rows = Vec::new();
        for index in start..end {
            let item = &items[filtered[index]];
            let row = div()
                .flex()
                .justify_between()
                .items_center()
                .px_2()
                .py_1()
                .w_full();
            let row = if index == selected {
                row.bg(rgb(0x094771))
            } else {
                row
            };
            rows.push(
                row.child(
                    div().flex().gap_2().child(item.title.clone()).child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x8a8a8a))
                            .child(item.id.clone()),
                    ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x8a8a8a))
                        .child(item.source.label().to_string()),
                ),
            );
        }

        if rows.is_empty() {
            rows.push(
                div()
                    .px_2()
                    .py_1()
                    .text_color(rgb(0x8a8a8a))
                    .child("no matching commands"),
            );
        }

        let hint = div()
            .px_2()
            .pt_1()
            .text_xs()
            .text_color(rgb(0x8a8a8a))
            .child("Enter run  ·  Esc close  ·  Up/Down move");

        div().absolute().top_0().left_0().w_full().p_4().child(
            div()
                .w_full()
                .bg(rgb(0x252526))
                .border_1()
                .border_color(rgb(0x3c3c3c))
                .flex()
                .flex_col()
                .child(query)
                .children(rows)
                .child(hint),
        )
    }

    fn render_pane(&self, pane_id: PaneId, cx: &mut Context<Self>, window: &mut Window) -> Div {
        let pane = self.panes.get(&pane_id);
        let document = pane
            .and_then(|pane| pane.document_id)
            .and_then(|id| self.documents.get(&id));
        let active = self.active_pane == Some(pane_id);

        let title = match (pane, document) {
            (Some(pane), Some(document)) => format!("{}  ·  pane {}", document.uri, pane.id),
            _ => format!("pane {pane_id} (empty)"),
        };
        let lines = match (pane, document) {
            (Some(pane), Some(document)) => self.visible_lines(pane, document, window),
            _ => Vec::new(),
        };

        let mut area = div()
            .relative()
            .flex_1()
            .overflow_hidden()
            .flex()
            .flex_col()
            .children(lines);

        if active {
            let view = cx.entity();
            area = area.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let focus = view.read(cx).focus_handle.clone();

                        if !view.read(cx).input_registered {
                            view.update(cx, |view, _| {
                                view.input_registered = true;
                                if view.frame_log {
                                    eprintln!("vy: input bounds={bounds:?}");
                                }
                            });
                        }

                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, view.clone()),
                            cx,
                        );
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }

        let border = if active { rgb(0x2b5e8c) } else { rgb(0x2d2d2d) };

        div()
            .flex()
            .flex_col()
            .size_full()
            .border_1()
            .border_color(border)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_1()
                    .text_xs()
                    .text_color(if active { rgb(0x9cdcfe) } else { rgb(0x8a8a8a) })
                    .child(title)
                    .child(if active { "[active]" } else { "" }),
            )
            .child(area)
    }
}

impl EditorView {
    fn log_frame(&mut self, started: Instant) {
        if !self.frame_log {
            return;
        }

        let now = Instant::now();
        let build = now.duration_since(started).as_secs_f64() * 1000.0;

        match self.last_frame.replace(now) {
            Some(last) => {
                let gap = now.duration_since(last).as_secs_f64() * 1000.0;
                eprintln!("vy: frame gap={gap:.1}ms build={build:.2}ms");
            }
            None => eprintln!("vy: frame gap=- build={build:.2}ms"),
        }
    }
}

/// Modification time of the newest settings file, if any exists.
fn settings_stamp(paths: &Paths, workspace: Option<&Path>) -> Option<SystemTime> {
    let project = workspace.map(|workspace| paths.workspace_settings(workspace));
    let mut newest: Option<SystemTime> = None;

    for path in std::iter::once(paths.settings_json()).chain(project) {
        if let Ok(modified) = std::fs::metadata(&path).and_then(|metadata| metadata.modified()) {
            newest = Some(newest.map_or(modified, |current| current.max(modified)));
        }
    }

    newest
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.0), px(700.0)), cx);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| cx.new(EditorView::new),
            )
            .expect("could not open the window");

        window
            .update(cx, |view, window, _cx| {
                view.focus_handle.focus(window);
            })
            .ok();

        cx.activate(true);
    });
}

/// Converts a GPUI keystroke into an editor key.
fn convert_key(keystroke: &Keystroke) -> Option<Key> {
    if keystroke.modifiers.control {
        let mut characters = keystroke.key.chars();
        return match (characters.next(), characters.next()) {
            (Some(character), None) if character.is_ascii_alphabetic() => {
                Some(Key::Ctrl(character.to_ascii_lowercase()))
            }
            _ => None,
        };
    }

    if keystroke.modifiers.alt || keystroke.modifiers.platform {
        return None;
    }

    match keystroke.key.as_str() {
        "escape" => return Some(Key::Esc),
        "enter" => return Some(Key::Enter),
        "backspace" => return Some(Key::Backspace),
        "delete" => return Some(Key::Delete),
        "tab" => return Some(Key::Tab),
        "left" => return Some(Key::Left),
        "right" => return Some(Key::Right),
        "up" => return Some(Key::Up),
        "down" => return Some(Key::Down),
        "home" => return Some(Key::Home),
        "end" => return Some(Key::End),
        "space" => return Some(Key::Char(' ')),
        _ => {}
    }

    // The committed text reaches Insert mode through the input handler, so commands use
    // the key name. Shift is applied here because key names are unshifted.
    let mut characters = keystroke.key.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }

    if keystroke.modifiers.shift && character.is_ascii_lowercase() {
        return Some(Key::Char(character.to_ascii_uppercase()));
    }

    Some(Key::Char(character))
}

/// Returns the viewport top that keeps `line` inside the window.
fn scroll_to_show(line: usize, top: usize, rows: usize, total: usize) -> usize {
    let rows = rows.max(1);
    let top = if line < top {
        line
    } else if line >= top + rows {
        line + 1 - rows
    } else {
        top
    };

    clamp_scroll_top(top, rows, total)
}

/// Clamps the viewport top so the last screen stays full.
fn clamp_scroll_top(top: usize, rows: usize, total: usize) -> usize {
    top.min(total.saturating_sub(rows.max(1)))
}

/// Moves the viewport by whole lines. Positive values go towards later lines.
fn scroll_lines(top: usize, lines: isize, rows: usize, total: usize) -> usize {
    let max_top = total.saturating_sub(rows.max(1)) as isize;
    (top as isize + lines).clamp(0, max_top) as usize
}

/// Indices of palette rows matching every whitespace-separated word.
fn filtered_palette_indices(items: &[CommandDescriptor], query: &str) -> Vec<usize> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return (0..items.len()).collect();
    }

    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let haystack = format!("{} {}", item.title, item.id).to_lowercase();
            tokens.iter().all(|token| haystack.contains(token))
        })
        .map(|(index, _)| index)
        .collect()
}

/// Moves around the palette list, wrapping at both ends.
fn move_palette_selection(index: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + delta).rem_euclid(len as isize) as usize
}

/// Collects pane ids from the layout tree in tree order.
fn collect_pane_ids(node: &LayoutNode, ids: &mut Vec<PaneId>) {
    match node {
        LayoutNode::Split { first, second, .. } => {
            collect_pane_ids(first, ids);
            collect_pane_ids(second, ids);
        }
        LayoutNode::Leaf { pane_id } => ids.push(*pane_id),
    }
}

#[derive(Debug, Clone, Copy)]
struct PaneRect {
    id: PaneId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl PaneRect {
    fn center(self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

fn collect_pane_rects(node: &LayoutNode, rect: PaneRect, rects: &mut Vec<PaneRect>) {
    match node {
        LayoutNode::Leaf { pane_id } => rects.push(PaneRect {
            id: *pane_id,
            ..rect
        }),
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let ratio = ratio.clamp(0.0, 1.0);
            let (first_rect, second_rect) = match direction {
                SplitDirection::Horizontal => (
                    PaneRect {
                        width: rect.width * ratio,
                        ..rect
                    },
                    PaneRect {
                        x: rect.x + rect.width * ratio,
                        width: rect.width * (1.0 - ratio),
                        ..rect
                    },
                ),
                SplitDirection::Vertical => (
                    PaneRect {
                        height: rect.height * ratio,
                        ..rect
                    },
                    PaneRect {
                        y: rect.y + rect.height * ratio,
                        height: rect.height * (1.0 - ratio),
                        ..rect
                    },
                ),
            };
            collect_pane_rects(first, first_rect, rects);
            collect_pane_rects(second, second_rect, rects);
        }
    }
}

/// Picks the nearest pane in `direction` from `active`, using the split ratios.
fn focus_neighbor(layout: &LayoutNode, active: PaneId, direction: &str) -> Option<PaneId> {
    let mut rects = Vec::new();
    collect_pane_rects(
        layout,
        PaneRect {
            id: active,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        &mut rects,
    );
    let current = rects.iter().find(|rect| rect.id == active)?;
    let (current_x, current_y) = current.center();

    rects
        .iter()
        .filter(|rect| rect.id != active)
        .filter_map(|rect| {
            let (x, y) = rect.center();
            let (dx, dy) = (x - current_x, y - current_y);
            let primary = match direction {
                "left" => -dx,
                "right" => dx,
                "up" => -dy,
                "down" => dy,
                _ => return None,
            };
            let secondary = match direction {
                "left" | "right" => dy.abs(),
                _ => dx.abs(),
            };
            (primary > 0.0).then_some((primary + secondary * 3.0, rect.id))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, id)| id)
}

/// Converts a UTF-16 offset to a UTF-8 byte offset. A position inside a surrogate
/// pair is rounded down to the start of the character.
fn utf16_to_byte(text: &str, units: u32) -> usize {
    let mut count = 0u32;

    for (index, character) in text.char_indices() {
        if count >= units || units < count + character.len_utf16() as u32 {
            return index;
        }
        count += character.len_utf16() as u32;
    }

    text.len()
}

/// The part of one line covered by the selection. [`None`] if the line is outside it.
fn selection_on_line(selection: Range, line: u32, length: u32) -> Option<(u32, u32)> {
    if selection.start == selection.end || line < selection.start.line || line > selection.end.line
    {
        return None;
    }

    let from = if line == selection.start.line {
        selection.start.character
    } else {
        0
    };
    let to = if line == selection.end.line {
        selection.end.character.min(length)
    } else {
        length
    };

    if from >= to {
        // Selecting only the line break: let the caller draw a marker.
        return (to >= length).then_some((length, length));
    }

    Some((from.min(length), to))
}

fn value_of<T: Serialize>(value: &T) -> Value {
    // rmpv's serde support turns structs into tuples, so go through rmp-serde.
    rmp_serde::to_vec_named(value)
        .ok()
        .and_then(|bytes| rmpv::decode::read_value(&mut bytes.as_slice()).ok())
        .unwrap_or(Value::Nil)
}

fn decode_value<T: DeserializeOwned>(value: &Value) -> Option<T> {
    let mut bytes = Vec::new();
    rmpv::encode::write_value(&mut bytes, value).ok()?;
    rmp_serde::from_slice(&bytes).ok()
}

/// Resolves the URI of the document to open.
///
/// Arguments starting with `file:` or `vy:` are used as-is. Other arguments are made
/// absolute and prefixed with `file:`. With no argument, opens an empty `vy:untitled`.
fn document_uri() -> String {
    match std::env::args().nth(1) {
        Some(argument) if argument.starts_with("file:") || argument.starts_with("vy:") => argument,
        Some(path) => match std::fs::canonicalize(&path) {
            Ok(absolute) => format!("file://{}", absolute.display()),
            // Let a file that does not exist yet open with the path as given.
            Err(_) => format!("file://{path}"),
        },
        None => "vy:untitled".to_string(),
    }
}

/// Resolves an Ex argument to a document URI. Relative paths use the workspace.
fn uri_from_argument(argument: &str, workspace: Option<&Path>) -> String {
    if argument.starts_with("file:") || argument.starts_with("vy:") {
        return argument.to_string();
    }

    let path = PathBuf::from(argument);
    let path = if path.is_absolute() {
        path
    } else {
        workspace
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .join(path)
    };

    match std::fs::canonicalize(&path) {
        Ok(absolute) => format!("file://{}", absolute.display()),
        Err(_) => format!("file://{}", path.display()),
    }
}

fn workspace_uri() -> String {
    match std::env::current_dir() {
        Ok(directory) => format!("file://{}", directory.display()),
        Err(_) => "file:///".to_string(),
    }
}

/// The workspace values scripts see through `workspace.*`.
fn workspace_info() -> WorkspaceInfo {
    let root = std::env::current_dir().ok();
    let root_uri = root
        .as_ref()
        .map(|path| format!("file://{}", path.display()));
    let name = root
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string);

    WorkspaceInfo {
        folders: root_uri.iter().cloned().collect(),
        root_uri,
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vy_core::Position;

    #[test]
    fn converts_utf16_units_to_byte_offsets() {
        assert_eq!(utf16_to_byte("a😀b", 0), 0);
        assert_eq!(utf16_to_byte("a😀b", 1), 1);
        // A position inside a surrogate pair is rounded down.
        assert_eq!(utf16_to_byte("a😀b", 2), 1);
        assert_eq!(utf16_to_byte("a😀b", 3), 5);
        assert_eq!(utf16_to_byte("a😀b", 4), 6);
    }

    #[test]
    fn clips_a_selection_to_each_line() {
        let selection = Range::new(Position::new(0, 1), Position::new(2, 1));

        assert_eq!(selection_on_line(selection, 0, 3), Some((1, 3)));
        assert_eq!(selection_on_line(selection, 1, 5), Some((0, 5)));
        assert_eq!(selection_on_line(selection, 2, 4), Some((0, 1)));
        assert_eq!(selection_on_line(selection, 3, 4), None);
    }

    #[test]
    fn marks_a_selected_line_break() {
        let selection = Range::new(Position::new(0, 3), Position::new(1, 0));
        assert_eq!(selection_on_line(selection, 0, 3), Some((3, 3)));
    }

    #[test]
    fn scrolls_only_when_the_cursor_leaves_the_window() {
        // 100 lines, 10 rows, starting at the top.
        assert_eq!(scroll_to_show(5, 0, 10, 100), 0);
        assert_eq!(scroll_to_show(10, 0, 10, 100), 1);
        assert_eq!(scroll_to_show(42, 20, 10, 100), 33);
        assert_eq!(scroll_to_show(3, 20, 10, 100), 3);
    }

    #[test]
    fn keeps_the_last_screen_full() {
        assert_eq!(clamp_scroll_top(95, 10, 100), 90);
        assert_eq!(clamp_scroll_top(4, 10, 100), 4);
        assert_eq!(clamp_scroll_top(4, 10, 3), 0);
    }

    #[test]
    fn scrolls_by_lines_within_the_document() {
        assert_eq!(scroll_lines(20, 5, 10, 100), 25);
        assert_eq!(scroll_lines(20, -5, 10, 100), 15);
        assert_eq!(scroll_lines(2, -5, 10, 100), 0);
        assert_eq!(scroll_lines(88, 5, 10, 100), 90);
    }

    #[test]
    fn round_trips_edits_through_message_pack() {
        let edits = vec![
            TextEdit::new(Range::new(Position::new(0, 1), Position::new(0, 3)), "x"),
            TextEdit::new(
                Range::new(Position::new(2, 0), Position::new(2, 0)),
                "こんにちは",
            ),
        ];

        let value = value_of(&edits);
        let decoded = decode_value::<Vec<TextEdit>>(&value).expect("decode");
        assert_eq!(decoded, edits);

        let selection = vec![Selection::caret(Position::new(1, 2))];
        let value = value_of(&selection);
        assert_eq!(
            decode_value::<Vec<Selection>>(&value).expect("decode"),
            selection
        );
    }

    #[test]
    fn collects_pane_ids_in_tree_order() {
        let layout = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf { pane_id: 2 }),
                second: Box::new(LayoutNode::Leaf { pane_id: 3 }),
            }),
        };

        let mut ids = Vec::new();
        collect_pane_ids(&layout, &mut ids);
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn picks_the_neighbor_in_the_requested_direction() {
        let layout = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };

        assert_eq!(focus_neighbor(&layout, 1, "right"), Some(2));
        assert_eq!(focus_neighbor(&layout, 2, "left"), Some(1));
        assert_eq!(focus_neighbor(&layout, 1, "left"), None);

        let stacked = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.25,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };
        assert_eq!(focus_neighbor(&stacked, 1, "down"), Some(2));
        assert_eq!(focus_neighbor(&stacked, 2, "up"), Some(1));
    }

    #[test]
    fn round_trips_the_layout_tree_through_message_pack() {
        let layout = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.3,
            first: Box::new(LayoutNode::Leaf { pane_id: 4 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 5 }),
        };

        let value = value_of(&layout);
        assert_eq!(get(&value, "type").and_then(Value::as_str), Some("split"));
        assert_eq!(
            get(&value, "direction").and_then(Value::as_str),
            Some("vertical")
        );
        assert_eq!(decode_value::<LayoutNode>(&value).expect("decode"), layout);
    }

    fn palette_item(id: &str, title: &str) -> CommandDescriptor {
        CommandDescriptor {
            id: id.to_string(),
            title: title.to_string(),
            source: vy_app::commands::CommandSource::Builtin,
            aliases: Vec::new(),
            hidden: false,
            accepts_args: false,
            accepts_range: false,
            accepts_bang: false,
        }
    }

    #[test]
    fn filters_the_palette_by_every_word() {
        let items = vec![
            palette_item("vy.pane.split_right", "Pane Split Right"),
            palette_item("vy.pane.split_down", "Pane Split Down"),
            palette_item("vy.edit.delete", "Edit Delete"),
        ];

        assert_eq!(filtered_palette_indices(&items, ""), vec![0, 1, 2]);
        assert_eq!(filtered_palette_indices(&items, "split"), vec![0, 1]);
        assert_eq!(filtered_palette_indices(&items, "pane down"), vec![1]);
        assert_eq!(filtered_palette_indices(&items, "edit"), vec![2]);
        assert_eq!(
            filtered_palette_indices(&items, "missing"),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn wraps_the_palette_selection() {
        assert_eq!(move_palette_selection(0, -1, 3), 2);
        assert_eq!(move_palette_selection(2, 1, 3), 0);
        assert_eq!(move_palette_selection(1, 1, 3), 2);
        assert_eq!(move_palette_selection(0, 1, 0), 0);
    }

    #[test]
    fn resolves_relative_ex_paths_against_the_workspace() {
        assert_eq!(
            uri_from_argument("README.md", Some(Path::new("/tmp/vy-ws"))),
            "file:///tmp/vy-ws/README.md"
        );
        assert_eq!(
            uri_from_argument("file:///tmp/readme.md", None),
            "file:///tmp/readme.md"
        );
    }
}
