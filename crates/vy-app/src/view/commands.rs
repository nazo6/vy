//! Command execution and palette/ex handling for `EditorView`.

use gpui::ScrollStrategy;
use vy_app::commands::{
    ArgKind, BuiltinCommand, CommandDescriptor, CommandSource, DocumentCommand, ExAddress, ExRange,
    Invocation, InvocationSource, KeymapArgs, LineRange, Owner, PaletteVisibility, PaneCommand,
    RangeSpec, SettingCommand, UiAction, UiActionCommand, UiCommand, ViewCommand, argument_error,
    ex_name_candidates, parse_ex_args, parse_ex_line, resolve_ex_name, validate_json_args,
};
use vy_app::editor::{Action, Key, Mode};
use vy_app::ex::ExState;
use vy_app::geometry::{collect_pane_ids, focus_neighbor};
use vy_app::overlay::{
    ApprovalState, CompletionCandidate, Overlay, PromptState, SettingChoiceState,
    SettingPromptState, SettingsState,
};
use vy_app::palette::{
    PaletteMode, PaletteState, PickerItem, RECENT_GROUP, approval_picker_items,
    filtered_picker_indices, move_palette_selection, palette_row_index, push_recent_command,
    shows_palette_headings,
};
use vy_app::state::DocumentState;
use vy_app::text_layout::{
    half_page_lines, scroll_cursor_bottom, scroll_cursor_center, scroll_cursor_top, scroll_lines,
};
use vy_app::ui_action::{self, UiActionArgs, UiActionOutcome};
use vy_app::workspace::{uri_from_argument_remote, uri_from_argument_with_base};
use vy_client_api::settings::{self, SettingKind, SettingScope};
use vy_proto::{
    DocumentIdParams, DocumentSave, EditorOpen, EditorOpenParams, PaneClose, PaneCloseParams,
    PaneFocus, PaneFocusParams, PaneOpen, PaneOpenParams, PanePlacement, PaneResize,
    PaneResizeParams,
};

use super::{EditorView, Pending};

impl EditorView {
    pub(super) fn run_pane_command(&mut self, command: PaneCommand, invocation: &Invocation) {
        match command {
            PaneCommand::SplitLeft => self.open_split("left"),
            PaneCommand::SplitRight => self.open_split("right"),
            PaneCommand::SplitUp => self.open_split("above"),
            PaneCommand::SplitDown => self.open_split("below"),
            PaneCommand::Close => self.close_active_pane(invocation.bang),
            PaneCommand::FocusLeft => self.focus_direction("left"),
            PaneCommand::FocusDown => self.focus_direction("down"),
            PaneCommand::FocusUp => self.focus_direction("up"),
            PaneCommand::FocusRight => self.focus_direction("right"),
            PaneCommand::Next => self.focus_next_pane(),
            PaneCommand::IncreaseWidth => {
                let count = invocation.args.count.unwrap_or(1) as f32;
                self.resize_active_pane(Some("horizontal"), Some(0.05 * count), None);
            }
            PaneCommand::DecreaseWidth => {
                let count = invocation.args.count.unwrap_or(1) as f32;
                self.resize_active_pane(Some("horizontal"), Some(-0.05 * count), None);
            }
            PaneCommand::IncreaseHeight => {
                let count = invocation.args.count.unwrap_or(1) as f32;
                self.resize_active_pane(Some("vertical"), Some(0.05 * count), None);
            }
            PaneCommand::DecreaseHeight => {
                let count = invocation.args.count.unwrap_or(1) as f32;
                self.resize_active_pane(Some("vertical"), Some(-0.05 * count), None);
            }
            PaneCommand::Equalize => self.equalize_panes(),
            PaneCommand::Resize => self.run_pane_resize_command(invocation),
        }
    }

    pub(super) fn resize_active_pane(
        &mut self,
        direction: Option<&str>,
        delta: Option<f32>,
        ratio: Option<f32>,
    ) {
        let Some(pane_id) = self.active_pane else {
            return;
        };
        self.request::<PaneResize>(
            PaneResizeParams {
                pane_id: Some(pane_id),
                direction: direction.map(str::to_string),
                delta,
                ratio,
                ..Default::default()
            },
            Pending::Resize,
        );
    }

    pub(super) fn equalize_panes(&mut self) {
        self.request::<PaneResize>(
            PaneResizeParams {
                equalize: Some(true),
                ..Default::default()
            },
            Pending::Resize,
        );
    }

    pub(super) fn run_pane_resize_command(&mut self, invocation: &Invocation) {
        let delta = invocation
            .args
            .f64("delta")
            .or_else(|| invocation.args.f64("amount"))
            .map(|v| v as f32);
        let ratio = invocation.args.f64("ratio").map(|v| v as f32);
        let direction = invocation.args.string("direction").map(str::to_string);

        let (delta, ratio) = if delta.is_none() && ratio.is_none() {
            if let Some(raw) = invocation
                .args
                .string("_raw")
                .or_else(|| invocation.args.string("value"))
            {
                let trimmed = raw.trim();
                if let Some(rest) = trimmed.strip_prefix('+') {
                    (rest.parse::<f32>().ok().map(|n| n * 0.02), None)
                } else if let Some(rest) = trimmed.strip_prefix('-') {
                    (rest.parse::<f32>().ok().map(|n| -n * 0.02), None)
                } else if let Ok(val) = trimmed.parse::<f32>() {
                    if val > 0.0 && val < 1.0 {
                        (None, Some(val))
                    } else if (1.0..=100.0).contains(&val) {
                        (None, Some(val / 100.0))
                    } else {
                        (Some(0.05), None)
                    }
                } else {
                    (Some(0.05), None)
                }
            } else {
                (Some(0.05), None)
            }
        } else {
            (delta, ratio)
        };

        self.resize_active_pane(direction.as_deref().or(Some("vertical")), delta, ratio);
    }

    pub(super) fn run_document_command(
        &mut self,
        command: DocumentCommand,
        invocation: &Invocation,
    ) {
        match command {
            DocumentCommand::Open => {
                let path_or_uri = invocation
                    .args
                    .string("uri")
                    .or_else(|| invocation.args.string("path"))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);

                let Some(path_or_uri) = path_or_uri else {
                    self.status = "editor.open requires a path".into();
                    return;
                };

                let target_pane_id = invocation
                    .args
                    .u64("paneId")
                    .or_else(|| invocation.args.u64("pane_id"));

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
                    .args
                    .string("path")
                    .is_some_and(|path| !path.trim().is_empty())
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
                if let Some(raw) = invocation
                    .args
                    .string("document")
                    .or_else(|| invocation.args.string("uri"))
                {
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
            ViewCommand::ScrollLeft => {
                let delta = count.unwrap_or(4) as isize;
                let max_width = self.max_line_display_width();
                if let Some(pane) = self.active_pane_state_mut() {
                    let cols = pane.viewport_cols;
                    pane.scroll_left = vy_app::text_layout::scroll_cols_clamped(
                        pane.scroll_left,
                        -delta,
                        cols,
                        max_width,
                    );
                }
            }
            ViewCommand::ScrollRight => {
                let delta = count.unwrap_or(4) as isize;
                let max_width = self.max_line_display_width();
                if let Some(pane) = self.active_pane_state_mut() {
                    let cols = pane.viewport_cols;
                    pane.scroll_left = vy_app::text_layout::scroll_cols_clamped(
                        pane.scroll_left,
                        delta,
                        cols,
                        max_width,
                    );
                }
            }
            ViewCommand::ToggleWrap => {
                let current = self
                    .active_pane_state()
                    .map(|p| p.is_wrap(&self.config))
                    .unwrap_or(false);
                let next = !current;
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.set_window_setting("editor.wrap", serde_json::Value::Bool(next));
                    if next {
                        pane.scroll_left = 0;
                    } else {
                        pane.scroll_sub_line = 0;
                    }
                }
                self.ensure_cursor_visible();
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

    pub(super) fn max_line_display_width(&self) -> usize {
        let Some(document) = self.active_document() else {
            return 0;
        };
        (0..document.editor.line_count())
            .filter_map(|line_idx| document.editor.line(line_idx))
            .map(|text| {
                vy_app::text_layout::str_display_width(
                    &text,
                    0,
                    vy_app::text_layout::DEFAULT_TAB_STOP,
                )
            })
            .max()
            .unwrap_or(0)
    }

    pub(super) fn close_active_pane(&mut self, force: bool) {
        let Some(pane_id) = self.active_pane else {
            return;
        };
        self.request::<PaneClose>(PaneCloseParams { pane_id, force }, Pending::Close);
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

    /// Rows shown in the floating overlay, if it has a list.
    pub(super) fn overlay_items(&self) -> Vec<PickerItem> {
        match self.overlay.as_ref() {
            Some(Overlay::Palette(palette)) => self.palette_mode_items(&palette.mode),
            Some(Overlay::Prompt(prompt)) => self.prompt_items(prompt),
            Some(Overlay::Approval(approval)) => approval_picker_items(
                &approval.plugin_id,
                &approval.plugin_name,
                &approval.missing,
            ),
            Some(Overlay::Settings(_)) => self.setting_items(),
            Some(Overlay::SettingChoice(choice)) => setting_choice_items(choice),
            _ => Vec::new(),
        }
    }

    /// One row per setting with its current value, scope, and source.
    fn setting_items(&self) -> Vec<PickerItem> {
        let mut items: Vec<PickerItem> = settings::SETTINGS
            .iter()
            .map(|decl| {
                let (value, source) = self.active_setting(decl.key);
                let text = value
                    .as_ref()
                    .map(setting_text)
                    .unwrap_or_else(|| "(unset)".to_string());
                PickerItem {
                    id: decl.key.to_string(),
                    title: decl.key.to_string(),
                    description: Some(text),
                    detail: Some(format!("{} · {}", decl.scope.label(), source.label())),
                    group: None,
                }
            })
            .collect();
        vy_app::palette::sort_picker_items(&mut items);
        items
    }

    /// Resolves a setting for the active pane, falling back to the config layers.
    fn active_setting(
        &self,
        key: &str,
    ) -> (Option<serde_json::Value>, vy_app::config::SettingSource) {
        match self.active_pane_state() {
            Some(pane) => pane.effective_setting(key, &self.config),
            None => self.config.effective_with_source(key),
        }
    }

    /// Choices for the argument the prompt is collecting.
    fn prompt_items(&self, prompt: &PromptState) -> Vec<PickerItem> {
        let Some(arg) = prompt.current_arg() else {
            return Vec::new();
        };
        if arg.kind != ArgKind::Choice {
            return Vec::new();
        }
        arg.choices
            .iter()
            .map(|choice| PickerItem::new(choice.clone(), choice.clone()))
            .collect()
    }

    fn palette_mode_items(&self, mode: &PaletteMode) -> Vec<PickerItem> {
        match mode {
            PaletteMode::Commands => {
                self.command_palette_items(self.overlay_input_value().trim().is_empty())
            }
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
            PaletteMode::Custom { items, .. } => items.clone(),
        }
    }

    /// Command palette rows, grouped by category with recent commands on top.
    fn command_palette_items(&self, show_recent: bool) -> Vec<PickerItem> {
        let mut items: Vec<PickerItem> = self
            .commands
            .descriptors()
            .into_iter()
            .filter(|command| command.palette == PaletteVisibility::Listed)
            .map(|command| PickerItem::from(&command))
            .collect();
        vy_app::palette::sort_picker_items(&mut items);

        if !show_recent {
            return items;
        }
        let recent: Vec<PickerItem> = self
            .recent_commands
            .iter()
            .filter_map(|id| items.iter().find(|item| &item.id == id).cloned())
            .map(|item| PickerItem {
                group: Some(RECENT_GROUP.to_string()),
                ..item
            })
            .collect();
        if recent.is_empty() {
            return items;
        }
        items.retain(|item| !recent.iter().any(|recent| recent.id == item.id));
        recent.into_iter().chain(items).collect()
    }

    fn overlay_input_value(&self) -> &str {
        self.overlay
            .as_ref()
            .map(|overlay| overlay.input().value())
            .unwrap_or("")
    }

    pub(super) fn overlay_filtered_len(&self) -> usize {
        if self.overlay.is_none() {
            return 0;
        }
        filtered_picker_indices(&self.overlay_items(), self.overlay_input_value()).len()
    }

    pub(super) fn overlay_selected_id(&self) -> Option<String> {
        let selected = self.overlay.as_ref()?.selected()?;
        let items = self.overlay_items();
        let filtered = filtered_picker_indices(&items, self.overlay_input_value());
        let index = selected.min(filtered.len().saturating_sub(1));
        filtered.get(index).map(|index| items[*index].id.clone())
    }

    /// Moves the selected row of the active overlay's list.
    fn move_overlay_selection(&mut self, delta: isize) {
        let len = self.overlay_filtered_len();
        let items = self.overlay_items();
        let input = self.overlay_input_value().to_string();
        let filtered = filtered_picker_indices(&items, &input);
        let headings = shows_palette_headings(&input);
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        let Some(selected) = overlay.selected_mut() else {
            return;
        };
        *selected = move_palette_selection(*selected, delta, len);
        let row = palette_row_index(&items, &filtered, headings, *selected);
        self.palette_scroll_handle
            .scroll_to_item(row, ScrollStrategy::Nearest);
    }

    pub(super) fn open_palette(&mut self) {
        self.open_palette_mode(PaletteMode::Commands, "command palette");
    }

    pub(super) fn open_document_picker(&mut self) {
        self.open_palette_mode(PaletteMode::Documents, "switch document");
    }

    fn open_palette_mode(&mut self, mode: PaletteMode, status: &str) {
        self.clear_preedit();
        self.overlay = Some(Overlay::Palette(PaletteState::with_mode(mode)));
        self.palette_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.status = status.into();
    }

    pub(super) fn open_custom_picker(
        &mut self,
        id: String,
        items: Vec<PickerItem>,
        placeholder: Option<String>,
        request: vy_js::HostRequest,
    ) {
        let status_text = placeholder.clone().unwrap_or_else(|| "select item".into());
        self.open_palette_mode(
            PaletteMode::Custom {
                id,
                items,
                placeholder,
                reply: Some(request),
            },
            &status_text,
        );
    }

    pub(super) fn open_ex(&mut self) {
        self.clear_preedit();
        self.overlay = Some(Overlay::Ex(ExState::default()));
        self.status = "ex command".into();
    }

    /// Opens the settings list (`:set` without arguments, or `vy.ui.open_settings`).
    pub(super) fn open_settings(&mut self) {
        self.clear_preedit();
        self.overlay = Some(Overlay::Settings(SettingsState::new()));
        self.palette_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.status = "settings".into();
    }

    /// Starts collecting the required arguments of a palette command.
    fn begin_palette_command(&mut self, id: String, palette: PaletteState) {
        let Some(descriptor) = self
            .commands
            .descriptors()
            .into_iter()
            .find(|command| command.id == id)
        else {
            return;
        };
        let required = descriptor
            .args
            .args()
            .iter()
            .filter(|arg| arg.required)
            .count();
        if required == 0 {
            self.enqueue_command(Invocation::palette(id));
            return;
        }

        let title = descriptor.title;
        let arg_name = descriptor
            .args
            .args()
            .iter()
            .find(|arg| arg.required)
            .map(|arg| arg.name.clone())
            .unwrap_or_default();
        self.overlay = Some(Overlay::Prompt(PromptState::new(
            id,
            title.clone(),
            descriptor.args,
            palette,
        )));
        self.palette_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.status = format!("{title}: {arg_name}").into();
    }

    /// Stores the current argument and either asks for the next one or runs the command.
    fn confirm_prompt_argument(&mut self) {
        let Some(Overlay::Prompt(prompt)) = self.overlay.as_ref() else {
            return;
        };
        let Some(arg) = prompt.current_arg() else {
            return;
        };
        let arg_name = arg.name.clone();
        let arg_kind = arg.kind;
        let command_id = prompt.command_id.clone();
        let title = prompt.title.clone();
        let spec = prompt.spec.clone();
        let value = if arg_kind == ArgKind::Choice {
            match self.overlay_selected_id() {
                Some(value) => value,
                None => {
                    self.status = format!("missing argument: {arg_name}").into();
                    return;
                }
            }
        } else {
            prompt.input.value().trim().to_string()
        };
        if value.is_empty() {
            self.status = format!("missing argument: {arg_name}").into();
            return;
        }

        let Some(Overlay::Prompt(prompt)) = self.overlay.as_mut() else {
            return;
        };
        prompt
            .values
            .insert(arg_name, serde_json::Value::String(value));
        let next_index = prompt.index + 1;
        if next_index < prompt.required().len() {
            let next_name = prompt.required()[next_index].name.clone();
            prompt.index = next_index;
            prompt.input.clear();
            prompt.selected = 0;
            self.status = format!("{title}: {next_name}").into();
            self.palette_scroll_handle
                .scroll_to_item(0, ScrollStrategy::Top);
            return;
        }

        let values = serde_json::Value::Object(prompt.values.clone());
        match validate_json_args(&spec, &values) {
            Ok(values) => {
                self.overlay = None;
                self.enqueue_command(Invocation::palette_with_args(command_id, values));
            }
            Err(error) => {
                self.status = argument_error(&command_id, &spec, &error).into();
            }
        }
    }

    /// Fallback key handling for the Ex line.
    ///
    /// `Esc`, `Enter`, history, completion, `Backspace`, and `Ctrl-u` are bound
    /// in the default keymap to `vy.ui.*`, so they normally never reach here.
    /// This remains the fallback for a key the keymap does not resolve, which
    /// keeps a user who unbinds those commands from losing text editing.
    pub(super) fn handle_ex_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.overlay = None;
                return;
            }
            Key::Enter => {
                let Some(Overlay::Ex(ex)) = self.overlay.take() else {
                    return;
                };
                let line = ex.input.value().trim().to_string();
                if line.is_empty() {
                    return;
                }
                self.ex_history.record(line.clone());
                self.execute_ex_line(&line);
                return;
            }
            Key::Up | Key::Ctrl('p') => {
                if let Some(Overlay::Ex(ex)) = self.overlay.as_mut() {
                    self.ex_history.navigate(ex, -1);
                }
                return;
            }
            Key::Down | Key::Ctrl('n') => {
                if let Some(Overlay::Ex(ex)) = self.overlay.as_mut() {
                    self.ex_history.navigate(ex, 1);
                }
                return;
            }
            Key::Tab => {
                self.complete_ex_name(false);
                return;
            }
            Key::BackTab => {
                self.complete_ex_name(true);
                return;
            }
            _ => {}
        }

        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        match key {
            Key::Backspace => overlay.pop_char(),
            Key::Ctrl('u') => overlay.clear(),
            Key::Char(character) => overlay.push_char(character),
            _ => {}
        }
    }

    /// Cycles Ex completion: command names, then names and values after `:set`.
    fn complete_ex_name(&mut self, backwards: bool) {
        if self.overlay_input_value().trim().is_empty() {
            return;
        }
        let descriptors = self.commands.descriptors();
        let Some(Overlay::Ex(ex)) = self.overlay.as_mut() else {
            return;
        };
        let typed = ex.input.value().to_string();
        if ex.completion.is_active_for(&typed) {
            ex.completion.cycle(if backwards { -1 } else { 1 });
        } else {
            let candidates = ex_completion_candidates(&typed, &descriptors);
            if candidates.is_empty() {
                ex.completion.reset();
                return;
            }
            ex.completion.set(typed, candidates);
            if backwards {
                ex.completion.select_last();
            }
        }
        if let Some(candidate) = ex.completion.selected().map(str::to_string) {
            ex.input.set_value(candidate);
            ex.history_index = None;
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
        let descriptors = self.commands.descriptors();
        let command = match resolve_ex_name(&parsed.name, &descriptors) {
            Ok(command) => command,
            Err(error) => {
                self.status = format!("{error}").into();
                return;
            }
        };
        if parsed.bang && !command.bang {
            self.status = format!("Ex command `{}` does not accept `!`", parsed.name).into();
            return;
        }
        let values = match parse_ex_args(&command.args, &parsed.args) {
            Ok(values) => values,
            Err(error) => {
                self.status = argument_error(&parsed.name, &command.args, &error).into();
                return;
            }
        };
        let range = match parsed.range {
            Some(range) => {
                if command.range == RangeSpec::None {
                    self.status =
                        format!("Ex command `{}` does not accept a range", parsed.name).into();
                    return;
                }
                match self.resolve_ex_range(range) {
                    Ok(range) => Some(range),
                    Err(error) => {
                        self.status = error.into();
                        return;
                    }
                }
            }
            None => None,
        };
        let id = command.id.clone();

        let bang_str = if parsed.bang { "!" } else { "" };
        self.status = format!(":{}{}", parsed.name, bang_str).into();
        self.enqueue_command(
            Invocation::ex(id, values)
                .with_range(range)
                .with_bang(parsed.bang),
        );
    }

    /// Resolves parsed Ex addresses against the active document.
    fn resolve_ex_range(&self, range: ExRange) -> Result<LineRange, String> {
        let Some(document) = self.active_document() else {
            return Err("Ex range requires an active document".into());
        };
        let line_count = document.editor.line_count().max(1) as u32;
        let cursor_line = document.editor.cursor().line + 1;
        let last_visual = document.editor.last_visual();

        let resolve = |address: ExAddress| -> Result<u32, String> {
            let line = match address {
                ExAddress::Line(line) => line,
                ExAddress::Current => cursor_line,
                ExAddress::Last => line_count,
                ExAddress::LastVisualStart => last_visual
                    .map(|range| range.start.line + 1)
                    .ok_or("no previous Visual range")?,
                ExAddress::LastVisualEnd => last_visual
                    .map(|range| range.end.line + 1)
                    .ok_or("no previous Visual range")?,
                ExAddress::Relative(delta) => {
                    let line = cursor_line as i64 + delta;
                    if line < 1 {
                        return Err(format!("line {line} is out of range"));
                    }
                    line as u32
                }
            };
            if line == 0 || line > line_count {
                return Err(format!("line {line} is out of range"));
            }
            Ok(line)
        };

        let start = resolve(range.start)?;
        let end = resolve(range.end)?;
        if start > end {
            return Err(format!("range start {start} is after end {end}"));
        }
        Ok(LineRange { start, end })
    }

    pub(super) fn show_next_approval_prompt(&mut self) {
        if let Some(req) = self.plugin_manager.next_approval_request() {
            self.overlay = Some(Overlay::Approval(ApprovalState::new(
                req.plugin_id.clone(),
                req.plugin_name.clone(),
                req.plugin_version.clone(),
                req.missing_capabilities.clone(),
            )));
        }
    }

    /// Dispatches one key to the active overlay.
    pub(super) fn handle_overlay_key(&mut self, key: Key) {
        match self.overlay.as_ref() {
            Some(Overlay::Palette(_)) => self.handle_palette_key(key),
            Some(Overlay::Ex(_)) => self.handle_ex_key(key),
            Some(Overlay::Prompt(_)) => self.handle_prompt_key(key),
            Some(Overlay::Approval(_)) => self.handle_approval_key(key),
            Some(Overlay::Settings(_)) => self.handle_settings_key(key),
            Some(Overlay::SettingChoice(_)) => self.handle_setting_choice_key(key),
            Some(Overlay::SettingPrompt(_)) => self.handle_setting_prompt_key(key),
            None => {}
        }
    }

    fn handle_palette_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                let Some(Overlay::Palette(palette)) = self.overlay.take() else {
                    return;
                };
                if let PaletteMode::Custom {
                    reply: Some(request),
                    ..
                } = palette.mode
                {
                    request.respond_value(serde_json::Value::Null);
                }
            }
            Key::Enter => {
                let Some(id) = self.overlay_selected_id() else {
                    return;
                };
                let Some(Overlay::Palette(palette)) = self.overlay.take() else {
                    return;
                };
                match palette.mode {
                    PaletteMode::Commands => self.begin_palette_command(id, palette),
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
                        if let Some(request) = reply {
                            request.respond_value(serde_json::json!({ "selectedId": id }));
                        }
                    }
                }
            }
            Key::Up | Key::Ctrl('p') => self.move_overlay_selection(-1),
            Key::Down | Key::Ctrl('n') => self.move_overlay_selection(1),
            Key::Backspace => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.pop_char();
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            Key::Char(character) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.push_char(character);
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            _ => {}
        }
    }

    fn handle_prompt_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                let Some(Overlay::Prompt(prompt)) = self.overlay.take() else {
                    return;
                };
                self.overlay = Some(Overlay::Palette(prompt.back));
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            Key::Enter => self.confirm_prompt_argument(),
            Key::Up | Key::Ctrl('p') => self.move_overlay_selection(-1),
            Key::Down | Key::Ctrl('n') => self.move_overlay_selection(1),
            Key::Backspace => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.pop_char();
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            Key::Char(character) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.push_char(character);
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            _ => {}
        }
    }

    fn handle_approval_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                let Some(Overlay::Approval(approval)) = self.overlay.take() else {
                    return;
                };
                self.plugin_manager.deny(&approval.plugin_id);
                self.status =
                    format!("Denied capabilities for plugin {}", approval.plugin_name).into();
                self.show_next_approval_prompt();
            }
            Key::Enter => {
                let Some(id) = self.overlay_selected_id() else {
                    return;
                };
                let Some(Overlay::Approval(approval)) = self.overlay.take() else {
                    return;
                };
                if id == "trust" {
                    if let Err(err) = self.plugin_manager.approve(&approval.plugin_id) {
                        self.status = format!("Error approving plugin: {err}").into();
                    } else {
                        self.status =
                            format!("Approved and enabled plugin {}", approval.plugin_name).into();
                        self.on_plugin_approved(&approval.plugin_id);
                    }
                    self.show_next_approval_prompt();
                } else if id == "deny" {
                    self.plugin_manager.deny(&approval.plugin_id);
                    self.status =
                        format!("Denied capabilities for plugin {}", approval.plugin_name).into();
                    self.show_next_approval_prompt();
                } else {
                    // The user selected an info row; keep the dialog open.
                    self.overlay = Some(Overlay::Approval(approval));
                }
            }
            Key::Up | Key::Ctrl('p') => self.move_overlay_selection(-1),
            Key::Down | Key::Ctrl('n') => self.move_overlay_selection(1),
            Key::Backspace => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.pop_char();
                }
            }
            Key::Char(character) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.push_char(character);
                }
            }
            _ => {}
        }
    }

    fn handle_settings_key(&mut self, key: Key) {
        match key {
            Key::Esc => self.overlay = None,
            Key::Enter => {
                let Some(key) = self.overlay_selected_id() else {
                    return;
                };
                self.begin_setting_change(&key);
            }
            Key::Up | Key::Ctrl('p') => self.move_overlay_selection(-1),
            Key::Down | Key::Ctrl('n') => self.move_overlay_selection(1),
            Key::Backspace => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.pop_char();
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            Key::Char(character) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.push_char(character);
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            _ => {}
        }
    }

    /// Opens a picker or prompt for the selected setting, or toggles a bool.
    fn begin_setting_change(&mut self, key: &str) {
        let Ok(decl) = settings::resolve(key) else {
            return;
        };
        match decl.kind {
            SettingKind::Bool => self.toggle_setting(key),
            SettingKind::Choice(_) => {
                let Some(Overlay::Settings(settings)) = self.overlay.take() else {
                    return;
                };
                let mut choice = SettingChoiceState::new(decl.key, settings);
                if let (Some(current), SettingKind::Choice(values)) =
                    (self.active_setting(decl.key).0, decl.kind)
                    && let Some(index) = values
                        .iter()
                        .position(|value| Some(*value) == current.as_str())
                {
                    choice.selected = index;
                }
                self.overlay = Some(Overlay::SettingChoice(choice));
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            SettingKind::Number | SettingKind::Text => {
                let Some(Overlay::Settings(settings)) = self.overlay.take() else {
                    return;
                };
                self.overlay = Some(Overlay::SettingPrompt(SettingPromptState::new(
                    key, settings,
                )));
                let current = self
                    .active_setting(key)
                    .0
                    .map(|value| setting_text(&value))
                    .unwrap_or_default();
                self.status = format!("{key}: {current} >").into();
            }
        }
    }

    fn handle_setting_choice_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                let Some(Overlay::SettingChoice(choice)) = self.overlay.take() else {
                    return;
                };
                self.overlay = Some(Overlay::Settings(choice.back));
            }
            Key::Enter => {
                let Some(value) = self.overlay_selected_id() else {
                    return;
                };
                let Some(Overlay::SettingChoice(choice)) = self.overlay.as_ref() else {
                    return;
                };
                let key = choice.key.clone();
                self.apply_setting(&key, serde_json::Value::String(value), None);
                let Some(Overlay::SettingChoice(choice)) = self.overlay.take() else {
                    return;
                };
                self.overlay = Some(Overlay::Settings(choice.back));
            }
            Key::Up | Key::Ctrl('p') => self.move_overlay_selection(-1),
            Key::Down | Key::Ctrl('n') => self.move_overlay_selection(1),
            Key::Backspace => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.pop_char();
                }
            }
            Key::Char(character) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.push_char(character);
                }
            }
            _ => {}
        }
    }

    fn handle_setting_prompt_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                let Some(Overlay::SettingPrompt(prompt)) = self.overlay.take() else {
                    return;
                };
                self.overlay = Some(Overlay::Settings(prompt.back));
            }
            Key::Enter => {
                let Some(Overlay::SettingPrompt(prompt)) = self.overlay.as_ref() else {
                    return;
                };
                let key = prompt.key.clone();
                let Ok(decl) = settings::resolve(&key) else {
                    return;
                };
                let Some(value) = decl.kind.parse(prompt.input.value()) else {
                    self.status =
                        format!("invalid value for {key}: {}", prompt.input.value()).into();
                    return;
                };
                self.apply_setting(&key, value, None);
                let Some(Overlay::SettingPrompt(prompt)) = self.overlay.take() else {
                    return;
                };
                self.overlay = Some(Overlay::Settings(prompt.back));
            }
            Key::Backspace => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.pop_char();
                }
            }
            Key::Char(character) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.push_char(character);
                }
            }
            _ => {}
        }
    }

    /// Runs `vy.setting.*` and the Ex `:set` / `:setglobal` commands.
    pub(super) fn run_setting_command(&mut self, command: SettingCommand, invocation: &Invocation) {
        match command {
            SettingCommand::Set => {
                let Some(key) = invocation.args.string("key").map(str::to_string) else {
                    self.status = "setting.set requires a key".into();
                    return;
                };
                let Some(value) = invocation.args.value("value").cloned() else {
                    self.status = "setting.set requires a value".into();
                    return;
                };
                let scope = invocation
                    .args
                    .string("scope")
                    .and_then(SettingScope::parse);
                self.apply_setting(&key, value, scope);
            }
            SettingCommand::Toggle => {
                let Some(key) = invocation.args.string("key") else {
                    self.status = "setting.toggle requires a key".into();
                    return;
                };
                self.toggle_setting(key);
            }
            SettingCommand::Reset => {
                let Some(key) = invocation.args.string("key") else {
                    self.status = "setting.reset requires a key".into();
                    return;
                };
                self.reset_setting(key);
            }
            SettingCommand::ExSet | SettingCommand::ExSetGlobal => {
                let option = invocation.args.string("option").unwrap_or("").to_string();
                self.apply_ex_set(&option, command == SettingCommand::ExSetGlobal);
            }
        }
    }

    /// Applies one setting value to the window or the memory layer.
    fn apply_setting(&mut self, key: &str, value: serde_json::Value, scope: Option<SettingScope>) {
        let Ok(decl) = settings::resolve(key) else {
            self.status = format!("unknown setting: {key}").into();
            return;
        };
        if !decl.kind.accepts(&value) {
            self.status =
                format!("invalid value for {}: {}", decl.key, setting_text(&value)).into();
            return;
        }
        let target = scope.unwrap_or(decl.scope);
        if target == SettingScope::Window && decl.scope != SettingScope::Window {
            self.status = format!("{} is a global setting", decl.key).into();
            return;
        }

        match target {
            SettingScope::Window => {
                let Some(pane) = self.active_pane_state_mut() else {
                    self.status = "no active pane".into();
                    return;
                };
                pane.set_window_setting(decl.key, value.clone());
            }
            SettingScope::Global => {
                let changed = self.config.set(decl.key, value.clone());
                if changed {
                    self.mark_config_changed(decl.key);
                }
            }
        }
        self.status = format!("{}={} ({})", decl.key, setting_text(&value), target.label()).into();
    }

    fn toggle_setting(&mut self, key: &str) {
        let Ok(decl) = settings::resolve(key) else {
            self.status = format!("unknown setting: {key}").into();
            return;
        };
        if decl.kind != SettingKind::Bool {
            self.status = format!("{} is not a boolean setting", decl.key).into();
            return;
        }
        let current = self
            .active_setting(decl.key)
            .0
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.apply_setting(decl.key, serde_json::Value::Bool(!current), None);
    }

    fn reset_setting(&mut self, key: &str) {
        let Ok(decl) = settings::resolve(key) else {
            self.status = format!("unknown setting: {key}").into();
            return;
        };
        let reset = match decl.scope {
            SettingScope::Window => self
                .active_pane_state_mut()
                .is_some_and(|pane| pane.reset_window_setting(decl.key)),
            SettingScope::Global => self.config.remove_memory(decl.key),
        };
        if reset {
            self.mark_config_changed(decl.key);
        }
        let (value, source) = self.active_setting(decl.key);
        let text = value
            .as_ref()
            .map(setting_text)
            .unwrap_or_else(|| "(unset)".to_string());
        self.status = format!("{}={} ({})", decl.key, text, source.label()).into();
    }

    /// Applies the argument of `:set` / `:setglobal`.
    fn apply_ex_set(&mut self, option: &str, global: bool) {
        match settings::parse_set_option(option) {
            Ok(settings::SettingEdit::OpenSettings) => self.open_settings(),
            Ok(settings::SettingEdit::Inspect(decl)) => {
                let (value, source) = self.active_setting(decl.key);
                let text = value
                    .as_ref()
                    .map(setting_text)
                    .unwrap_or_else(|| "(unset)".to_string());
                self.status = format!("{}={} ({})", decl.key, text, source.label()).into();
            }
            Ok(settings::SettingEdit::Apply(decl, value)) => {
                let scope = if global {
                    SettingScope::Global
                } else {
                    decl.scope
                };
                self.apply_setting(decl.key, value, Some(scope));
            }
            Err(error) => self.status = error.into(),
        }
    }

    /// Marks settings that were changed at runtime so the next render reloads
    /// the cached config and theme, and notifies scripts.
    fn mark_config_changed(&mut self, key: &str) {
        self.settings_dirty = true;
        self.emit_config_change_to_scripts(vec![key.to_string()]);
    }

    /// Queues a command and starts it when the previous command is done.
    pub(super) fn enqueue_command(&mut self, invocation: Invocation) {
        if invocation.source == InvocationSource::Palette {
            push_recent_command(&mut self.recent_commands, &invocation.id);
        }
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

        if invocation.source != InvocationSource::Script {
            let descriptors = self.commands.descriptors();
            if let Some(descriptor) = descriptors.iter().find(|command| command.id == id)
                && (!descriptor.args.is_empty()
                    || !matches!(
                        descriptor.source,
                        CommandSource::Init | CommandSource::Plugin(_)
                    ))
                && let Err(error) = validate_json_args(&descriptor.args, &invocation.args.values)
            {
                self.status = argument_error(&id, &descriptor.args, &error).into();
                return false;
            }
        }

        if let Some(command) = UiCommand::parse(&id) {
            self.run_ui_command(command, &invocation);
            return false;
        }
        if let Some(command) = UiActionCommand::parse(&id) {
            self.run_ui_action_command(command, &invocation);
            return false;
        }
        if let Some(command) = PaneCommand::parse(&id) {
            self.run_pane_command(command, &invocation);
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
        if let Some(command) = SettingCommand::parse(&id) {
            self.run_setting_command(command, &invocation);
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
            let args = invocation.args.values.clone();
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
            UiCommand::OpenSettings => self.open_settings(),
            UiCommand::ReconnectServer => {
                self.reconnect_requested = true;
                self.status = "reconnecting".into();
            }
            UiCommand::ScriptEval => {
                let Some(code) = invocation.args.string("code") else {
                    self.status = "script.eval requires an expression".into();
                    return;
                };
                let code = code.trim();
                if code.is_empty() {
                    self.status = "script.eval requires an expression".into();
                    return;
                }
                if let Some(plugin) = self.plugin.as_ref() {
                    plugin.eval(code);
                } else {
                    self.status = "the script host is not running".into();
                }
            }
            UiCommand::ViewOpen => {
                if let Some(view_id) = invocation.args.string("view") {
                    let direction = invocation.args.string("direction").unwrap_or("left");
                    self.open_view_pane(view_id, direction);
                }
            }
            UiCommand::ViewClose => {
                if let Some(view_id) = invocation.args.string("view") {
                    self.close_view_pane(view_id);
                }
            }
            UiCommand::ViewToggle => {
                if let Some(view_id) = invocation.args.string("view") {
                    let direction = invocation.args.string("direction").unwrap_or("left");
                    self.toggle_view_pane(view_id, direction);
                }
            }
        }
    }

    /// Runs one `vy.ui.*` action against the focused widget or surface.
    ///
    /// Movement and selection stay in the client: the host defaults in
    /// `ui_action` run here instead of going through the command queue twice
    /// ([ADR 0024](../../docs/adr/0024-keyboard-first-plugin-ui.md) §5).
    pub(super) fn run_ui_action_command(
        &mut self,
        command: UiActionCommand,
        invocation: &Invocation,
    ) {
        let count = invocation.keymap_args().count;

        // Commands that only edit a surface's input line have no widget role.
        if command.action().is_none() {
            self.run_ui_input_command(command);
            return;
        }

        // `args.action` lets a key expand `activate` into another action name.
        let default_name = command.action_name().unwrap_or("activate");
        let action = invocation
            .args
            .string("action")
            .map(str::to_string)
            .unwrap_or_else(|| default_name.to_string());
        let args = UiAction::parse(&action)
            .unwrap_or_else(|| command.action().unwrap_or(UiAction::Activate));

        // A surface owns the keys while it is open, so actions apply to it first.
        if self.overlay.is_some() {
            if self.run_ui_action_on_overlay(args, &action, invocation) {
                return;
            }
            if args == UiAction::Cancel {
                // Nothing below the surface handled the key, so close it.
                self.handle_overlay_key(Key::Esc);
            }
            return;
        }

        if self.run_ui_action_on_widget(args, &action, count) {
            return;
        }

        // No widget handled the action: fall back to the role-agnostic cases.
        match args {
            UiAction::FocusNext => {
                if !self.step_widget_focus(true) {
                    self.status = "there is no widget to focus".into();
                }
            }
            UiAction::FocusPrev => {
                if !self.step_widget_focus(false) {
                    self.status = "there is no widget to focus".into();
                }
            }
            UiAction::Cancel => {
                let mode = self.editor().mode();
                if mode != Mode::Normal {
                    self.activate_active_view();
                    let action = self
                        .editor_mut()
                        .execute(BuiltinCommand::NormalMode, KeymapArgs::default());
                    self.store_active_view();
                    self.clamp_active_views();
                    if let Action::Edit(transaction) = action {
                        self.send_transaction(transaction);
                    }
                }
            }
            _ => {}
        }
    }

    /// Runs a `vy.ui.*` command that edits a surface's input line.
    ///
    /// These have no widget role: `Backspace`, `Ctrl-u`, and the Ex line's
    /// history and completion keys. The surface decides what the command means
    /// (ADR 0032 §2: `bottomBar` for Ex, a floating surface otherwise).
    fn run_ui_input_command(&mut self, command: UiActionCommand) {
        match command {
            UiActionCommand::HistoryPrev | UiActionCommand::HistoryNext => {
                let delta = if command == UiActionCommand::HistoryPrev {
                    -1
                } else {
                    1
                };
                if let Some(Overlay::Ex(ex)) = self.overlay.as_mut() {
                    self.ex_history.navigate(ex, delta);
                }
            }
            UiActionCommand::CompleteNext | UiActionCommand::CompletePrev => {
                let backwards = command == UiActionCommand::CompletePrev;
                self.complete_ex_name(backwards);
            }
            UiActionCommand::DeleteChar => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.pop_char();
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            UiActionCommand::ClearInput => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.clear();
                }
                self.palette_scroll_handle
                    .scroll_to_item(0, ScrollStrategy::Top);
            }
            _ => {}
        }
    }

    /// Applies an action to the list, tree, or table that holds focus.
    ///
    /// Returns whether the widget handled the action. A view that maps the
    /// action to one of its own commands wins first (ADR 0024 §5 step 3).
    fn run_ui_action_on_widget(
        &mut self,
        action: UiAction,
        action_name: &str,
        count: Option<u32>,
    ) -> bool {
        let Some(pane_id) = self.active_pane else {
            return false;
        };
        let Some(pane) = self.panes.get(&pane_id) else {
            return false;
        };
        let Some((widget_id, widget)) = pane.focused_list() else {
            return false;
        };
        let widget_id = widget_id.to_string();
        let widget = widget.clone();

        let args = self.ui_action_args(&widget_id, &widget, action_name, count);

        // A view can override an action by naming one of its own commands.
        if let Some(command) = self
            .view_actions
            .get(&args.view_id)
            .and_then(|actions| actions.get(action_name).cloned())
        {
            self.enqueue_command(Invocation::script(
                command,
                serde_json::to_value(&args).unwrap_or(serde_json::Value::Null),
            ));
            return true;
        }

        if action == UiAction::Activate {
            self.activate_focused_entry(&widget_id, &widget, action_name);
            return true;
        }

        if action == UiAction::Yank {
            let text = self
                .panes
                .get(&pane_id)
                .and_then(|pane| pane.list_state(&widget_id))
                .and_then(|state| ui_action::yank_text(&widget, state));
            if let Some(text) = text {
                self.activate_active_view();
                self.editor_mut().yank_text(text, None);
                self.status = "yanked".into();
                return true;
            }
            return false;
        }

        let page = self
            .active_pane_state()
            .map_or(1, |pane| pane.viewport_rows);
        let outcome = {
            let Some(pane) = self.panes.get_mut(&pane_id) else {
                return false;
            };
            let state = pane.list_state_mut(&widget_id);
            ui_action::apply_to_widget(&widget, state, action_name, count, page.max(1))
        };

        matches!(outcome, UiActionOutcome::Handled)
    }

    /// Builds the arguments a plugin command receives for one action.
    fn ui_action_args(
        &self,
        widget_id: &str,
        widget: &vy_app::widget::WidgetNode,
        action_name: &str,
        count: Option<u32>,
    ) -> UiActionArgs {
        let pane = self.active_pane.and_then(|id| self.panes.get(&id));
        let state = pane
            .and_then(|pane| pane.list_state(widget_id))
            .cloned()
            .unwrap_or_default();

        let rows = vy_app::ui_list::rows(widget, &state);
        let depth = state
            .cursor()
            .and_then(|cursor| rows.iter().find(|row| row.id == cursor))
            .map_or(0, |row| row.depth);

        UiActionArgs {
            action: action_name.to_string(),
            view_id: pane.map_or_else(String::new, |pane| pane.view_type.clone()),
            pane_id: pane.map_or(0, |pane| pane.id),
            role: widget.focus_role(),
            focus_id: pane.and_then(|pane| pane.focus_path.focused().map(str::to_string)),
            selected_ids: state.selected().cloned().collect(),
            anchor_id: state.cursor().map(str::to_string),
            expanded_ids: state.expanded_ids(),
            depth,
            query: self.overlay_input_value().to_string(),
            count,
            register: None,
        }
    }

    /// `vy.ui.activate`: run the row's command, or the view's action mapping.
    ///
    /// Lists and trees carry no per-row command yet, so the host reports that
    /// nothing happened instead of guessing.
    fn activate_focused_entry(
        &mut self,
        widget_id: &str,
        widget: &vy_app::widget::WidgetNode,
        action_name: &str,
    ) {
        let selected = self
            .panes
            .get(&self.active_pane.unwrap_or_default())
            .and_then(|pane| pane.list_state(widget_id))
            .and_then(|state| state.cursor().map(str::to_string));
        let Some(selected) = selected else {
            return;
        };
        let _ = widget;
        self.status = format!("{action_name}: {selected}").into();
    }

    /// Applies an action to the open surface. Returns whether it was handled.
    fn run_ui_action_on_overlay(
        &mut self,
        action: UiAction,
        action_name: &str,
        _invocation: &Invocation,
    ) -> bool {
        match action {
            UiAction::Next => {
                self.move_overlay_selection(1);
                true
            }
            UiAction::Prev => {
                self.move_overlay_selection(-1);
                true
            }
            UiAction::First => {
                self.move_overlay_selection(isize::MIN / 2);
                true
            }
            UiAction::Last => {
                self.move_overlay_selection(isize::MAX / 2);
                true
            }
            UiAction::PageDown => {
                let page = self
                    .active_pane_state()
                    .map_or(10, |pane| pane.viewport_rows);
                self.move_overlay_selection(page.max(1) as isize);
                true
            }
            UiAction::PageUp => {
                let page = self
                    .active_pane_state()
                    .map_or(10, |pane| pane.viewport_rows);
                self.move_overlay_selection(-(page.max(1) as isize));
                true
            }
            UiAction::Toggle => self.toggle_overlay_mark(),
            UiAction::Activate => self.confirm_overlay(action_name),
            UiAction::Cancel => {
                // Layer 1 of the Esc rules: a surface restores its temporary
                // state, and closes when there is nothing to restore
                // (ADR 0024 §9, ADR 0032 §7). `handle_overlay_key` already
                // clears the query before closing, so let it decide.
                self.handle_overlay_key(Key::Esc);
                true
            }
            UiAction::Yank => {
                if let Some(id) = self.overlay_selected_id() {
                    self.editor_mut().yank_text(id, None);
                    self.status = "yanked".into();
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Toggles a mark in the open list surface. Returns whether it changed.
    fn toggle_overlay_mark(&mut self) -> bool {
        let Some(id) = self.overlay_selected_id() else {
            return false;
        };
        match self.overlay.as_mut() {
            Some(Overlay::Palette(_))
            | Some(Overlay::Settings(_))
            | Some(Overlay::SettingChoice(_)) => {
                self.marks.insert(id);
                true
            }
            _ => false,
        }
    }

    /// Confirms the selected row of the open surface.
    fn confirm_overlay(&mut self, _action_name: &str) -> bool {
        match self.overlay.as_ref() {
            Some(Overlay::Ex(_)) => {
                self.handle_ex_key(Key::Enter);
                true
            }
            _ => {
                self.handle_overlay_key(Key::Enter);
                true
            }
        }
    }

    /// Executes a command specified as a string (e.g. `"vy.editor.open foo.txt"` or `"my.tree.toggle"`).
    pub(crate) fn execute_command_string(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        let (id, raw) = match cmd.split_once(char::is_whitespace) {
            Some((id, args)) => (id, Some(args.trim())),
            None => (cmd, None),
        };
        let descriptors = self.commands.descriptors();
        let Some(descriptor) = descriptors.iter().find(|command| command.id == id) else {
            self.status = format!("unknown command: {id}").into();
            return;
        };
        let invocation = match raw {
            None => Invocation::palette(id),
            Some(raw)
                if matches!(
                    descriptor.source,
                    CommandSource::Init | CommandSource::Plugin(_)
                ) =>
            {
                Invocation::script(id, serde_json::json!({ "raw": raw }))
            }
            Some(raw) => match parse_ex_args(&descriptor.args, raw) {
                Ok(values) => Invocation::ex(id, values),
                Err(error) => {
                    self.status = argument_error(id, &descriptor.args, &error).into();
                    return;
                }
            },
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
            self.request::<PaneClose>(
                PaneCloseParams {
                    pane_id: pid,
                    force: true,
                },
                Pending::Close,
            );
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

/// Choice rows for an enumerated setting.
fn setting_choice_items(choice: &SettingChoiceState) -> Vec<PickerItem> {
    let Some(decl) = settings::find(&choice.key) else {
        return Vec::new();
    };
    let SettingKind::Choice(values) = decl.kind else {
        return Vec::new();
    };
    values
        .iter()
        .map(|value| PickerItem::new(*value, *value))
        .collect()
}

/// Renders a setting value for a status line or a settings row.
fn setting_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(number) => match number.as_f64() {
            Some(float) => format!("{float}"),
            None => number.to_string(),
        },
        other => other.to_string(),
    }
}

/// Completion candidates for the current Ex input.
fn ex_completion_candidates(
    typed: &str,
    descriptors: &[CommandDescriptor],
) -> Vec<CompletionCandidate> {
    if !typed.contains(char::is_whitespace) {
        return ex_name_candidates(typed, descriptors)
            .into_iter()
            .map(CompletionCandidate::plain)
            .collect();
    }
    ex_setting_candidates(typed)
}

/// Completes setting names and choice values after `:set` / `:setglobal`.
fn ex_setting_candidates(typed: &str) -> Vec<CompletionCandidate> {
    let Some(space) = typed.find(char::is_whitespace) else {
        return Vec::new();
    };
    let (name, after) = typed.split_at(space);
    if name != "set" && name != "setglobal" {
        return Vec::new();
    }

    let token_start = after.len() - after.trim_start().len();
    let lead = &after[..token_start];
    let token = &after[token_start..];
    if token.contains(char::is_whitespace) {
        return Vec::new();
    }
    let base = format!("{name}{lead}");

    if let Some((option, value_prefix)) = token.split_once('=') {
        let Ok(decl) = settings::resolve(option) else {
            return Vec::new();
        };
        let SettingKind::Choice(values) = decl.kind else {
            return Vec::new();
        };
        return values
            .iter()
            .filter(|value| value.starts_with(value_prefix) && **value != value_prefix)
            .map(|value| CompletionCandidate::new(format!("{base}{option}={value}"), *value))
            .collect();
    }

    let mut candidates = Vec::new();
    for decl in settings::SETTINGS {
        if token.contains('.') {
            if decl.key.starts_with(token) && decl.key != token {
                candidates.push(CompletionCandidate::new(
                    format!("{base}{}", decl.key),
                    decl.key,
                ));
            }
            continue;
        }
        if decl.name().starts_with(token) && decl.name() != token {
            candidates.push(CompletionCandidate::new(
                format!("{base}{}", decl.name()),
                decl.name(),
            ));
        }
        if decl.kind == SettingKind::Bool {
            let negated = format!("no{}", decl.name());
            if negated.starts_with(token) && negated != token {
                candidates.push(CompletionCandidate::new(
                    format!("{base}{negated}"),
                    negated,
                ));
            }
        }
    }
    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.value == right.value);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_setting_names_and_values() {
        assert_eq!(
            ex_setting_candidates("set wr"),
            vec![CompletionCandidate::new("set wrap", "wrap")]
        );
        assert_eq!(
            ex_setting_candidates("set now"),
            vec![CompletionCandidate::new("set nowrap", "nowrap")]
        );
        assert_eq!(
            ex_setting_candidates("set theme=l"),
            vec![CompletionCandidate::new("set theme=light", "light")]
        );
        assert!(ex_setting_candidates("set wrap").is_empty());
        assert!(ex_setting_candidates("split wr").is_empty());
        assert!(
            ex_setting_candidates("setglobal wr")
                .contains(&CompletionCandidate::new("setglobal wrap", "wrap"))
        );
    }

    #[test]
    fn completes_command_names_before_arguments() {
        let descriptors = vy_app::commands::CommandRegistry::new().descriptors();

        assert_eq!(
            ex_completion_candidates("wr", &descriptors),
            vec![CompletionCandidate::plain("write")]
        );
        assert_eq!(
            ex_completion_candidates("set wr", &descriptors),
            vec![CompletionCandidate::new("set wrap", "wrap")]
        );
    }
}
