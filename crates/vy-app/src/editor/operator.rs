//! Vim operators (delete, yank) and text objects.

use crate::commands::BuiltinCommand;
use crate::keymap::Mode;
use vy_core::{Position, Range, TextEdit};

use super::Editor;
use super::action::Action;
use super::motion::{CharClass, Motion, combined_count, motion_of};
use super::register::RegisterKind;
use super::repeat::Repeat;
/// A command waiting for a motion or a text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Operator {
    Delete,
    Yank,
}

/// Whether a text object excludes or includes its delimiters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextObjectKind {
    Inner,
    Around,
}

/// A text object that can follow an operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextObject {
    Word,
    Paragraph,
    Pair(char, char),
    Quote(char),
}

/// A range an operator will change.
#[derive(Debug, Clone, Copy)]
pub(super) struct OperatorRange {
    pub(super) start: Position,
    pub(super) end: Position,
    pub(super) linewise: bool,
}

pub(super) fn is_operator_command(command: BuiltinCommand) -> bool {
    motion_of(command).is_some()
        || is_text_object_command(command)
        || matches!(
            command,
            BuiltinCommand::OperatorDelete
                | BuiltinCommand::OperatorYank
                | BuiltinCommand::YankLine
                | BuiltinCommand::NormalMode
        )
}

pub(super) fn is_text_object_command(command: BuiltinCommand) -> bool {
    matches!(
        command,
        BuiltinCommand::OperatorInner
            | BuiltinCommand::OperatorAround
            | BuiltinCommand::NormalMode
            | BuiltinCommand::TextObjectWord
            | BuiltinCommand::TextObjectParagraph
            | BuiltinCommand::TextObjectParen
            | BuiltinCommand::TextObjectBracket
            | BuiltinCommand::TextObjectBrace
            | BuiltinCommand::TextObjectAngle
            | BuiltinCommand::TextObjectDoubleQuote
            | BuiltinCommand::TextObjectSingleQuote
            | BuiltinCommand::TextObjectBacktick
    )
}

impl Editor {
    pub(super) fn start_operator(&mut self, operator: Operator, count: Option<u32>) -> Action {
        if self.mode.is_select() {
            return match operator {
                Operator::Delete => self.delete_selection(),
                Operator::Yank => self.yank_selection(),
            };
        }

        if self.operator == Some(operator) {
            let times = combined_count(self.operator_count, count).unwrap_or(1);
            self.clear_operator();
            return match operator {
                Operator::Delete => self.delete_lines(times),
                Operator::Yank => self.yank_lines(times),
            };
        }

        self.operator = Some(operator);
        self.operator_count = count;
        self.text_object = None;
        Action::None
    }

    pub(super) fn begin_text_object(&mut self, kind: TextObjectKind) -> Action {
        if self.operator.is_some() {
            self.text_object = Some(kind);
        }
        Action::None
    }

    pub(super) fn apply_text_object(&mut self, object: TextObject) -> Action {
        let Some(kind) = self.text_object.take() else {
            return Action::None;
        };
        let Some(operator) = self.operator else {
            self.clear_operator();
            return Action::None;
        };
        let Some(range) = self.text_object_range(object, kind) else {
            self.clear_operator();
            return Action::None;
        };
        self.apply_operator_range(operator, range)
    }

    pub(super) fn text_object_range(
        &self,
        object: TextObject,
        kind: TextObjectKind,
    ) -> Option<OperatorRange> {
        match object {
            TextObject::Word => self.word_object(kind),
            TextObject::Paragraph => self.paragraph_object(kind),
            TextObject::Pair(open, close) => self.pair_object(open, close, kind),
            TextObject::Quote(quote) => self.quote_object(quote, kind),
        }
    }

    pub(super) fn word_object(&self, kind: TextObjectKind) -> Option<OperatorRange> {
        let index = self.text.position_to_char(self.current()).ok()?;
        let length = self.text.len_chars();
        if index >= length {
            return None;
        }

        let class = self.class_at(index)?;
        let mut start = index;
        while start > 0 && self.class_at(start - 1) == Some(class) {
            start -= 1;
        }
        let mut end = index + 1;
        while end < length && self.class_at(end) == Some(class) {
            end += 1;
        }

        if kind == TextObjectKind::Around {
            let mut extended = end;
            while extended < length
                && self.class_at(extended) == Some(CharClass::Space)
                && self.text.char_at_index(extended) != Some('\n')
            {
                extended += 1;
            }
            if extended == end {
                while start > 0
                    && self.class_at(start - 1) == Some(CharClass::Space)
                    && self.text.char_at_index(start - 1) != Some('\n')
                {
                    start -= 1;
                }
            } else {
                end = extended;
            }
        }

        Some(OperatorRange {
            start: self.position_of(start),
            end: self.position_of(end),
            linewise: false,
        })
    }

    pub(super) fn paragraph_object(&self, kind: TextObjectKind) -> Option<OperatorRange> {
        let line = self.current().line as usize;
        let line_count = self.text.line_count();
        let blank = |line: usize| {
            self.text
                .line_string(line)
                .is_some_and(|text| text.trim().is_empty())
        };
        let current_blank = blank(line);

        let mut first = line;
        while first > 0 && blank(first - 1) == current_blank {
            first -= 1;
        }
        let mut last = line;
        while last + 1 < line_count && blank(last + 1) == current_blank {
            last += 1;
        }
        if kind == TextObjectKind::Around && !current_blank {
            while last + 1 < line_count && blank(last + 1) {
                last += 1;
            }
        }

        Some(self.linewise_range(first as u32, last as u32))
    }

    pub(super) fn pair_object(
        &self,
        open: char,
        close: char,
        kind: TextObjectKind,
    ) -> Option<OperatorRange> {
        let index = self.text.position_to_char(self.current()).ok()?;
        let length = self.text.len_chars();

        let mut depth = 0usize;
        let mut opening = None;
        let mut cursor = (index + 1).min(length);
        while cursor > 0 {
            cursor -= 1;
            match self.text.char_at_index(cursor) {
                Some(character) if character == close => depth += 1,
                Some(character) if character == open => {
                    if depth == 0 {
                        opening = Some(cursor);
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }

        let opening = opening?;
        let mut depth = 0usize;
        let mut closing = None;
        let mut cursor = index;
        while cursor < length {
            match self.text.char_at_index(cursor) {
                Some(character) if character == open => depth += 1,
                Some(character) if character == close => {
                    if depth == 0 {
                        closing = Some(cursor);
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
            cursor += 1;
        }

        let closing = closing?;
        let (start, end) = match kind {
            TextObjectKind::Inner => (opening + 1, closing),
            TextObjectKind::Around => (opening, closing + 1),
        };
        Some(OperatorRange {
            start: self.position_of(start),
            end: self.position_of(end),
            linewise: false,
        })
    }

    pub(super) fn quote_object(&self, quote: char, kind: TextObjectKind) -> Option<OperatorRange> {
        let line = self.current().line as usize;
        let text = self.text.line_string(line)?;
        let line_start = self
            .text
            .position_to_char(Position::new(line as u32, 0))
            .ok()?;
        let cursor = self.text.position_to_char(self.current()).ok()?;

        let quotes: Vec<usize> = text
            .chars()
            .enumerate()
            .filter(|(_, character)| *character == quote)
            .map(|(column, _)| line_start + column)
            .collect();

        for pair in quotes.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let (opening, closing) = (pair[0], pair[1]);
            if opening <= cursor && cursor <= closing {
                let (start, end) = match kind {
                    TextObjectKind::Inner => (opening + 1, closing),
                    TextObjectKind::Around => (opening, closing + 1),
                };
                return Some(OperatorRange {
                    start: self.position_of(start),
                    end: self.position_of(end),
                    linewise: false,
                });
            }
        }
        None
    }

    pub(super) fn linewise_range(&self, first: u32, last: u32) -> OperatorRange {
        let last = last.min(self.last_line());
        let first = first.min(last);
        let end = if last + 1 < self.text.line_count() as u32 {
            Position::new(last + 1, 0)
        } else {
            Position::new(
                last,
                self.text.line_len_utf16(last as usize).unwrap_or_default(),
            )
        };

        OperatorRange {
            start: Position::new(first, 0),
            end,
            linewise: true,
        }
    }

    pub(super) fn apply_operator_motion(
        &mut self,
        operator: Operator,
        motion: Motion,
        count: Option<u32>,
    ) -> Action {
        let count = combined_count(self.operator_count, count);
        let Some(range) = self.motion_range(motion, count) else {
            self.clear_operator();
            return Action::None;
        };
        self.apply_operator_range(operator, range)
    }

    pub(super) fn apply_operator_range(
        &mut self,
        operator: Operator,
        range: OperatorRange,
    ) -> Action {
        self.clear_operator();

        let kind = if range.linewise {
            RegisterKind::Linewise
        } else {
            RegisterKind::Charwise
        };
        let text_range = Range::new(range.start, range.end);
        let text = self.text.slice(text_range).unwrap_or_default();
        let stored = match kind {
            RegisterKind::Linewise => text.trim_end_matches(['\r', '\n']).to_string(),
            RegisterKind::Charwise => text,
        };

        match operator {
            Operator::Yank => {
                self.store_register(&stored, kind);
                Action::None
            }
            Operator::Delete => {
                if range.start == range.end {
                    return Action::None;
                }
                self.store_register(&stored, kind);
                self.last_change = Some(Repeat::Delete {
                    count: self.char_count(text_range),
                });
                self.commit(
                    vec![TextEdit::delete(text_range)],
                    range.start,
                    Mode::Normal,
                )
            }
        }
    }

    pub(super) fn delete_lines(&mut self, count: u32) -> Action {
        let line = self.current().line;
        let last = (line + count.saturating_sub(1)).min(self.last_line());
        let range = self.linewise_range(line, last);
        self.apply_operator_range(Operator::Delete, range)
    }

    pub(super) fn yank_lines(&mut self, count: u32) -> Action {
        let line = self.current().line;
        let last = (line + count.saturating_sub(1)).min(self.last_line());
        let range = self.linewise_range(line, last);
        self.apply_operator_range(Operator::Yank, range)
    }

    /// Deletes `count` characters from the cursor and stores them in a register.
    pub(super) fn delete_charwise(&mut self, count: u32) -> Action {
        let action = self.delete_forward(count);
        if matches!(action, Action::Edit(_)) {
            self.last_change = Some(Repeat::Delete { count });
        }
        action
    }

    /// Deletes `count` characters without touching the repeat history.
    pub(super) fn delete_forward(&mut self, count: u32) -> Action {
        let start = self.ordered().0;
        let end = self.position_forward(start, count);
        if start == end {
            return Action::None;
        }

        let range = Range::new(start, end);
        let deleted = self.text.slice(range).unwrap_or_default();
        self.store_register(&deleted, RegisterKind::Charwise);
        self.commit(vec![TextEdit::delete(range)], start, Mode::Normal)
    }

    pub(super) fn delete_selection(&mut self) -> Action {
        match self.mode {
            Mode::SelectLine => {
                let first = self.anchor.line.min(self.head.line);
                let last = self.anchor.line.max(self.head.line);
                let range = self.linewise_range(first, last);
                self.apply_operator_range(Operator::Delete, range)
            }
            Mode::SelectBlock => self.delete_block(),
            _ => {
                let (start, end) = self.ordered();
                if start == end {
                    return Action::None;
                }

                let range = Range::new(start, end);
                let deleted = self.text.slice(range).unwrap_or_default();
                self.store_register(&deleted, RegisterKind::Charwise);
                self.last_change = Some(Repeat::Delete {
                    count: self.char_count(range),
                });

                let edit = TextEdit::delete(range);
                let mode = if self.mode.is_select() {
                    Mode::Normal
                } else {
                    self.mode
                };
                self.commit(vec![edit], start, mode)
            }
        }
    }

    pub(super) fn delete_block(&mut self) -> Action {
        let first_line = self.anchor.line.min(self.head.line);
        let last_line = self.anchor.line.max(self.head.line);
        let first_col = self.anchor.character.min(self.head.character);
        let last_col = self.anchor.character.max(self.head.character);

        let mut edits = Vec::new();
        let mut yanked_lines = Vec::new();

        for line in (first_line..=last_line).rev() {
            let length = self.text.line_len_utf16(line as usize).unwrap_or(0);
            if length > first_col {
                let end_col = (last_col + 1).min(length);
                let range =
                    Range::new(Position::new(line, first_col), Position::new(line, end_col));
                if let Ok(sliced) = self.text.slice(range) {
                    yanked_lines.push(sliced);
                }
                edits.push(TextEdit::delete(range));
            } else {
                yanked_lines.push(String::new());
            }
        }
        yanked_lines.reverse();
        let stored = yanked_lines.join("\n");
        self.store_register(&stored, RegisterKind::Charwise);

        let start = Position::new(first_line, first_col);
        self.commit(edits, start, Mode::Normal)
    }
}
