//! Geometric calculations and navigation across pane layouts.

use vy_core::{LayoutNode, PaneId, SplitDirection};

/// Collects pane ids from the layout tree in tree order.
pub fn collect_pane_ids(node: &LayoutNode, ids: &mut Vec<PaneId>) {
    match node {
        LayoutNode::Split { first, second, .. } => {
            collect_pane_ids(first, ids);
            collect_pane_ids(second, ids);
        }
        LayoutNode::Leaf { pane_id } => ids.push(*pane_id),
    }
}

/// Normalized bounding box of a pane within the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneRect {
    pub id: PaneId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PaneRect {
    pub fn center(self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// Computes the normalized bounding rectangles of all panes in the layout tree.
pub fn collect_pane_rects(node: &LayoutNode, rect: PaneRect, rects: &mut Vec<PaneRect>) {
    match node {
        LayoutNode::Leaf { pane_id } => rects.push(PaneRect {
            id: *pane_id,
            ..rect
        }),
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let ratio = ratio.clamp(0.0, 1.0);
            let (first_rect, second_rect) = match direction {
                SplitDirection::Horizontal => (
                    PaneRect {
                        width: rect.width * ratio,
                        ..rect
                    },
                    PaneRect {
                        x: rect.x + rect.width * ratio,
                        width: rect.width * (1.0 - ratio),
                        ..rect
                    },
                ),
                SplitDirection::Vertical => (
                    PaneRect {
                        height: rect.height * ratio,
                        ..rect
                    },
                    PaneRect {
                        y: rect.y + rect.height * ratio,
                        height: rect.height * (1.0 - ratio),
                        ..rect
                    },
                ),
            };
            collect_pane_rects(first, first_rect, rects);
            collect_pane_rects(second, second_rect, rects);
        }
    }
}

/// Picks the nearest pane in `direction` from `active`, using split ratios.
pub fn focus_neighbor(layout: &LayoutNode, active: PaneId, direction: &str) -> Option<PaneId> {
    let mut rects = Vec::new();
    collect_pane_rects(
        layout,
        PaneRect {
            id: active,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        &mut rects,
    );
    let current = rects.iter().find(|rect| rect.id == active)?;
    let (current_x, current_y) = current.center();

    rects
        .iter()
        .filter(|rect| rect.id != active)
        .filter_map(|rect| {
            let (x, y) = rect.center();
            let (dx, dy) = (x - current_x, y - current_y);
            let primary = match direction {
                "left" => -dx,
                "right" => dx,
                "up" => -dy,
                "down" => dy,
                _ => return None,
            };
            let secondary = match direction {
                "left" | "right" => dy.abs(),
                _ => dx.abs(),
            };
            (primary > 0.0).then_some((primary + secondary * 3.0, rect.id))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_pane_ids_in_tree_order() {
        let layout = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf { pane_id: 2 }),
                second: Box::new(LayoutNode::Leaf { pane_id: 3 }),
            }),
        };

        let mut ids = Vec::new();
        collect_pane_ids(&layout, &mut ids);
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn picks_the_neighbor_in_the_requested_direction() {
        let layout = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };

        assert_eq!(focus_neighbor(&layout, 1, "right"), Some(2));
        assert_eq!(focus_neighbor(&layout, 2, "left"), Some(1));
        assert_eq!(focus_neighbor(&layout, 1, "left"), None);

        let stacked = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.25,
            first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
            second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
        };
        assert_eq!(focus_neighbor(&stacked, 1, "down"), Some(2));
        assert_eq!(focus_neighbor(&stacked, 2, "up"), Some(1));
    }
}
