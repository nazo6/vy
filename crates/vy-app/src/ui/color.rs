//! Maps semantic color tokens to GPUI colors.

use gpui::Rgba;
use vy_api_schema::ThemeColor;

use crate::theme::Theme;

/// Resolves a token to the color the current theme gives it.
pub fn color(theme: &Theme, token: ThemeColor) -> Rgba {
    match token {
        ThemeColor::Default => theme.foreground.default,
        ThemeColor::Muted | ThemeColor::Secondary => theme.foreground.muted,
        ThemeColor::Subtle => theme.foreground.subtle,
        ThemeColor::Accent => theme.foreground.accent,
        ThemeColor::Prompt => theme.foreground.prompt,
        ThemeColor::Primary | ThemeColor::Info => theme.statusline.info,
        ThemeColor::Success => theme.statusline.success,
        ThemeColor::Warning => theme.statusline.warning,
        ThemeColor::Danger | ThemeColor::Error => theme.statusline.error,
    }
}
