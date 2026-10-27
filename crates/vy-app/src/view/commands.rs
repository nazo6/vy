//! Command execution and palette/ex handling for `EditorView`.

use gpui::ScrollStrategy;
use vy_app::commands::{
    ArgKind, BuiltinCommand, CommandDescriptor, CommandSource, DocumentCommand, ExAddress, ExRange,
    Invocation, InvocationSource, LineRange, Owner, PaletteVisibility, PaneCommand, RangeSpec,
    SettingCommand, UiCommand, ViewCommand, argument_error, ex_name_candidates, parse_ex_args,
    parse_ex_line, resolve_ex_name, validate_json_args,
};
use vy_app::editor::Action;
use vy_app::ex::ExState;
use vy_app::focus_path::FocusPath;
use vy_app::geometry::{collect_pane_ids, focus_neighbor};
use vy_app::overlay::{
    ApprovalState, CompletionCandidate, PromptState, SettingChoiceState, SettingPromptState,
    SettingsState, SurfaceKind,
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
use vy_app::workspace::{uri_from_argument_remote, uri_from_argument_with_base};
use vy_client_api::namespaces::ui::UiActionArgs;
use vy_client_api::settings::{self, SettingKind, SettingScope};
use vy_client_api::widget::WidgetNode;
use vy_proto::{
    DocumentIdParams, DocumentSave, EditorOpen, EditorOpenParams, PaneClose, PaneCloseParams,
    PaneFocus, PaneFocusParams, PaneOpen, PaneOpenParams, PanePlacement, PaneResize,
    PaneResizeParams,
};

use super::{EditorView, Pending};
use crate::ui::palette::{PALETTE_LIST_HEIGHT, PALETTE_ROW_HEIGHT};

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
        match self.surface_top_kind() {
            Some(SurfaceKind::Palette(palette)) => self.palette_mode_items(&palette.mode),
            Some(SurfaceKind::Prompt(prompt)) => self.prompt_items(prompt),
            Some(SurfaceKind::Approval(approval)) => approval_picker_items(
                &approval.plugin_id,
                &approval.plugin_name,
                &approval.missing,
            ),
            Some(SurfaceKind::Settings(_)) => self.setting_items(),
            Some(SurfaceKind::SettingChoice(choice)) => setting_choice_items(choice),
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
        self.surface_top_kind()
            .map(|kind| kind.input().value())
            .unwrap_or("")
    }

    pub(super) fn overlay_filtered_len(&self) -> usize {
        if self.surface_top_kind().is_none() {
            return 0;
        }
        filtered_picker_indices(&self.overlay_items(), self.overlay_input_value()).len()
    }

    pub(super) fn overlay_selected_id(&self) -> Option<String> {
        let selected = self.surface_top_kind()?.selected()?;
        let items = self.overlay_items();
        let filtered = filtered_picker_indices(&items, self.overlay_input_value());
        let index = selected.min(filtered.len().saturating_sub(1));
        filtered.get(index).map(|index| items[*index].id.clone())
    }

    /// Moves the selected row of the active overlay's list.
    /// Number of rows one overlay page moves, derived from the visible
    /// rows (ADR 0032 review m5): the list viewport is 340px and item
    /// rows are 28px, so a page is 12 rows. Headings are shorter, so
    /// with headings present a page slightly under-scrolls rather
    /// than over-scrolls.
    const OVERLAY_PAGE_SIZE: isize = (PALETTE_LIST_HEIGHT / PALETTE_ROW_HEIGHT) as isize;

    /// Runs a `vy.ui.*` action on the open overlay (ADR 0032 Phase 3).
    /// Count prefixes are ignored for overlay movement, as they are for
    /// pane focus movement in Phase 2a. Actions with no meaning for the
    /// open overlay (focus movement, toggle, yank) return false.
    fn apply_overlay_action(&mut self, args: &UiActionArgs) -> bool {
        match args.action.as_str() {
            "next" => self.overlay_move(1),
            "prev" => self.overlay_move(-1),
            "first" => self.overlay_set_selection(0),
            "last" => {
                let len = self.overlay_filtered_len();
                if len == 0 {
                    false
                } else {
                    self.overlay_set_selection(len - 1)
                }
            }
            "pageDown" => self.overlay_move_page(Self::OVERLAY_PAGE_SIZE),
            "pageUp" => self.overlay_move_page(-Self::OVERLAY_PAGE_SIZE),
            "activate" => self.apply_overlay_activate(),
            "cancel" => self.apply_overlay_cancel(),
            "clearLine" => {
                if let Some(surface) = self.surface_top_mut() {
                    surface.kind.clear();
                    true
                } else {
                    false
                }
            }
            "completeNext" => {
                if matches!(self.surface_top_kind(), Some(SurfaceKind::Ex(_))) {
                    self.complete_ex_name(false);
                    true
                } else {
                    false
                }
            }
            "completePrev" => {
                if matches!(self.surface_top_kind(), Some(SurfaceKind::Ex(_))) {
                    self.complete_ex_name(true);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Moves the open overlay's selection (or Ex history) by `delta`.
    /// Ex history and list selection share the sign: next is +1, prev -1.
    fn overlay_move(&mut self, delta: isize) -> bool {
        // Direct field access keeps the `surfaces` borrow disjoint
        // from the `ex_history` borrow below.
        if let Some(surface) = self.surfaces.last_mut()
            && let SurfaceKind::Ex(ex) = &mut surface.kind
        {
            self.ex_history.navigate(ex, delta);
            return true;
        }
        if self
            .surface_top_kind()
            .is_some_and(|kind| kind.selected().is_some())
        {
            self.move_overlay_selection(delta);
            true
        } else {
            false
        }
    }

    /// Moves the open overlay's selection by a page, if it has a list.
    fn overlay_move_page(&mut self, delta: isize) -> bool {
        if self
            .surface_top_kind()
            .is_some_and(|kind| kind.selected().is_some())
        {
            self.move_overlay_selection(delta);
            true
        } else {
            false
        }
    }

    /// Sets the open overlay's selection to an absolute filtered index.
    fn overlay_set_selection(&mut self, index: usize) -> bool {
        let len = self.overlay_filtered_len();
        if len == 0 {
            return false;
        }
        let items = self.overlay_items();
        let input = self.overlay_input_value().to_string();
        let filtered = filtered_picker_indices(&items, &input);
        let headings = shows_palette_headings(&input);
        let selected_value = {
            let Some(surface) = self.surface_top_mut() else {
                return false;
            };
            let Some(selected) = surface.kind.selected_mut() else {
                return false;
            };
            *selected = index.min(len - 1);
            *selected
        };
        let row = palette_row_index(&items, &filtered, headings, selected_value);
        if let Some(surface) = self.surface_top_mut() {
            surface.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        }
        true
    }

    /// Runs the frontmost surface's `Enter` action, driven by the
    /// `vy.ui.activate` keymap binding. Popping reveals the surface
    /// below, which replaces the old `back` fields.
    fn apply_overlay_activate(&mut self) -> bool {
        match self.surface_top_kind() {
            Some(SurfaceKind::Palette(_)) => {
                let Some(id) = self.overlay_selected_id() else {
                    return false;
                };
                let Some(popped) = self.pop_surface() else {
                    return false;
                };
                let SurfaceKind::Palette(palette) = popped.kind else {
                    return false;
                };
                match palette.mode {
                    PaletteMode::Commands => self.begin_palette_command(id),
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
                true
            }
            Some(SurfaceKind::Ex(_)) => {
                let Some(popped) = self.pop_surface() else {
                    return false;
                };
                let SurfaceKind::Ex(ex) = popped.kind else {
                    return false;
                };
                let line = ex.input.value().trim().to_string();
                if line.is_empty() {
                    // Enter on an empty Ex line closes it, as before.
                    return true;
                }
                self.ex_history.record(line.clone());
                self.execute_ex_line(&line);
                true
            }
            Some(SurfaceKind::Prompt(_)) => {
                self.confirm_prompt_argument();
                true
            }
            Some(SurfaceKind::Approval(_)) => {
                let Some(id) = self.overlay_selected_id() else {
                    return false;
                };
                let Some(SurfaceKind::Approval(approval)) = self.surface_top_kind() else {
                    return false;
                };
                let plugin_id = approval.plugin_id.clone();
                let plugin_name = approval.plugin_name.clone();
                if id == "trust" {
                    self.pop_surface();
                    if let Err(err) = self.plugin_manager.approve(&plugin_id) {
                        self.status = format!("Error approving plugin: {err}").into();
                    } else {
                        self.status = format!("Approved and enabled plugin {plugin_name}").into();
                        self.on_plugin_approved(&plugin_id);
                    }
                    self.show_next_approval_prompt();
                } else if id == "deny" {
                    self.pop_surface();
                    self.plugin_manager.deny(&plugin_id);
                    self.status = format!("Denied capabilities for plugin {plugin_name}").into();
                    self.show_next_approval_prompt();
                } else {
                    // The user selected an info row; keep the dialog open.
                }
                true
            }
            Some(SurfaceKind::Settings(_)) => {
                let Some(key) = self.overlay_selected_id() else {
                    return false;
                };
                self.begin_setting_change(&key);
                true
            }
            Some(SurfaceKind::SettingChoice(_)) => {
                let Some(value) = self.overlay_selected_id() else {
                    return false;
                };
                let Some(SurfaceKind::SettingChoice(choice)) = self.surface_top_kind() else {
                    return false;
                };
                let key = choice.key.clone();
                self.apply_setting(&key, serde_json::Value::String(value), None);
                // Popping reveals the settings list below.
                self.pop_surface();
                true
            }
            Some(SurfaceKind::SettingPrompt(_)) => {
                let Some(SurfaceKind::SettingPrompt(prompt)) = self.surface_top_kind() else {
                    return false;
                };
                let key = prompt.key.clone();
                let Ok(decl) = settings::resolve(&key) else {
                    return false;
                };
                let Some(value) = decl.kind.parse(prompt.input.value()) else {
                    self.status =
                        format!("invalid value for {key}: {}", prompt.input.value()).into();
                    return false;
                };
                self.apply_setting(&key, value, None);
                self.pop_surface();
                true
            }
            None => false,
        }
    }

    /// Runs the frontmost surface's `Esc` action (Esc layer 1): a
    /// palette in insert mode switches to normal mode instead of
    /// closing; every other case pops, revealing the surface below.
    fn apply_overlay_cancel(&mut self) -> bool {
        if matches!(self.surface_top_kind(), Some(SurfaceKind::Palette(_))) && self.palette_insert {
            self.palette_insert = false;
            return true;
        }
        let Some(popped) = self.pop_surface() else {
            return false;
        };
        match popped.kind {
            SurfaceKind::Palette(palette) => {
                if let PaletteMode::Custom {
                    reply: Some(request),
                    ..
                } = palette.mode
                {
                    request.respond_value(serde_json::Value::Null);
                }
                true
            }
            SurfaceKind::Ex(_) => true,
            SurfaceKind::Prompt(_) => {
                // Revealing the palette below scrolls it to the top,
                // as the old `back` restore did.
                if let Some(surface) = self.surface_top_mut() {
                    surface.scroll.scroll_to_item(0, ScrollStrategy::Top);
                }
                true
            }
            SurfaceKind::Approval(approval) => {
                self.plugin_manager.deny(&approval.plugin_id);
                self.status =
                    format!("Denied capabilities for plugin {}", approval.plugin_name).into();
                self.show_next_approval_prompt();
                true
            }
            SurfaceKind::Settings(_) => true,
            SurfaceKind::SettingChoice(_) => true,
            SurfaceKind::SettingPrompt(_) => true,
        }
    }

    fn move_overlay_selection(&mut self, delta: isize) {
        let len = self.overlay_filtered_len();
        let items = self.overlay_items();
        let input = self.overlay_input_value().to_string();
        let filtered = filtered_picker_indices(&items, &input);
        let headings = shows_palette_headings(&input);
        let selected_value = {
            let Some(surface) = self.surface_top_mut() else {
                return;
            };
            let Some(selected) = surface.kind.selected_mut() else {
                return;
            };
            *selected = move_palette_selection(*selected, delta, len);
            *selected
        };
        let row = palette_row_index(&items, &filtered, headings, selected_value);
        if let Some(surface) = self.surface_top_mut() {
            surface.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }

    pub(super) fn open_palette(&mut self) {
        self.open_palette_mode(PaletteMode::Commands, "command palette");
    }

    pub(super) fn open_document_picker(&mut self) {
        self.open_palette_mode(PaletteMode::Documents, "switch document");
    }

    fn open_palette_mode(&mut self, mode: PaletteMode, status: &str) {
        self.clear_preedit();
        // Palettes open in insert mode (ADR 0024): typed characters filter.
        self.palette_insert = true;
        self.replace_top_surface(SurfaceKind::Palette(PaletteState::with_mode(mode)));
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
        self.replace_top_surface(SurfaceKind::Ex(ExState::default()));
        self.status = "ex command".into();
    }

    /// Opens the settings list (`:set` without arguments, or `vy.ui.open_settings`).
    pub(super) fn open_settings(&mut self) {
        self.clear_preedit();
        self.replace_top_surface(SurfaceKind::Settings(SettingsState::new()));
        self.status = "settings".into();
    }

    /// Starts collecting the required arguments of a palette command.
    fn begin_palette_command(&mut self, id: String) {
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
        self.push_surface(SurfaceKind::Prompt(PromptState::new(
            id,
            title.clone(),
            descriptor.args,
        )));
        self.status = format!("{title}: {arg_name}").into();
    }

    /// Stores the current argument and either asks for the next one or runs the command.
    fn confirm_prompt_argument(&mut self) {
        let Some(SurfaceKind::Prompt(prompt)) = self.surface_top_kind() else {
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

        // The mutation borrows the top surface, so it runs in a block
        // and the status, scroll, and command steps follow after.
        enum ConfirmOutcome {
            Next(String),
            Done(serde_json::Map<String, serde_json::Value>),
        }
        let outcome = {
            let Some(surface) = self.surface_top_mut() else {
                return;
            };
            let SurfaceKind::Prompt(prompt) = &mut surface.kind else {
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
                ConfirmOutcome::Next(next_name)
            } else {
                ConfirmOutcome::Done(prompt.values.clone())
            }
        };
        match outcome {
            ConfirmOutcome::Next(next_name) => {
                self.status = format!("{title}: {next_name}").into();
                if let Some(surface) = self.surface_top_mut() {
                    surface.scroll.scroll_to_item(0, ScrollStrategy::Top);
                }
            }
            ConfirmOutcome::Done(values) => {
                let values = serde_json::Value::Object(values);
                match validate_json_args(&spec, &values) {
                    Ok(values) => {
                        // Success closes the whole stack: the palette
                        // below the prompt must not reappear.
                        self.clear_surfaces();
                        self.enqueue_command(Invocation::palette_with_args(command_id, values));
                    }
                    Err(error) => {
                        self.status = argument_error(&command_id, &spec, &error).into();
                    }
                }
            }
        }
    }

    /// Cycles Ex completion: command names, then names and values after `:set`.
    fn complete_ex_name(&mut self, backwards: bool) {
        if self.overlay_input_value().trim().is_empty() {
            return;
        }
        let descriptors = self.commands.descriptors();
        let Some(surface) = self.surface_top_mut() else {
            return;
        };
        let SurfaceKind::Ex(ex) = &mut surface.kind else {
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
            self.push_surface(SurfaceKind::Approval(ApprovalState::new(
                req.plugin_id.clone(),
                req.plugin_name.clone(),
                req.plugin_version.clone(),
                req.missing_capabilities.clone(),
            )));
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
                let mut choice = SettingChoiceState::new(decl.key);
                if let (Some(current), SettingKind::Choice(values)) =
                    (self.active_setting(decl.key).0, decl.kind)
                    && let Some(index) = values
                        .iter()
                        .position(|value| Some(*value) == current.as_str())
                {
                    choice.selected = index;
                }
                let selected = choice.selected;
                self.push_surface(SurfaceKind::SettingChoice(choice));
                // ADR 0032 review m2: the new surface starts with a fresh
                // scroll handle at the top, so a preselected row past the
                // first screen would open invisible. Scroll to it.
                self.overlay_set_selection(selected);
            }
            SettingKind::Number | SettingKind::Text => {
                self.push_surface(SurfaceKind::SettingPrompt(SettingPromptState::new(key)));
                let current = self
                    .active_setting(key)
                    .0
                    .map(|value| setting_text(&value))
                    .unwrap_or_default();
                self.status = format!("{key}: {current} >").into();
            }
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
            // ADR 0032 Phase 2: plugin UI actions. These run in client Rust,
            // not on the script thread. `apply_ui_action` resolves the action
            // name (canonical, or `args.action` when a binding attaches one)
            // and runs the host default for button-list views; actions with
            // no host default yet (page, toggle, yank, cancel) return false
            // and are left for a future views-actions mapping.
            UiCommand::UiNext => {
                self.apply_ui_action(&self.ui_action_args("next", invocation));
            }
            UiCommand::UiPrev => {
                self.apply_ui_action(&self.ui_action_args("prev", invocation));
            }
            UiCommand::UiPageDown => {
                self.apply_ui_action(&self.ui_action_args("pageDown", invocation));
            }
            UiCommand::UiPageUp => {
                self.apply_ui_action(&self.ui_action_args("pageUp", invocation));
            }
            UiCommand::UiFirst => {
                self.apply_ui_action(&self.ui_action_args("first", invocation));
            }
            UiCommand::UiLast => {
                self.apply_ui_action(&self.ui_action_args("last", invocation));
            }
            UiCommand::UiActivate => {
                self.apply_ui_action(&self.ui_action_args("activate", invocation));
            }
            UiCommand::UiCancel => {
                self.apply_ui_action(&self.ui_action_args("cancel", invocation));
            }
            UiCommand::UiToggle => {
                self.apply_ui_action(&self.ui_action_args("toggle", invocation));
            }
            UiCommand::UiYank => {
                self.apply_ui_action(&self.ui_action_args("yank", invocation));
            }
            UiCommand::UiFocusNext => {
                self.apply_ui_action(&self.ui_action_args("focusNext", invocation));
            }
            UiCommand::UiFocusPrev => {
                self.apply_ui_action(&self.ui_action_args("focusPrev", invocation));
            }
            UiCommand::UiClearLine => {
                self.apply_ui_action(&self.ui_action_args("clearLine", invocation));
            }
            UiCommand::UiCompleteNext => {
                self.apply_ui_action(&self.ui_action_args("completeNext", invocation));
            }
            UiCommand::UiCompletePrev => {
                self.apply_ui_action(&self.ui_action_args("completePrev", invocation));
            }
        }
    }

    /// Builds the `UiActionArgs` for a `vy.ui.*` invocation from the active
    /// pane (ADR 0024 §5). Button-list views have no host selection, tree
    /// expansion, or picker query state, so those fields stay empty; the
    /// focused widget is both the selection and the anchor.
    fn ui_action_args(&self, canonical: &str, invocation: &Invocation) -> UiActionArgs {
        let action = invocation
            .args
            .string("action")
            .unwrap_or(canonical)
            .to_string();
        let (view_id, pane_id, role, focus_id) = match self.active_pane_state() {
            Some(pane) => {
                let tree = pane.widget_tree.as_ref();
                let resolved = tree.map(|tree| pane.focus_path.resolve(tree));
                let role = resolved
                    .as_ref()
                    .zip(tree)
                    .and_then(|(path, tree)| path.focused_role(tree))
                    .map(|role| role.to_string());
                let focus_id = resolved.and_then(|path| path.last().cloned());
                (pane.view_type.clone(), pane.id, role, focus_id)
            }
            None => (String::new(), 0, None, None),
        };
        let selected_ids = focus_id.clone().into_iter().collect();
        UiActionArgs {
            action,
            view_id,
            pane_id,
            role,
            focus_id: focus_id.clone(),
            selected_ids,
            anchor_id: focus_id,
            expanded_ids: Vec::new(),
            depth: None,
            query: None,
            count: invocation.args.count,
            register: invocation
                .args
                .register
                .map(|register| register.to_string()),
        }
    }

    /// Runs the host default for a `vy.ui.*` action on the active UI pane.
    ///
    /// Returns true when the action moved focus or activated a widget.
    /// Editor panes, panes with no widget tree, and actions with no host
    /// default yet return false and change nothing.
    fn apply_ui_action(&mut self, args: &UiActionArgs) -> bool {
        // A modal overlay owns the key: overlay actions run first, even
        // over an editor pane behind the overlay.
        if !self.surfaces.is_empty() {
            return self.apply_overlay_action(args);
        }
        // Host defaults only apply to UI panes; editor panes keep their
        // own keymap.
        let Some(pane) = self.active_pane_state() else {
            return false;
        };
        if super::pane_kind_for(pane.view_type.as_str()).as_str() == "editor" {
            return false;
        }
        if pane.widget_tree.is_none() {
            return false;
        }
        match args.action.as_str() {
            "next" | "focusNext" => self.move_widget_focus(true),
            "prev" | "focusPrev" => self.move_widget_focus(false),
            "first" => self.set_ui_focus_to_first(),
            "last" => self.set_ui_focus_to_last(),
            "activate" => self.activate_ui_focused_button(),
            _ => false,
        }
    }

    fn set_ui_focus_to_first(&mut self) -> bool {
        let Some(pane) = self.active_pane_state_mut() else {
            return false;
        };
        let Some(tree) = pane.widget_tree.clone() else {
            return false;
        };
        let Some(first) = FocusPath::focusable(&tree).into_iter().next() else {
            return false;
        };
        pane.focus_path = first.path;
        true
    }

    fn set_ui_focus_to_last(&mut self) -> bool {
        let Some(pane) = self.active_pane_state_mut() else {
            return false;
        };
        let Some(tree) = pane.widget_tree.clone() else {
            return false;
        };
        let Some(last) = FocusPath::focusable(&tree).into_iter().last() else {
            return false;
        };
        pane.focus_path = last.path;
        true
    }

    /// Executes the focused button's command, as a click would.
    fn activate_ui_focused_button(&mut self) -> bool {
        let Some(command) = self
            .active_pane_state()
            .and_then(|pane| {
                let tree = pane.widget_tree.as_ref()?;
                let focus = pane.focus_path.last()?;
                focused_button_command(tree, focus)
            })
            .filter(|command| !command.is_empty())
        else {
            return false;
        };
        self.execute_command_string(&command);
        true
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
                let tree = self.view_widget_trees.get(view_id).cloned();
                pane.set_widget_tree(tree);
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

/// Command string of the button with `focus_id` in `tree`, if any.
///
/// Button ids are unique within a view tree (for example `dir_<uri>` and
/// `file_<uri>` in the example explorer), so matching on the focused id
/// alone is enough; container ids in the focus path are not needed here.
fn focused_button_command(tree: &WidgetNode, focus_id: &str) -> Option<String> {
    match tree {
        WidgetNode::Container { children, .. } => children
            .iter()
            .find_map(|child| focused_button_command(child, focus_id)),
        WidgetNode::Button { id, command, .. } if id == focus_id => Some(command.clone()),
        _ => None,
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
    fn overlay_page_size_matches_visible_rows() {
        // ADR 0032 review m5: a page is the rows that fit in the list
        // viewport (340px at 28px per row), not a fixed guess.
        assert_eq!(EditorView::OVERLAY_PAGE_SIZE, 12);
    }

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
