//! Command execution and palette/ex handling for `EditorView`.

use gpui::ScrollStrategy;
use vy_app::commands::{
    BuiltinCommand, DocumentCommand, Invocation, InvocationArgs, Owner, PaneCommand, UiCommand,
    ViewCommand, parse_ex_line, resolve_ex_name,
};
use vy_app::editor::{Action, Key};
use vy_app::ex::ExState;
use vy_app::geometry::{collect_pane_ids, focus_neighbor};
use vy_app::palette::{
    PaletteMode, PaletteState, PickerItem, approval_picker_items, filtered_picker_indices,
    move_palette_selection,
};
use vy_app::state::DocumentState;
use vy_app::text_layout::{
    half_page_lines, scroll_cursor_bottom, scroll_cursor_center, scroll_cursor_top, scroll_lines,
};
use vy_app::workspace::{uri_from_argument_remote, uri_from_argument_with_base};
use vy_proto::{
    DocumentIdParams, DocumentSave, EditorOpen, EditorOpenParams, PaneClose, PaneCloseParams,
    PaneFocus, PaneFocusParams, PaneOpen, PaneOpenParams, PanePlacement,
};

use super::{EditorView, Pending};

impl EditorView {
    pub(super) fn run_pane_command(&mut self, command: PaneCommand) {
        match command {
            PaneCommand::SplitLeft => self.open_split("left"),
            PaneCommand::SplitRight => self.open_split("right"),
            PaneCommand::SplitUp => self.open_split("above"),
            PaneCommand::SplitDown => self.open_split("below"),
            PaneCommand::Close => self.close_active_pane(),
            PaneCommand::FocusLeft => self.focus_direction("left"),
            PaneCommand::FocusDown => self.focus_direction("down"),
            PaneCommand::FocusUp => self.focus_direction("up"),
            PaneCommand::FocusRight => self.focus_direction("right"),
            PaneCommand::Next => self.focus_next_pane(),
        }
    }

    pub(super) fn run_document_command(
        &mut self,
        command: DocumentCommand,
        invocation: &Invocation,
    ) {
        match command {
            DocumentCommand::Open => {
                let path_or_uri = if let Some(raw) = invocation.ex_raw() {
                    Some(raw.trim().to_string())
                } else if let InvocationArgs::Script(val) = &invocation.args {
                    if let Some(s) = val.as_str() {
                        Some(s.trim().to_string())
                    } else if let Some(obj) = val.as_object() {
                        obj.get("uri")
                            .or_else(|| obj.get("path"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let Some(path_or_uri) = path_or_uri else {
                    self.status = "editor.open requires a path".into();
                    return;
                };
                if path_or_uri.is_empty() {
                    self.status = "editor.open requires a path".into();
                    return;
                }

                let target_pane_id = if let InvocationArgs::Script(val) = &invocation.args {
                    val.get("pane_id")
                        .or_else(|| val.get("paneId"))
                        .and_then(|v| v.as_u64())
                } else {
                    None
                };

                let target_pane = target_pane_id.or_else(|| {
                    if let Some(active_id) = self.active_pane {
                        if let Some(active_pane) = self.panes.get(&active_id) {
                            if active_pane.view_type != "editor" {
                                self.panes
                                    .iter()
                                    .find(|(_, p)| p.view_type == "editor")
                                    .map(|(id, _)| *id)
                            } else {
                                Some(active_id)
                            }
                        } else {
                            Some(active_id)
                        }
                    } else {
                        None
                    }
                });

                let uri = if self.connection.is_remote() {
                    uri_from_argument_remote(&path_or_uri, Some(&self.workspace_uri))
                } else {
                    uri_from_argument_with_base(&path_or_uri, Some(&self.workspace_uri))
                };
                self.request::<EditorOpen>(
                    EditorOpenParams {
                        uri,
                        pane_id: target_pane,
                    },
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
                self.request::<DocumentSave>(DocumentIdParams { document_id }, Pending::Save);
            }
            DocumentCommand::Switch => {
                if let Some(raw) = invocation.ex_raw() {
                    let raw = raw.trim();
                    if !raw.is_empty() {
                        // Switch directly to matching document if argument provided.
                        let matched_uri = self.documents.values().find_map(|doc| {
                            if doc.id.to_string() == raw
                                || doc.uri.ends_with(raw)
                                || doc.uri.contains(raw)
                            {
                                Some(doc.uri.clone())
                            } else {
                                None
                            }
                        });
                        if let Some(uri) = matched_uri {
                            self.request::<EditorOpen>(
                                EditorOpenParams { uri, pane_id: None },
                                Pending::EditorOpen,
                            );
                            return;
                        } else {
                            self.status = format!("no buffer matches: {raw}").into();
                            return;
                        }
                    }
                }
                self.open_document_picker();
            }
        }
    }

    pub(super) fn run_view_command(&mut self, command: ViewCommand, invocation: &Invocation) {
        let count = invocation.keymap_args().count;
        self.activate_active_view();

        let Some(pane) = self.active_pane_state() else {
            return;
        };
        let viewport_rows = pane.viewport_rows;
        let scroll_top = pane.scroll_top;
        let Some(document) = self.active_document() else {
            return;
        };
        let line_count = document.editor.line_count();
        let cursor_line = document.editor.cursor().line as usize;

        match command {
            ViewCommand::ScrollHalfPageDown => {
                let delta = half_page_lines(viewport_rows, count.unwrap_or(1));
                self.editor_mut().move_vertical(delta as i64);
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_top =
                        scroll_lines(scroll_top, delta as isize, viewport_rows, line_count);
                }
                self.store_active_view();
                self.clamp_active_views();
                self.ensure_cursor_visible();
            }
            ViewCommand::ScrollHalfPageUp => {
                let delta = half_page_lines(viewport_rows, count.unwrap_or(1));
                self.editor_mut().move_vertical(-(delta as i64));
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_top =
                        scroll_lines(scroll_top, -(delta as isize), viewport_rows, line_count);
                }
                self.store_active_view();
                self.clamp_active_views();
                self.ensure_cursor_visible();
            }
            ViewCommand::CenterCursor => {
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_top = scroll_cursor_center(cursor_line, viewport_rows, line_count);
                }
            }
            ViewCommand::ScrollCursorTop => {
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_top = scroll_cursor_top(cursor_line, viewport_rows, line_count);
                }
            }
            ViewCommand::ScrollCursorBottom => {
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_top = scroll_cursor_bottom(cursor_line, viewport_rows, line_count);
                }
            }
        }
    }

    /// Splits the active pane and opens the same document in the new one.
    pub(super) fn open_split(&mut self, direction: &str) {
        let Some(target) = self.active_pane else {
            return;
        };
        let Some(uri) = self.active_document().map(|document| document.uri.clone()) else {
            self.status = "cannot split a pane without a document".into();
            return;
        };

        self.request::<PaneOpen>(
            PaneOpenParams {
                view: Some("editor".into()),
                uri,
                placement: Some(PanePlacement {
                    target: Some(target),
                    direction: Some(direction.to_string()),
                    ratio: None,
                }),
            },
            Pending::Open,
        );
        self.status = format!("splitting pane {target} {direction}").into();
    }

    pub(super) fn close_active_pane(&mut self) {
        let Some(pane_id) = self.active_pane else {
            return;
        };
        self.request::<PaneClose>(PaneCloseParams { pane_id }, Pending::Close);
    }

    pub(super) fn focus_direction(&mut self, direction: &str) {
        let (Some(layout), Some(active)) = (self.layout.as_ref(), self.active_pane) else {
            return;
        };
        let Some(pane_id) = focus_neighbor(layout, active, direction) else {
            return;
        };
        self.request::<PaneFocus>(PaneFocusParams { pane_id }, Pending::Focus);
    }

    pub(super) fn focus_next_pane(&mut self) {
        let (Some(layout), Some(active)) = (self.layout.as_ref(), self.active_pane) else {
            return;
        };
        let mut pane_ids = Vec::new();
        collect_pane_ids(layout, &mut pane_ids);
        let Some(index) = pane_ids.iter().position(|pane| *pane == active) else {
            return;
        };
        let next = pane_ids[(index + 1) % pane_ids.len()];
        self.request::<PaneFocus>(PaneFocusParams { pane_id: next }, Pending::Focus);
    }

    pub(super) fn palette_items(&self) -> Vec<PickerItem> {
        let Some(palette) = self.palette.as_ref() else {
            return Vec::new();
        };

        match palette.mode {
            PaletteMode::Commands => self
                .commands
                .descriptors()
                .into_iter()
                .filter(|command| !command.hidden)
                .map(|cmd| PickerItem::from(&cmd))
                .collect(),
            PaletteMode::Documents => {
                let mut docs: Vec<&DocumentState> = self.documents.values().collect();
                docs.sort_by_key(|doc| doc.id);
                docs.into_iter()
                    .map(|doc| {
                        let dirty = if doc.is_dirty() { " [+]" } else { "" };
                        let display_name = doc
                            .uri
                            .rsplit_once('/')
                            .map(|(_, name)| name)
                            .unwrap_or(&doc.uri);
                        let title = format!("{}{dirty}", display_name);
                        PickerItem::new(doc.uri.clone(), title)
                            .with_description(doc.uri.clone())
                            .with_detail(format!("#{}", doc.id))
                    })
                    .collect()
            }
            PaletteMode::Custom { ref items, .. } => items.clone(),
            PaletteMode::Approval {
                ref plugin_id,
                ref plugin_name,
                ref missing,
                ..
            } => approval_picker_items(plugin_id, plugin_name, missing),
        }
    }

    pub(super) fn palette_filtered_len(&self) -> usize {
        let Some(palette) = self.palette.as_ref() else {
            return 0;
        };
        filtered_picker_indices(&self.palette_items(), &palette.query).len()
    }

    pub(super) fn palette_selected_id(&self) -> Option<String> {
        let palette = self.palette.as_ref()?;
        let items = self.palette_items();
        let filtered = filtered_picker_indices(&items, &palette.query);
        let index = palette.selected.min(filtered.len().saturating_sub(1));
        filtered.get(index).map(|index| items[*index].id.clone())
    }

    pub(super) fn open_palette(&mut self) {
        self.clear_preedit();
        self.ex = None;
        self.palette = Some(PaletteState::with_mode(PaletteMode::Commands));
        self.palette_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.status = "command palette".into();
    }

    pub(super) fn open_document_picker(&mut self) {
        self.clear_preedit();
        self.ex = None;
        self.palette = Some(PaletteState::with_mode(PaletteMode::Documents));
        self.palette_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.status = "switch document".into();
    }

    pub(super) fn open_custom_picker(
        &mut self,
        id: String,
        items: Vec<PickerItem>,
        placeholder: Option<String>,
        request: vy_js::HostRequest,
    ) {
        self.clear_preedit();
        self.ex = None;
        let status_text = placeholder.clone().unwrap_or_else(|| "select item".into());
        self.palette = Some(PaletteState::with_mode(PaletteMode::Custom {
            id,
            items,
            placeholder,
            reply: Some(request),
        }));
        self.palette_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.status = status_text.into();
    }

    pub(super) fn open_ex(&mut self) {
        self.clear_preedit();
        self.palette = None;
        self.ex = Some(ExState::default());
        self.status = "ex command".into();
    }

    pub(super) fn handle_ex_key(&mut self, key: Key) {
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
                self.ex_history.record(line.clone());
                self.execute_ex_line(&line);
                return;
            }
            Key::Up | Key::Ctrl('p') => {
                if let Some(state) = self.ex.as_mut() {
                    self.ex_history.navigate(state, -1);
                }
                return;
            }
            Key::Down | Key::Ctrl('n') => {
                if let Some(state) = self.ex.as_mut() {
                    self.ex_history.navigate(state, 1);
                }
                return;
            }
            _ => {}
        }

        let Some(state) = self.ex.as_mut() else {
            return;
        };
        match key {
            Key::Backspace => state.pop_char(),
            Key::Ctrl('u') => state.clear(),
            Key::Char(character) => state.push_char(character),
            _ => {}
        }
    }

    pub(super) fn execute_ex_line(&mut self, line: &str) {
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

    pub(super) fn show_next_approval_prompt(&mut self) {
        if let Some(req) = self.plugin_manager.next_approval_request() {
            self.palette = Some(PaletteState::with_mode(PaletteMode::Approval {
                plugin_id: req.plugin_id.clone(),
                plugin_name: req.plugin_name.clone(),
                plugin_version: req.plugin_version.clone(),
                missing: req.missing_capabilities.clone(),
            }));
        }
    }

    pub(super) fn handle_palette_key(&mut self, key: Key) -> bool {
        match key {
            Key::Esc => {
                let mode = self.palette.take().map(|p| p.mode).unwrap_or_default();
                match mode {
                    PaletteMode::Custom {
                        reply: Some(req), ..
                    } => {
                        req.respond_value(serde_json::Value::Null);
                    }
                    PaletteMode::Approval {
                        plugin_id,
                        plugin_name,
                        ..
                    } => {
                        self.plugin_manager.deny(&plugin_id);
                        self.status =
                            format!("Denied capabilities for plugin {plugin_name}").into();
                        self.show_next_approval_prompt();
                    }
                    _ => {}
                }
                return true;
            }
            Key::Enter => {
                let Some(id) = self.palette_selected_id() else {
                    return true;
                };
                let mode = self.palette.take().map(|p| p.mode).unwrap_or_default();
                match mode {
                    PaletteMode::Commands => {
                        self.enqueue_command(Invocation::palette(id));
                    }
                    PaletteMode::Documents => {
                        self.request::<EditorOpen>(
                            EditorOpenParams {
                                uri: id,
                                pane_id: None,
                            },
                            Pending::EditorOpen,
                        );
                    }
                    PaletteMode::Custom { reply, .. } => {
                        if let Some(req) = reply {
                            req.respond_value(serde_json::json!({ "selectedId": id }));
                        }
                    }
                    PaletteMode::Approval {
                        plugin_id,
                        plugin_name,
                        plugin_version,
                        missing,
                    } => {
                        if id == "trust" {
                            if let Err(err) = self.plugin_manager.approve(&plugin_id) {
                                self.status = format!("Error approving plugin: {err}").into();
                            } else {
                                self.status =
                                    format!("Approved and enabled plugin {plugin_name}").into();
                                self.on_plugin_approved(&plugin_id);
                            }
                            self.show_next_approval_prompt();
                        } else if id == "deny" {
                            self.plugin_manager.deny(&plugin_id);
                            self.status =
                                format!("Denied capabilities for plugin {plugin_name}").into();
                            self.show_next_approval_prompt();
                        } else {
                            // User selected an info row; keep approval dialog open
                            self.palette = Some(PaletteState::with_mode(PaletteMode::Approval {
                                plugin_id,
                                plugin_name,
                                plugin_version,
                                missing,
                            }));
                        }
                    }
                }
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
                self.palette_scroll_handle
                    .scroll_to_item(palette.selected, ScrollStrategy::Nearest);
            }
            Key::Down | Key::Ctrl('n') => {
                palette.selected = move_palette_selection(palette.selected, 1, len);
                self.palette_scroll_handle
                    .scroll_to_item(palette.selected, ScrollStrategy::Nearest);
            }
            Key::Backspace => {
                palette.query.pop();
                palette.selected = 0;
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            Key::Char(character) => {
                palette.query.push(character);
                palette.selected = 0;
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            _ => {}
        }
        true
    }

    /// Queues a command and starts it when the previous command is done.
    pub(super) fn enqueue_command(&mut self, invocation: Invocation) {
        self.command_queue.push_back(invocation);
        self.drain_command_queue();
    }

    pub(super) fn drain_command_queue(&mut self) {
        while self.running_command.is_none() {
            let Some(invocation) = self.command_queue.pop_front() else {
                break;
            };
            if self.execute_invocation(invocation) {
                break;
            }
        }
    }

    /// Runs a registered script command on the runtime that owns it.
    pub(super) fn execute_script_command(&mut self, id: &str, args: serde_json::Value) {
        self.activate_plugins_for_command(id);
        let owner = self.commands.owner_of(id).cloned();
        match owner {
            Some(Owner::Plugin(plugin_id)) => {
                if let Some(runtime) = self.plugin_runtimes.get(&plugin_id) {
                    runtime.host.execute(id.to_string(), args);
                } else {
                    self.status = format!("{id}: plugin {plugin_id} is not running").into();
                }
            }
            _ => {
                if let Some(plugin) = self.plugin.as_ref() {
                    plugin.execute(id.to_string(), args);
                } else {
                    self.status = format!("{id}: the script host is not running").into();
                }
            }
        }
    }

    /// Runs one command. Returns true when it continues on the script thread.
    pub(super) fn execute_invocation(&mut self, invocation: Invocation) -> bool {
        let id = invocation.id.clone();
        self.activate_plugins_for_command(&id);

        if let Some(command) = UiCommand::parse(&id) {
            self.run_ui_command(command, &invocation);
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
        if let Some(command) = ViewCommand::parse(&id) {
            self.run_view_command(command, &invocation);
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
                Action::Undo => self.send_history::<vy_proto::BufferUndo>(Pending::Undo),
                Action::Redo => self.send_history::<vy_proto::BufferRedo>(Pending::Redo),
            }
            self.ensure_cursor_visible();
            return false;
        }
        if self.commands.is_registered(&id) {
            // Script commands run on the script thread. Any edits they make come
            // back as host requests.
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
            self.execute_script_command(&id, args);
            self.status = format!("running {id}").into();
            return false;
        }

        self.status = format!("unknown command: {id}").into();
        false
    }

    pub(super) fn run_ui_command(&mut self, command: UiCommand, invocation: &Invocation) {
        match command {
            UiCommand::CommandPalette => self.open_palette(),
            UiCommand::ExCommandLine => self.open_ex(),
            UiCommand::ReconnectServer => {
                self.reconnect_requested = true;
                self.status = "reconnecting".into();
            }
            UiCommand::ScriptEval => {
                let Some(raw) = invocation.ex_raw() else {
                    self.status = "script.eval requires an expression".into();
                    return;
                };
                let raw = raw.trim();
                if raw.is_empty() {
                    self.status = "script.eval requires an expression".into();
                    return;
                }
                if let Some(plugin) = self.plugin.as_ref() {
                    plugin.eval(raw);
                } else {
                    self.status = "the script host is not running".into();
                }
            }
            UiCommand::ViewOpen => {
                let (view_id, direction) = parse_view_args(invocation);
                if let Some(view_id) = view_id {
                    self.open_view_pane(&view_id, &direction);
                }
            }
            UiCommand::ViewClose => {
                let (view_id, _) = parse_view_args(invocation);
                if let Some(view_id) = view_id {
                    self.close_view_pane(&view_id);
                }
            }
            UiCommand::ViewToggle => {
                let (view_id, direction) = parse_view_args(invocation);
                if let Some(view_id) = view_id {
                    self.toggle_view_pane(&view_id, &direction);
                }
            }
        }
    }

    /// Executes a command specified as a string (e.g. `"vy.editor.open foo.txt"` or `"my.tree.toggle"`).
    pub(crate) fn execute_command_string(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        let (id, args) = match cmd.split_once(' ') {
            Some((id, args)) => (id, Some(args.trim())),
            None => (cmd, None),
        };
        let invocation = if let Some(raw) = args {
            Invocation::ex(id, raw)
        } else {
            Invocation::palette(id)
        };
        self.execute_invocation(invocation);
    }

    pub(super) fn open_view_pane(&mut self, view_id: &str, direction: &str) {
        // If a pane with this view already exists, focus it
        if let Some((pane_id, pane)) = self.panes.iter_mut().find(|(_, p)| p.view_type == view_id) {
            if pane.widget_tree.is_none() {
                pane.widget_tree = self.view_widget_trees.get(view_id).cloned();
            }
            let pid = *pane_id;
            self.active_pane = Some(pid);
            return;
        }

        self.activate_plugins_for_view(view_id);

        let Some(target) = self
            .active_pane
            .or_else(|| self.panes.keys().next().copied())
        else {
            return;
        };
        let uri = self
            .active_document()
            .map(|d| d.uri.clone())
            .unwrap_or_else(|| {
                self.documents
                    .values()
                    .next()
                    .map(|d| d.uri.clone())
                    .unwrap_or_else(|| "untitled:1".into())
            });

        self.pending_view_for_next_open = Some(view_id.to_string());
        self.request::<PaneOpen>(
            PaneOpenParams {
                view: Some(view_id.to_string()),
                uri,
                placement: Some(PanePlacement {
                    target: Some(target),
                    direction: Some(direction.to_string()),
                    ratio: Some(0.25),
                }),
            },
            Pending::Open,
        );
    }

    pub(super) fn close_view_pane(&mut self, view_id: &str) {
        if let Some((pane_id, _)) = self.panes.iter().find(|(_, p)| p.view_type == view_id) {
            let pid = *pane_id;
            self.request::<PaneClose>(PaneCloseParams { pane_id: pid }, Pending::Close);
        }
    }

    pub(super) fn toggle_view_pane(&mut self, view_id: &str, direction: &str) {
        if self.panes.values().any(|p| p.view_type == view_id) {
            self.close_view_pane(view_id);
        } else {
            self.open_view_pane(view_id, direction);
        }
    }
}

fn parse_view_args(invocation: &Invocation) -> (Option<String>, String) {
    if let Some(raw) = invocation.ex_raw() {
        let parts: Vec<&str> = raw.split_whitespace().collect();
        let view_id = parts.first().map(|s| s.to_string());
        let dir = parts.get(1).unwrap_or(&"left").to_string();
        (view_id, dir)
    } else if let InvocationArgs::Script(val) = &invocation.args {
        if let Some(s) = val.as_str() {
            (Some(s.to_string()), "left".to_string())
        } else if let Some(obj) = val.as_object() {
            let view_id = obj
                .get("view")
                .or_else(|| obj.get("id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let dir = obj
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("left")
                .to_string();
            (view_id, dir)
        } else {
            (None, "left".to_string())
        }
    } else {
        (None, "left".to_string())
    }
}
