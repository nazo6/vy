//! Editor pane and line rendering.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use gpui::{
    App, BorderStyle, Bounds, ContentMask, Pixels, SharedString, TextAlign, TextRun,
    UnderlineStyle, Window, fill, outline, point, px, size,
};
use vy_app::config::{Config, LineNumbers};
use vy_app::editor::Mode;
use vy_app::slots::GutterRegistry;
use vy_app::state::{DocumentState, PaneState};
use vy_app::text_layout::{cursor_in_chunk, selection_in_chunk, utf16_to_byte};

use crate::theme::Theme;
use crate::ui::color::color;

static CHAR_WIDTH_CACHE: LazyLock<Mutex<HashMap<(String, u32), f32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Width of one character at default font size (14pt). For standard monospace fonts,
/// character width is approximately 0.6 of font size (14 * 0.6 = 8.4px).
pub const CHARACTER_WIDTH: f32 = 8.4;
/// Width of the vertical insert caret bar in pixels.
pub const CARET_WIDTH: f32 = 2.0;

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

/// Measures the width of a single monospace character using the font system with caching.
pub fn font_character_width(config: &Config, window: &mut Window) -> f32 {
    let family = config.font_family();
    let size = config.font_size();
    let key = (family.clone(), size.to_bits());

    if let Ok(cache) = CHAR_WIDTH_CACHE.lock()
        && let Some(&w) = cache.get(&key)
    {
        return w;
    }

    let run = TextRun {
        len: 1,
        font: gpui::font(family),
        color: gpui::rgba(0x00000000).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(SharedString::from("0"), px(size), &[run], None);
    let w = f32::from(shaped.width);
    let width = if w > 0.0 { w } else { character_width(config) };

    if let Ok(mut cache) = CHAR_WIDTH_CACHE.lock() {
        cache.insert(key, width);
    }

    width
}

/// Width of the line number gutter using a measured character width.
pub fn gutter_width(config: &Config, line_count: usize, char_w: f32) -> f32 {
    let base = if config.line_numbers() == LineNumbers::Off {
        0.0
    } else {
        let digits = line_number_width(line_count);
        digits as f32 * char_w + char_w * 1.2
    };
    // Reserve 2.0 characters width for gutter marks (allowing multiple marks side-by-side)
    base + char_w * 2.0
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

pub struct VisualRow {
    pub line_number: usize,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub text: String,
    pub utf16_range: std::ops::Range<usize>,
    pub line_length: u32,
    pub y: Pixels,
}

pub struct EditorPaintData {
    pub config: Arc<Config>,
    pub theme: Arc<Theme>,
    pub line_count: usize,
    pub view: vy_app::editor::ViewState,
    /// Selection range derived from the view and the document text.
    pub selection: vy_core::Range,
    pub preedit: String,
    pub scroll_left: usize,
    pub wrap: bool,
    pub visual_rows: Vec<VisualRow>,
    pub gutter_marks: Vec<(usize, Vec<vy_app::slots::GutterItem>)>,
    pub cursor_visible: bool,
}

pub fn prepare_editor_paint(
    config: Arc<Config>,
    theme: Arc<Theme>,
    pane: &PaneState,
    document: &DocumentState,
    gutter_registry: &GutterRegistry,
    bounds: Bounds<Pixels>,
    cursor_visible: bool,
) -> EditorPaintData {
    let visual_rows = collect_visual_rows(&config, pane, document, bounds);
    let mut gutter_marks = Vec::new();
    for row in &visual_rows {
        if row.chunk_index == 0 {
            let items = gutter_registry.items_for_line(row.line_number);
            if !items.is_empty() {
                gutter_marks.push((row.line_number, items.into_iter().cloned().collect()));
            }
        }
    }

    let wrap = pane.is_wrap(&config);
    let view = pane.view;
    let selection = document
        .editor
        .range_for(view.mode(), view.anchor(), view.head());
    EditorPaintData {
        config,
        theme,
        line_count: document.editor.line_count(),
        view,
        selection,
        preedit: pane.preedit.clone(),
        scroll_left: pane.scroll_left,
        wrap,
        visual_rows,
        gutter_marks,
        cursor_visible,
    }
}

fn collect_visual_rows(
    config: &Config,
    pane: &PaneState,
    document: &DocumentState,
    bounds: Bounds<Pixels>,
) -> Vec<VisualRow> {
    let line_count = document.editor.line_count();
    let lh = line_height(config);
    let wrap = pane.is_wrap(config);
    let mut visual_rows = Vec::with_capacity(pane.viewport_rows.max(1));

    if !wrap {
        let start = pane.scroll_top;
        let end = (start + pane.viewport_rows.max(1) + 1).min(line_count);
        for (row_idx, line_idx) in (start..end).enumerate() {
            let row_y = bounds.top() + px(row_idx as f32 * lh);
            if row_y >= bounds.bottom() {
                break;
            }
            let text = document.editor.line(line_idx).unwrap_or_default();
            let len = text.encode_utf16().count() as u32;
            visual_rows.push(VisualRow {
                line_number: line_idx,
                chunk_index: 0,
                total_chunks: 1,
                text,
                utf16_range: 0..len as usize,
                line_length: len,
                y: row_y,
            });
        }
    } else {
        let cols = pane.viewport_cols.max(1);
        let index = document.editor.visual_index_with(Some(cols));
        let mut line_idx = pane.scroll_top;
        let mut chunk_idx = pane.scroll_sub_line;
        let mut row_idx = 0;

        while line_idx < line_count && row_idx < pane.viewport_rows {
            let text = document.editor.line(line_idx).unwrap_or_default();
            let line_chunks = index.chunks(line_idx as u32);
            let chunks = &line_chunks.chunks;
            let total_chunks = chunks.len();
            let line_len = line_chunks.utf16_length;

            while chunk_idx < total_chunks && row_idx < pane.viewport_rows {
                let row_y = bounds.top() + px(row_idx as f32 * lh);
                if row_y >= bounds.bottom() {
                    break;
                }
                let chunk = &chunks[chunk_idx];
                let chunk_text = text[chunk.byte_range.clone()].to_string();
                visual_rows.push(VisualRow {
                    line_number: line_idx,
                    chunk_index: chunk_idx,
                    total_chunks,
                    text: chunk_text,
                    utf16_range: chunk.utf16_range.clone(),
                    line_length: line_len,
                    y: row_y,
                });
                chunk_idx += 1;
                row_idx += 1;
            }
            line_idx += 1;
            chunk_idx = 0;
        }
    }

    visual_rows
}

/// Paints the editor pane using GPUI's low-level paint primitives.
pub fn paint_editor(
    data: EditorPaintData,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let config = &data.config;
    let theme = &data.theme;
    let line_count = data.line_count;
    let lh = px(line_height(config));
    let char_w = font_character_width(config, window);
    let gutter_w = px(gutter_width(config, line_count, char_w));

    let content_origin_x = bounds.left() + gutter_w;
    let content_w = (bounds.size.width - gutter_w).max(px(0.0));
    let content_bounds = Bounds::new(
        point(content_origin_x, bounds.top()),
        size(content_w, bounds.size.height),
    );

    // 1. Paint pane background
    window.paint_quad(fill(bounds, theme.editor.background));

    // 2. Paint gutter separator if gutter exists
    if gutter_w > px(0.0) {
        let sep_bounds = Bounds::new(
            point(content_origin_x - px(1.0), bounds.top()),
            size(px(1.0), bounds.size.height),
        );
        window.paint_quad(fill(sep_bounds, theme.border.subtle));
    }

    // 3. Paint gutter marks and line numbers
    let cursor_line = data.view.cursor().line as usize;
    if gutter_w > px(0.0) {
        let digits = line_number_width(line_count);
        let font = gpui::font(config.font_family());
        let font_size = px(config.font_size());

        for row in &data.visual_rows {
            if row.chunk_index == 0 {
                if let Some((_, marks)) = data
                    .gutter_marks
                    .iter()
                    .find(|(line, _)| *line == row.line_number)
                {
                    let mut mark_x = bounds.left() + px(char_w * 0.2);
                    for mark in marks.iter().take(2) {
                        let mark_color =
                            mark.color.map_or(theme.editor.gutter, |c| color(theme, c));
                        let mark_run = TextRun {
                            len: mark.text.len(),
                            font: font.clone(),
                            color: mark_color.into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let shaped = window.text_system().shape_line(
                            SharedString::from(mark.text.clone()),
                            font_size,
                            &[mark_run],
                            None,
                        );
                        let _ = shaped.paint(
                            point(mark_x, row.y),
                            lh,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                        mark_x += px(char_w * 0.9);
                    }
                }

                if let Some(label) = config.line_numbers().label(row.line_number, cursor_line) {
                    let num_text = format!("{label:>digits$} ");
                    let num_run = TextRun {
                        len: num_text.len(),
                        font: font.clone(),
                        color: theme.editor.gutter.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped = window.text_system().shape_line(
                        SharedString::from(num_text),
                        font_size,
                        &[num_run],
                        None,
                    );
                    let num_x = bounds.left() + px(char_w * 2.0);
                    let _ =
                        shaped.paint(point(num_x, row.y), lh, TextAlign::Left, None, window, cx);
                }
            }
        }
    }

    // 4. Paint editor text and cursor in clipped content bounds
    window.with_content_mask(
        Some(ContentMask {
            bounds: content_bounds,
        }),
        |window| {
            let scroll_left_px = if data.wrap {
                0.0
            } else {
                data.scroll_left as f32 * char_w
            };
            let text_origin_x = content_origin_x - px(scroll_left_px);
            let font = gpui::font(config.font_family());
            let font_size = px(config.font_size());

            for row in &data.visual_rows {
                // Paint current line highlight
                if row.line_number == cursor_line {
                    let line_quad = fill(
                        Bounds::new(point(content_origin_x, row.y), size(content_w, lh)),
                        theme.editor.line_highlight,
                    );
                    window.paint_quad(line_quad);
                }

                let cursor_chunk_col = cursor_in_chunk(
                    data.view.cursor(),
                    row.line_number,
                    row.chunk_index,
                    row.total_chunks,
                    &row.utf16_range,
                );

                let selection_range = match data.view.mode() {
                    Mode::SelectBlock => {
                        let first_line = data.view.anchor().line.min(data.view.head().line);
                        let last_line = data.view.anchor().line.max(data.view.head().line);
                        if (row.line_number as u32) >= first_line
                            && (row.line_number as u32) <= last_line
                        {
                            let first_col =
                                data.view.anchor().character.min(data.view.head().character);
                            let last_col =
                                data.view.anchor().character.max(data.view.head().character);
                            let from = first_col.min(row.line_length);
                            let to = (last_col + 1).min(row.line_length);
                            let from_chunk = (from as usize).max(row.utf16_range.start);
                            let to_chunk = (to as usize).min(row.utf16_range.end);
                            if from_chunk < to_chunk {
                                Some((
                                    (from_chunk - row.utf16_range.start) as u32,
                                    (to_chunk - row.utf16_range.start) as u32,
                                ))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => selection_in_chunk(
                        data.selection,
                        row.line_number,
                        row.chunk_index,
                        row.total_chunks,
                        &row.utf16_range,
                        row.line_length,
                    ),
                };

                // Prepare text and preedit insertion if in Insert mode
                let (display_text, caret_byte, preedit_byte_range) =
                    if data.view.mode() == Mode::Insert {
                        if let Some(col) = cursor_chunk_col {
                            let (d, cb, pb) = insert_marker_at(&data.preedit, col, &row.text);
                            (d, Some(cb), pb)
                        } else {
                            (row.text.clone(), None, None)
                        }
                    } else {
                        (row.text.clone(), None, None)
                    };

                // Shape line text
                let run = TextRun {
                    len: display_text.len(),
                    font: font.clone(),
                    color: theme.editor.text.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(
                    SharedString::from(display_text.clone()),
                    font_size,
                    &[run],
                    None,
                );

                // Paint selection highlight
                if let Some((from_u16, to_u16)) = selection_range {
                    let from_b = utf16_to_byte(&display_text, from_u16);
                    let to_b = utf16_to_byte(&display_text, to_u16);
                    let start_x = text_origin_x + shaped.x_for_index(from_b);
                    let end_x = text_origin_x + shaped.x_for_index(to_b);
                    let sel_w = if end_x <= start_x {
                        px(char_w)
                    } else {
                        end_x - start_x
                    };
                    let sel_quad = fill(
                        Bounds::new(point(start_x, row.y), size(sel_w, lh)),
                        theme.editor.selection,
                    );
                    window.paint_quad(sel_quad);
                }

                // Paint block cursor highlight for Normal mode
                if data.cursor_visible
                    && data.view.mode() == Mode::Normal
                    && let Some(col) = cursor_chunk_col
                {
                    let from_b = utf16_to_byte(&display_text, col);
                    let to_b = utf16_to_byte(&display_text, col + 1);
                    let start_x = text_origin_x + shaped.x_for_index(from_b);
                    let end_x = text_origin_x + shaped.x_for_index(to_b);
                    let cur_w = if end_x <= start_x {
                        px(char_w)
                    } else {
                        end_x - start_x
                    };
                    let cursor_quad = fill(
                        Bounds::new(point(start_x, row.y), size(cur_w, lh)),
                        theme.editor.selection,
                    );
                    window.paint_quad(cursor_quad);
                }

                // Paint cursor head indicator for Select modes
                if data.cursor_visible
                    && matches!(
                        data.view.mode(),
                        Mode::Select | Mode::SelectLine | Mode::SelectBlock
                    )
                    && let Some(col) = cursor_chunk_col
                {
                    let from_b = utf16_to_byte(&display_text, col);
                    let to_b = utf16_to_byte(&display_text, col + 1);
                    let start_x = text_origin_x + shaped.x_for_index(from_b);
                    let end_x = text_origin_x + shaped.x_for_index(to_b);
                    let cur_w = if end_x <= start_x {
                        px(char_w)
                    } else {
                        end_x - start_x
                    };
                    let cursor_quad = outline(
                        Bounds::new(point(start_x, row.y), size(cur_w, lh)),
                        theme.editor.caret,
                        BorderStyle::Solid,
                    );
                    window.paint_quad(cursor_quad);
                }

                // Paint text
                let _ = shaped.paint(
                    point(text_origin_x, row.y),
                    lh,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );

                // Paint Insert mode vertical caret bar
                if data.cursor_visible
                    && data.view.mode() == Mode::Insert
                    && let Some(caret_b) = caret_byte
                {
                    let caret_x = text_origin_x + shaped.x_for_index(caret_b);
                    let caret_quad = fill(
                        Bounds::new(point(caret_x, row.y), size(px(CARET_WIDTH), lh)),
                        theme.editor.caret,
                    );
                    window.paint_quad(caret_quad);
                }

                // Paint preedit underline
                if let Some(range) = preedit_byte_range {
                    let p_start_x = text_origin_x + shaped.x_for_index(range.start);
                    let p_end_x = text_origin_x + shaped.x_for_index(range.end);
                    let style = UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(theme.editor.caret.into()),
                        wavy: false,
                    };
                    window.paint_underline(
                        point(p_start_x, row.y + lh - px(2.0)),
                        p_end_x - p_start_x,
                        &style,
                    );
                }
            }
        },
    );
}
