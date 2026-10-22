//! Command execution and palette/ex handling for `EditorView`.

use rmpv::Value;
use vy_app::commands::{
    BuiltinCommand, CommandDescriptor, DocumentCommand, Invocation, InvocationArgs, PaneCommand,
    UiCommand, parse_ex_line, resolve_ex_name,
};
use vy_app::editor::{Action, Key};
use vy_app::ex::ExState;
use vy_app::geometry::{collect_pane_ids, focus_neighbor};
use vy_app::palette::{PaletteState, filtered_palette_indices, move_palette_selection};
use vy_app::workspace::uri_from_argument;
use vy_proto::object;

use super::{EditorView, Pending};

impl EditorView {
    pub(super) fn run_pane_command(&mut self, command: PaneCommand) {
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

    pub(super) fn run_document_command(
        &mut self,
        command: DocumentCommand,
        invocation: &Invocation,
    ) {
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
    pub(super) fn open_split(&mut self, direction: &str) {
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

    pub(super) fn close_active_pane(&mut self) {
        let Some(pane_id) = self.active_pane else {
            return;
        };
        self.request(
            "pane/close",
            object([("pane_id", Value::from(pane_id))]),
            Pending::Close,
        );
    }

    pub(super) fn focus_direction(&mut self, direction: &str) {
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
        self.request(
            "pane/focus",
            object([("pane_id", Value::from(next))]),
            Pending::Focus,
        );
    }

    pub(super) fn palette_items(&self) -> Vec<CommandDescriptor> {
        self.commands
            .descriptors()
            .into_iter()
            .filter(|command| !command.hidden)
            .collect()
    }

    pub(super) fn palette_filtered_len(&self) -> usize {
        let Some(palette) = self.palette.as_ref() else {
            return 0;
        };
        filtered_palette_indices(&self.palette_items(), &palette.query).len()
    }

    pub(super) fn palette_selected_id(&self) -> Option<String> {
        let palette = self.palette.as_ref()?;
        let items = self.palette_items();
        let filtered = filtered_palette_indices(&items, &palette.query);
        let index = palette.selected.min(filtered.len().saturating_sub(1));
        filtered.get(index).map(|index| items[*index].id.clone())
    }

    pub(super) fn open_palette(&mut self) {
        self.clear_preedit();
        self.ex = None;
        self.palette = Some(PaletteState::default());
        self.status = "command palette".into();
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

    pub(super) fn handle_palette_key(&mut self, key: Key) -> bool {
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

    /// Runs one command. Returns true when it continues on the script thread.
    pub(super) fn execute_invocation(&mut self, invocation: Invocation) -> bool {
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

    pub(super) fn run_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::CommandPalette => self.open_palette(),
            UiCommand::ExCommandLine => self.open_ex(),
        }
    }
}
