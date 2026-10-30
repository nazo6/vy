//! Modal input on the client side.
//!
//! It holds the client's copy of the text, the cursor, the selection, and the mode.
//! Commands come from the keymap (`keymap.rs`) and build the `buffer/did_change`
//! payload for the core. See "Where state lives" in `docs/specs/architecture.md` and the
//! key flow in `docs/specs/ui.md`.
//!
//! Phase 1 covers Normal, Insert, and Select plus `h j k l w b e 0 $ G x d p P v i a .`
//! and counts. Yank and text objects come with the keymap in Phase 2.

use std::time::{Duration, Instant};

use vy_core::{EditError, Position, Range, Selection, Text, TextEdit};

use crate::commands::{BuiltinCommand, KeymapArgs};

pub use crate::keymap::{Key, Mode};

/// Interval within which consecutive input is grouped into one undo entry.
pub const TRANSACTION_IDLE: Duration = Duration::from_millis(500);

mod action;
mod input_state;
mod motion;
mod operator;
mod register;
mod repeat;
mod view;
mod visual;

pub use action::{Action, Transaction};
pub use view::ViewState;
pub use visual::{VisualIndex, VisualOptions, VisualRow};

use self::input_state::{InsertSession, PendingInput};
use self::motion::{Motion, combined_count, motion_of};
use self::operator::{
    Operator, TextObject, TextObjectKind, is_operator_command, is_text_object_command,
};
use self::register::{Register, register_index};
use self::repeat::{InsertKind, Repeat};

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
    /// Last delete or yank. It is also the unnamed register.
    unnamed_register: Register,
    /// Registers `a` to `z`.
    named_registers: [Register; 26],
    /// Operator or text object waiting for the next key.
    pending: PendingInput,
    /// Last change, replayed by `.`.
    last_change: Option<Repeat>,
    /// Last Visual range. `gv` restores it.
    last_visual: Option<Range>,
    /// Text inserted and how the current Insert session started, replayed by `.`.
    insert_session: InsertSession,
    /// Column kept across vertical moves. Updated by horizontal moves and dropped by others.
    preferred_character: Option<u32>,
    /// Display column kept across visual/display vertical moves.
    preferred_display_col: Option<usize>,
    /// Visual wrap width (viewport columns) if line wrapping is enabled.
    wrap_width: Option<usize>,
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
            unnamed_register: Register::default(),
            named_registers: std::array::from_fn(|_| Register::default()),
            pending: PendingInput::Idle,
            last_change: None,
            last_visual: None,
            insert_session: InsertSession::default(),
            preferred_character: None,
            preferred_display_col: None,
            wrap_width: None,
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
        self.last_visual = None;
        self.insert_session.clear();
        self.preferred_character = None;
        self.preferred_display_col = None;
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

    /// The whole text of the client's copy. Scripts read it through
    /// `document.getText`, and large documents stay in the client's rope.
    pub fn text(&self) -> String {
        self.text.as_string()
    }

    /// The current cursor as an anchor and a head, in the shape scripts use.
    pub fn selections(&self) -> Vec<Selection> {
        match self.mode {
            Mode::SelectBlock => {
                let first_line = self.anchor.line.min(self.head.line);
                let last_line = self.anchor.line.max(self.head.line);
                let first_col = self.anchor.character.min(self.head.character);
                let last_col = self.anchor.character.max(self.head.character);
                (first_line..=last_line)
                    .map(|line| Selection {
                        anchor: Position::new(line, first_col),
                        head: Position::new(line, last_col),
                    })
                    .collect()
            }
            _ => vec![self.selection_state()],
        }
    }

    /// Moves the cursor. Positions are clamped to the document.
    pub fn set_selections(&mut self, selections: &[Selection]) {
        let Some(selection) = selections.first().copied() else {
            return;
        };
        self.anchor = self.clamp(selection.anchor);
        self.head = self.clamp(selection.head);
        self.preferred_character = None;
        self.preferred_display_col = None;
    }

    /// Moves the cursor to `position` and collapses the selection.
    ///
    /// Normal mode keeps one character selected, as every other motion does,
    /// and stays before the line break. Insert and the select modes may put the
    /// caret on the end of the line.
    pub fn move_cursor_to(&mut self, position: Position) {
        let mut position = self.clamp(position);
        if self.mode == Mode::Normal {
            let length = self
                .text
                .line_len_utf16(position.line as usize)
                .unwrap_or_default();
            if length > 0 && position.character >= length {
                position.character = length - 1;
            }
        }
        self.last_edit = None;
        self.preferred_character = None;
        self.preferred_display_col = None;
        match self.mode {
            Mode::Normal => self.select_one_char(position),
            _ => {
                self.anchor = position;
                self.head = position;
            }
        }
    }

    /// Starts or updates a Visual selection with an inclusive `head`.
    ///
    /// Used by the mouse. The selection is characterwise (`Mode::Select`), and
    /// an anchor equal to the head still selects the character under it, as
    /// `v` does.
    pub fn set_visual_selection(&mut self, anchor: Position, head: Position) {
        self.set_mode(Mode::Select);
        self.anchor = self.clamp(anchor);
        self.head = self.clamp(head);
        self.preferred_character = None;
        self.preferred_display_col = None;
    }

    /// Extends the current selection to `position`, starting a Visual one when
    /// the editor is not already selecting.
    pub fn extend_selection_to(&mut self, position: Position) {
        let head = self.clamp(position);
        if matches!(
            self.mode,
            Mode::Select | Mode::SelectLine | Mode::SelectBlock
        ) {
            self.head = head;
            self.preferred_character = None;
            self.preferred_display_col = None;
        } else {
            let anchor = self.current();
            self.set_visual_selection(anchor, head);
        }
    }

    /// Selects the word under `position`.
    ///
    /// Word, punctuation, and whitespace runs are separate classes, matching
    /// the word motions. A run stops at a line break, so a double click never
    /// selects across lines.
    pub fn select_word_at(&mut self, position: Position) {
        match self.word_bounds_at(position) {
            Some((start, end)) => {
                let head = self.previous_char(end).unwrap_or(start);
                self.set_visual_selection(start, head);
            }
            None => self.set_visual_selection(position, position),
        }
    }

    /// Selects the whole line at `position` linewise.
    pub fn select_line_at(&mut self, position: Position) {
        let line = position.line.min(self.last_line());
        self.set_mode(Mode::SelectLine);
        self.anchor = Position::new(line, 0);
        self.head = Position::new(line, 0);
        self.preferred_character = None;
        self.preferred_display_col = None;
    }

    /// Half-open character range of the word at `position`.
    fn word_bounds_at(&self, position: Position) -> Option<(Position, Position)> {
        let length = self.text.len_chars();
        if length == 0 {
            return None;
        }

        let mut index = self
            .text
            .position_to_char(self.clamp(position))
            .ok()?
            .min(length - 1);
        if is_line_break(self.text.char_at_index(index)?) {
            if index == 0 {
                return None;
            }
            index -= 1;
            if is_line_break(self.text.char_at_index(index)?) {
                return None;
            }
        }

        let class = self.class_at(index)?;
        let mut start = index;
        while start > 0 {
            if is_line_break(self.text.char_at_index(start - 1)?)
                || self.class_at(start - 1) != Some(class)
            {
                break;
            }
            start -= 1;
        }

        let mut end = index + 1;
        while end < length {
            if is_line_break(self.text.char_at_index(end)?) || self.class_at(end) != Some(class) {
                break;
            }
            end += 1;
        }

        Some((self.position_of(start), self.position_of(end)))
    }

    fn selection_state(&self) -> Selection {
        Selection {
            anchor: self.anchor,
            head: self.head,
        }
    }

    /// Text of the unnamed register. Only used by tests.
    #[cfg(test)]
    pub fn register_text(&self) -> &str {
        &self.register(None).text
    }

    /// Cursor used for drawing. The selection start in Normal, otherwise the end.
    pub fn cursor(&self) -> Position {
        self.current()
    }

    /// Computes the selection range for the given mode and endpoints.
    pub fn range_for(&self, mode: Mode, anchor: Position, head: Position) -> Range {
        match mode {
            Mode::Normal => {
                let (start, end) = if anchor <= head {
                    (anchor, head)
                } else {
                    (head, anchor)
                };
                Range::new(start, end)
            }
            Mode::Insert => Range::empty(head),
            Mode::Select => {
                let min_pos = anchor.min(head);
                let max_pos = anchor.max(head);
                let end = self.next_char(max_pos).unwrap_or(max_pos);
                Range::new(min_pos, end)
            }
            Mode::SelectLine => {
                let first_line = anchor.line.min(head.line);
                let last_line = anchor.line.max(head.line);
                let range = self.linewise_range(first_line, last_line);
                Range::new(range.start, range.end)
            }
            Mode::SelectBlock => {
                let first_line = anchor.line.min(head.line);
                let last_line = anchor.line.max(head.line);
                let first_col = anchor.character.min(head.character);
                let last_col = anchor.character.max(head.character);
                Range::new(
                    Position::new(first_line, first_col),
                    Position::new(last_line, last_col + 1),
                )
            }
        }
    }

    /// The selection range. Empty in Insert.
    pub fn selection(&self) -> Range {
        self.range_for(self.mode, self.anchor, self.head)
    }

    /// Copies the pane-local state out of the editor.
    pub fn view_state(&self) -> ViewState {
        ViewState {
            mode: self.mode,
            anchor: self.anchor,
            head: self.head,
            pending: self.pending,
            preferred_character: self.preferred_character,
            preferred_display_col: self.preferred_display_col,
        }
    }

    /// Replaces the pane-local state, clamping positions to the current text.
    pub fn set_view_state(&mut self, state: ViewState) {
        self.mode = state.mode;
        self.anchor = self.clamp(state.anchor);
        self.head = self.clamp(state.head);
        self.pending = state.pending;
        self.preferred_character = state.preferred_character;
        self.preferred_display_col = state.preferred_display_col;

        // Normal mode always keeps one character selected, even for a fresh pane.
        if self.mode == Mode::Normal && self.anchor == self.head {
            self.select_one_char(self.head);
        }
    }

    /// Sets the visual wrap width in columns for visual/display line motions.
    pub fn set_wrap_width(&mut self, width: Option<usize>) {
        self.wrap_width = width;
    }

    /// The visual wrap width in columns, or `None` if wrap is disabled.
    pub fn wrap_width(&self) -> Option<usize> {
        self.wrap_width
    }

    /// Clamps a stored pane state to the current text after another pane edited it.
    pub fn clamp_view_state(&self, mut state: ViewState) -> ViewState {
        state.anchor = self.clamp(state.anchor);
        state.head = self.clamp(state.head);
        state
    }

    /// Remembers a Visual range so `gv` can restore it from any pane.
    pub fn set_last_visual(&mut self, range: Range) {
        let start = self.clamp(range.start);
        let end = self.clamp(range.end);
        self.last_visual = Some(Range::new(start, end));
    }

    /// The last recorded Visual range, used by Ex addresses such as `'<`.
    pub fn last_visual(&self) -> Option<Range> {
        self.last_visual
    }

    /// Restores the last Visual range. Does nothing when none was recorded.
    pub fn select_last_visual(&mut self) -> Action {
        let Some(range) = self.last_visual else {
            return Action::None;
        };

        self.mode = Mode::Select;
        self.anchor = self.clamp(range.start);
        self.head = self.previous_char(range.end).unwrap_or(self.anchor);
        self.last_edit = None;
        self.preferred_character = None;
        self.preferred_display_col = None;
        Action::None
    }

    /// Whether `d` or `y` is waiting for a motion or text object.
    pub fn operator_pending(&self) -> bool {
        self.pending.operator_pending()
    }

    /// Whether `i` or `a` after an operator is waiting for the object key.
    pub fn text_object_pending(&self) -> bool {
        self.pending.text_object_pending()
    }

    /// Runs a built-in command. The keymap supplies the count and register.
    pub fn execute(&mut self, command: BuiltinCommand, args: KeymapArgs) -> Action {
        let count = args.count.unwrap_or(1).max(1);
        let register = args.register.and_then(register_index);

        // A pending operator consumes the next motion.
        if let Some(pending) = self.pending.operator() {
            if let Some(motion) = motion_of(command) {
                self.pending.clear();
                let count = combined_count(pending.count, args.count);
                return self.apply_operator_motion(
                    pending.operator,
                    motion,
                    count,
                    pending.register.or(register),
                );
            }
            if !is_operator_command(command) {
                self.pending.clear();
            }
        }

        if self.pending.text_object_pending() && !is_text_object_command(command) {
            let cancel = matches!(
                command,
                BuiltinCommand::OperatorDelete | BuiltinCommand::OperatorYank
            );
            self.pending.clear();
            if cancel {
                return Action::None;
            }
        }

        match command {
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
            BuiltinCommand::MoveDisplayUp => {
                self.apply_motion(Motion::DisplayUp, args.count);
                Action::None
            }
            BuiltinCommand::MoveDisplayDown => {
                self.apply_motion(Motion::DisplayDown, args.count);
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
                if self.mode.is_select() {
                    self.delete_selection(register)
                } else {
                    self.delete_charwise(count, register)
                }
            }
            BuiltinCommand::PasteAfter => self.paste(count, false, register),
            BuiltinCommand::PasteBefore => self.paste(count, true, register),
            BuiltinCommand::Repeat => self.repeat_last(count),
            BuiltinCommand::LastVisual => self.select_last_visual(),
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
            BuiltinCommand::InsertLineEnd => {
                let line = self.current().line;
                let end = self.text.line_len_utf16(line as usize).unwrap_or(0);
                let at = Position::new(line, end);
                self.begin_insert(at, at);
                Action::None
            }
            BuiltinCommand::InsertLineStart => {
                let line = self.current().line;
                let start = self.line_first_non_blank(line as usize);
                let at = Position::new(line, start);
                self.begin_insert(at, at);
                Action::None
            }
            BuiltinCommand::OpenBelow => self.open_below(),
            BuiltinCommand::OpenAbove => self.open_above(),
            BuiltinCommand::SelectMode => self.toggle_select_mode(Mode::Select),
            BuiltinCommand::SelectLineMode => self.toggle_select_mode(Mode::SelectLine),
            BuiltinCommand::SelectBlockMode => self.toggle_select_mode(Mode::SelectBlock),
            BuiltinCommand::NormalMode => {
                self.pending.clear();
                if self.mode == Mode::Insert {
                    self.finish_insert();
                    self.set_mode(Mode::Normal);
                    self.select_near_caret();
                } else {
                    let cursor = self.head;
                    self.set_mode(Mode::Normal);
                    self.select_one_char(cursor);
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
            BuiltinCommand::OperatorDelete => {
                self.start_operator(Operator::Delete, args.count, register)
            }
            BuiltinCommand::OperatorYank => {
                self.start_operator(Operator::Yank, args.count, register)
            }
            BuiltinCommand::YankLine => self.yank_lines(count, register),
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
        }
    }

    pub(super) fn position_of(&self, index: usize) -> Position {
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

    /// Applies edits requested by a script as one transaction.
    ///
    /// The edits are sorted into the coordinate space `Text::apply` expects;
    /// overlapping edits are still rejected. The caller sends the transaction to
    /// the core. A failure leaves the mirror and the version untouched.
    pub fn apply_edits(&mut self, edits: Vec<TextEdit>) -> Result<Transaction, EditError> {
        let selection_before = vec![self.selection_state()];
        let base_version = self.version;
        let transaction_id = self.next_transaction_id();

        // `Text::apply` needs a sorted batch. A stable sort keeps the order of
        // insertions at the same position.
        let mut edits = edits;
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));

        self.text.apply(&edits)?;
        self.version += 1;
        self.last_edit = Some(Instant::now());
        self.preferred_character = None;
        self.preferred_display_col = None;
        self.anchor = self.clamp(self.anchor);
        self.head = self.clamp(self.head);

        Ok(Transaction {
            transaction_id,
            base_version,
            edits,
            selection_before,
            selection_after: vec![self.selection_state()],
        })
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
        self.insert_session.clear();
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
        self.insert_session.buffer.push_str(value);
        self.commit(vec![edit], new_head, Mode::Insert)
    }

    fn delete_before_caret(&mut self) -> Action {
        let caret = self.head;
        let Some(previous) = self.previous_char(caret) else {
            return Action::None;
        };
        let edit = TextEdit::delete(Range::new(previous, caret));
        self.insert_session.buffer.pop();
        self.commit(vec![edit], previous, Mode::Insert)
    }

    /// Inserts text at a position and leaves the cursor on the last inserted character.
    pub(super) fn insert_at(&mut self, at: Position, text: &str) -> Action {
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
    pub(super) fn insert_at_with_cursor(
        &mut self,
        at: Position,
        text: &str,
        cursor: Position,
    ) -> Action {
        if text.is_empty() {
            return Action::None;
        }

        self.commit(
            vec![TextEdit::new(Range::empty(at), text)],
            cursor,
            Mode::Normal,
        )
    }

    /// Returns the position `count` characters after `from`, stopping at the end.
    pub(super) fn position_forward(&self, from: Position, count: u32) -> Position {
        let index = self.text.position_to_char(from).unwrap_or(0);
        let target = (index + count as usize).min(self.text.len_chars());
        self.text.char_to_position(target).unwrap_or(from)
    }

    /// Number of characters in a range.
    pub(super) fn char_count(&self, range: Range) -> u32 {
        let start = self.text.position_to_char(range.start).unwrap_or(0);
        let end = self.text.position_to_char(range.end).unwrap_or(start);
        end.saturating_sub(start).min(u32::MAX as usize) as u32
    }

    /// Applies the change to the mirror and builds the transaction for the core.
    pub(super) fn commit(
        &mut self,
        edits: Vec<TextEdit>,
        new_head: Position,
        mode: Mode,
    ) -> Action {
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
            Mode::Select | Mode::SelectLine | Mode::SelectBlock => {
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

    /// Changes the mode. Scripts call this through `editor.setMode`.
    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            // A mode change starts a new undo entry for the next edit.
            self.last_edit = None;
        }
    }

    /// Enters a Visual mode, or leaves it when that mode is already active.
    pub(super) fn toggle_select_mode(&mut self, mode: Mode) -> Action {
        if self.mode == mode {
            self.set_mode(Mode::Normal);
            self.select_one_char(self.head);
        } else {
            let cursor = self.current();
            let was_normal = self.mode == Mode::Normal;
            self.set_mode(mode);
            if was_normal {
                self.anchor = cursor;
                self.head = cursor;
            }
        }
        Action::None
    }

    /// Current position: the selection start in Normal, the end in Insert and Select.
    pub(super) fn current(&self) -> Position {
        match self.mode {
            Mode::Normal => self.ordered().0,
            _ => self.head,
        }
    }

    pub(super) fn ordered(&self) -> (Position, Position) {
        let range = self.selection();
        (range.start, range.end)
    }

    /// Selects one character at the cursor. The selection is empty at the end of the text.
    pub(super) fn select_one_char(&mut self, at: Position) {
        let at = self.clamp(at);
        self.anchor = at;
        self.head = self.next_char(at).unwrap_or(at);
    }

    /// When leaving Insert, selects the character right before the caret.
    pub(super) fn select_near_caret(&mut self) {
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

    pub(super) fn clamp(&self, position: Position) -> Position {
        let line = position.line.min(self.last_line());
        let length = self.text.line_len_utf16(line as usize).unwrap_or_default();
        Position::new(line, position.character.min(length))
    }

    fn line_first_non_blank(&self, line: usize) -> u32 {
        let Some(text) = self.text.line_string(line) else {
            return 0;
        };
        let mut offset = 0;
        for ch in text.chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            if !ch.is_whitespace() {
                return offset;
            }
            offset += ch.len_utf16() as u32;
        }
        offset
    }

    pub(super) fn open_below(&mut self) -> Action {
        self.insert_session.begin(InsertKind::OpenBelow);
        let line = self.current().line;
        let end = self.text.line_len_utf16(line as usize).unwrap_or(0);
        let at = Position::new(line, end);
        let cursor = Position::new(line + 1, 0);
        self.commit(
            vec![TextEdit::new(Range::empty(at), "\n")],
            cursor,
            Mode::Insert,
        )
    }

    pub(super) fn open_above(&mut self) -> Action {
        self.insert_session.begin(InsertKind::OpenAbove);
        let line = self.current().line;
        let at = Position::new(line, 0);
        let cursor = Position::new(line, 0);
        self.commit(
            vec![TextEdit::new(Range::empty(at), "\n")],
            cursor,
            Mode::Insert,
        )
    }

    pub(super) fn repeat_open_below(&mut self, text: &str) -> Action {
        let line = self.current().line;
        let end = self.text.line_len_utf16(line as usize).unwrap_or(0);
        let at = Position::new(line, end);
        let content = format!("\n{text}");
        self.insert_at(at, &content)
    }

    pub(super) fn repeat_open_above(&mut self, text: &str) -> Action {
        let line = self.current().line;
        let at = Position::new(line, 0);
        let content = format!("{text}\n");
        let cursor = if text.is_empty() {
            Position::new(line, 0)
        } else {
            let last = text.chars().next_back().unwrap();
            let char_len = last.len_utf16() as u32;
            let total_len = text.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
            Position::new(line, total_len.saturating_sub(char_len))
        };
        self.insert_at_with_cursor(at, &content, cursor)
    }
}

/// Whether a character ends a line, so mouse word selection stops there.
fn is_line_break(character: char) -> bool {
    matches!(character, '\n' | '\r')
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
