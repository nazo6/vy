//! Statusline rendering using GPUI.

use gpui::{Div, div, prelude::*, px};
use vy_app::editor::{Mode, ViewState};
use vy_app::statusline::{Statusline, StatuslineAlignment};

use crate::theme::Theme;
use crate::ui::color::color;

/// Truncates a file path or URI to at most `max_len` characters using an ellipsis (`...`).
///
/// Preserves the file name and trailing directory components when possible, e.g.:
/// `/very/long/path/to/project/src/main.rs` -> `.../src/main.rs`.
pub fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    if max_len <= 3 {
        return "...".chars().take(max_len).collect();
    }

    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return path.to_string();
    }

    let filename = parts.last().unwrap();
    if filename.len() + 4 > max_len {
        let keep = max_len.saturating_sub(3);
        let start_idx = filename
            .char_indices()
            .map(|(i, _)| i)
            .nth(filename.chars().count().saturating_sub(keep))
            .unwrap_or(0);
        return format!("...{}", &filename[start_idx..]);
    }

    let mut chosen_parts = Vec::new();
    let mut current_len = 3;
    for part in parts.iter().rev() {
        let added_len = 1 + part.len();
        if current_len + added_len <= max_len {
            chosen_parts.push(*part);
            current_len += added_len;
        } else {
            break;
        }
    }

    chosen_parts.reverse();
    format!(".../{}", chosen_parts.join("/"))
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
            Mode::SelectLine => "VISUAL LINE",
            Mode::SelectBlock => "VISUAL BLOCK",
        };

        let mode_bg = match active_view.mode() {
            Mode::Normal => theme.statusline.normal,
            Mode::Insert => theme.statusline.insert,
            Mode::Select | Mode::SelectLine | Mode::SelectBlock => theme.statusline.select,
        };

        let raw_uri = document_uri
            .map(|u| u.strip_prefix("file://").unwrap_or(u))
            .unwrap_or("[No Name]");
        let uri = truncate_path(raw_uri, 50);

        let left = div()
            .flex()
            .items_center()
            .overflow_hidden()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .px_1()
                    .bg(mode_bg)
                    .text_color(theme.statusline.foreground)
                    .child(mode_label),
            )
            .child(div().text_color(theme.statusline.uri).child(uri))
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
            .flex_shrink_0()
            .gap_3()
            .text_color(theme.statusline.message)
            .child(format!("{}:{}", cursor.line + 1, cursor.character + 1))
            .child(progress);

        div()
            .flex()
            .justify_between()
            .items_center()
            .h(px(height))
            .line_height(px(height))
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
                    .text_color(
                        item.color
                            .map_or(theme.foreground.default, |token| color(theme, token)),
                    )
                    .child(item.text.clone())
            })
            .collect();

        let center_items: Vec<Div> = statusline
            .items_for(StatuslineAlignment::Center)
            .into_iter()
            .map(|item| {
                div()
                    .text_color(
                        item.color
                            .map_or(theme.foreground.default, |token| color(theme, token)),
                    )
                    .child(item.text.clone())
            })
            .collect();

        let right_items: Vec<Div> = statusline
            .items_for(StatuslineAlignment::Right)
            .into_iter()
            .map(|item| {
                div()
                    .text_color(
                        item.color
                            .map_or(theme.foreground.default, |token| color(theme, token)),
                    )
                    .child(item.text.clone())
            })
            .collect();

        div()
            .flex()
            .justify_between()
            .items_center()
            .h(px(height))
            .line_height(px(height))
            .px_2()
            .bg(theme.statusline.background)
            .text_xs()
            .child(div().flex().items_center().gap_2().children(left_items))
            .child(div().flex().items_center().gap_2().children(center_items))
            .child(div().flex().items_center().gap_2().children(right_items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_paths() {
        assert_eq!(truncate_path("/short/path.rs", 20), "/short/path.rs");
        assert_eq!(
            truncate_path("/a/b/c/d/e/f/g/h/file.rs", 15),
            ".../g/h/file.rs"
        );
        let res = truncate_path(
            "/home/user/workspace/src/deep/module/very_long_file_name.rs",
            25,
        );
        assert!(res.len() <= 25);
        assert!(res.starts_with("..."));
    }
}
