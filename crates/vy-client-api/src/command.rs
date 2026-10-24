//! Built-in command identifiers owned by the client.
//!
//! These are part of the client API surface: `commands.execute` accepts the
//! same ids, and `CommandId.ts` is generated from them. They live here rather
//! than in `vy-app` so the codegen can run without building the GPUI client.

use strum::VariantArray;

macro_rules! define_command_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$doc:meta])*
                $variant:ident => ($id:literal, $title:literal),
            )*
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $(
                $(#[$doc])*
                $variant,
            )*
        }

        impl $name {
            /// Stable identifier used by key bindings and `commands.execute`.
            pub fn id(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)*
                }
            }

            pub fn title(self) -> &'static str {
                match self {
                    $(Self::$variant => $title,)*
                }
            }

            pub fn parse(id: &str) -> Option<Self> {
                match id {
                    $($id => Some(Self::$variant),)*
                    _ => None,
                }
            }

            pub fn all() -> &'static [Self] {
                &[$(Self::$variant,)*]
            }
        }
    };
}

define_command_enum! {
    /// A command that acts on a document rather than the cursor.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DocumentCommand {
        Open => ("vy.editor.open", "Editor Open"),
        Save => ("vy.document.save", "Document Save"),
        Switch => ("vy.editor.switch_document", "Switch Document"),
    }
}

define_command_enum! {
    /// A command handled by the client UI instead of the editor.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum UiCommand {
        CommandPalette => ("vy.ui.command_palette", "Command Palette"),
        ExCommandLine => ("vy.ui.ex_command_line", "Ex Command Line"),
        ScriptEval => ("vy.script.eval", "Script Evaluate"),
        ReconnectServer => ("vy.reconnect_server", "Reconnect Server"),
        ViewOpen => ("vy.view.open", "Open View"),
        ViewClose => ("vy.view.close", "Close View"),
        ViewToggle => ("vy.view.toggle", "Toggle View"),
    }
}

define_command_enum! {
    /// A command that scrolls or positions the view within an editor pane.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ViewCommand {
        ScrollHalfPageDown => ("vy.view.scroll_half_page_down", "Scroll Half Page Down"),
        ScrollHalfPageUp => ("vy.view.scroll_half_page_up", "Scroll Half Page Up"),
        CenterCursor => ("vy.view.center_cursor", "Center Cursor"),
        ScrollCursorTop => ("vy.view.scroll_cursor_top", "Scroll Cursor Top"),
        ScrollCursorBottom => ("vy.view.scroll_cursor_bottom", "Scroll Cursor Bottom"),
    }
}

/// A command that changes the window layout instead of the buffer.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr, strum::EnumString, strum::VariantArray,
)]
pub enum PaneCommand {
    #[strum(serialize = "vy.pane.split_left")]
    SplitLeft,
    #[strum(serialize = "vy.pane.split_right")]
    SplitRight,
    #[strum(serialize = "vy.pane.split_up")]
    SplitUp,
    #[strum(serialize = "vy.pane.split_down")]
    SplitDown,
    #[strum(serialize = "vy.pane.close")]
    Close,
    #[strum(serialize = "vy.pane.focus_left")]
    FocusLeft,
    #[strum(serialize = "vy.pane.focus_down")]
    FocusDown,
    #[strum(serialize = "vy.pane.focus_up")]
    FocusUp,
    #[strum(serialize = "vy.pane.focus_right")]
    FocusRight,
    #[strum(serialize = "vy.pane.next")]
    Next,
}

impl PaneCommand {
    /// Stable identifier used by key bindings and `commands.execute`.
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn parse(id: &str) -> Option<Self> {
        id.parse().ok()
    }

    pub fn all() -> &'static [Self] {
        Self::VARIANTS
    }
}

/// A command implemented in Rust.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    strum::IntoStaticStr,
    strum::EnumString,
    strum::VariantArray,
)]
pub enum BuiltinCommand {
    #[strum(serialize = "vy.motion.left")]
    MoveLeft,
    #[strum(serialize = "vy.motion.right")]
    MoveRight,
    #[strum(serialize = "vy.motion.up")]
    MoveUp,
    #[strum(serialize = "vy.motion.down")]
    MoveDown,
    #[strum(serialize = "vy.motion.word_forward")]
    WordForward,
    #[strum(serialize = "vy.motion.word_backward")]
    WordBackward,
    #[strum(serialize = "vy.motion.word_end")]
    WordEnd,
    #[strum(serialize = "vy.motion.line_start")]
    LineStart,
    #[strum(serialize = "vy.motion.line_end")]
    LineEnd,
    #[strum(serialize = "vy.motion.document_start")]
    DocumentStart,
    #[strum(serialize = "vy.motion.document_end")]
    DocumentEnd,
    #[strum(serialize = "vy.edit.delete")]
    Delete,
    #[strum(serialize = "vy.edit.paste_after")]
    PasteAfter,
    #[strum(serialize = "vy.edit.paste_before")]
    PasteBefore,
    #[strum(serialize = "vy.edit.repeat")]
    Repeat,
    #[strum(serialize = "vy.edit.last_visual")]
    LastVisual,
    #[strum(serialize = "vy.mode.insert")]
    InsertHere,
    #[strum(serialize = "vy.mode.insert_after")]
    InsertAfter,
    #[strum(serialize = "vy.mode.insert_line_end")]
    InsertLineEnd,
    #[strum(serialize = "vy.mode.insert_line_start")]
    InsertLineStart,
    #[strum(serialize = "vy.mode.open_below")]
    OpenBelow,
    #[strum(serialize = "vy.mode.open_above")]
    OpenAbove,
    #[strum(serialize = "vy.mode.select")]
    SelectMode,
    #[strum(serialize = "vy.mode.select_line")]
    SelectLineMode,
    #[strum(serialize = "vy.mode.select_block")]
    SelectBlockMode,
    #[strum(serialize = "vy.mode.normal")]
    NormalMode,
    #[strum(serialize = "vy.history.undo")]
    Undo,
    #[strum(serialize = "vy.history.redo")]
    Redo,
    #[strum(serialize = "vy.insert.newline")]
    InsertNewline,
    #[strum(serialize = "vy.insert.tab")]
    InsertTab,
    #[strum(serialize = "vy.insert.backspace")]
    InsertBackspace,
    #[strum(serialize = "vy.insert.left")]
    InsertLeft,
    #[strum(serialize = "vy.insert.right")]
    InsertRight,
    #[strum(serialize = "vy.insert.up")]
    InsertUp,
    #[strum(serialize = "vy.insert.down")]
    InsertDown,
    #[strum(serialize = "vy.insert.home")]
    InsertHome,
    #[strum(serialize = "vy.insert.end")]
    InsertEnd,
    #[strum(serialize = "vy.operator.delete")]
    OperatorDelete,
    #[strum(serialize = "vy.operator.yank")]
    OperatorYank,
    #[strum(serialize = "vy.edit.yank_line")]
    YankLine,
    #[strum(serialize = "vy.operator.inner")]
    OperatorInner,
    #[strum(serialize = "vy.operator.around")]
    OperatorAround,
    #[strum(serialize = "vy.text_object.word")]
    TextObjectWord,
    #[strum(serialize = "vy.text_object.paragraph")]
    TextObjectParagraph,
    #[strum(serialize = "vy.text_object.paren")]
    TextObjectParen,
    #[strum(serialize = "vy.text_object.bracket")]
    TextObjectBracket,
    #[strum(serialize = "vy.text_object.brace")]
    TextObjectBrace,
    #[strum(serialize = "vy.text_object.angle")]
    TextObjectAngle,
    #[strum(serialize = "vy.text_object.double_quote")]
    TextObjectDoubleQuote,
    #[strum(serialize = "vy.text_object.single_quote")]
    TextObjectSingleQuote,
    #[strum(serialize = "vy.text_object.backtick")]
    TextObjectBacktick,
}

impl BuiltinCommand {
    /// Stable identifier used by key bindings and `commands.execute`.
    pub fn id(self) -> &'static str {
        self.into()
    }

    pub fn parse(id: &str) -> Option<Self> {
        id.parse().ok()
    }

    /// Every built-in command. Used by `commands.getCommands`.
    pub fn all() -> &'static [BuiltinCommand] {
        Self::VARIANTS
    }
}
