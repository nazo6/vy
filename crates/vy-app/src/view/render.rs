//! GPUI Render implementation and layout rendering for `EditorView`.

use std::time::Instant;

use gpui::{
    Context, CursorStyle, DefiniteLength, Div, ElementInputHandler, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, Styled, Window, canvas, div, px,
};

use vy_app::overlay::Overlay;
use vy_app::text_layout::{
    DEFAULT_TAB_STOP, cached_chunk_line, clamp_scroll_top, scroll_to_show,
    scroll_to_show_horizontal, utf16_to_display_col,
};
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
            let text = document.editor.line(cursor_line).unwrap_or_default();
            let display_col = vy_app::text_layout::utf16_to_display_col(
                &text,
                cursor.character as usize,
                vy_app::text_layout::DEFAULT_TAB_STOP,
            );
            let next_left = scroll_to_show_horizontal(display_col, pane.scroll_left, cols);
            if let Some(pane) = self.active_pane_state_mut() {
                pane.scroll_top = next_top;
                pane.scroll_left = next_left;
                pane.scroll_sub_line = 0;
            }
        } else {
            let text = document.editor.line(cursor_line).unwrap_or_default();
            let line_chunks = vy_app::text_layout::cached_chunk_line(&text, cols, DEFAULT_TAB_STOP);
            let chunks = &line_chunks.chunks;
            let cursor_chunk = chunks
                .iter()
                .enumerate()
                .find_map(|(idx, chunk)| {
                    vy_app::text_layout::cursor_in_chunk(
                        cursor,
                        cursor_line,
                        idx,
                        chunks.len(),
                        &chunk.utf16_range,
                    )
                    .map(|_| idx)
                })
                .unwrap_or(0);

            let (scroll_top, scroll_sub) = (pane.scroll_top, pane.scroll_sub_line);

            if (cursor_line, cursor_chunk) < (scroll_top, scroll_sub) {
                if let Some(pane) = self.active_pane_state_mut() {
                    pane.scroll_top = cursor_line;
                    pane.scroll_sub_line = cursor_chunk;
                    pane.scroll_left = 0;
                }
            } else {
                let mut rows_between = 0;
                let mut cur_l = scroll_top;
                let mut cur_c = scroll_sub;
                let mut found = false;

                while cur_l <= cursor_line {
                    let cur_text = document.editor.line(cur_l).unwrap_or_default();
                    let cur_chunks_len =
                        vy_app::text_layout::cached_chunk_line(&cur_text, cols, DEFAULT_TAB_STOP)
                            .chunks
                            .len();
                    while cur_c < cur_chunks_len {
                        if cur_l == cursor_line && cur_c == cursor_chunk {
                            found = true;
                            break;
                        }
                        rows_between += 1;
                        cur_c += 1;
                    }
                    if found {
                        break;
                    }
                    cur_l += 1;
                    cur_c = 0;
                }

                if rows_between >= rows {
                    let mut needed_steps = rows.saturating_sub(1);
                    let mut b_line = cursor_line;
                    let mut b_chunk = cursor_chunk;

                    while needed_steps > 0 {
                        if b_chunk > 0 {
                            b_chunk -= 1;
                            needed_steps -= 1;
                        } else if b_line > 0 {
                            b_line -= 1;
                            let prev_text = document.editor.line(b_line).unwrap_or_default();
                            let prev_chunks = vy_app::text_layout::cached_chunk_line(
                                &prev_text,
                                cols,
                                DEFAULT_TAB_STOP,
                            )
                            .chunks
                            .len();
                            b_chunk = prev_chunks.saturating_sub(1);
                            needed_steps -= 1;
                        } else {
                            break;
                        }
                    }
                    if let Some(pane) = self.active_pane_state_mut() {
                        pane.scroll_top = b_line;
                        pane.scroll_sub_line = b_chunk;
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

        let theme = cx.theme().clone();

        let content = match self.layout.clone() {
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
            }
        };

        let element = div()
            .id("vy-editor")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
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
                    .flex_1()
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
    ) -> Div {
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
                .flex_basis(DefiniteLength::Fraction(*ratio))
                .flex_shrink_0()
                .overflow_hidden()
                .child(self.render_node(first, first_w, first_h, cx, window));

                let second_wrapper = match direction {
                    SplitDirection::Horizontal => div().h_full(),
                    SplitDirection::Vertical => div().w_full(),
                }
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

    fn render_pane(&self, pane_id: PaneId, cx: &mut Context<Self>, _window: &mut Window) -> Div {
        let theme = cx.theme().clone();
        let pane = self.panes.get(&pane_id);
        let document = pane
            .and_then(|pane| pane.document_id)
            .and_then(|id| self.documents.get(&id));
        let active = self.active_pane == Some(pane_id);

        if let Some(pane) = pane
            && (pane.view_type == "ui" || pane.widget_tree.is_some())
        {
            let mut area = div()
                .relative()
                .flex_1()
                .overflow_hidden()
                .p_3()
                .bg(theme.background.surface);
            if let Some(tree) = &pane.widget_tree {
                area = area.child(ui::widget::render_widget_node(&theme, tree, cx));
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
            return area.border_1().border_color(border);
        }

        let view_entity = cx.entity();
        let view_entity_paint = view_entity.clone();
        let editor_canvas = canvas(
            move |bounds, _window, cx| {
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
                let line_text = doc.editor.line(hover.line).unwrap_or_default();

                let pos = if !pane.is_wrap(&self.config) {
                    if hover.line >= pane.scroll_top
                        && hover.line < pane.scroll_top + pane.viewport_rows
                    {
                        let display_col =
                            utf16_to_display_col(&line_text, hover.character, DEFAULT_TAB_STOP);
                        let scroll_left_px = pane.scroll_left as f32 * ch_w;
                        let top = (hover.line - pane.scroll_top) as f32 * lh + lh;
                        let left = gutter + display_col as f32 * ch_w - scroll_left_px;
                        Some((top, left))
                    } else {
                        None
                    }
                } else {
                    let cols = pane.viewport_cols.max(1);
                    let mut l_idx = pane.scroll_top;
                    let mut s_idx = pane.scroll_sub_line;
                    let mut v_row = 0;
                    let total_lines = doc.editor.line_count();
                    let mut found = None;

                    while l_idx < total_lines && v_row < pane.viewport_rows {
                        let text = doc.editor.line(l_idx).unwrap_or_default();
                        let line_chunks = cached_chunk_line(&text, cols, DEFAULT_TAB_STOP);
                        let chunks = &line_chunks.chunks;
                        let total_chunks = chunks.len();

                        while s_idx < total_chunks && v_row < pane.viewport_rows {
                            if l_idx == hover.line {
                                let chunk = &chunks[s_idx];
                                let char_idx = hover.character;
                                if char_idx >= chunk.utf16_range.start
                                    && (char_idx < chunk.utf16_range.end
                                        || s_idx + 1 == total_chunks)
                                {
                                    let chunk_text = &text[chunk.byte_range.clone()];
                                    let chunk_char_offset =
                                        char_idx.saturating_sub(chunk.utf16_range.start);
                                    let col_in_chunk = utf16_to_display_col(
                                        chunk_text,
                                        chunk_char_offset,
                                        DEFAULT_TAB_STOP,
                                    );
                                    let top = v_row as f32 * lh + lh;
                                    let left = gutter + col_in_chunk as f32 * ch_w;
                                    found = Some((top, left));
                                    break;
                                }
                            }
                            s_idx += 1;
                            v_row += 1;
                        }
                        if found.is_some() || l_idx > hover.line {
                            break;
                        }
                        l_idx += 1;
                        s_idx = 0;
                    }
                    found
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
