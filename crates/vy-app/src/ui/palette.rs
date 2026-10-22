//! Command palette UI rendering.

use gpui::{Div, div, prelude::*, px, rgb};
use vy_app::commands::CommandDescriptor;
use vy_app::palette::{
    PALETTE_MAX_HEIGHT, PALETTE_MAX_WIDTH, PaletteState, filtered_palette_indices,
};

use super::ex::input_caret;

/// Renders the floating modal command palette.
pub fn render_palette(
    palette: Option<&PaletteState>,
    items: &[CommandDescriptor],
    font_size: f32,
    caret_width: f32,
) -> Div {
    let Some(palette) = palette else {
        return div();
    };

    let filtered = filtered_palette_indices(items, &palette.query);
    let selected = palette.selected.min(filtered.len().saturating_sub(1));

    let window = 12usize;
    let start = if filtered.len() <= window {
        0
    } else {
        selected
            .saturating_sub(window / 2)
            .min(filtered.len() - window)
    };
    let end = (start + window).min(filtered.len());

    let query = div()
        .flex()
        .justify_between()
        .items_center()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(rgb(0x3c3c3c))
        .child(
            div()
                .flex()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(format!("> {}", palette.query))
                .child(input_caret(font_size, caret_width)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(rgb(0x8a8a8a))
                .child(format!("{} commands", filtered.len())),
        );

    let mut rows = Vec::new();
    for index in start..end {
        let item = &items[filtered[index]];
        let row = div()
            .flex()
            .justify_between()
            .items_center()
            .px_2()
            .py_1()
            .w_full()
            .overflow_hidden();
        let row = if index == selected {
            row.bg(rgb(0x094771))
        } else {
            row
        };
        rows.push(
            row.child(
                div()
                    .flex()
                    .gap_2()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(item.title.clone())
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x8a8a8a))
                            .child(item.id.clone()),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(0x8a8a8a))
                    .child(item.source.label().to_string()),
            ),
        );
    }

    if rows.is_empty() {
        rows.push(
            div()
                .px_2()
                .py_1()
                .text_color(rgb(0x8a8a8a))
                .child("no matching commands"),
        );
    }

    let hint = div()
        .px_2()
        .pt_1()
        .text_xs()
        .text_color(rgb(0x8a8a8a))
        .whitespace_nowrap()
        .child("Enter run  ·  Esc close  ·  Up/Down move");

    div()
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .p_4()
        .flex()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(px(PALETTE_MAX_WIDTH))
                .max_h(px(PALETTE_MAX_HEIGHT))
                .overflow_hidden()
                .bg(rgb(0x252526))
                .border_1()
                .border_color(rgb(0x3c3c3c))
                .flex()
                .flex_col()
                .child(query)
                .children(rows)
                .child(hint),
        )
}
