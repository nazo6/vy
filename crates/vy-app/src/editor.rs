//! Modal input on the client side.
//!
//! It holds the client's copy of the text, the cursor, the selection, and the mode.
//! Commands come from the keymap (`keymap.rs`) and build the `buffer/did_change`
//! payload for the core. See "Where state lives" in `docs/architecture.md` and the
//! key flow in `docs/ui.md`.
//!
//! Phase 1 covers Normal, Insert, and Select plus `h j k l w b e 0 $ G x d p P v i a .`
//! and counts. Yank and text objects come with the keymap in Phase 2.

use std::time::{Duration, Instant};

use vy_core::{EditError, Position, Range, Selection, Text, TextEdit};

use crate::commands::{BuiltinCommand, CommandArgs};

pub use crate::keymap::{Key, Mode};

/// Interval within which consecutive input is grouped into one undo entry.
pub const TRANSACTION_IDLE: Duration = Duration::from_millis(500);

/// One change to send to the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub transaction_id: u64,
    pub base_version: u64,
    pub edits: Vec<TextEdit>,
    pub selection_before: Vec<Selection>,
    pub selection_after: Vec<Selection>,
}

/// The result of handling a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Edit(Transaction),
    Undo,
    Redo,
}

/// A change that `.` can repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Repeat {
    /// Deletes `count` characters from the cursor.
    Delete { count: u32 },
    /// Inserts text the way an Insert session did.
    Insert { text: String },
    /// Repeats a paste with the same register content.
    Paste {
        text: String,
        kind: RegisterKind,
        before: bool,
    },
}

/// A command waiting for a motion or a text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Delete,
    Yank,
}

/// Whether a text object excludes or includes its delimiters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextObjectKind {
    Inner,
    Around,
}

/// A text object that can follow an operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextObject {
    Word,
    Paragraph,
    Pair(char, char),
    Quote(char),
}

/// Motions shared by normal movement and operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordForward,
    WordBackward,
    WordEnd,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// How register text is pasted back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum RegisterKind {
    #[default]
    Charwise,
    Linewise,
}

#[derive(Debug, Clone, Default)]
struct Register {
    text: String,
    kind: RegisterKind,
}

/// Character classes used by word motions and the word text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punctuation,
}

/// A range an operator will change.
#[derive(Debug, Clone, Copy)]
struct OperatorRange {
    start: Position,
    end: Position,
    linewise: bool,
}

fn motion_of(command: BuiltinCommand) -> Option<Motion> {
    match command {
        BuiltinCommand::MoveLeft => Some(Motion::Left),
        BuiltinCommand::MoveRight => Some(Motion::Right),
        BuiltinCommand::MoveUp => Some(Motion::Up),
        BuiltinCommand::MoveDown => Some(Motion::Down),
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

fn combined_count(left: Option<u32>, right: Option<u32>) -> Option<u32> {
    match (left, right) {
        (None, count) => count,
        (count, None) => count,
        (Some(left), Some(right)) => Some(left.saturating_mul(right)),
    }
}

fn is_operator_command(command: BuiltinCommand) -> bool {
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

fn is_text_object_command(command: BuiltinCommand) -> bool {
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

pub struct Editor {
    text: Text,
    mode: Mode,
    anchor: Position,
    head: Position,
    version: u64,
    next_transaction_id: u64,
    last_edit: Option<Instant>,
    /// Idle interval that splits undo transactions.
    transaction_idle: Duration,
    /// Register chosen for the current command by `args.register`.
    active_register: Option<usize>,
    /// Last delete or yank. It is also the unnamed register.
    unnamed_register: Register,
    /// Registers `a` to `z`.
    named_registers: [Register; 26],
    /// Operator waiting for a motion or text object (`d`, `y`).
    operator: Option<Operator>,
    /// Count typed before the operator, applied to the motion.
    operator_count: Option<u32>,
    /// Text object waiting for its key (`i` or `a` after an operator).
    text_object: Option<TextObjectKind>,
    /// Last change, replayed by `.`.
    last_change: Option<Repeat>,
    /// Text inserted in the current Insert session, replayed by `.`.
    insert_buffer: String,
    /// Column kept across vertical moves. Updated by horizontal moves and dropped by others.
    preferred_character: Option<u32>,
}

impl Editor {
    pub fn new() -> Self {
        Self::with_text("", 0)
    }

    pub fn with_text(text: impl AsRef<str>, version: u64) -> Self {
        let mut editor = Self {
            text: Text::new(text),
            mode: Mode::Normal,
            anchor: Position::new(0, 0),
            head: Position::new(0, 0),
            version,
            next_transaction_id: 0,
            last_edit: None,
            transaction_idle: TRANSACTION_IDLE,
            active_register: None,
            unnamed_register: Register::default(),
            named_registers: std::array::from_fn(|_| Register::default()),
            operator: None,
            operator_count: None,
            text_object: None,
            last_change: None,
            insert_buffer: String::new(),
            preferred_character: None,
        };
        editor.select_one_char(Position::new(0, 0));
        editor
    }

    /// Replaces the text and version with a snapshot or a resync result.
    pub fn reset(&mut self, text: impl AsRef<str>, version: u64) {
        self.text = Text::new(text);
        self.version = version;
        self.mode = Mode::Normal;
        self.last_edit = None;
        self.insert_buffer.clear();
        self.preferred_character = None;
        self.select_one_char(Position::new(0, 0));
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// Sets the idle interval that starts a new undo transaction.
    pub fn set_transaction_idle(&mut self, idle: Duration) {
        self.transaction_idle = idle;
    }

    pub fn line_count(&self) -> usize {
        self.text.line_count()
    }

    pub fn line(&self, line: usize) -> Option<String> {
        self.text.line_string(line)
    }

    /// Total length of the text in UTF-16 code units.
    pub fn len_utf16(&self) -> usize {
        self.text.len_utf16()
    }

    /// Converts a position to a flat UTF-16 offset from the start of the text.
    pub fn utf16_offset(&self, position: Position) -> usize {
        self.text.utf16_offset(position).unwrap_or_default()
    }

    /// Converts a flat UTF-16 offset to a position. The offset is clamped.
    pub fn position_at_utf16(&self, offset: usize) -> Position {
        self.text.position_at_utf16(offset)
    }

    /// Returns the text in a flat UTF-16 offset range. The range is clamped.
    pub fn text_utf16(&self, range: std::ops::Range<usize>) -> String {
        self.text.slice_utf16(range)
    }

    #[cfg(test)]
    pub fn text(&self) -> String {
        self.text.as_string()
    }

    /// Text of the unnamed register. Only used by tests.
    #[cfg(test)]
    pub fn register_text(&self) -> &str {
        &self.register().text
    }

    /// Cursor used for drawing. The selection start in Normal, otherwise the end.
    pub fn cursor(&self) -> Position {
        self.current()
    }

    /// The selection range. Empty in Insert.
    pub fn selection(&self) -> Range {
        let (start, end) = self.ordered();
        Range::new(start, end)
    }

    /// Whether `d` or `y` is waiting for a motion or text object.
    pub fn operator_pending(&self) -> bool {
        self.operator.is_some()
    }

    /// Whether `i` or `a` after an operator is waiting for the object key.
    pub fn text_object_pending(&self) -> bool {
        self.text_object.is_some()
    }

    /// Runs a built-in command. The keymap supplies the count and register.
    pub fn execute(&mut self, command: BuiltinCommand, args: CommandArgs) -> Action {
        let count = args.count.unwrap_or(1).max(1);
        self.active_register = args.register.and_then(register_index);

        // A pending operator consumes the next motion.
        if let Some(operator) = self.operator {
            if let Some(motion) = motion_of(command) {
                let action = self.apply_operator_motion(operator, motion, args.count);
                self.active_register = None;
                return action;
            }
            if !is_operator_command(command) {
                self.clear_operator();
            }
        }

        if self.text_object.is_some() && !is_text_object_command(command) {
            let cancel = matches!(
                command,
                BuiltinCommand::OperatorDelete | BuiltinCommand::OperatorYank
            );
            self.clear_operator();
            if cancel {
                self.active_register = None;
                return Action::None;
            }
        }

        let action = match command {
            BuiltinCommand::MoveLeft => {
                self.apply_motion(Motion::Left, args.count);
                Action::None
            }
            BuiltinCommand::MoveRight => {
                self.apply_motion(Motion::Right, args.count);
                Action::None
            }
            BuiltinCommand::MoveUp => {
                self.apply_motion(Motion::Up, args.count);
                Action::None
            }
            BuiltinCommand::MoveDown => {
                self.apply_motion(Motion::Down, args.count);
                Action::None
            }
            BuiltinCommand::WordForward => {
                self.apply_motion(Motion::WordForward, args.count);
                Action::None
            }
            BuiltinCommand::WordBackward => {
                self.apply_motion(Motion::WordBackward, args.count);
                Action::None
            }
            BuiltinCommand::WordEnd => {
                self.apply_motion(Motion::WordEnd, args.count);
                Action::None
            }
            BuiltinCommand::LineStart => {
                self.apply_motion(Motion::LineStart, args.count);
                Action::None
            }
            BuiltinCommand::LineEnd => {
                self.apply_motion(Motion::LineEnd, args.count);
                Action::None
            }
            BuiltinCommand::DocumentStart => {
                self.apply_motion(Motion::DocumentStart, args.count);
                Action::None
            }
            BuiltinCommand::DocumentEnd => {
                self.apply_motion(Motion::DocumentEnd, args.count);
                Action::None
            }
            BuiltinCommand::Delete => {
                if self.mode == Mode::Select {
                    self.delete_selection()
                } else {
                    self.delete_charwise(count)
                }
            }
            BuiltinCommand::PasteAfter => self.paste(count, false),
            BuiltinCommand::PasteBefore => self.paste(count, true),
            BuiltinCommand::Repeat => self.repeat_last(count),
            BuiltinCommand::InsertHere => {
                let start = self.ordered().0;
                self.begin_insert(start, start);
                Action::None
            }
            BuiltinCommand::InsertAfter => {
                let (_, end) = self.ordered();
                self.begin_insert(end, end);
                Action::None
            }
            BuiltinCommand::SelectMode => {
                self.set_mode(Mode::Select);
                Action::None
            }
            BuiltinCommand::NormalMode => {
                self.clear_operator();
                if self.mode == Mode::Insert {
                    self.finish_insert();
                    self.set_mode(Mode::Normal);
                    self.select_near_caret();
                } else {
                    self.set_mode(Mode::Normal);
                    self.select_one_char(self.current());
                }
                Action::None
            }
            BuiltinCommand::Undo => Action::Undo,
            BuiltinCommand::Redo => Action::Redo,
            BuiltinCommand::InsertNewline => self.insert_text("\n"),
            BuiltinCommand::InsertTab => self.insert_text("\t"),
            BuiltinCommand::InsertBackspace => self.delete_before_caret(),
            BuiltinCommand::InsertLeft => {
                self.move_horizontal(-1);
                Action::None
            }
            BuiltinCommand::InsertRight => {
                self.move_horizontal(1);
                Action::None
            }
            BuiltinCommand::InsertUp => {
                self.move_vertical(-1);
                Action::None
            }
            BuiltinCommand::InsertDown => {
                self.move_vertical(1);
                Action::None
            }
            BuiltinCommand::InsertHome => {
                self.move_to(Position::new(self.head.line, 0));
                Action::None
            }
            BuiltinCommand::InsertEnd => {
                let length = self
                    .text
                    .line_len_utf16(self.head.line as usize)
                    .unwrap_or(0);
                self.move_to(Position::new(self.head.line, length));
                Action::None
            }
            BuiltinCommand::OperatorDelete => self.start_operator(Operator::Delete, args.count),
            BuiltinCommand::OperatorYank => self.start_operator(Operator::Yank, args.count),
            BuiltinCommand::YankLine => self.yank_lines(count),
            BuiltinCommand::OperatorInner => self.begin_text_object(TextObjectKind::Inner),
            BuiltinCommand::OperatorAround => self.begin_text_object(TextObjectKind::Around),
            BuiltinCommand::TextObjectWord => self.apply_text_object(TextObject::Word),
            BuiltinCommand::TextObjectParagraph => self.apply_text_object(TextObject::Paragraph),
            BuiltinCommand::TextObjectParen => self.apply_text_object(TextObject::Pair('(', ')')),
            BuiltinCommand::TextObjectBracket => self.apply_text_object(TextObject::Pair('[', ']')),
            BuiltinCommand::TextObjectBrace => self.apply_text_object(TextObject::Pair('{', '}')),
            BuiltinCommand::TextObjectAngle => self.apply_text_object(TextObject::Pair('<', '>')),
            BuiltinCommand::TextObjectDoubleQuote => self.apply_text_object(TextObject::Quote('"')),
            BuiltinCommand::TextObjectSingleQuote => {
                self.apply_text_object(TextObject::Quote('\''))
            }
            BuiltinCommand::TextObjectBacktick => self.apply_text_object(TextObject::Quote('`')),
        };

        self.active_register = None;
        action
    }

    fn clear_operator(&mut self) {
        self.operator = None;
        self.operator_count = None;
        self.text_object = None;
    }

    fn apply_motion(&mut self, motion: Motion, count: Option<u32>) {
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

    fn start_operator(&mut self, operator: Operator, count: Option<u32>) -> Action {
        if self.mode == Mode::Select {
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

    fn begin_text_object(&mut self, kind: TextObjectKind) -> Action {
        if self.operator.is_some() {
            self.text_object = Some(kind);
        }
        Action::None
    }

    fn apply_text_object(&mut self, object: TextObject) -> Action {
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

    fn text_object_range(&self, object: TextObject, kind: TextObjectKind) -> Option<OperatorRange> {
        match object {
            TextObject::Word => self.word_object(kind),
            TextObject::Paragraph => self.paragraph_object(kind),
            TextObject::Pair(open, close) => self.pair_object(open, close, kind),
            TextObject::Quote(quote) => self.quote_object(quote, kind),
        }
    }

    fn word_object(&self, kind: TextObjectKind) -> Option<OperatorRange> {
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

    fn paragraph_object(&self, kind: TextObjectKind) -> Option<OperatorRange> {
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

    fn pair_object(&self, open: char, close: char, kind: TextObjectKind) -> Option<OperatorRange> {
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

    fn quote_object(&self, quote: char, kind: TextObjectKind) -> Option<OperatorRange> {
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

    fn linewise_range(&self, first: u32, last: u32) -> OperatorRange {
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

    fn motion_range(&self, motion: Motion, count: Option<u32>) -> Option<OperatorRange> {
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
            Motion::DocumentStart => Some(self.linewise_range(0, start.line)),
            Motion::DocumentEnd => Some(self.linewise_range(start.line, self.last_line())),
        }
    }

    fn apply_operator_motion(
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

    fn apply_operator_range(&mut self, operator: Operator, range: OperatorRange) -> Action {
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

    fn delete_lines(&mut self, count: u32) -> Action {
        let line = self.current().line;
        let last = (line + count.saturating_sub(1)).min(self.last_line());
        let range = self.linewise_range(line, last);
        self.apply_operator_range(Operator::Delete, range)
    }

    fn yank_lines(&mut self, count: u32) -> Action {
        let line = self.current().line;
        let last = (line + count.saturating_sub(1)).min(self.last_line());
        let range = self.linewise_range(line, last);
        self.apply_operator_range(Operator::Yank, range)
    }

    fn yank_selection(&mut self) -> Action {
        let (start, end) = self.ordered();
        if start == end {
            return Action::None;
        }

        let text = self.text.slice(Range::new(start, end)).unwrap_or_default();
        self.store_register(&text, RegisterKind::Charwise);
        self.set_mode(Mode::Normal);
        self.select_one_char(start);
        Action::None
    }

    fn position_of(&self, index: usize) -> Position {
        self.text
            .char_to_position(index.min(self.text.len_chars()))
            .unwrap_or(Position::new(0, 0))
    }

    /// Inserts text committed by the input method. Only Insert mode accepts it.
    pub fn insert_commit(&mut self, text: &str) -> Action {
        if self.mode != Mode::Insert {
            return Action::None;
        }

        self.insert_text(text)
    }

    /// Applies the undo or redo result returned by the core.
    ///
    /// The core sends ordered batches, each using the coordinate space after the
    /// previous batch. A failure leaves the mirror and the version untouched; the
    /// caller reports it and waits for `buffer/resync`.
    pub fn apply_history(
        &mut self,
        batches: &[Vec<TextEdit>],
        version: u64,
        selection: &[Selection],
    ) -> Result<(), EditError> {
        let mut text = self.text.clone();
        for batch in batches {
            text.apply(batch)?;
        }
        self.text = text;

        self.version = version;
        self.last_edit = None;
        self.insert_buffer.clear();
        self.mode = Mode::Normal;

        if let Some(selection) = selection.first().copied() {
            self.anchor = self.clamp(selection.anchor);
            self.head = self.clamp(selection.head);
        } else {
            let cursor = self.cursor();
            self.select_one_char(cursor);
        }

        Ok(())
    }

    fn insert_text(&mut self, value: &str) -> Action {
        let at = self.head;
        let edit = TextEdit::new(Range::empty(at), value);
        let new_head = self.advance(at, value);
        self.insert_buffer.push_str(value);
        self.commit(vec![edit], new_head, Mode::Insert)
    }

    fn delete_before_caret(&mut self) -> Action {
        let caret = self.head;
        let Some(previous) = self.previous_char(caret) else {
            return Action::None;
        };
        let edit = TextEdit::delete(Range::new(previous, caret));
        self.insert_buffer.pop();
        self.commit(vec![edit], previous, Mode::Insert)
    }

    /// Enters Insert mode with the caret collapsed at `at`.
    fn begin_insert(&mut self, anchor: Position, at: Position) {
        self.insert_buffer.clear();
        self.set_mode(Mode::Insert);
        self.anchor = anchor;
        self.head = at;
    }

    /// Ends the Insert session and remembers its text for `.`.
    fn finish_insert(&mut self) {
        if !self.insert_buffer.is_empty() {
            self.last_change = Some(Repeat::Insert {
                text: std::mem::take(&mut self.insert_buffer),
            });
        }
        self.insert_buffer.clear();
    }

    /// Deletes `count` characters from the cursor and stores them in a register.
    fn delete_charwise(&mut self, count: u32) -> Action {
        let action = self.delete_forward(count);
        if matches!(action, Action::Edit(_)) {
            self.last_change = Some(Repeat::Delete { count });
        }
        action
    }

    /// Deletes `count` characters without touching the repeat history.
    fn delete_forward(&mut self, count: u32) -> Action {
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

    fn delete_selection(&mut self) -> Action {
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
        let mode = if self.mode == Mode::Select {
            Mode::Normal
        } else {
            self.mode
        };
        self.commit(vec![edit], start, mode)
    }

    /// Pastes the selected register text after the cursor, or before with `before`.
    fn paste(&mut self, count: u32, before: bool) -> Action {
        let register = self.register().clone();
        if register.text.is_empty() {
            return Action::None;
        }
        self.paste_text(&register.text, register.kind, before, count)
    }

    fn paste_text(&mut self, text: &str, kind: RegisterKind, before: bool, count: u32) -> Action {
        self.last_change = Some(Repeat::Paste {
            text: text.to_string(),
            kind,
            before,
        });

        match kind {
            RegisterKind::Charwise => {
                let (start, end) = self.ordered();
                let at = if before { start } else { end };
                self.insert_at(at, &text.repeat(count as usize))
            }
            RegisterKind::Linewise => {
                let line = self.current().line;
                if before {
                    let at = Position::new(line, 0);
                    let payload = format!("{text}\n").repeat(count as usize);
                    self.insert_at_with_cursor(at, &payload, at)
                } else if line + 1 < self.text.line_count() as u32 {
                    let at = Position::new(line + 1, 0);
                    let payload = format!("{text}\n").repeat(count as usize);
                    self.insert_at_with_cursor(at, &payload, at)
                } else {
                    let length = self.text.line_len_utf16(line as usize).unwrap_or_default();
                    let at = Position::new(line, length);
                    let payload = format!("\n{text}").repeat(count as usize);
                    let cursor = Position::new(line + 1, 0);
                    self.insert_at_with_cursor(at, &payload, cursor)
                }
            }
        }
    }

    /// Repeats the last change `count` times.
    fn repeat_last(&mut self, count: u32) -> Action {
        match self.last_change.clone() {
            Some(Repeat::Delete { count: deleted }) => {
                self.delete_forward(deleted.saturating_mul(count))
            }
            Some(Repeat::Insert { text }) => {
                let at = self.ordered().0;
                self.insert_at(at, &text.repeat(count as usize))
            }
            Some(Repeat::Paste { text, kind, before }) => {
                self.paste_text(&text, kind, before, count)
            }
            None => Action::None,
        }
    }

    /// Inserts text at a position and leaves the cursor on the last inserted character.
    fn insert_at(&mut self, at: Position, text: &str) -> Action {
        if text.is_empty() {
            return Action::None;
        }

        let new_head = self.advance(at, text);
        // The cursor ends on the last inserted character, or at the line start after a
        // trailing line break.
        let last = text.chars().next_back().unwrap_or(' ');
        let cursor = match last {
            '\n' => new_head,
            _ => Position::new(
                new_head.line,
                new_head.character.saturating_sub(last.len_utf16() as u32),
            ),
        };

        self.insert_at_with_cursor(at, text, cursor)
    }

    /// Inserts text and places the cursor at a caller-chosen position.
    fn insert_at_with_cursor(&mut self, at: Position, text: &str, cursor: Position) -> Action {
        if text.is_empty() {
            return Action::None;
        }

        self.commit(
            vec![TextEdit::new(Range::empty(at), text)],
            cursor,
            Mode::Normal,
        )
    }

    fn store_register(&mut self, text: &str, kind: RegisterKind) {
        let register = Register {
            text: text.to_string(),
            kind,
        };
        self.unnamed_register = register.clone();

        if let Some(index) = self.active_register {
            self.named_registers[index] = register;
        }
    }

    fn register(&self) -> &Register {
        match self.active_register {
            Some(index) => &self.named_registers[index],
            None => &self.unnamed_register,
        }
    }

    /// Returns the position `count` characters after `from`, stopping at the end.
    fn position_forward(&self, from: Position, count: u32) -> Position {
        let index = self.text.position_to_char(from).unwrap_or(0);
        let target = (index + count as usize).min(self.text.len_chars());
        self.text.char_to_position(target).unwrap_or(from)
    }

    /// Number of characters in a range.
    fn char_count(&self, range: Range) -> u32 {
        let start = self.text.position_to_char(range.start).unwrap_or(0);
        let end = self.text.position_to_char(range.end).unwrap_or(start);
        end.saturating_sub(start).min(u32::MAX as usize) as u32
    }

    /// Applies the change to the mirror and builds the transaction for the core.
    fn commit(&mut self, edits: Vec<TextEdit>, new_head: Position, mode: Mode) -> Action {
        let selection_before = vec![Selection {
            anchor: self.anchor,
            head: self.head,
        }];
        let base_version = self.version;
        let transaction_id = self.next_transaction_id();

        if let Err(error) = self.text.apply(&edits) {
            eprintln!("vy: could not apply the edit: {error}");
            return Action::None;
        }

        self.version += 1;
        self.last_edit = Some(Instant::now());
        self.mode = mode;
        self.preferred_character = None;

        match mode {
            Mode::Insert => {
                self.anchor = new_head;
                self.head = new_head;
            }
            Mode::Normal => self.select_one_char(new_head),
            Mode::Select => {
                self.head = self.clamp(new_head);
            }
        }

        Action::Edit(Transaction {
            transaction_id,
            base_version,
            edits,
            selection_before,
            selection_after: vec![Selection {
                anchor: self.anchor,
                head: self.head,
            }],
        })
    }

    fn next_transaction_id(&mut self) -> u64 {
        let stale = self
            .last_edit
            .is_none_or(|at| at.elapsed() > self.transaction_idle);

        if stale {
            self.next_transaction_id += 1;
        }

        self.next_transaction_id.max(1)
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            // A mode change starts a new undo entry for the next edit.
            self.last_edit = None;
        }
    }

    /// Current position: the selection start in Normal, the end in Insert and Select.
    fn current(&self) -> Position {
        match self.mode {
            Mode::Normal => self.ordered().0,
            _ => self.head,
        }
    }

    fn ordered(&self) -> (Position, Position) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    fn move_to(&mut self, position: Position) {
        self.preferred_character = None;
        self.move_to_inner(position);
    }

    fn move_to_inner(&mut self, position: Position) {
        let position = self.clamp(position);
        match self.mode {
            Mode::Normal => self.select_one_char(position),
            Mode::Select => self.head = position,
            Mode::Insert => {
                self.anchor = position;
                self.head = position;
            }
        }
    }

    fn move_horizontal(&mut self, delta: isize) {
        let current = self.current();
        let index = self.text.position_to_char(current).unwrap_or(0);
        let target = if delta < 0 {
            index.saturating_sub(1)
        } else {
            (index + 1).min(self.text.len_chars())
        };
        let position = self.text.char_to_position(target).unwrap_or(current);
        self.preferred_character = Some(position.character);
        self.move_to_inner(position);
    }

    fn move_vertical(&mut self, delta: i64) {
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
        self.move_to_inner(position);
    }

    /// Selects one character at the cursor. The selection is empty at the end of the text.
    fn select_one_char(&mut self, at: Position) {
        let at = self.clamp(at);
        self.anchor = at;
        self.head = self.next_char(at).unwrap_or(at);
    }

    /// When leaving Insert, selects the character right before the caret.
    fn select_near_caret(&mut self) {
        let caret = self.clamp(self.head);
        let index = self.text.position_to_char(caret).unwrap_or(0);

        if index >= self.text.len_chars()
            && let Some(previous) = self.previous_char(caret)
        {
            self.select_one_char(previous);
            return;
        }

        self.select_one_char(caret);
    }

    fn clamp(&self, position: Position) -> Position {
        let line = position.line.min(self.last_line());
        let length = self.text.line_len_utf16(line as usize).unwrap_or_default();
        Position::new(line, position.character.min(length))
    }

    fn last_line(&self) -> u32 {
        self.text.line_count().saturating_sub(1) as u32
    }

    fn line_end(&self, line: u32) -> Position {
        let length = self.text.line_len_utf16(line as usize).unwrap_or(0);
        Position::new(line, length.saturating_sub(1))
    }

    fn next_char(&self, at: Position) -> Option<Position> {
        let index = self.text.position_to_char(at).ok()?;
        self.text.char_to_position(index + 1).ok()
    }

    fn previous_char(&self, at: Position) -> Option<Position> {
        let index = self.text.position_to_char(at).ok()?;
        if index == 0 {
            return None;
        }
        self.text.char_to_position(index - 1).ok()
    }

    /// Returns the position right after inserted text.
    fn advance(&self, from: Position, value: &str) -> Position {
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

    fn class_at(&self, index: usize) -> Option<CharClass> {
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

    fn word_forward_at(&self, from: Position) -> Position {
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

    fn word_backward_at(&self, from: Position) -> Position {
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

    fn word_end_at(&self, from: Position) -> Position {
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

/// Maps a register name `a` to `z` to an index. Other characters name no register.
fn register_index(character: char) -> Option<usize> {
    character
        .is_ascii_lowercase()
        .then(|| (character as u8 - b'a') as usize)
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{KeyContext, Keymap, KeymapOutcome};

    fn at(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    /// Runs keys through the default keymap and the editor, like `vy-app` does.
    struct Harness {
        editor: super::Editor,
        keymap: Keymap,
    }

    /// The tests below use `Editor` as the harness so they exercise the keymap too.
    type Editor = Harness;

    impl Harness {
        fn with_text(text: impl AsRef<str>, version: u64) -> Self {
            Self {
                editor: super::Editor::with_text(text, version),
                keymap: Keymap::load_default(Duration::from_millis(500)).expect("keymap"),
            }
        }

        fn context(&self) -> KeyContext {
            let selection = self.editor.selection();
            KeyContext {
                mode: self.editor.mode(),
                has_selection: selection.start != selection.end,
                operator_pending: self.editor.operator_pending(),
                text_object_pending: self.editor.text_object_pending(),
                ..KeyContext::default()
            }
        }

        fn handle_key(&mut self, key: Key) -> Action {
            if self.editor.mode() == Mode::Insert
                && let Key::Char(character) = key
            {
                let mut buffer = [0u8; 4];
                return self
                    .editor
                    .insert_commit(character.encode_utf8(&mut buffer));
            }

            match self.keymap.feed(key, &self.context()) {
                KeymapOutcome::Command { id, args } => {
                    let command = BuiltinCommand::parse(&id).expect("built-in command");
                    self.editor.execute(command, args)
                }
                KeymapOutcome::Pending | KeymapOutcome::Ignored => Action::None,
            }
        }
    }

    impl std::ops::Deref for Harness {
        type Target = super::Editor;

        fn deref(&self) -> &Self::Target {
            &self.editor
        }
    }

    impl std::ops::DerefMut for Harness {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.editor
        }
    }

    fn typed(editor: &mut Harness, text: &str) -> Vec<Transaction> {
        let mut transactions = Vec::new();
        for character in text.chars() {
            if let Action::Edit(transaction) = editor.handle_key(Key::Char(character)) {
                transactions.push(transaction);
            }
        }
        transactions
    }

    fn last_edit(editor: &mut Harness, key: Key) -> Transaction {
        match editor.handle_key(key) {
            Action::Edit(transaction) => transaction,
            other => panic!("expected an edit: {other:?}"),
        }
    }

    fn press(editor: &mut Harness, keys: &str) {
        for character in keys.chars() {
            editor.handle_key(Key::Char(character));
        }
    }

    #[test]
    fn starts_in_normal_mode_with_one_character_selected() {
        let editor = Editor::with_text("hello", 3);
        assert_eq!(editor.mode(), Mode::Normal);
        assert_eq!(editor.cursor(), at(0, 0));
        assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 1)));
        assert_eq!(editor.version(), 3);
    }

    #[test]
    fn inserts_text_in_insert_mode() {
        let mut editor = Editor::with_text("abc", 0);
        assert_eq!(editor.handle_key(Key::Char('i')), Action::None);
        assert_eq!(editor.mode(), Mode::Insert);

        let transactions = typed(&mut editor, "XY");
        assert_eq!(editor.text(), "XYabc");
        assert_eq!(editor.cursor(), at(0, 2));
        assert_eq!(editor.version(), 2);

        assert_eq!(transactions[0].base_version, 0);
        assert_eq!(transactions[0].edits.len(), 1);
        assert_eq!(transactions[0].edits[0].text, "X");
        assert_eq!(transactions[1].base_version, 1);
    }

    #[test]
    fn groups_consecutive_typing_into_one_transaction() {
        let mut editor = Editor::with_text("", 0);
        editor.handle_key(Key::Char('i'));
        let transactions = typed(&mut editor, "abc");

        assert_eq!(
            transactions[0].transaction_id,
            transactions[1].transaction_id
        );
        assert_eq!(
            transactions[1].transaction_id,
            transactions[2].transaction_id
        );

        // A mode change starts a new entry for the next edit.
        editor.handle_key(Key::Esc);
        editor.handle_key(Key::Char('i'));
        let next = last_edit(&mut editor, Key::Char('d'));
        assert_ne!(next.transaction_id, transactions[0].transaction_id);
    }

    #[test]
    fn deletes_a_character_with_x() {
        let mut editor = Editor::with_text("あ😀い", 0);
        editor.handle_key(Key::Char('l'));
        // Moves one character even for a surrogate pair; the selection is 2 UTF-16 units.
        assert_eq!(editor.selection(), Range::new(at(0, 1), at(0, 3)));

        // First delete the leading character.
        let mut editor = Editor::with_text("あ😀い", 0);
        let deleted = last_edit(&mut editor, Key::Char('x'));
        assert_eq!(deleted.edits[0].range, Range::new(at(0, 0), at(0, 1)));
        assert_eq!(editor.text(), "😀い");

        // The cursor stays at the deletion point.
        assert_eq!(editor.cursor(), at(0, 0));
        assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 2)));
    }

    #[test]
    fn selects_and_deletes_with_v_and_d() {
        let mut editor = Editor::with_text("abcdef", 0);
        editor.handle_key(Key::Char('v'));
        assert_eq!(editor.mode(), Mode::Select);

        editor.handle_key(Key::Char('l'));
        editor.handle_key(Key::Char('l'));
        assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 3)));

        let transaction = last_edit(&mut editor, Key::Char('d'));
        assert_eq!(transaction.edits[0].range, Range::new(at(0, 0), at(0, 3)));
        assert_eq!(editor.text(), "def");
        assert_eq!(editor.mode(), Mode::Normal);
        assert_eq!(editor.cursor(), at(0, 0));
    }

    #[test]
    fn moves_by_word() {
        let mut editor = Editor::with_text("foo bar baz", 0);

        editor.handle_key(Key::Char('w'));
        assert_eq!(editor.cursor(), at(0, 4));
        editor.handle_key(Key::Char('w'));
        assert_eq!(editor.cursor(), at(0, 8));
        editor.handle_key(Key::Char('b'));
        assert_eq!(editor.cursor(), at(0, 4));
        editor.handle_key(Key::Char('e'));
        assert_eq!(editor.cursor(), at(0, 6));
    }

    #[test]
    fn handles_gg_and_shift_g() {
        let mut editor = Editor::with_text("one\ntwo\nthree", 0);
        editor.handle_key(Key::Char('G'));
        assert_eq!(editor.cursor(), at(2, 0));

        editor.handle_key(Key::Char('g'));
        editor.handle_key(Key::Char('g'));
        assert_eq!(editor.cursor(), at(0, 0));
    }

    #[test]
    fn moves_vertically_and_clamps_the_column() {
        let mut editor = Editor::with_text("long line\nx\nthird", 0);
        editor.handle_key(Key::Char('$'));
        assert_eq!(editor.cursor(), at(0, 8));

        editor.handle_key(Key::Char('j'));
        assert_eq!(editor.cursor(), at(1, 0));
        editor.handle_key(Key::Char('k'));
        assert_eq!(editor.cursor(), at(0, 8));
    }

    #[test]
    fn backspace_deletes_the_previous_character() {
        let mut editor = Editor::with_text("あい", 0);
        editor.handle_key(Key::Char('a'));
        assert_eq!(editor.cursor(), at(0, 1));

        let transaction = last_edit(&mut editor, Key::Backspace);
        assert_eq!(transaction.edits[0].range, Range::new(at(0, 0), at(0, 1)));
        assert_eq!(editor.text(), "い");
        assert_eq!(editor.cursor(), at(0, 0));
    }

    #[test]
    fn enter_inserts_a_line_break() {
        let mut editor = Editor::with_text("ab", 0);
        editor.handle_key(Key::Char('i'));
        editor.handle_key(Key::Enter);
        assert_eq!(editor.text(), "\nab");
        assert_eq!(editor.cursor(), at(1, 0));
        assert_eq!(editor.line_count(), 2);
    }

    #[test]
    fn esc_leaves_insert_mode_and_selects_the_previous_character() {
        let mut editor = Editor::with_text("abc", 0);
        editor.handle_key(Key::Char('a'));
        editor.handle_key(Key::Esc);

        assert_eq!(editor.mode(), Mode::Normal);
        assert_eq!(editor.selection(), Range::new(at(0, 1), at(0, 2)));
    }

    #[test]
    fn applies_history_from_the_core() {
        let mut editor = Editor::with_text("aXYc", 1);
        let batches = vec![vec![TextEdit::new(Range::new(at(0, 1), at(0, 3)), "b")]];
        editor
            .apply_history(&batches, 2, &[Selection::caret(at(0, 1))])
            .expect("history");

        assert_eq!(editor.text(), "abc");
        assert_eq!(editor.version(), 2);
        assert_eq!(editor.cursor(), at(0, 1));
    }

    #[test]
    fn keeps_the_version_when_a_history_batch_fails() {
        let mut editor = Editor::with_text("abc", 1);
        let batches = vec![vec![TextEdit::new(Range::new(at(0, 5), at(0, 6)), "x")]];

        assert!(editor.apply_history(&batches, 2, &[]).is_err());
        assert_eq!(editor.text(), "abc");
        assert_eq!(editor.version(), 1);
    }

    #[test]
    fn reset_replaces_the_text_and_version() {
        let mut editor = Editor::with_text("old", 4);
        editor.reset("新しい\n本文", 9);

        assert_eq!(editor.text(), "新しい\n本文");
        assert_eq!(editor.version(), 9);
        assert_eq!(editor.mode(), Mode::Normal);
        assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 1)));
    }

    #[test]
    fn reports_undo_and_redo_keys() {
        let mut editor = Editor::with_text("abc", 0);
        assert_eq!(editor.handle_key(Key::Char('u')), Action::Undo);
        assert_eq!(editor.handle_key(Key::Ctrl('r')), Action::Redo);
    }

    #[test]
    fn inserts_committed_text_only_in_insert_mode() {
        let mut editor = Editor::with_text("ab", 0);
        assert_eq!(editor.insert_commit("x"), Action::None);
        assert_eq!(editor.text(), "ab");

        editor.handle_key(Key::Char('i'));
        let committed = editor.insert_commit("日本");
        let Action::Edit(transaction) = committed else {
            panic!("expected an edit: {committed:?}");
        };

        assert_eq!(editor.text(), "日本ab");
        assert_eq!(editor.cursor(), at(0, 2));
        assert_eq!(transaction.edits.len(), 1);
        assert_eq!(editor.version(), 1);
    }

    #[test]
    fn converts_positions_to_flat_utf16_offsets() {
        let editor = Editor::with_text("a😀b\n日本", 7);

        assert_eq!(editor.len_utf16(), 7);
        assert_eq!(editor.utf16_offset(at(0, 3)), 3);
        assert_eq!(editor.position_at_utf16(6), at(1, 1));
        assert_eq!(editor.text_utf16(4..7), "\n日本");
    }

    #[test]
    fn yanks_a_word_and_pastes_it() {
        let mut editor = Editor::with_text("foo bar", 0);

        press(&mut editor, "yw");
        assert_eq!(editor.register_text(), "foo ");

        press(&mut editor, "$");
        last_edit(&mut editor, Key::Char('p'));
        assert_eq!(editor.text(), "foo barfoo ");
    }

    #[test]
    fn deletes_and_pastes_whole_lines() {
        let mut editor = Editor::with_text("one\ntwo\nthree", 0);

        press(&mut editor, "dd");
        assert_eq!(editor.text(), "two\nthree");
        assert_eq!(editor.register_text(), "one");

        last_edit(&mut editor, Key::Char('p'));
        assert_eq!(editor.text(), "two\none\nthree");
        assert_eq!(editor.cursor(), at(1, 0));

        press(&mut editor, "yy");
        assert_eq!(editor.register_text(), "one");
    }

    #[test]
    fn deletes_text_objects() {
        let mut editor = Editor::with_text("foo bar", 0);
        press(&mut editor, "diw");
        assert_eq!(editor.text(), " bar");

        let mut editor = Editor::with_text("foo bar", 0);
        press(&mut editor, "daw");
        assert_eq!(editor.text(), "bar");

        let mut editor = Editor::with_text("say \"hello\" now", 0);
        for _ in 0..6 {
            editor.handle_key(Key::Char('l'));
        }
        press(&mut editor, "di\"");
        assert_eq!(editor.text(), "say \"\" now");
    }

    #[test]
    fn operator_motions_use_linewise_ranges() {
        let mut editor = Editor::with_text("a\nb\nc", 0);
        press(&mut editor, "dj");
        assert_eq!(editor.text(), "c");

        let mut editor = Editor::with_text("abc\ndef", 0);
        press(&mut editor, "d$");
        assert_eq!(editor.text(), "\ndef");
    }

    #[test]
    fn word_motions_stop_at_punctuation() {
        let mut editor = Editor::with_text("foo_bar.baz()", 0);

        press(&mut editor, "w");
        assert_eq!(editor.cursor(), at(0, 7));
        press(&mut editor, "w");
        assert_eq!(editor.cursor(), at(0, 8));
        press(&mut editor, "w");
        assert_eq!(editor.cursor(), at(0, 11));
    }

    #[test]
    fn escape_cancels_a_pending_operator() {
        let mut editor = Editor::with_text("abc", 0);
        press(&mut editor, "d");
        assert!(editor.operator_pending());

        editor.handle_key(Key::Esc);
        assert!(!editor.operator_pending());
        assert_eq!(editor.text(), "abc");
    }

    #[test]
    fn applies_counts_to_motions() {
        let mut editor = Editor::with_text("abcdef\nghijkl\nmnopqr", 0);

        press(&mut editor, "3l");
        assert_eq!(editor.cursor(), at(0, 3));

        press(&mut editor, "2j");
        assert_eq!(editor.cursor(), at(2, 3));

        press(&mut editor, "3gg");
        assert_eq!(editor.cursor(), at(2, 0));

        press(&mut editor, "2G");
        assert_eq!(editor.cursor(), at(1, 0));

        press(&mut editor, "1G");
        assert_eq!(editor.cursor(), at(0, 0));
    }

    #[test]
    fn counts_delete_and_leave_the_text_in_the_register() {
        let mut editor = Editor::with_text("abcdef", 0);

        editor.handle_key(Key::Char('3'));
        let transaction = last_edit(&mut editor, Key::Char('x'));
        assert_eq!(transaction.edits.len(), 1);
        assert_eq!(editor.text(), "def");

        // `p` inserts the unnamed register after the cursor.
        last_edit(&mut editor, Key::Char('p'));
        assert_eq!(editor.text(), "dabcef");
        assert_eq!(editor.cursor(), at(0, 3));
    }

    #[test]
    fn named_registers_keep_their_own_text() {
        let mut editor = Editor::with_text("bc\ndef", 0);

        press(&mut editor, "\"b");
        last_edit(&mut editor, Key::Char('x'));
        assert_eq!(editor.text(), "c\ndef");

        press(&mut editor, "j");
        assert_eq!(editor.cursor(), at(1, 0));

        press(&mut editor, "\"b");
        last_edit(&mut editor, Key::Char('P'));
        assert_eq!(editor.text(), "c\nbdef");
    }

    #[test]
    fn dot_repeats_the_last_delete() {
        let mut editor = Editor::with_text("abcdef", 0);

        editor.handle_key(Key::Char('2'));
        last_edit(&mut editor, Key::Char('x'));
        assert_eq!(editor.text(), "cdef");

        last_edit(&mut editor, Key::Char('.'));
        assert_eq!(editor.text(), "ef");
    }

    #[test]
    fn dot_replays_the_insert_session() {
        let mut editor = Editor::with_text("", 0);

        press(&mut editor, "i");
        typed(&mut editor, "ab");
        editor.handle_key(Key::Esc);
        assert_eq!(editor.text(), "ab");
        assert_eq!(editor.cursor(), at(0, 1));

        last_edit(&mut editor, Key::Char('.'));
        assert_eq!(editor.text(), "aabb");
    }

    #[test]
    fn dot_keeps_the_first_change_when_insert_is_empty() {
        let mut editor = Editor::with_text("abc", 0);

        last_edit(&mut editor, Key::Char('x'));
        press(&mut editor, "i");
        editor.handle_key(Key::Esc);
        last_edit(&mut editor, Key::Char('.'));
        assert_eq!(editor.text(), "c");
    }

    #[test]
    fn insert_mode_end_moves_past_the_last_character() {
        let mut editor = Editor::with_text("abc\nde", 0);

        press(&mut editor, "i");
        editor.handle_key(Key::End);
        assert_eq!(editor.cursor(), at(0, 3));

        typed(&mut editor, "X");
        assert_eq!(editor.text(), "abcX\nde");
    }
}
