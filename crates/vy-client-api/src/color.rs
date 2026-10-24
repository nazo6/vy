//! Semantic color tokens scripts can use for slots and widgets.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A color named after a theme token.
///
/// The client maps each token to a concrete color, so a script never hard-codes
/// a value and themes keep working. The tokens are closed: adding one is a
/// client change, not a plugin change.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    TS,
    strum::IntoStaticStr,
    strum::EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ThemeColor {
    Default,
    Muted,
    Subtle,
    Accent,
    Prompt,
    Primary,
    Info,
    Success,
    Warning,
    Danger,
    Error,
    Secondary,
}

impl ThemeColor {
    pub fn parse(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }
}
