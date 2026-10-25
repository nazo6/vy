//! Ex command line UI rendering.

use gpui::{Div, FocusHandle, div, prelude::*, px};
use gpui_base::FocusTrapElement;
use vy_app::ex::ExState;

use crate::theme::Theme;

/// Renders a thin caret bar for single-line text inputs.
pub fn input_caret(theme: &Theme, font_size: f32, caret_width: f32) -> Div {
    div()
        .w(px(caret_width))
        .h(px(font_size))
        .flex_shrink_0()
        .bg(theme.editor.caret)
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
        .child(
            div()
                .flex()
                .items_center()
                .child(ex.input.clone())
                .when(!ex.preedit.is_empty(), |el| {
                    el.child(
                        div()
                            .border_b_1()
                            .border_color(theme.foreground.default)
                            .child(ex.preedit.clone()),
                    )
                })
                .child(input_caret(theme, font_size, caret_width)),
        )
        .focus_trap("ex-bar-trap", focus_handle);

    div().child(bar)
}
