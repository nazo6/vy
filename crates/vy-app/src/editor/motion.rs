//! Cursor motions and character classification.

use crate::commands::BuiltinCommand;

/// Motions shared by normal movement and operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Motion {
    Left,
    Right,
    Up,
    Down,
    DisplayUp,
    DisplayDown,
    WordForward,
    WordBackward,
    WordEnd,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// Character classes used by word motions and the word text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CharClass {
    Space,
    Word,
    Punctuation,
}

pub(super) fn motion_of(command: BuiltinCommand) -> Option<Motion> {
    match command {
        BuiltinCommand::MoveLeft => Some(Motion::Left),
        BuiltinCommand::MoveRight => Some(Motion::Right),
        BuiltinCommand::MoveUp => Some(Motion::Up),
        BuiltinCommand::MoveDown => Some(Motion::Down),
        BuiltinCommand::MoveDisplayUp => Some(Motion::DisplayUp),
        BuiltinCommand::MoveDisplayDown => Some(Motion::DisplayDown),
        BuiltinCommand::WordForward => Some(Motion::WordForward),
        BuiltinCommand::WordBackward => Some(Motion::WordBackward),
        BuiltinCommand::WordEnd => Some(Motion::WordEnd),
        BuiltinCommand::LineStart => Some(Motion::LineStart),
        BuiltinCommand::LineEnd => Some(Motion::LineEnd),
        BuiltinCommand::DocumentStart => Some(Motion::DocumentStart),
        BuiltinCommand::DocumentEnd => Some(Motion::DocumentEnd),
        _ => None,
    }
}

pub(super) fn combined_count(left: Option<u32>, right: Option<u32>) -> Option<u32> {
    match (left, right) {
        (None, count) => count,
        (count, None) => count,
        (Some(left), Some(right)) => Some(left.saturating_mul(right)),
    }
}

use super::Editor;
use super::operator::OperatorRange;
use crate::keymap::Mode;
use crate::text_layout::{
    DEFAULT_TAB_STOP, cached_chunk_line, display_col_to_utf16, utf16_to_display_col,
};
use vy_core::Position;

impl Editor {
    pub(super) fn apply_motion(&mut self, motion: Motion, count: Option<u32>) {
        let times = count.unwrap_or(1).max(1);
        match motion {
            Motion::Left => {
                for _ in 0..times {
                    self.move_horizontal(-1);
                }
            }
            Motion::Right => {
                for _ in 0..times {
                    self.move_horizontal(1);
                }
            }
            Motion::Up => self.move_vertical(-(times as i64)),
            Motion::Down => self.move_vertical(times as i64),
            Motion::DisplayUp => self.move_display_vertical(-(times as i64)),
            Motion::DisplayDown => self.move_display_vertical(times as i64),
            Motion::WordForward => {
                for _ in 0..times {
                    let target = self.word_forward_at(self.current());
                    self.move_to(target);
                }
            }
            Motion::WordBackward => {
                for _ in 0..times {
                    let target = self.word_backward_at(self.current());
                    self.move_to(target);
                }
            }
            Motion::WordEnd => {
                for _ in 0..times {
                    let target = self.word_end_at(self.current());
                    self.move_to(target);
                }
            }
            Motion::LineStart => self.move_to(Position::new(self.current().line, 0)),
            Motion::LineEnd => self.move_to(self.line_end(self.current().line)),
            Motion::DocumentStart => {
                let line = count
                    .map_or(0, |count| count.saturating_sub(1))
                    .min(self.last_line());
                self.move_to(Position::new(line, 0));
            }
            Motion::DocumentEnd => {
                let line = match count {
                    Some(count) => count.saturating_sub(1).min(self.last_line()),
                    None => self.last_line(),
                };
                self.move_to(Position::new(line, 0));
            }
        }
    }

    pub(super) fn motion_range(&self, motion: Motion, count: Option<u32>) -> Option<OperatorRange> {
        let times = count.unwrap_or(1).max(1);
        let (start, end) = self.ordered();

        match motion {
            Motion::Left => {
                let from = self.text.position_to_char(start).ok()?;
                let target = from.saturating_sub(times as usize);
                if target == from {
                    return None;
                }
                Some(OperatorRange {
                    start: self.position_of(target),
                    end,
                    linewise: false,
                })
            }
            Motion::Right => {
                let from = self.text.position_to_char(start).ok()?;
                let target = (from + times as usize).min(self.text.len_chars());
                if target == from {
                    return None;
                }
                Some(OperatorRange {
                    start,
                    end: self.position_of(target),
                    linewise: false,
                })
            }
            Motion::WordForward => {
                let mut target = start;
                for _ in 0..times {
                    target = self.word_forward_at(target);
                }
                if target == start {
                    return None;
                }
                Some(OperatorRange {
                    start,
                    end: target,
                    linewise: false,
                })
            }
            Motion::WordBackward => {
                let mut target = start;
                for _ in 0..times {
                    target = self.word_backward_at(target);
                }
                if target == start {
                    return None;
                }
                Some(OperatorRange {
                    start: target,
                    end,
                    linewise: false,
                })
            }
            Motion::WordEnd => {
                let mut target = start;
                for _ in 0..times {
                    target = self.word_end_at(target);
                }
                let end = self.next_char(target).unwrap_or(target);
                Some(OperatorRange {
                    start,
                    end,
                    linewise: false,
                })
            }
            Motion::LineStart => Some(OperatorRange {
                start: Position::new(start.line, 0),
                end,
                linewise: false,
            }),
            Motion::LineEnd => {
                let length = self
                    .text
                    .line_len_utf16(start.line as usize)
                    .unwrap_or_default();
                let end = Position::new(start.line, length);
                if start == end {
                    return None;
                }
                Some(OperatorRange {
                    start,
                    end,
                    linewise: false,
                })
            }
            Motion::Up | Motion::Down => {
                let target = match motion {
                    Motion::Up => start.line as i64 - times as i64,
                    _ => start.line as i64 + times as i64,
                }
                .clamp(0, self.last_line() as i64) as u32;
                let (first, last) = (start.line.min(target), start.line.max(target));
                Some(self.linewise_range(first, last))
            }
            Motion::DisplayUp | Motion::DisplayDown => {
                let delta = match motion {
                    Motion::DisplayUp => -(times as i64),
                    _ => times as i64,
                };
                let (target, _) = self.target_display_vertical(start, delta, None);
                if target == start {
                    return None;
                }
                let (first, last) = if start <= target {
                    (start, target)
                } else {
                    (target, start)
                };
                Some(OperatorRange {
                    start: first,
                    end: last,
                    linewise: false,
                })
            }
            Motion::DocumentStart => Some(self.linewise_range(0, start.line)),
            Motion::DocumentEnd => Some(self.linewise_range(start.line, self.last_line())),
        }
    }

    pub(super) fn move_to(&mut self, position: Position) {
        self.preferred_character = None;
        self.preferred_display_col = None;
        self.move_to_inner(position);
    }

    pub(super) fn move_to_inner(&mut self, position: Position) {
        let position = self.clamp(position);
        match self.mode {
            Mode::Normal => self.select_one_char(position),
            Mode::Select | Mode::SelectLine | Mode::SelectBlock => self.head = position,
            Mode::Insert => {
                self.anchor = position;
                self.head = position;
            }
        }
    }

    pub(super) fn move_horizontal(&mut self, delta: isize) {
        let current = self.current();
        let index = self.text.position_to_char(current).unwrap_or(0);
        let target = if delta < 0 {
            index.saturating_sub(1)
        } else {
            (index + 1).min(self.text.len_chars())
        };
        let position = self.text.char_to_position(target).unwrap_or(current);
        self.preferred_character = Some(position.character);
        self.preferred_display_col = None;
        self.move_to_inner(position);
    }

    pub fn move_vertical(&mut self, delta: i64) {
        let current = self.current();
        let desired = self.preferred_character.unwrap_or(current.character);
        let line = (current.line as i64 + delta).clamp(0, self.last_line() as i64) as u32;
        let length = self.text.line_len_utf16(line as usize).unwrap_or_default();
        // Normal stays on a character; Select may reach the line end (the line break).
        let limit = match self.mode {
            Mode::Normal => length.saturating_sub(1),
            _ => length,
        };
        let position = Position::new(line, desired.min(limit));
        self.preferred_character = Some(desired);
        self.preferred_display_col = None;
        self.move_to_inner(position);
    }

    pub(super) fn target_display_vertical(
        &self,
        from: Position,
        delta: i64,
        preferred_col: Option<usize>,
    ) -> (Position, usize) {
        let Some(cols) = self.wrap_width else {
            let desired = preferred_col
                .unwrap_or_else(|| self.preferred_character.unwrap_or(from.character) as usize);
            let line = (from.line as i64 + delta).clamp(0, self.last_line() as i64) as u32;
            let length = self.text.line_len_utf16(line as usize).unwrap_or_default();
            let limit = match self.mode {
                Mode::Normal => length.saturating_sub(1),
                _ => length,
            };
            let position = Position::new(line, (desired as u32).min(limit));
            return (position, desired);
        };

        let cols = cols.max(1);
        let tab_stop = DEFAULT_TAB_STOP;

        let from_line_text = self
            .text
            .line_string(from.line as usize)
            .unwrap_or_default();
        let from_chunks = cached_chunk_line(&from_line_text, cols, tab_stop);
        let char_offset = from.character as usize;

        let current_chunk_idx = from_chunks
            .chunks
            .iter()
            .enumerate()
            .find_map(|(idx, chunk)| {
                let is_last = idx + 1 >= from_chunks.chunks.len();
                if is_last {
                    if char_offset >= chunk.utf16_range.start {
                        Some(idx)
                    } else {
                        None
                    }
                } else if char_offset >= chunk.utf16_range.start
                    && char_offset < chunk.utf16_range.end
                {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let current_chunk = &from_chunks.chunks[current_chunk_idx];
        let offset_in_chunk = char_offset.saturating_sub(current_chunk.utf16_range.start);
        let chunk_text = from_line_text
            .get(current_chunk.byte_range.clone())
            .unwrap_or("");
        let current_col = utf16_to_display_col(chunk_text, offset_in_chunk, tab_stop);
        let desired_col = preferred_col.unwrap_or(current_col);

        let mut line = from.line as usize;
        let mut chunk_idx = current_chunk_idx;
        let total_lines = self.text.line_count();

        if delta > 0 {
            let mut remaining = delta as usize;
            while remaining > 0 {
                let line_text = self.text.line_string(line).unwrap_or_default();
                let chunks = cached_chunk_line(&line_text, cols, tab_stop);
                if chunk_idx + 1 < chunks.chunks.len() {
                    chunk_idx += 1;
                    remaining -= 1;
                } else if line + 1 < total_lines {
                    line += 1;
                    chunk_idx = 0;
                    remaining -= 1;
                } else {
                    break;
                }
            }
        } else if delta < 0 {
            let mut remaining = delta.unsigned_abs() as usize;
            while remaining > 0 {
                if chunk_idx > 0 {
                    chunk_idx -= 1;
                    remaining -= 1;
                } else if line > 0 {
                    line -= 1;
                    let prev_text = self.text.line_string(line).unwrap_or_default();
                    let prev_chunks = cached_chunk_line(&prev_text, cols, tab_stop);
                    chunk_idx = prev_chunks.chunks.len().saturating_sub(1);
                    remaining -= 1;
                } else {
                    break;
                }
            }
        }

        let target_line_text = self.text.line_string(line).unwrap_or_default();
        let target_chunks = cached_chunk_line(&target_line_text, cols, tab_stop);
        let target_chunk = &target_chunks.chunks[chunk_idx];
        let target_chunk_text = target_line_text
            .get(target_chunk.byte_range.clone())
            .unwrap_or("");

        let char_in_chunk = display_col_to_utf16(target_chunk_text, desired_col, tab_stop);
        let chunk_len = target_chunk
            .utf16_range
            .end
            .saturating_sub(target_chunk.utf16_range.start);

        let clamped_char_in_chunk = match self.mode {
            Mode::Normal => {
                if chunk_len == 0 {
                    0
                } else {
                    char_in_chunk.min(chunk_len.saturating_sub(1))
                }
            }
            _ => char_in_chunk.min(chunk_len),
        };

        let target_char = (target_chunk.utf16_range.start + clamped_char_in_chunk) as u32;
        let position = Position::new(line as u32, target_char);
        (position, desired_col)
    }

    pub fn move_display_vertical(&mut self, delta: i64) {
        let current = self.current();
        let (position, desired_col) =
            self.target_display_vertical(current, delta, self.preferred_display_col);
        self.preferred_display_col = Some(desired_col);
        self.preferred_character = Some(position.character);
        self.move_to_inner(position);
    }

    pub(super) fn last_line(&self) -> u32 {
        self.text.line_count().saturating_sub(1) as u32
    }

    pub(super) fn line_end(&self, line: u32) -> Position {
        let length = self.text.line_len_utf16(line as usize).unwrap_or(0);
        Position::new(line, length.saturating_sub(1))
    }

    pub(super) fn next_char(&self, at: Position) -> Option<Position> {
        let index = self.text.position_to_char(at).ok()?;
        self.text.char_to_position(index + 1).ok()
    }

    pub(super) fn previous_char(&self, at: Position) -> Option<Position> {
        let index = self.text.position_to_char(at).ok()?;
        if index == 0 {
            return None;
        }
        self.text.char_to_position(index - 1).ok()
    }

    /// Returns the position right after inserted text.
    pub(super) fn advance(&self, from: Position, value: &str) -> Position {
        let mut line = from.line;
        let mut character = from.character;

        for character_in_text in value.chars() {
            if character_in_text == '\n' {
                line += 1;
                character = 0;
            } else {
                character += character_in_text.len_utf16() as u32;
            }
        }

        Position::new(line, character)
    }

    pub(super) fn class_at(&self, index: usize) -> Option<CharClass> {
        self.text.char_at_index(index).map(|character| {
            if character.is_whitespace() {
                CharClass::Space
            } else if character.is_alphanumeric() || character == '_' {
                CharClass::Word
            } else {
                CharClass::Punctuation
            }
        })
    }

    pub(super) fn word_forward_at(&self, from: Position) -> Position {
        let length = self.text.len_chars();
        let mut index = self.text.position_to_char(from).unwrap_or(0);
        let Some(class) = self.class_at(index) else {
            return from;
        };

        while index < length && self.class_at(index) == Some(class) {
            index += 1;
        }
        while index < length && self.class_at(index) == Some(CharClass::Space) {
            index += 1;
        }

        self.text.char_to_position(index).unwrap_or(from)
    }

    pub(super) fn word_backward_at(&self, from: Position) -> Position {
        let mut index = self.text.position_to_char(from).unwrap_or(0);
        if index == 0 {
            return from;
        }

        index -= 1;
        while index > 0 && self.class_at(index) == Some(CharClass::Space) {
            index -= 1;
        }
        let class = self.class_at(index);
        while index > 0 && self.class_at(index - 1) == class {
            index -= 1;
        }

        self.text.char_to_position(index).unwrap_or(from)
    }

    pub(super) fn word_end_at(&self, from: Position) -> Position {
        let length = self.text.len_chars();
        let mut index = self.text.position_to_char(from).unwrap_or(0);
        if index >= length {
            return from;
        }

        while index < length && self.class_at(index) == Some(CharClass::Space) {
            index += 1;
        }
        if index >= length {
            return self.text.char_to_position(length).unwrap_or(from);
        }

        let class = self.class_at(index);
        while index + 1 < length && self.class_at(index + 1) == class {
            index += 1;
        }

        self.text.char_to_position(index).unwrap_or(from)
    }
}
