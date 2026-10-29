//! Selection, cursor, and expansion state for list, tree, and table widgets.
//!
//! The host owns this state, not the plugin, so `vy.ui.*` can act on it without
//! a round trip through the script thread
//! ([ADR 0024](../../../docs/adr/0024-keyboard-first-plugin-ui.md) §4, §5).

use std::collections::BTreeSet;

use crate::widget::{TreeNode, WidgetNode};

/// Where the cursor sits and which rows are marked.
///
/// `cursor` is the focused row. `selected` holds the marked ids: a single
/// selection keeps one id, a multiple selection keeps every marked row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListState {
    cursor: Option<String>,
    selected: BTreeSet<String>,
    expanded: BTreeSet<String>,
}

impl ListState {
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty state usable in a `static`.
    pub const fn empty() -> Self {
        Self {
            cursor: None,
            selected: BTreeSet::new(),
            expanded: BTreeSet::new(),
        }
    }

    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    pub fn selected(&self) -> impl Iterator<Item = &String> {
        self.selected.iter()
    }

    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub fn is_selected(&self, id: &str) -> bool {
        self.selected.contains(id)
    }

    pub fn is_expanded(&self, id: &str) -> bool {
        self.expanded.contains(id)
    }

    pub fn expanded_ids(&self) -> Vec<String> {
        self.expanded.iter().cloned().collect()
    }

    /// Sets the cursor and selects it. Multiple selection is cleared.
    pub fn select_single(&mut self, id: impl Into<String>) {
        let id = id.into();
        self.selected.clear();
        self.selected.insert(id.clone());
        self.cursor = Some(id);
    }

    /// Moves the cursor.
    ///
    /// A single-selection widget moves its selection with the cursor. A
    /// multiple-selection widget leaves the marks alone: moving is not marking,
    /// so `x` decides which rows are marked.
    pub fn move_cursor(&mut self, id: impl Into<String>, multiple: bool) {
        let id = id.into();
        if multiple {
            self.cursor = Some(id);
            return;
        }
        self.select_single(id);
    }

    /// Marks or unmarks `id`, keeping the other marks.
    pub fn toggle(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.selected.remove(&id) {
            self.selected.insert(id.clone());
        }
        self.cursor = Some(id);
    }

    /// Expands or collapses a tree node.
    pub fn toggle_expanded(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }

    /// Clears marks, the cursor, and expansion. Used when the tree changes shape.
    pub fn reset(&mut self) {
        self.cursor = None;
        self.selected.clear();
        self.expanded.clear();
    }

    /// Drops ids that no longer exist and keeps the cursor on a live row.
    ///
    /// A redraw replaces the widget data, so the remembered ids may point at
    /// rows that are gone. Missing ids are dropped; the cursor moves to the
    /// first row when its own id disappeared.
    pub fn normalize(&mut self, rows: &[String]) {
        self.selected.retain(|id| rows.iter().any(|row| row == id));
        self.expanded.retain(|id| rows.iter().any(|row| row == id));
        match &self.cursor {
            Some(cursor) if rows.iter().any(|row| row == cursor) => {}
            _ => {
                self.cursor = rows.first().cloned();
                if self.cursor.is_some() && self.selected.is_empty() {
                    self.selected.extend(self.cursor.clone());
                }
            }
        }
    }
}

/// One visible row of a list, table, or tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    /// Depth in a tree. Lists and tables always report zero.
    pub depth: usize,
    /// Whether this tree row has children.
    pub has_children: bool,
    /// Whether this tree row has children hidden by a collapse.
    pub collapsed: bool,
}

impl Row {
    fn new(id: &str, label: &str, description: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            description: description.map(str::to_string),
            depth: 0,
            has_children: false,
            collapsed: false,
        }
    }
}

/// Rows of a list or table, in display order.
pub fn list_rows(node: &WidgetNode) -> Vec<Row> {
    match node {
        WidgetNode::List { items, .. } | WidgetNode::Table { items, .. } => items
            .iter()
            .map(|item| Row::new(&item.id, &item.label, item.description.as_deref()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Every tree row, in depth-first order, ignoring expansion.
pub fn tree_rows(nodes: &[TreeNode]) -> Vec<Row> {
    fn walk(nodes: &[TreeNode], depth: usize, rows: &mut Vec<Row>) {
        for node in nodes {
            let mut row = Row::new(&node.id, &node.label, None);
            row.depth = depth;
            rows.push(row);
            walk(&node.children, depth + 1, rows);
        }
    }
    let mut rows = Vec::new();
    walk(nodes, 0, &mut rows);
    rows
}

/// The rows a user can see: collapsed subtrees are skipped.
///
/// A node that sets `expanded` owns its own expansion, which is what a lazily
/// loaded directory needs: the plugin decides whether children are shown, and
/// the host only keeps state for nodes that leave it unset.
pub fn visible_tree_rows(nodes: &[TreeNode], expanded: &BTreeSet<String>) -> Vec<Row> {
    fn walk(nodes: &[TreeNode], depth: usize, expanded: &BTreeSet<String>, rows: &mut Vec<Row>) {
        for node in nodes {
            let has_children = !node.children.is_empty();
            let is_expanded = node.expanded.unwrap_or_else(|| expanded.contains(&node.id));
            let mut row = Row::new(&node.id, &node.label, None);
            row.depth = depth;
            row.has_children = has_children;
            row.collapsed = has_children && !is_expanded;
            rows.push(row);
            // Children are listed only when the node is expanded. A collapsed
            // node hides them even if they are present in the data.
            if is_expanded {
                walk(&node.children, depth + 1, expanded, rows);
            }
        }
    }
    let mut rows = Vec::new();
    walk(nodes, 0, expanded, &mut rows);
    rows
}

/// Rows a widget shows, or an empty list for other nodes.
pub fn rows(node: &WidgetNode, state: &ListState) -> Vec<Row> {
    match node {
        WidgetNode::Tree { nodes, .. } => visible_tree_rows(nodes, &state.expanded),
        _ => list_rows(node),
    }
}

/// Position of the cursor among the rows the widget shows.
///
/// A scroll container addresses its children by this index, so the view uses it
/// to reveal the cursor row. A cursor left on a hidden tree row has no position.
pub fn cursor_row_index(node: &WidgetNode, state: &ListState) -> Option<usize> {
    let cursor = state.cursor()?;
    rows(node, state).iter().position(|row| row.id == cursor)
}

/// Every id the widget can address, including hidden tree rows.
pub fn all_ids(node: &WidgetNode) -> Vec<String> {
    match node {
        WidgetNode::Tree { nodes, .. } => tree_rows(nodes).into_iter().map(|row| row.id).collect(),
        _ => list_rows(node).into_iter().map(|row| row.id).collect(),
    }
}

/// The label or text to copy for `id`.
pub fn yank_text(node: &WidgetNode, id: &str) -> Option<String> {
    match node {
        WidgetNode::List { items, .. } | WidgetNode::Table { items, .. } => items
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.yank_text().to_string()),
        WidgetNode::Tree { nodes, .. } => {
            fn find(nodes: &[TreeNode], id: &str) -> Option<String> {
                for node in nodes {
                    if node.id == id {
                        return Some(node.yank_text().to_string());
                    }
                    if let Some(text) = find(&node.children, id) {
                        return Some(text);
                    }
                }
                None
            }
            find(nodes, id)
        }
        _ => None,
    }
}

/// Whether a tree row is open, reading the node's own flag when it has one.
pub fn is_expanded(node: &WidgetNode, id: &str, state: &ListState) -> bool {
    fn find(nodes: &[TreeNode], id: &str) -> Option<bool> {
        for node in nodes {
            if node.id == id {
                return Some(node.expanded.unwrap_or(false));
            }
            if let Some(found) = find(&node.children, id) {
                return Some(found);
            }
        }
        None
    }

    match node {
        WidgetNode::Tree { nodes, .. } => find(nodes, id).unwrap_or_else(|| state.is_expanded(id)),
        _ => state.is_expanded(id),
    }
}

/// The command `vy.ui.activate` runs for `id`.
///
/// A row without a command is still selectable; activation then reports the id
/// instead of running anything.
pub fn command_for(node: &WidgetNode, id: &str) -> Option<String> {
    match node {
        WidgetNode::List { items, .. } | WidgetNode::Table { items, .. } => items
            .iter()
            .find(|item| item.id == id)
            .and_then(|item| item.command.clone()),
        WidgetNode::Tree { nodes, .. } => {
            fn find(nodes: &[TreeNode], id: &str) -> Option<String> {
                for node in nodes {
                    if node.id == id {
                        return node.command.clone();
                    }
                    if let Some(command) = find(&node.children, id) {
                        return Some(command);
                    }
                }
                None
            }
            find(nodes, id)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{ListItem, TreeNode, WidgetNode};

    fn list() -> WidgetNode {
        WidgetNode::List {
            id: "results".to_string(),
            items: vec![
                ListItem::new("a", "Alpha"),
                ListItem::new("b", "Beta"),
                ListItem::new("c", "Gamma"),
            ],
            multiple: false,
        }
    }

    fn tree() -> WidgetNode {
        let nodes = vec![
            TreeNode::new("src", "src").with_children(vec![
                TreeNode::new("main", "main.ts"),
                TreeNode::new("lib", "lib.ts"),
            ]),
            TreeNode::new("readme", "README.md"),
        ];
        WidgetNode::Tree {
            id: "files".to_string(),
            nodes,
            multiple: false,
        }
    }

    #[test]
    fn selects_rows_of_a_list() {
        let node = list();
        let mut state = ListState::new();
        state.normalize(&all_ids(&node));

        // The cursor starts on the first row, which is also selected.
        assert_eq!(state.cursor(), Some("a"));
        assert_eq!(state.selected_count(), 1);

        state.select_single("b");
        assert_eq!(state.cursor(), Some("b"));
        assert!(state.is_selected("b"));
        assert!(!state.is_selected("a"));

        state.toggle("c");
        assert!(state.is_selected("b"));
        assert!(state.is_selected("c"));
        assert_eq!(state.selected_count(), 2);

        state.toggle("c");
        assert!(!state.is_selected("c"));
        assert_eq!(state.selected_count(), 1);
    }

    #[test]
    fn a_tree_starts_collapsed_and_expands_on_toggle() {
        let node = tree();
        let state = ListState::new();
        let visible = rows(&node, &state);
        assert_eq!(
            visible
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["src", "readme"]
        );
        assert!(visible[0].collapsed);
        assert!(!visible[1].collapsed);

        let mut state = ListState::new();
        state.toggle_expanded("src");
        let visible = rows(&node, &state);
        assert_eq!(
            visible
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["src", "main", "lib", "readme"]
        );
        assert_eq!(visible[1].depth, 1);
        assert!(!visible[0].collapsed);

        state.toggle_expanded("src");
        assert!(!state.is_expanded("src"));
    }

    #[test]
    fn normalize_drops_stale_ids_and_moves_the_cursor() {
        let mut state = ListState::new();
        state.normalize(&["a".to_string(), "b".to_string()]);
        state.select_single("b");
        state.toggle("a");
        state.toggle_expanded("b");

        // "b" is gone and "a" survives.
        state.normalize(&["a".to_string(), "c".to_string()]);
        assert!(state.is_selected("a"));
        assert!(!state.is_selected("b"));
        assert_eq!(state.cursor(), Some("a"));
        assert!(!state.is_expanded("b"));

        // Nothing survives; the state resets to the first row.
        state.normalize(&["z".to_string()]);
        assert_eq!(state.cursor(), Some("z"));
        assert_eq!(state.selected_count(), 1);
    }

    #[test]
    fn a_redraw_keeps_a_live_cursor() {
        let mut state = ListState::new();
        state.normalize(&["a".to_string(), "b".to_string()]);
        state.select_single("b");
        state.normalize(&["a".to_string(), "b".to_string()]);
        assert_eq!(state.cursor(), Some("b"));
    }

    #[test]
    fn yanks_the_text_then_the_label() {
        let node = WidgetNode::List {
            id: "results".to_string(),
            items: vec![
                ListItem::new("a", "Alpha"),
                ListItem {
                    id: "b".to_string(),
                    label: "Beta".to_string(),
                    description: None,
                    text: Some("beta-line".to_string()),
                    command: None,
                },
            ],
            multiple: false,
        };
        assert_eq!(yank_text(&node, "a").as_deref(), Some("Alpha"));
        assert_eq!(yank_text(&node, "b").as_deref(), Some("beta-line"));
        assert_eq!(yank_text(&node, "missing"), None);
    }

    #[test]
    fn yanks_a_tree_node_anywhere_in_the_tree() {
        let node = tree();
        assert_eq!(yank_text(&node, "lib").as_deref(), Some("lib.ts"));
        assert_eq!(yank_text(&node, "nope"), None);
    }

    #[test]
    fn all_ids_include_collapsed_tree_rows() {
        let node = tree();
        assert_eq!(
            all_ids(&node),
            vec!["src", "main", "lib", "readme"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_cursor_row_follows_the_visible_rows() {
        let node = tree();
        let mut state = ListState::new();

        // A collapsed tree hides the children of "src".
        state.select_single("src");
        assert_eq!(cursor_row_index(&node, &state), Some(0));
        state.select_single("readme");
        assert_eq!(cursor_row_index(&node, &state), Some(1));

        // Expanding moves the rows below it down.
        state.toggle_expanded("src");
        assert_eq!(cursor_row_index(&node, &state), Some(3));
        state.select_single("lib");
        assert_eq!(cursor_row_index(&node, &state), Some(2));

        // A cursor on a row the widget no longer shows has no position.
        state.select_single("gone");
        assert_eq!(cursor_row_index(&node, &state), None);
    }

    #[test]
    fn a_table_is_a_list_of_rows() {
        let node = WidgetNode::Table {
            id: "rows".to_string(),
            columns: vec!["name".to_string()],
            items: vec![ListItem::new("a", "Alpha")],
            multiple: true,
        };
        assert!(node.multiple());
        let state = ListState::new();
        let listed = rows(&node, &state);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "a");
    }
}
