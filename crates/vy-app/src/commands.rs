//! Command identifiers and the registry.
//!
//! Built-in commands live in Rust. Commands registered from `init.ts` are tracked
//! here so a reload can drop everything the script added.

use std::collections::HashMap;

use strum::VariantArray;

use serde_json::Value as JsonValue;
use vy_core::Range;

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

/// Where a command came from. The palette shows this next to the title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSource {
    Builtin,
    Pane,
    Ui,
    Init,
    Plugin(String),
}

impl CommandSource {
    pub fn label(&self) -> &str {
        match self {
            Self::Builtin => "built-in",
            Self::Pane => "pane",
            Self::Ui => "ui",
            Self::Init => "script",
            Self::Plugin(_) => "plugin",
        }
    }
}

impl From<&Owner> for CommandSource {
    fn from(owner: &Owner) -> Self {
        match owner {
            Owner::Builtin => Self::Builtin,
            Owner::Init => Self::Init,
            Owner::Plugin(id) => Self::Plugin(id.clone()),
        }
    }
}

/// Everything the palette and the Ex command line need to know about a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub id: String,
    pub title: String,
    pub source: CommandSource,
    pub aliases: Vec<String>,
    pub hidden: bool,
    pub accepts_args: bool,
    pub accepts_range: bool,
    pub accepts_bang: bool,
}

impl CommandDescriptor {
    fn new(id: impl Into<String>, title: impl Into<String>, source: CommandSource) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            source,
            aliases: Vec::new(),
            hidden: false,
            accepts_args: false,
            accepts_range: false,
            accepts_bang: false,
        }
    }
}

/// Where an invocation came from. The execution path is the same for all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvocationSource {
    #[default]
    Keymap,
    Palette,
    Ex,
    Script,
}

/// Arguments in the shape each entry point can provide.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum InvocationArgs {
    #[default]
    None,
    Keymap(CommandArgs),
    Script(JsonValue),
    Ex {
        raw: String,
    },
}

/// One command call, before it reaches the registry.
#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    pub id: String,
    pub args: InvocationArgs,
    pub range: Option<Range>,
    pub bang: bool,
    pub source: InvocationSource,
}

impl Invocation {
    pub fn keymap(id: impl Into<String>, args: CommandArgs) -> Self {
        Self {
            id: id.into(),
            args: InvocationArgs::Keymap(args),
            range: None,
            bang: false,
            source: InvocationSource::Keymap,
        }
    }

    pub fn palette(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            args: InvocationArgs::None,
            range: None,
            bang: false,
            source: InvocationSource::Palette,
        }
    }

    pub fn ex(id: impl Into<String>, raw: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            args: InvocationArgs::Ex { raw: raw.into() },
            range: None,
            bang: false,
            source: InvocationSource::Ex,
        }
    }

    pub fn script(id: impl Into<String>, args: JsonValue) -> Self {
        Self {
            id: id.into(),
            args: InvocationArgs::Script(args),
            range: None,
            bang: false,
            source: InvocationSource::Script,
        }
    }

    pub fn keymap_args(&self) -> CommandArgs {
        match self.args {
            InvocationArgs::Keymap(args) => args,
            _ => CommandArgs::default(),
        }
    }

    pub fn ex_raw(&self) -> Option<&str> {
        match &self.args {
            InvocationArgs::Ex { raw } => Some(raw),
            _ => None,
        }
    }
}

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

/// One parsed Ex command line, without the leading `:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExCommandLine {
    pub name: String,
    pub bang: bool,
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExParseError {
    #[error("empty Ex command")]
    Empty,
}

/// Parses `name[!] [args]`. Ranges are reserved for a later step.
pub fn parse_ex_line(input: &str) -> Result<ExCommandLine, ExParseError> {
    let input = input.trim_start();
    if input.is_empty() {
        return Err(ExParseError::Empty);
    }

    let name_end = input
        .char_indices()
        .find(|(_, character)| character.is_whitespace() || *character == '!')
        .map(|(index, _)| index)
        .unwrap_or(input.len());
    let name = input[..name_end].to_string();
    if name.is_empty() {
        return Err(ExParseError::Empty);
    }

    let rest = &input[name_end..];
    let (bang, rest) = match rest.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, rest),
    };

    Ok(ExCommandLine {
        name,
        bang,
        args: rest.trim_start().to_string(),
    })
}

/// An Ex name and the command it calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExAlias {
    pub name: &'static str,
    pub id: &'static str,
}

pub const EX_ALIASES: &[ExAlias] = &[
    ExAlias {
        name: "sp",
        id: "vy.pane.split_down",
    },
    ExAlias {
        name: "vsp",
        id: "vy.pane.split_right",
    },
    ExAlias {
        name: "close",
        id: "vy.pane.close",
    },
    ExAlias {
        name: "q",
        id: "vy.pane.close",
    },
    ExAlias {
        name: "e",
        id: "vy.editor.open",
    },
    ExAlias {
        name: "w",
        id: "vy.document.save",
    },
    ExAlias {
        name: "b",
        id: "vy.editor.switch_document",
    },
    ExAlias {
        name: "buffer",
        id: "vy.editor.switch_document",
    },
    ExAlias {
        name: "ts",
        id: "vy.script.eval",
    },
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExResolveError {
    #[error("unknown Ex command: {0}")]
    Unknown(String),
    #[error("ambiguous Ex command `{name}`: {}", candidates.join(", "))]
    Ambiguous {
        name: String,
        candidates: Vec<String>,
    },
}

/// Resolves an exact name or a unique prefix to a command id.
pub fn resolve_ex_name(name: &str) -> Result<&'static str, ExResolveError> {
    if let Some(alias) = EX_ALIASES.iter().find(|alias| alias.name == name) {
        return Ok(alias.id);
    }

    let candidates: Vec<&ExAlias> = EX_ALIASES
        .iter()
        .filter(|alias| alias.name.starts_with(name))
        .collect();

    match candidates.as_slice() {
        [] => Err(ExResolveError::Unknown(name.to_string())),
        [alias] => Ok(alias.id),
        aliases => Err(ExResolveError::Ambiguous {
            name: name.to_string(),
            candidates: aliases.iter().map(|alias| alias.name.to_string()).collect(),
        }),
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr, strum::EnumString, strum::VariantArray,
)]
pub enum PaneCommand {
    #[strum(serialize = "vy.pane.split_right")]
    SplitRight,
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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("invalid command id: {0}")]
    InvalidId(String),
    #[error("command ids starting with `vy.` are reserved: {0}")]
    Reserved(String),
    #[error("command {0} is already registered")]
    Duplicate(String),
}

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

    /// Builds the command list used by the palette and the Ex resolver.
    pub fn descriptors(&self) -> Vec<CommandDescriptor> {
        let mut descriptors = Vec::new();

        for command in BuiltinCommand::all() {
            descriptors.push(CommandDescriptor::new(
                command.id(),
                display_title(command.id()),
                CommandSource::Builtin,
            ));
        }
        for command in PaneCommand::all() {
            descriptors.push(CommandDescriptor::new(
                command.id(),
                display_title(command.id()),
                CommandSource::Pane,
            ));
        }
        for command in DocumentCommand::all() {
            let mut descriptor =
                CommandDescriptor::new(command.id(), command.title(), CommandSource::Builtin);
            descriptor.hidden = *command == DocumentCommand::Open;
            descriptor.accepts_args =
                matches!(command, DocumentCommand::Open | DocumentCommand::Switch);
            descriptors.push(descriptor);
        }
        for command in UiCommand::all() {
            let mut descriptor =
                CommandDescriptor::new(command.id(), command.title(), CommandSource::Ui);
            descriptor.hidden = matches!(
                command,
                UiCommand::CommandPalette | UiCommand::ExCommandLine | UiCommand::ScriptEval
            );
            descriptor.accepts_args = *command == UiCommand::ScriptEval;
            descriptors.push(descriptor);
        }
        for command in ViewCommand::all() {
            descriptors.push(CommandDescriptor::new(
                command.id(),
                display_title(command.id()),
                CommandSource::Builtin,
            ));
        }
        for id in self.script_commands() {
            let source = self
                .owner_of(&id)
                .map(CommandSource::from)
                .unwrap_or(CommandSource::Init);
            descriptors.push(CommandDescriptor::new(
                id.clone(),
                display_title(&id),
                source,
            ));
        }

        descriptors
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
    fn builds_a_catalog_with_every_command_source() {
        let mut registry = CommandRegistry::new();
        registry
            .register("user.hello", Owner::Init)
            .expect("register");

        let descriptors = registry.descriptors();
        assert!(
            descriptors
                .iter()
                .any(|command| command.id == "vy.motion.left"
                    && command.source == CommandSource::Builtin)
        );
        assert!(
            descriptors
                .iter()
                .any(|command| command.id == "vy.pane.split_right"
                    && command.source == CommandSource::Pane)
        );
        assert!(descriptors.iter().any(|command| {
            command.id == "vy.ui.command_palette"
                && command.source == CommandSource::Ui
                && command.hidden
        }));
        assert!(descriptors.iter().any(|command| {
            command.id == "user.hello" && command.source == CommandSource::Init
        }));
    }

    #[test]
    fn invocations_keep_their_source_and_arguments() {
        let args = CommandArgs {
            count: Some(3),
            register: Some('a'),
        };
        let invocation = Invocation::keymap("vy.motion.down", args);
        assert_eq!(invocation.source, InvocationSource::Keymap);
        assert_eq!(invocation.keymap_args(), args);

        let invocation = Invocation::ex("vy.pane.close", "");
        assert_eq!(invocation.source, InvocationSource::Ex);
        assert_eq!(invocation.args, InvocationArgs::Ex { raw: String::new() });
    }

    #[test]
    fn parses_ui_commands() {
        assert_eq!(
            UiCommand::parse("vy.ui.command_palette"),
            Some(UiCommand::CommandPalette)
        );
        assert_eq!(
            UiCommand::parse("vy.ui.ex_command_line"),
            Some(UiCommand::ExCommandLine)
        );
        assert_eq!(UiCommand::parse("vy.ui.unknown"), None);
        assert_eq!(UiCommand::CommandPalette.title(), "Command Palette");
        assert_eq!(UiCommand::ExCommandLine.title(), "Ex Command Line");
    }

    #[test]
    fn parses_view_commands() {
        for command in ViewCommand::all() {
            assert_eq!(ViewCommand::parse(command.id()), Some(*command));
        }
        assert_eq!(ViewCommand::parse("vy.view.unknown"), None);
    }

    #[test]
    fn parses_ex_names_arguments_and_bang() {
        assert_eq!(
            parse_ex_line("sp"),
            Ok(ExCommandLine {
                name: "sp".to_string(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("  close!"),
            Ok(ExCommandLine {
                name: "close".to_string(),
                bang: true,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("e  src/main.rs"),
            Ok(ExCommandLine {
                name: "e".to_string(),
                bang: false,
                args: "src/main.rs".to_string(),
            })
        );
        assert_eq!(parse_ex_line("   "), Err(ExParseError::Empty));
    }

    #[test]
    fn resolves_exact_names_and_unique_prefixes() {
        assert_eq!(resolve_ex_name("sp"), Ok("vy.pane.split_down"));
        assert_eq!(resolve_ex_name("s"), Ok("vy.pane.split_down"));
        assert_eq!(resolve_ex_name("vsp"), Ok("vy.pane.split_right"));
        assert_eq!(resolve_ex_name("close"), Ok("vy.pane.close"));
        assert_eq!(resolve_ex_name("q"), Ok("vy.pane.close"));
        assert_eq!(resolve_ex_name("e"), Ok("vy.editor.open"));
        assert_eq!(resolve_ex_name("w"), Ok("vy.document.save"));
        assert_eq!(resolve_ex_name("b"), Ok("vy.editor.switch_document"));
        assert_eq!(resolve_ex_name("buffer"), Ok("vy.editor.switch_document"));
        assert_eq!(resolve_ex_name("ts"), Ok("vy.script.eval"));
        assert_eq!(
            resolve_ex_name("zz"),
            Err(ExResolveError::Unknown("zz".to_string()))
        );
        assert_eq!(
            resolve_ex_name(""),
            Err(ExResolveError::Ambiguous {
                name: String::new(),
                candidates: vec![
                    "sp".to_string(),
                    "vsp".to_string(),
                    "close".to_string(),
                    "q".to_string(),
                    "e".to_string(),
                    "w".to_string(),
                    "b".to_string(),
                    "buffer".to_string(),
                    "ts".to_string(),
                ],
            })
        );
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

    fn export_dir() -> std::path::PathBuf {
        std::env::var("TS_RS_EXPORT_DIR").map_or_else(
            |_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../packages/vy-api-types")
            },
            std::path::PathBuf::from,
        )
    }

    /// Writes `CommandId.ts` for `packages/vy-api-types`.
    #[test]
    fn export_bindings_command_ids() {
        let mut ids: Vec<&str> = Vec::new();
        ids.extend(BuiltinCommand::all().iter().map(|command| command.id()));
        ids.extend(PaneCommand::all().iter().map(|command| command.id()));
        ids.extend(DocumentCommand::all().iter().map(|command| command.id()));
        ids.extend(UiCommand::all().iter().map(|command| command.id()));
        ids.extend(ViewCommand::all().iter().map(|command| command.id()));
        ids.sort_unstable();
        ids.dedup();

        let mut output = String::new();
        output.push_str("// This file was generated by vy-app. Do not edit it manually.\n");
        output.push_str("// Regenerate with `cargo test --workspace export_bindings`.\n\n");
        output.push_str("/** Identifiers of the commands built into the client. */\n");
        output.push_str("export type CommandId =\n");
        output.push_str(
            &ids.iter()
                .map(|id| format!("  | \"{id}\""))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        output.push_str(";\n");

        let directory = export_dir();
        std::fs::create_dir_all(&directory).expect("create the export directory");
        std::fs::write(directory.join("CommandId.ts"), output).expect("write CommandId.ts");
    }
}
