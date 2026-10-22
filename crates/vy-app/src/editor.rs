//! クライアント側のモーダル入力です。
//!
//! 本文の写しと、カーソル、選択、モードを持ちます。キーを意味のある操作に変え、
//! コアへ送る変更（`buffer/did_change`）を作ります。状態の所在は
//! `docs/architecture.md` の「状態の所在」、キー入力の流れは `docs/ui.md` にあります。
//!
//! Phase 1 の範囲は Normal、Insert、Select の基本と、`h j k l w b e 0 $ G x d i a v`
//! です。カウント、レジスタ、`.` の繰り返しは次の段階で足します。

use std::time::{Duration, Instant};

use vy_core::{Position, Range, Selection, Text, TextEdit};

/// 入力が続いている間、同じアンドゥの項目にまとめる間隔です。
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

/// GPUI のキーイベントを、テストできる形に直したものです。
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

/// コアへ送る 1 回分の変更です。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub transaction_id: u64,
    pub base_version: u64,
    pub edits: Vec<TextEdit>,
    pub selection_before: Vec<Selection>,
    pub selection_after: Vec<Selection>,
}

/// キーを処理した結果です。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Edit(Transaction),
    Undo,
    Redo,
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
    /// 縦移動で保つ桁です。横移動で更新し、それ以外の移動で捨てます。
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
            preferred_character: None,
        };
        editor.select_one_char(Position::new(0, 0));
        editor
    }

    /// スナップショットか再同期の結果で、本文と version を置き換えます。
    pub fn reset(&mut self, text: impl AsRef<str>, version: u64) {
        self.text = Text::new(text);
        self.version = version;
        self.mode = Mode::Normal;
        self.last_edit = None;
        self.pending_g = false;
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

    #[cfg(test)]
    pub fn text(&self) -> String {
        self.text.as_string()
    }

    /// 描画に使うカーソルです。Normal では選択の始点、それ以外は選択の終端です。
    pub fn cursor(&self) -> Position {
        self.current()
    }

    /// 選択の範囲です。Insert では空になります。
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

    /// コアが返したアンドゥとリドゥの結果を反映します。
    pub fn apply_history(&mut self, edits: &[TextEdit], version: u64, selection: &[Selection]) {
        if let Err(error) = self.text.apply(edits) {
            eprintln!("vy: 履歴の変更を適用できません: {error}");
            return;
        }

        self.version = version;
        self.last_edit = None;
        self.pending_g = false;
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
        if self.pending_g {
            self.pending_g = false;
            if key == Key::Char('g') {
                self.move_to(Position::new(0, 0));
            }
            return Action::None;
        }

        match key {
            Key::Char('g') => {
                self.pending_g = true;
                Action::None
            }
            Key::Char('h') | Key::Left => {
                self.move_horizontal(-1);
                Action::None
            }
            Key::Char('l') | Key::Right => {
                self.move_horizontal(1);
                Action::None
            }
            Key::Char('j') | Key::Down => {
                self.move_vertical(1);
                Action::None
            }
            Key::Char('k') | Key::Up => {
                self.move_vertical(-1);
                Action::None
            }
            Key::Char('w') => {
                self.move_to(self.word_forward());
                Action::None
            }
            Key::Char('b') => {
                self.move_to(self.word_backward());
                Action::None
            }
            Key::Char('e') => {
                self.move_to(self.word_end());
                Action::None
            }
            Key::Char('0') | Key::Home => {
                self.move_to(Position::new(self.current().line, 0));
                Action::None
            }
            Key::Char('$') | Key::End => {
                self.move_to(self.line_end(self.current().line));
                Action::None
            }
            Key::Char('G') => {
                let line = self.last_line();
                self.move_to(Position::new(line, 0));
                Action::None
            }
            Key::Char('i') => {
                let start = self.ordered().0;
                self.set_mode(Mode::Insert);
                self.anchor = start;
                self.head = start;
                Action::None
            }
            Key::Char('a') => {
                let end = self.ordered().1;
                self.set_mode(Mode::Insert);
                self.anchor = end;
                self.head = end;
                Action::None
            }
            Key::Char('v') => {
                self.set_mode(Mode::Select);
                Action::None
            }
            Key::Char('x') | Key::Char('d') => self.delete_selection(),
            Key::Char('u') => Action::Undo,
            Key::Ctrl('r') => Action::Redo,
            Key::Ctrl('u') => Action::Undo,
            Key::Esc => {
                self.set_mode(Mode::Normal);
                self.select_one_char(self.current());
                Action::None
            }
            _ => Action::None,
        }
    }

    fn insert_text(&mut self, value: &str) -> Action {
        let at = self.head;
        let edit = TextEdit::new(Range::empty(at), value);
        let new_head = self.advance(at, value);
        self.commit(vec![edit], new_head, Mode::Insert)
    }

    fn delete_before_caret(&mut self) -> Action {
        let caret = self.head;
        let Some(previous) = self.previous_char(caret) else {
            return Action::None;
        };
        let edit = TextEdit::delete(Range::new(previous, caret));
        self.commit(vec![edit], previous, Mode::Insert)
    }

    fn delete_selection(&mut self) -> Action {
        let (start, end) = self.ordered();
        if start == end {
            return Action::None;
        }

        let edit = TextEdit::delete(Range::new(start, end));
        let mode = if self.mode == Mode::Select {
            Mode::Normal
        } else {
            self.mode
        };
        self.commit(vec![edit], start, mode)
    }

    /// 変更を写しに適用し、コアへ送るトランザクションを作ります。
    fn commit(&mut self, edits: Vec<TextEdit>, new_head: Position, mode: Mode) -> Action {
        let selection_before = vec![Selection {
            anchor: self.anchor,
            head: self.head,
        }];
        let base_version = self.version;
        let transaction_id = self.next_transaction_id();

        if let Err(error) = self.text.apply(&edits) {
            eprintln!("vy: 編集を適用できません: {error}");
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
            // モードが変わったら、次の編集は別のアンドゥの項目にする。
            self.last_edit = None;
        }
    }

    /// 現在の位置です。Normal は選択の始点、Insert と Select は終端です。
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
        // Normal は行の文字の上に置く。Select は行末（改行の位置）まで許す。
        let limit = match self.mode {
            Mode::Normal => length.saturating_sub(1),
            _ => length,
        };
        let position = Position::new(line, desired.min(limit));
        self.preferred_character = Some(desired);
        self.move_to_inner(position);
    }

    /// カーソルの位置に 1 文字ぶんの選択を置きます。末尾では空の選択になります。
    fn select_one_char(&mut self, at: Position) {
        let at = self.clamp(at);
        self.anchor = at;
        self.head = self.next_char(at).unwrap_or(at);
    }

    /// Insert から戻るときに、カーソルの直前の文字を選びます。
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

    /// 挿入した文字列の直後の位置を返します。
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
            other => panic!("編集ではありません: {other:?}"),
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

        // モードを変えると次の編集は別の項目になる。
        editor.handle_key(Key::Esc);
        editor.handle_key(Key::Char('i'));
        let next = last_edit(&mut editor, Key::Char('d'));
        assert_ne!(next.transaction_id, transactions[0].transaction_id);
    }

    #[test]
    fn deletes_a_character_with_x() {
        let mut editor = Editor::with_text("あ😀い", 0);
        editor.handle_key(Key::Char('l'));
        // サロゲートペアでも 1 文字ぶん進み、選択は UTF-16 で 2 単位になる。
        assert_eq!(editor.selection(), Range::new(at(0, 1), at(0, 3)));

        // まずは先頭の「あ」を消す。
        let mut editor = Editor::with_text("あ😀い", 0);
        let deleted = last_edit(&mut editor, Key::Char('x'));
        assert_eq!(deleted.edits[0].range, Range::new(at(0, 0), at(0, 1)));
        assert_eq!(editor.text(), "😀い");

        // カーソルは消した位置に残る。
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
}
