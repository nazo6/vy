//! Semantic design tokens and theme configuration for `vy-app`.

use gpui::{App, Context, Global, Rgba, rgb};

/// Semantic design tokens for the entire UI.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub background: BackgroundTokens,
    pub foreground: ForegroundTokens,
    pub border: BorderTokens,
    pub editor: EditorTokens,
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

    /// Access the global theme from an `App` reference.
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }
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
        let theme = Theme::dark();
        assert_eq!(theme.background.app, rgb(0x1e1e1e));
        assert_eq!(theme.statusline.normal, rgb(0x007acc));
    }
}
