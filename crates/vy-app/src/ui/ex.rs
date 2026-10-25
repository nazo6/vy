//! Ex command line UI rendering.

use std::ops::Range;

use gpui::{Div, FocusHandle, div, prelude::*, px};
use gpui_base::FocusTrapElement;
use vy_app::ex::ExState;

use super::input::render_input_text;
use crate::theme::Theme;

/// Maximum completion candidates shown above the Ex line.
const MAX_COMPLETION_ROWS: usize = 8;
/// Height of one completion row.
const COMPLETION_ROW_HEIGHT: f32 = 22.0;
/// Space taken by the completion list, including its top border.
pub fn completion_height(ex: &ExState) -> f32 {
    let rows = visible_candidates(ex).len();
    if rows == 0 {
        0.0
    } else {
        rows as f32 * COMPLETION_ROW_HEIGHT + 1.0
    }
}

fn visible_candidates(ex: &ExState) -> Range<usize> {
    let len = ex.completion.candidates().len();
    if len == 0 {
        return 0..0;
    }
    let selected = ex.completion.selected_index().min(len - 1);
    let start = selected.saturating_sub(MAX_COMPLETION_ROWS - 1);
    start..(start + MAX_COMPLETION_ROWS).min(len)
}

/// Renders the Ex command line input bar at the bottom of the window.
pub fn render_ex(
    theme: &Theme,
    focus_handle: &FocusHandle,
    ex: Option<&ExState>,
    font_size: f32,
    caret_width: f32,
) -> Div {
    let Some(ex) = ex else {
        return div();
    };

    let bar = div()
        .flex()
        .gap_1()
        .items_center()
        .px_2()
        .py_1()
        .bg(theme.background.surface)
        .text_color(theme.foreground.default)
        .text_sm()
        .child(div().text_color(theme.foreground.prompt).child(":"))
        .child(render_input_text(theme, &ex.input, font_size, caret_width))
        .focus_trap("ex-bar-trap", focus_handle);

    div()
        .flex()
        .flex_col()
        .child(render_completion(theme, ex))
        .child(bar)
}

/// Renders the Ex command-name completion candidates above the input.
fn render_completion(theme: &Theme, ex: &ExState) -> Div {
    let candidates = ex.completion.candidates();
    let visible = visible_candidates(ex);
    if visible.is_empty() {
        return div();
    }

    let selected = ex.completion.selected_index();
    let mut list = div()
        .flex()
        .flex_col()
        .px_2()
        .bg(theme.background.surface)
        .border_t_1()
        .border_color(theme.border.subtle);

    for index in visible {
        let row = div()
            .h(px(COMPLETION_ROW_HEIGHT))
            .line_height(px(COMPLETION_ROW_HEIGHT))
            .px_2()
            .text_sm()
            .child(candidates[index].clone());
        list = list.child(if index == selected {
            row.bg(theme.list.active_background)
                .text_color(theme.list.active_foreground)
        } else {
            row.text_color(theme.foreground.muted)
        });
    }

    list
}
