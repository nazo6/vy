use ropey::Rope;

use crate::edit::{EditError, TextEdit};
use crate::position::{Position, Range};

/// 本文だけを持つ型です。
///
/// 位置の数え方（UTF-16 と char index の変換）と、変更の適用を 1 か所にまとめます。
/// `Buffer` は version とアンドゥの履歴を足したもの、クライアントは本文の写しとして
/// これを使います。
pub struct Text {
    rope: Rope,
}

impl Text {
    pub fn new(text: impl AsRef<str>) -> Self {
        Self {
            rope: Rope::from_str(text.as_ref()),
        }
    }

    pub fn as_string(&self) -> String {
        self.rope.to_string()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// 行の内容の長さを UTF-16 のコード単位で返します。改行は含みません。
    pub fn line_len_utf16(&self, line: usize) -> Option<u32> {
        if line >= self.rope.len_lines() {
            return None;
        }

        let units: usize = self
            .rope
            .line(line)
            .chars()
            .take_while(|character| *character != '\n' && *character != '\r')
            .map(|character| character.len_utf16())
            .sum();

        Some(units as u32)
    }

    pub fn line_string(&self, line: usize) -> Option<String> {
        if line >= self.rope.len_lines() {
            return None;
        }

        let content: String = self
            .rope
            .line(line)
            .chars()
            .take_while(|character| *character != '\n' && *character != '\r')
            .collect();

        Some(content)
    }

    /// 位置の文字を返します。行末や文書の外では [`None`] です。
    pub fn char_at(&self, position: Position) -> Option<char> {
        let index = self.position_to_char(position).ok()?;
        self.char_at_index(index)
    }

    pub fn char_at_index(&self, index: usize) -> Option<char> {
        self.rope.get_char(index)
    }

    /// UTF-16 単位の位置を、char index に変換します。
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

        for character in line.chars() {
            if character == '\n' || character == '\r' {
                break;
            }
            if position.character <= units {
                break;
            }
            let width = character.len_utf16() as u32;
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
            .map(|character| character.len_utf16() as u32)
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

    /// 1 回分の変更を適用し、戻すための変更を返します。
    ///
    /// 変更は同じ座標系（適用前の本文）で表されている必要があります。位置の昇順に
    /// 並べ替え、前の変更でずれた分を足しながら適用します。
    pub fn apply(&mut self, edits: &[TextEdit]) -> Result<Vec<TextEdit>, EditError> {
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

    fn at(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    #[test]
    fn counts_positions_in_utf16_units() {
        let mut text = Text::new("a😀b\n日本");

        assert_eq!(text.position_to_char(at(0, 1)).expect("char"), 1);
        assert_eq!(text.position_to_char(at(0, 2)).expect("clamp"), 1);
        assert_eq!(text.position_to_char(at(0, 3)).expect("char"), 2);
        assert_eq!(text.char_to_position(2).expect("position"), at(0, 3));
        assert_eq!(text.position_to_char(at(1, 2)).expect("char"), 6);
        assert_eq!(text.char_to_position(6).expect("position"), at(1, 2));

        assert_eq!(
            text.position_to_char(at(0, 5)),
            Err(EditError::InvalidPosition(at(0, 5)))
        );

        text.apply(&[TextEdit::new(Range::new(at(1, 0), at(1, 2)), "日本酒")])
            .expect("apply");
        assert_eq!(text.as_string(), "a😀b\n日本酒");
    }

    #[test]
    fn reports_line_lengths_without_breaks() {
        let text = Text::new("a😀b\r\n日本\n");
        assert_eq!(text.line_len_utf16(0), Some(4));
        assert_eq!(text.line_len_utf16(1), Some(2));
        assert_eq!(text.line_len_utf16(2), Some(0));
        assert_eq!(text.line_len_utf16(3), None);
        assert_eq!(text.line_string(1).as_deref(), Some("日本"));
    }

    #[test]
    fn applies_edits_in_ascending_order_with_shifting_offsets() {
        let mut text = Text::new("aaa");
        text.apply(&[
            TextEdit::new(Range::new(at(0, 0), at(0, 1)), "XX"),
            TextEdit::new(Range::new(at(0, 2), at(0, 3)), "YY"),
        ])
        .expect("apply");
        assert_eq!(text.as_string(), "XXaYY");
    }

    #[test]
    fn returns_inverse_edits() {
        let mut text = Text::new("abc");
        let inverse = text
            .apply(&[TextEdit::new(Range::new(at(0, 1), at(0, 2)), "XY")])
            .expect("apply");
        assert_eq!(
            inverse,
            vec![TextEdit::new(Range::new(at(0, 1), at(0, 3)), "b")]
        );
    }

    #[test]
    fn rejects_overlapping_edits() {
        let mut text = Text::new("abcd");
        assert_eq!(
            text.apply(&[
                TextEdit::new(Range::new(at(0, 0), at(0, 3)), "x"),
                TextEdit::new(Range::new(at(0, 2), at(0, 4)), "y"),
            ]),
            Err(EditError::OverlappingEdits)
        );
        assert_eq!(text.as_string(), "abcd");
    }
}
