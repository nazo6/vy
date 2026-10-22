//! Statusline rendering using GPUI.

use gpui::{Div, div, prelude::*, px};
use vy_app::editor::{Mode, ViewState};
use vy_app::statusline::{Statusline, StatuslineAlignment};

use crate::theme::Theme;

/// Maps a semantic color token or hex string to a GPUI RGBA color.
pub fn color_for_token(theme: &Theme, token: Option<&str>) -> gpui::Rgba {
    match token {
        Some("primary" | "info") => theme.statusline.info,
        Some("success") => theme.statusline.success,
        Some("warning") => theme.statusline.warning,
        Some("danger" | "error") => theme.statusline.error,
        Some("secondary" | "muted") => theme.foreground.muted,
        _ => theme.foreground.default,
    }
}

/// Renders the bottom statusline bar.
pub fn render_statusline(
    theme: &Theme,
    statusline: &Statusline,
    active_view: Option<ViewState>,
    document_uri: Option<&str>,
    document_line_count: usize,
    status_message: &str,
    height: f32,
) -> Div {
    if statusline.is_empty() {
        let active_view = active_view.unwrap_or_default();
        let cursor = active_view.cursor();

        let mode_label = match active_view.mode() {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Select => "VISUAL",
        };

        let mode_bg = match active_view.mode() {
            Mode::Normal => theme.statusline.normal,
            Mode::Insert => theme.statusline.insert,
            Mode::Select => theme.statusline.select,
        };

        let uri = document_uri
            .map(|u| u.strip_prefix("file://").unwrap_or(u))
            .unwrap_or("[No Name]");

        let left = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .px_1()
                    .bg(mode_bg)
                    .text_color(theme.statusline.foreground)
                    .child(mode_label),
            )
            .child(
                div()
                    .text_color(theme.statusline.uri)
                    .child(uri.to_string()),
            )
            .children(if !status_message.is_empty() {
                Some(
                    div()
                        .text_color(theme.statusline.position)
                        .child(format!(" | {status_message}")),
                )
            } else {
                None
            });

        let doc_line_count = document_line_count.max(1) as u32;
        let progress = if cursor.line == 0 {
            "Top".to_string()
        } else if cursor.line + 1 >= doc_line_count {
            "Bot".to_string()
        } else {
            let pct = ((cursor.line + 1) as f32 / doc_line_count as f32 * 100.0) as u32;
            format!("{pct}%")
        };

        let right = div()
            .flex()
            .items_center()
            .gap_3()
            .text_color(theme.statusline.message)
            .child(format!("{}:{}", cursor.line + 1, cursor.character + 1))
            .child(progress);

        div()
            .flex()
            .justify_between()
            .items_center()
            .h(px(height))
            .px_2()
            .bg(theme.statusline.background)
            .text_xs()
            .child(left)
            .child(right)
    } else {
        let left_items: Vec<Div> = statusline
            .items_for(StatuslineAlignment::Left)
            .into_iter()
            .map(|item| {
                div()
                    .text_color(color_for_token(theme, item.color.as_deref()))
                    .child(item.text.clone())
            })
            .collect();

        let center_items: Vec<Div> = statusline
            .items_for(StatuslineAlignment::Center)
            .into_iter()
            .map(|item| {
                div()
                    .text_color(color_for_token(theme, item.color.as_deref()))
                    .child(item.text.clone())
            })
            .collect();

        let right_items: Vec<Div> = statusline
            .items_for(StatuslineAlignment::Right)
            .into_iter()
            .map(|item| {
                div()
                    .text_color(color_for_token(theme, item.color.as_deref()))
                    .child(item.text.clone())
            })
            .collect();

        div()
            .flex()
            .justify_between()
            .items_center()
            .h(px(height))
            .px_2()
            .bg(theme.statusline.background)
            .text_xs()
            .child(div().flex().items_center().gap_2().children(left_items))
            .child(div().flex().items_center().gap_2().children(center_items))
            .child(div().flex().items_center().gap_2().children(right_items))
    }
}
