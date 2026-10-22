//! Ex command line UI rendering.

use gpui::{Div, div, prelude::*, px, rgb};
use vy_app::ex::ExState;

/// Renders a thin caret bar for single-line text inputs.
pub fn input_caret(font_size: f32, caret_width: f32) -> Div {
    div()
        .w(px(caret_width))
        .h(px(font_size))
        .flex_shrink_0()
        .bg(rgb(0xd4d4d4))
}

/// Renders the Ex command line input bar at the bottom of the window.
pub fn render_ex(ex: Option<&ExState>, font_size: f32, caret_width: f32) -> Div {
    let Some(ex) = ex else {
        return div();
    };

    div()
        .flex()
        .gap_1()
        .items_center()
        .px_2()
        .py_1()
        .bg(rgb(0x252526))
        .text_sm()
        .child(div().text_color(rgb(0x9a9a9a)).child(":"))
        .child(
            div()
                .flex()
                .items_center()
                .child(ex.input.clone())
                .child(input_caret(font_size, caret_width)),
        )
}
