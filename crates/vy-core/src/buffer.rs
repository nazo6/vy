use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::edit::{EditError, TextEdit};
use crate::position::{Position, Range, Selection};
use crate::text::Text;
use crate::uri::Uri;

pub type DocumentId = u64;
pub type Version = u64;

/// Upper bound on undo entries. Older entries are dropped first.
pub const DEFAULT_UNDO_LIMIT: usize = 1000;

/// The result of applying an edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    /// Version after the edit.
    pub version: Version,
    /// Edits the client must apply, in order. Each batch uses the coordinate space
    /// that results from applying the previous batches. Empty for edit requests.
    pub edits: Vec<Vec<TextEdit>>,
    /// Selection to place after the edit.
    pub selection: Vec<Selection>,
}

#[derive(Debug, Clone)]
struct UndoEntry {
    /// Grouping key. Consecutive edits with the same ID become one entry.
    transaction_id: u64,
    /// Inverse edits per applied batch, in application order. Undo applies them in
    /// reverse order, one batch at a time.
    inverse_batches: Vec<Vec<TextEdit>>,
    /// Forward edits per applied batch, in application order. Redo applies them in
    /// order, one batch at a time.
    forward_batches: Vec<Vec<TextEdit>>,
    selection_before: Vec<Selection>,
    selection_after: Vec<Selection>,
}

/// The text, version, and undo history of one document.
///
/// The core owns the text (see `docs/adr/0003-editor-state.md`). The client owns the
/// cursor and selection, so it passes the selections before and after an edit.
pub struct Buffer {
    id: DocumentId,
    uri: Uri,
    text: Text,
    version: Version,
    saved_version: Version,
    undo_limit: usize,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
}

impl Buffer {
    pub fn new(id: DocumentId, uri: Uri, text: impl AsRef<str>) -> Self {
        Self {
            id,
            uri,
            text: Text::new(text),
            version: 0,
            saved_version: 0,
            undo_limit: DEFAULT_UNDO_LIMIT,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Reads a file and creates a buffer.
    pub fn open(id: DocumentId, uri: Uri, path: impl AsRef<Path>) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::new(id, uri, text))
    }

    /// Changes the undo limit. Used by tests and diagnostics.
    pub fn with_undo_limit(mut self, limit: usize) -> Self {
        self.undo_limit = limit.max(1);
        self
    }

    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn line_count(&self) -> usize {
        self.text.line_count()
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn text(&self) -> String {
        self.text.as_string()
    }

    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub fn mark_saved(&mut self) {
        self.saved_version = self.version;
    }

    /// Converts a UTF-16 position to the core's char index.
    ///
    /// A position past the end of the line fails. A position inside a surrogate pair is
    /// rounded down to the start of the character.
    pub fn position_to_char(&self, position: Position) -> Result<usize, EditError> {
        self.text.position_to_char(position)
    }

    /// Converts a char index to a UTF-16 position.
    pub fn char_to_position(&self, char_index: usize) -> Result<Position, EditError> {
        self.text.char_to_position(char_index)
    }

    /// Returns the text in a range.
    pub fn slice(&self, range: Range) -> Result<String, EditError> {
        self.text.slice(range)
    }

    /// Applies an edit transaction.
    ///
    /// If `base_version` does not match the current version, nothing changes. The caller
    /// resends the text and resyncs (see "Buffer change notifications" in `docs/protocol.md`).
    pub fn apply_edits(
        &mut self,
        base_version: Version,
        transaction_id: u64,
        edits: Vec<TextEdit>,
        selection_before: Vec<Selection>,
        selection_after: Vec<Selection>,
    ) -> Result<Applied, EditError> {
        if base_version != self.version {
            return Err(EditError::VersionMismatch {
                base: base_version,
                current: self.version,
            });
        }

        if edits.is_empty() {
            return Ok(Applied {
                version: self.version,
                edits: Vec::new(),
                selection: selection_after,
            });
        }

        let inverse = self.text.apply(&edits)?;
        self.version += 1;
        self.redo.clear();

        let grouped = self
            .undo
            .last()
            .is_some_and(|entry| entry.transaction_id == transaction_id);

        if grouped {
            let entry = self.undo.last_mut().expect("checked just above");
            entry.inverse_batches.push(inverse);
            entry.forward_batches.push(edits);
            entry.selection_after = selection_after.clone();
        } else {
            self.undo.push(UndoEntry {
                transaction_id,
                inverse_batches: vec![inverse],
                forward_batches: vec![edits],
                selection_before,
                selection_after: selection_after.clone(),
            });
            if self.undo.len() > self.undo_limit {
                self.undo.remove(0);
            }
        }

        Ok(Applied {
            version: self.version,
            edits: Vec::new(),
            selection: selection_after,
        })
    }

    /// Undoes the previous entry. [`None`] if there is nothing to undo.
    ///
    /// The inverted batches are applied to a copy first, so a failure leaves the text
    /// and the history unchanged.
    pub fn undo(&mut self) -> Result<Option<Applied>, EditError> {
        let Some(entry) = self.undo.last() else {
            return Ok(None);
        };

        let mut text = self.text.clone();
        let mut batches = Vec::with_capacity(entry.inverse_batches.len());
        for batch in entry.inverse_batches.iter().rev() {
            text.apply(batch)?;
            batches.push(batch.clone());
        }

        let entry = self.undo.pop().expect("checked just above");
        self.text = text;
        self.version += 1;

        let selection = entry.selection_before.clone();
        self.redo.push(entry);

        Ok(Some(Applied {
            version: self.version,
            edits: batches,
            selection,
        }))
    }

    /// Redoes the entry that was undone. [`None`] if there is nothing to redo.
    ///
    /// The forward batches are applied to a copy first, so a failure leaves the text
    /// and the history unchanged.
    pub fn redo(&mut self) -> Result<Option<Applied>, EditError> {
        let Some(entry) = self.redo.last() else {
            return Ok(None);
        };

        let mut text = self.text.clone();
        let mut batches = Vec::with_capacity(entry.forward_batches.len());
        for batch in &entry.forward_batches {
            text.apply(batch)?;
            batches.push(batch.clone());
        }

        let entry = self.redo.pop().expect("checked just above");
        self.text = text;
        self.version += 1;

        let selection = entry.selection_after.clone();
        self.undo.push(entry);

        Ok(Some(Applied {
            version: self.version,
            edits: batches,
            selection,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> Buffer {
        Buffer::new(1, Uri::parse("file:///tmp/test.txt").expect("uri"), text)
    }

    fn at(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    fn edit(start: Position, end: Position, text: &str) -> TextEdit {
        TextEdit::new(Range::new(start, end), text)
    }

    fn apply(buffer: &mut Buffer, edits: Vec<TextEdit>) -> Applied {
        let version = buffer.version();
        buffer
            .apply_edits(version, 1, edits, Vec::new(), Vec::new())
            .expect("apply")
    }

    #[test]
    fn starts_at_version_zero() {
        let buffer = buffer("hello");
        assert_eq!(buffer.version(), 0);
        assert_eq!(buffer.line_count(), 1);
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn inserts_at_a_position() {
        let mut buffer = buffer("hello");
        let applied = apply(&mut buffer, vec![edit(at(0, 5), at(0, 5), " world")]);
        assert_eq!(applied.version, 1);
        assert!(applied.edits.is_empty());
        assert_eq!(buffer.text(), "hello world");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn rejects_a_mismatched_base_version() {
        let mut buffer = buffer("hello");
        let error = buffer
            .apply_edits(9, 1, vec![edit(at(0, 0), at(0, 0), "x")], vec![], vec![])
            .expect_err("must fail");
        assert_eq!(
            error,
            EditError::VersionMismatch {
                base: 9,
                current: 0
            }
        );
        assert_eq!(buffer.text(), "hello");
    }

    #[test]
    fn shifts_offsets_between_edits_in_one_batch() {
        let mut buffer = buffer("aaa");
        apply(
            &mut buffer,
            vec![
                edit(at(0, 0), at(0, 1), "XX"),
                edit(at(0, 2), at(0, 3), "YY"),
            ],
        );
        assert_eq!(buffer.text(), "XXaYY");
    }

    #[test]
    fn rejects_overlapping_edits() {
        let mut buffer = buffer("abcd");
        let error = buffer
            .apply_edits(
                0,
                1,
                vec![edit(at(0, 0), at(0, 3), "x"), edit(at(0, 2), at(0, 4), "y")],
                vec![],
                vec![],
            )
            .expect_err("must fail");
        assert_eq!(error, EditError::OverlappingEdits);
        assert_eq!(buffer.text(), "abcd");
    }

    #[test]
    fn undo_and_redo_restore_text_and_selection() {
        let mut buffer = buffer("abc");
        let before = vec![Selection::caret(at(0, 0))];
        let after = vec![Selection::caret(at(0, 3))];

        buffer
            .apply_edits(
                0,
                1,
                vec![edit(at(0, 1), at(0, 2), "XY")],
                before.clone(),
                after.clone(),
            )
            .expect("apply");
        assert_eq!(buffer.text(), "aXYc");

        let undone = buffer.undo().expect("undo").expect("entry");
        assert_eq!(undone.version, 2);
        assert_eq!(undone.edits, vec![vec![edit(at(0, 1), at(0, 3), "b")]]);
        assert_eq!(undone.selection, before);
        assert_eq!(buffer.text(), "abc");

        let redone = buffer.redo().expect("redo").expect("entry");
        assert_eq!(redone.version, 3);
        assert_eq!(redone.edits, vec![vec![edit(at(0, 1), at(0, 2), "XY")]]);
        assert_eq!(redone.selection, after);
        assert_eq!(buffer.text(), "aXYc");
    }

    #[test]
    fn groups_edits_with_the_same_transaction_id() {
        let mut buffer = buffer("");
        buffer
            .apply_edits(0, 7, vec![TextEdit::insert(at(0, 0), "a")], vec![], vec![])
            .expect("first");
        buffer
            .apply_edits(1, 7, vec![TextEdit::insert(at(0, 1), "b")], vec![], vec![])
            .expect("second");
        assert_eq!(buffer.text(), "ab");

        let undone = buffer.undo().expect("undo").expect("entry");
        assert_eq!(buffer.text(), "");
        assert_eq!(undone.edits.len(), 2);
        assert_eq!(buffer.undo().expect("undo"), None);
    }

    #[test]
    fn undoes_an_insert_followed_by_a_backspace() {
        let mut buffer = buffer("");
        buffer
            .apply_edits(0, 7, vec![TextEdit::insert(at(0, 0), "a")], vec![], vec![])
            .expect("insert");
        buffer
            .apply_edits(1, 7, vec![edit(at(0, 0), at(0, 1), "")], vec![], vec![])
            .expect("delete");
        assert_eq!(buffer.text(), "");

        let undone = buffer.undo().expect("undo").expect("entry");
        assert_eq!(buffer.text(), "");
        assert_eq!(undone.edits.len(), 2);

        let redone = buffer.redo().expect("redo").expect("entry");
        assert_eq!(buffer.text(), "");
        assert_eq!(redone.edits.len(), 2);
    }

    #[test]
    fn undoes_repeated_deletions_in_one_transaction() {
        let mut buffer = buffer("abcd");
        for version in 0..2 {
            buffer
                .apply_edits(
                    version,
                    3,
                    vec![edit(at(0, 0), at(0, 1), "")],
                    vec![],
                    vec![],
                )
                .expect("delete");
        }
        assert_eq!(buffer.text(), "cd");

        buffer.undo().expect("undo").expect("entry");
        assert_eq!(buffer.text(), "abcd");
        buffer.redo().expect("redo").expect("entry");
        assert_eq!(buffer.text(), "cd");
    }

    #[test]
    fn restores_the_text_across_random_edits_in_one_transaction() {
        let original = "a😀b\n日本\nx";
        let mut buffer = buffer(original);
        let mut state = 0x1234_5678_u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as usize
        };

        for version in 0..40 {
            let len = buffer.len_chars();
            let index = next() % (len + 1);
            let start = buffer.char_to_position(index).expect("position");
            let edits = if len > 0 && next() % 2 == 0 {
                let end = buffer
                    .char_to_position((index + 1).min(len))
                    .expect("end position");
                vec![edit(start, end, "")]
            } else {
                let text = if next() % 3 == 0 { "\n" } else { "日" };
                vec![TextEdit::insert(start, text)]
            };
            buffer
                .apply_edits(version, 42, edits, vec![], vec![])
                .expect("apply");
        }

        let expected = buffer.text();
        buffer.undo().expect("undo").expect("entry");
        assert_eq!(buffer.text(), original);
        buffer.redo().expect("redo").expect("entry");
        assert_eq!(buffer.text(), expected);
    }

    #[test]
    fn clears_redo_when_a_new_edit_arrives() {
        let mut buffer = buffer("");
        apply(&mut buffer, vec![TextEdit::insert(at(0, 0), "a")]);
        buffer.undo().expect("undo").expect("entry");
        apply(&mut buffer, vec![TextEdit::insert(at(0, 0), "b")]);
        assert_eq!(buffer.redo().expect("redo"), None);
    }

    #[test]
    fn counts_positions_in_utf16_units() {
        let mut buffer = buffer("a😀b\n日本");

        assert_eq!(buffer.position_to_char(at(0, 1)).expect("char"), 1);
        assert_eq!(buffer.position_to_char(at(0, 2)).expect("clamp"), 1);
        assert_eq!(buffer.position_to_char(at(0, 3)).expect("char"), 2);
        assert_eq!(buffer.char_to_position(2).expect("position"), at(0, 3));
        assert_eq!(buffer.position_to_char(at(1, 2)).expect("char"), 6);
        assert_eq!(buffer.char_to_position(6).expect("position"), at(1, 2));

        assert_eq!(
            buffer.position_to_char(at(0, 5)),
            Err(EditError::InvalidPosition(at(0, 5)))
        );

        apply(&mut buffer, vec![edit(at(1, 0), at(1, 2), "日本酒")]);
        assert_eq!(buffer.text(), "a😀b\n日本酒");
    }

    #[test]
    fn deletes_a_crlf_line_break() {
        let mut buffer = buffer("a\r\nb");
        apply(&mut buffer, vec![edit(at(0, 1), at(1, 0), "")]);
        assert_eq!(buffer.text(), "ab");
    }

    #[test]
    fn slices_by_position() {
        let buffer = buffer("hello\nworld");
        assert_eq!(
            buffer.slice(Range::new(at(0, 1), at(1, 3))).expect("slice"),
            "ello\nwor"
        );
    }

    #[test]
    fn drops_the_oldest_undo_entry_at_the_limit() {
        let mut buffer = buffer("").with_undo_limit(2);
        for (index, text) in ["a", "b", "c"].iter().enumerate() {
            let version = index as u64;
            buffer
                .apply_edits(
                    version,
                    (index + 1) as u64,
                    vec![TextEdit::insert(at(0, index as u32), *text)],
                    vec![],
                    vec![],
                )
                .expect("apply");
        }
        assert_eq!(buffer.text(), "abc");

        buffer.undo().expect("undo").expect("c");
        buffer.undo().expect("undo").expect("b");
        assert_eq!(buffer.undo().expect("undo"), None);
        assert_eq!(buffer.text(), "a");
    }
}
