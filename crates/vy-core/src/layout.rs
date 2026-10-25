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

    /// Rebuilds a layout from a saved tree.
    ///
    /// An active pane that is not in the tree falls back to the first leaf.
    pub fn restore(
        window_id: WindowId,
        root: Option<LayoutNode>,
        active_pane: Option<PaneId>,
        revision: u64,
    ) -> Self {
        let mut layout = Self {
            window_id,
            root,
            active_pane: None,
            revision,
        };
        let leaves = layout.leaves();
        layout.active_pane = active_pane
            .filter(|pane| leaves.contains(pane))
            .or_else(|| leaves.first().copied());
        layout
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

    /// Resets all split ratios in the layout tree to 0.5.
    pub fn equalize(&mut self) {
        if let Some(root) = self.root.as_mut() {
            equalize_node(root);
            self.revision += 1;
        }
    }

    /// Resizes the boundary between two adjacent panes (their lowest common ancestor split).
    pub fn resize_boundary(
        &mut self,
        first_pane: PaneId,
        second_pane: PaneId,
        ratio: f32,
    ) -> Result<(), LayoutError> {
        if !self.contains(first_pane) {
            return Err(LayoutError::PaneNotFound(first_pane));
        }
        if !self.contains(second_pane) {
            return Err(LayoutError::PaneNotFound(second_pane));
        }
        let Some(root) = self.root.as_mut() else {
            return Err(LayoutError::PaneNotFound(first_pane));
        };

        let clamped = ratio.clamp(0.05, 0.95);
        if set_boundary_ratio(root, first_pane, second_pane, clamped) {
            self.revision += 1;
            Ok(())
        } else {
            Err(LayoutError::PaneNotFound(second_pane))
        }
    }

    /// Adjusts the size of a pane along `direction`, or the nearest split if `direction` is None.
    ///
    /// When `delta` is given, increases or decreases the pane's share of the split.
    /// When `ratio` is given, sets the pane's share to that absolute value.
    pub fn resize_pane(
        &mut self,
        pane: PaneId,
        direction: Option<SplitDirection>,
        delta: Option<f32>,
        ratio: Option<f32>,
    ) -> Result<(), LayoutError> {
        if !self.contains(pane) {
            return Err(LayoutError::PaneNotFound(pane));
        }
        let Some(root) = self.root.as_mut() else {
            return Err(LayoutError::PaneNotFound(pane));
        };

        let adjusted = adjust_pane_split(root, pane, direction, delta, ratio)
            || (direction.is_some() && adjust_pane_split(root, pane, None, delta, ratio));
        if adjusted {
            self.revision += 1;
        }
        Ok(())
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

fn equalize_node(node: &mut LayoutNode) {
    if let LayoutNode::Split {
        ratio,
        first,
        second,
        ..
    } = node
    {
        *ratio = 0.5;
        equalize_node(first);
        equalize_node(second);
    }
}

pub fn set_boundary_ratio(
    node: &mut LayoutNode,
    first_pane: PaneId,
    second_pane: PaneId,
    new_ratio: f32,
) -> bool {
    let LayoutNode::Split {
        ratio,
        first,
        second,
        ..
    } = node
    else {
        return false;
    };

    let first_has_a = node_contains(first, first_pane);
    let second_has_b = node_contains(second, second_pane);

    if first_has_a && second_has_b {
        *ratio = new_ratio;
        return true;
    }

    let first_has_b = node_contains(first, second_pane);
    let second_has_a = node_contains(second, first_pane);

    if first_has_b && second_has_a {
        *ratio = 1.0 - new_ratio;
        return true;
    }

    if first_has_a && first_has_b {
        return set_boundary_ratio(first, first_pane, second_pane, new_ratio);
    }

    if second_has_a && second_has_b {
        return set_boundary_ratio(second, first_pane, second_pane, new_ratio);
    }

    false
}

fn node_contains(node: &LayoutNode, pane: PaneId) -> bool {
    match node {
        LayoutNode::Leaf { pane_id } => *pane_id == pane,
        LayoutNode::Split { first, second, .. } => {
            node_contains(first, pane) || node_contains(second, pane)
        }
    }
}

fn adjust_pane_split(
    node: &mut LayoutNode,
    target: PaneId,
    direction: Option<SplitDirection>,
    delta: Option<f32>,
    ratio_val: Option<f32>,
) -> bool {
    let LayoutNode::Split {
        direction: split_dir,
        ratio,
        first,
        second,
    } = node
    else {
        return false;
    };

    let target_in_first = node_contains(first, target);
    let target_in_second = node_contains(second, target);

    if !target_in_first && !target_in_second {
        return false;
    }

    if target_in_first && adjust_pane_split(first, target, direction, delta, ratio_val) {
        return true;
    }
    if target_in_second && adjust_pane_split(second, target, direction, delta, ratio_val) {
        return true;
    }

    if direction.is_none() || direction == Some(*split_dir) {
        if let Some(delta) = delta {
            if target_in_first {
                *ratio = (*ratio + delta).clamp(0.05, 0.95);
            } else {
                *ratio = (*ratio - delta).clamp(0.05, 0.95);
            }
        } else if let Some(new_r) = ratio_val {
            let clamped = new_r.clamp(0.05, 0.95);
            if target_in_first {
                *ratio = clamped;
            } else {
                *ratio = 1.0 - clamped;
            }
        }
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_a_saved_tree_and_active_pane() {
        let root = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.3,
            first: Box::new(LayoutNode::Leaf { pane_id: 4 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 5 }),
        };
        let layout = Layout::restore(7, Some(root.clone()), Some(5), 12);

        assert_eq!(layout.window_id(), 7);
        assert_eq!(layout.root(), Some(&root));
        assert_eq!(layout.active_pane(), Some(5));
        assert_eq!(layout.revision(), 12);
        assert_eq!(layout.leaves(), vec![4, 5]);
    }

    #[test]
    fn restore_falls_back_when_the_active_pane_is_missing() {
        let root = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };
        let layout = Layout::restore(1, Some(root), Some(99), 0);
        assert_eq!(layout.active_pane(), Some(1));
    }

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

    #[test]
    fn resizes_boundary_between_two_panes() {
        let mut layout = Layout::new(1);
        layout
            .open(1, None, SplitDirection::Horizontal, false)
            .unwrap();
        layout
            .open(2, Some(1), SplitDirection::Horizontal, false)
            .unwrap();

        layout.resize_boundary(1, 2, 0.7).unwrap();
        let expected = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.7,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };
        assert_eq!(layout.root(), Some(&expected));

        // Reversing pane order should invert ratio.
        layout.resize_boundary(2, 1, 0.7).unwrap();
        let expected_inverted = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.3,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };
        assert_eq!(layout.root(), Some(&expected_inverted));
    }

    #[test]
    fn adjusts_pane_size_with_delta() {
        let mut layout = Layout::new(1);
        layout
            .open(1, None, SplitDirection::Horizontal, false)
            .unwrap();
        layout
            .open(2, Some(1), SplitDirection::Horizontal, false)
            .unwrap();

        // Expanding first pane increases ratio
        layout
            .resize_pane(1, Some(SplitDirection::Horizontal), Some(0.1), None)
            .unwrap();
        let LayoutNode::Split { ratio, .. } = layout.root().unwrap() else {
            panic!("expected split");
        };
        assert!((ratio - 0.6).abs() < 1e-4);

        // Expanding second pane decreases ratio (giving more to second)
        layout
            .resize_pane(2, Some(SplitDirection::Horizontal), Some(0.2), None)
            .unwrap();
        let LayoutNode::Split { ratio, .. } = layout.root().unwrap() else {
            panic!("expected split");
        };
        assert!((ratio - 0.4).abs() < 1e-4);
    }

    #[test]
    fn equalizes_all_splits() {
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

        layout.resize_boundary(1, 2, 0.8).unwrap();
        layout.resize_boundary(2, 3, 0.2).unwrap();

        layout.equalize();

        let LayoutNode::Split {
            ratio: r1, second, ..
        } = layout.root().unwrap()
        else {
            panic!("expected split");
        };
        assert_eq!(*r1, 0.5);
        let LayoutNode::Split { ratio: r2, .. } = &**second else {
            panic!("expected second split");
        };
        assert_eq!(*r2, 0.5);
    }
}
