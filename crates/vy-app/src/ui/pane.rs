//! Editor pane and line rendering.

use gpui::{
    Div, HighlightStyle, Pixels, SharedString, StyledText, TextRun, Window, div, prelude::*, px,
};
use vy_app::config::{Config, LineNumbers};
use vy_app::editor::Mode;
use vy_app::slots::GutterRegistry;
use vy_app::state::{DocumentState, PaneState};
use vy_app::text_layout::{selection_on_line, utf16_to_byte};

use crate::theme::Theme;

/// Width of one character at default font size. Used to scale the gutter.
pub const CHARACTER_WIDTH: f32 = 10.0;
/// Width of the vertical insert caret bar in pixels.
pub const CARET_WIDTH: f32 = 2.0;

/// The background style used for the selection and for active IME composition text.
pub fn cursor_highlight(theme: &Theme) -> HighlightStyle {
    HighlightStyle {
        background_color: Some(theme.editor.selection.into()),
        ..Default::default()
    }
}

/// Computes the number of digits needed for line numbers (at least 4).
pub fn line_number_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len().max(4)
}

/// Height of one text line from the configuration.
pub fn line_height(config: &Config) -> f32 {
    config.line_height()
}

/// Approximate width of one character at the configured font size.
pub fn character_width(config: &Config) -> f32 {
    CHARACTER_WIDTH * config.font_size() / 14.0
}

/// Width of the line number gutter in pixels, or 0 when line numbers are off.
pub fn gutter_width(config: &Config, line_count: usize) -> f32 {
    let base = if config.line_numbers() == LineNumbers::Off {
        0.0
    } else {
        let width = character_width(config);
        let digits = line_number_width(line_count);
        digits as f32 * width + width * 1.2
    };
    // Reserve an extra 1.5 characters width for gutter marks (git diff, breakpoints, diagnostics)
    base + character_width(config) * 1.5
}

/// Returns the text to draw on the Insert cursor line, the caret's byte offset,
/// and the preedit byte range.
pub fn insert_marker(
    pane: &PaneState,
    text: &str,
) -> (String, usize, Option<std::ops::Range<usize>>) {
    let mut display = text.to_string();
    let caret = utf16_to_byte(&display, pane.view.cursor().character);

    if pane.preedit.is_empty() {
        return (display, caret, None);
    }

    display.insert_str(caret, &pane.preedit);
    let end = caret + pane.preedit.len();
    (display, end, Some(caret..end))
}

/// Measures the pixel width from the start of a shaped line up to byte offset `index`.
pub fn text_width(
    config: &Config,
    theme: &Theme,
    text: &str,
    index: usize,
    window: &mut Window,
) -> Pixels {
    if index == 0 {
        return px(0.);
    }

    let run = TextRun {
        len: text.len(),
        font: gpui::font(config.font_family()),
        color: theme.editor.text.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(
        SharedString::from(text.to_string()),
        px(config.font_size()),
        &[run],
        None,
    );
    shaped.x_for_index(index)
}

/// Returns the text to draw on the cursor line and the byte range to mark for Normal/Visual.
pub fn cursor_marker(pane: &PaneState, text: &str) -> (String, std::ops::Range<usize>) {
    let mut display = text.to_string();

    let length = display
        .chars()
        .map(|character| character.len_utf16() as u32)
        .sum();
    let Some((from, to)) =
        selection_on_line(pane.view.selection(), pane.view.cursor().line, length)
    else {
        display.push(' ');
        let start = display.len() - 1;
        let end = display.len();
        return (display, start..end);
    };

    let from = utf16_to_byte(&display, from);
    let mut to = utf16_to_byte(&display, to);

    if to <= from {
        display.push(' ');
        to = display.len();
    }

    (display, from..to)
}

/// Renders a single row of text in an editor pane.
pub fn render_line(
    config: &Config,
    theme: &Theme,
    pane: &PaneState,
    document: &DocumentState,
    gutter_registry: &GutterRegistry,
    line_number: usize,
    window: &mut Window,
) -> Div {
    let cursor_line = pane.view.cursor().line as usize;
    let lh = line_height(config);
    let mut row = div()
        .relative()
        .flex()
        .flex_shrink_0()
        .h(px(lh))
        .line_height(px(lh))
        .items_center();

    let marks = gutter_registry.items_for_line(line_number);
    let mark_text = if let Some(first) = marks.first() {
        first.text.clone()
    } else {
        " ".to_string()
    };

    let mark_color = if let Some(first) = marks.first() {
        match first.color.as_deref() {
            Some("error" | "danger") => theme.statusline.error,
            Some("warning") => theme.statusline.warning,
            Some("info" | "primary") => theme.statusline.info,
            Some("success") => theme.statusline.success,
            _ => theme.editor.gutter,
        }
    } else {
        theme.editor.gutter
    };

    let line_count = document.editor.line_count();
    let digits = line_number_width(line_count);
    let num_label = config.line_numbers().label(line_number, cursor_line);

    row = row.child(
        div()
            .flex()
            .items_center()
            .h(px(lh))
            .line_height(px(lh))
            .w(px(gutter_width(config, line_count)))
            .flex_shrink_0()
            .child(
                div()
                    .w(px(character_width(config) * 1.2))
                    .text_color(mark_color)
                    .child(mark_text),
            )
            .child(div().flex_1().text_color(theme.editor.gutter).child(
                if let Some(label) = num_label {
                    format!("{label:>digits$} ")
                } else {
                    "".to_string()
                },
            )),
    );

    let text = document.editor.line(line_number).unwrap_or_default();
    let cursor = pane.view.cursor();
    let is_cursor_line = line_number as u32 == cursor.line;

    if pane.view.mode() == Mode::Insert {
        if !is_cursor_line {
            return row.child(text);
        }

        let (display, caret, preedit) = insert_marker(pane, &text);
        let width = text_width(config, theme, &display, caret, window);
        let gutter = gutter_width(config, document.editor.line_count());

        let mut line = StyledText::new(display);
        if let Some(range) = preedit {
            line = line.with_highlights(vec![(range, cursor_highlight(theme))]);
        }

        return row.child(line).child(
            div()
                .absolute()
                .top_0()
                .left(px(gutter + f32::from(width)))
                .w(px(CARET_WIDTH))
                .h(px(lh))
                .bg(theme.editor.caret),
        );
    }

    if pane.view.mode() == Mode::Normal {
        if !is_cursor_line {
            return row.child(text);
        }
        let (display, highlight) = cursor_marker(pane, &text);
        return row.child(
            StyledText::new(display).with_highlights(vec![(highlight, cursor_highlight(theme))]),
        );
    }

    // Visual modes: Select, SelectLine, SelectBlock
    let length = text
        .chars()
        .map(|character| character.len_utf16() as u32)
        .sum();

    let selection_range = match pane.view.mode() {
        Mode::SelectBlock => {
            let first_line = pane.view.anchor().line.min(pane.view.head().line);
            let last_line = pane.view.anchor().line.max(pane.view.head().line);
            if (line_number as u32) >= first_line && (line_number as u32) <= last_line {
                let first_col = pane.view.anchor().character.min(pane.view.head().character);
                let last_col = pane.view.anchor().character.max(pane.view.head().character);
                let from = first_col.min(length);
                let to = (last_col + 1).min(length);
                Some((from, to))
            } else {
                None
            }
        }
        _ => selection_on_line(pane.view.selection(), line_number as u32, length),
    };

    let Some((from, to)) = selection_range else {
        if is_cursor_line {
            let (display, highlight) = cursor_marker(pane, &text);
            return row.child(
                StyledText::new(display)
                    .with_highlights(vec![(highlight, cursor_highlight(theme))]),
            );
        }
        return row.child(text);
    };

    let mut display = text.to_string();
    let from_byte = utf16_to_byte(&display, from);
    let mut to_byte = utf16_to_byte(&display, to);

    if to_byte <= from_byte {
        display.push(' ');
        to_byte = display.len();
    }

    row.child(
        StyledText::new(display)
            .with_highlights(vec![(from_byte..to_byte, cursor_highlight(theme))]),
    )
}

/// Collects all visible lines in one pane.
pub fn visible_lines(
    config: &Config,
    theme: &Theme,
    pane: &PaneState,
    document: &DocumentState,
    gutter_registry: &GutterRegistry,
    window: &mut Window,
) -> Vec<Div> {
    let start = pane.scroll_top;
    let end = (start + pane.viewport_rows.max(1)).min(document.editor.line_count());

    (start..end)
        .map(|line| render_line(config, theme, pane, document, gutter_registry, line, window))
        .collect()
}
