//! Semantic design tokens and theme configuration for `vy-app`.

use gpui::{App, Context, Global, Rgba, rgb};
use serde_json::Value;
use vy_app::config::Config;

pub use vy_app::color::ThemeColor;

/// Semantic design tokens for the entire UI.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub background: BackgroundTokens,
    pub foreground: ForegroundTokens,
    pub border: BorderTokens,
    pub editor: EditorTokens,
    pub titlebar: TitlebarTokens,
    pub statusline: StatuslineTokens,
    pub list: ListTokens,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundTokens {
    pub app: Rgba,
    pub surface: Rgba,
    pub overlay: Rgba,
    pub split_active: Rgba,
    pub split_inactive: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForegroundTokens {
    pub default: Rgba,
    pub muted: Rgba,
    pub subtle: Rgba,
    pub accent: Rgba,
    pub prompt: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BorderTokens {
    pub subtle: Rgba,
    pub focused: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorTokens {
    pub background: Rgba,
    pub text: Rgba,
    pub gutter: Rgba,
    pub selection: Rgba,
    pub caret: Rgba,
    pub line_highlight: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TitlebarTokens {
    pub background: Rgba,
    pub border: Rgba,
    pub foreground: Rgba,
    pub muted: Rgba,
    pub dirty: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatuslineTokens {
    pub background: Rgba,
    pub foreground: Rgba,
    pub uri: Rgba,
    pub position: Rgba,
    pub message: Rgba,
    pub normal: Rgba,
    pub insert: Rgba,
    pub select: Rgba,
    pub info: Rgba,
    pub success: Rgba,
    pub warning: Rgba,
    pub error: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListTokens {
    pub active_background: Rgba,
    pub active_foreground: Rgba,
    pub match_highlight: Rgba,
}

impl Global for Theme {}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Default dark theme (VS Code Dark Modern / Vy default).
    pub fn dark() -> Self {
        Self {
            background: BackgroundTokens {
                app: rgb(0x1e1e1e),
                surface: rgb(0x252526),
                overlay: rgb(0x252526),
                split_active: rgb(0x2b5e8c),
                split_inactive: rgb(0x2d2d2d),
            },
            foreground: ForegroundTokens {
                default: rgb(0xd4d4d4),
                muted: rgb(0x8a8a8a),
                subtle: rgb(0x6a6a6a),
                accent: rgb(0x9cdcfe),
                prompt: rgb(0x9a9a9a),
            },
            border: BorderTokens {
                subtle: rgb(0x3c3c3c),
                focused: rgb(0x2b5e8c),
            },
            editor: EditorTokens {
                background: rgb(0x1e1e1e),
                text: rgb(0xd4d4d4),
                gutter: rgb(0x5a5a5a),
                selection: rgb(0x264f78),
                caret: rgb(0xd4d4d4),
                line_highlight: rgb(0x282828),
            },
            titlebar: TitlebarTokens {
                background: rgb(0x181818),
                border: rgb(0x2b2b2b),
                foreground: rgb(0xcccccc),
                muted: rgb(0x8a8a8a),
                dirty: rgb(0x9cdcfe),
            },
            statusline: StatuslineTokens {
                background: rgb(0x181818),
                foreground: rgb(0xffffff),
                uri: rgb(0xcccccc),
                position: rgb(0x9cdcfe),
                message: rgb(0x9a9a9a),
                normal: rgb(0x007acc),
                insert: rgb(0x008000),
                select: rgb(0xaf00db),
                info: rgb(0x007acc),
                success: rgb(0x4ec9b0),
                warning: rgb(0xce9178),
                error: rgb(0xf44747),
            },
            list: ListTokens {
                active_background: rgb(0x094771),
                active_foreground: rgb(0xffffff),
                match_highlight: rgb(0x9cdcfe),
            },
        }
    }

    /// Default light theme.
    pub fn light() -> Self {
        Self {
            background: BackgroundTokens {
                app: rgb(0xffffff),
                surface: rgb(0xf3f3f3),
                overlay: rgb(0xf3f3f3),
                split_active: rgb(0x007acc),
                split_inactive: rgb(0xe5e5e5),
            },
            foreground: ForegroundTokens {
                default: rgb(0x1f1f1f),
                muted: rgb(0x6e6e6e),
                subtle: rgb(0x8c8c8c),
                accent: rgb(0x005fb8),
                prompt: rgb(0x616161),
            },
            border: BorderTokens {
                subtle: rgb(0xd4d4d4),
                focused: rgb(0x007acc),
            },
            editor: EditorTokens {
                background: rgb(0xffffff),
                text: rgb(0x1f1f1f),
                gutter: rgb(0x6e6e6e),
                selection: rgb(0xadd6ff),
                caret: rgb(0x000000),
                line_highlight: rgb(0xf0f0f0),
            },
            titlebar: TitlebarTokens {
                background: rgb(0xf3f3f3),
                border: rgb(0xe5e5e5),
                foreground: rgb(0x333333),
                muted: rgb(0x717171),
                dirty: rgb(0x005fb8),
            },
            statusline: StatuslineTokens {
                background: rgb(0xf3f3f3),
                foreground: rgb(0x333333),
                uri: rgb(0x444444),
                position: rgb(0x005fb8),
                message: rgb(0x616161),
                normal: rgb(0x007acc),
                insert: rgb(0x008000),
                select: rgb(0xaf00db),
                info: rgb(0x007acc),
                success: rgb(0x388a34),
                warning: rgb(0xbf8803),
                error: rgb(0xe51400),
            },
            list: ListTokens {
                active_background: rgb(0x0060c0),
                active_foreground: rgb(0xffffff),
                match_highlight: rgb(0x005fb8),
            },
        }
    }

    /// Builds the theme selected by the configuration and applies color overrides.
    ///
    /// `ui.theme` picks the built-in `dark` or `light` base. The accent colors
    /// below can then be replaced from settings so a user can tune the active
    /// pane and focus indicators without a custom theme file.
    pub fn from_config(config: &Config) -> Self {
        let mut theme = if config.theme() == "light" {
            Self::light()
        } else {
            Self::dark()
        };

        if let Some(color) = config_color(config, "ui.activePaneBorder") {
            theme.background.split_active = color;
        }
        if let Some(color) = config_color(config, "ui.inactivePaneBorder") {
            theme.background.split_inactive = color;
        }
        if let Some(color) = config_color(config, "ui.focusBorder") {
            theme.border.focused = color;
        }
        if let Some(color) = config_color(config, "ui.selectionBackground") {
            theme.list.active_background = color;
        }
        if let Some(color) = config_color(config, "ui.selectionForeground") {
            theme.list.active_foreground = color;
        }

        theme
    }

    /// Access the global theme from an `App` reference.
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }
}

/// Reads a color from settings. Unset or unparsable values keep the theme default.
fn config_color(config: &Config, key: &str) -> Option<Rgba> {
    config
        .get(key)
        .and_then(Value::as_str)
        .and_then(parse_hex_color)
}

/// Parses `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa`. The leading `#` is optional.
///
/// `transparent` is accepted as an explicit fully transparent color, which lets
/// a setting hide an indicator without falling back to the theme default.
pub fn parse_hex_color(value: &str) -> Option<Rgba> {
    let text = value.trim();
    if text.eq_ignore_ascii_case("transparent") {
        return Some(gpui::rgba(0x00000000));
    }

    let text = text.strip_prefix('#').unwrap_or(text);
    if !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    let expand = |digit: char| -> Option<u32> { digit.to_digit(16).map(|value| value * 0x11) };

    let rgba = match text.len() {
        3 => {
            let mut digits = text.chars();
            let r = expand(digits.next()?)?;
            let g = expand(digits.next()?)?;
            let b = expand(digits.next()?)?;
            (r << 24) | (g << 16) | (b << 8) | 0xff
        }
        4 => {
            let mut digits = text.chars();
            let r = expand(digits.next()?)?;
            let g = expand(digits.next()?)?;
            let b = expand(digits.next()?)?;
            let a = expand(digits.next()?)?;
            (r << 24) | (g << 16) | (b << 8) | a
        }
        6 => (u32::from_str_radix(text, 16).ok()? << 8) | 0xff,
        8 => u32::from_str_radix(text, 16).ok()?,
        _ => return None,
    };

    Some(gpui::rgba(rgba))
}

/// Helper trait to access the current theme from an application context.
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

impl<T> ActiveTheme for Context<'_, T> {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_default_theme_tokens() {
        let dark = Theme::dark();
        assert_eq!(dark.background.app, rgb(0x1e1e1e));
        assert_eq!(dark.editor.line_highlight, rgb(0x282828));
        assert_eq!(dark.statusline.normal, rgb(0x007acc));

        let light = Theme::light();
        assert_eq!(light.background.app, rgb(0xffffff));
        assert_eq!(light.editor.line_highlight, rgb(0xf0f0f0));
    }

    #[test]
    fn parses_color_notation() {
        assert_eq!(parse_hex_color("#abc"), Some(rgb(0xaabbcc)));
        assert_eq!(parse_hex_color("abc"), Some(rgb(0xaabbcc)));
        assert_eq!(parse_hex_color("#112233"), Some(rgb(0x112233)));
        assert_eq!(parse_hex_color("#11223380"), Some(gpui::rgba(0x11223380)));
        assert_eq!(parse_hex_color("transparent"), Some(gpui::rgba(0x00000000)));
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("#12"), None);
        assert_eq!(parse_hex_color("nope"), None);
    }

    #[test]
    fn settings_override_accent_colors() {
        let mut config = Config::with_defaults();
        assert_eq!(Theme::from_config(&config), Theme::dark());

        config.set("ui.theme", Value::from("light"));
        let light = Theme::from_config(&config);
        assert_eq!(light.background.split_active, rgb(0x007acc));

        config.set("ui.activePaneBorder", Value::from("#ff0000"));
        config.set("ui.inactivePaneBorder", Value::from("#00ff00"));
        config.set("ui.focusBorder", Value::from("#0000ff"));
        config.set("ui.selectionBackground", Value::from("#123456"));
        config.set("ui.selectionForeground", Value::from("#abcdef"));

        let theme = Theme::from_config(&config);
        assert_eq!(theme.background.split_active, rgb(0xff0000));
        assert_eq!(theme.background.split_inactive, rgb(0x00ff00));
        assert_eq!(theme.border.focused, rgb(0x0000ff));
        assert_eq!(theme.list.active_background, rgb(0x123456));
        assert_eq!(theme.list.active_foreground, rgb(0xabcdef));
        // Unrelated tokens still come from the base theme.
        assert_eq!(theme.background.app, rgb(0xffffff));
    }

    #[test]
    fn blank_or_invalid_overrides_keep_theme_defaults() {
        let mut config = Config::with_defaults();
        config.set("ui.activePaneBorder", Value::from(""));
        config.set("ui.focusBorder", Value::from("not-a-color"));

        let theme = Theme::from_config(&config);
        assert_eq!(
            theme.background.split_active,
            Theme::dark().background.split_active
        );
        assert_eq!(theme.border.focused, Theme::dark().border.focused);
    }
}
