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
mod surface;

use gpui::{
    Context, FocusHandle, KeyDownEvent, Modifiers, Pixels, Point, ScrollStrategy, ScrollWheelEvent,
    SharedString, Window, px,
};
use vy_app::commands::{CommandRegistry, Invocation, UiCommand};
use vy_app::config::Config;
use vy_app::connection::{ConnectionSpec, Startup};
use vy_app::editor::{Editor, Key, Mode, ViewState, VisualRow};
use vy_app::ex::ExHistory;
use vy_app::keymap::{KeyContext, Keymap, KeymapOutcome};
use vy_app::overlay::{ApprovalState, SurfaceKind};
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

/// What the overlay path does with a key the keymap did not consume.
///
/// This is the fallback after `Keymap::feed` returns `Ignored`, and the
/// whole decision for keystrokes `convert_key` drops (notably
/// Option-composed characters on macOS, which arrive with no `Key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayFallback {
    /// Let the input handler insert the character. Never stop propagation:
    /// on macOS, stopping marks the key as handled and GPUI never calls
    /// the input method (`insertText`), so overlay text input breaks.
    Input,
    /// Stop propagation; the key does nothing (modal overlay owns it).
    Stop,
    /// Switch a normal-mode palette back to insert mode, then let the
    /// input handler insert the character.
    SwitchToInsert,
    /// Delete the last input character (Backspace), as the old handlers did.
    PopChar,
}

/// Fallback for an overlay key the keymap ignored.
///
/// `palette` says the open overlay is a palette; `insert` says it is in
/// insert mode (palettes) or, for single-mode overlays, always true.
/// A `Char` in a normal-mode palette switches back to insert mode and
/// types (type-to-search); anywhere else it is plain input. Backspace
/// always edits. Anything else is swallowed by the modal overlay.
///
/// Note: a lone sequence prefix (`g` of `g g`) is swallowed by the
/// keymap while it waits, so typing `go` in a normal-mode palette
/// inserts only `o`. That is the usual prefix-key trade-off.
fn overlay_fallback(
    palette: bool,
    insert: bool,
    key: Option<Key>,
    key_char: Option<&str>,
    modifiers: &Modifiers,
) -> OverlayFallback {
    match key {
        None => {
            if !modifiers.control
                && !modifiers.platform
                && let Some(character) = key_char
                && !character.is_empty()
            {
                OverlayFallback::Input
            } else {
                OverlayFallback::Stop
            }
        }
        Some(Key::Char(_)) => {
            if palette && !insert {
                OverlayFallback::SwitchToInsert
            } else {
                OverlayFallback::Input
            }
        }
        Some(Key::Backspace) => OverlayFallback::PopChar,
        Some(_) => OverlayFallback::Stop,
    }
}

/// Whether a keymap command may run while a modal overlay is open.
///
/// Overlay keys run only overlay actions (`vy.ui.*` movement, activation,
/// cancel, completion, clear, and the Ex line). Editor, pane, document,
/// and overlay-opening commands (`vy.ui.command_palette`,
/// `vy.ui.open_settings`) are ignored: while a modal overlay is open,
/// global and editor bindings must not leak through and edit the
/// document behind it or abandon a picker reply.
fn is_overlay_action_command(id: &str) -> bool {
    matches!(
        UiCommand::parse(id),
        Some(
            UiCommand::UiNext
                | UiCommand::UiPrev
                | UiCommand::UiPageDown
                | UiCommand::UiPageUp
                | UiCommand::UiFirst
                | UiCommand::UiLast
                | UiCommand::UiActivate
                | UiCommand::UiCancel
                | UiCommand::UiToggle
                | UiCommand::UiYank
                | UiCommand::UiFocusNext
                | UiCommand::UiFocusPrev
                | UiCommand::UiClearLine
                | UiCommand::UiCompleteNext
                | UiCommand::UiCompletePrev
                | UiCommand::ExCommandLine
        )
    )
}

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

/// A pane's rectangle within the content area, in pixels, computed by
/// the single layout walk in `update_pane_viewports` (ADR 0032 Phase 4:
/// geometry lives in one place). Used by the surface layer (hover) to
/// position anchored popups without a second layout walk.
/// Full pane geometry; hover uses only the origin today, but
/// anchored surfaces (menus, plugin surfaces) need the whole rect.
#[derive(Debug, Clone, Copy)]
pub(super) struct PaneRect {
    pub(super) x: f32,
    pub(super) y: f32,
    #[allow(dead_code)]
    pub(super) width: f32,
    #[allow(dead_code)]
    pub(super) height: f32,
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
    /// Surface stack, frontmost last (ADR 0032 Phase 4). Empty means no
    /// modal surface is open; the frontmost modal surface owns input.
    pub(crate) surfaces: Vec<surface::Surface>,
    /// Leaf pane rectangles from the latest layout walk, keyed by pane.
    pane_geometries: std::collections::HashMap<PaneId, PaneRect>,
    /// Counter for disambiguating duplicate surface ids.
    next_surface_id: u64,
    /// Palette input mode (ADR 0032 Phase 3): palettes open in insert mode,
    /// where typed characters filter; `Esc` switches to normal mode, where
    /// list keys move the selection. Only meaningful while the overlay is
    /// a palette; other overlays are single-mode.
    palette_insert: bool,
    /// Commands run from the palette, most recent first. Session-local.
    recent_commands: Vec<String>,
    ex_history: ExHistory,
    /// Commands waiting to run. Only one command runs at a time.
    command_queue: VecDeque<Invocation>,
    /// The script command that is currently running, if any.
    running_command: Option<String>,
    pub focus_handle: FocusHandle,
    pub overlay_focus_handle: FocusHandle,
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
    /// Where the pointer went down on the titlebar. A window move starts only
    /// once the pointer leaves this point, so that double clicks still reach
    /// the titlebar instead of being swallowed by the compositor's move grab.
    titlebar_press: Rc<Cell<Option<Point<Pixels>>>>,
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
    pub(super) resizing_split: Option<SplitDragState>,
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

        let initial_surfaces = plugin_manager
            .next_approval_request()
            .map(|req| {
                surface::Surface::new(
                    "approval".to_string(),
                    SurfaceKind::Approval(ApprovalState::new(
                        req.plugin_id.clone(),
                        req.plugin_name.clone(),
                        req.plugin_version.clone(),
                        req.missing_capabilities.clone(),
                    )),
                    None,
                )
            })
            .into_iter()
            .collect();

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
            surfaces: initial_surfaces,
            pane_geometries: std::collections::HashMap::new(),
            next_surface_id: 0,
            palette_insert: true,
            recent_commands: Vec::new(),
            ex_history: ExHistory::new(),
            command_queue: VecDeque::new(),
            running_command: None,
            focus_handle: cx.focus_handle(),
            overlay_focus_handle: cx.focus_handle(),
            focus_stack: FocusStack::new(),
            status: "starting vy-server".into(),
            frame_log: std::env::var_os("VY_FRAME_LOG").is_some(),
            last_frame: None,
            char_width,
            statusline: Statusline::new(),
            gutter: GutterRegistry::new(),
            hover: None,
            titlebar_press: Rc::new(Cell::new(None)),
            last_reported_document_id: None,
            last_reported_selections: None,
            pending_view_for_next_open: None,
            view_widget_trees: HashMap::new(),
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

    /// Snapshot of the active UI pane's focused widget for `ui.focused()`.
    ///
    /// Editor panes answer null, as do UI panes with no widget tree or no
    /// focusable widget. The snapshot is taken when a host request is
    /// handled, because the script thread cannot read live view state.
    fn ui_focus_info(&self) -> Option<vy_client_api::namespaces::ui::UiFocused> {
        let pane = self.active_pane_state()?;
        if pane_kind_for(pane.view_type.as_str()).as_str() == "editor" {
            return None;
        }
        let tree = pane.widget_tree.as_ref()?;
        let resolved = pane.focus_path.resolve(tree);
        let role = resolved.focused_role(tree)?;
        let focus_id = resolved.last()?.clone();
        Some(vy_client_api::namespaces::ui::UiFocused {
            view_id: pane.view_type.clone(),
            pane_id: pane.id,
            role: role.to_string(),
            focus_id,
        })
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

        let theme = if self.config.theme() == "light" {
            Theme::light()
        } else {
            Theme::dark()
        };
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
        if !self.surfaces.is_empty() {
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

        // ADR 0032 Phase 3: overlay keys go through the keymap. The old
        // early return (classify, then `handle_overlay_key`) is gone:
        // non-text keys feed `Keymap::feed` and run overlay actions, while
        // text still reaches the input handler via `replace_text_in_range`
        // without stopping propagation (macOS input-method invariant).
        if self.surface_top().is_some_and(|surface| surface.modal) {
            let is_palette = matches!(self.surface_top_kind(), Some(SurfaceKind::Palette(_)));
            // Single-mode overlays (Ex, prompt, approval, settings) always
            // behave like insert mode for characters.
            let insert = !is_palette || self.palette_insert;
            let key_char = event.keystroke.key_char.as_deref();
            let modifiers = &event.keystroke.modifiers;
            // Preserved affordance: `:` in an empty command palette opens
            // the Ex line instead of inserting text. In a normal-mode
            // palette `:` goes through the keymap binding instead.
            let mut stop = true;
            if is_palette
                && insert
                && key == Some(Key::Char(':'))
                && self.opens_ex_from_palette(Key::Char(':'))
            {
                self.open_ex();
            } else {
                match key {
                    None => {
                        // Dropped keystrokes (Option-composed characters):
                        // input when they carry a character, else swallowed.
                        // ADR 0032 review m1: unlike a regular character
                        // key, the fallback never returns SwitchToInsert
                        // for a dropped key, so typing one in a normal-mode
                        // palette would leave `palette_insert` false and
                        // the next key would move the list instead of
                        // typing. Restore insert mode here as well.
                        let fallback =
                            overlay_fallback(is_palette, insert, None, key_char, modifiers);
                        if is_palette && !insert && fallback == OverlayFallback::Input {
                            self.palette_insert = true;
                        }
                        stop = fallback != OverlayFallback::Input;
                    }
                    Some(key) => {
                        if matches!(key, Key::Char(_)) && insert {
                            // Insert-mode text goes straight to the input
                            // handler; it must not stop propagation.
                            stop = false;
                        } else {
                            let context = self.key_context();
                            match self.keymap.feed(key, &context) {
                                KeymapOutcome::Command { id, args, values } => {
                                    if is_overlay_action_command(&id) {
                                        self.enqueue_command(Invocation::keymap_with_args(
                                            id, args, values,
                                        ));
                                    } else {
                                        // A non-overlay command matched (an
                                        // editor or global binding): fall
                                        // through to the fallback as if
                                        // the keymap ignored the key.
                                        stop = self.apply_overlay_fallback(
                                            is_palette, insert, key, key_char, modifiers,
                                        );
                                    }
                                }
                                KeymapOutcome::Pending => {}
                                KeymapOutcome::Ignored => {
                                    stop = self.apply_overlay_fallback(
                                        is_palette, insert, key, key_char, modifiers,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            self.sync_modal_focus(window, cx);
            if stop {
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

        // Tab moves widget focus inside a `ui` pane (ADR 0024 §3,
        // ADR 0032 Phase 1). Editor panes fall through to the keymap,
        // so insert-mode Tab still inserts a tab there.
        if matches!(key, Key::Tab | Key::BackTab) && self.move_widget_focus(key == Key::Tab) {
            self.sync_modal_focus(window, cx);
            cx.notify();
            return;
        }

        if self.apply_key(key) {
            self.sync_modal_focus(window, cx);
            cx.notify();
        }
    }

    /// Applies the fallback for an overlay key the keymap ignored (or
    /// matched to a non-overlay command). Returns whether to stop
    /// propagation.
    fn apply_overlay_fallback(
        &mut self,
        is_palette: bool,
        insert: bool,
        key: Key,
        key_char: Option<&str>,
        modifiers: &Modifiers,
    ) -> bool {
        match overlay_fallback(is_palette, insert, Some(key), key_char, modifiers) {
            OverlayFallback::Input => false,
            OverlayFallback::SwitchToInsert => {
                self.palette_insert = true;
                false
            }
            OverlayFallback::PopChar => {
                self.pop_overlay_char();
                true
            }
            OverlayFallback::Stop => true,
        }
    }

    /// Deletes the last overlay input character, as the old handlers did.
    /// Lists scroll back to the top after an edit: `pop_char` resets the
    /// selection to 0 for every list kind, so the viewport must follow
    /// (ADR 0032 review m3; previously only palette and settings did).
    fn pop_overlay_char(&mut self) {
        if let Some(surface) = self.surface_top_mut() {
            surface.kind.pop_char();
        }
        if matches!(
            self.surface_top_kind(),
            Some(SurfaceKind::Palette(_))
                | Some(SurfaceKind::Prompt(_))
                | Some(SurfaceKind::Approval(_))
                | Some(SurfaceKind::Settings(_))
                | Some(SurfaceKind::SettingChoice(_))
        ) && let Some(surface) = self.surface_top_mut()
        {
            surface.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
    }

    /// `:` in an empty command palette closes it and opens the Ex command line.
    fn opens_ex_from_palette(&self, key: Key) -> bool {
        if !matches!(key, Key::Char(':')) {
            return false;
        }
        self.surface_top_kind().is_some_and(|kind| {
            kind.palette().is_some_and(|palette| {
                palette.input.is_empty()
                    && matches!(palette.mode, vy_app::palette::PaletteMode::Commands)
            })
        })
    }

    /// Key hint for a command in the current overlay context, from the
    /// keymap (ADR 0032 Phase 3). Returns [`None`] when no binding
    /// matches, so overlay hints can fall back to hard-coded keys.
    /// Sequences are joined with a space (`g g`).
    pub(crate) fn key_hint(&self, command: &str) -> Option<String> {
        let context = self.key_context();
        let keys = self.keymap.keys_for(command, &context)?;
        Some(
            keys.iter()
                .map(|key| key.display_name())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    /// Builds the `when` context for the active pane (ADR 0032 Phase 1).
    /// `surfaceId` stays unset until Phase 4 introduces surfaces, and
    /// `uiRole` / `uiFocus` are set only while a focusable widget in the
    /// active pane's widget tree has focus.
    fn key_context(&self) -> KeyContext {
        let pane = self.active_pane_state();
        let (pane_kind, view_type) = match pane {
            Some(pane) => (
                pane_kind_for(pane.view_type.as_str()),
                pane.view_type.clone(),
            ),
            None => ("editor".to_string(), "editor".to_string()),
        };
        let overlay = match self.surface_top_kind() {
            None => "none",
            Some(kind) => overlay_name(kind),
        };
        let surface_id = self.surface_top().map(|surface| surface.id.clone());
        let (ui_role, ui_focus) = match pane {
            Some(pane) => match &pane.widget_tree {
                Some(tree) => (
                    pane.focus_path
                        .focused_role(tree)
                        .map(|role| role.to_string()),
                    pane.focus_path.last().cloned(),
                ),
                None => (None, None),
            },
            None => (None, None),
        };
        // While a palette is open, the keymap sees the palette input mode,
        // not the editor mode: `mode == 'normal'` bindings move the list,
        // `mode == 'insert'` bindings (and plain typing) filter it.
        let mode = match self.surface_top_kind() {
            Some(SurfaceKind::Palette(_)) => {
                if self.palette_insert {
                    Mode::Insert
                } else {
                    Mode::Normal
                }
            }
            _ => self.editor().mode(),
        };
        KeyContext {
            mode,
            has_selection: {
                let selection = self.editor().selection();
                selection.start != selection.end
            },
            editor_focus: pane_kind.as_str() == "editor",
            operator_pending: self.editor().operator_pending(),
            text_object_pending: self.editor().text_object_pending(),
            pane_id: self.active_pane,
            pane_kind,
            view_type,
            overlay: overlay.to_string(),
            surface_id,
            ui_role,
            ui_focus,
            ..KeyContext::default()
        }
    }

    /// Moves widget focus inside the active pane, if it is a `ui` pane
    /// with a widget tree. Returns false for editor panes and for panes
    /// with no focusable widget, so the key falls through to the keymap.
    fn move_widget_focus(&mut self, forward: bool) -> bool {
        let Some(pane_id) = self.active_pane else {
            return false;
        };
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return false;
        };
        if pane_kind_for(pane.view_type.as_str()).as_str() == "editor" {
            return false;
        }
        let Some(tree) = pane.widget_tree.clone() else {
            return false;
        };
        let next = if forward {
            pane.focus_path.next(&tree)
        } else {
            pane.focus_path.prev(&tree)
        };
        if next.is_empty() {
            return false;
        }
        pane.focus_path = next;
        true
    }

    /// Passes the key to the keymap and runs the command it resolves to.
    fn apply_key(&mut self, key: Key) -> bool {
        self.activate_active_view();
        let before = self.active_view();
        let before_scroll = self.active_pane_state().map(scroll_position);
        let before_version = self.editor().version();
        let before_overlay = !self.surfaces.is_empty();

        let context = self.key_context();
        // Focus moves via `vy.ui.*` change no editor state, so compare the
        // focus path too, or a focus-only key would not repaint.
        let before_focus = self.active_pane_state().map(|pane| pane.focus_path.clone());

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

        self.ensure_cursor_visible();

        // Do not repaint for keys that change nothing, such as a motion at a boundary.
        // Scrolling commands (`zz`, `zt`, `zb`, `zh`, `zl`) move the view without
        // touching the cursor, so the scroll offsets have to be compared as well.
        before != self.active_view()
            || before_scroll != self.active_pane_state().map(scroll_position)
            || before_version != self.editor().version()
            || before_overlay != !self.surfaces.is_empty()
            || before_focus != self.active_pane_state().map(|pane| pane.focus_path.clone())
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

/// Maps a pane's view id to the `paneKind` `when` value. Plugin views
/// are `ui` views; only the four known kinds keep their own name.
fn pane_kind_for(view_type: &str) -> String {
    match view_type {
        "editor" | "ui" | "webview" | "terminal" => view_type.to_string(),
        _ => "ui".to_string(),
    }
}

/// Maps an open overlay to the `overlay` `when` value (ADR 0032 §6).
/// The palette covers command and document lists; a plugin picker is
/// the `Custom` palette mode. Settings variants share `settings`.
fn overlay_name(kind: &SurfaceKind) -> &'static str {
    match kind {
        SurfaceKind::Palette(palette) => match &palette.mode {
            vy_app::palette::PaletteMode::Commands | vy_app::palette::PaletteMode::Documents => {
                "palette"
            }
            vy_app::palette::PaletteMode::Custom { .. } => "picker",
        },
        SurfaceKind::Ex(_) => "ex",
        SurfaceKind::Prompt(_) => "prompt",
        SurfaceKind::Approval(_) => "approval",
        SurfaceKind::Settings(_)
        | SurfaceKind::SettingChoice(_)
        | SurfaceKind::SettingPrompt(_) => "settings",
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Modifiers {
        Modifiers::default()
    }

    #[test]
    fn overlay_fallback_routes_text_and_keys() {
        // Plain printable text goes to the input handler.
        assert_eq!(
            overlay_fallback(false, true, Some(Key::Char('a')), Some("a"), &plain()),
            OverlayFallback::Input
        );
        // Shifted text is still text.
        assert_eq!(
            overlay_fallback(
                false,
                true,
                Some(Key::Char('A')),
                Some("A"),
                &Modifiers {
                    shift: true,
                    ..plain()
                },
            ),
            OverlayFallback::Input
        );
        // A normal-mode palette switches back to insert and types.
        assert_eq!(
            overlay_fallback(true, false, Some(Key::Char('j')), Some("j"), &plain()),
            OverlayFallback::SwitchToInsert
        );
        // An insert-mode palette types directly.
        assert_eq!(
            overlay_fallback(true, true, Some(Key::Char('j')), Some("j"), &plain()),
            OverlayFallback::Input
        );
        // Option-modified input (e.g. Option-s -> ß on macOS) is dropped by
        // `convert_key`, so it arrives with no `Key` but a `key_char`.
        assert_eq!(
            overlay_fallback(
                false,
                true,
                None,
                Some("ß"),
                &Modifiers {
                    alt: true,
                    ..plain()
                },
            ),
            OverlayFallback::Input
        );
        // Navigation and editing keys are swallowed by the modal overlay.
        assert_eq!(
            overlay_fallback(false, true, Some(Key::Enter), None, &plain()),
            OverlayFallback::Stop
        );
        assert_eq!(
            overlay_fallback(false, true, Some(Key::Tab), None, &plain()),
            OverlayFallback::Stop
        );
        // Backspace edits the input.
        assert_eq!(
            overlay_fallback(false, true, Some(Key::Backspace), None, &plain()),
            OverlayFallback::PopChar
        );
        // Control keys stay with the overlay (e.g. Ctrl-p / Ctrl-n selection).
        assert_eq!(
            overlay_fallback(
                false,
                true,
                Some(Key::Ctrl('u')),
                None,
                &Modifiers {
                    control: true,
                    ..plain()
                },
            ),
            OverlayFallback::Stop
        );
        // Cmd+V is not text here; paste is out of scope for this fix.
        assert_eq!(
            overlay_fallback(
                false,
                true,
                None,
                None,
                &Modifiers {
                    platform: true,
                    ..plain()
                },
            ),
            OverlayFallback::Stop
        );
        // An empty `key_char` with no converted key is not text.
        assert_eq!(
            overlay_fallback(false, true, None, Some(""), &plain()),
            OverlayFallback::Stop
        );
    }

    #[test]
    fn overlay_action_commands_are_vy_ui_actions() {
        // Overlay actions run through the keymap; editor, pane, and
        // overlay-opening commands must not leak through a modal overlay.
        assert!(is_overlay_action_command("vy.ui.next"));
        assert!(is_overlay_action_command("vy.ui.activate"));
        assert!(is_overlay_action_command("vy.ui.cancel"));
        assert!(is_overlay_action_command("vy.ui.clearLine"));
        assert!(is_overlay_action_command("vy.ui.completeNext"));
        assert!(is_overlay_action_command("vy.ui.ex_command_line"));
        assert!(!is_overlay_action_command("vy.ui.command_palette"));
        assert!(!is_overlay_action_command("vy.ui.open_settings"));
        assert!(!is_overlay_action_command("vy.motion.down"));
        assert!(!is_overlay_action_command("vy.editor.open"));
        assert!(!is_overlay_action_command("vy.ui.unknown"));
    }
}
