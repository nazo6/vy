//! Command identifiers and the registry.
//!
//! Built-in commands live in Rust. Commands registered from `init.ts` are tracked
//! here so a reload can drop everything the script added.

use std::collections::HashMap;

use serde_json::Value as JsonValue;
use vy_core::Range;

pub use vy_client_api::command::{
    Arg, ArgKind, ArgSpec, BuiltinCommand, CommandCategory, CommandMeta, DocumentCommand,
    PaletteVisibility, PaneCommand, RangeSpec, UiCommand, ViewCommand,
};

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
    pub category: CommandCategory,
    pub palette: PaletteVisibility,
    pub ex_names: Vec<String>,
    pub args: ArgSpec,
    pub range: RangeSpec,
    pub bang: bool,
}

impl CommandDescriptor {
    fn from_meta(
        id: impl Into<String>,
        title: impl Into<String>,
        source: CommandSource,
        meta: CommandMeta,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            source,
            category: meta.category,
            palette: meta.palette,
            ex_names: meta
                .ex_names
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
            args: meta.args,
            range: meta.range,
            bang: meta.bang,
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

    pub fn with_bang(mut self, bang: bool) -> Self {
        self.bang = bang;
        self
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

/// Resolves an exact Ex name or a unique prefix to a command descriptor.
pub fn resolve_ex_name<'a>(
    name: &str,
    commands: &'a [CommandDescriptor],
) -> Result<&'a CommandDescriptor, ExResolveError> {
    if let Some(command) = commands
        .iter()
        .find(|command| command.ex_names.iter().any(|ex_name| ex_name == name))
    {
        return Ok(command);
    }

    let mut candidates: Vec<(&str, &CommandDescriptor)> = commands
        .iter()
        .flat_map(|command| {
            command
                .ex_names
                .iter()
                .map(move |ex_name| (ex_name.as_str(), command))
        })
        .filter(|(ex_name, _)| ex_name.starts_with(name))
        .collect();
    candidates.sort_by_key(|(ex_name, _)| *ex_name);

    match candidates.as_slice() {
        [] => Err(ExResolveError::Unknown(name.to_string())),
        [(_, command)] => Ok(command),
        _ => Err(ExResolveError::Ambiguous {
            name: name.to_string(),
            candidates: candidates
                .iter()
                .map(|(ex_name, _)| (*ex_name).to_string())
                .collect(),
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
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'
                        || character == '_'
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
    titles: HashMap<String, String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a command from a script. Built-in ids and foreign duplicates are rejected.
    /// Re-registration by the same plugin owner (implementing a contributed command) is allowed.
    pub fn register(&mut self, id: impl Into<String>, owner: Owner) -> Result<(), CommandError> {
        let id = CommandId::parse(id)?;
        if BuiltinCommand::parse(id.as_str()).is_some() {
            return Err(CommandError::Reserved(id.as_str().to_string()));
        }
        if let Some(existing_owner) = self.scripts.get(id.as_str()) {
            match (existing_owner, &owner) {
                (Owner::Plugin(_), Owner::Init) => {
                    // Allowed: script runtime implementing a contributed command
                    return Ok(());
                }
                (Owner::Plugin(p1), Owner::Plugin(p2)) if p1 == p2 => {
                    // Allowed: plugin implementing its own contributed command
                }
                _ => return Err(CommandError::Duplicate(id.as_str().to_string())),
            }
        }
        self.scripts.insert(id.as_str().to_string(), owner);
        Ok(())
    }

    /// Registers a contributed command from a plugin manifest.
    pub fn register_contributed(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        plugin_id: &str,
    ) -> Result<(), CommandError> {
        let id = CommandId::parse(id)?;
        if BuiltinCommand::parse(id.as_str()).is_some() {
            return Err(CommandError::Reserved(id.as_str().to_string()));
        }
        let id_str = id.as_str().to_string();
        let title_str = title.into();
        self.scripts
            .insert(id_str.clone(), Owner::Plugin(plugin_id.to_string()));
        if !title_str.is_empty() {
            self.titles.insert(id_str, title_str);
        }
        Ok(())
    }

    /// Drops every command registered by an owner. Used when `init.ts` reloads.
    pub fn unregister_owner(&mut self, owner: &Owner) {
        let removed: Vec<String> = self
            .scripts
            .iter()
            .filter(|(_, reg)| *reg == owner)
            .map(|(k, _)| k.clone())
            .collect();
        for id in removed {
            self.scripts.remove(&id);
            self.titles.remove(&id);
        }
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
            descriptors.push(CommandDescriptor::from_meta(
                command.id(),
                display_title(command.id()),
                CommandSource::Builtin,
                command.meta(),
            ));
        }
        for command in PaneCommand::all() {
            descriptors.push(CommandDescriptor::from_meta(
                command.id(),
                display_title(command.id()),
                CommandSource::Pane,
                command.meta(),
            ));
        }
        for command in DocumentCommand::all() {
            descriptors.push(CommandDescriptor::from_meta(
                command.id(),
                command.title(),
                CommandSource::Builtin,
                command.meta(),
            ));
        }
        for command in UiCommand::all() {
            descriptors.push(CommandDescriptor::from_meta(
                command.id(),
                command.title(),
                CommandSource::Ui,
                command.meta(),
            ));
        }
        for command in ViewCommand::all() {
            descriptors.push(CommandDescriptor::from_meta(
                command.id(),
                display_title(command.id()),
                CommandSource::Builtin,
                command.meta(),
            ));
        }
        for id in self.script_commands() {
            let source = self
                .owner_of(&id)
                .map(CommandSource::from)
                .unwrap_or(CommandSource::Init);
            let title = self
                .titles
                .get(&id)
                .cloned()
                .unwrap_or_else(|| display_title(&id));
            descriptors.push(CommandDescriptor::from_meta(
                id,
                title,
                source,
                CommandMeta::listed(CommandCategory::Plugin, &[]),
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
                && command.palette == PaletteVisibility::Hidden
        }));
        assert!(descriptors.iter().any(|command| {
            command.id == "vy.view.scroll_left"
                && command.source == CommandSource::Builtin
                && command.category == CommandCategory::View
        }));
        assert!(descriptors.iter().any(|command| {
            command.id == "user.hello"
                && command.source == CommandSource::Init
                && command.category == CommandCategory::Plugin
                && command.palette == PaletteVisibility::Listed
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

        let invocation = Invocation::ex("vy.pane.close", "").with_bang(true);
        assert_eq!(invocation.source, InvocationSource::Ex);
        assert!(invocation.bang);
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
        let commands = CommandRegistry::new().descriptors();
        let resolve =
            |name: &str| resolve_ex_name(name, &commands).map(|command| command.id.as_str());

        assert_eq!(resolve("sp"), Ok("vy.pane.split_down"));
        assert_eq!(resolve("vsp"), Ok("vy.pane.split_right"));
        assert_eq!(resolve("split-left"), Ok("vy.pane.split_left"));
        assert_eq!(resolve("split-up"), Ok("vy.pane.split_up"));
        assert_eq!(resolve("close"), Ok("vy.pane.close"));
        assert_eq!(resolve("q"), Ok("vy.pane.close"));
        assert_eq!(resolve("e"), Ok("vy.editor.open"));
        assert_eq!(resolve("edit"), Ok("vy.editor.open"));
        assert_eq!(resolve("w"), Ok("vy.document.save"));
        assert_eq!(resolve("write"), Ok("vy.document.save"));
        assert_eq!(resolve("b"), Ok("vy.editor.switch_document"));
        assert_eq!(resolve("buffer"), Ok("vy.editor.switch_document"));
        assert_eq!(resolve("ts"), Ok("vy.script.eval"));
        assert_eq!(resolve("reconnect"), Ok("vy.reconnect_server"));
        assert_eq!(resolve("view"), Ok("vy.view.open"));
        assert_eq!(resolve("view-close"), Ok("vy.view.close"));
        assert_eq!(resolve("view-toggle"), Ok("vy.view.toggle"));
        assert_eq!(resolve("undo"), Ok("vy.history.undo"));
        assert_eq!(resolve("redo"), Ok("vy.history.redo"));
        assert_eq!(
            resolve("zz"),
            Err(ExResolveError::Unknown("zz".to_string()))
        );
        // Transitional until `:set wrap` replaces it in Phase 5.
        assert_eq!(resolve("wrap"), Ok("vy.view.toggle_wrap"));

        match resolve("s") {
            Err(ExResolveError::Ambiguous { candidates, .. }) => {
                assert_eq!(candidates, vec!["sp", "split-left", "split-up"]);
            }
            other => panic!("expected an ambiguous Ex command, got {other:?}"),
        }
        match resolve("v") {
            Err(ExResolveError::Ambiguous { candidates, .. }) => {
                assert_eq!(candidates, vec!["view", "view-close", "view-toggle", "vsp"]);
            }
            other => panic!("expected an ambiguous Ex command, got {other:?}"),
        }
        assert!(matches!(resolve(""), Err(ExResolveError::Ambiguous { .. })));
    }

    #[test]
    fn declares_palette_metadata_for_every_builtin_command() {
        let commands = CommandRegistry::new().descriptors();
        let find = |id: &str| commands.iter().find(|command| command.id == id).unwrap();

        for command in &commands {
            assert!(!command.title.is_empty(), "{} has no title", command.id);
        }

        assert_eq!(find("vy.motion.left").palette, PaletteVisibility::Hidden);
        assert_eq!(find("vy.edit.delete").palette, PaletteVisibility::Hidden);
        assert_eq!(
            find("vy.view.scroll_left").palette,
            PaletteVisibility::Hidden
        );
        assert_eq!(
            find("vy.view.toggle_wrap").palette,
            PaletteVisibility::Listed
        );
        assert_eq!(
            find("vy.ui.command_palette").palette,
            PaletteVisibility::Hidden
        );
        assert_eq!(find("vy.editor.open").palette, PaletteVisibility::Hidden);
        assert_eq!(find("vy.view.open").palette, PaletteVisibility::Hidden);
        assert_eq!(find("vy.document.save").palette, PaletteVisibility::Listed);
        assert_eq!(
            find("vy.pane.split_right").palette,
            PaletteVisibility::Listed
        );
        assert_eq!(find("vy.history.undo").palette, PaletteVisibility::Listed);
    }

    #[test]
    fn ex_names_are_unique_and_valid() {
        let commands = CommandRegistry::new().descriptors();
        let mut seen = Vec::new();

        for command in &commands {
            for name in &command.ex_names {
                assert!(
                    name.chars().all(|character| character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'),
                    "invalid Ex name: {name}"
                );
                assert!(!seen.contains(name), "duplicate Ex name: {name}");
                seen.push(name.clone());
            }
        }

        assert!(seen.contains(&"sp".to_string()));
        assert!(seen.contains(&"undo".to_string()));
    }

    #[test]
    fn declares_arguments_on_commands_that_need_them() {
        let commands = CommandRegistry::new().descriptors();
        let find = |id: &str| commands.iter().find(|command| command.id == id).unwrap();

        let open = find("vy.editor.open");
        assert_eq!(open.args.args().len(), 1);
        assert_eq!(open.args.args()[0].kind, ArgKind::Path);
        assert!(open.args.args()[0].rest);

        assert!(find("vy.document.save").args.is_none());

        let view = find("vy.view.open");
        assert_eq!(view.args.args().len(), 2);
        assert_eq!(view.args.args()[1].kind, ArgKind::Choice);

        assert!(find("vy.pane.close").bang);
    }

    #[test]
    fn validates_command_ids() {
        assert!(CommandId::parse("user.uppercase").is_ok());
        assert!(CommandId::parse("user.uppercase_document").is_ok());
        assert!(CommandId::parse("example.hello.greet").is_ok());
        assert!(CommandId::parse("example.tree.toggle_dir").is_ok());
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
    fn allows_contributed_command_handler_registration() {
        let mut registry = CommandRegistry::new();
        registry
            .register_contributed(
                "example.tree.toggle",
                "File Explorer: Toggle",
                "example.tree",
            )
            .expect("contribute");
        assert!(registry.is_registered("example.tree.toggle"));

        // Script runtime registers implementation handler
        assert!(
            registry
                .register("example.tree.toggle", Owner::Init)
                .is_ok()
        );
        assert!(registry.is_registered("example.tree.toggle"));
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
