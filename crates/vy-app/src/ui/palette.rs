//! Command palette UI rendering.

use std::rc::Rc;

use gpui::{AnyElement, Entity, IntoElement, div, prelude::*, px, size};
use gpui_base::{FocusTrapElement, v_virtual_list};
use vy_app::palette::{
    PALETTE_MAX_HEIGHT, PALETTE_MAX_WIDTH, PaletteMode, PickerItem, filtered_picker_indices,
};

use super::ex::input_caret;
use crate::theme::Theme;
use crate::view::EditorView;

/// Height of one row in the picker list.
pub const PALETTE_ROW_HEIGHT: f32 = 28.0;

/// Renders a single row in the picker / palette.
pub fn render_picker_row(theme: &Theme, item: &PickerItem, is_selected: bool) -> gpui::Div {
    let row = div()
        .flex()
        .justify_between()
        .items_center()
        .h(px(PALETTE_ROW_HEIGHT))
        .line_height(px(PALETTE_ROW_HEIGHT))
        .px_2()
        .w_full()
        .overflow_hidden();
    let row = if is_selected {
        row.bg(theme.list.active_background)
            .text_color(theme.list.active_foreground)
    } else {
        row.text_color(theme.foreground.default)
    };

    let title_section = div()
        .flex()
        .gap_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .child(item.title.clone());

    let title_section = if let Some(description) = &item.description {
        title_section.child(
            div()
                .text_xs()
                .text_color(theme.foreground.muted)
                .child(description.clone()),
        )
    } else {
        title_section
    };

    let row = row.child(title_section);

    if let Some(detail) = &item.detail {
        row.child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.foreground.muted)
                .child(detail.clone()),
        )
    } else {
        row
    }
}

/// Renders the floating modal picker / command palette with virtual scrolling.
pub fn render_palette(
    theme: &Theme,
    view: &EditorView,
    entity: Entity<EditorView>,
    items: &[PickerItem],
    font_size: f32,
    caret_width: f32,
) -> gpui::Div {
    let Some(palette) = view.palette.as_ref() else {
        return div();
    };

    let focus_handle = &view.palette_focus_handle;
    let scroll_handle = &view.palette_scroll_handle;

    let filtered = filtered_picker_indices(items, &palette.query);
    let filtered_count = filtered.len();

    let count_label = match palette.mode {
        PaletteMode::Commands => format!("{filtered_count} commands"),
        PaletteMode::Documents => format!("{filtered_count} documents"),
    };

    let query = div()
        .flex()
        .justify_between()
        .items_center()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(theme.border.subtle)
        .child(
            div()
                .flex()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(format!("> {}", palette.query))
                .child(input_caret(theme, font_size, caret_width)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.foreground.muted)
                .child(count_label),
        );

    let empty_label = match palette.mode {
        PaletteMode::Commands => "no matching commands",
        PaletteMode::Documents => "no matching documents",
    };

    let rows_element: AnyElement = if filtered.is_empty() {
        div()
            .px_2()
            .py_2()
            .text_color(theme.foreground.muted)
            .child(empty_label)
            .into_any_element()
    } else {
        let item_sizes = Rc::new(vec![
            size(px(PALETTE_MAX_WIDTH), px(PALETTE_ROW_HEIGHT));
            filtered_count
        ]);
        let theme_clone = theme.clone();
        let items_clone = items.to_vec();
        let filtered_clone = filtered.clone();

        v_virtual_list(
            entity,
            "palette-virtual-list",
            item_sizes,
            move |view, visible_range, _window, _cx| {
                let selected = view.palette.as_ref().map(|p| p.selected).unwrap_or(0);
                visible_range
                    .map(|ix| {
                        let item_idx = filtered_clone[ix];
                        let item = &items_clone[item_idx];
                        render_picker_row(&theme_clone, item, ix == selected)
                    })
                    .collect()
            },
        )
        .track_scroll(scroll_handle)
        .h(px(
            (filtered_count as f32 * PALETTE_ROW_HEIGHT).min(PALETTE_MAX_HEIGHT - 80.0)
        ))
        .into_any_element()
    };

    let hint_label = match palette.mode {
        PaletteMode::Commands => "Enter run  ·  Esc close  ·  Up/Down move",
        PaletteMode::Documents => "Enter switch  ·  Esc close  ·  Up/Down move",
    };

    let hint = div()
        .px_2()
        .pt_1()
        .text_xs()
        .text_color(theme.foreground.muted)
        .whitespace_nowrap()
        .child(hint_label);

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
                .bg(theme.background.overlay)
                .border_1()
                .border_color(theme.border.subtle)
                .flex()
                .flex_col()
                .child(query)
                .child(rows_element)
                .child(hint)
                .focus_trap("palette-modal-trap", focus_handle),
        )
}
