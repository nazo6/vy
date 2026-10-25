//! State and item filtering for the command palette.

use crate::commands::CommandDescriptor;
use crate::overlay::TextInput;

pub use vy_client_api::palette::PickerItem;

/// Maximum number of recently used commands the palette remembers per session.
pub const RECENT_COMMAND_LIMIT: usize = 5;

/// Heading used for the recently used commands.
pub const RECENT_GROUP: &str = "Recent";

impl From<&CommandDescriptor> for PickerItem {
    fn from(command: &CommandDescriptor) -> Self {
        let detail = match command.ex_names.first() {
            Some(name) => format!(":{name}"),
            None => command.source.label().to_string(),
        };
        Self {
            id: command.id.clone(),
            title: command.title.clone(),
            description: Some(command.id.clone()),
            detail: Some(detail),
            group: Some(command.category.label().to_string()),
        }
    }
}

/// Records `id` as the most recently used command, keeping at most
/// [`RECENT_COMMAND_LIMIT`] entries.
pub fn push_recent_command(recent: &mut Vec<String>, id: &str) {
    recent.retain(|entry| entry != id);
    recent.insert(0, id.to_string());
    recent.truncate(RECENT_COMMAND_LIMIT);
}

/// Headings are shown while browsing; a search result is a ranked flat list.
pub fn shows_palette_headings(query: &str) -> bool {
    query.trim().is_empty()
}

/// The mode/kind of picker currently active.
#[derive(Debug, Default)]
pub enum PaletteMode {
    /// Command palette for searching and executing commands.
    #[default]
    Commands,
    /// Document / buffer picker for switching open documents.
    Documents,
    /// Custom picker triggered by a script or plugin via `ui.picker.show`.
    Custom {
        id: String,
        items: Vec<PickerItem>,
        placeholder: Option<String>,
        reply: Option<vy_js::HostRequest>,
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
        PickerItem::new("trust", "Trust and Enable")
            .with_description(format!(
                "Grant {} permission(s) and enable {}",
                missing.len(),
                plugin_name
            ))
            .with_group("Action"),
        PickerItem::new("deny", "Deny")
            .with_description(format!("Keep {} disabled", plugin_name))
            .with_group("Action"),
    ];

    for cap in missing {
        items.push(
            PickerItem::new(format!("cap:{cap}"), cap.clone())
                .with_description(format!("Requested by {plugin_id}"))
                .with_group("Permission"),
        );
    }

    items
}

/// State of an active command palette modal.
#[derive(Debug, Default)]
pub struct PaletteState {
    pub mode: PaletteMode,
    pub input: TextInput,
    pub selected: usize,
}

impl PaletteState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mode: PaletteMode) -> Self {
        Self {
            mode,
            input: TextInput::default(),
            selected: 0,
        }
    }

    /// Text drawn before the palette input.
    pub fn prefix(&self) -> String {
        match &self.mode {
            PaletteMode::Custom {
                placeholder: Some(placeholder),
                ..
            } => format!("{placeholder} > "),
            _ => "> ".to_string(),
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
        self.input.push_char(character);
        self.reset_selection();
    }

    /// Appends committed text to the query.
    pub fn push_str(&mut self, text: &str) {
        self.input.push_str(text);
        self.reset_selection();
    }

    /// Removes the last character from the query.
    pub fn pop_char(&mut self) {
        self.input.pop_char();
        self.reset_selection();
    }
}

/// Indices of palette rows matching every whitespace-separated word in `query`.
pub fn filtered_palette_indices(items: &[CommandDescriptor], query: &str) -> Vec<usize> {
    let picker_items: Vec<PickerItem> = items.iter().map(PickerItem::from).collect();
    filtered_picker_indices(&picker_items, query)
}

/// How well an item matched the query. Lower ranks sort first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchRank {
    /// The query equals the title, the id, or an Ex name.
    Exact,
    /// Every query word starts one of the item's words.
    WordPrefix,
    /// Every query word appears somewhere in the item.
    Partial,
}

/// Indices of generic picker items matching every whitespace-separated word in
/// `query`, best matches first. Ties keep category and title order.
pub fn filtered_picker_indices(items: &[PickerItem], query: &str) -> Vec<usize> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return (0..items.len()).collect();
    }

    let mut matches: Vec<(usize, MatchRank)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match_rank(item, &tokens).map(|rank| (index, rank)))
        .collect();
    matches.sort_by(|(left, left_rank), (right, right_rank)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| sort_key(&items[*left]).cmp(&sort_key(&items[*right])))
            .then_with(|| left.cmp(right))
    });
    matches.into_iter().map(|(index, _)| index).collect()
}

/// Fields searched for a query, without the group heading.
fn search_fields(item: &PickerItem) -> Vec<String> {
    let mut fields = vec![item.title.clone(), item.id.clone()];
    if let Some(description) = &item.description {
        fields.push(description.clone());
    }
    if let Some(detail) = &item.detail {
        fields.push(detail.trim_start_matches(':').to_string());
    }
    fields
}

fn sort_key(item: &PickerItem) -> (String, String) {
    (
        item.group.as_deref().unwrap_or("").to_lowercase(),
        item.title.to_lowercase(),
    )
}

/// Sorts items by group heading, then title.
pub fn sort_picker_items(items: &mut [PickerItem]) {
    items.sort_by_key(sort_key);
}

/// Rank of one item for the whole query, or `None` when it does not match.
fn match_rank(item: &PickerItem, tokens: &[String]) -> Option<MatchRank> {
    let fields = search_fields(item);
    let haystack = fields.join(" ").to_lowercase();
    if !tokens.iter().all(|token| haystack.contains(token.as_str())) {
        return None;
    }

    if tokens.len() == 1
        && fields
            .iter()
            .any(|field| field.trim().to_lowercase() == tokens[0])
    {
        return Some(MatchRank::Exact);
    }

    let words: Vec<&str> = haystack
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let every_word_matches = tokens
        .iter()
        .all(|token| words.iter().any(|word| word.starts_with(token.as_str())));
    Some(if every_word_matches {
        MatchRank::WordPrefix
    } else {
        MatchRank::Partial
    })
}

/// One line of the rendered palette: a group heading or an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteRow {
    Heading(String),
    /// Index into the unfiltered item list.
    Item(usize),
}

/// Rows for the palette, inserting a heading whenever the group changes.
pub fn palette_rows(items: &[PickerItem], filtered: &[usize], headings: bool) -> Vec<PaletteRow> {
    let mut rows = Vec::with_capacity(filtered.len());
    let mut current: Option<&str> = None;
    for index in filtered {
        let Some(item) = items.get(*index) else {
            continue;
        };
        if headings
            && let Some(group) = item.group.as_deref()
            && current != Some(group)
        {
            rows.push(PaletteRow::Heading(group.to_string()));
            current = Some(group);
        }
        rows.push(PaletteRow::Item(*index));
    }
    rows
}

/// Row index of the selected item, accounting for the headings above it.
pub fn palette_row_index(
    items: &[PickerItem],
    filtered: &[usize],
    headings: bool,
    selected: usize,
) -> usize {
    let Some(target) = filtered.get(selected) else {
        return 0;
    };
    palette_rows(items, filtered, headings)
        .iter()
        .position(|row| matches!(row, PaletteRow::Item(index) if index == target))
        .unwrap_or(0)
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
        // Equal ranks are ordered by category, then title.
        assert_eq!(filtered_palette_indices(&items, "split"), vec![1, 0]);
        assert_eq!(filtered_palette_indices(&items, "pane down"), vec![1]);
        assert_eq!(filtered_palette_indices(&items, "edit"), vec![2]);
        assert!(
            filtered_palette_indices(&items, "missing").is_empty(),
            "unexpected matches"
        );
    }

    #[test]
    fn ranks_exact_then_word_prefix_then_partial_matches() {
        let items = vec![
            PickerItem::new("vy.view.toggle_wrap", "View Toggle Wrap")
                .with_detail(":wrap")
                .with_group("View"),
            PickerItem::new("vy.view.scroll_left", "View Scroll Left").with_group("View"),
            PickerItem::new("vy.app.reconnect", "App Reconnect Unwrap").with_group("App"),
        ];

        // `:wrap` is an exact Ex name, so the command comes first even though
        // the third item also contains "wrap".
        assert_eq!(filtered_picker_indices(&items, "wrap"), vec![0, 2]);
        // "Toggle Wra" is only a word prefix.
        assert_eq!(filtered_picker_indices(&items, "toggle wra"), vec![0]);
        assert_eq!(filtered_picker_indices(&items, "togg"), vec![0]);
    }

    #[test]
    fn groups_items_under_headings() {
        let items = vec![
            PickerItem::new("a", "App One").with_group("App"),
            PickerItem::new("b", "File One").with_group("File"),
            PickerItem::new("c", "File Two").with_group("File"),
        ];
        let filtered = vec![0, 1, 2];

        assert_eq!(
            palette_rows(&items, &filtered, true),
            vec![
                PaletteRow::Heading("App".into()),
                PaletteRow::Item(0),
                PaletteRow::Heading("File".into()),
                PaletteRow::Item(1),
                PaletteRow::Item(2),
            ]
        );
        assert_eq!(
            palette_rows(&items, &filtered, false),
            vec![
                PaletteRow::Item(0),
                PaletteRow::Item(1),
                PaletteRow::Item(2)
            ]
        );
        assert_eq!(palette_row_index(&items, &filtered, true, 0), 1);
        assert_eq!(palette_row_index(&items, &filtered, true, 2), 4);
    }

    #[test]
    fn keeps_recent_commands_unique_and_bounded() {
        let mut recent = Vec::new();
        for id in ["a", "b", "c", "d", "e", "f"] {
            push_recent_command(&mut recent, id);
        }
        assert_eq!(recent, vec!["f", "e", "d", "c", "b"]);

        push_recent_command(&mut recent, "d");
        assert_eq!(recent, vec!["d", "f", "e", "c", "b"]);
    }

    #[test]
    fn picker_items_show_ex_names_and_categories() {
        let mut command = palette_item("vy.document.save", "Document Save");
        command.ex_names = vec!["w".into(), "write".into()];
        command.category = crate::commands::CommandCategory::File;

        let item = PickerItem::from(&command);
        assert_eq!(item.detail.as_deref(), Some(":w"));
        assert_eq!(item.group.as_deref(), Some("File"));

        let item = PickerItem::from(&palette_item("user.run", "User Run"));
        assert_eq!(item.detail.as_deref(), Some("built-in"));
        assert_eq!(item.group.as_deref(), Some("Edit"));
    }

    #[test]
    fn appends_text_with_push_str() {
        let mut palette = PaletteState::new();
        palette.push_str("file");
        assert_eq!(palette.input.value(), "file");
        palette.push_str(" open");
        assert_eq!(palette.input.value(), "file open");
    }

    #[test]
    fn wraps_the_palette_selection() {
        assert_eq!(move_palette_selection(0, -1, 3), 2);
        assert_eq!(move_palette_selection(2, 1, 3), 0);
        assert_eq!(move_palette_selection(1, 1, 3), 2);
        assert_eq!(move_palette_selection(0, 1, 0), 0);
    }
}
