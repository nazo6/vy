use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::position::{Position, Range};

/// 本文への 1 つの変更です。`range` を `text` で置き換えます。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    pub text: String,
}

impl TextEdit {
    pub fn new(range: Range, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    /// 位置への挿入です。
    pub fn insert(at: Position, text: impl Into<String>) -> Self {
        Self::new(Range::empty(at), text)
    }

    /// 範囲の削除です。
    pub fn delete(range: Range) -> Self {
        Self::new(range, "")
    }
}

/// 適用できなかった変更の理由です。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("base version {base} が現在の version {current} と一致しません")]
    VersionMismatch { base: u64, current: u64 },

    #[error("位置 {0:?} は文書の外を指しています")]
    InvalidPosition(Position),

    #[error("char index {0} は文書の外を指しています")]
    InvalidOffset(usize),

    #[error("範囲 {0:?} の始点が終点より後ろにあります")]
    InvalidRange(Range),

    #[error("変更は位置の昇順で、重ならずに並んでいる必要があります")]
    OverlappingEdits,
}
