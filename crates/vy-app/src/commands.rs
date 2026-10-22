//! Command identifiers and the registry.
//!
//! Built-in commands live in Rust. Commands registered from `init.ts` are tracked
//! here so a reload can drop everything the script added.

use std::collections::HashMap;

/// Who registered a command. Registrations from `init.ts` are dropped on reload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Owner {
    Builtin,
    Init,
    Plugin(String),
}

/// Arguments passed from the keymap to a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommandArgs {
    /// Count typed before the key sequence (`5` in `5j`).
    pub count: Option<u32>,
    /// Register chosen with `"`, as a lowercase letter.
    pub register: Option<char>,
}

/// A command handled by the client UI instead of the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    CommandPalette,
}

impl UiCommand {
    pub fn id(self) -> &'static str {
        match self {
            Self::CommandPalette => "vy.ui.command_palette",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::CommandPalette => "Command Palette",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|command| command.id() == id)
    }

    pub fn all() -> &'static [Self] {
        &[Self::CommandPalette]
    }
}

/// Turns a command id into a short label for the command palette.
pub fn display_title(id: &str) -> String {
    let tail = id.strip_prefix("vy.").unwrap_or(id);
    let mut title = String::new();

    for word in tail.split(['.', '_']).filter(|word| !word.is_empty()) {
        if !title.is_empty() {
            title.push(' ');
        }
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            title.extend(first.to_uppercase());
            title.push_str(characters.as_str());
        }
    }

    title
}

/// A command that changes the window layout instead of the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneCommand {
    SplitRight,
    SplitDown,
    Close,
    FocusLeft,
    FocusDown,
    FocusUp,
    FocusRight,
    Next,
}

impl PaneCommand {
    /// Stable identifier used by key bindings and `commands.execute`.
    pub fn id(self) -> &'static str {
        match self {
            Self::SplitRight => "vy.pane.split_right",
            Self::SplitDown => "vy.pane.split_down",
            Self::Close => "vy.pane.close",
            Self::FocusLeft => "vy.pane.focus_left",
            Self::FocusDown => "vy.pane.focus_down",
            Self::FocusUp => "vy.pane.focus_up",
            Self::FocusRight => "vy.pane.focus_right",
            Self::Next => "vy.pane.next",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|command| command.id() == id)
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::SplitRight,
            Self::SplitDown,
            Self::Close,
            Self::FocusLeft,
            Self::FocusDown,
            Self::FocusUp,
            Self::FocusRight,
            Self::Next,
        ]
    }
}

/// A command implemented in Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinCommand {
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    WordForward,
    WordBackward,
    WordEnd,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
    Delete,
    PasteAfter,
    PasteBefore,
    Repeat,
    LastVisual,
    InsertHere,
    InsertAfter,
    SelectMode,
    NormalMode,
    Undo,
    Redo,
    InsertNewline,
    InsertTab,
    InsertBackspace,
    InsertLeft,
    InsertRight,
    InsertUp,
    InsertDown,
    InsertHome,
    InsertEnd,
    OperatorDelete,
    OperatorYank,
    YankLine,
    OperatorInner,
    OperatorAround,
    TextObjectWord,
    TextObjectParagraph,
    TextObjectParen,
    TextObjectBracket,
    TextObjectBrace,
    TextObjectAngle,
    TextObjectDoubleQuote,
    TextObjectSingleQuote,
    TextObjectBacktick,
}

impl BuiltinCommand {
    /// Stable identifier used by key bindings and `commands.execute`.
    pub fn id(self) -> &'static str {
        match self {
            Self::MoveLeft => "vy.motion.left",
            Self::MoveRight => "vy.motion.right",
            Self::MoveUp => "vy.motion.up",
            Self::MoveDown => "vy.motion.down",
            Self::WordForward => "vy.motion.word_forward",
            Self::WordBackward => "vy.motion.word_backward",
            Self::WordEnd => "vy.motion.word_end",
            Self::LineStart => "vy.motion.line_start",
            Self::LineEnd => "vy.motion.line_end",
            Self::DocumentStart => "vy.motion.document_start",
            Self::DocumentEnd => "vy.motion.document_end",
            Self::Delete => "vy.edit.delete",
            Self::PasteAfter => "vy.edit.paste_after",
            Self::PasteBefore => "vy.edit.paste_before",
            Self::Repeat => "vy.edit.repeat",
            Self::LastVisual => "vy.edit.last_visual",
            Self::InsertHere => "vy.mode.insert",
            Self::InsertAfter => "vy.mode.insert_after",
            Self::SelectMode => "vy.mode.select",
            Self::NormalMode => "vy.mode.normal",
            Self::Undo => "vy.history.undo",
            Self::Redo => "vy.history.redo",
            Self::InsertNewline => "vy.insert.newline",
            Self::InsertTab => "vy.insert.tab",
            Self::InsertBackspace => "vy.insert.backspace",
            Self::InsertLeft => "vy.insert.left",
            Self::InsertRight => "vy.insert.right",
            Self::InsertUp => "vy.insert.up",
            Self::InsertDown => "vy.insert.down",
            Self::InsertHome => "vy.insert.home",
            Self::InsertEnd => "vy.insert.end",
            Self::OperatorDelete => "vy.operator.delete",
            Self::OperatorYank => "vy.operator.yank",
            Self::YankLine => "vy.edit.yank_line",
            Self::OperatorInner => "vy.operator.inner",
            Self::OperatorAround => "vy.operator.around",
            Self::TextObjectWord => "vy.text_object.word",
            Self::TextObjectParagraph => "vy.text_object.paragraph",
            Self::TextObjectParen => "vy.text_object.paren",
            Self::TextObjectBracket => "vy.text_object.bracket",
            Self::TextObjectBrace => "vy.text_object.brace",
            Self::TextObjectAngle => "vy.text_object.angle",
            Self::TextObjectDoubleQuote => "vy.text_object.double_quote",
            Self::TextObjectSingleQuote => "vy.text_object.single_quote",
            Self::TextObjectBacktick => "vy.text_object.backtick",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|command| command.id() == id)
    }

    /// Every built-in command. Used by `commands.getCommands`.
    pub fn all() -> &'static [BuiltinCommand] {
        const COMMANDS: [BuiltinCommand; 45] = [
            BuiltinCommand::MoveLeft,
            BuiltinCommand::MoveRight,
            BuiltinCommand::MoveUp,
            BuiltinCommand::MoveDown,
            BuiltinCommand::WordForward,
            BuiltinCommand::WordBackward,
            BuiltinCommand::WordEnd,
            BuiltinCommand::LineStart,
            BuiltinCommand::LineEnd,
            BuiltinCommand::DocumentStart,
            BuiltinCommand::DocumentEnd,
            BuiltinCommand::Delete,
            BuiltinCommand::PasteAfter,
            BuiltinCommand::PasteBefore,
            BuiltinCommand::Repeat,
            BuiltinCommand::LastVisual,
            BuiltinCommand::InsertHere,
            BuiltinCommand::InsertAfter,
            BuiltinCommand::SelectMode,
            BuiltinCommand::NormalMode,
            BuiltinCommand::Undo,
            BuiltinCommand::Redo,
            BuiltinCommand::InsertNewline,
            BuiltinCommand::InsertTab,
            BuiltinCommand::InsertBackspace,
            BuiltinCommand::InsertLeft,
            BuiltinCommand::InsertRight,
            BuiltinCommand::InsertUp,
            BuiltinCommand::InsertDown,
            BuiltinCommand::InsertHome,
            BuiltinCommand::InsertEnd,
            BuiltinCommand::OperatorDelete,
            BuiltinCommand::OperatorYank,
            BuiltinCommand::YankLine,
            BuiltinCommand::OperatorInner,
            BuiltinCommand::OperatorAround,
            BuiltinCommand::TextObjectWord,
            BuiltinCommand::TextObjectParagraph,
            BuiltinCommand::TextObjectParen,
            BuiltinCommand::TextObjectBracket,
            BuiltinCommand::TextObjectBrace,
            BuiltinCommand::TextObjectAngle,
            BuiltinCommand::TextObjectDoubleQuote,
            BuiltinCommand::TextObjectSingleQuote,
            BuiltinCommand::TextObjectBacktick,
        ];

        &COMMANDS
    }
}

/// A validated command identifier: `publisher.name` with lowercase segments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandId(String);

impl CommandId {
    pub fn parse(value: impl Into<String>) -> Result<Self, CommandError> {
        let value = value.into();
        let invalid = || CommandError::InvalidId(value.clone());

        if value.starts_with("vy.") {
            return Err(CommandError::Reserved(value));
        }

        let mut segments = value.split('.');
        let mut count = 0;
        for segment in &mut segments {
            count += 1;
            if segment.is_empty()
                || !segment.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
                })
            {
                return Err(invalid());
            }
        }
        if count < 2 {
            return Err(invalid());
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a command could not be registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    InvalidId(String),
    Reserved(String),
    Duplicate(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId(id) => write!(formatter, "invalid command id: {id}"),
            Self::Reserved(id) => write!(
                formatter,
                "command ids starting with `vy.` are reserved: {id}"
            ),
            Self::Duplicate(id) => write!(formatter, "command {id} is already registered"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Script commands registered at runtime.
#[derive(Debug, Default)]
pub struct CommandRegistry {
    scripts: HashMap<String, Owner>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a command from a script. Built-in ids and duplicates are rejected.
    pub fn register(&mut self, id: impl Into<String>, owner: Owner) -> Result<(), CommandError> {
        let id = CommandId::parse(id)?;
        if BuiltinCommand::parse(id.as_str()).is_some() || self.scripts.contains_key(id.as_str()) {
            return Err(CommandError::Duplicate(id.as_str().to_string()));
        }
        self.scripts.insert(id.as_str().to_string(), owner);
        Ok(())
    }

    /// Drops every command registered by an owner. Used when `init.ts` reloads.
    pub fn unregister_owner(&mut self, owner: &Owner) {
        self.scripts.retain(|_, registered| registered != owner);
    }

    pub fn owner_of(&self, id: &str) -> Option<&Owner> {
        self.scripts.get(id)
    }

    pub fn is_registered(&self, id: &str) -> bool {
        BuiltinCommand::parse(id).is_some() || self.scripts.contains_key(id)
    }

    /// Registered script command ids, sorted for stable output.
    pub fn script_commands(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.scripts.keys().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_builtin_id() {
        for command in [
            BuiltinCommand::MoveLeft,
            BuiltinCommand::Delete,
            BuiltinCommand::NormalMode,
            BuiltinCommand::InsertEnd,
        ] {
            assert_eq!(BuiltinCommand::parse(command.id()), Some(command));
        }
        assert_eq!(BuiltinCommand::parse("vy.motion.unknown"), None);
    }

    #[test]
    fn builds_palette_titles_from_ids() {
        assert_eq!(display_title("vy.pane.split_right"), "Pane Split Right");
        assert_eq!(display_title("vy.motion.left"), "Motion Left");
        assert_eq!(
            display_title("user.uppercase-document"),
            "User Uppercase-document"
        );
    }

    #[test]
    fn parses_ui_commands() {
        assert_eq!(
            UiCommand::parse("vy.ui.command_palette"),
            Some(UiCommand::CommandPalette)
        );
        assert_eq!(UiCommand::parse("vy.ui.unknown"), None);
        assert_eq!(UiCommand::CommandPalette.title(), "Command Palette");
    }

    #[test]
    fn validates_command_ids() {
        assert!(CommandId::parse("user.uppercase").is_ok());
        assert!(CommandId::parse("example.hello.greet").is_ok());
        assert_eq!(
            CommandId::parse("vy.find_files"),
            Err(CommandError::Reserved("vy.find_files".into()))
        );
        assert!(matches!(
            CommandId::parse("Uppercase"),
            Err(CommandError::InvalidId(_))
        ));
        assert!(matches!(
            CommandId::parse("user..name"),
            Err(CommandError::InvalidId(_))
        ));
        assert!(matches!(
            CommandId::parse("user.Upper"),
            Err(CommandError::InvalidId(_))
        ));
    }

    #[test]
    fn rejects_duplicates_and_reserved_ids() {
        let mut registry = CommandRegistry::new();
        registry
            .register("user.first", Owner::Init)
            .expect("register");
        assert_eq!(
            registry.register("user.first", Owner::Init),
            Err(CommandError::Duplicate("user.first".into()))
        );
        assert!(matches!(
            registry.register("vy.history.undo", Owner::Init),
            Err(CommandError::Reserved(_))
        ));
        assert!(registry.is_registered("vy.history.undo"));
        assert!(registry.is_registered("user.first"));
    }

    #[test]
    fn drops_commands_by_owner() {
        let mut registry = CommandRegistry::new();
        registry
            .register("user.first", Owner::Init)
            .expect("register");
        registry
            .register(
                "example.plugin.command",
                Owner::Plugin("example.plugin".into()),
            )
            .expect("register");

        registry.unregister_owner(&Owner::Init);
        assert_eq!(registry.owner_of("user.first"), None);
        assert_eq!(registry.script_commands().len(), 1);
    }
}
