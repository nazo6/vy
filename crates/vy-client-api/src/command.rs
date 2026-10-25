//! Built-in command identifiers owned by the client.
//!
//! These are part of the client API surface: `commands.execute` accepts the
//! same ids, and `BuiltinCommandMap.ts` is generated from them. They live here
//! rather than in `vy-app` so the codegen can run without building the GPUI
//! client.

use serde::{Deserialize, Serialize};
use strum::VariantArray;

/// The user-facing group a command belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CommandCategory {
    File,
    Edit,
    Selection,
    View,
    Pane,
    App,
    Settings,
    Plugin,
}

/// Whether the command appears in the command palette list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PaletteVisibility {
    Listed,
    Hidden,
}

/// The ranges a command accepts. `None` rejects a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RangeSpec {
    None,
    CurrentLine,
    Selection,
    WholeDocument,
}

/// The kind of one positional argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ArgKind {
    Text,
    Path,
    Document,
    Number,
    Choice,
}

/// One positional argument declared by a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arg {
    pub name: &'static str,
    pub kind: ArgKind,
    /// Allowed values when `kind` is [`ArgKind::Choice`].
    pub choices: &'static [&'static str],
    pub required: bool,
    /// When true, this argument consumes the rest of the line.
    pub rest: bool,
}

impl Arg {
    pub const fn required(name: &'static str, kind: ArgKind) -> Self {
        Self {
            name,
            kind,
            choices: &[],
            required: true,
            rest: false,
        }
    }

    pub const fn optional(name: &'static str, kind: ArgKind) -> Self {
        Self {
            name,
            kind,
            choices: &[],
            required: false,
            rest: false,
        }
    }

    pub const fn rest(name: &'static str, kind: ArgKind) -> Self {
        Self {
            name,
            kind,
            choices: &[],
            required: true,
            rest: true,
        }
    }

    pub const fn choice(
        name: &'static str,
        choices: &'static [&'static str],
        required: bool,
    ) -> Self {
        Self {
            name,
            kind: ArgKind::Choice,
            choices,
            required,
            rest: false,
        }
    }
}

/// How a command receives its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArgSpec {
    #[default]
    None,
    Positionals(&'static [Arg]),
}

impl ArgSpec {
    pub const fn args(self) -> &'static [Arg] {
        match self {
            Self::None => &[],
            Self::Positionals(args) => args,
        }
    }

    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

/// Metadata that every command declares next to its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMeta {
    pub category: CommandCategory,
    pub palette: PaletteVisibility,
    pub ex_names: &'static [&'static str],
    pub args: ArgSpec,
    pub range: RangeSpec,
    pub bang: bool,
}

impl CommandMeta {
    /// A command shown in the palette.
    pub const fn listed(category: CommandCategory, ex_names: &'static [&'static str]) -> Self {
        Self {
            category,
            palette: PaletteVisibility::Listed,
            ex_names,
            args: ArgSpec::None,
            range: RangeSpec::None,
            bang: false,
        }
    }

    /// A key-only command: executable, but absent from the palette.
    pub const fn hidden(category: CommandCategory) -> Self {
        Self {
            category,
            palette: PaletteVisibility::Hidden,
            ex_names: &[],
            args: ArgSpec::None,
            range: RangeSpec::None,
            bang: false,
        }
    }

    pub const fn with_args(mut self, args: ArgSpec) -> Self {
        self.args = args;
        self
    }

    pub const fn with_ex_names(mut self, ex_names: &'static [&'static str]) -> Self {
        self.ex_names = ex_names;
        self
    }

    pub const fn with_range(mut self, range: RangeSpec) -> Self {
        self.range = range;
        self
    }

    pub const fn with_bang(mut self) -> Self {
        self.bang = true;
        self
    }
}

const PATH_ARGS: &[Arg] = &[Arg::rest("path", ArgKind::Path)];
const DOCUMENT_ARGS: &[Arg] = &[Arg::optional("document", ArgKind::Document)];
const CODE_ARGS: &[Arg] = &[Arg::rest("code", ArgKind::Text)];
const VIEW_ID_ARGS: &[Arg] = &[Arg::required("view", ArgKind::Text)];
const VIEW_OPEN_ARGS: &[Arg] = &[
    Arg::required("view", ArgKind::Text),
    Arg::choice("direction", &["left", "right", "up", "down"], false),
];

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

impl DocumentCommand {
    pub fn meta(self) -> CommandMeta {
        match self {
            // Hidden until the palette can prompt for the path (Phase 2).
            Self::Open => CommandMeta::hidden(CommandCategory::File)
                .with_ex_names(&["e", "edit"])
                .with_args(ArgSpec::Positionals(PATH_ARGS)),
            Self::Save => CommandMeta::listed(CommandCategory::File, &["w", "write"]),
            Self::Switch => CommandMeta::listed(CommandCategory::File, &["b", "buffer"])
                .with_args(ArgSpec::Positionals(DOCUMENT_ARGS)),
        }
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

impl UiCommand {
    pub fn meta(self) -> CommandMeta {
        match self {
            Self::CommandPalette | Self::ExCommandLine => CommandMeta::hidden(CommandCategory::App),
            Self::ScriptEval => CommandMeta::hidden(CommandCategory::App)
                .with_ex_names(&["ts"])
                .with_args(ArgSpec::Positionals(CODE_ARGS)),
            Self::ReconnectServer => CommandMeta::listed(CommandCategory::App, &["reconnect"]),
            Self::ViewOpen => CommandMeta::hidden(CommandCategory::View)
                .with_ex_names(&["view"])
                .with_args(ArgSpec::Positionals(VIEW_OPEN_ARGS)),
            Self::ViewClose => CommandMeta::hidden(CommandCategory::View)
                .with_ex_names(&["view-close"])
                .with_args(ArgSpec::Positionals(VIEW_ID_ARGS)),
            Self::ViewToggle => CommandMeta::hidden(CommandCategory::View)
                .with_ex_names(&["view-toggle"])
                .with_args(ArgSpec::Positionals(VIEW_ID_ARGS)),
        }
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
        ScrollLeft => ("vy.view.scroll_left", "Scroll Left"),
        ScrollRight => ("vy.view.scroll_right", "Scroll Right"),
        ToggleWrap => ("vy.view.toggle_wrap", "Toggle Wrap"),
    }
}

impl ViewCommand {
    pub fn meta(self) -> CommandMeta {
        match self {
            // Transitional: `:set wrap` replaces this in Phase 5.
            Self::ToggleWrap => CommandMeta::listed(CommandCategory::View, &["wrap"]),
            Self::ScrollHalfPageDown
            | Self::ScrollHalfPageUp
            | Self::CenterCursor
            | Self::ScrollCursorTop
            | Self::ScrollCursorBottom
            | Self::ScrollLeft
            | Self::ScrollRight => CommandMeta::hidden(CommandCategory::View),
        }
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

    pub fn meta(self) -> CommandMeta {
        match self {
            Self::SplitLeft => CommandMeta::listed(CommandCategory::Pane, &["split-left"]),
            Self::SplitRight => CommandMeta::listed(CommandCategory::Pane, &["vsp"]),
            Self::SplitUp => CommandMeta::listed(CommandCategory::Pane, &["split-up"]),
            Self::SplitDown => CommandMeta::listed(CommandCategory::Pane, &["sp"]),
            Self::Close => CommandMeta::listed(CommandCategory::Pane, &["close", "q"]).with_bang(),
            Self::FocusLeft | Self::FocusDown | Self::FocusUp | Self::FocusRight | Self::Next => {
                CommandMeta::hidden(CommandCategory::Pane)
            }
        }
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

    pub fn meta(self) -> CommandMeta {
        match self {
            Self::Undo => CommandMeta::listed(CommandCategory::Edit, &["undo"]),
            Self::Redo => CommandMeta::listed(CommandCategory::Edit, &["redo"]),
            Self::MoveLeft
            | Self::MoveRight
            | Self::MoveUp
            | Self::MoveDown
            | Self::WordForward
            | Self::WordBackward
            | Self::WordEnd
            | Self::LineStart
            | Self::LineEnd
            | Self::DocumentStart
            | Self::DocumentEnd
            | Self::Delete
            | Self::PasteAfter
            | Self::PasteBefore
            | Self::Repeat
            | Self::LastVisual
            | Self::InsertHere
            | Self::InsertAfter
            | Self::InsertLineEnd
            | Self::InsertLineStart
            | Self::OpenBelow
            | Self::OpenAbove
            | Self::SelectMode
            | Self::SelectLineMode
            | Self::SelectBlockMode
            | Self::NormalMode
            | Self::InsertNewline
            | Self::InsertTab
            | Self::InsertBackspace
            | Self::InsertLeft
            | Self::InsertRight
            | Self::InsertUp
            | Self::InsertDown
            | Self::InsertHome
            | Self::InsertEnd
            | Self::OperatorDelete
            | Self::OperatorYank
            | Self::YankLine
            | Self::OperatorInner
            | Self::OperatorAround
            | Self::TextObjectWord
            | Self::TextObjectParagraph
            | Self::TextObjectParen
            | Self::TextObjectBracket
            | Self::TextObjectBrace
            | Self::TextObjectAngle
            | Self::TextObjectDoubleQuote
            | Self::TextObjectSingleQuote
            | Self::TextObjectBacktick => CommandMeta::hidden(CommandCategory::Edit),
        }
    }
}
