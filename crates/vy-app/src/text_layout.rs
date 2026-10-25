//! Text offset and viewport scroll calculations for views.

use unicode_width::UnicodeWidthChar;
use vy_core::{Position, Range};

/// Default tab stop width in display columns.
pub const DEFAULT_TAB_STOP: usize = 4;

/// Returns the display cell width of a character, taking tab stops into account.
pub fn char_cell_width(ch: char, current_col: usize, tab_stop: usize) -> usize {
    if ch == '\t' {
        let ts = tab_stop.max(1);
        ts - (current_col % ts)
    } else if ch == '\n' || ch == '\r' {
        0
    } else {
        ch.width().unwrap_or(0)
    }
}

/// Returns the total display width (in columns/cells) of a string.
pub fn str_display_width(text: &str, start_col: usize, tab_stop: usize) -> usize {
    let mut col = start_col;
    for ch in text.chars() {
        col += char_cell_width(ch, col, tab_stop);
    }
    col - start_col
}

/// Converts a UTF-16 code unit offset on a line into its visual display column.
pub fn utf16_to_display_col(line: &str, utf16_offset: usize, tab_stop: usize) -> usize {
    let mut current_utf16 = 0;
    let mut current_col = 0;

    for ch in line.chars() {
        if current_utf16 >= utf16_offset {
            break;
        }
        current_col += char_cell_width(ch, current_col, tab_stop);
        current_utf16 += ch.len_utf16();
    }

    current_col
}

/// Converts a visual display column on a line into its UTF-16 code unit offset.
pub fn display_col_to_utf16(line: &str, target_col: usize, tab_stop: usize) -> usize {
    let mut current_utf16 = 0;
    let mut current_col = 0;

    for ch in line.chars() {
        if current_col >= target_col {
            break;
        }
        current_col += char_cell_width(ch, current_col, tab_stop);
        current_utf16 += ch.len_utf16();
    }

    current_utf16
}

/// Returns the viewport top that keeps `line` inside the window.
pub fn scroll_to_show(line: usize, top: usize, rows: usize, total: usize) -> usize {
    let rows = rows.max(1);
    let top = if line < top {
        line
    } else if line >= top + rows {
        line + 1 - rows
    } else {
        top
    };

    clamp_scroll_top(top, rows, total)
}

/// Clamps the viewport top so the last screen stays full.
pub fn clamp_scroll_top(top: usize, rows: usize, total: usize) -> usize {
    top.min(total.saturating_sub(rows.max(1)))
}

/// Moves the viewport by whole lines. Positive values go towards later lines.
pub fn scroll_lines(top: usize, lines: isize, rows: usize, total: usize) -> usize {
    let max_top = total.saturating_sub(rows.max(1)) as isize;
    (top as isize + lines).clamp(0, max_top) as usize
}

/// Returns the viewport left that keeps `column` inside the window.
pub fn scroll_to_show_horizontal(col: usize, left: usize, cols: usize) -> usize {
    let cols = cols.max(1);
    let margin = if cols > 8 { 2 } else { 0 };

    if col < left + margin {
        col.saturating_sub(margin)
    } else if col + margin >= left + cols {
        (col + margin + 1).saturating_sub(cols)
    } else {
        left
    }
}

/// Moves the horizontal viewport by whole columns. Positive values go towards later columns.
pub fn scroll_cols(left: usize, delta: isize) -> usize {
    (left as isize + delta).max(0) as usize
}

/// Clamps the viewport left so the viewport does not scroll past the longest line.
pub fn clamp_scroll_left(left: usize, cols: usize, max_line_width: usize) -> usize {
    let max_left = max_line_width.saturating_sub(cols.max(1));
    left.min(max_left)
}

/// Moves the horizontal viewport by whole columns, clamped between 0 and `max_line_width`.
pub fn scroll_cols_clamped(left: usize, delta: isize, cols: usize, max_line_width: usize) -> usize {
    let max_left = max_line_width.saturating_sub(cols.max(1)) as isize;
    (left as isize + delta).clamp(0, max_left.max(0)) as usize
}

/// Returns the number of lines in a half-page scroll.
pub fn half_page_lines(rows: usize, count: u32) -> usize {
    let half = (rows.max(1) / 2).max(1);
    half.saturating_mul(count.max(1) as usize)
}

/// Returns the viewport top that centers `line` in the window.
pub fn scroll_cursor_center(line: usize, rows: usize, total: usize) -> usize {
    let rows = rows.max(1);
    let top = line.saturating_sub(rows / 2);
    clamp_scroll_top(top, rows, total)
}

/// Returns the viewport top that places `line` at the top of the window.
pub fn scroll_cursor_top(line: usize, rows: usize, total: usize) -> usize {
    clamp_scroll_top(line, rows, total)
}

/// Returns the viewport top that places `line` at the bottom of the window.
pub fn scroll_cursor_bottom(line: usize, rows: usize, total: usize) -> usize {
    let rows = rows.max(1);
    let top = (line + 1).saturating_sub(rows);
    clamp_scroll_top(top, rows, total)
}

pub use vy_core::{byte_to_utf16, utf16_to_byte};

/// The part of one line covered by the selection. [`None`] if the line is outside it.
pub fn selection_on_line(selection: Range, line: u32, length: u32) -> Option<(u32, u32)> {
    if selection.start == selection.end || line < selection.start.line || line > selection.end.line
    {
        return None;
    }

    let from = if line == selection.start.line {
        selection.start.character
    } else {
        0
    };
    let to = if line == selection.end.line {
        selection.end.character.min(length)
    } else {
        length
    };

    if from >= to {
        // Selecting only the line break: let the caller draw a marker.
        return (to >= length).then_some((length, length));
    }

    Some((from.min(length), to))
}

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex};

/// A slice of a logical line wrapped to fit within the viewport width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineChunk {
    /// Byte range in the line's UTF-8 text.
    pub byte_range: std::ops::Range<usize>,
    /// UTF-16 code unit range in the line.
    pub utf16_range: std::ops::Range<usize>,
}

/// A collection of visual chunks for a line and its total UTF-16 code unit count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineChunks {
    pub chunks: Vec<LineChunk>,
    pub utf16_length: u32,
}

type ChunkCache = HashMap<(u64, usize, usize), Arc<LineChunks>>;

static CHUNK_CACHE: LazyLock<Mutex<ChunkCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns cached visual chunks and UTF-16 length for `text`.
pub fn cached_chunk_line(text: &str, max_cols: usize, tab_stop: usize) -> Arc<LineChunks> {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let text_hash = hasher.finish();
    let key = (text_hash, max_cols, tab_stop);

    if let Ok(cache) = CHUNK_CACHE.lock()
        && let Some(entry) = cache.get(&key)
    {
        return Arc::clone(entry);
    }

    let entry = Arc::new(compute_line_chunks(text, max_cols, tab_stop));

    if let Ok(mut cache) = CHUNK_CACHE.lock() {
        if cache.len() >= 1024 {
            cache.clear();
        }
        cache.insert(key, Arc::clone(&entry));
    }

    entry
}

fn compute_line_chunks(text: &str, max_cols: usize, tab_stop: usize) -> LineChunks {
    let max_cols = max_cols.max(1);
    if text.is_empty() {
        return LineChunks {
            chunks: vec![LineChunk {
                byte_range: 0..0,
                utf16_range: 0..0,
            }],
            utf16_length: 0,
        };
    }

    let mut chunks = Vec::new();
    let mut chunk_byte_start = 0;
    let mut chunk_utf16_start = 0;
    let mut current_cols = 0;
    let mut current_byte = 0;
    let mut current_utf16 = 0;

    for (byte_idx, ch) in text.char_indices() {
        let ch_width = char_cell_width(ch, current_cols, tab_stop);
        let ch_utf16 = ch.len_utf16();

        if current_cols + ch_width > max_cols && current_cols > 0 {
            chunks.push(LineChunk {
                byte_range: chunk_byte_start..byte_idx,
                utf16_range: chunk_utf16_start..current_utf16,
            });
            chunk_byte_start = byte_idx;
            chunk_utf16_start = current_utf16;
            current_cols = 0;
        }
        current_cols += char_cell_width(ch, current_cols, tab_stop);
        current_byte = byte_idx + ch.len_utf8();
        current_utf16 += ch_utf16;
    }

    if chunk_byte_start < current_byte || chunks.is_empty() {
        chunks.push(LineChunk {
            byte_range: chunk_byte_start..current_byte,
            utf16_range: chunk_utf16_start..current_utf16,
        });
    }

    LineChunks {
        chunks,
        utf16_length: current_utf16 as u32,
    }
}

/// Splits `text` into chunks of at most `max_cols` visual display cells without breaking characters or surrogate pairs.
pub fn chunk_line(text: &str, max_cols: usize) -> Vec<LineChunk> {
    chunk_line_with_tab_stop(text, max_cols, DEFAULT_TAB_STOP)
}

/// Splits `text` into chunks of at most `max_cols` visual display cells using `tab_stop`.
pub fn chunk_line_with_tab_stop(text: &str, max_cols: usize, tab_stop: usize) -> Vec<LineChunk> {
    cached_chunk_line(text, max_cols, tab_stop).chunks.clone()
}

/// Returns the cursor's character offset within `chunk` if it lies on this visual chunk.
pub fn cursor_in_chunk(
    cursor: Position,
    line_number: usize,
    chunk_index: usize,
    total_chunks: usize,
    utf16_range: &std::ops::Range<usize>,
) -> Option<u32> {
    if cursor.line as usize != line_number {
        return None;
    }

    let pos = cursor.character as usize;
    let is_last = chunk_index + 1 >= total_chunks;

    if is_last {
        if pos >= utf16_range.start {
            Some((pos.min(utf16_range.end) - utf16_range.start) as u32)
        } else {
            None
        }
    } else if pos >= utf16_range.start && pos < utf16_range.end {
        Some((pos - utf16_range.start) as u32)
    } else {
        None
    }
}

/// Returns the selection range in UTF-16 code units within `chunk` if it intersects this visual chunk.
pub fn selection_in_chunk(
    selection: Range,
    line_number: usize,
    chunk_index: usize,
    total_chunks: usize,
    utf16_range: &std::ops::Range<usize>,
    line_length: u32,
) -> Option<(u32, u32)> {
    let (sel_from, sel_to) = selection_on_line(selection, line_number as u32, line_length)?;

    let chunk_len = (utf16_range.end - utf16_range.start) as u32;
    let from = (sel_from as usize).max(utf16_range.start);
    let to = (sel_to as usize).min(utf16_range.end);

    if from < to {
        Some((
            (from - utf16_range.start) as u32,
            (to - utf16_range.start) as u32,
        ))
    } else if sel_from >= sel_to
        && sel_to as usize >= utf16_range.end
        && chunk_index + 1 >= total_chunks
    {
        Some((chunk_len, chunk_len))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_units_to_byte_offsets() {
        assert_eq!(utf16_to_byte("a😀b", 0), 0);
        assert_eq!(utf16_to_byte("a😀b", 1), 1);
        // A position inside a surrogate pair is rounded down.
        assert_eq!(utf16_to_byte("a😀b", 2), 1);
        assert_eq!(utf16_to_byte("a😀b", 3), 5);
        assert_eq!(utf16_to_byte("a😀b", 4), 6);
    }

    #[test]
    fn clips_a_selection_to_each_line() {
        let selection = Range::new(Position::new(0, 1), Position::new(2, 1));

        assert_eq!(selection_on_line(selection, 0, 3), Some((1, 3)));
        assert_eq!(selection_on_line(selection, 1, 5), Some((0, 5)));
        assert_eq!(selection_on_line(selection, 2, 4), Some((0, 1)));
        assert_eq!(selection_on_line(selection, 3, 4), None);
    }

    #[test]
    fn marks_a_selected_line_break() {
        let selection = Range::new(Position::new(0, 3), Position::new(1, 0));
        assert_eq!(selection_on_line(selection, 0, 3), Some((3, 3)));
    }

    #[test]
    fn scrolls_only_when_the_cursor_leaves_the_window() {
        // 100 lines, 10 rows, starting at the top.
        assert_eq!(scroll_to_show(5, 0, 10, 100), 0);
        assert_eq!(scroll_to_show(10, 0, 10, 100), 1);
        assert_eq!(scroll_to_show(42, 20, 10, 100), 33);
        assert_eq!(scroll_to_show(3, 20, 10, 100), 3);
    }

    #[test]
    fn keeps_the_last_screen_full() {
        assert_eq!(clamp_scroll_top(95, 10, 100), 90);
        assert_eq!(clamp_scroll_top(4, 10, 100), 4);
        assert_eq!(clamp_scroll_top(4, 10, 3), 0);
    }

    #[test]
    fn scrolls_by_lines_within_the_document() {
        assert_eq!(scroll_lines(20, 5, 10, 100), 25);
        assert_eq!(scroll_lines(20, -5, 10, 100), 15);
        assert_eq!(scroll_lines(2, -5, 10, 100), 0);
        assert_eq!(scroll_lines(88, 5, 10, 100), 90);
    }

    #[test]
    fn scrolls_horizontally_with_margin() {
        // 80 cols, starting at 0.
        assert_eq!(scroll_to_show_horizontal(5, 0, 80), 0);
        assert_eq!(scroll_to_show_horizontal(77, 0, 80), 0);
        assert_eq!(scroll_to_show_horizontal(78, 0, 80), 1);
        assert_eq!(scroll_to_show_horizontal(100, 0, 80), 23);
        assert_eq!(scroll_to_show_horizontal(20, 23, 80), 18);
        assert_eq!(scroll_to_show_horizontal(0, 23, 80), 0);
    }

    #[test]
    fn scrolls_cols_clamps_at_zero() {
        assert_eq!(scroll_cols(10, 5), 15);
        assert_eq!(scroll_cols(10, -5), 5);
        assert_eq!(scroll_cols(10, -15), 0);
    }

    #[test]
    fn scrolls_cols_clamps_at_max_line_width() {
        // cols = 10, max_line_width = 30 -> max_left = 20
        assert_eq!(scroll_cols_clamped(15, 10, 10, 30), 20);
        assert_eq!(scroll_cols_clamped(15, -10, 10, 30), 5);
        assert_eq!(scroll_cols_clamped(15, -20, 10, 30), 0);
        assert_eq!(clamp_scroll_left(25, 10, 30), 20);
        assert_eq!(clamp_scroll_left(5, 10, 30), 5);
    }

    #[test]
    fn chunks_empty_line() {
        let chunks = chunk_line("", 10);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].byte_range, 0..0);
        assert_eq!(chunks[0].utf16_range, 0..0);
    }

    #[test]
    fn chunks_ascii_text() {
        let chunks = chunk_line("abcdefghijklmno", 5);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].byte_range, 0..5);
        assert_eq!(chunks[0].utf16_range, 0..5);
        assert_eq!(chunks[1].byte_range, 5..10);
        assert_eq!(chunks[1].utf16_range, 5..10);
        assert_eq!(chunks[2].byte_range, 10..15);
        assert_eq!(chunks[2].utf16_range, 10..15);
    }

    #[test]
    fn chunks_multibyte_and_surrogate_text() {
        let chunks = chunk_line("あ😀い", 2);
        // 'あ' has display width 2 (1 utf16 unit, 3 bytes).
        // '😀' has display width 2 (2 utf16 units, 4 bytes).
        // 'い' has display width 2 (1 utf16 unit, 3 bytes).
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].byte_range, 0..3); // "あ"
        assert_eq!(chunks[0].utf16_range, 0..1);
        assert_eq!(chunks[1].byte_range, 3..7); // "😀"
        assert_eq!(chunks[1].utf16_range, 1..3);
        assert_eq!(chunks[2].byte_range, 7..10); // "い"
        assert_eq!(chunks[2].utf16_range, 3..4);
    }

    #[test]
    fn chunks_with_mixed_width_and_tabs() {
        // "あaい": "あ"(2) + "a"(1) fits in 3 cols; "い"(2) goes to next chunk.
        let chunks = chunk_line("あaい", 3);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].byte_range, 0..4); // "あa"
        assert_eq!(chunks[0].utf16_range, 0..2);
        assert_eq!(chunks[1].byte_range, 4..7); // "い"
        assert_eq!(chunks[1].utf16_range, 2..3);

        // Tab with tab stop 4 takes 4 cols
        let chunks = chunk_line("\ta", 4);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].byte_range, 0..1); // "\t"
        assert_eq!(chunks[1].byte_range, 1..2); // "a"
    }

    #[test]
    fn converts_utf16_to_display_column() {
        // "あa\tい":
        // "あ": col 0 -> 2
        // "a": col 2 -> 3
        // "\t": col 3 -> 4 (tab stop 4)
        // "い": col 4 -> 6
        assert_eq!(utf16_to_display_col("あa\tい", 0, 4), 0);
        assert_eq!(utf16_to_display_col("あa\tい", 1, 4), 2);
        assert_eq!(utf16_to_display_col("あa\tい", 2, 4), 3);
        assert_eq!(utf16_to_display_col("あa\tい", 3, 4), 4);
        assert_eq!(utf16_to_display_col("あa\tい", 4, 4), 6);

        assert_eq!(display_col_to_utf16("あa\tい", 0, 4), 0);
        assert_eq!(display_col_to_utf16("あa\tい", 2, 4), 1);
        assert_eq!(display_col_to_utf16("あa\tい", 3, 4), 2);
        assert_eq!(display_col_to_utf16("あa\tい", 4, 4), 3);
        assert_eq!(display_col_to_utf16("あa\tい", 6, 4), 4);
    }

    #[test]
    fn locates_cursor_in_correct_chunk() {
        let chunks = chunk_line("abcdefghij", 5);
        // Chunk 0: 0..5, Chunk 1: 5..10
        assert_eq!(
            cursor_in_chunk(Position::new(0, 0), 0, 0, 2, &chunks[0].utf16_range),
            Some(0)
        );
        assert_eq!(
            cursor_in_chunk(Position::new(0, 4), 0, 0, 2, &chunks[0].utf16_range),
            Some(4)
        );
        assert_eq!(
            cursor_in_chunk(Position::new(0, 5), 0, 0, 2, &chunks[0].utf16_range),
            None
        );
        assert_eq!(
            cursor_in_chunk(Position::new(0, 5), 0, 1, 2, &chunks[1].utf16_range),
            Some(0)
        );
        assert_eq!(
            cursor_in_chunk(Position::new(0, 10), 0, 1, 2, &chunks[1].utf16_range),
            Some(5)
        );
    }

    #[test]
    fn clips_selection_across_chunks() {
        let text = "abcdefghij";
        let chunks = chunk_line(text, 5);
        let selection = Range::new(Position::new(0, 2), Position::new(0, 8));

        assert_eq!(
            selection_in_chunk(selection, 0, 0, 2, &chunks[0].utf16_range, 10),
            Some((2, 5))
        );
        assert_eq!(
            selection_in_chunk(selection, 0, 1, 2, &chunks[1].utf16_range, 10),
            Some((0, 3))
        );
    }

    #[test]
    fn calculates_half_page_scroll_lines() {
        assert_eq!(half_page_lines(20, 1), 10);
        assert_eq!(half_page_lines(20, 2), 20);
        assert_eq!(half_page_lines(1, 1), 1);
        assert_eq!(half_page_lines(0, 1), 1);
    }

    #[test]
    fn scrolls_cursor_relative_to_viewport() {
        // Center: 100 lines, 20 rows.
        assert_eq!(scroll_cursor_center(50, 20, 100), 40);
        assert_eq!(scroll_cursor_center(5, 20, 100), 0);
        assert_eq!(scroll_cursor_center(95, 20, 100), 80);

        // Top:
        assert_eq!(scroll_cursor_top(50, 20, 100), 50);
        assert_eq!(scroll_cursor_top(95, 20, 100), 80);

        // Bottom:
        assert_eq!(scroll_cursor_bottom(50, 20, 100), 31);
        assert_eq!(scroll_cursor_bottom(5, 20, 100), 0);
        assert_eq!(scroll_cursor_bottom(95, 20, 100), 76);
    }

    #[test]
    fn caches_line_chunks_stably() {
        let first = cached_chunk_line("line with words to wrap", 10, DEFAULT_TAB_STOP);
        let second = cached_chunk_line("line with words to wrap", 10, DEFAULT_TAB_STOP);
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(first.utf16_length, 23);
    }
}
