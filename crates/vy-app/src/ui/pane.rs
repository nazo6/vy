//! Editor pane and line rendering.

use gpui::{
    Div, HighlightStyle, Pixels, SharedString, StyledText, TextRun, Window, div, prelude::*, px,
};
use vy_app::config::{Config, LineNumbers};
use vy_app::editor::Mode;
use vy_app::slots::GutterRegistry;
use vy_app::state::{DocumentState, PaneState};
use vy_app::text_layout::{
    chunk_line, cursor_in_chunk, selection_in_chunk, selection_on_line, utf16_to_byte,
};

use crate::theme::Theme;
use crate::ui::color::color;

/// Width of one character at default font size (14pt). For standard monospace fonts,
/// character width is approximately 0.6 of font size (14 * 0.6 = 8.4px).
pub const CHARACTER_WIDTH: f32 = 8.4;
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

/// Measures the width of a single monospace character using the font system.
pub fn font_character_width(config: &Config, window: &mut Window) -> f32 {
    let run = TextRun {
        len: 1,
        font: gpui::font(config.font_family()),
        color: gpui::rgba(0x00000000).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(
        SharedString::from("0"),
        px(config.font_size()),
        &[run],
        None,
    );
    let w = f32::from(shaped.width);
    if w > 0.0 { w } else { character_width(config) }
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
#[allow(dead_code)]
pub fn insert_marker(
    pane: &PaneState,
    text: &str,
) -> (String, usize, Option<std::ops::Range<usize>>) {
    insert_marker_at(&pane.preedit, pane.view.cursor().character, text)
}

/// Returns the text to draw on an Insert cursor line/chunk with an explicit character offset.
pub fn insert_marker_at(
    preedit: &str,
    char_offset: u32,
    text: &str,
) -> (String, usize, Option<std::ops::Range<usize>>) {
    let mut display = text.to_string();
    let caret = utf16_to_byte(&display, char_offset);

    if preedit.is_empty() {
        return (display, caret, None);
    }

    display.insert_str(caret, preedit);
    let end = caret + preedit.len();
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
#[allow(dead_code)]
pub fn cursor_marker(pane: &PaneState, text: &str) -> (String, std::ops::Range<usize>) {
    let length = text.chars().map(|c| c.len_utf16() as u32).sum();
    let selection = selection_on_line(pane.view.selection(), pane.view.cursor().line, length);
    cursor_marker_at(selection, pane.view.cursor().character, text)
}

/// Returns the text to draw on a cursor line/chunk with an explicit character offset.
pub fn cursor_marker_at(
    selection_range: Option<(u32, u32)>,
    char_offset: u32,
    text: &str,
) -> (String, std::ops::Range<usize>) {
    let mut display = text.to_string();

    let (from, to) = match selection_range {
        Some((from, to)) => (from, to),
        None => {
            let from = char_offset;
            let to = char_offset + 1;
            (from, to)
        }
    };

    let from_byte = utf16_to_byte(&display, from);
    let mut to_byte = utf16_to_byte(&display, to);

    if to_byte <= from_byte {
        display.push(' ');
        to_byte = display.len();
    }

    (display, from_byte..to_byte)
}

/// Renders a single row of text or a chunk of a wrapped line in an editor pane.
#[allow(clippy::too_many_arguments)]
pub fn render_line(
    config: &Config,
    theme: &Theme,
    pane: &PaneState,
    document: &DocumentState,
    gutter_registry: &GutterRegistry,
    line_number: usize,
    chunk_index: usize,
    total_chunks: usize,
    chunk_text: &str,
    utf16_range: &std::ops::Range<usize>,
    line_length: u32,
    scroll_left_px: f32,
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

    let is_first_chunk = chunk_index == 0;
    let marks = if is_first_chunk {
        gutter_registry.items_for_line(line_number)
    } else {
        Vec::new()
    };

    let mark_text = if let Some(first) = marks.first() {
        first.text.clone()
    } else {
        " ".to_string()
    };

    let mark_color = if let Some(first) = marks.first() {
        first
            .color
            .map_or(theme.editor.gutter, |token| color(theme, token))
    } else {
        theme.editor.gutter
    };

    let line_count = document.editor.line_count();
    let digits = line_number_width(line_count);
    let num_label = if is_first_chunk {
        config.line_numbers().label(line_number, cursor_line)
    } else {
        None
    };

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
                    format!("{:>digits$} ", "")
                },
            )),
    );

    let cursor_chunk_col = cursor_in_chunk(
        pane.view.cursor(),
        line_number,
        chunk_index,
        total_chunks,
        utf16_range,
    );

    let selection_range = match pane.view.mode() {
        Mode::SelectBlock => {
            let first_line = pane.view.anchor().line.min(pane.view.head().line);
            let last_line = pane.view.anchor().line.max(pane.view.head().line);
            if (line_number as u32) >= first_line && (line_number as u32) <= last_line {
                let first_col = pane.view.anchor().character.min(pane.view.head().character);
                let last_col = pane.view.anchor().character.max(pane.view.head().character);
                let from = first_col.min(line_length);
                let to = (last_col + 1).min(line_length);
                let from_chunk = (from as usize).max(utf16_range.start);
                let to_chunk = (to as usize).min(utf16_range.end);
                if from_chunk < to_chunk {
                    Some((
                        (from_chunk - utf16_range.start) as u32,
                        (to_chunk - utf16_range.start) as u32,
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => selection_in_chunk(
            pane.view.selection(),
            line_number,
            chunk_index,
            total_chunks,
            utf16_range,
            line_length,
        ),
    };

    let text_area = div()
        .relative()
        .flex_1()
        .h(px(lh))
        .overflow_hidden()
        .whitespace_nowrap();

    let mut text_container = div()
        .absolute()
        .top_0()
        .left(-px(scroll_left_px))
        .h(px(lh))
        .whitespace_nowrap();

    if pane.view.mode() == Mode::Insert {
        if let Some(col) = cursor_chunk_col {
            let (display, caret, preedit) = insert_marker_at(&pane.preedit, col, chunk_text);
            let width = text_width(config, theme, &display, caret, window);

            let mut line = StyledText::new(display);
            if let Some(range) = preedit {
                line = line.with_highlights(vec![(range, cursor_highlight(theme))]);
            }

            let caret_el = div()
                .absolute()
                .top_0()
                .left(width)
                .w(px(CARET_WIDTH))
                .h(px(lh))
                .bg(theme.editor.caret);

            text_container = text_container.child(line).child(caret_el);
        } else {
            text_container = text_container.child(chunk_text.to_string());
        }
        return row.child(text_area.child(text_container));
    }

    if pane.view.mode() == Mode::Normal {
        if let Some(col) = cursor_chunk_col {
            let (display, highlight) = cursor_marker_at(None, col, chunk_text);
            let line = StyledText::new(display)
                .with_highlights(vec![(highlight, cursor_highlight(theme))]);
            text_container = text_container.child(line);
        } else {
            text_container = text_container.child(chunk_text.to_string());
        }
        return row.child(text_area.child(text_container));
    }

    // Visual modes: Select, SelectLine, SelectBlock
    if let Some((from, to)) = selection_range {
        let mut display = chunk_text.to_string();
        let from_byte = utf16_to_byte(&display, from);
        let mut to_byte = utf16_to_byte(&display, to);

        if to_byte <= from_byte {
            display.push(' ');
            to_byte = display.len();
        }

        let line = StyledText::new(display)
            .with_highlights(vec![(from_byte..to_byte, cursor_highlight(theme))]);
        text_container = text_container.child(line);
    } else if let Some(col) = cursor_chunk_col {
        let (display, highlight) = cursor_marker_at(None, col, chunk_text);
        let line =
            StyledText::new(display).with_highlights(vec![(highlight, cursor_highlight(theme))]);
        text_container = text_container.child(line);
    } else {
        text_container = text_container.child(chunk_text.to_string());
    }

    row.child(text_area.child(text_container))
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
    let wrap = pane.is_wrap(config);
    let line_count = document.editor.line_count();
    let char_w = font_character_width(config, window);

    if !wrap {
        let start = pane.scroll_top;
        let end = (start + pane.viewport_rows.max(1)).min(line_count);
        let scroll_left_px = pane.scroll_left as f32 * char_w;

        return (start..end)
            .map(|line| {
                let text = document.editor.line(line).unwrap_or_default();
                let len = text.chars().map(|c| c.len_utf16() as u32).sum();
                render_line(
                    config,
                    theme,
                    pane,
                    document,
                    gutter_registry,
                    line,
                    0,
                    1,
                    &text,
                    &(0..len as usize),
                    len,
                    scroll_left_px,
                    window,
                )
            })
            .collect();
    }

    // Wrap mode: breaks lines exceeding viewport_cols into chunks
    let mut visual_lines = Vec::with_capacity(pane.viewport_rows);
    let mut line_idx = pane.scroll_top;
    let mut chunk_idx = pane.scroll_sub_line;
    let cols = pane.viewport_cols.max(1);

    while line_idx < line_count && visual_lines.len() < pane.viewport_rows {
        let text = document.editor.line(line_idx).unwrap_or_default();
        let chunks = chunk_line(&text, cols);
        let total_chunks = chunks.len();
        let line_len = text.chars().map(|c| c.len_utf16() as u32).sum();

        while chunk_idx < total_chunks && visual_lines.len() < pane.viewport_rows {
            let chunk = &chunks[chunk_idx];
            let chunk_text = &text[chunk.byte_range.clone()];
            visual_lines.push(render_line(
                config,
                theme,
                pane,
                document,
                gutter_registry,
                line_idx,
                chunk_idx,
                total_chunks,
                chunk_text,
                &chunk.utf16_range,
                line_len,
                0.0,
                window,
            ));
            chunk_idx += 1;
        }

        line_idx += 1;
        chunk_idx = 0;
    }

    visual_lines
}
