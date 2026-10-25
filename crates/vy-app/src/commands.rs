//! Command identifiers and the registry.
//!
//! Built-in commands live in Rust. Commands registered from `init.ts` are tracked
//! here so a reload can drop everything the script added.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

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

/// Count and register prefixes captured by the keymap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeymapArgs {
    /// Count typed before the key sequence (`5` in `5j`).
    pub count: Option<u32>,
    /// Register chosen with `"`, as a lowercase letter.
    pub register: Option<char>,
}

/// Arguments passed to a command, independent of the entry point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandArgs {
    /// Values produced by the command's [`ArgSpec`], or the script's own JSON.
    pub values: JsonValue,
    /// Count typed before a key sequence.
    pub count: Option<u32>,
    /// Register chosen with `"`.
    pub register: Option<char>,
}

impl CommandArgs {
    pub fn keymap(args: KeymapArgs) -> Self {
        Self {
            values: JsonValue::Null,
            count: args.count,
            register: args.register,
        }
    }

    pub fn values(values: JsonValue) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }

    pub fn value(&self, name: &str) -> Option<&JsonValue> {
        self.values.get(name)
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        self.value(name).and_then(JsonValue::as_str)
    }

    pub fn u64(&self, name: &str) -> Option<u64> {
        self.value(name).and_then(JsonValue::as_u64)
    }

    pub fn keymap_args(&self) -> KeymapArgs {
        KeymapArgs {
            count: self.count,
            register: self.register,
        }
    }
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

/// One command call, before it reaches the registry.
#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    pub id: String,
    pub args: CommandArgs,
    pub range: Option<LineRange>,
    pub bang: bool,
    pub source: InvocationSource,
}

impl Invocation {
    pub fn keymap(id: impl Into<String>, args: KeymapArgs) -> Self {
        Self {
            id: id.into(),
            args: CommandArgs::keymap(args),
            range: None,
            bang: false,
            source: InvocationSource::Keymap,
        }
    }

    pub fn keymap_with_args(id: impl Into<String>, args: KeymapArgs, values: JsonValue) -> Self {
        Self {
            id: id.into(),
            args: CommandArgs {
                values,
                count: args.count,
                register: args.register,
            },
            range: None,
            bang: false,
            source: InvocationSource::Keymap,
        }
    }

    pub fn palette(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            args: CommandArgs::default(),
            range: None,
            bang: false,
            source: InvocationSource::Palette,
        }
    }

    pub fn palette_with_args(id: impl Into<String>, values: JsonValue) -> Self {
        Self {
            id: id.into(),
            args: CommandArgs::values(values),
            range: None,
            bang: false,
            source: InvocationSource::Palette,
        }
    }

    pub fn ex(id: impl Into<String>, values: JsonValue) -> Self {
        Self {
            id: id.into(),
            args: CommandArgs::values(values),
            range: None,
            bang: false,
            source: InvocationSource::Ex,
        }
    }

    pub fn script(id: impl Into<String>, values: JsonValue) -> Self {
        Self {
            id: id.into(),
            args: CommandArgs::values(values),
            range: None,
            bang: false,
            source: InvocationSource::Script,
        }
    }

    pub fn keymap_args(&self) -> KeymapArgs {
        self.args.keymap_args()
    }

    pub fn with_range(mut self, range: Option<LineRange>) -> Self {
        self.range = range;
        self
    }

    pub fn with_bang(mut self, bang: bool) -> Self {
        self.bang = bang;
        self
    }
}

/// A 1-based inclusive line range resolved on the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

/// One unresolved address in an Ex range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExAddress {
    Line(u32),
    Current,
    Last,
    LastVisualStart,
    LastVisualEnd,
    Relative(i64),
}

/// An unresolved Ex range, before the client knows the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExRange {
    pub start: ExAddress,
    pub end: ExAddress,
}

/// One parsed Ex command line, without the leading `:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExCommandLine {
    pub range: Option<ExRange>,
    pub name: String,
    pub bang: bool,
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExParseError {
    #[error("empty Ex command")]
    Empty,
    #[error("invalid Ex range: {0}")]
    InvalidRange(String),
}

/// Parses `[range] name[!] [args]`.
pub fn parse_ex_line(input: &str) -> Result<ExCommandLine, ExParseError> {
    let input = input.trim_start();
    if input.is_empty() {
        return Err(ExParseError::Empty);
    }

    let (range, rest) = parse_ex_range(input)?;
    let input = rest.trim_start();
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
        range,
        name,
        bang,
        args: rest.trim_start().to_string(),
    })
}

fn parse_ex_range(input: &str) -> Result<(Option<ExRange>, &str), ExParseError> {
    let mut end = 0;
    for (index, character) in input.char_indices() {
        if character.is_whitespace() || character == '!' || !is_address_character(character) {
            break;
        }
        end = index + character.len_utf8();
    }
    if end == 0 {
        return Ok((None, input));
    }

    let token = &input[..end];
    let rest = &input[end..];
    if token == "%" {
        return Ok((
            Some(ExRange {
                start: ExAddress::Line(1),
                end: ExAddress::Last,
            }),
            rest,
        ));
    }

    let mut parts = token.split(',');
    let start = parse_ex_address(parts.next().unwrap_or(""))?;
    let end = match parts.next() {
        Some(part) => parse_ex_address(part)?,
        None => start,
    };
    if parts.next().is_some() {
        return Err(ExParseError::InvalidRange(token.to_string()));
    }
    Ok((Some(ExRange { start, end }), rest))
}

fn is_address_character(character: char) -> bool {
    character.is_ascii_digit()
        || matches!(
            character,
            '.' | '$' | '\'' | '<' | '>' | '+' | '-' | ',' | '%'
        )
}

fn parse_ex_address(text: &str) -> Result<ExAddress, ExParseError> {
    match text {
        "." => Ok(ExAddress::Current),
        "$" => Ok(ExAddress::Last),
        "'<" => Ok(ExAddress::LastVisualStart),
        "'>" => Ok(ExAddress::LastVisualEnd),
        "" => Err(ExParseError::InvalidRange(String::new())),
        _ if text.starts_with('+') => Ok(ExAddress::Relative(parse_relative(text, 1)?)),
        _ if text.starts_with('-') => Ok(ExAddress::Relative(parse_relative(text, -1)?)),
        _ => text
            .parse::<u32>()
            .map(ExAddress::Line)
            .map_err(|_| ExParseError::InvalidRange(text.to_string())),
    }
}

fn parse_relative(text: &str, sign: i64) -> Result<i64, ExParseError> {
    let digits = &text[1..];
    if digits.is_empty() {
        return Ok(sign);
    }
    digits
        .parse::<i64>()
        .map(|number| sign * number)
        .map_err(|_| ExParseError::InvalidRange(text.to_string()))
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

/// Why a command's arguments could not be parsed or validated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgumentError {
    #[error("this command does not take arguments")]
    NoArguments,
    #[error("missing argument: {0}")]
    Missing(&'static str),
    #[error("too many arguments: {0}")]
    Unexpected(String),
    #[error("invalid value for {name}: {value}")]
    Invalid { name: &'static str, value: String },
    #[error("unterminated quote")]
    UnterminatedQuote,
}

/// Parses the raw text after an Ex command name into an argument object.
pub fn parse_ex_args(spec: ArgSpec, raw: &str) -> Result<JsonValue, ArgumentError> {
    let raw = raw.trim();
    if spec.is_none() {
        if raw.is_empty() {
            return Ok(JsonValue::Object(serde_json::Map::new()));
        }
        return Err(ArgumentError::NoArguments);
    }

    let args = spec.args();
    if args.len() == 1 && args[0].rest {
        if raw.is_empty() {
            return Err(ArgumentError::Missing(args[0].name));
        }
        let mut values = serde_json::Map::new();
        values.insert(
            args[0].name.to_string(),
            parse_arg_value(&args[0], raw.to_string())?,
        );
        return Ok(JsonValue::Object(values));
    }

    let mut tokens = split_argument_tokens(raw)?.into_iter();
    let mut values = serde_json::Map::new();
    for arg in args {
        let Some(token) = tokens.next() else {
            if arg.required {
                return Err(ArgumentError::Missing(arg.name));
            }
            continue;
        };
        values.insert(arg.name.to_string(), parse_arg_value(arg, token)?);
    }
    if let Some(extra) = tokens.next() {
        return Err(ArgumentError::Unexpected(extra));
    }
    Ok(JsonValue::Object(values))
}

/// Validates a JSON argument object against a command's argument declaration.
pub fn validate_json_args(spec: ArgSpec, value: &JsonValue) -> Result<JsonValue, ArgumentError> {
    if spec.is_none() {
        return match value {
            JsonValue::Null => Ok(JsonValue::Object(serde_json::Map::new())),
            JsonValue::Object(object)
                if object.keys().all(|key| key == "count" || key == "register") =>
            {
                Ok(JsonValue::Object(serde_json::Map::new()))
            }
            _ => Err(ArgumentError::NoArguments),
        };
    }

    let Some(object) = value.as_object() else {
        if value.is_null() {
            if let Some(required) = spec.args().iter().find(|arg| arg.required) {
                return Err(ArgumentError::Missing(required.name));
            }
            return Ok(JsonValue::Object(serde_json::Map::new()));
        }
        return Err(ArgumentError::Invalid {
            name: spec.args().first().map(|arg| arg.name).unwrap_or("args"),
            value: value.to_string(),
        });
    };

    let args = spec.args();
    let mut values = serde_json::Map::new();
    for arg in args {
        let Some(value) = object.get(arg.name) else {
            if arg.required {
                return Err(ArgumentError::Missing(arg.name));
            }
            continue;
        };
        let text = match value {
            JsonValue::String(text) => text.clone(),
            JsonValue::Number(number) => number.to_string(),
            other => {
                return Err(ArgumentError::Invalid {
                    name: arg.name,
                    value: other.to_string(),
                });
            }
        };
        values.insert(arg.name.to_string(), parse_arg_value(arg, text)?);
    }
    if let Some(key) = object.keys().find(|key| {
        *key != "count" && *key != "register" && !args.iter().any(|arg| arg.name == *key)
    }) {
        return Err(ArgumentError::Unexpected(key.clone()));
    }
    Ok(JsonValue::Object(values))
}

fn parse_arg_value(arg: &Arg, value: String) -> Result<JsonValue, ArgumentError> {
    match arg.kind {
        ArgKind::Text | ArgKind::Path | ArgKind::Document => Ok(JsonValue::String(value)),
        ArgKind::Number => {
            value
                .parse::<i64>()
                .map(JsonValue::from)
                .map_err(|_| ArgumentError::Invalid {
                    name: arg.name,
                    value,
                })
        }
        ArgKind::Choice => {
            if arg.choices.contains(&value.as_str()) {
                Ok(JsonValue::String(value))
            } else {
                Err(ArgumentError::Invalid {
                    name: arg.name,
                    value,
                })
            }
        }
    }
}

fn split_argument_tokens(raw: &str) -> Result<Vec<String>, ArgumentError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut has_token = false;

    for character in raw.chars() {
        match quote {
            Some(quote_character) if character == quote_character => {
                quote = None;
                has_token = true;
            }
            Some(_) => {
                current.push(character);
                has_token = true;
            }
            None if character.is_whitespace() => {
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            None if character == '"' || character == '\'' => {
                quote = Some(character);
                has_token = true;
            }
            None => {
                current.push(character);
                has_token = true;
            }
        }
    }

    if quote.is_some() {
        return Err(ArgumentError::UnterminatedQuote);
    }
    if has_token {
        tokens.push(current);
    }
    Ok(tokens)
}

/// A human-readable argument list such as `<uri> [direction]`.
pub fn usage(spec: ArgSpec) -> String {
    spec.args()
        .iter()
        .map(|arg| {
            if arg.required {
                format!("<{}>", arg.name)
            } else {
                format!("[{}]", arg.name)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Formats an argument error with the command's usage.
pub fn argument_error(name: &str, spec: ArgSpec, error: &ArgumentError) -> String {
    let usage = usage(spec);
    if usage.is_empty() {
        format!("{error}")
    } else {
        format!("{error} (usage: :{name} {usage})")
    }
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

/// Ex names that extend `typed`, sorted and deduplicated for completion.
///
/// The exact name is not returned: completing it would not change the input.
pub fn ex_name_candidates(typed: &str, commands: &[CommandDescriptor]) -> Vec<String> {
    let mut candidates: Vec<String> = commands
        .iter()
        .flat_map(|command| command.ex_names.iter())
        .filter(|ex_name| ex_name.starts_with(typed) && ex_name.as_str() != typed)
        .cloned()
        .collect();
    candidates.sort();
    candidates.dedup();
    candidates
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
        let args = KeymapArgs {
            count: Some(3),
            register: Some('a'),
        };
        let invocation = Invocation::keymap("vy.motion.down", args);
        assert_eq!(invocation.source, InvocationSource::Keymap);
        assert_eq!(invocation.keymap_args(), args);

        let values = serde_json::json!({});
        let invocation = Invocation::ex("vy.pane.close", values.clone()).with_bang(true);
        assert_eq!(invocation.source, InvocationSource::Ex);
        assert!(invocation.bang);
        assert_eq!(invocation.args.values, values);
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
                range: None,
                name: "sp".to_string(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("  close!"),
            Ok(ExCommandLine {
                range: None,
                name: "close".to_string(),
                bang: true,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("e  src/main.rs"),
            Ok(ExCommandLine {
                range: None,
                name: "e".to_string(),
                bang: false,
                args: "src/main.rs".to_string(),
            })
        );
        assert_eq!(parse_ex_line("   "), Err(ExParseError::Empty));
    }

    #[test]
    fn parses_ex_ranges() {
        let range = |start, end| Some(ExRange { start, end });

        assert_eq!(
            parse_ex_line("1,5d"),
            Ok(ExCommandLine {
                range: range(ExAddress::Line(1), ExAddress::Line(5)),
                name: "d".into(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("%w"),
            Ok(ExCommandLine {
                range: range(ExAddress::Line(1), ExAddress::Last),
                name: "w".into(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line(".d"),
            Ok(ExCommandLine {
                range: range(ExAddress::Current, ExAddress::Current),
                name: "d".into(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("$d"),
            Ok(ExCommandLine {
                range: range(ExAddress::Last, ExAddress::Last),
                name: "d".into(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("'<,'>d"),
            Ok(ExCommandLine {
                range: range(ExAddress::LastVisualStart, ExAddress::LastVisualEnd),
                name: "d".into(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("+2d"),
            Ok(ExCommandLine {
                range: range(ExAddress::Relative(2), ExAddress::Relative(2)),
                name: "d".into(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("-1d"),
            Ok(ExCommandLine {
                range: range(ExAddress::Relative(-1), ExAddress::Relative(-1)),
                name: "d".into(),
                bang: false,
                args: String::new(),
            })
        );
        assert_eq!(
            parse_ex_line("1,2,3d"),
            Err(ExParseError::InvalidRange("1,2,3".into()))
        );
        assert_eq!(
            parse_ex_line("1.5d"),
            Err(ExParseError::InvalidRange("1.5".into()))
        );
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
    fn completes_ex_names_by_prefix() {
        let commands = CommandRegistry::new().descriptors();

        assert_eq!(
            ex_name_candidates("wr", &commands),
            vec!["wrap".to_string(), "write".to_string()]
        );
        assert_eq!(
            ex_name_candidates("split-", &commands),
            vec!["split-left".to_string(), "split-up".to_string()]
        );
        assert_eq!(
            ex_name_candidates("v", &commands),
            vec![
                "view".to_string(),
                "view-close".to_string(),
                "view-toggle".to_string(),
                "vsp".to_string(),
            ]
        );
        // An exact name has nothing left to complete.
        assert!(ex_name_candidates("write", &commands).is_empty());
        assert!(ex_name_candidates("zz", &commands).is_empty());
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
        assert_eq!(find("vy.editor.open").palette, PaletteVisibility::Listed);
        assert_eq!(find("vy.view.open").palette, PaletteVisibility::Listed);
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
    fn parses_ex_arguments_into_a_typed_object() {
        const VIEW: ArgSpec = ArgSpec::Positionals(&[
            Arg::required("view", ArgKind::Text),
            Arg::choice("direction", &["left", "right"], false),
        ]);
        const OPEN: ArgSpec = ArgSpec::Positionals(&[Arg::rest("uri", ArgKind::Path)]);
        const TWO: ArgSpec = ArgSpec::Positionals(&[
            Arg::required("first", ArgKind::Text),
            Arg::required("second", ArgKind::Text),
        ]);

        assert_eq!(
            parse_ex_args(VIEW, "tree left"),
            Ok(serde_json::json!({ "view": "tree", "direction": "left" }))
        );
        assert_eq!(
            parse_ex_args(VIEW, "tree"),
            Ok(serde_json::json!({ "view": "tree" }))
        );
        assert_eq!(parse_ex_args(VIEW, ""), Err(ArgumentError::Missing("view")));
        assert_eq!(
            parse_ex_args(VIEW, "tree sideways"),
            Err(ArgumentError::Invalid {
                name: "direction",
                value: "sideways".into(),
            })
        );
        assert_eq!(
            parse_ex_args(VIEW, "tree left extra"),
            Err(ArgumentError::Unexpected("extra".into()))
        );
        assert_eq!(
            parse_ex_args(OPEN, "src/a file.rs"),
            Ok(serde_json::json!({ "uri": "src/a file.rs" }))
        );
        assert_eq!(
            parse_ex_args(TWO, r#""a b" c"#),
            Ok(serde_json::json!({ "first": "a b", "second": "c" }))
        );
        assert_eq!(
            parse_ex_args(TWO, "\"unterminated"),
            Err(ArgumentError::UnterminatedQuote)
        );
        assert_eq!(parse_ex_args(ArgSpec::None, ""), Ok(serde_json::json!({})));
        assert_eq!(
            parse_ex_args(ArgSpec::None, "extra"),
            Err(ArgumentError::NoArguments)
        );
    }

    #[test]
    fn validates_json_arguments() {
        const OPEN: ArgSpec = ArgSpec::Positionals(&[Arg::rest("uri", ArgKind::Path)]);
        const OPTIONAL: ArgSpec =
            ArgSpec::Positionals(&[Arg::optional("document", ArgKind::Document)]);

        assert_eq!(
            validate_json_args(OPEN, &serde_json::json!({ "uri": "src/main.rs" })),
            Ok(serde_json::json!({ "uri": "src/main.rs" }))
        );
        assert_eq!(
            validate_json_args(OPEN, &serde_json::json!({ "path": "src/main.rs" })),
            Err(ArgumentError::Missing("uri"))
        );
        assert_eq!(
            validate_json_args(
                OPEN,
                &serde_json::json!({ "uri": "src/main.rs", "extra": 1 })
            ),
            Err(ArgumentError::Unexpected("extra".into()))
        );
        assert_eq!(
            validate_json_args(OPEN, &serde_json::json!({})),
            Err(ArgumentError::Missing("uri"))
        );
        assert_eq!(
            validate_json_args(OPEN, &serde_json::Value::Null),
            Err(ArgumentError::Missing("uri"))
        );
        assert_eq!(
            validate_json_args(OPTIONAL, &serde_json::Value::Null),
            Ok(serde_json::json!({}))
        );
        assert_eq!(
            validate_json_args(ArgSpec::None, &serde_json::json!({ "count": 2 })),
            Ok(serde_json::json!({}))
        );
        assert_eq!(
            validate_json_args(ArgSpec::None, &serde_json::json!({ "extra": 1 })),
            Err(ArgumentError::NoArguments)
        );
        assert_eq!(usage(OPEN), "<uri>");
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
