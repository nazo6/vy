//! Shared single-line input rendering.

use gpui::{Div, div, prelude::*, px};
use vy_app::overlay::TextInput;

use crate::theme::Theme;

/// Renders a thin caret bar for single-line text inputs.
pub fn input_caret(theme: &Theme, font_size: f32, caret_width: f32) -> Div {
    div()
        .w(px(caret_width))
        .h(px(font_size))
        .flex_shrink_0()
        .bg(theme.editor.caret)
}

/// Renders an input's committed text, IME preedit, and caret.
pub fn render_input_text(
    theme: &Theme,
    input: &TextInput,
    font_size: f32,
    caret_width: f32,
) -> Div {
    div()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .child(input.value().to_string())
        .when(!input.preedit().is_empty(), |element| {
            element.child(
                div()
                    .border_b_1()
                    .border_color(theme.foreground.default)
                    .child(input.preedit().to_string()),
            )
        })
        .child(input_caret(theme, font_size, caret_width))
}
