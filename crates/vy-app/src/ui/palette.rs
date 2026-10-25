//! Command palette UI rendering.

use std::rc::Rc;

use gpui::{AnyElement, Entity, IntoElement, div, prelude::*, px, size};
use gpui_base::{FocusTrapElement, v_virtual_list};
use vy_app::palette::{
    PALETTE_MAX_HEIGHT, PALETTE_MAX_WIDTH, PaletteMode, PaletteRow, PickerItem,
    filtered_picker_indices, palette_rows, shows_palette_headings,
};

use super::ex::input_caret;
use crate::theme::Theme;
use crate::view::EditorView;

/// Height of one row in the picker list.
pub const PALETTE_ROW_HEIGHT: f32 = 28.0;
/// Height of a group heading row in the picker list.
pub const PALETTE_HEADING_HEIGHT: f32 = 22.0;

/// Height of one rendered palette line.
fn palette_row_height(row: &PaletteRow) -> f32 {
    match row {
        PaletteRow::Heading(_) => PALETTE_HEADING_HEIGHT,
        PaletteRow::Item(_) => PALETTE_ROW_HEIGHT,
    }
}

/// Renders a group heading between palette rows.
pub fn render_picker_heading(theme: &Theme, label: &str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .h(px(PALETTE_HEADING_HEIGHT))
        .px_2()
        .text_xs()
        .text_color(theme.foreground.muted)
        .child(label.to_string())
}

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
    let rows = palette_rows(items, &filtered, shows_palette_headings(&palette.query));

    let count_label = match &palette.mode {
        PaletteMode::Commands => format!("{filtered_count} commands"),
        PaletteMode::Documents => format!("{filtered_count} documents"),
        PaletteMode::Arguments { spec, index, .. } => {
            let required = spec.args().iter().filter(|arg| arg.required).count();
            format!("{} / {required}", index + 1)
        }
        PaletteMode::Custom { .. } => format!("{filtered_count} items"),
        PaletteMode::Approval { missing, .. } => format!("{} permission(s)", missing.len()),
    };

    let prefix = match &palette.mode {
        PaletteMode::Custom {
            placeholder: Some(p),
            ..
        } => format!("{p} > "),
        PaletteMode::Approval { plugin_name, .. } => format!("Approve {plugin_name}? > "),
        PaletteMode::Arguments {
            title, spec, index, ..
        } => {
            let required: Vec<_> = spec.args().iter().filter(|arg| arg.required).collect();
            match required.get(*index) {
                Some(arg) => format!("{title} · {} > ", arg.name),
                None => format!("{title} > "),
            }
        }
        _ => "> ".to_string(),
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
                .child(format!("{prefix}{}", palette.query))
                .when(!palette.preedit.is_empty(), |el| {
                    el.child(
                        div()
                            .border_b_1()
                            .border_color(theme.foreground.default)
                            .child(palette.preedit.clone()),
                    )
                })
                .child(input_caret(theme, font_size, caret_width)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.foreground.muted)
                .child(count_label),
        );

    let empty_label = match &palette.mode {
        PaletteMode::Commands => "no matching commands",
        PaletteMode::Documents => "no matching documents",
        PaletteMode::Arguments { spec, index, .. } => {
            let required: Vec<_> = spec.args().iter().filter(|arg| arg.required).collect();
            match required.get(*index) {
                Some(arg) if arg.choices.is_empty() => "type a value and press Enter",
                Some(_) => "no matching choices",
                None => "argument",
            }
        }
        PaletteMode::Custom { .. } => "no matching items",
        PaletteMode::Approval { .. } => "no matching options",
    };

    let rows_element: AnyElement = if filtered.is_empty() {
        div()
            .px_2()
            .py_2()
            .text_color(theme.foreground.muted)
            .child(empty_label)
            .into_any_element()
    } else {
        let item_sizes = Rc::new(
            rows.iter()
                .map(|row| size(px(PALETTE_MAX_WIDTH), px(palette_row_height(row))))
                .collect::<Vec<_>>(),
        );
        let theme_clone = theme.clone();
        let items_clone = items.to_vec();
        let rows_clone = rows.clone();
        let selected_item = filtered.get(palette.selected).copied();

        v_virtual_list(
            entity,
            "palette-virtual-list",
            item_sizes,
            move |_view, visible_range, _window, _cx| {
                visible_range
                    .map(|ix| match &rows_clone[ix] {
                        PaletteRow::Heading(label) => render_picker_heading(&theme_clone, label),
                        PaletteRow::Item(index) => render_picker_row(
                            &theme_clone,
                            &items_clone[*index],
                            Some(*index) == selected_item,
                        ),
                    })
                    .collect()
            },
        )
        .track_scroll(scroll_handle)
        .h(px(rows
            .iter()
            .map(palette_row_height)
            .sum::<f32>()
            .min(PALETTE_MAX_HEIGHT - 80.0)))
        .into_any_element()
    };

    let hint_label = match &palette.mode {
        PaletteMode::Commands => {
            "Enter run  ·  : for named commands  ·  Esc close  ·  Up/Down move"
        }
        PaletteMode::Documents => "Enter switch  ·  Esc close  ·  Up/Down move",
        PaletteMode::Arguments { .. } => "Enter confirm  ·  Esc back",
        PaletteMode::Custom { .. } => "Enter select  ·  Esc close  ·  Up/Down move",
        PaletteMode::Approval { .. } => "Enter choose  ·  Esc deny  ·  Up/Down move",
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
