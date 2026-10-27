//! Command palette UI rendering.

use std::rc::Rc;

use gpui::{AnyElement, Entity, IntoElement, div, prelude::*, px, size};
use gpui_base::{FocusTrapElement, v_virtual_list};
use vy_app::overlay::{SurfaceKind, SurfacePlacement};
use vy_app::palette::{
    PALETTE_MAX_HEIGHT, PALETTE_MAX_WIDTH, PaletteMode, PaletteRow, PickerItem,
    filtered_picker_indices, palette_rows, shows_palette_headings,
};

use super::input::render_input_text;
use crate::theme::Theme;
use crate::view::EditorView;

/// Height of one row in the picker list.
pub const PALETTE_ROW_HEIGHT: f32 = 28.0;
/// Height of a group heading row in the picker list.
pub const PALETTE_HEADING_HEIGHT: f32 = 22.0;
/// Height of the overlay list viewport: the panel's max height minus the
/// query and hint rows. Page movement is derived from this (ADR 0032
/// review m5), so it must stay in sync with the list height below.
pub const PALETTE_LIST_HEIGHT: f32 = PALETTE_MAX_HEIGHT - 80.0;

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

/// Labels around the floating overlay list.
struct OverlayLabels {
    count: String,
    empty: &'static str,
    hint: String,
}

/// Builds overlay labels, with hint keys taken from the keymap when a
/// binding matches the current overlay context, and hard-coded keys
/// otherwise (ADR 0032 Phase 3). `lookup` maps a command id to its
/// display keys, or [`None`] when unbound.
fn overlay_labels(
    overlay: &SurfaceKind,
    filtered_count: usize,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> OverlayLabels {
    // Key names for the overlay actions; fall back to the keys the old
    // hard-coded hints showed when a command is unbound.
    let activate = lookup("vy.ui.activate").unwrap_or_else(|| "Enter".to_string());
    let cancel = lookup("vy.ui.cancel").unwrap_or_else(|| "Esc".to_string());
    let prev = lookup("vy.ui.prev").unwrap_or_else(|| "Up".to_string());
    let next = lookup("vy.ui.next").unwrap_or_else(|| "Down".to_string());
    match overlay {
        SurfaceKind::Palette(palette) => match &palette.mode {
            PaletteMode::Commands => OverlayLabels {
                count: format!("{filtered_count} commands"),
                empty: "no matching commands",
                hint: format!(
                    "{activate} run  ·  : for named commands  ·  {cancel} close  ·  {prev}/{next} move"
                ),
            },
            PaletteMode::Documents => OverlayLabels {
                count: format!("{filtered_count} documents"),
                empty: "no matching documents",
                hint: format!("{activate} switch  ·  {cancel} close  ·  {prev}/{next} move"),
            },
            PaletteMode::Custom { .. } => OverlayLabels {
                count: format!("{filtered_count} items"),
                empty: "no matching items",
                hint: format!("{activate} select  ·  {cancel} close  ·  {prev}/{next} move"),
            },
        },
        SurfaceKind::Prompt(prompt) => OverlayLabels {
            count: format!("{} / {}", prompt.index + 1, prompt.required().len()),
            empty: match prompt.current_arg() {
                Some(arg) if arg.choices.is_empty() => "type a value and press Enter",
                Some(_) => "no matching choices",
                None => "argument",
            },
            hint: format!("{activate} confirm  ·  {cancel} back"),
        },
        SurfaceKind::Approval(approval) => OverlayLabels {
            count: format!("{} permission(s)", approval.missing.len()),
            empty: "no matching options",
            hint: format!("{activate} choose  ·  {cancel} deny  ·  {prev}/{next} move"),
        },
        SurfaceKind::Settings(_) => OverlayLabels {
            count: format!("{filtered_count} settings"),
            empty: "no matching settings",
            hint: format!("{activate} change  ·  {cancel} close  ·  {prev}/{next} move"),
        },
        SurfaceKind::SettingChoice(_) => OverlayLabels {
            count: format!("{filtered_count} choices"),
            empty: "no matching choices",
            hint: format!("{activate} select  ·  {cancel} back  ·  {prev}/{next} move"),
        },
        SurfaceKind::SettingPrompt(_) => OverlayLabels {
            count: "set value".to_string(),
            empty: "type a value and press Enter",
            hint: format!("{activate} apply  ·  {cancel} back"),
        },
        SurfaceKind::Ex(_) => OverlayLabels {
            count: String::new(),
            empty: "",
            hint: String::new(),
        },
    }
}

/// Renders the floating modal overlay (palette, argument prompt, or approval).
pub fn render_overlay(
    theme: &Theme,
    view: &EditorView,
    entity: Entity<EditorView>,
    items: &[PickerItem],
    font_size: f32,
    caret_width: f32,
) -> gpui::Div {
    let Some(surface) = view
        .surface_top()
        .filter(|surface| surface.placement == SurfacePlacement::Center)
    else {
        return div();
    };
    let overlay = &surface.kind;

    let focus_handle = &view.overlay_focus_handle;
    let scroll_handle = &surface.scroll;
    let input = overlay.input();
    let selected = overlay.selected().unwrap_or(0);
    let prefix = overlay.prefix();

    let filtered = filtered_picker_indices(items, input.value());
    let filtered_count = filtered.len();
    let rows = palette_rows(items, &filtered, shows_palette_headings(input.value()));
    let labels = overlay_labels(overlay, filtered_count, &|command| view.key_hint(command));

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
                .child(prefix)
                .child(render_input_text(theme, input, font_size, caret_width)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.foreground.muted)
                .child(labels.count),
        );

    let rows_element: AnyElement = if filtered.is_empty() {
        div()
            .px_2()
            .py_2()
            .text_color(theme.foreground.muted)
            .child(labels.empty)
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
        let selected_item = filtered.get(selected).copied();

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
            .min(PALETTE_LIST_HEIGHT)))
        .into_any_element()
    };

    let hint = div()
        .px_2()
        .pt_1()
        .text_xs()
        .text_color(theme.foreground.muted)
        .whitespace_nowrap()
        .child(labels.hint);

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
                .focus_trap("overlay-modal-trap", focus_handle),
        )
}
