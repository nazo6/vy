//! Modal input on the client side.
//!
//! It holds the client's copy of the text, the cursor, the selection, and the mode.
//! It turns keys into meaningful actions and builds the `buffer/did_change` payload
//! for the core. See "Where state lives" in `docs/architecture.md` and the key flow
//! in `docs/ui.md`.
//!
//! Phase 1 covers Normal, Insert, and Select plus `h j k l w b e 0 $ G x d p P v i a .`
//! and counts. Yank and text objects come with the keymap in Phase 2.

use std::time::{Duration, Instant};

use vy_core::{Position, Range, Selection, Text, TextEdit};

/// Interval within which consecutive input is grouped into one undo entry.
pub const TRANSACTION_IDLE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Select,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Select => "SELECT",
        }
    }
}

/// A GPUI key event reduced to a testable form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Esc,
    Enter,
    Backspace,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

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
}

pub struct Editor {
    text: Text,
    mode: Mode,
    anchor: Position,
    head: Position,
    version: u64,
    next_transaction_id: u64,
    last_edit: Option<Instant>,
    pending_g: bool,
    /// Count typed before a command. [`None`] until a digit arrives.
    count: Option<u32>,
    /// `"` was typed and a register name follows.
    pending_register: bool,
    /// Register chosen with `"`. [`None`] is the unnamed register.
    active_register: Option<usize>,
    /// Text of the last delete. It is also the unnamed register.
    unnamed_register: String,
    /// Registers `a` to `z`.
    named_registers: [String; 26],
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
            pending_g: false,
            count: None,
            pending_register: false,
            active_register: None,
            unnamed_register: String::new(),
            named_registers: std::array::from_fn(|_| String::new()),
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
        self.pending_g = false;
        self.count = None;
        self.pending_register = false;
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

    /// Cursor used for drawing. The selection start in Normal, otherwise the end.
    pub fn cursor(&self) -> Position {
        self.current()
    }

    /// The selection range. Empty in Insert.
    pub fn selection(&self) -> Range {
        let (start, end) = self.ordered();
        Range::new(start, end)
    }

    pub fn handle_key(&mut self, key: Key) -> Action {
        match self.mode {
            Mode::Insert => self.handle_insert(key),
            Mode::Normal | Mode::Select => self.handle_normal(key),
        }
    }

    /// Inserts text committed by the input method. Only Insert mode accepts it.
    pub fn insert_commit(&mut self, text: &str) -> Action {
        if self.mode != Mode::Insert {
            return Action::None;
        }

        self.insert_text(text)
    }

    /// Applies the undo or redo result returned by the core.
    pub fn apply_history(&mut self, edits: &[TextEdit], version: u64, selection: &[Selection]) {
        if let Err(error) = self.text.apply(edits) {
            eprintln!("vy: could not apply the history edit: {error}");
            return;
        }

        self.version = version;
        self.last_edit = None;
        self.pending_g = false;
        self.count = None;
        self.insert_buffer.clear();
        self.mode = Mode::Normal;

        if let Some(selection) = selection.first().copied() {
            self.anchor = self.clamp(selection.anchor);
            self.head = self.clamp(selection.head);
        } else {
            let cursor = self.cursor();
            self.select_one_char(cursor);
        }
    }

    fn handle_insert(&mut self, key: Key) -> Action {
        match key {
            Key::Esc => {
                self.finish_insert();
                self.set_mode(Mode::Normal);
                self.select_near_caret();
                Action::None
            }
            Key::Enter => self.insert_text("\n"),
            Key::Tab => self.insert_text("\t"),
            Key::Backspace => self.delete_before_caret(),
            Key::Char(character) => {
                let mut buffer = [0u8; 4];
                self.insert_text(character.encode_utf8(&mut buffer))
            }
            Key::Left => {
                self.move_horizontal(-1);
                Action::None
            }
            Key::Right => {
                self.move_horizontal(1);
                Action::None
            }
            Key::Up => {
                self.move_vertical(-1);
                Action::None
            }
            Key::Down => {
                self.move_vertical(1);
                Action::None
            }
            Key::Home => {
                self.move_to(Position::new(self.head.line, 0));
                Action::None
            }
            Key::End => {
                self.move_to(self.line_end(self.head.line));
                Action::None
            }
            Key::Ctrl(_) => Action::None,
        }
    }

    fn handle_normal(&mut self, key: Key) -> Action {
        if self.pending_register {
            self.pending_register = false;
            if let Key::Char(character) = key {
                self.active_register = register_index(character);
            }
            return Action::None;
        }

        // Digits build a count. `0` is a motion until a count has started.
        if let Key::Char(character) = key
            && character.is_ascii_digit()
            && (character != '0' || self.count.is_some())
        {
            let digit = character.to_digit(10).unwrap_or(0);
            let count = self
                .count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit);
            self.count = Some(count);
            return Action::None;
        }

        if self.pending_g {
            self.pending_g = false;
            if key == Key::Char('g') {
                let line = self.take_count().saturating_sub(1);
                self.move_to(Position::new(line.min(self.last_line()), 0));
            }
            return Action::None;
        }

        match key {
            Key::Char('g') => {
                self.pending_g = true;
                Action::None
            }
            Key::Char('"') => {
                self.pending_register = true;
                Action::None
            }
            Key::Char('h') | Key::Left => {
                for _ in 0..self.take_count() {
                    self.move_horizontal(-1);
                }
                Action::None
            }
            Key::Char('l') | Key::Right => {
                for _ in 0..self.take_count() {
                    self.move_horizontal(1);
                }
                Action::None
            }
            Key::Char('j') | Key::Down => {
                let count = self.take_count() as i64;
                self.move_vertical(count);
                Action::None
            }
            Key::Char('k') | Key::Up => {
                let count = self.take_count() as i64;
                self.move_vertical(-count);
                Action::None
            }
            Key::Char('w') => {
                for _ in 0..self.take_count() {
                    self.move_to(self.word_forward());
                }
                Action::None
            }
            Key::Char('b') => {
                for _ in 0..self.take_count() {
                    self.move_to(self.word_backward());
                }
                Action::None
            }
            Key::Char('e') => {
                for _ in 0..self.take_count() {
                    self.move_to(self.word_end());
                }
                Action::None
            }
            Key::Char('0') | Key::Home => {
                self.count = None;
                self.move_to(Position::new(self.current().line, 0));
                Action::None
            }
            Key::Char('$') | Key::End => {
                self.count = None;
                self.move_to(self.line_end(self.current().line));
                Action::None
            }
            Key::Char('G') => {
                let line = match self.count.take() {
                    Some(count) => count.saturating_sub(1).min(self.last_line()),
                    None => self.last_line(),
                };
                self.move_to(Position::new(line, 0));
                Action::None
            }
            Key::Char('i') => {
                self.count = None;
                let start = self.ordered().0;
                self.begin_insert(start, start);
                Action::None
            }
            Key::Char('a') => {
                self.count = None;
                let (_, end) = self.ordered();
                self.begin_insert(end, end);
                Action::None
            }
            Key::Char('v') => {
                self.count = None;
                self.set_mode(Mode::Select);
                Action::None
            }
            Key::Char('x') | Key::Char('d') => {
                let count = self.take_count();
                if self.mode == Mode::Select {
                    self.delete_selection()
                } else {
                    self.delete_charwise(count)
                }
            }
            Key::Char('p') => {
                let count = self.take_count();
                self.paste(count, false)
            }
            Key::Char('P') => {
                let count = self.take_count();
                self.paste(count, true)
            }
            Key::Char('.') => {
                let count = self.take_count();
                self.repeat_last(count)
            }
            Key::Char('u') => {
                self.count = None;
                Action::Undo
            }
            Key::Ctrl('r') => {
                self.count = None;
                Action::Redo
            }
            Key::Ctrl('u') => {
                self.count = None;
                Action::Undo
            }
            Key::Esc => {
                self.count = None;
                self.set_mode(Mode::Normal);
                self.select_one_char(self.current());
                Action::None
            }
            _ => {
                self.count = None;
                Action::None
            }
        }
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

    /// Consumes the typed count, defaulting to one.
    fn take_count(&mut self) -> u32 {
        self.count.take().unwrap_or(1).max(1)
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
        self.store_register(&deleted);
        self.commit(vec![TextEdit::delete(range)], start, Mode::Normal)
    }

    fn delete_selection(&mut self) -> Action {
        let (start, end) = self.ordered();
        if start == end {
            return Action::None;
        }

        let range = Range::new(start, end);
        let deleted = self.text.slice(range).unwrap_or_default();
        self.store_register(&deleted);
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

    /// Inserts register text after the cursor, or before it with `before`.
    fn paste(&mut self, count: u32, before: bool) -> Action {
        let text = self.register_text().to_string();
        if text.is_empty() {
            return Action::None;
        }

        let (start, end) = self.ordered();
        let at = if before { start } else { end };
        self.last_change = Some(Repeat::Insert { text: text.clone() });

        let text = text.repeat(count as usize);
        self.insert_at(at, &text)
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

        self.commit(
            vec![TextEdit::new(Range::empty(at), text)],
            cursor,
            Mode::Normal,
        )
    }

    fn store_register(&mut self, text: &str) {
        self.unnamed_register.clear();
        self.unnamed_register.push_str(text);

        if let Some(index) = self.active_register {
            self.named_registers[index].clear();
            self.named_registers[index].push_str(text);
        }
    }

    fn register_text(&self) -> &str {
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
            .is_none_or(|at| at.elapsed() > TRANSACTION_IDLE);

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

    fn is_space_at(&self, index: usize) -> bool {
        self.text
            .char_at_index(index)
            .is_some_and(|character| character.is_whitespace())
    }

    fn word_forward(&self) -> Position {
        let current = self.current();
        let length = self.text.len_chars();
        let mut index = self.text.position_to_char(current).unwrap_or(0);

        while index < length && !self.is_space_at(index) {
            index += 1;
        }
        while index < length && self.is_space_at(index) {
            index += 1;
        }

        self.text.char_to_position(index).unwrap_or(current)
    }

    fn word_backward(&self) -> Position {
        let current = self.current();
        let mut index = self.text.position_to_char(current).unwrap_or(0);
        if index == 0 {
            return current;
        }

        index -= 1;
        while index > 0 && self.is_space_at(index) {
            index -= 1;
        }
        while index > 0 && !self.is_space_at(index - 1) {
            index -= 1;
        }

        self.text.char_to_position(index).unwrap_or(current)
    }

    fn word_end(&self) -> Position {
        let current = self.current();
        let length = self.text.len_chars();
        let mut index = self.text.position_to_char(current).unwrap_or(0);
        if index >= length {
            return current;
        }

        while index < length && self.is_space_at(index) {
            index += 1;
        }
        while index + 1 < length && !self.is_space_at(index + 1) {
            index += 1;
        }

        self.text.char_to_position(index).unwrap_or(current)
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

    fn at(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn typed(editor: &mut Editor, text: &str) -> Vec<Transaction> {
        let mut transactions = Vec::new();
        for character in text.chars() {
            if let Action::Edit(transaction) = editor.handle_key(Key::Char(character)) {
                transactions.push(transaction);
            }
        }
        transactions
    }

    fn last_edit(editor: &mut Editor, key: Key) -> Transaction {
        match editor.handle_key(key) {
            Action::Edit(transaction) => transaction,
            other => panic!("expected an edit: {other:?}"),
        }
    }

    fn press(editor: &mut Editor, keys: &str) {
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
        let edits = vec![TextEdit::new(Range::new(at(0, 1), at(0, 3)), "b")];
        editor.apply_history(&edits, 2, &[Selection::caret(at(0, 1))]);

        assert_eq!(editor.text(), "abc");
        assert_eq!(editor.version(), 2);
        assert_eq!(editor.cursor(), at(0, 1));
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
}
