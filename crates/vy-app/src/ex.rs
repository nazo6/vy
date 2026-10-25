//! Ex command line state and history management.

/// State of an active Ex command line prompt.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExState {
    pub input: String,
    pub preedit: String,
    pub history_index: Option<usize>,
}

impl ExState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a typed character to the input line.
    pub fn push_char(&mut self, character: char) {
        self.input.push(character);
        self.history_index = None;
    }

    /// Appends committed text to the input line.
    pub fn push_str(&mut self, text: &str) {
        self.input.push_str(text);
        self.history_index = None;
    }

    /// Removes the last character from the input line.
    pub fn pop_char(&mut self) {
        self.input.pop();
        self.history_index = None;
    }

    /// Clears the input line.
    pub fn clear(&mut self) {
        self.input.clear();
        self.history_index = None;
    }
}

/// History of executed Ex commands with navigation support.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExHistory {
    entries: Vec<String>,
}

impl ExHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an executed command line, deduplicating consecutive/identical entries.
    pub fn record(&mut self, line: String) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }
        self.entries.retain(|entry| entry != trimmed);
        self.entries.push(trimmed.to_string());
    }

    /// Navigates through command history, updating `state.input` and `state.history_index`.
    pub fn navigate(&self, state: &mut ExState, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as isize;
        let index = match state.history_index {
            Some(index) => (index as isize + delta).clamp(0, len - 1) as usize,
            None if delta < 0 => self.entries.len() - 1,
            None => return,
        };
        state.history_index = Some(index);
        state.input = self.entries[index].clone();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_deduplicates_entries() {
        let mut history = ExHistory::new();
        history.record("w".into());
        history.record("q".into());
        history.record("w".into());

        assert_eq!(history.len(), 2);
    }

    #[test]
    fn navigates_through_history() {
        let mut history = ExHistory::new();
        history.record("w".into());
        history.record("q".into());

        let mut state = ExState::new();
        history.navigate(&mut state, -1);
        assert_eq!(state.input, "q");
        assert_eq!(state.history_index, Some(1));

        history.navigate(&mut state, -1);
        assert_eq!(state.input, "w");
        assert_eq!(state.history_index, Some(0));

        // Clamps at the oldest entry
        history.navigate(&mut state, -1);
        assert_eq!(state.input, "w");
        assert_eq!(state.history_index, Some(0));

        // Moves back forward
        history.navigate(&mut state, 1);
        assert_eq!(state.input, "q");
        assert_eq!(state.history_index, Some(1));
    }

    #[test]
    fn appends_text_with_push_str() {
        let mut state = ExState::new();
        state.push_str("edit");
        assert_eq!(state.input, "edit");
        state.push_str(" main.rs");
        assert_eq!(state.input, "edit main.rs");
    }
}
