//! Modal overlay state and the shared single-line input.
//!
//! Only one overlay owns input and focus at a time. The palette, the Ex command
//! line, the argument prompt, and capability approvals share [`TextInput`], so
//! IME composition, caret drawing, and completion behave the same everywhere.

use serde_json::{Map, Value};

use crate::commands::{ArgList, CommandArg};
use crate::ex::ExState;
use crate::palette::PaletteState;

/// The modal overlay currently owning input and focus.
///
/// Exactly one overlay exists at a time, so the size difference between
/// variants is not worth boxing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Overlay {
    /// Command, document, or plugin picker.
    Palette(PaletteState),
    /// Ex command line at the bottom of the window.
    Ex(ExState),
    /// One command argument, entered after choosing a palette command.
    Prompt(PromptState),
    /// Capability approval for an external plugin.
    Approval(ApprovalState),
    /// Settings list opened by `:set` or `vy.ui.open_settings`.
    Settings(SettingsState),
    /// Choice picker for an enumerated setting.
    SettingChoice(SettingChoiceState),
    /// Text or number input for one setting.
    SettingPrompt(SettingPromptState),
}

impl Overlay {
    /// The editable line of this overlay.
    pub fn input(&self) -> &TextInput {
        match self {
            Self::Palette(palette) => &palette.input,
            Self::Ex(ex) => &ex.input,
            Self::Prompt(prompt) => &prompt.input,
            Self::Approval(approval) => &approval.input,
            Self::Settings(settings) => &settings.input,
            Self::SettingChoice(choice) => &choice.input,
            Self::SettingPrompt(prompt) => &prompt.input,
        }
    }

    pub fn input_mut(&mut self) -> &mut TextInput {
        match self {
            Self::Palette(palette) => &mut palette.input,
            Self::Ex(ex) => &mut ex.input,
            Self::Prompt(prompt) => &mut prompt.input,
            Self::Approval(approval) => &mut approval.input,
            Self::Settings(settings) => &mut settings.input,
            Self::SettingChoice(choice) => &mut choice.input,
            Self::SettingPrompt(prompt) => &mut prompt.input,
        }
    }

    pub fn palette(&self) -> Option<&PaletteState> {
        match self {
            Self::Palette(palette) => Some(palette),
            _ => None,
        }
    }

    pub fn palette_mut(&mut self) -> Option<&mut PaletteState> {
        match self {
            Self::Palette(palette) => Some(palette),
            _ => None,
        }
    }

    pub fn ex(&self) -> Option<&ExState> {
        match self {
            Self::Ex(ex) => Some(ex),
            _ => None,
        }
    }

    pub fn prompt(&self) -> Option<&PromptState> {
        match self {
            Self::Prompt(prompt) => Some(prompt),
            _ => None,
        }
    }

    pub fn prompt_mut(&mut self) -> Option<&mut PromptState> {
        match self {
            Self::Prompt(prompt) => Some(prompt),
            _ => None,
        }
    }

    pub fn approval(&self) -> Option<&ApprovalState> {
        match self {
            Self::Approval(approval) => Some(approval),
            _ => None,
        }
    }

    /// Index of the selected list row, if this overlay has a list.
    pub fn selected(&self) -> Option<usize> {
        match self {
            Self::Palette(palette) => Some(palette.selected),
            Self::Prompt(prompt) => Some(prompt.selected),
            Self::Approval(approval) => Some(approval.selected),
            Self::Settings(settings) => Some(settings.selected),
            Self::SettingChoice(choice) => Some(choice.selected),
            Self::Ex(_) | Self::SettingPrompt(_) => None,
        }
    }

    pub fn selected_mut(&mut self) -> Option<&mut usize> {
        match self {
            Self::Palette(palette) => Some(&mut palette.selected),
            Self::Prompt(prompt) => Some(&mut prompt.selected),
            Self::Approval(approval) => Some(&mut approval.selected),
            Self::Settings(settings) => Some(&mut settings.selected),
            Self::SettingChoice(choice) => Some(&mut choice.selected),
            Self::Ex(_) | Self::SettingPrompt(_) => None,
        }
    }

    /// Whether this overlay floats inside the editor area.
    ///
    /// The Ex line is drawn below the statusline instead.
    pub fn is_floating(&self) -> bool {
        !matches!(self, Self::Ex(_))
    }

    /// Text drawn before the input, shared by rendering and caret placement.
    pub fn prefix(&self) -> String {
        match self {
            Self::Palette(palette) => palette.prefix(),
            Self::Prompt(prompt) => prompt.prefix(),
            Self::Approval(approval) => format!("Approve {}? > ", approval.plugin_name),
            Self::Settings(_) => "settings > ".to_string(),
            Self::SettingChoice(choice) => format!("{} > ", choice.key),
            Self::SettingPrompt(prompt) => format!("{} > ", prompt.key),
            Self::Ex(_) => ":".to_string(),
        }
    }

    /// Appends a typed character through the overlay-specific bookkeeping.
    pub fn push_char(&mut self, character: char) {
        match self {
            Self::Palette(palette) => palette.push_char(character),
            Self::Ex(ex) => ex.push_char(character),
            Self::Prompt(prompt) => {
                prompt.input.push_char(character);
                prompt.selected = 0;
            }
            Self::Approval(approval) => {
                approval.input.push_char(character);
                approval.selected = 0;
            }
            Self::Settings(settings) => {
                settings.input.push_char(character);
                settings.selected = 0;
            }
            Self::SettingChoice(choice) => {
                choice.input.push_char(character);
                choice.selected = 0;
            }
            Self::SettingPrompt(prompt) => prompt.input.push_char(character),
        }
    }

    /// Appends committed text through the overlay-specific bookkeeping.
    pub fn push_str(&mut self, text: &str) {
        match self {
            Self::Palette(palette) => palette.push_str(text),
            Self::Ex(ex) => ex.push_str(text),
            Self::Prompt(prompt) => {
                prompt.input.push_str(text);
                prompt.selected = 0;
            }
            Self::Approval(approval) => {
                approval.input.push_str(text);
                approval.selected = 0;
            }
            Self::Settings(settings) => {
                settings.input.push_str(text);
                settings.selected = 0;
            }
            Self::SettingChoice(choice) => {
                choice.input.push_str(text);
                choice.selected = 0;
            }
            Self::SettingPrompt(prompt) => prompt.input.push_str(text),
        }
    }

    /// Removes the last character through the overlay-specific bookkeeping.
    pub fn pop_char(&mut self) {
        match self {
            Self::Palette(palette) => palette.pop_char(),
            Self::Ex(ex) => ex.pop_char(),
            Self::Prompt(prompt) => {
                prompt.input.pop_char();
                prompt.selected = 0;
            }
            Self::Approval(approval) => {
                approval.input.pop_char();
                approval.selected = 0;
            }
            Self::Settings(settings) => {
                settings.input.pop_char();
                settings.selected = 0;
            }
            Self::SettingChoice(choice) => {
                choice.input.pop_char();
                choice.selected = 0;
            }
            Self::SettingPrompt(prompt) => prompt.input.pop_char(),
        }
    }

    pub fn set_preedit(&mut self, text: &str) {
        self.input_mut().set_preedit(text);
    }

    pub fn clear_preedit(&mut self) {
        self.input_mut().clear_preedit();
    }

    /// Clears the input and moves the selection back to the first row.
    pub fn clear(&mut self) {
        match self {
            Self::Palette(palette) => {
                palette.input.clear();
                palette.selected = 0;
            }
            Self::Ex(ex) => ex.clear(),
            Self::Prompt(prompt) => {
                prompt.input.clear();
                prompt.selected = 0;
            }
            Self::Approval(approval) => {
                approval.input.clear();
                approval.selected = 0;
            }
            Self::Settings(settings) => {
                settings.input.clear();
                settings.selected = 0;
            }
            Self::SettingChoice(choice) => {
                choice.input.clear();
                choice.selected = 0;
            }
            Self::SettingPrompt(prompt) => prompt.input.clear(),
        }
    }
}

/// A single-line editable input shared by every overlay.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TextInput {
    value: String,
    preedit: String,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Committed text.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// IME composition text that has not been committed yet.
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty() && self.preedit.is_empty()
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
    }

    pub fn set_preedit(&mut self, text: &str) {
        self.preedit.clear();
        self.preedit.push_str(text);
    }

    pub fn clear_preedit(&mut self) {
        self.preedit.clear();
    }

    pub fn push_char(&mut self, character: char) {
        self.value.push(character);
    }

    pub fn push_str(&mut self, text: &str) {
        self.value.push_str(text);
    }

    pub fn pop_char(&mut self) {
        self.value.pop();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.preedit.clear();
    }

    /// Committed text followed by the preedit.
    pub fn full(&self) -> String {
        format!("{}{}", self.value, self.preedit)
    }
}

/// One completion candidate: the text to put into the input and the label to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub value: String,
    pub label: String,
}

impl CompletionCandidate {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }

    /// A candidate whose value and label are the same.
    pub fn plain(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            label: value.clone(),
            value,
        }
    }
}

/// Completion candidates for a single-line input.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Input text the candidates were built for.
    prefix: String,
    candidates: Vec<CompletionCandidate>,
    selected: usize,
}

impl Completion {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub fn candidates(&self) -> &[CompletionCandidate] {
        &self.candidates
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> Option<&str> {
        self.candidates
            .get(self.selected)
            .map(|candidate| candidate.value.as_str())
    }

    pub fn reset(&mut self) {
        self.prefix.clear();
        self.candidates.clear();
        self.selected = 0;
    }

    /// Replaces the candidates with `candidates`, built from `prefix`.
    pub fn set(&mut self, prefix: impl Into<String>, candidates: Vec<CompletionCandidate>) {
        self.prefix = prefix.into();
        self.selected = 0;
        self.candidates = candidates;
    }

    /// Moves the selection, wrapping at both ends.
    pub fn cycle(&mut self, delta: isize) {
        if self.candidates.is_empty() {
            return;
        }
        let len = self.candidates.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    /// Selects the last candidate.
    pub fn select_last(&mut self) {
        self.selected = self.candidates.len().saturating_sub(1);
    }

    /// Whether `input` is still the prefix or one of the candidates.
    pub fn is_active_for(&self, input: &str) -> bool {
        !self.candidates.is_empty() && (self.prefix == input || self.selected() == Some(input))
    }
}

/// One command argument being collected after a palette selection.
#[derive(Debug)]
pub struct PromptState {
    pub command_id: String,
    pub title: String,
    pub spec: ArgList,
    pub values: Map<String, Value>,
    /// Index into the required arguments of `spec`.
    pub index: usize,
    pub input: TextInput,
    pub selected: usize,
    /// Palette to restore when the prompt is cancelled.
    pub back: PaletteState,
}

impl PromptState {
    pub fn new(
        command_id: impl Into<String>,
        title: impl Into<String>,
        spec: ArgList,
        back: PaletteState,
    ) -> Self {
        Self {
            command_id: command_id.into(),
            title: title.into(),
            spec,
            values: Map::new(),
            index: 0,
            input: TextInput::default(),
            selected: 0,
            back,
        }
    }

    /// Required arguments still to be collected.
    pub fn required(&self) -> Vec<&CommandArg> {
        self.spec.args().iter().filter(|arg| arg.required).collect()
    }

    pub fn current_arg(&self) -> Option<&CommandArg> {
        self.required().get(self.index).copied()
    }

    /// Text drawn before the prompt input.
    pub fn prefix(&self) -> String {
        match self.current_arg() {
            Some(arg) => format!("{} · {} > ", self.title, arg.name),
            None => format!("{} > ", self.title),
        }
    }
}

/// Capability approval for an external plugin.
#[derive(Debug)]
pub struct ApprovalState {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub missing: Vec<String>,
    pub input: TextInput,
    pub selected: usize,
}

impl ApprovalState {
    pub fn new(
        plugin_id: impl Into<String>,
        plugin_name: impl Into<String>,
        plugin_version: impl Into<String>,
        missing: Vec<String>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            plugin_name: plugin_name.into(),
            plugin_version: plugin_version.into(),
            missing,
            input: TextInput::default(),
            selected: 0,
        }
    }
}

/// The settings list opened by `:set` or `vy.ui.open_settings`.
#[derive(Debug, Default)]
pub struct SettingsState {
    pub input: TextInput,
    pub selected: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Choice picker for an enumerated setting.
#[derive(Debug)]
pub struct SettingChoiceState {
    pub key: String,
    pub input: TextInput,
    pub selected: usize,
    /// Settings list restored when the choice is cancelled.
    pub back: SettingsState,
}

impl SettingChoiceState {
    pub fn new(key: impl Into<String>, back: SettingsState) -> Self {
        Self {
            key: key.into(),
            input: TextInput::default(),
            selected: 0,
            back,
        }
    }
}

/// Text or number input for one setting.
#[derive(Debug)]
pub struct SettingPromptState {
    pub key: String,
    pub input: TextInput,
    /// Settings list restored when the prompt is cancelled.
    pub back: SettingsState,
}

impl SettingPromptState {
    pub fn new(key: impl Into<String>, back: SettingsState) -> Self {
        Self {
            key: key.into(),
            input: TextInput::default(),
            back,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_input_keeps_committed_text_and_preedit_apart() {
        let mut input = TextInput::new();
        input.push_str("edi");
        input.set_preedit("た");
        assert_eq!(input.value(), "edi");
        assert_eq!(input.full(), "ediた");

        input.push_char('t');
        input.clear_preedit();
        assert_eq!(input.full(), "edit");

        input.pop_char();
        assert_eq!(input.full(), "edi");
    }

    #[test]
    fn completion_cycles_and_wraps() {
        let mut completion = Completion::new();
        completion.set(
            "wr",
            vec![
                CompletionCandidate::plain("wrap"),
                CompletionCandidate::plain("write"),
            ],
        );

        assert!(completion.is_active_for("wr"));
        assert_eq!(completion.selected(), Some("wrap"));
        completion.cycle(1);
        assert_eq!(completion.selected(), Some("write"));
        completion.cycle(1);
        assert_eq!(completion.selected(), Some("wrap"));
        completion.cycle(-1);
        assert_eq!(completion.selected(), Some("write"));
        assert!(completion.is_active_for("write"));
        assert!(!completion.is_active_for("wrapper"));

        completion.select_last();
        assert_eq!(completion.selected(), Some("write"));
        completion.select_last();
        assert_eq!(completion.selected(), Some("write"));
    }

    #[test]
    fn completion_keeps_labels_apart_from_values() {
        let mut completion = Completion::new();
        completion.set("set wr", vec![CompletionCandidate::new("set wrap", "wrap")]);
        assert_eq!(completion.selected(), Some("set wrap"));
        assert_eq!(completion.candidates()[0].label, "wrap");
    }

    #[test]
    fn prompt_walks_required_arguments() {
        const ARGS: &[crate::commands::Arg] = &[
            crate::commands::Arg::required("view", crate::commands::ArgKind::Text),
            crate::commands::Arg::choice("direction", &["left", "right"], false),
        ];
        let prompt = PromptState::new(
            "vy.view.open",
            "View Open",
            ArgList::from_spec(crate::commands::ArgSpec::Positionals(ARGS)),
            PaletteState::new(),
        );

        assert_eq!(prompt.required().len(), 1);
        assert_eq!(
            prompt.current_arg().map(|arg| arg.name.as_str()),
            Some("view")
        );
        assert_eq!(prompt.prefix(), "View Open · view > ");
    }
}
