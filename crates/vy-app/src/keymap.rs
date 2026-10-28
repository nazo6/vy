//! Key sequences, bindings, and matching.
//!
//! The keymap owns the count (`5j`) and register (`"a`) prefixes. It walks the
//! bindings in order, so later bindings (from `init.ts`) override earlier ones
//! (the default keymap). See `docs/specs/keymap.md`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::commands::{KeymapArgs, Owner};
use crate::when::{self, Expr, Value};
use crate::widget::UiRole;

pub use vy_client_api::keymap::Mode;

/// What a pane shows, as seen by `when` conditions (ADR 0024 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PaneKind {
    #[default]
    Editor,
    Ui,
    Webview,
    Terminal,
    Overlay,
}

impl PaneKind {
    /// Name used by `when` expressions.
    pub fn name(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Ui => "ui",
            Self::Webview => "webview",
            Self::Terminal => "terminal",
            Self::Overlay => "overlay",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "editor" => Some(Self::Editor),
            "ui" => Some(Self::Ui),
            "webview" => Some(Self::Webview),
            "terminal" => Some(Self::Terminal),
            "overlay" => Some(Self::Overlay),
            _ => None,
        }
    }
}

/// The topmost floating surface, as seen by `when` conditions.
///
/// The names match the surface kinds in
/// [ADR 0032](../../../docs/adr/0032-floating-surfaces.md) §6, so one value set
/// serves both the overlay and the surface model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OverlayKind {
    #[default]
    None,
    Palette,
    Picker,
    Ex,
    Prompt,
    Approval,
    Settings,
    Hover,
    Menu,
    Custom,
}

impl OverlayKind {
    /// Name used by `when` expressions.
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Palette => "palette",
            Self::Picker => "picker",
            Self::Ex => "ex",
            Self::Prompt => "prompt",
            Self::Approval => "approval",
            Self::Settings => "settings",
            Self::Hover => "hover",
            Self::Menu => "menu",
            Self::Custom => "custom",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "palette" => Some(Self::Palette),
            "picker" => Some(Self::Picker),
            "ex" => Some(Self::Ex),
            "prompt" => Some(Self::Prompt),
            "approval" => Some(Self::Approval),
            "settings" => Some(Self::Settings),
            "hover" => Some(Self::Hover),
            "menu" => Some(Self::Menu),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// Whether the surface locks keyboard focus while it is open.
    pub fn is_modal(self) -> bool {
        !matches!(self, Self::None | Self::Hover | Self::Menu | Self::Custom)
    }

    /// Default widget role reported by this surface (ADR 0034 §1).
    pub fn default_ui_role(self) -> Option<UiRole> {
        match self {
            Self::Palette | Self::Picker | Self::Settings => Some(UiRole::List),
            Self::Ex | Self::Prompt => Some(UiRole::TextInput),
            Self::Approval => Some(UiRole::Button),
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
    BackTab,
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
            "backtab" | "s-tab" | "shift-tab" => Self::BackTab,
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

    /// The name [`Key::parse`] accepts for this key.
    ///
    /// The name is what a hint shows: `j`, `ctrl-d`, `esc`, `g g` is two names.
    pub fn name(self) -> String {
        match self {
            Self::Char(' ') => "space".to_string(),
            Self::Char(character) => character.to_string(),
            Self::Ctrl(character) => format!("ctrl-{character}"),
            Self::Esc => "esc".to_string(),
            Self::Enter => "enter".to_string(),
            Self::Backspace => "backspace".to_string(),
            Self::Delete => "delete".to_string(),
            Self::Tab => "tab".to_string(),
            Self::BackTab => "backtab".to_string(),
            Self::Left => "left".to_string(),
            Self::Right => "right".to_string(),
            Self::Up => "up".to_string(),
            Self::Down => "down".to_string(),
            Self::Home => "home".to_string(),
            Self::End => "end".to_string(),
        }
    }
}

/// Renders a key sequence the way a binding spells it (`g g`, `ctrl-d`).
pub fn format_sequence(keys: &[Key]) -> String {
    keys.iter()
        .map(|key| key.name())
        .collect::<Vec<_>>()
        .join(" ")
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
///
/// The fields below are the host-owned values from ADR 0024 §2 and ADR 0032 §6.
/// Scripts cannot overwrite them; they add their own names with
/// `keymap.setContext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyContext {
    pub mode: Mode,
    pub has_selection: bool,
    pub editor_focus: bool,
    pub operator_pending: bool,
    pub text_object_pending: bool,
    pub remote: bool,
    /// `editor`, `ui`, `webview`, or `terminal`.
    pub pane_kind: PaneKind,
    /// View id the focused pane shows. Empty for an editor pane.
    pub view_type: String,
    /// Role of the focused widget, when the pane has widget focus.
    pub ui_role: Option<UiRole>,
    /// `id` of the focused widget.
    pub ui_focus: Option<String>,
    /// Number of selected items in the focused widget.
    pub ui_selection_count: usize,
    /// Name of the topmost surface, or `none`.
    pub overlay: OverlayKind,
    /// `id` of the topmost surface, when it has one.
    pub surface_id: Option<String>,
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
            pane_kind: PaneKind::Editor,
            view_type: String::new(),
            ui_role: None,
            ui_focus: None,
            ui_selection_count: 0,
            overlay: OverlayKind::None,
            surface_id: None,
        }
    }
}

impl KeyContext {
    /// Role of the focused widget, falling back to the surface's default role when modal.
    pub fn effective_ui_role(&self) -> Option<UiRole> {
        self.ui_role.or_else(|| self.overlay.default_ui_role())
    }

    fn value(&self, name: &str) -> Option<Value> {
        match name {
            "editorFocus" => Some(Value::Bool(
                self.editor_focus
                    && self.overlay == OverlayKind::None
                    && self.pane_kind == PaneKind::Editor,
            )),
            "mode" => Some(Value::Text(self.mode.name().to_string())),
            "normalMode" => Some(Value::Bool(self.mode == Mode::Normal)),
            "insertMode" => Some(Value::Bool(self.mode == Mode::Insert)),
            "selectMode" => Some(Value::Bool(self.mode.is_select())),
            "selectLineMode" => Some(Value::Bool(self.mode == Mode::SelectLine)),
            "selectBlockMode" => Some(Value::Bool(self.mode == Mode::SelectBlock)),
            "modal" => Some(Value::Bool(self.overlay.is_modal())),
            "hasSelection" => Some(Value::Bool(
                self.effective_ui_role().is_some_and(UiRole::has_selection),
            )),
            "operatorPending" => Some(Value::Bool(self.operator_pending)),
            "textObjectPending" => Some(Value::Bool(self.text_object_pending)),
            "remote" => Some(Value::Bool(self.remote)),
            "paneKind" => Some(Value::Text(self.pane_kind.name().to_string())),
            "viewType" => Some(Value::Text(self.view_type.clone())),
            "uiRole" => self
                .effective_ui_role()
                .map(|role| Value::Text(role.name().to_string())),
            "uiFocus" => self.ui_focus.clone().map(Value::Text),
            "uiSelectionCount" => Some(Value::Number(self.ui_selection_count as f64)),
            "overlay" => Some(Value::Text(self.overlay.name().to_string())),
            "surfaceId" => self.surface_id.clone().map(Value::Text),
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
    pub values: JsonValue,
    pub owner: Owner,
}

/// Result of feeding one key to the keymap.
#[derive(Debug, Clone, PartialEq)]
pub enum KeymapOutcome {
    /// More keys are needed to complete a binding.
    Pending,
    /// No binding matched; the key is not consumed.
    Ignored,
    /// A binding completed.
    Command {
        id: String,
        args: KeymapArgs,
        values: JsonValue,
    },
}

/// Keys typed before a binding resolves.
///
/// A count and a register can be typed before a sequence starts, so the
/// sequence variant carries both. [`Prefix::None`] means no input is waiting.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Prefix {
    #[default]
    None,
    /// Digits typed as a count. `0` only starts a count after another digit.
    Count { value: u32 },
    /// `"` waits for a register name. `count` may already exist.
    Register { count: Option<u32> },
    /// Part of a key sequence, with any count and register typed before it.
    ///
    /// Empty `keys` means the count or register is complete and the next key
    /// starts a binding.
    Sequence {
        keys: Vec<Key>,
        count: Option<u32>,
        register: Option<char>,
    },
}

impl Prefix {
    fn count(&self) -> Option<u32> {
        match self {
            Self::Count { value } => Some(*value),
            Self::Register { count } => *count,
            Self::Sequence { count, .. } => *count,
            Self::None => None,
        }
    }

    /// The keys of the partial sequence. Empty for counts and registers.
    fn keys(&self) -> &[Key] {
        match self {
            Self::Sequence { keys, .. } => keys,
            _ => &[],
        }
    }

    fn append_count(&mut self, digit: u32) {
        let value = self
            .count()
            .unwrap_or(0)
            .saturating_mul(10)
            .saturating_add(digit);
        let next = match self {
            Self::Sequence { keys, register, .. } => Self::Sequence {
                keys: keys.clone(),
                count: Some(value),
                register: *register,
            },
            _ => Self::Count { value },
        };
        *self = next;
    }

    /// Starts a sequence with `key` appended to the keys typed so far.
    fn push_key(&mut self, key: Key) {
        let mut keys = self.keys().to_vec();
        keys.push(key);
        let next = Self::Sequence {
            keys,
            count: self.count(),
            register: match self {
                Self::Sequence { register, .. } => *register,
                _ => None,
            },
        };
        *self = next;
    }
}

/// Key bindings plus the partially typed sequence.
#[derive(Debug)]
pub struct Keymap {
    bindings: Vec<Binding>,
    contexts: HashMap<String, Value>,
    sequence_timeout: Duration,
    prefix: Prefix,
    last_key: Option<Instant>,
}

impl Keymap {
    pub fn new(sequence_timeout: Duration) -> Self {
        Self {
            bindings: Vec::new(),
            contexts: HashMap::new(),
            sequence_timeout,
            prefix: Prefix::None,
            last_key: None,
        }
    }

    /// Builds the keymap from `builtin/keymaps/default.json`.
    pub fn load_default(sequence_timeout: Duration) -> Result<Self, KeymapError> {
        let mut keymap = Self::new(sequence_timeout);
        let blocks: Vec<ContextBlockSpec> =
            serde_json::from_str(include_str!("../../../builtin/keymaps/default.json"))
                .map_err(|error| KeymapError::DefaultKeymap(error.to_string()))?;

        for block in blocks {
            for (keys, target) in block.bindings {
                let (command, args) = match target {
                    BindingTarget::Command(cmd) => (cmd, JsonValue::Null),
                    BindingTarget::Detailed { command, args } => {
                        (command, args.unwrap_or(JsonValue::Null))
                    }
                };
                keymap.bind_with_args(
                    &keys,
                    command,
                    block.when.as_deref(),
                    args,
                    Owner::Builtin,
                )?;
            }
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
        self.bind_with_args(keys, command, when, JsonValue::Null, owner)
    }

    /// Adds a binding with explicit command arguments.
    pub fn bind_with_args(
        &mut self,
        keys: &str,
        command: impl Into<String>,
        when: Option<&str>,
        values: JsonValue,
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
            values,
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

    /// Keys bound to `command` in `context`, shortest first.
    ///
    /// Surfaces use this to draw their hint line from the keymap instead of a
    /// hard-coded string, so a rebind changes the hint too
    /// ([ADR 0024](../../../docs/adr/0024-keyboard-first-plugin-ui.md) §7).
    fn keys_for_command(&self, command: &str, context: &KeyContext) -> Vec<Vec<Key>> {
        self.bindings
            .iter()
            .filter(|binding| binding.command == command && self.matches(binding, context))
            .map(|binding| binding.keys.clone())
            .collect()
    }

    /// The first key sequence bound to `commands`, in the order given.
    ///
    /// Returns the sequence that is easiest to show: a single key beats a
    /// sequence, and shorter beats longer.
    pub fn hint_for(&self, commands: &[&str], context: &KeyContext) -> Option<String> {
        let mut best: Option<Vec<Key>> = None;
        for command in commands {
            for keys in self.keys_for_command(command, context) {
                let better = best
                    .as_ref()
                    .is_none_or(|current| keys.len() < current.len());
                if better {
                    best = Some(keys);
                }
            }
        }
        best.map(|keys| format_sequence(&keys))
    }

    /// Every key sequence bound to `commands`, for a complete hint list.
    pub fn hints_for(&self, commands: &[&str], context: &KeyContext) -> Vec<String> {
        let mut hints: Vec<String> = commands
            .iter()
            .flat_map(|command| self.keys_for_command(command, context))
            .map(|keys| format_sequence(&keys))
            .collect();
        hints.sort();
        hints.dedup();
        hints
    }

    pub fn sequence_timeout(&self) -> Duration {
        self.sequence_timeout
    }

    pub fn set_sequence_timeout(&mut self, timeout: Duration) {
        self.sequence_timeout = timeout;
    }

    /// Returns whether input is waiting for more keys (a count, register, or sequence).
    pub fn is_pending(&self) -> bool {
        !matches!(self.prefix, Prefix::None)
    }

    /// Clears a prefix that waited too long. Returns whether anything was cleared.
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

        if let Prefix::Register { count } = &self.prefix {
            let count = *count;
            return self.feed_register(key, count);
        }

        // `"` selects a register unless a binding uses it.
        if key == Key::Char('"') && !self.has_prefix(&[key], context) {
            self.prefix = Prefix::Register {
                count: self.prefix.count(),
            };
            self.last_key = Some(Instant::now());
            return KeymapOutcome::Pending;
        }

        // Digits build a count. `0` is a motion until a count has started.
        if let Key::Char(character) = key
            && character.is_ascii_digit()
            && (self.prefix.count().is_some()
                || (character != '0' && !self.has_prefix(&[key], context)))
        {
            let digit = character.to_digit(10).unwrap_or(0);
            self.prefix.append_count(digit);
            self.last_key = Some(Instant::now());
            return KeymapOutcome::Pending;
        }

        self.feed_binding(key, context)
    }

    /// Consumes the key that names a register after `"`.
    fn feed_register(&mut self, key: Key, count: Option<u32>) -> KeymapOutcome {
        if let Key::Char(character) = key {
            self.prefix = Prefix::Sequence {
                keys: Vec::new(),
                count,
                register: character.is_ascii_lowercase().then_some(character),
            };
            self.last_key = Some(Instant::now());
            return KeymapOutcome::Pending;
        }

        self.reset();
        KeymapOutcome::Ignored
    }

    /// Appends `key` to the sequence and resolves or waits for a binding.
    fn feed_binding(&mut self, key: Key, context: &KeyContext) -> KeymapOutcome {
        self.prefix.push_key(key);

        let keys = self.prefix.keys();
        let mut exact = None;
        let mut prefix = false;
        for (index, binding) in self.bindings.iter().enumerate() {
            if !self.matches(binding, context) {
                continue;
            }
            if binding.keys == keys {
                exact = Some(index);
            } else if binding.keys.len() > keys.len() && binding.keys.starts_with(keys) {
                prefix = true;
            }
        }

        if let Some(index) = exact {
            let binding = &self.bindings[index];
            let args = KeymapArgs {
                count: self.prefix.count(),
                register: match &self.prefix {
                    Prefix::Sequence { register, .. } => *register,
                    _ => None,
                },
            };
            let id = binding.command.clone();
            let values = binding.values.clone();
            self.reset();
            return KeymapOutcome::Command { id, args, values };
        }

        if prefix {
            self.last_key = Some(Instant::now());
            return KeymapOutcome::Pending;
        }

        self.reset();
        KeymapOutcome::Ignored
    }

    fn reset(&mut self) {
        self.prefix = Prefix::None;
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
        let mut sequence = self.prefix.keys().to_vec();
        sequence.extend_from_slice(keys);
        self.bindings.iter().any(|binding| {
            self.matches(binding, context)
                && binding.keys.len() >= sequence.len()
                && binding.keys.starts_with(&sequence)
        })
    }
}

#[derive(Debug, Deserialize)]
struct ContextBlockSpec {
    #[serde(default)]
    when: Option<String>,
    bindings: indexmap::IndexMap<String, BindingTarget>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BindingTarget {
    Command(String),
    Detailed {
        command: String,
        #[serde(default)]
        args: Option<JsonValue>,
    },
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

    fn command(outcome: KeymapOutcome) -> (String, KeymapArgs) {
        match outcome {
            KeymapOutcome::Command { id, args, .. } => (id, args),
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
            assert_eq!(args, KeymapArgs::default());
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
    fn zero_starts_a_motion_until_a_count_has_started() {
        let mut keymap = keymap();
        let context = normal();

        let (id, args) = command(keymap.feed(Key::Char('0'), &context));
        assert_eq!(id, "vy.motion.line_start");
        assert_eq!(args.count, None);

        assert_eq!(
            keymap.feed(Key::Char('2'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('0'), &context),
            KeymapOutcome::Pending
        );
        let (id, args) = command(keymap.feed(Key::Char('j'), &context));
        assert_eq!(id, "vy.motion.down");
        assert_eq!(args.count, Some(20));
    }

    #[test]
    fn a_register_survives_a_partial_sequence() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('"'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('a'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('g'), &context),
            KeymapOutcome::Pending
        );
        let (id, args) = command(keymap.feed(Key::Char('g'), &context));
        assert_eq!(id, "vy.motion.document_start");
        assert_eq!(args.register, Some('a'));
    }

    #[test]
    fn a_count_and_a_register_combine() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('2'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('"'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('a'), &context),
            KeymapOutcome::Pending
        );
        let (id, args) = command(keymap.feed(Key::Char('j'), &context));
        assert_eq!(id, "vy.motion.down");
        assert_eq!(args.count, Some(2));
        assert_eq!(args.register, Some('a'));
    }

    #[test]
    fn a_chosen_register_expires_after_the_timeout() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('"'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('a'), &context),
            KeymapOutcome::Pending
        );
        assert!(keymap.expire(Instant::now() + Duration::from_secs(1)));
        assert!(!keymap.is_pending());
    }

    #[test]
    fn escape_clears_a_count() {
        let mut keymap = keymap();
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('2'), &context),
            KeymapOutcome::Pending
        );
        let (id, args) = command(keymap.feed(Key::Esc, &context));
        assert_eq!(id, "vy.mode.normal");
        assert_eq!(args.count, None);
        assert!(!keymap.is_pending());
    }

    #[test]
    fn passes_binding_arguments() {
        let mut keymap = Keymap::new(Duration::from_millis(500));
        keymap
            .bind_with_args(
                "g z",
                "user.hello",
                None,
                serde_json::json!({ "name": "Ada" }),
                Owner::Init,
            )
            .expect("bind");
        let context = normal();

        assert_eq!(
            keymap.feed(Key::Char('g'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            keymap.feed(Key::Char('z'), &context),
            KeymapOutcome::Command {
                id: "user.hello".to_string(),
                args: KeymapArgs::default(),
                values: serde_json::json!({ "name": "Ada" }),
            }
        );
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

        // The editor motion, the select/visual motion, and the tree collapse key
        // (`vy.ui.collapse`, limited to `uiRole == 'tree'`).
        assert_eq!(keymap.unbind("h").expect("unbind"), 3);
        // `unbind` removes the defaults too, so the key is no longer bound.
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

    #[test]
    fn pane_kind_and_overlay_separate_editor_keys_from_panel_keys() {
        let mut keymap = Keymap::new(Duration::from_millis(500));
        keymap
            .bind(
                "j",
                "vy.motion.down",
                Some("mode != 'insert' && paneKind == 'editor' && overlay == 'none'"),
                Owner::Builtin,
            )
            .expect("bind");
        keymap
            .bind("j", "vy.ui.next", Some("uiRole == 'list'"), Owner::Builtin)
            .expect("bind");

        let editor = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &editor)).0,
            "vy.motion.down"
        );

        // In a UI pane with no widget focus, the editor binding no longer applies.
        let ui = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            view_type: "example.tree".to_string(),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(keymap.feed(Key::Char('j'), &ui), KeymapOutcome::Ignored);

        // Inside an overlay the editor keys are shadowed and the list keys apply.
        let overlay = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            overlay: OverlayKind::Palette,
            ui_role: Some(UiRole::List),
            ui_focus: Some("results".to_string()),
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &overlay)).0,
            "vy.ui.next"
        );
    }

    #[test]
    fn the_default_keymap_binds_the_ui_action_vocabulary() {
        let mut keymap = keymap();

        let list = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::List),
            ui_focus: Some("results".to_string()),
            ..KeyContext::default()
        };
        for (keys, expected) in [
            (vec![Key::Char('j')], "vy.ui.next"),
            (vec![Key::Down], "vy.ui.next"),
            (vec![Key::Ctrl('n')], "vy.ui.next"),
            (vec![Key::Char('k')], "vy.ui.prev"),
            (vec![Key::Up], "vy.ui.prev"),
            (vec![Key::Ctrl('p')], "vy.ui.prev"),
            (vec![Key::Ctrl('d')], "vy.ui.page_down"),
            (vec![Key::Ctrl('u')], "vy.ui.page_up"),
            (vec![Key::Char('g'), Key::Char('g')], "vy.ui.first"),
            (vec![Key::Char('G')], "vy.ui.last"),
            (vec![Key::Enter], "vy.ui.activate"),
            (vec![Key::Char('x')], "vy.ui.toggle"),
            (vec![Key::Char('y')], "vy.ui.yank"),
        ] {
            let mut last = None;
            for key in keys {
                last = Some(keymap.feed(key, &list));
            }
            let (id, _) = command(last.expect("at least one key"));
            assert_eq!(id, expected, "list keys");
        }

        // A list has no expansion, so `h` and `l` stay unbound in a list.
        assert_eq!(keymap.feed(Key::Char('l'), &list), KeymapOutcome::Ignored);

        let tree = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::Tree),
            ui_focus: Some("files".to_string()),
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('l'), &tree)).0,
            "vy.ui.expand"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('h'), &tree)).0,
            "vy.ui.collapse"
        );

        // An editor pane keeps its own keys.
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &normal())).0,
            "vy.motion.down"
        );
    }

    #[test]
    fn focus_keys_reach_the_keymap_in_a_ui_pane() {
        let mut keymap = keymap();
        let context = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::List),
            ui_focus: Some("results".to_string()),
            ..KeyContext::default()
        };

        assert_eq!(
            command(keymap.feed(Key::Tab, &context)).0,
            "vy.ui.focus_next"
        );
        assert_eq!(
            command(keymap.feed(Key::BackTab, &context)).0,
            "vy.ui.focus_prev"
        );
    }

    #[test]
    fn surfaces_bind_their_own_keys() {
        let mut keymap = keymap();
        for overlay in [OverlayKind::Palette, OverlayKind::Picker] {
            let context = KeyContext {
                mode: Mode::Normal,
                overlay,
                ..KeyContext::default()
            };
            assert_eq!(command(keymap.feed(Key::Esc, &context)).0, "vy.ui.cancel");
            assert_eq!(
                command(keymap.feed(Key::Char('j'), &context)).0,
                "vy.ui.next"
            );
            assert_eq!(
                command(keymap.feed(Key::Enter, &context)).0,
                "vy.ui.activate"
            );
        }

        // The Ex line keeps no motion keys, but it does own its editing keys.
        let ex = KeyContext {
            mode: Mode::Normal,
            overlay: OverlayKind::Ex,
            ..KeyContext::default()
        };
        assert_eq!(keymap.feed(Key::Char('j'), &ex), KeymapOutcome::Ignored);
        for (key, expected) in [
            (Key::Up, "vy.ui.history_prev"),
            (Key::Ctrl('p'), "vy.ui.history_prev"),
            (Key::Down, "vy.ui.history_next"),
            (Key::Ctrl('n'), "vy.ui.history_next"),
            (Key::Tab, "vy.ui.complete_next"),
            (Key::BackTab, "vy.ui.complete_prev"),
            (Key::Backspace, "vy.ui.delete_char"),
            (Key::Ctrl('u'), "vy.ui.clear_input"),
            (Key::Esc, "vy.ui.cancel"),
            (Key::Enter, "vy.ui.activate"),
        ] {
            match keymap.feed(key, &ex) {
                KeymapOutcome::Command { id, .. } => {
                    assert_eq!(id, expected, "key {key:?}");
                }
                other => panic!("key {key:?} resolved to {other:?}"),
            }
        }
    }

    #[test]
    fn hints_come_from_the_bindings_and_follow_a_rebind() {
        let mut keymap = keymap();
        let context = KeyContext {
            mode: Mode::Insert,
            overlay: OverlayKind::Palette,
            ..KeyContext::default()
        };

        // The shortest binding is shown: `j` beats `down` and `ctrl-n`.
        assert_eq!(
            keymap.hint_for(&["vy.ui.next"], &context).as_deref(),
            Some("j")
        );
        assert_eq!(
            keymap.hint_for(&["vy.ui.activate"], &context).as_deref(),
            Some("enter")
        );
        assert_eq!(
            keymap.hint_for(&["vy.ui.cancel"], &context).as_deref(),
            Some("esc")
        );

        // A rebind of the same length wins, matching dispatch order (the last
        // binding is the one that runs).
        keymap
            .bind(
                "ctrl-j",
                "vy.ui.next",
                Some("overlay == 'palette'"),
                Owner::Init,
            )
            .expect("bind");
        assert_eq!(
            keymap.hints_for(&["vy.ui.next"], &context),
            vec![
                "ctrl-j".to_string(),
                "ctrl-n".to_string(),
                "down".to_string(),
                "j".to_string(),
            ]
        );

        // `j` is still the shortest, so it stays the shown hint.
        assert_eq!(
            keymap.hint_for(&["vy.ui.next"], &context).as_deref(),
            Some("j")
        );
        assert_eq!(
            keymap.hint_for(&["vy.ui.next"], &context).as_deref(),
            Some("j")
        );

        // Unbinding `j` promotes the next shortest key to the hint.
        keymap.unbind("j").expect("unbind");
        assert_eq!(
            keymap.hint_for(&["vy.ui.next"], &context).as_deref(),
            Some("down")
        );

        // Commands with no matching binding report no hint.
        assert_eq!(keymap.hint_for(&["vy.ui.expand"], &context), None);
    }

    #[test]
    fn key_names_round_trip_through_parse() {
        for name in [
            "j",
            "G",
            "space",
            "ctrl-d",
            "esc",
            "enter",
            "tab",
            "backtab",
            "left",
            "right",
            "up",
            "down",
            "home",
            "end",
            "backspace",
            "delete",
        ] {
            let key = Key::parse(name).expect("known key");
            assert_eq!(key.name(), name, "name of {name}");
            assert_eq!(parse_sequence(&key.name()), Ok(vec![key]));
        }

        assert_eq!(
            format_sequence(&parse_sequence("g g").expect("sequence")),
            "g g"
        );
        assert_eq!(
            format_sequence(&parse_sequence("space f f").expect("sequence")),
            "space f f"
        );
    }

    #[test]
    fn unset_ui_context_values_are_absent() {
        let context = KeyContext::default();
        // `uiRole` and `uiFocus` only exist while a widget has focus, so a
        // condition on them is false in an editor pane.
        assert_eq!(context.value("uiRole"), None);
        assert_eq!(context.value("uiFocus"), None);
        assert_eq!(
            context.value("paneKind"),
            Some(Value::Text("editor".to_string()))
        );
        assert_eq!(
            context.value("overlay"),
            Some(Value::Text("none".to_string()))
        );
        assert_eq!(context.value("uiSelectionCount"), Some(Value::Number(0.0)));
    }

    #[test]
    fn ui_pane_without_a_role_still_gets_the_movement_keys() {
        let mut keymap = keymap();
        // A UI pane whose tree has not arrived yet reports no `uiRole`.
        let context = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            view_type: "example.tree".to_string(),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(context.value("uiRole"), None);

        for (key, expected) in [
            (Key::Char('j'), "vy.ui.next"),
            (Key::Char('k'), "vy.ui.prev"),
            (Key::Char('G'), "vy.ui.last"),
            (Key::Tab, "vy.ui.focus_next"),
            (Key::BackTab, "vy.ui.focus_prev"),
        ] {
            assert_eq!(
                command(keymap.feed(key, &context)).0,
                expected,
                "key {key:?}"
            );
        }

        // `g g` needs two keys.
        assert_eq!(
            keymap.feed(Key::Char('g'), &context),
            KeymapOutcome::Pending
        );
        assert_eq!(
            command(keymap.feed(Key::Char('g'), &context)).0,
            "vy.ui.first"
        );

        // An editor pane keeps its own keys.
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &normal())).0,
            "vy.motion.down"
        );
    }

    #[test]
    fn overlay_and_pane_kind_names_round_trip() {
        for kind in [
            PaneKind::Editor,
            PaneKind::Ui,
            PaneKind::Webview,
            PaneKind::Terminal,
            PaneKind::Overlay,
        ] {
            assert_eq!(PaneKind::parse(kind.name()), Some(kind));
        }
        for kind in [
            OverlayKind::None,
            OverlayKind::Palette,
            OverlayKind::Picker,
            OverlayKind::Ex,
            OverlayKind::Prompt,
            OverlayKind::Approval,
            OverlayKind::Settings,
            OverlayKind::Hover,
            OverlayKind::Menu,
            OverlayKind::Custom,
        ] {
            assert_eq!(OverlayKind::parse(kind.name()), Some(kind));
        }
        assert_eq!(PaneKind::parse("bogus"), None);
        assert_eq!(OverlayKind::parse("bogus"), None);
        assert!(OverlayKind::Palette.is_modal());
        assert!(!OverlayKind::Hover.is_modal());
        assert!(!OverlayKind::Custom.is_modal());
    }

    #[test]
    fn semantic_predicates_and_orthogonal_context() {
        // 1. Editor context
        let editor = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(editor.value("editorFocus"), Some(Value::Bool(true)));
        assert_eq!(editor.value("modal"), Some(Value::Bool(false)));
        assert_eq!(editor.value("hasSelection"), Some(Value::Bool(false)));

        // 2. UI context with list
        let ui_list = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::List),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(ui_list.value("editorFocus"), Some(Value::Bool(false)));
        assert_eq!(ui_list.value("modal"), Some(Value::Bool(false)));
        assert_eq!(ui_list.value("hasSelection"), Some(Value::Bool(true)));
        assert_eq!(ui_list.value("uiRole"), Some(Value::Text("list".into())));

        // 3. UI context without widget focus (!uiRole)
        let ui_no_role = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: None,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(ui_no_role.value("editorFocus"), Some(Value::Bool(false)));
        assert_eq!(ui_no_role.value("modal"), Some(Value::Bool(false)));
        assert_eq!(ui_no_role.value("hasSelection"), Some(Value::Bool(false)));
        assert_eq!(ui_no_role.value("uiRole"), None);

        // 4. Modal overlay contexts inherit default surface role and mask editor focus
        for (overlay, expected_role, expected_has_selection) in [
            (OverlayKind::Palette, "list", true),
            (OverlayKind::Picker, "list", true),
            (OverlayKind::Ex, "textInput", false),
            (OverlayKind::Prompt, "textInput", false),
            (OverlayKind::Approval, "button", false),
            (OverlayKind::Settings, "list", true),
        ] {
            let modal_ctx = KeyContext {
                mode: Mode::Insert,
                pane_kind: PaneKind::Overlay,
                overlay,
                ..KeyContext::default()
            };
            assert_eq!(
                modal_ctx.value("editorFocus"),
                Some(Value::Bool(false)),
                "modal {overlay:?} masks editor focus"
            );
            assert_eq!(
                modal_ctx.value("modal"),
                Some(Value::Bool(true)),
                "modal {overlay:?}"
            );
            assert_eq!(
                modal_ctx.value("hasSelection"),
                Some(Value::Bool(expected_has_selection)),
                "hasSelection for {overlay:?}"
            );
            assert_eq!(
                modal_ctx.value("uiRole"),
                Some(Value::Text(expected_role.into())),
                "uiRole for {overlay:?}"
            );
        }
    }

    #[test]
    fn reachable_contexts_key_dispatch_matrix() {
        let mut keymap = keymap();

        // Editor normal mode
        let ed_normal = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &ed_normal)).0,
            "vy.motion.down"
        );
        assert_eq!(
            command(keymap.feed(Key::Char(':'), &ed_normal)).0,
            "vy.ui.ex_command_line"
        );
        assert_eq!(
            command(keymap.feed(Key::Char(' '), &ed_normal)).0,
            "vy.ui.command_palette"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('i'), &ed_normal)).0,
            "vy.mode.insert"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('a'), &ed_normal)).0,
            "vy.mode.insert_after"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('d'), &ed_normal)).0,
            "vy.operator.delete"
        );

        // Editor operator pending: motions still work, 'i' and 'a' become inner/around
        let ed_op_pending = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            operator_pending: true,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('w'), &ed_op_pending)).0,
            "vy.motion.word_forward"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('d'), &ed_op_pending)).0,
            "vy.operator.delete"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('i'), &ed_op_pending)).0,
            "vy.operator.inner"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('a'), &ed_op_pending)).0,
            "vy.operator.around"
        );

        // Editor text-object pending: text object bindings apply, general motions excluded
        let ed_to_pending = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            text_object_pending: true,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('w'), &ed_to_pending)).0,
            "vy.text_object.word"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('p'), &ed_to_pending)).0,
            "vy.text_object.paragraph"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('('), &ed_to_pending)).0,
            "vy.text_object.paren"
        );
        assert_eq!(
            keymap.feed(Key::Char('j'), &ed_to_pending),
            KeymapOutcome::Ignored
        );

        // Editor insert mode: insert navigation/editing keys apply
        let ed_insert = KeyContext {
            mode: Mode::Insert,
            pane_kind: PaneKind::Editor,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Enter, &ed_insert)).0,
            "vy.insert.newline"
        );
        assert_eq!(
            command(keymap.feed(Key::Tab, &ed_insert)).0,
            "vy.insert.tab"
        );
        assert_eq!(
            command(keymap.feed(Key::Backspace, &ed_insert)).0,
            "vy.insert.backspace"
        );
        assert_eq!(
            command(keymap.feed(Key::Esc, &ed_insert)).0,
            "vy.mode.normal"
        );
        assert_eq!(
            keymap.feed(Key::Char('j'), &ed_insert),
            KeymapOutcome::Ignored
        );

        // UI pane with List: shared UI motion and selection actions
        let ui_list = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::List),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &ui_list)).0,
            "vy.ui.next"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('k'), &ui_list)).0,
            "vy.ui.prev"
        );
        assert_eq!(
            command(keymap.feed(Key::Ctrl('d'), &ui_list)).0,
            "vy.ui.page_down"
        );
        assert_eq!(
            command(keymap.feed(Key::Ctrl('u'), &ui_list)).0,
            "vy.ui.page_up"
        );
        assert_eq!(
            command(keymap.feed(Key::Tab, &ui_list)).0,
            "vy.ui.focus_next"
        );
        assert_eq!(
            keymap.feed(Key::Char('l'), &ui_list),
            KeymapOutcome::Ignored
        );

        // UI pane with Tree: expand and collapse
        let ui_tree = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::Tree),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('l'), &ui_tree)).0,
            "vy.ui.expand"
        );
        assert_eq!(
            command(keymap.feed(Key::Char('h'), &ui_tree)).0,
            "vy.ui.collapse"
        );

        // UI pane without focused widget: navigation still works, but not page_up/yank
        let ui_unfocused = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: None,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &ui_unfocused)).0,
            "vy.ui.next"
        );
        assert_eq!(
            command(keymap.feed(Key::Tab, &ui_unfocused)).0,
            "vy.ui.focus_next"
        );
        assert_eq!(
            keymap.feed(Key::Ctrl('d'), &ui_unfocused),
            KeymapOutcome::Ignored
        );

        // Modal Palette/Picker
        let palette = KeyContext {
            mode: Mode::Insert,
            pane_kind: PaneKind::Overlay,
            overlay: OverlayKind::Palette,
            ..KeyContext::default()
        };
        assert_eq!(
            command(keymap.feed(Key::Char('j'), &palette)).0,
            "vy.ui.next"
        );
        assert_eq!(command(keymap.feed(Key::Esc, &palette)).0, "vy.ui.cancel");
        assert_eq!(
            command(keymap.feed(Key::Backspace, &palette)).0,
            "vy.ui.delete_char"
        );
        assert_eq!(
            command(keymap.feed(Key::Ctrl('u'), &palette)).0,
            "vy.ui.clear_input"
        );
        // Background keys are masked
        assert_eq!(
            keymap.feed(Key::Char(' '), &palette),
            KeymapOutcome::Ignored
        );

        // Modal Ex line
        let ex = KeyContext {
            mode: Mode::Insert,
            pane_kind: PaneKind::Overlay,
            overlay: OverlayKind::Ex,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Up, &ex)).0, "vy.ui.history_prev");
        assert_eq!(command(keymap.feed(Key::Down, &ex)).0, "vy.ui.history_next");
        assert_eq!(command(keymap.feed(Key::Tab, &ex)).0, "vy.ui.complete_next");
        assert_eq!(command(keymap.feed(Key::Enter, &ex)).0, "vy.ui.activate");
        assert_eq!(command(keymap.feed(Key::Esc, &ex)).0, "vy.ui.cancel");
        assert_eq!(keymap.feed(Key::Char('j'), &ex), KeymapOutcome::Ignored);
    }

    #[test]
    fn semantic_predicates_and_orthogonal_context() {
        // 1. Editor context
        let editor = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(editor.value("editorFocus"), Some(Value::Bool(true)));
        assert_eq!(editor.value("modal"), Some(Value::Bool(false)));
        assert_eq!(editor.value("hasSelection"), Some(Value::Bool(false)));

        // 2. UI context with list
        let ui_list = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::List),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(ui_list.value("editorFocus"), Some(Value::Bool(false)));
        assert_eq!(ui_list.value("modal"), Some(Value::Bool(false)));
        assert_eq!(ui_list.value("hasSelection"), Some(Value::Bool(true)));
        assert_eq!(ui_list.value("uiRole"), Some(Value::Text("list".into())));

        // 3. UI context without widget focus (!uiRole)
        let ui_no_role = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: None,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(ui_no_role.value("editorFocus"), Some(Value::Bool(false)));
        assert_eq!(ui_no_role.value("modal"), Some(Value::Bool(false)));
        assert_eq!(ui_no_role.value("hasSelection"), Some(Value::Bool(false)));
        assert_eq!(ui_no_role.value("uiRole"), None);

        // 4. Modal overlay contexts inherit default surface role and mask editor focus
        for (overlay, expected_role, expected_has_selection) in [
            (OverlayKind::Palette, "list", true),
            (OverlayKind::Picker, "list", true),
            (OverlayKind::Ex, "textInput", false),
            (OverlayKind::Prompt, "textInput", false),
            (OverlayKind::Approval, "button", false),
            (OverlayKind::Settings, "list", true),
        ] {
            let modal_ctx = KeyContext {
                mode: Mode::Insert,
                pane_kind: PaneKind::Overlay,
                overlay,
                ..KeyContext::default()
            };
            assert_eq!(
                modal_ctx.value("editorFocus"),
                Some(Value::Bool(false)),
                "modal {overlay:?} masks editor focus"
            );
            assert_eq!(
                modal_ctx.value("modal"),
                Some(Value::Bool(true)),
                "modal {overlay:?}"
            );
            assert_eq!(
                modal_ctx.value("hasSelection"),
                Some(Value::Bool(expected_has_selection)),
                "hasSelection for {overlay:?}"
            );
            assert_eq!(
                modal_ctx.value("uiRole"),
                Some(Value::Text(expected_role.into())),
                "uiRole for {overlay:?}"
            );
        }
    }

    #[test]
    fn reachable_contexts_key_dispatch_matrix() {
        let mut keymap = keymap();

        // Editor normal mode
        let ed_normal = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Char('j'), &ed_normal)).0, "vy.motion.down");
        assert_eq!(command(keymap.feed(Key::Char(':'), &ed_normal)).0, "vy.ui.ex_command_line");
        assert_eq!(command(keymap.feed(Key::Char(' '), &ed_normal)).0, "vy.ui.command_palette");
        assert_eq!(command(keymap.feed(Key::Char('i'), &ed_normal)).0, "vy.mode.insert");
        assert_eq!(command(keymap.feed(Key::Char('a'), &ed_normal)).0, "vy.mode.insert_after");
        assert_eq!(command(keymap.feed(Key::Char('d'), &ed_normal)).0, "vy.operator.delete");

        // Editor operator pending: motions still work, 'i' and 'a' become inner/around
        let ed_op_pending = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            operator_pending: true,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Char('w'), &ed_op_pending)).0, "vy.motion.word_forward");
        assert_eq!(command(keymap.feed(Key::Char('d'), &ed_op_pending)).0, "vy.operator.delete");
        assert_eq!(command(keymap.feed(Key::Char('i'), &ed_op_pending)).0, "vy.operator.inner");
        assert_eq!(command(keymap.feed(Key::Char('a'), &ed_op_pending)).0, "vy.operator.around");

        // Editor text-object pending: text object bindings apply, general motions excluded
        let ed_to_pending = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Editor,
            text_object_pending: true,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Char('w'), &ed_to_pending)).0, "vy.text_object.word");
        assert_eq!(command(keymap.feed(Key::Char('p'), &ed_to_pending)).0, "vy.text_object.paragraph");
        assert_eq!(command(keymap.feed(Key::Char('('), &ed_to_pending)).0, "vy.text_object.paren");
        assert_eq!(keymap.feed(Key::Char('j'), &ed_to_pending), KeymapOutcome::Ignored);

        // Editor insert mode: insert navigation/editing keys apply
        let ed_insert = KeyContext {
            mode: Mode::Insert,
            pane_kind: PaneKind::Editor,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Enter, &ed_insert)).0, "vy.insert.newline");
        assert_eq!(command(keymap.feed(Key::Tab, &ed_insert)).0, "vy.insert.tab");
        assert_eq!(command(keymap.feed(Key::Backspace, &ed_insert)).0, "vy.insert.backspace");
        assert_eq!(command(keymap.feed(Key::Esc, &ed_insert)).0, "vy.mode.normal");
        assert_eq!(keymap.feed(Key::Char('j'), &ed_insert), KeymapOutcome::Ignored);

        // UI pane with List: shared UI motion and selection actions
        let ui_list = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::List),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Char('j'), &ui_list)).0, "vy.ui.next");
        assert_eq!(command(keymap.feed(Key::Char('k'), &ui_list)).0, "vy.ui.prev");
        assert_eq!(command(keymap.feed(Key::Ctrl('d'), &ui_list)).0, "vy.ui.page_down");
        assert_eq!(command(keymap.feed(Key::Ctrl('u'), &ui_list)).0, "vy.ui.page_up");
        assert_eq!(command(keymap.feed(Key::Tab, &ui_list)).0, "vy.ui.focus_next");
        assert_eq!(keymap.feed(Key::Char('l'), &ui_list), KeymapOutcome::Ignored);

        // UI pane with Tree: expand and collapse
        let ui_tree = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: Some(UiRole::Tree),
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Char('l'), &ui_tree)).0, "vy.ui.expand");
        assert_eq!(command(keymap.feed(Key::Char('h'), &ui_tree)).0, "vy.ui.collapse");

        // UI pane without focused widget: navigation still works, but not page_up/yank
        let ui_unfocused = KeyContext {
            mode: Mode::Normal,
            pane_kind: PaneKind::Ui,
            ui_role: None,
            overlay: OverlayKind::None,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Char('j'), &ui_unfocused)).0, "vy.ui.next");
        assert_eq!(command(keymap.feed(Key::Tab, &ui_unfocused)).0, "vy.ui.focus_next");
        assert_eq!(keymap.feed(Key::Ctrl('d'), &ui_unfocused), KeymapOutcome::Ignored);

        // Modal Palette/Picker
        let palette = KeyContext {
            mode: Mode::Insert,
            pane_kind: PaneKind::Overlay,
            overlay: OverlayKind::Palette,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Char('j'), &palette)).0, "vy.ui.next");
        assert_eq!(command(keymap.feed(Key::Esc, &palette)).0, "vy.ui.cancel");
        assert_eq!(command(keymap.feed(Key::Backspace, &palette)).0, "vy.ui.delete_char");
        assert_eq!(command(keymap.feed(Key::Ctrl('u'), &palette)).0, "vy.ui.clear_input");
        // Background keys are masked
        assert_eq!(keymap.feed(Key::Char(' '), &palette), KeymapOutcome::Ignored);

        // Modal Ex line
        let ex = KeyContext {
            mode: Mode::Insert,
            pane_kind: PaneKind::Overlay,
            overlay: OverlayKind::Ex,
            ..KeyContext::default()
        };
        assert_eq!(command(keymap.feed(Key::Up, &ex)).0, "vy.ui.history_prev");
        assert_eq!(command(keymap.feed(Key::Down, &ex)).0, "vy.ui.history_next");
        assert_eq!(command(keymap.feed(Key::Tab, &ex)).0, "vy.ui.complete_next");
        assert_eq!(command(keymap.feed(Key::Enter, &ex)).0, "vy.ui.activate");
        assert_eq!(command(keymap.feed(Key::Esc, &ex)).0, "vy.ui.cancel");
        assert_eq!(keymap.feed(Key::Char('j'), &ex), KeymapOutcome::Ignored);
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    #[test]
    fn serde_accepts_the_names_scripts_use() {
        for name in ["normal", "insert", "select", "select_line", "select_block"] {
            let mode = Mode::parse(name).expect("known name");
            let json = serde_json::to_string(&mode).expect("serialize");
            assert_eq!(json, format!("\"{name}\""));
            let parsed: Mode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, mode);
        }

        for alias in ["visual", "select-line", "visual_line", "visual-block"] {
            let parsed: Mode = serde_json::from_value(serde_json::json!(alias)).expect("alias");
            assert_eq!(parsed, Mode::parse(alias).expect("known alias"));
        }

        assert!(serde_json::from_value::<Mode>(serde_json::json!("bogus")).is_err());
    }
}
