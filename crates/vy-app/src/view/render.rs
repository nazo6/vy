//! GPUI Render implementation and layout rendering for `EditorView`.

use std::collections::HashSet;
use std::time::Instant;

use gpui::{
    AnyElement, Context, CursorStyle, DefiniteLength, Div, ElementInputHandler, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled, Window,
    canvas, div, px,
};

use vy_app::editor::VisualRow;
use vy_app::overlay::Overlay;
use vy_app::text_layout::{clamp_scroll_top, scroll_to_show, scroll_to_show_horizontal};
use vy_core::{LayoutNode, PaneId, SplitDirection};

use super::{EditorView, Pending};
use crate::theme::{ActiveTheme, Theme};
use crate::ui;

/// Height of the global statusline.
const STATUSLINE_HEIGHT: f32 = 24.0;
/// Width of the bar that marks the insertion point.
const CARET_WIDTH: f32 = 2.0;

impl EditorView {
    /// Height of one text row from the settings.
    pub(super) fn line_height(&self) -> f32 {
        ui::pane::line_height(&self.config)
    }

    /// Width of the line number gutter. It grows with the number of lines and is
    /// zero when line numbers are turned off.
    pub(super) fn gutter_width(&self, line_count: usize) -> f32 {
        ui::pane::gutter_width(&self.config, line_count, self.char_width)
    }

    /// Updates the row and column capacity of all panes based on their allocated dimensions.
    pub(super) fn update_pane_viewports(&mut self, width: f32, height: f32) {
        let lh = self.line_height();
        let ch_w = self.char_width;
        let active_doc_lines = self
            .active_document()
            .map(|d| d.editor.line_count())
            .unwrap_or(1);
        let gutter = self.gutter_width(active_doc_lines);
        let rows = ((height / lh).floor().max(1.0)) as usize;
        let cols = (((width - gutter).max(0.0) / ch_w).floor().max(1.0)) as usize;

        let Some(layout) = self.layout.clone() else {
            if let Some(pane) = self.active_pane_state_mut() {
                pane.viewport_rows = rows;
                pane.viewport_cols = cols;
            }
            return;
        };

        self.update_node_viewports(&layout, width, height);
    }

    fn update_node_viewports(&mut self, node: &LayoutNode, width: f32, height: f32) {
        match node {
            LayoutNode::Leaf { pane_id } => {
                let lh = self.line_height();
                let ch_w = self.char_width;
                let doc_lines = self
                    .panes
                    .get(pane_id)
                    .and_then(|p| p.document_id)
                    .and_then(|id| self.documents.get(&id))
                    .map(|d| d.editor.line_count())
                    .unwrap_or(1);
                let gutter = self.gutter_width(doc_lines);
                if let Some(pane) = self.panes.get_mut(pane_id) {
                    pane.viewport_rows = ((height / lh).floor().max(1.0)) as usize;
                    pane.viewport_cols =
                        (((width - gutter).max(0.0) / ch_w).floor().max(1.0)) as usize;
                }
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => match direction {
                SplitDirection::Horizontal => {
                    self.update_node_viewports(first, width * ratio, height);
                    self.update_node_viewports(second, width * (1.0 - ratio), height);
                }
                SplitDirection::Vertical => {
                    self.update_node_viewports(first, width, height * ratio);
                    self.update_node_viewports(second, width, height * (1.0 - ratio));
                }
            },
        }
    }

    /// Drops the scroll state of panes that are gone and makes one for every
    /// UI pane.
    ///
    /// The handle owns the pane's scroll offset, so it has to outlive a frame.
    pub(super) fn sync_pane_scroll_handles(&mut self) {
        let panes: HashSet<PaneId> = self
            .panes
            .iter()
            .filter(|(_, pane)| pane.view_type == "ui" || pane.widget_tree.is_some())
            .map(|(pane_id, _)| *pane_id)
            .collect();
        self.ui_pane_scroll
            .retain(|pane_id, _| panes.contains(pane_id));
        for pane_id in panes {
            self.ui_pane_scroll.entry(pane_id).or_default();
        }
    }

    /// Keeps the per-pane bounds cells in step with the panes that exist.
    ///
    /// A handler reads the bounds the canvas wrote during the last paint, so a
    /// cell has to outlive the frame that created it.
    pub(super) fn sync_pane_bounds(&mut self) {
        self.pane_bounds
            .retain(|pane_id, _| self.panes.contains_key(pane_id));
        let pane_ids: Vec<PaneId> = self.panes.keys().copied().collect();
        for pane_id in pane_ids {
            self.pane_bounds.entry(pane_id).or_default();
        }
    }

    /// Moves the active viewport just enough to show the cursor.
    pub(super) fn ensure_cursor_visible(&mut self) {
        let Some(pane) = self.active_pane_state() else {
            return;
        };
        let Some(document) = self.active_document() else {
            return;
        };

        let is_wrap = pane.is_wrap(&self.config);
        let cursor = pane.view.cursor();
        let cursor_line = cursor.line as usize;
        let line_count = document.editor.line_count();
        let rows = pane.viewport_rows;
        let cols = pane.viewport_cols;

        if !is_wrap {
            let next_top = scroll_to_show(cursor_line, pane.scroll_top, rows, line_count);
            let display_col = document.editor.visual_index_with(None).display_col(cursor);
            let next_left = scroll_to_show_horizontal(display_col, pane.scroll_left, cols);
            if let Some(pane) = self.active_pane_state_mut() {
                pane.scroll_top = next_top;
                pane.scroll_left = next_left;
                pane.scroll_sub_line = 0;
            }
        } else {
            let index = document.editor.visual_index_with(Some(cols));
            let cursor_row = index.row_of(cursor);
            let scroll_row = VisualRow {
                line: pane.scroll_top as u32,
                chunk: pane.scroll_sub_line,
            };

            if cursor_row < scroll_row {
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_top = cursor_row.line as usize;
                    pane.scroll_sub_line = cursor_row.chunk;
                    pane.scroll_left = 0;
                }
            } else {
                let rows_between = index.rows_between(scroll_row, cursor_row);

                if rows_between >= rows {
                    let target = index.step(cursor_row, -(rows.saturating_sub(1) as i64));
                    if let Some(pane) = self.active_pane_state_mut() {
                        pane.scroll_top = target.line as usize;
                        pane.scroll_sub_line = target.chunk;
                        pane.scroll_left = 0;
                    }
                } else if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_left = 0;
                }
            }
        }

        self.check_and_emit_editor_events();
    }

    pub(super) fn clamp_scroll(&mut self) {
        let (rows, scroll_top, total, is_wrap) = {
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
                pane.is_wrap(&self.config),
            )
        };

        if !is_wrap {
            let next = clamp_scroll_top(scroll_top, rows, total);
            if let Some(pane) = self.active_pane_state_mut() {
                pane.scroll_top = next;
                pane.scroll_sub_line = 0;
            }
        } else if let Some(pane) = self.active_pane_state_mut() {
            pane.scroll_top = pane.scroll_top.min(total.saturating_sub(1));
            pane.scroll_left = 0;
        }

        self.check_and_emit_editor_events();
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = Instant::now();

        self.apply_config_change(cx);
        self.char_width = ui::pane::font_character_width(&self.config, window);

        // On Linux the compositor may hand the frame to the app; reserve the
        // resize margin before laying anything out.
        let frame = ui::window_frame::WindowFrame::new(window.window_decorations());
        window.set_client_inset(frame.inset());

        // Fit the viewport to the window before choosing which lines to draw.
        let chrome_height = ui::titlebar::TITLEBAR_HEIGHT
            + STATUSLINE_HEIGHT
            + self
                .overlay
                .as_ref()
                .and_then(Overlay::ex)
                .map(|ex| STATUSLINE_HEIGHT + ui::ex::completion_height(ex))
                .unwrap_or(0.0);
        let height =
            f32::from(window.viewport_size().height) - chrome_height - f32::from(frame.vertical());
        let width = f32::from(window.viewport_size().width) - f32::from(frame.horizontal());
        self.update_pane_viewports(width, height);
        self.clamp_scroll();
        self.sync_modal_focus(window, cx);
        // One scroll handle per UI pane, so a focus move can reveal the focused
        // widget and a redraw keeps the offset.
        self.sync_pane_scroll_handles();
        self.sync_pane_bounds();

        let theme = cx.theme().clone();

        let content: AnyElement = match self.layout.clone() {
            Some(layout) => self.render_node(&layout, width, height, cx, window),
            None => {
                let view_entity = cx.entity();
                div()
                    .relative()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.foreground.subtle)
                    .child("waiting for vy-server")
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, cx| {
                                let view = view_entity.read(cx);
                                window.handle_input(
                                    &view.focus_handle,
                                    ElementInputHandler::new(bounds, view_entity.clone()),
                                    cx,
                                );
                                window.handle_input(
                                    &view.overlay_focus_handle,
                                    ElementInputHandler::new(bounds, view_entity.clone()),
                                    cx,
                                );
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .into_any_element()
            }
        };

        let element = div()
            .id("vy-editor")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.editor.background)
            .text_color(theme.editor.text)
            .font_family(self.config.font_family())
            .text_size(px(self.config.font_size()))
            .line_height(px(self.line_height()))
            .child(self.render_titlebar(&theme, window))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(content)
                    .children(
                        self.overlay
                            .as_ref()
                            .filter(|overlay| overlay.is_floating())
                            .map(|_| self.render_floating_overlay(&theme, cx)),
                    ),
            )
            .child(self.render_statusline(&theme))
            .children(
                self.overlay
                    .as_ref()
                    .and_then(Overlay::ex)
                    .map(|_| self.render_ex(&theme)),
            );

        self.log_frame(started);
        ui::window_frame::apply(frame, theme.titlebar.background, element)
    }
}

impl EditorView {
    fn on_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref mut drag) = self.resizing_split {
            if event.pressed_button != Some(MouseButton::Left) {
                self.finish_split_resize(cx);
                return;
            }

            let delta_px = match drag.direction {
                SplitDirection::Horizontal => f32::from(event.position.x - drag.start_mouse.x),
                SplitDirection::Vertical => f32::from(event.position.y - drag.start_mouse.y),
            };

            if drag.total_pixels > 0.0 {
                let new_ratio = (drag.start_ratio + delta_px / drag.total_pixels).clamp(0.05, 0.95);
                drag.current_ratio = new_ratio;
                if let Some(ref mut layout) = self.layout {
                    vy_core::set_boundary_ratio(
                        layout,
                        drag.first_pane,
                        drag.second_pane,
                        new_ratio,
                    );
                }
                cx.notify();
            }
        }
    }

    fn on_mouse_up(
        &mut self,
        _event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_split_resize(cx);
    }

    /// Handles a left click inside a pane.
    ///
    /// The click moves focus to the pane. In an editor pane it also puts the
    /// cursor on the clicked character, using the bounds the canvas recorded
    /// while painting.
    fn on_pane_mouse_down(
        &mut self,
        pane_id: PaneId,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay.is_some() {
            return;
        }

        let is_editor = self
            .panes
            .get(&pane_id)
            .is_some_and(|pane| pane.pane_kind() == vy_app::keymap::PaneKind::Editor);
        let position = if is_editor {
            self.pane_bounds
                .get(&pane_id)
                .and_then(|bounds| bounds.get())
                .and_then(|bounds| {
                    self.position_for_pane_point(pane_id, bounds, event.position, window, cx)
                })
        } else {
            None
        };

        let pane_changed = self.active_pane != Some(pane_id);
        if pane_changed {
            self.switch_active_pane(Some(pane_id));
            self.request::<vy_proto::PaneFocus>(
                vy_proto::PaneFocusParams { pane_id },
                Pending::Focus,
            );
        }

        if let Some(position) = position {
            self.activate_active_view();
            self.editor_mut().move_cursor_to(position);
            self.store_active_view();
            self.ensure_cursor_visible();
        }

        if pane_changed || position.is_some() {
            cx.notify();
        }
    }

    /// Scrolls one editor pane with the wheel, without moving focus.
    fn on_editor_pane_scroll(
        &mut self,
        pane_id: PaneId,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_pane_scroll(pane_id, event, window, cx);
    }

    fn finish_split_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.resizing_split.take() {
            self.request::<vy_proto::PaneResize>(
                vy_proto::PaneResizeParams {
                    pane_id: Some(drag.first_pane),
                    sibling_pane_id: Some(drag.second_pane),
                    ratio: Some(drag.current_ratio),
                    ..Default::default()
                },
                Pending::Resize,
            );
            cx.notify();
        }
    }

    fn render_node(
        &self,
        node: &LayoutNode,
        total_w: f32,
        total_h: f32,
        cx: &mut Context<Self>,
        window: &mut Window,
    ) -> AnyElement {
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

                let (first_w, first_h, second_w, second_h, split_total_pixels) = match direction {
                    SplitDirection::Horizontal => (
                        total_w * ratio,
                        total_h,
                        total_w * (1.0 - ratio),
                        total_h,
                        total_w,
                    ),
                    SplitDirection::Vertical => (
                        total_w,
                        total_h * ratio,
                        total_w,
                        total_h * (1.0 - ratio),
                        total_h,
                    ),
                };

                let first_wrapper = match direction {
                    SplitDirection::Horizontal => div().h_full(),
                    SplitDirection::Vertical => div().w_full(),
                }
                .flex()
                .flex_col()
                .flex_basis(DefiniteLength::Fraction(*ratio))
                .flex_shrink_0()
                .overflow_hidden()
                .child(self.render_node(first, first_w, first_h, cx, window));

                let second_wrapper = match direction {
                    SplitDirection::Horizontal => div().h_full(),
                    SplitDirection::Vertical => div().w_full(),
                }
                .flex()
                .flex_col()
                .flex_basis(DefiniteLength::Fraction(1.0 - *ratio))
                .flex_shrink_0()
                .overflow_hidden()
                .child(self.render_node(second, second_w, second_h, cx, window));

                let first_pane = vy_app::geometry::last_leaf(first);
                let second_pane = vy_app::geometry::first_leaf(second);
                let dir = *direction;
                let current_ratio = *ratio;
                let theme = cx.theme().clone();

                let divider = match dir {
                    SplitDirection::Horizontal => div()
                        .h_full()
                        .w(px(5.0))
                        .mx(px(-2.5))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(CursorStyle::ResizeLeftRight)
                        .occlude()
                        .child(div().h_full().w(px(1.0)).bg(theme.titlebar.border))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, event: &gpui::MouseDownEvent, _window, cx| {
                                view.resizing_split = Some(super::SplitDragState {
                                    first_pane,
                                    second_pane,
                                    direction: dir,
                                    start_mouse: event.position,
                                    start_ratio: current_ratio,
                                    total_pixels: split_total_pixels,
                                    current_ratio,
                                });
                                cx.notify();
                            }),
                        ),
                    SplitDirection::Vertical => div()
                        .w_full()
                        .h(px(5.0))
                        .my(px(-2.5))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(CursorStyle::ResizeUpDown)
                        .occlude()
                        .child(div().w_full().h(px(1.0)).bg(theme.titlebar.border))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, event: &gpui::MouseDownEvent, _window, cx| {
                                view.resizing_split = Some(super::SplitDragState {
                                    first_pane,
                                    second_pane,
                                    direction: dir,
                                    start_mouse: event.position,
                                    start_ratio: current_ratio,
                                    total_pixels: split_total_pixels,
                                    current_ratio,
                                });
                                cx.notify();
                            }),
                        ),
                };

                row.size_full()
                    .child(first_wrapper)
                    .child(divider)
                    .child(second_wrapper)
                    .into_any_element()
            }
        }
    }

    fn render_statusline(&self, theme: &Theme) -> Div {
        ui::statusline::render_statusline(
            theme,
            &self.statusline,
            self.active_view(),
            self.active_document().map(|d| d.uri.as_str()),
            self.active_document()
                .map(|d| d.editor.line_count())
                .unwrap_or(1),
            &self.status,
            STATUSLINE_HEIGHT,
        )
    }

    fn render_ex(&self, theme: &Theme) -> Div {
        ui::ex::render_ex(
            theme,
            &self.overlay_focus_handle,
            self.overlay.as_ref().and_then(Overlay::ex),
            self.config.font_size(),
            CARET_WIDTH,
        )
    }

    fn render_floating_overlay(&self, theme: &Theme, cx: &Context<Self>) -> Div {
        ui::palette::render_overlay(
            theme,
            self,
            cx.entity(),
            &self.overlay_items(),
            self.config.font_size(),
            CARET_WIDTH,
        )
    }

    fn render_pane(
        &self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
        _window: &mut Window,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let pane = self.panes.get(&pane_id);
        let document = pane
            .and_then(|pane| pane.document_id)
            .and_then(|id| self.documents.get(&id));
        let active = self.active_pane == Some(pane_id);

        if let Some(pane) = pane
            && (pane.view_type == "ui" || pane.widget_tree.is_some())
        {
            // The area is the scroll viewport: a panel is taller than the pane
            // for any layout, not only for lists, so the whole tree scrolls.
            let scroll = self.ui_pane_scroll.get(&pane.id);
            let mut area = div()
                .id(gpui::SharedString::from(format!("vy-ui-pane-{}", pane.id)))
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .p_3()
                .bg(theme.background.surface);
            if let Some(scroll) = scroll {
                area = area.track_scroll(&scroll.handle);
            }
            if let Some(tree) = &pane.widget_tree {
                area = area.child(ui::widget::render_widget_node(
                    &theme,
                    tree,
                    pane.id,
                    &pane.focus_path,
                    &pane.list_states,
                    scroll,
                    cx,
                ));
            } else {
                area = area.child(
                    div()
                        .text_color(theme.foreground.muted)
                        .child("empty ui view"),
                );
            }

            if active {
                let view_entity = cx.entity();
                area = area.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            let view = view_entity.read(cx);
                            window.handle_input(
                                &view.focus_handle,
                                ElementInputHandler::new(bounds, view_entity.clone()),
                                cx,
                            );
                            window.handle_input(
                                &view.overlay_focus_handle,
                                ElementInputHandler::new(bounds, view_entity.clone()),
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

            let border = if active && self.panes.len() > 1 {
                theme.background.split_active
            } else if self.panes.len() > 1 {
                theme.background.split_inactive
            } else {
                gpui::rgba(0x00000000)
            };
            // A click anywhere in the panel activates its pane. Widgets also
            // set their own focus; that handler runs first and this one only
            // moves the pane when it is still inactive.
            let click_pane_id = pane.id;
            area = area.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.on_pane_mouse_down(click_pane_id, event, window, cx);
                }),
            );
            return area.border_1().border_color(border).into_any_element();
        }

        let view_entity = cx.entity();
        let view_entity_paint = view_entity.clone();
        let bounds_cell = self.pane_bounds.get(&pane_id).cloned();
        let bounds_cell_paint = bounds_cell.clone();
        let editor_canvas = canvas(
            move |bounds, _window, cx| {
                if let Some(cell) = &bounds_cell_paint {
                    cell.set(Some(bounds));
                }
                let view = view_entity.read(cx);
                let pane = view.panes.get(&pane_id);
                let document = pane
                    .and_then(|pane| pane.document_id)
                    .and_then(|id| view.documents.get(&id));

                pane.zip(document).map(|(pane, document)| {
                    ui::pane::prepare_editor_paint(
                        view.config_arc.clone(),
                        view.theme_arc.clone(),
                        pane,
                        document,
                        &view.gutter,
                        bounds,
                        view.cursor_visible,
                    )
                })
            },
            move |bounds, paint_data, window, cx| {
                if let Some(data) = paint_data {
                    ui::pane::paint_editor(data, bounds, window, cx);
                }

                if active {
                    let view = view_entity_paint.read(cx);
                    window.handle_input(
                        &view.focus_handle,
                        ElementInputHandler::new(bounds, view_entity_paint.clone()),
                        cx,
                    );
                    window.handle_input(
                        &view.overlay_focus_handle,
                        ElementInputHandler::new(bounds, view_entity_paint.clone()),
                        cx,
                    );
                }
            },
        )
        .size_full();

        let mut area = div()
            .relative()
            .flex_1()
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(
                move |view, event: &gpui::ScrollWheelEvent, window, cx| {
                    view.on_editor_pane_scroll(pane_id, event, window, cx);
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                    view.on_pane_mouse_down(pane_id, event, window, cx);
                }),
            )
            .child(editor_canvas);

        if active {
            // Render hover popup if active
            if let Some(hover) = &self.hover
                && let Some(pane) = pane
                && let Some(doc) = document
            {
                let lh = self.line_height();
                let ch_w = self.char_width;
                let gutter = self.gutter_width(doc.editor.line_count());
                let hover_position =
                    vy_core::Position::new(hover.line as u32, hover.character as u32);

                let pos = if !pane.is_wrap(&self.config) {
                    if hover.line >= pane.scroll_top
                        && hover.line < pane.scroll_top + pane.viewport_rows
                    {
                        let display_col = doc
                            .editor
                            .visual_index_with(None)
                            .display_col(hover_position);
                        let scroll_left_px = pane.scroll_left as f32 * ch_w;
                        let top = (hover.line - pane.scroll_top) as f32 * lh + lh;
                        let left = gutter + display_col as f32 * ch_w - scroll_left_px;
                        Some((top, left))
                    } else {
                        None
                    }
                } else {
                    let cols = pane.viewport_cols.max(1);
                    let index = doc.editor.visual_index_with(Some(cols));
                    let scroll_row = VisualRow {
                        line: pane.scroll_top as u32,
                        chunk: pane.scroll_sub_line,
                    };
                    let target = index.row_of(hover_position);
                    let v_row = index.rows_between(scroll_row, target);

                    if v_row >= pane.viewport_rows {
                        None
                    } else {
                        let col_in_chunk = index.display_col(hover_position);
                        let top = v_row as f32 * lh + lh;
                        let left = gutter + col_in_chunk as f32 * ch_w;
                        Some((top, left))
                    }
                };

                if let Some((top, left)) = pos
                    && left >= gutter
                {
                    area = area.child(
                        div()
                            .absolute()
                            .top(px(top))
                            .left(px(left))
                            .max_w(px(400.0))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(theme.background.surface)
                            .border_1()
                            .border_color(theme.border.subtle)
                            .shadow_md()
                            .text_xs()
                            .text_color(theme.foreground.default)
                            .child(hover.text.clone()),
                    );
                }
            }
        }

        let border = if active && self.panes.len() > 1 {
            theme.background.split_active
        } else if self.panes.len() > 1 {
            theme.background.split_inactive
        } else {
            theme.editor.background
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .border_1()
            .border_color(border)
            .child(area)
            .into_any_element()
    }

    fn render_titlebar(&self, theme: &Theme, window: &mut Window) -> impl IntoElement {
        let (title, is_dirty) = if let Some(doc) = self.active_document() {
            let name = doc.uri.strip_prefix("file://").unwrap_or(&doc.uri);
            let display_name = if name == "vy:empty" || name == "vy:untitled" {
                "[No Name]"
            } else {
                std::path::Path::new(name)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(name)
            };
            (format!("vy — {display_name}"), doc.is_dirty())
        } else {
            ("vy".to_string(), false)
        };

        ui::titlebar::render_titlebar(
            theme,
            title,
            is_dirty,
            window,
            self.titlebar_press.clone(),
            |window| {
                #[cfg(target_os = "macos")]
                window.titlebar_double_click();
                #[cfg(not(target_os = "macos"))]
                window.zoom_window();
            },
        )
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
