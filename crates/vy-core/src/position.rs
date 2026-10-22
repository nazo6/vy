use serde::{Deserialize, Serialize};

/// 0 始まりの行と、UTF-16 コード単位の桁です。
///
/// JS の文字列と同じ数え方にそろえています。コアの内部では char index で持ち、
/// 境界でこの型に変換します（`docs/architecture.md` の「位置の表現」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// 2 つの位置で挟まれた範囲です。端は含みません。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// 同じ位置を指す空の範囲です。
    pub const fn empty(at: Position) -> Self {
        Self { start: at, end: at }
    }
}

/// カーソルの位置です。`anchor` と `head` が同じなら点、違えば選択です。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub const fn caret(at: Position) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }
}
