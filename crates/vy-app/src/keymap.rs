//! Key sequences, bindings, and matching.
//!
//! The keymap owns the count (`5j`) and register (`"a`) prefixes. It walks the
//! bindings in order, so later bindings (from `init.ts`) override earlier ones
//! (the default keymap). See `docs/keymap.md`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::commands::{CommandArgs, Owner};
use crate::when::{self, Expr, Value};

/// Editor mode. It is part of the keymap context and of the client's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Normal,
    Insert,
    Select,
    SelectLine,
    SelectBlock,
}

impl Mode {
    /// Whether this mode is any visual/select variant.
    pub fn is_select(self) -> bool {
        matches!(self, Self::Select | Self::SelectLine | Self::SelectBlock)
    }

    /// Lowercase name used by `when` expressions.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Insert => "insert",
            Self::Select => "select",
            Self::SelectLine => "select_line",
            Self::SelectBlock => "select_block",
        }
    }

    /// Uppercase label shown in the status line.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Select => "VISUAL",
            Self::SelectLine => "VISUAL LINE",
            Self::SelectBlock => "VISUAL BLOCK",
        }
    }

    /// Parses the lowercase name used by `when` and by `editor.setMode`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "insert" => Some(Self::Insert),
            "select" | "visual" => Some(Self::Select),
            "select_line" | "select-line" | "visual_line" | "visual-line" => Some(Self::SelectLine),
            "select_block" | "select-block" | "visual_block" | "visual-block" => {
                Some(Self::SelectBlock)
            }
            _ => None,
        }
    }
}

/// One key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Esc,
    Enter,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

impl Key {
    /// Parses one key name. Returns [`None`] for names the keymap does not know.
    pub fn parse(name: &str) -> Option<Self> {
        let ctrl_rest = name
            .strip_prefix("ctrl-")
            .or_else(|| name.strip_prefix("c-"));
        if let Some(rest) = ctrl_rest
            && rest.chars().count() == 1
        {
            return rest
                .chars()
                .next()
                .filter(|character| character.is_ascii_alphabetic())
                .map(|character| Self::Ctrl(character.to_ascii_lowercase()));
        }

        let key = match name {
            "space" => Self::Char(' '),
            "esc" | "escape" => Self::Esc,
            "enter" | "return" => Self::Enter,
            "tab" => Self::Tab,
            "backspace" => Self::Backspace,
            "delete" => Self::Delete,
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            "home" => Self::Home,
            "end" => Self::End,
            _ => {
                let mut characters = name.chars();
                match (characters.next(), characters.next()) {
                    (Some(character), None) => Self::Char(character),
                    _ => return None,
                }
            }
        };
        Some(key)
    }
}

/// Parses a space-separated sequence such as `g g` or `space f f`.
pub fn parse_sequence(keys: &str) -> Result<Vec<Key>, KeymapError> {
    let mut sequence = Vec::new();
    for name in keys.split_whitespace() {
        let key = Key::parse(name).ok_or_else(|| KeymapError::InvalidKey(name.to_string()))?;
        sequence.push(key);
    }
    if sequence.is_empty() {
        return Err(KeymapError::EmptySequence);
    }
    Ok(sequence)
}

/// State used to evaluate `when` expressions at dispatch time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyContext {
    pub mode: Mode,
    pub has_selection: bool,
    pub editor_focus: bool,
    pub operator_pending: bool,
    pub text_object_pending: bool,
    pub remote: bool,
}

impl Default for KeyContext {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            has_selection: false,
            editor_focus: true,
            operator_pending: false,
            text_object_pending: false,
            remote: false,
        }
    }
}

impl KeyContext {
    fn value(&self, name: &str) -> Option<Value> {
        match name {
            "editorFocus" => Some(Value::Bool(self.editor_focus)),
            "mode" => Some(Value::Text(self.mode.name().to_string())),
            "normalMode" => Some(Value::Bool(self.mode == Mode::Normal)),
            "insertMode" => Some(Value::Bool(self.mode == Mode::Insert)),
            "selectMode" => Some(Value::Bool(self.mode.is_select())),
            "selectLineMode" => Some(Value::Bool(self.mode == Mode::SelectLine)),
            "selectBlockMode" => Some(Value::Bool(self.mode == Mode::SelectBlock)),
            "hasSelection" => Some(Value::Bool(self.has_selection)),
            "operatorPending" => Some(Value::Bool(self.operator_pending)),
            "textObjectPending" => Some(Value::Bool(self.text_object_pending)),
            "remote" => Some(Value::Bool(self.remote)),
            _ => None,
        }
    }
}

/// A registered key sequence.
#[derive(Debug, Clone)]
pub struct Binding {
    pub keys: Vec<Key>,
    pub command: String,
    pub when: Option<Expr>,
    pub owner: Owner,
}

/// Result of feeding one key to the keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapOutcome {
    /// More keys are needed to complete a binding.
    Pending,
    /// No binding matched; the key is not consumed.
    Ignored,
    /// A binding completed.
    Command { id: String, args: CommandArgs },
}

/// Key bindings plus the partially typed sequence.
#[derive(Debug)]
pub struct Keymap {
    bindings: Vec<Binding>,
    contexts: HashMap<String, Value>,
    sequence_timeout: Duration,
    pending: Vec<Key>,
    count: Option<u32>,
    awaiting_register: bool,
    register: Option<char>,
    last_key: Option<Instant>,
}

impl Keymap {
    pub fn new(sequence_timeout: Duration) -> Self {
        Self {
            bindings: Vec::new(),
            contexts: HashMap::new(),
            sequence_timeout,
            pending: Vec::new(),
            count: None,
            awaiting_register: false,
            register: None,
            last_key: None,
        }
    }

    /// Builds the keymap from `builtin/keymaps/default.json`.
    pub fn load_default(sequence_timeout: Duration) -> Result<Self, KeymapError> {
        let mut keymap = Self::new(sequence_timeout);
        let specs: Vec<BindingSpec> =
            serde_json::from_str(include_str!("../../../builtin/keymaps/default.json"))
                .map_err(|error| KeymapError::DefaultKeymap(error.to_string()))?;

        for spec in specs {
            keymap.bind(
                &spec.keys,
                spec.command,
                spec.when.as_deref(),
                Owner::Builtin,
            )?;
        }
        Ok(keymap)
    }

    /// Adds a binding. Later bindings win when several sequences match.
    pub fn bind(
        &mut self,
        keys: &str,
        command: impl Into<String>,
        when: Option<&str>,
        owner: Owner,
    ) -> Result<(), KeymapError> {
        let keys = parse_sequence(keys)?;
        let condition = match when {
            Some(source) => Some(when::parse(source).map_err(KeymapError::When)?),
            None => None,
        };
        self.bindings.push(Binding {
            keys,
            command: command.into(),
            when: condition,
            owner,
        });
        Ok(())
    }

    /// Removes every binding with this sequence. Returns how many were removed.
    pub fn unbind(&mut self, keys: &str) -> Result<usize, KeymapError> {
        let keys = parse_sequence(keys)?;
        let before = self.bindings.len();
        self.bindings.retain(|binding| binding.keys != keys);
        Ok(before - self.bindings.len())
    }

    /// Drops every binding added by an owner. Used when `init.ts` reloads.
    pub fn unbind_owner(&mut self, owner: &Owner) -> usize {
        let before = self.bindings.len();
        self.bindings.retain(|binding| &binding.owner != owner);
        before - self.bindings.len()
    }

    /// Adds a context value for `when` expressions from scripts.
    pub fn set_context(&mut self, key: impl Into<String>, value: Value) {
        self.contexts.insert(key.into(), value);
    }

    pub fn sequence_timeout(&self) -> Duration {
        self.sequence_timeout
    }

    pub fn set_sequence_timeout(&mut self, timeout: Duration) {
        self.sequence_timeout = timeout;
    }

    /// Returns whether a partially typed sequence is waiting for more keys.
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty() || self.count.is_some() || self.awaiting_register
    }

    /// Clears buffers that waited too long. Returns whether anything was cleared.
    pub fn expire(&mut self, now: Instant) -> bool {
        if !self.is_pending() {
            return false;
        }
        if self
            .last_key
            .is_none_or(|at| now.duration_since(at) > self.sequence_timeout)
        {
            self.reset();
            return true;
        }
        false
    }

    /// Feeds one key and either completes a command, waits, or ignores it.
    pub fn feed(&mut self, key: Key, context: &KeyContext) -> KeymapOutcome {
        self.expire(Instant::now());

        if key == Key::Esc {
            self.reset();
        }

        if self.awaiting_register {
            self.awaiting_register = false;
            self.last_key = Some(Instant::now());
            if let Key::Char(character) = key {
                self.register = character.is_ascii_lowercase().then_some(character);
                return KeymapOutcome::Pending;
            }
            self.reset();
            return KeymapOutcome::Ignored;
        }

        // `"` selects a register unless a binding uses it.
        if key == Key::Char('"') && !self.has_prefix(&[key], context) {
            self.awaiting_register = true;
            self.pending.clear();
            self.last_key = Some(Instant::now());
            return KeymapOutcome::Pending;
        }

        // Digits build a count. `0` is a motion until a count has started.
        if let Key::Char(character) = key
            && character.is_ascii_digit()
            && (self.count.is_some() || (character != '0' && !self.has_prefix(&[key], context)))
        {
            let digit = character.to_digit(10).unwrap_or(0);
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit),
            );
            self.last_key = Some(Instant::now());
            return KeymapOutcome::Pending;
        }

        self.pending.push(key);

        let mut exact = None;
        let mut prefix = false;
        for (index, binding) in self.bindings.iter().enumerate() {
            if !self.matches(binding, context) {
                continue;
            }
            if binding.keys == self.pending {
                exact = Some(index);
            } else if binding.keys.len() > self.pending.len()
                && binding.keys.starts_with(&self.pending)
            {
                prefix = true;
            }
        }

        if let Some(index) = exact {
            let binding = &self.bindings[index];
            let args = CommandArgs {
                count: self.count.take(),
                register: self.register.take(),
            };
            let id = binding.command.clone();
            self.pending.clear();
            self.last_key = None;
            return KeymapOutcome::Command { id, args };
        }

        if prefix {
            self.last_key = Some(Instant::now());
            return KeymapOutcome::Pending;
        }

        self.reset();
        KeymapOutcome::Ignored
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.count = None;
        self.awaiting_register = false;
        self.register = None;
        self.last_key = None;
    }

    fn matches(&self, binding: &Binding, context: &KeyContext) -> bool {
        match &binding.when {
            None => true,
            Some(expr) => expr.eval(&|name| {
                context
                    .value(name)
                    .or_else(|| self.contexts.get(name).cloned())
            }),
        }
    }

    fn has_prefix(&self, keys: &[Key], context: &KeyContext) -> bool {
        let mut sequence = self.pending.clone();
        sequence.extend_from_slice(keys);
        self.bindings.iter().any(|binding| {
            self.matches(binding, context)
                && binding.keys.len() >= sequence.len()
                && binding.keys.starts_with(&sequence)
        })
    }
}

#[derive(Debug, Deserialize)]
struct BindingSpec {
    keys: String,
    command: String,
    #[serde(default)]
    when: Option<String>,
}

/// Why a binding could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapError {
    #[error("unknown key name: {0}")]
    InvalidKey(String),
    #[error("a key binding needs at least one key")]
    EmptySequence,
    #[error("invalid `when` expression: {0}")]
    When(#[from] when::ParseError),
    #[error("could not load the default keymap: {0}")]
    DefaultKeymap(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keymap() -> Keymap {
        Keymap::load_default(Duration::from_millis(500)).expect("default keymap")
    }

    fn normal() -> KeyContext {
        KeyContext::default()
    }

    fn command(outcome: KeymapOutcome) -> (String, CommandArgs) {
        match outcome {
            KeymapOutcome::Command { id, args } => (id, args),
            other => panic!("expected a command: {other:?}"),
        }
    }

    #[test]
    fn parses_key_names() {
        assert_eq!(Key::parse("a"), Some(Key::Char('a')));
        assert_eq!(Key::parse("G"), Some(Key::Char('G')));
        assert_eq!(Key::parse("space"), Some(Key::Char(' ')));
        assert_eq!(Key::parse("ctrl-r"), Some(Key::Ctrl('r')));
        assert_eq!(Key::parse("ctrl-R"), Some(Key::Ctrl('r')));
        assert_eq!(Key::parse("escape"), Some(Key::Esc));
        assert_eq!(Key::parse("nope"), None);
        assert_eq!(
            parse_sequence("g g"),
            Ok(vec![Key::Char('g'), Key::Char('g')])
        );
        assert_eq!(parse_sequence("  "), Err(KeymapError::EmptySequence));
    }

    #[test]
    fn matches_every_migrated_key() {
        let mut keymap = keymap();
        let cases = [
            (Key::Char('h'), "vy.motion.left", Mode::Normal),
            (Key::Char('j'), "vy.motion.down", Mode::Normal),
            (Key::Char('w'), "vy.motion.word_forward", Mode::Normal),
            (Key::Char('0'), "vy.motion.line_start", Mode::Normal),
            (Key::Char('$'), "vy.motion.line_end", Mode::Normal),
            (Key::Char('G'), "vy.motion.document_end", Mode::Normal),
            (Key::Char('x'), "vy.edit.delete", Mode::Normal),
            (Key::Char('p'), "vy.edit.paste_after", Mode::Normal),
            (Key::Char('i'), "vy.mode.insert", Mode::Normal),
            (Key::Char('a'), "vy.mode.insert_after", Mode::Normal),
            (Key::Char('v'), "vy.mode.select", Mode::Normal),
            (Key::Char('u'), "vy.history.undo", Mode::Normal),
            (Key::Ctrl('r'), "vy.history.redo", Mode::Normal),
            (Key::Char('x'), "vy.edit.delete", Mode::Select),
            (Key::Esc, "vy.mode.normal", Mode::Insert),
            (Key::Enter, "vy.insert.newline", Mode::Insert),
            (Key::Backspace, "vy.insert.backspace", Mode::Insert),
            (Key::Home, "vy.insert.home", Mode::Insert),
        ];

        for (key, expected, mode) in cases {
            let context = KeyContext { mode, ..normal() };
            let (id, args) = command(keymap.feed(key, &context));
            assert_eq!(id, expected, "key {key:?} in {mode:?}");
            assert_eq!(args, CommandArgs::default());
        }
    }

    #[test]
    fn waits_for_a_sequence() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('g'), &context),
            KeymapOutcome::Pending
        );
        let (id, _) = command(keymap.feed(Key::Char('g'), &context));
        assert_eq!(id, "vy.motion.document_start");
    }

    #[test]
    fn passes_counts_and_registers() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('5'), &context),
            KeymapOutcome::Pending
        );
        let (id, args) = command(keymap.feed(Key::Char('x'), &context));
        assert_eq!(id, "vy.edit.delete");
        assert_eq!(args.count, Some(5));

        assert_eq!(
            keymap.feed(Key::Char('"'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('a'), &context),
            KeymapOutcome::Pending
        );
        let (_, args) = command(keymap.feed(Key::Char('p'), &context));
        assert_eq!(args.register, Some('a'));
    }

    #[test]
    fn ignores_unknown_keys_and_clears_the_sequence() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('g'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('q'), &context),
            KeymapOutcome::Ignored
        );
        assert!(!keymap.is_pending());
        assert_eq!(
            keymap.feed(Key::Char('g'), &context),
            KeymapOutcome::Pending
        );
    }

    #[test]
    fn expires_a_pending_sequence() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('g'), &context),
            KeymapOutcome::Pending
        );
        assert!(keymap.expire(Instant::now() + Duration::from_secs(1)));
        assert!(!keymap.is_pending());
    }

    #[test]
    fn evaluates_when_conditions() {
        let mut keymap = keymap();
        let insert = KeyContext {
            mode: Mode::Insert,
            ..normal()
        };

        // `x` is bound to normal and select, not insert.
        assert_eq!(keymap.feed(Key::Char('x'), &insert), KeymapOutcome::Ignored);
    }

    #[test]
    fn user_bindings_override_defaults_and_unbind() {
        let mut keymap = keymap();
        keymap
            .bind("h", "user.left", None, Owner::Init)
            .expect("bind");
        let (id, _) = command(keymap.feed(Key::Char('h'), &normal()));
        assert_eq!(id, "user.left");

        assert_eq!(keymap.unbind("h").expect("unbind"), 2);
        // `unbind` removes the default too, so the key is no longer bound.
        assert_eq!(
            keymap.feed(Key::Char('h'), &normal()),
            KeymapOutcome::Ignored
        );
    }

    #[test]
    fn custom_contexts_are_visible_to_when() {
        let mut keymap = keymap();
        keymap.set_context("myFlag", Value::Bool(true));
        keymap
            .bind(
                "z",
                "user.only_with_flag",
                Some("myFlag && mode == 'normal'"),
                Owner::Init,
            )
            .expect("bind");

        let (id, _) = command(keymap.feed(Key::Char('z'), &normal()));
        assert_eq!(id, "user.only_with_flag");
    }
}
