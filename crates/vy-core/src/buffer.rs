use std::path::Path;

use ropey::Rope;
use serde::{Deserialize, Serialize};

use crate::edit::{EditError, TextEdit};
use crate::position::{Position, Range, Selection};
use crate::uri::Uri;

pub type DocumentId = u64;
pub type Version = u64;

/// アンドゥの履歴に積む項目の上限です。超えた分は古いものから捨てます。
pub const DEFAULT_UNDO_LIMIT: usize = 1000;

/// 編集を適用した結果です。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    /// 適用後の version です。
    pub version: Version,
    /// クライアントが適用すべき変更です。編集の要求では空です。
    pub edits: Vec<TextEdit>,
    /// 適用後に置く選択です。
    pub selection: Vec<Selection>,
}

#[derive(Debug, Clone)]
struct UndoEntry {
    /// まとめる単位です。同じ ID が続く間は 1 つの項目にします。
    transaction_id: u64,
    /// 1 つの項目を戻すための変更です。適用した順に並びます。
    inverse: Vec<TextEdit>,
    /// やり直すための変更です。元の座標で並びます。
    forward: Vec<TextEdit>,
    selection_before: Vec<Selection>,
    selection_after: Vec<Selection>,
}

/// 1 つの文書の本文と version、アンドゥの履歴です。
///
/// 本文の正はコアにあります（`docs/adr/0003-editor-state.md`）。カーソルと選択は
/// クライアントが持つため、編集の前後の選択を呼び出し側から受け取ります。
pub struct Buffer {
    id: DocumentId,
    uri: Uri,
    rope: Rope,
    version: Version,
    saved_version: Version,
    undo_limit: usize,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
}

impl Buffer {
    pub fn new(id: DocumentId, uri: Uri, text: impl Into<String>) -> Self {
        Self {
            id,
            uri,
            rope: Rope::from_str(&text.into()),
            version: 0,
            saved_version: 0,
            undo_limit: DEFAULT_UNDO_LIMIT,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// ファイルを読み込んでバッファを作ります。
    pub fn open(id: DocumentId, uri: Uri, path: impl AsRef<Path>) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::new(id, uri, text))
    }

    /// アンドゥの履歴の上限を変えます。テストと診断に使います。
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
        self.rope.len_lines()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub fn mark_saved(&mut self) {
        self.saved_version = self.version;
    }

    /// UTF-16 単位の位置を、コアの char index に変換します。
    ///
    /// 行末を超える位置は失敗します。サロゲートペアの途中を指した場合は、
    /// その文字の先頭に切り下げます。
    pub fn position_to_char(&self, position: Position) -> Result<usize, EditError> {
        let line_index = position.line as usize;
        if line_index >= self.rope.len_lines() {
            return Err(EditError::InvalidPosition(position));
        }

        let line = self.rope.line(line_index);
        let mut units = 0u32;
        let mut chars = 0usize;
        let mut clamped = false;

        for ch in line.chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            if position.character <= units {
                break;
            }
            let width = ch.len_utf16() as u32;
            if position.character < units + width {
                clamped = true;
                break;
            }
            units += width;
            chars += 1;
        }

        if position.character > units && !clamped {
            return Err(EditError::InvalidPosition(position));
        }

        Ok(self.rope.line_to_char(line_index) + chars)
    }

    /// char index を UTF-16 単位の位置に変換します。
    pub fn char_to_position(&self, char_index: usize) -> Result<Position, EditError> {
        if char_index > self.rope.len_chars() {
            return Err(EditError::InvalidOffset(char_index));
        }

        let line_index = self.rope.char_to_line(char_index);
        let line_start = self.rope.line_to_char(line_index);
        let character: u32 = self
            .rope
            .slice(line_start..char_index)
            .chars()
            .map(|ch| ch.len_utf16() as u32)
            .sum();

        Ok(Position::new(line_index as u32, character))
    }

    /// 範囲の本文を取り出します。
    pub fn slice(&self, range: Range) -> Result<String, EditError> {
        let start = self.position_to_char(range.start)?;
        let end = self.position_to_char(range.end)?;
        if start > end {
            return Err(EditError::InvalidRange(range));
        }
        Ok(self.rope.slice(start..end).to_string())
    }

    /// 編集のトランザクションを適用します。
    ///
    /// `base_version` が現在の version と合わない場合は何も変えません。呼び出し側は
    /// 本文を送り直して再同期します（`docs/protocol.md` の「バッファ差分通知」）。
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

        let inverse = self.apply_batch(&edits)?;
        self.version += 1;
        self.redo.clear();

        let grouped = self
            .undo
            .last()
            .is_some_and(|entry| entry.transaction_id == transaction_id);

        if grouped {
            let entry = self.undo.last_mut().expect("checked just above");
            entry.inverse.extend(inverse);
            entry.forward.extend(edits.iter().cloned());
            entry.selection_after = selection_after.clone();
        } else {
            self.undo.push(UndoEntry {
                transaction_id,
                inverse,
                forward: edits,
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

    /// 直前の項目を戻します。戻す変更がなければ [`None`] です。
    pub fn undo(&mut self) -> Result<Option<Applied>, EditError> {
        let Some(entry) = self.undo.pop() else {
            return Ok(None);
        };

        let inverse = entry.inverse.clone();
        self.apply_batch(&inverse)?;
        self.version += 1;

        let selection = entry.selection_before.clone();
        self.redo.push(entry);

        Ok(Some(Applied {
            version: self.version,
            edits: inverse,
            selection,
        }))
    }

    /// 戻した項目をやり直します。やり直す変更がなければ [`None`] です。
    pub fn redo(&mut self) -> Result<Option<Applied>, EditError> {
        let Some(entry) = self.redo.pop() else {
            return Ok(None);
        };

        let forward = entry.forward.clone();
        self.apply_batch(&forward)?;
        self.version += 1;

        let selection = entry.selection_after.clone();
        self.undo.push(entry);

        Ok(Some(Applied {
            version: self.version,
            edits: forward,
            selection,
        }))
    }

    /// 1 回分の変更を適用し、戻すための変更を返します。
    ///
    /// `edits` は同じ座標系（適用前の本文）で表されている必要があります。位置の
    /// 昇順に並べ替え、前の変更でずれた分を足しながら適用します。
    fn apply_batch(&mut self, edits: &[TextEdit]) -> Result<Vec<TextEdit>, EditError> {
        let mut resolved = Vec::with_capacity(edits.len());

        for edit in edits {
            let start = self.position_to_char(edit.range.start)?;
            let end = self.position_to_char(edit.range.end)?;
            if start > end {
                return Err(EditError::InvalidRange(edit.range));
            }
            resolved.push((start, end, edit.text.as_str()));
        }

        resolved.sort_by_key(|(start, _, _)| *start);
        for pair in resolved.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(EditError::OverlappingEdits);
            }
        }

        let mut delta: isize = 0;
        let mut inverse = Vec::with_capacity(resolved.len());

        for (start, end, text) in resolved {
            let start = (start as isize + delta) as usize;
            let end = (end as isize + delta) as usize;

            let removed = self.rope.slice(start..end).to_string();
            let inserted = text.chars().count();

            self.rope.remove(start..end);
            self.rope.insert(start, text);

            inverse.push(TextEdit {
                range: Range::new(
                    self.char_to_position(start)?,
                    self.char_to_position(start + inserted)?,
                ),
                text: removed,
            });

            delta += inserted as isize - (end - start) as isize;
        }

        Ok(inverse)
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
        assert_eq!(undone.edits, vec![edit(at(0, 1), at(0, 3), "b")]);
        assert_eq!(undone.selection, before);
        assert_eq!(buffer.text(), "abc");

        let redone = buffer.redo().expect("redo").expect("entry");
        assert_eq!(redone.version, 3);
        assert_eq!(redone.edits, vec![edit(at(0, 1), at(0, 2), "XY")]);
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
