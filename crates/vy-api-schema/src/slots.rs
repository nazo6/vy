//! Slot value types shared with scripts.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Horizontal alignment of a statusline item.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    strum::IntoStaticStr,
    strum::EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[ts(export)]
pub enum StatuslineAlignment {
    Left,
    Center,
    Right,
}

impl StatuslineAlignment {
    pub fn parse(value: &str) -> Option<Self> {
        value.parse().ok()
    }
}
