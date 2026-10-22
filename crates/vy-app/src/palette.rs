//! State and item filtering for the command palette.

use crate::commands::CommandDescriptor;

/// Maximum width the command palette grows before it stops filling the window.
pub const PALETTE_MAX_WIDTH: f32 = 560.0;
/// Maximum height the command palette grows before the row list is clipped.
pub const PALETTE_MAX_HEIGHT: f32 = 420.0;

/// State of an active command palette modal.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PaletteState {
    pub query: String,
    pub selected: usize,
}

impl PaletteState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the selection index by `delta`, wrapping around if out of bounds.
    pub fn move_selection(&mut self, delta: isize, total: usize) {
        self.selected = move_palette_selection(self.selected, delta, total);
    }

    /// Resets the selection to the top item.
    pub fn reset_selection(&mut self) {
        self.selected = 0;
    }

    /// Appends a typed character to the query.
    pub fn push_char(&mut self, character: char) {
        self.query.push(character);
        self.reset_selection();
    }

    /// Removes the last character from the query.
    pub fn pop_char(&mut self) {
        self.query.pop();
        self.reset_selection();
    }
}

/// Indices of palette rows matching every whitespace-separated word in `query`.
pub fn filtered_palette_indices(items: &[CommandDescriptor], query: &str) -> Vec<usize> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return (0..items.len()).collect();
    }

    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let haystack = format!("{} {}", item.title, item.id).to_lowercase();
            tokens.iter().all(|token| haystack.contains(token))
        })
        .map(|(index, _)| index)
        .collect()
}

/// Moves around the palette list, wrapping at both ends.
pub fn move_palette_selection(index: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + delta).rem_euclid(len as isize) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette_item(id: &str, title: &str) -> CommandDescriptor {
        CommandDescriptor {
            id: id.to_string(),
            title: title.to_string(),
            source: crate::commands::CommandSource::Builtin,
            aliases: Vec::new(),
            hidden: false,
            accepts_args: false,
            accepts_range: false,
            accepts_bang: false,
        }
    }

    #[test]
    fn filters_the_palette_by_every_word() {
        let items = vec![
            palette_item("vy.pane.split_right", "Pane Split Right"),
            palette_item("vy.pane.split_down", "Pane Split Down"),
            palette_item("vy.edit.delete", "Edit Delete"),
        ];

        assert_eq!(filtered_palette_indices(&items, ""), vec![0, 1, 2]);
        assert_eq!(filtered_palette_indices(&items, "split"), vec![0, 1]);
        assert_eq!(filtered_palette_indices(&items, "pane down"), vec![1]);
        assert_eq!(filtered_palette_indices(&items, "edit"), vec![2]);
        assert!(
            filtered_palette_indices(&items, "missing").is_empty(),
            "unexpected matches"
        );
    }

    #[test]
    fn wraps_the_palette_selection() {
        assert_eq!(move_palette_selection(0, -1, 3), 2);
        assert_eq!(move_palette_selection(2, 1, 3), 0);
        assert_eq!(move_palette_selection(1, 1, 3), 2);
        assert_eq!(move_palette_selection(0, 1, 0), 0);
    }
}
