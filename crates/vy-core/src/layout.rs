//! The layout tree of one window.
//!
//! The core owns the tree; clients only render the confirmed result
//! (`docs/panes.md`). Panes are leaves, and a split is a binary node with a ratio.
//! Floating panes and saved sessions are not implemented yet.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type PaneId = u64;
pub type WindowId = u64;

/// Direction in which the children of a split are placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    /// Children are placed left and right.
    Horizontal,
    /// Children are placed above and below.
    Vertical,
}

impl SplitDirection {
    /// Parses the names used by `panes.open` placements.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "horizontal" | "left" | "right" => Some(Self::Horizontal),
            "vertical" | "above" | "below" => Some(Self::Vertical),
            _ => None,
        }
    }

    /// Whether a pane placed in this direction comes before the target.
    pub fn is_before(value: &str) -> bool {
        matches!(value, "left" | "above")
    }
}

/// A node of the layout tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LayoutNode {
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
    Leaf {
        pane_id: PaneId,
    },
}

/// A rejected layout operation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LayoutError {
    #[error("pane {0} is not in the layout")]
    PaneNotFound(PaneId),
    #[error("cannot close the last pane")]
    LastPane,
}

/// The layout tree, focused pane, and revision of one window.
#[derive(Debug, Clone)]
pub struct Layout {
    window_id: WindowId,
    root: Option<LayoutNode>,
    active_pane: Option<PaneId>,
    revision: u64,
}

impl Layout {
    pub fn new(window_id: WindowId) -> Self {
        Self {
            window_id,
            root: None,
            active_pane: None,
            revision: 0,
        }
    }

    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub fn root(&self) -> Option<&LayoutNode> {
        self.root.as_ref()
    }

    pub fn active_pane(&self) -> Option<PaneId> {
        self.active_pane
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Panes in tree order.
    pub fn leaves(&self) -> Vec<PaneId> {
        let mut leaves = Vec::new();
        if let Some(root) = &self.root {
            collect_leaves(root, &mut leaves);
        }
        leaves
    }

    pub fn contains(&self, pane: PaneId) -> bool {
        self.leaves().contains(&pane)
    }

    /// Adds a pane next to `target`, or makes it the root when the tree is empty.
    ///
    /// `before` puts the new pane in the first slot. When `target` is omitted, the
    /// active pane is used, matching `panes.open` in `docs/panes.md`.
    pub fn open(
        &mut self,
        pane: PaneId,
        target: Option<PaneId>,
        direction: SplitDirection,
        before: bool,
    ) -> Result<(), LayoutError> {
        if self.contains(pane) {
            return Err(LayoutError::PaneNotFound(pane));
        }

        if self.root.is_none() {
            self.root = Some(LayoutNode::Leaf { pane_id: pane });
            self.active_pane = Some(pane);
            self.revision += 1;
            return Ok(());
        }

        let target = target
            .or(self.active_pane)
            .ok_or(LayoutError::PaneNotFound(pane))?;
        if !self.contains(target) {
            return Err(LayoutError::PaneNotFound(target));
        }

        let root = self
            .root
            .as_mut()
            .ok_or(LayoutError::PaneNotFound(target))?;
        let replaced = split_leaf(
            root,
            target,
            LayoutNode::Split {
                direction,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf {
                    pane_id: if before { pane } else { target },
                }),
                second: Box::new(LayoutNode::Leaf {
                    pane_id: if before { target } else { pane },
                }),
            },
        );
        if !replaced {
            return Err(LayoutError::PaneNotFound(target));
        }

        self.active_pane = Some(pane);
        self.revision += 1;
        Ok(())
    }

    /// Moves the focus to a pane that is in the tree.
    pub fn focus(&mut self, pane: PaneId) -> Result<(), LayoutError> {
        if !self.contains(pane) {
            return Err(LayoutError::PaneNotFound(pane));
        }
        self.active_pane = Some(pane);
        self.revision += 1;
        Ok(())
    }

    /// Removes a pane and collapses the split that becomes empty.
    ///
    /// Closing the last pane fails; callers close the window instead.
    pub fn close(&mut self, pane: PaneId) -> Result<(), LayoutError> {
        if !self.contains(pane) {
            return Err(LayoutError::PaneNotFound(pane));
        }
        let Some(root) = self.root.as_mut() else {
            return Err(LayoutError::PaneNotFound(pane));
        };
        if matches!(root, LayoutNode::Leaf { pane_id } if *pane_id == pane) {
            return Err(LayoutError::LastPane);
        }

        remove_leaf(root, pane);
        if self.active_pane == Some(pane) {
            self.active_pane = self.leaves().first().copied();
        }
        self.revision += 1;
        Ok(())
    }
}

fn collect_leaves(node: &LayoutNode, leaves: &mut Vec<PaneId>) {
    match node {
        LayoutNode::Split { first, second, .. } => {
            collect_leaves(first, leaves);
            collect_leaves(second, leaves);
        }
        LayoutNode::Leaf { pane_id } => leaves.push(*pane_id),
    }
}

/// Replaces the leaf `target` with `replacement`. Returns whether it was found.
fn split_leaf(node: &mut LayoutNode, target: PaneId, replacement: LayoutNode) -> bool {
    match node {
        LayoutNode::Leaf { pane_id } if *pane_id == target => {
            *node = replacement;
            true
        }
        LayoutNode::Leaf { .. } => false,
        LayoutNode::Split { first, second, .. } => {
            split_leaf(first, target, replacement.clone())
                || split_leaf(second, target, replacement)
        }
    }
}

/// Removes the leaf `pane` and promotes the sibling of an emptied split.
fn remove_leaf(node: &mut LayoutNode, pane: PaneId) {
    let LayoutNode::Split { first, second, .. } = node else {
        return;
    };

    remove_leaf(first, pane);
    remove_leaf(second, pane);

    let remove_first = matches!(&**first, LayoutNode::Leaf { pane_id } if *pane_id == pane);
    let remove_second = matches!(&**second, LayoutNode::Leaf { pane_id } if *pane_id == pane);

    if remove_first {
        *node = (**second).clone();
    } else if remove_second {
        *node = (**first).clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_open_becomes_the_root() {
        let mut layout = Layout::new(1);
        layout
            .open(10, None, SplitDirection::Horizontal, false)
            .unwrap();

        assert_eq!(layout.root(), Some(&LayoutNode::Leaf { pane_id: 10 }));
        assert_eq!(layout.active_pane(), Some(10));
        assert_eq!(layout.revision(), 1);
        assert_eq!(layout.leaves(), vec![10]);
    }

    #[test]
    fn split_places_new_panes_on_either_side() {
        let mut layout = Layout::new(1);
        layout
            .open(1, None, SplitDirection::Horizontal, false)
            .unwrap();
        layout
            .open(2, Some(1), SplitDirection::Horizontal, false)
            .unwrap();
        layout
            .open(3, Some(1), SplitDirection::Vertical, true)
            .unwrap();

        assert_eq!(layout.leaves(), vec![3, 1, 2]);
        assert_eq!(layout.active_pane(), Some(3));
        assert_eq!(
            layout.root(),
            Some(&LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::Leaf { pane_id: 3 }),
                    second: Box::new(LayoutNode::Leaf { pane_id: 1 }),
                }),
                second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
            })
        );
    }

    #[test]
    fn close_collapses_the_split() {
        let mut layout = Layout::new(1);
        layout
            .open(1, None, SplitDirection::Horizontal, false)
            .unwrap();
        layout
            .open(2, Some(1), SplitDirection::Horizontal, false)
            .unwrap();

        layout.close(1).unwrap();
        assert_eq!(layout.root(), Some(&LayoutNode::Leaf { pane_id: 2 }));
        assert_eq!(layout.active_pane(), Some(2));
    }

    #[test]
    fn closing_the_active_pane_focuses_the_first_remaining_one() {
        let mut layout = Layout::new(1);
        layout
            .open(1, None, SplitDirection::Horizontal, false)
            .unwrap();
        layout
            .open(2, Some(1), SplitDirection::Horizontal, false)
            .unwrap();
        layout
            .open(3, Some(2), SplitDirection::Vertical, false)
            .unwrap();

        layout.close(3).unwrap();
        assert_eq!(layout.leaves(), vec![1, 2]);
        assert_eq!(layout.active_pane(), Some(1));
    }

    #[test]
    fn rejects_closing_the_last_pane_and_unknown_panes() {
        let mut layout = Layout::new(1);
        layout
            .open(1, None, SplitDirection::Horizontal, false)
            .unwrap();

        assert_eq!(layout.close(1), Err(LayoutError::LastPane));
        assert_eq!(layout.focus(9), Err(LayoutError::PaneNotFound(9)));
        assert_eq!(
            layout.open(2, Some(9), SplitDirection::Horizontal, false),
            Err(LayoutError::PaneNotFound(9))
        );
    }
}
