//! Keyboard focus inside one pane's widget tree (ADR 0024 §3,
//! ADR 0032 Phase 1).
//!
//! A [`FocusPath`] is the ids from the widget-tree root down to the
//! focused widget. Only `button` and `textInput` nodes are focusable in
//! the current widget tree; containers, text, and separators never hold
//! focus, and a `textInput` without an id cannot be addressed by a path,
//! so it is skipped by Tab navigation until it gets an id.
//! This module lives in the library crate (unlike `focus.rs`, which is
//! the binary crate's GPUI focus stack), so PaneState can use it.

use crate::widget::WidgetNode;

/// Keyboard focus inside one pane, as ids from the tree root to the
/// focused widget. An empty path means no widget has focus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FocusPath {
    ids: Vec<String>,
}

/// One focusable widget, with its path and its `when` role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusableNode {
    pub path: FocusPath,
    pub role: &'static str,
}

impl FocusPath {
    pub fn new(ids: Vec<String>) -> Self {
        Self { ids }
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn last(&self) -> Option<&String> {
        self.ids.last()
    }

    /// Focusable widgets of `tree` in depth-first order, with their paths.
    /// Container ids are kept in the path when present; a container
    /// without an id contributes nothing to the path.
    pub fn focusable(tree: &WidgetNode) -> Vec<FocusableNode> {
        let mut out = Vec::new();
        Self::collect(tree, &mut Vec::new(), &mut out);
        out
    }

    fn collect(node: &WidgetNode, ancestors: &mut Vec<String>, out: &mut Vec<FocusableNode>) {
        match node {
            WidgetNode::Container { id, children, .. } => {
                let pushed = id.is_some();
                if let Some(id) = id {
                    ancestors.push(id.clone());
                }
                for child in children {
                    Self::collect(child, ancestors, out);
                }
                if pushed {
                    ancestors.pop();
                }
            }
            WidgetNode::Button { id, .. } => {
                let mut path = ancestors.clone();
                path.push(id.clone());
                out.push(FocusableNode {
                    path: FocusPath::new(path),
                    role: "button",
                });
            }
            WidgetNode::TextInput { id, .. } => {
                if let Some(id) = id {
                    let mut path = ancestors.clone();
                    path.push(id.clone());
                    out.push(FocusableNode {
                        path: FocusPath::new(path),
                        role: "textInput",
                    });
                }
            }
            WidgetNode::Text { .. } | WidgetNode::Separator { .. } => {}
        }
    }

    /// Role of the focused widget (`button` or `textInput`), if it still
    /// exists in `tree`.
    pub fn focused_role(&self, tree: &WidgetNode) -> Option<&'static str> {
        Self::focusable(tree)
            .into_iter()
            .find(|node| node.path == *self)
            .map(|node| node.role)
    }

    /// Resolve this path against a new tree. An exact match is kept.
    /// Otherwise the first focusable widget under the longest surviving
    /// ancestor prefix wins; with no surviving ancestor, the first
    /// focusable widget wins. An empty path resolves to the first
    /// focusable widget, so a newly rendered tree starts with focus.
    /// With no focusable widget at all, the result is empty.
    pub fn resolve(&self, tree: &WidgetNode) -> FocusPath {
        let list = Self::focusable(tree);
        if list.is_empty() {
            return FocusPath::default();
        }
        if list.iter().any(|node| node.path == *self) {
            return self.clone();
        }
        for prefix_len in (0..self.ids.len()).rev() {
            if let Some(node) = list.iter().find(|node| {
                node.path.ids.len() > prefix_len
                    && node.path.ids[..prefix_len] == self.ids[..prefix_len]
            }) {
                return node.path.clone();
            }
        }
        list.into_iter()
            .next()
            .map(|node| node.path)
            .unwrap_or_default()
    }

    /// Next focusable widget in depth-first order, wrapping around.
    /// With no current focus, this is the first focusable widget.
    pub fn next(&self, tree: &WidgetNode) -> FocusPath {
        let list = Self::focusable(tree);
        if list.is_empty() {
            return FocusPath::default();
        }
        match list.iter().position(|node| node.path == *self) {
            Some(index) => list[(index + 1) % list.len()].path.clone(),
            None => self.resolve(tree),
        }
    }

    /// Previous focusable widget in depth-first order, wrapping around.
    /// With no current focus, this is the last focusable widget.
    pub fn prev(&self, tree: &WidgetNode) -> FocusPath {
        let list = Self::focusable(tree);
        if list.is_empty() {
            return FocusPath::default();
        }
        match list.iter().position(|node| node.path == *self) {
            Some(0) => list[list.len() - 1].path.clone(),
            Some(index) => list[index - 1].path.clone(),
            None => list[list.len() - 1].path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> WidgetNode {
        WidgetNode::Container {
            id: Some("root".to_string()),
            kind: crate::widget::ContainerKind::Column,
            gap: None,
            children: vec![
                WidgetNode::Button {
                    id: "ok".to_string(),
                    label: "OK".to_string(),
                    command: "example.ok".to_string(),
                    variant: None,
                },
                WidgetNode::Container {
                    id: Some("form".to_string()),
                    kind: crate::widget::ContainerKind::Column,
                    gap: None,
                    children: vec![
                        WidgetNode::TextInput {
                            id: Some("name".to_string()),
                            value: String::new(),
                            placeholder: None,
                        },
                        // No id, so not addressable by a path: skipped.
                        WidgetNode::TextInput {
                            id: None,
                            value: String::new(),
                            placeholder: None,
                        },
                        WidgetNode::Text {
                            id: None,
                            text: "label".to_string(),
                            color: None,
                            font: None,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn focus_path_collects_focusable_nodes_in_depth_first_order() {
        let paths = FocusPath::focusable(&tree());
        let ids: Vec<Vec<String>> = paths.iter().map(|node| node.path.ids().to_vec()).collect();
        assert_eq!(
            ids,
            vec![
                vec!["root".to_string(), "ok".to_string()],
                vec!["root".to_string(), "form".to_string(), "name".to_string()],
            ]
        );
        assert_eq!(paths[0].role, "button");
        assert_eq!(paths[1].role, "textInput");
        assert_eq!(
            FocusPath::new(vec!["root".to_string(), "ok".to_string()]).focused_role(&tree()),
            Some("button")
        );
    }

    #[test]
    fn focus_path_resolves_to_the_closest_surviving_ancestor() {
        let gone = FocusPath::new(vec![
            "root".to_string(),
            "form".to_string(),
            "missing".to_string(),
        ]);
        // The `form` ancestor survives, so its first widget wins.
        assert_eq!(
            gone.resolve(&tree()),
            FocusPath::new(vec![
                "root".to_string(),
                "form".to_string(),
                "name".to_string()
            ])
        );
        let everything_gone = FocusPath::new(vec!["other".to_string()]);
        assert_eq!(
            everything_gone.resolve(&tree()),
            FocusPath::new(vec!["root".to_string(), "ok".to_string()])
        );
        assert_eq!(
            FocusPath::default().resolve(&tree()),
            FocusPath::new(vec!["root".to_string(), "ok".to_string()])
        );
    }

    #[test]
    fn focus_path_moves_forward_and_backward_with_wrapping() {
        let first = FocusPath::new(vec!["root".to_string(), "ok".to_string()]);
        let second = FocusPath::new(vec![
            "root".to_string(),
            "form".to_string(),
            "name".to_string(),
        ]);
        assert_eq!(first.next(&tree()), second);
        assert_eq!(second.next(&tree()), first);
        assert_eq!(first.prev(&tree()), second);
        assert_eq!(second.prev(&tree()), first);
    }
}
