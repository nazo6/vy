//! Text offset and viewport scroll calculations for views.

use vy_core::Range;

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

#[cfg(test)]
mod tests {
    use super::*;
    use vy_core::Position;

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
}
