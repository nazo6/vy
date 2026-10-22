//! GPUI Render implementation and layout rendering for `EditorView`.

use std::time::Instant;

use gpui::{
    Context, DefiniteLength, Div, ElementInputHandler, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window, canvas, div, px, rgb,
};
use vy_app::state::{DocumentState, PaneState};
use vy_app::text_layout::{clamp_scroll_top, scroll_to_show};
use vy_core::{LayoutNode, PaneId, SplitDirection};

use super::EditorView;
use crate::ui;

/// Height of the global statusline.
const STATUSLINE_HEIGHT: f32 = 24.0;
/// Width of the bar that marks the insertion point.
const CARET_WIDTH: f32 = 2.0;

impl EditorView {
    /// Collects the lines visible in one pane.
    pub(super) fn visible_lines(
        &self,
        pane: &PaneState,
        document: &DocumentState,
        window: &mut Window,
    ) -> Vec<Div> {
        ui::pane::visible_lines(&self.config, pane, document, window)
    }

    /// Height of one text row from the settings.
    pub(super) fn line_height(&self) -> f32 {
        ui::pane::line_height(&self.config)
    }

    /// Width of the line number gutter. It grows with the number of lines and is
    /// zero when line numbers are turned off.
    pub(super) fn gutter_width(&self, line_count: usize) -> f32 {
        ui::pane::gutter_width(&self.config, line_count)
    }

    /// Moves the active viewport just enough to show the cursor.
    pub(super) fn ensure_cursor_visible(&mut self) {
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

    pub(super) fn clamp_scroll(&mut self) {
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

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = Instant::now();

        // Fit the viewport to the window before choosing which lines to draw.
        let chrome_height = STATUSLINE_HEIGHT
            + if self.ex.is_some() {
                STATUSLINE_HEIGHT
            } else {
                0.0
            };
        let height = f32::from(window.viewport_size().height) - chrome_height;
        let rows = ((height / self.line_height()).floor().max(1.0)) as usize;
        if let Some(pane) = self.active_pane_state_mut() {
            pane.viewport_rows = rows;
        }
        self.clamp_scroll();

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
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .child(content)
                    .children(self.palette.as_ref().map(|_| self.render_palette())),
            )
            .child(self.render_statusline())
            .children(self.ex.as_ref().map(|_| self.render_ex()));

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

    fn render_statusline(&self) -> Div {
        ui::statusline::render_statusline(
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

    fn render_ex(&self) -> Div {
        ui::ex::render_ex(self.ex.as_ref(), self.config.font_size(), CARET_WIDTH)
    }

    fn render_palette(&self) -> Div {
        ui::palette::render_palette(
            self.palette.as_ref(),
            &self.palette_items(),
            self.config.font_size(),
            CARET_WIDTH,
        )
    }

    fn render_pane(&self, pane_id: PaneId, cx: &mut Context<Self>, window: &mut Window) -> Div {
        let pane = self.panes.get(&pane_id);
        let document = pane
            .and_then(|pane| pane.document_id)
            .and_then(|id| self.documents.get(&id));
        let active = self.active_pane == Some(pane_id);

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

        let border = if active && self.panes.len() > 1 {
            rgb(0x2b5e8c)
        } else if self.panes.len() > 1 {
            rgb(0x2d2d2d)
        } else {
            rgb(0x1e1e1e)
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .border_1()
            .border_color(border)
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
