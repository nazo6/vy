//! Focus inside a widget tree.
//!
//! A pane keeps the ids from the root of its widget tree down to the focused
//! widget. The keymap reads the focused widget's role and id from the same
//! path, so `when` conditions such as `uiRole == 'list'` need no extra state.
//!
//! `Tab` and `Shift-Tab` walk the focusable widgets in depth-first order. When
//! a redraw drops the focused widget, the path falls back to the nearest
//! surviving ancestor, then to the first focusable widget
//! ([ADR 0024](../../../docs/adr/0024-keyboard-first-plugin-ui.md) §3).

use crate::widget::WidgetNode;

/// Ids from the root of a widget tree down to the focused node.
///
/// Nodes without an id are transparent, so a path only holds the ids that can
/// address a widget. An empty path means nothing in the tree has focus. The
/// same type describes the focus of a surface
/// ([ADR 0032](../../../docs/adr/0032-floating-surfaces.md) §2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct FocusPath {
    ids: Vec<String>,
}

impl FocusPath {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a path from the root id to the focused id.
    pub fn from_ids(ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            ids: ids.into_iter().map(Into::into).collect(),
        }
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// The id of the focused node, when something is focused.
    pub fn focused(&self) -> Option<&str> {
        self.ids.last().map(String::as_str)
    }

    /// Appends one id, returning a new path.
    pub fn joined(&self, id: impl Into<String>) -> Self {
        let mut ids = self.ids.clone();
        ids.push(id.into());
        Self { ids }
    }
}

/// Every focusable path in the tree, in depth-first order.
pub fn focusable_paths(root: &WidgetNode) -> Vec<FocusPath> {
    let mut paths = Vec::new();
    let mut ancestors = FocusPath::new();
    walk(root, &mut ancestors, &mut paths);
    paths
}

fn walk(node: &WidgetNode, ancestors: &mut FocusPath, paths: &mut Vec<FocusPath>) {
    let Some(id) = node.id() else {
        for child in node.children() {
            walk(child, ancestors, paths);
        }
        return;
    };

    ancestors.ids.push(id.to_string());
    if node.focus_role().is_some() {
        paths.push(ancestors.clone());
    }
    for child in node.children() {
        walk(child, ancestors, paths);
    }
    ancestors.ids.pop();
}

/// Every node in the tree, in depth-first order.
pub fn walk_widgets(root: &WidgetNode, visit: &mut impl FnMut(&WidgetNode)) {
    visit(root);
    for child in root.children() {
        walk_widgets(child, visit);
    }
}

/// The focusable node named by `path`.
pub fn focused_node<'a>(root: &'a WidgetNode, path: &FocusPath) -> Option<&'a WidgetNode> {
    if path.is_empty() {
        return None;
    }
    let mut ancestors = FocusPath::new();
    find(root, path, &mut ancestors)
}

fn find<'a>(
    node: &'a WidgetNode,
    path: &FocusPath,
    ancestors: &mut FocusPath,
) -> Option<&'a WidgetNode> {
    let Some(id) = node.id() else {
        return node
            .children()
            .iter()
            .find_map(|child| find(child, path, ancestors));
    };

    ancestors.ids.push(id.to_string());
    let found = if node.focus_role().is_some() && ancestors == path {
        Some(node)
    } else {
        node.children()
            .iter()
            .find_map(|child| find(child, path, ancestors))
    };
    ancestors.ids.pop();
    found
}

/// Keeps `path` when it still names a focusable node, otherwise moves it.
///
/// The fallback walks up to the nearest surviving ancestor and takes the first
/// focusable widget below it. When nothing survives, the first focusable widget
/// in the tree wins. An empty path stays empty: a pane with no focus does not
/// gain focus just because a new tree arrived.
pub fn normalize(root: Option<&WidgetNode>, path: &FocusPath) -> FocusPath {
    if path.is_empty() {
        return FocusPath::new();
    }
    let Some(root) = root else {
        return FocusPath::new();
    };

    let paths = focusable_paths(root);
    if paths.iter().any(|candidate| candidate == path) {
        return path.clone();
    }

    for len in (0..path.len()).rev() {
        let prefix = &path.ids[..len];
        if let Some(candidate) = paths
            .iter()
            .find(|candidate| candidate.ids.starts_with(prefix))
        {
            return candidate.clone();
        }
    }

    paths.first().cloned().unwrap_or_default()
}

/// The next (`forward`) or previous focusable path, wrapping at both ends.
pub fn step(root: &WidgetNode, path: &FocusPath, forward: bool) -> FocusPath {
    let paths = focusable_paths(root);
    let Some(len) = (!paths.is_empty()).then_some(paths.len()) else {
        return FocusPath::new();
    };

    let index = match paths.iter().position(|candidate| candidate == path) {
        Some(index) if forward => (index + 1) % len,
        Some(index) => (index + len - 1) % len,
        None if forward => 0,
        None => len - 1,
    };
    paths[index].clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::WidgetNode;

    fn button(id: &str) -> WidgetNode {
        WidgetNode::Button {
            id: id.to_string(),
            label: id.to_string(),
            command: format!("example.{id}"),
            variant: None,
        }
    }

    fn input(id: &str) -> WidgetNode {
        WidgetNode::TextInput {
            id: Some(id.to_string()),
            value: String::new(),
            placeholder: None,
        }
    }

    fn column(children: Vec<WidgetNode>) -> WidgetNode {
        WidgetNode::Container {
            id: None,
            kind: crate::widget::ContainerKind::Column,
            gap: None,
            children,
        }
    }

    fn tree() -> WidgetNode {
        WidgetNode::Container {
            id: Some("panel".to_string()),
            kind: crate::widget::ContainerKind::Column,
            gap: None,
            children: vec![
                button("first"),
                column(vec![input("query"), button("last")]),
            ],
        }
    }

    fn paths() -> Vec<Vec<String>> {
        focusable_paths(&tree())
            .into_iter()
            .map(|path| path.ids().to_vec())
            .collect()
    }

    #[test]
    fn walks_focusable_nodes_depth_first() {
        assert_eq!(
            paths(),
            vec![
                vec!["panel".to_string(), "first".to_string()],
                vec!["panel".to_string(), "query".to_string()],
                vec!["panel".to_string(), "last".to_string()],
            ]
        );
    }

    #[test]
    fn unidentified_widgets_are_not_focusable() {
        let tree = column(vec![
            WidgetNode::TextInput {
                id: None,
                value: String::new(),
                placeholder: None,
            },
            WidgetNode::Text {
                id: None,
                text: "hello".to_string(),
                color: None,
                font: None,
            },
        ]);
        assert!(focusable_paths(&tree).is_empty());
    }

    #[test]
    fn resolves_the_focused_node() {
        let tree = tree();
        let path = FocusPath::from_ids(["panel", "query"]);
        let node = focused_node(&tree, &path).expect("focused node");
        assert_eq!(node.id(), Some("query"));
        assert_eq!(node.focus_role(), Some(crate::widget::UiRole::TextInput));

        assert!(focused_node(&tree, &FocusPath::from_ids(["panel", "missing"])).is_none());
        assert!(focused_node(&tree, &FocusPath::new()).is_none());
    }

    #[test]
    fn steps_and_wraps_in_both_directions() {
        let tree = tree();
        let first = FocusPath::from_ids(["panel", "first"]);
        let last = FocusPath::from_ids(["panel", "last"]);

        assert_eq!(
            step(&tree, &first, true),
            FocusPath::from_ids(["panel", "query"])
        );
        assert_eq!(step(&tree, &first, false), last.clone());
        assert_eq!(step(&tree, &last, true), first.clone());
        // An unknown path starts from either end.
        assert_eq!(
            step(&tree, &FocusPath::from_ids(["panel", "gone"]), true),
            first
        );
        assert_eq!(step(&tree, &FocusPath::new(), true), first);
    }

    #[test]
    fn keeps_a_live_path_and_falls_back_when_it_disappears() {
        let tree = tree();
        let live = FocusPath::from_ids(["panel", "query"]);
        assert_eq!(normalize(Some(&tree), &live), live);

        // The focused widget is gone; its parent keeps the first focusable child.
        let gone = FocusPath::from_ids(["panel", "removed"]);
        assert_eq!(
            normalize(Some(&tree), &gone),
            FocusPath::from_ids(["panel", "first"])
        );

        // The whole subtree is gone; the first focusable widget wins.
        let deeper = FocusPath::from_ids(["panel", "removed", "child"]);
        assert_eq!(
            normalize(Some(&tree), &deeper),
            FocusPath::from_ids(["panel", "first"])
        );

        // An empty path stays empty, and a missing tree clears it.
        assert_eq!(normalize(Some(&tree), &FocusPath::new()), FocusPath::new());
        assert_eq!(normalize(None, &live), FocusPath::new());

        let empty = column(vec![]);
        assert_eq!(normalize(Some(&empty), &live), FocusPath::new());
    }

    #[test]
    fn a_tree_without_focusable_nodes_has_no_focus() {
        let tree = column(vec![]);
        assert!(focusable_paths(&tree).is_empty());
        assert_eq!(step(&tree, &FocusPath::new(), true), FocusPath::new());
    }
}
