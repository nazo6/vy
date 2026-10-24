//! Editor mode and keymap value types shared with the plugin API.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Editor mode. It is part of the keymap context and of the client's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Normal,
    Insert,
    Select,
    SelectLine,
    SelectBlock,
}

/// Accepts the same aliases as [`Mode::parse`], so scripts written against
/// older names keep working.
impl<'de> Deserialize<'de> for Mode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::parse(&name).ok_or_else(|| serde::de::Error::custom(format!("unknown mode: {name}")))
    }
}

impl Mode {
    /// Whether this mode is any visual/select variant.
    pub fn is_select(self) -> bool {
        matches!(self, Self::Select | Self::SelectLine | Self::SelectBlock)
    }

    /// Lowercase name used by `when` expressions.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Insert => "insert",
            Self::Select => "select",
            Self::SelectLine => "select_line",
            Self::SelectBlock => "select_block",
        }
    }

    /// Uppercase label shown in the status line.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Select => "VISUAL",
            Self::SelectLine => "VISUAL LINE",
            Self::SelectBlock => "VISUAL BLOCK",
        }
    }

    /// Parses the lowercase name used by `when` and by `editor.setMode`.
    ///
    /// Aliases such as `visual` and `select-line` are accepted so that scripts
    /// written against older names keep working.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "insert" => Some(Self::Insert),
            "select" | "visual" => Some(Self::Select),
            "select_line" | "select-line" | "visual_line" | "visual-line" => Some(Self::SelectLine),
            "select_block" | "select-block" | "visual_block" | "visual-block" => {
                Some(Self::SelectBlock)
            }
            _ => None,
        }
    }
}
