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

use crate::commands::{BuiltinCommand, KeymapArgs};

pub use crate::keymap::{Key, Mode};

/// Interval within which consecutive input is grouped into one undo entry.
pub const TRANSACTION_IDLE: Duration = Duration::from_millis(500);

mod action;
mod motion;
mod operator;
mod register;
mod repeat;
mod view;

pub use action::{Action, Transaction};
pub use view::ViewState;

use self::motion::{Motion, motion_of};
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
    /// Last Visual range. `gv` restores it.
    last_visual: Option<Range>,
    /// Text inserted in the current Insert session, replayed by `.`.
    insert_buffer: String,
    /// The kind of Insert session started (e.g. `o` or `O`).
    insert_kind: InsertKind,
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
            active_register: None,
            unnamed_register: Register::default(),
            named_registers: std::array::from_fn(|_| Register::default()),
            operator: None,
            operator_count: None,
            text_object: None,
            last_change: None,
            last_visual: None,
            insert_buffer: String::new(),
            insert_kind: InsertKind::Normal,
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
        self.insert_buffer.clear();
        self.insert_kind = InsertKind::Normal;
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

    fn selection_state(&self) -> Selection {
        Selection {
            anchor: self.anchor,
            head: self.head,
        }
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
            selection: self.selection(),
            operator: self.operator,
            operator_count: self.operator_count,
            text_object: self.text_object,
            preferred_character: self.preferred_character,
            preferred_display_col: self.preferred_display_col,
        }
    }

    /// Replaces the pane-local state, clamping positions to the current text.
    pub fn set_view_state(&mut self, state: ViewState) {
        self.mode = state.mode;
        self.anchor = self.clamp(state.anchor);
        self.head = self.clamp(state.head);
        self.operator = state.operator;
        self.operator_count = state.operator_count;
        self.text_object = state.text_object;
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
        state.selection = self.range_for(state.mode, state.anchor, state.head);
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
        self.operator.is_some()
    }

    /// Whether `i` or `a` after an operator is waiting for the object key.
    pub fn text_object_pending(&self) -> bool {
        self.text_object.is_some()
    }

    /// Runs a built-in command. The keymap supplies the count and register.
    pub fn execute(&mut self, command: BuiltinCommand, args: KeymapArgs) -> Action {
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
                    self.delete_selection()
                } else {
                    self.delete_charwise(count)
                }
            }
            BuiltinCommand::PasteAfter => self.paste(count, false),
            BuiltinCommand::PasteBefore => self.paste(count, true),
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
            BuiltinCommand::SelectMode => {
                if self.mode == Mode::Select {
                    self.set_mode(Mode::Normal);
                    self.select_one_char(self.head);
                } else {
                    let cursor = self.current();
                    let was_normal = self.mode == Mode::Normal;
                    self.set_mode(Mode::Select);
                    if was_normal {
                        self.anchor = cursor;
                        self.head = cursor;
                    }
                }
                Action::None
            }
            BuiltinCommand::SelectLineMode => {
                if self.mode == Mode::SelectLine {
                    self.set_mode(Mode::Normal);
                    self.select_one_char(self.head);
                } else {
                    let cursor = self.current();
                    let was_normal = self.mode == Mode::Normal;
                    self.set_mode(Mode::SelectLine);
                    if was_normal {
                        self.anchor = cursor;
                        self.head = cursor;
                    }
                }
                Action::None
            }
            BuiltinCommand::SelectBlockMode => {
                if self.mode == Mode::SelectBlock {
                    self.set_mode(Mode::Normal);
                    self.select_one_char(self.head);
                } else {
                    let cursor = self.current();
                    let was_normal = self.mode == Mode::Normal;
                    self.set_mode(Mode::SelectBlock);
                    if was_normal {
                        self.anchor = cursor;
                        self.head = cursor;
                    }
                }
                Action::None
            }
            BuiltinCommand::NormalMode => {
                self.clear_operator();
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

    pub(super) fn clear_operator(&mut self) {
        self.operator = None;
        self.operator_count = None;
        self.text_object = None;
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
        self.insert_buffer.clear();
        self.insert_kind = InsertKind::OpenBelow;
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
        self.insert_buffer.clear();
        self.insert_kind = InsertKind::OpenAbove;
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

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
