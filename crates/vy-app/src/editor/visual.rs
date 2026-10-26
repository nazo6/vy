//! Visual rows: lines, wrapped chunks, and the mapping in between.
//!
//! Cursor motions, scrolling, hit testing, and painting all need to translate
//! between positions and screen rows. They share this one implementation so the
//! clamps at chunk and line ends cannot drift apart.
//!
//! See `docs/proposals/0005-logic-state-refactor.md` (Phase A).

use std::sync::Arc;

use vy_core::{Position, Text};

use super::Editor;
use crate::keymap::Mode;
use crate::text_layout::{
    DEFAULT_TAB_STOP, LineChunks, cached_chunk_line, display_col_to_utf16, utf16_to_display_col,
};

/// How a line is split into visual rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualOptions {
    /// Wrap width in display columns. [`None`] disables wrapping: one row per line.
    pub cols: Option<usize>,
    /// Tab stop width in display columns.
    pub tab_stop: usize,
}

impl VisualOptions {
    /// One row per line. Columns are UTF-16 offsets, as in `j` / `k` without wrap.
    pub fn without_wrap() -> Self {
        Self {
            cols: None,
            tab_stop: DEFAULT_TAB_STOP,
        }
    }

    /// Wraps each line into chunks of at most `cols` display cells.
    pub fn wrapped(cols: usize) -> Self {
        Self {
            cols: Some(cols.max(1)),
            tab_stop: DEFAULT_TAB_STOP,
        }
    }
}

/// A visual row: a chunk of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VisualRow {
    pub line: u32,
    pub chunk: usize,
}

/// Maps positions to visual rows and back for one document and wrap width.
#[derive(Clone, Copy)]
pub struct VisualIndex<'a> {
    text: &'a Text,
    options: VisualOptions,
}

impl<'a> VisualIndex<'a> {
    pub fn new(text: &'a Text, options: VisualOptions) -> Self {
        Self { text, options }
    }

    pub fn options(&self) -> VisualOptions {
        self.options
    }

    pub fn text(&self) -> &'a Text {
        self.text
    }

    /// The chunks of `line`. Without wrap this is always one chunk.
    pub fn chunks(&self, line: u32) -> Arc<LineChunks> {
        let text = self.line_text(line);
        let cols = self.options.cols.unwrap_or(usize::MAX);
        cached_chunk_line(&text, cols, self.options.tab_stop)
    }

    /// Number of visual rows occupied by `line`.
    pub fn rows_in(&self, line: u32) -> usize {
        self.chunks(line).chunks.len().max(1)
    }

    /// The row that holds `position`.
    pub fn row_of(&self, position: Position) -> VisualRow {
        let line = position.line;
        if self.options.cols.is_none() {
            return VisualRow { line, chunk: 0 };
        }

        let chunks = self.chunks(line);
        let offset = position.character as usize;
        let total = chunks.chunks.len();
        let index = chunks
            .chunks
            .iter()
            .enumerate()
            .find_map(|(index, chunk)| {
                let is_last = index + 1 >= total;
                if is_last {
                    (offset >= chunk.utf16_range.start).then_some(index)
                } else if offset >= chunk.utf16_range.start && offset < chunk.utf16_range.end {
                    Some(index)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        VisualRow { line, chunk: index }
    }

    /// The position at `desired_col` of `row`.
    ///
    /// With wrap, `desired_col` counts display cells inside the row. Without
    /// wrap, it counts UTF-16 code units from the start of the line.
    pub fn position_at(&self, row: VisualRow, desired_col: usize, mode: Mode) -> Position {
        let line = row.line.min(self.last_line());
        let length = self.text.line_len_utf16(line as usize).unwrap_or(0);
        if self.options.cols.is_none() {
            let limit = match mode {
                Mode::Normal => length.saturating_sub(1),
                _ => length,
            };
            return Position::new(line, (desired_col as u32).min(limit));
        }

        let text = self.line_text(line);
        let chunks = self.chunks(line);
        let index = row.chunk.min(chunks.chunks.len().saturating_sub(1));
        let chunk = &chunks.chunks[index];
        let chunk_text = text.get(chunk.byte_range.clone()).unwrap_or("");
        let char_in_chunk = display_col_to_utf16(chunk_text, desired_col, self.options.tab_stop);
        // Normal stays on a character; Select may reach the row end (the line break
        // on the last row).
        let chunk_len = chunk
            .utf16_range
            .end
            .saturating_sub(chunk.utf16_range.start);
        let clamped = match mode {
            Mode::Normal => char_in_chunk.min(chunk_len.saturating_sub(1)),
            _ => char_in_chunk.min(chunk_len),
        };
        Position::new(line, (chunk.utf16_range.start + clamped) as u32)
    }

    /// Moves `delta` visual rows, stopping at the first and last row.
    pub fn step(&self, row: VisualRow, delta: i64) -> VisualRow {
        let mut row = self.clamp_row(row);
        if delta > 0 {
            let mut remaining = delta as usize;
            while remaining > 0 {
                if row.chunk + 1 < self.rows_in(row.line) {
                    row.chunk += 1;
                } else if (row.line as usize) + 1 < self.text.line_count() {
                    row.line += 1;
                    row.chunk = 0;
                } else {
                    break;
                }
                remaining -= 1;
            }
        } else if delta < 0 {
            let mut remaining = delta.unsigned_abs() as usize;
            while remaining > 0 {
                if row.chunk > 0 {
                    row.chunk -= 1;
                } else if row.line > 0 {
                    row.line -= 1;
                    row.chunk = self.rows_in(row.line).saturating_sub(1);
                } else {
                    break;
                }
                remaining -= 1;
            }
        }
        row
    }

    /// The display column of `position` inside its row.
    ///
    /// Without wrap this is the display column from the start of the line.
    pub fn display_col(&self, position: Position) -> usize {
        let row = self.row_of(position);
        let text = self.line_text(row.line);
        let chunks = self.chunks(row.line);
        let chunk = &chunks.chunks[row.chunk.min(chunks.chunks.len().saturating_sub(1))];
        let offset = (position.character as usize).saturating_sub(chunk.utf16_range.start);
        let chunk_text = text.get(chunk.byte_range.clone()).unwrap_or("");
        utf16_to_display_col(chunk_text, offset, self.options.tab_stop)
    }

    /// The vertical motion used by `j` / `k`, `Ctrl-d` / `Ctrl-u`, and display motions.
    ///
    /// Returns the target position and the column to keep for the next motion.
    /// With wrap the column is a display cell in the current row; without wrap it
    /// is a UTF-16 offset in the line.
    pub fn move_vertical(
        &self,
        from: Position,
        delta: i64,
        preferred_col: Option<usize>,
        mode: Mode,
    ) -> (Position, usize) {
        if self.options.cols.is_none() {
            let desired = preferred_col.unwrap_or(from.character as usize);
            let row = VisualRow {
                line: from.line,
                chunk: 0,
            };
            let target = self.step(row, delta);
            return (self.position_at(target, desired, mode), desired);
        }

        let row = self.row_of(from);
        let desired = preferred_col.unwrap_or_else(|| self.display_col(from));

        let target = self.step(row, delta);
        (self.position_at(target, desired, mode), desired)
    }

    /// The number of rows from `from` up to (but not including) `to`.
    pub fn rows_between(&self, from: VisualRow, to: VisualRow) -> usize {
        let from = self.clamp_row(from);
        let to = self.clamp_row(to);
        if to <= from {
            return 0;
        }
        if from.line == to.line {
            return to.chunk - from.chunk;
        }

        let mut rows = self.rows_in(from.line) - from.chunk;
        for line in from.line + 1..to.line {
            rows += self.rows_in(line);
        }
        rows + to.chunk
    }

    /// Clamps `row` into the document.
    pub fn clamp_row(&self, row: VisualRow) -> VisualRow {
        let line = row.line.min(self.last_line());
        let chunk = row.chunk.min(self.rows_in(line).saturating_sub(1));
        VisualRow { line, chunk }
    }

    /// The last line of the document (0 for an empty document).
    pub fn last_line(&self) -> u32 {
        self.text.line_count().saturating_sub(1) as u32
    }

    fn line_text(&self, line: u32) -> String {
        self.text.line_string(line as usize).unwrap_or_default()
    }
}

impl Editor {
    /// The visual grid of this editor with its current wrap width.
    pub fn visual_index(&self) -> VisualIndex<'_> {
        self.visual_index_with(self.wrap_width)
    }

    /// The visual grid with an explicit wrap width. [`None`] disables wrapping.
    ///
    /// Callers outside `vy-app` read the width from the pane rather than the
    /// editor, because only the active pane is synchronized into the editor.
    pub fn visual_index_with(&self, cols: Option<usize>) -> VisualIndex<'_> {
        let options = match cols {
            Some(cols) => VisualOptions::wrapped(cols),
            None => VisualOptions::without_wrap(),
        };
        VisualIndex::new(&self.text, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrapped(text: &str, cols: usize) -> (Text, VisualOptions) {
        (Text::new(text), VisualOptions::wrapped(cols))
    }

    fn rows_of(index: &VisualIndex<'_>, line: u32) -> Vec<VisualRow> {
        (0..index.rows_in(line))
            .map(|chunk| VisualRow { line, chunk })
            .collect()
    }

    #[test]
    fn splits_long_lines_into_rows() {
        let (text, options) = wrapped("abcdefghij", 5);
        let index = VisualIndex::new(&text, options);

        assert_eq!(index.rows_in(0), 2);
        assert_eq!(
            rows_of(&index, 0),
            vec![
                VisualRow { line: 0, chunk: 0 },
                VisualRow { line: 0, chunk: 1 }
            ]
        );
        assert_eq!(index.row_of(Position::new(0, 0)).chunk, 0);
        assert_eq!(index.row_of(Position::new(0, 4)).chunk, 0);
        assert_eq!(index.row_of(Position::new(0, 5)).chunk, 1);
        assert_eq!(index.row_of(Position::new(0, 10)).chunk, 1);
    }

    #[test]
    fn round_trips_positions_through_rows() {
        let (text, options) = wrapped("abcdefghij\nあaい", 4);
        let index = VisualIndex::new(&text, options);

        for line in 0..text.line_count() as u32 {
            let chunks = index.chunks(line);
            for chunk in &chunks.chunks {
                for offset in chunk.utf16_range.clone() {
                    let position = Position::new(line, offset as u32);
                    let row = index.row_of(position);
                    let col = index.display_col(position);
                    assert_eq!(
                        index.position_at(row, col, Mode::Select),
                        position,
                        "line {line} offset {offset}"
                    );
                }
            }
        }
    }

    #[test]
    fn steps_across_lines_and_clamps_at_the_ends() {
        let (text, options) = wrapped("abc\ndefghij\nk", 5);
        let index = VisualIndex::new(&text, options);

        // "abc" (1 row) -> "defghij" (2 rows) -> "k" (1 row).
        let first = VisualRow { line: 0, chunk: 0 };
        assert_eq!(index.step(first, 1), VisualRow { line: 1, chunk: 0 });
        assert_eq!(index.step(first, 2), VisualRow { line: 1, chunk: 1 });
        assert_eq!(index.step(first, 3), VisualRow { line: 2, chunk: 0 });
        assert_eq!(index.step(first, 10), VisualRow { line: 2, chunk: 0 });

        let last = VisualRow { line: 2, chunk: 0 };
        assert_eq!(index.step(last, -1), VisualRow { line: 1, chunk: 1 });
        assert_eq!(index.step(last, -3), VisualRow { line: 0, chunk: 0 });
        assert_eq!(index.step(last, -10), VisualRow { line: 0, chunk: 0 });
    }

    #[test]
    fn keeps_the_display_column_across_rows() {
        let (text, options) = wrapped("abcdefghij\nklmnopqrst", 5);
        let index = VisualIndex::new(&text, options);

        // Down from offset 3 keeps display column 3 in the next row of the same line.
        let (position, col) = index.move_vertical(Position::new(0, 3), 1, None, Mode::Normal);
        assert_eq!(col, 3);
        assert_eq!(position, Position::new(0, 8));

        // Column 4 carries from the end of the first row to the next line.
        let (position, col) = index.move_vertical(Position::new(0, 4), 2, None, Mode::Normal);
        assert_eq!(col, 4);
        assert_eq!(position, Position::new(1, 4));

        // An explicit preferred column wins over the current one.
        let (position, _) = index.move_vertical(Position::new(0, 9), 1, Some(1), Mode::Normal);
        assert_eq!(position, Position::new(1, 1));
    }

    #[test]
    fn empty_lines_and_the_last_line_have_one_row() {
        let (text, options) = wrapped("", 5);
        let index = VisualIndex::new(&text, options);

        assert_eq!(index.rows_in(0), 1);
        assert_eq!(
            index.position_at(VisualRow { line: 0, chunk: 0 }, 3, Mode::Normal),
            Position::new(0, 0)
        );
        assert_eq!(
            index.step(VisualRow { line: 0, chunk: 0 }, 5),
            VisualRow { line: 0, chunk: 0 }
        );
    }

    #[test]
    fn normal_mode_stays_before_the_row_end() {
        let (text, options) = wrapped("abcdefghij", 5);
        let index = VisualIndex::new(&text, options);
        let second = VisualRow { line: 0, chunk: 1 };

        // The second row holds offsets 5..10; Normal clamps to the last character.
        assert_eq!(
            index.position_at(second, 99, Mode::Normal),
            Position::new(0, 9)
        );
        assert_eq!(
            index.position_at(second, 99, Mode::Select),
            Position::new(0, 10)
        );
    }

    #[test]
    fn counts_tabs_and_wide_characters_as_display_cells() {
        let (text, options) = wrapped("あ\tb", 4);
        let index = VisualIndex::new(&text, options);

        // "あ" is 2 cells, the tab fills to 4, "b" starts a new row.
        assert_eq!(index.rows_in(0), 2);
        assert_eq!(
            index.position_at(VisualRow { line: 0, chunk: 1 }, 0, Mode::Select),
            Position::new(0, 2)
        );
        assert_eq!(
            index.position_at(VisualRow { line: 0, chunk: 1 }, 1, Mode::Select),
            Position::new(0, 3)
        );
    }

    #[test]
    fn without_wrap_uses_utf16_columns_and_one_row_per_line() {
        let text = Text::new("abcdef\nghijkl");
        let index = VisualIndex::new(&text, VisualOptions::without_wrap());

        assert_eq!(index.rows_in(0), 1);
        assert_eq!(
            index.row_of(Position::new(0, 4)),
            VisualRow { line: 0, chunk: 0 }
        );

        // Columns are UTF-16 offsets, and Normal clamps to the last character.
        assert_eq!(
            index.position_at(VisualRow { line: 0, chunk: 0 }, 4, Mode::Normal),
            Position::new(0, 4)
        );
        assert_eq!(
            index.position_at(VisualRow { line: 0, chunk: 0 }, 99, Mode::Normal),
            Position::new(0, 5)
        );
        assert_eq!(
            index.position_at(VisualRow { line: 0, chunk: 0 }, 99, Mode::Select),
            Position::new(0, 6)
        );

        let (position, col) = index.move_vertical(Position::new(0, 3), 1, None, Mode::Select);
        assert_eq!(col, 3);
        assert_eq!(position, Position::new(1, 3));
    }
}
