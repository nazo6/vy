//! State and item filtering for the command palette.

use serde_json::{Map, Value};

use crate::commands::{ArgSpec, CommandDescriptor};

pub use vy_client_api::palette::PickerItem;

impl From<&CommandDescriptor> for PickerItem {
    fn from(command: &CommandDescriptor) -> Self {
        Self {
            id: command.id.clone(),
            title: command.title.clone(),
            description: Some(command.id.clone()),
            detail: Some(command.source.label().to_string()),
        }
    }
}

/// The mode/kind of picker currently active.
#[derive(Debug, Default)]
pub enum PaletteMode {
    /// Command palette for searching and executing commands.
    #[default]
    Commands,
    /// Document / buffer picker for switching open documents.
    Documents,
    /// Collects one required argument of a command after the palette selection.
    Arguments {
        command_id: String,
        title: String,
        spec: ArgSpec,
        values: Map<String, Value>,
        /// Index into the required arguments of `spec`.
        index: usize,
    },
    /// Custom picker triggered by a script or plugin via `ui.picker.show`.
    Custom {
        id: String,
        items: Vec<PickerItem>,
        placeholder: Option<String>,
        reply: Option<vy_js::HostRequest>,
    },
    /// Capability approval prompt for an external plugin.
    Approval {
        plugin_id: String,
        plugin_name: String,
        plugin_version: String,
        missing: Vec<String>,
    },
}

/// Maximum width the command palette grows before it stops filling the window.
pub const PALETTE_MAX_WIDTH: f32 = 560.0;
/// Maximum height the command palette grows before the row list is clipped.
pub const PALETTE_MAX_HEIGHT: f32 = 420.0;

/// Generates picker items for a plugin capability approval prompt.
pub fn approval_picker_items(
    plugin_id: &str,
    plugin_name: &str,
    missing: &[String],
) -> Vec<PickerItem> {
    let mut items = vec![
        PickerItem {
            id: "trust".into(),
            title: "Trust and Enable".into(),
            description: Some(format!(
                "Grant {} permission(s) and enable {}",
                missing.len(),
                plugin_name
            )),
            detail: Some("Action".into()),
        },
        PickerItem {
            id: "deny".into(),
            title: "Deny".into(),
            description: Some(format!("Keep {} disabled", plugin_name)),
            detail: Some("Action".into()),
        },
    ];

    for cap in missing {
        items.push(PickerItem {
            id: format!("cap:{cap}"),
            title: cap.clone(),
            description: Some(format!("Requested by {plugin_id}")),
            detail: Some("Permission".into()),
        });
    }

    items
}

/// State of an active command palette modal.
#[derive(Debug, Default)]
pub struct PaletteState {
    pub mode: PaletteMode,
    pub query: String,
    pub preedit: String,
    pub selected: usize,
}

impl PaletteState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mode: PaletteMode) -> Self {
        Self {
            mode,
            query: String::new(),
            preedit: String::new(),
            selected: 0,
        }
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

    /// Appends committed text to the query.
    pub fn push_str(&mut self, text: &str) {
        self.query.push_str(text);
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
    let picker_items: Vec<PickerItem> = items.iter().map(PickerItem::from).collect();
    filtered_picker_indices(&picker_items, query)
}

/// Indices of generic picker items matching every whitespace-separated word in `query`.
pub fn filtered_picker_indices(items: &[PickerItem], query: &str) -> Vec<usize> {
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
            let mut haystack = format!("{} {}", item.title, item.id);
            if let Some(desc) = &item.description {
                haystack.push(' ');
                haystack.push_str(desc);
            }
            if let Some(detail) = &item.detail {
                haystack.push(' ');
                haystack.push_str(detail);
            }
            let haystack = haystack.to_lowercase();
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
            category: crate::commands::CommandCategory::Edit,
            palette: crate::commands::PaletteVisibility::Listed,
            ex_names: Vec::new(),
            args: crate::commands::ArgSpec::None,
            range: crate::commands::RangeSpec::None,
            bang: false,
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
    fn appends_text_with_push_str() {
        let mut palette = PaletteState::new();
        palette.push_str("file");
        assert_eq!(palette.query, "file");
        palette.push_str(" open");
        assert_eq!(palette.query, "file open");
    }

    #[test]
    fn wraps_the_palette_selection() {
        assert_eq!(move_palette_selection(0, -1, 3), 2);
        assert_eq!(move_palette_selection(2, 1, 3), 0);
        assert_eq!(move_palette_selection(1, 1, 3), 2);
        assert_eq!(move_palette_selection(0, 1, 0), 0);
    }
}
