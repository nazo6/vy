//! Native widget tree rendering for `ui` views.
//!
//! Defined in `docs/specs/ui.md` and `docs/adr/0005-native-plugin-ui.md`.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Context, EntityId, IntoElement, Pixels, Point, ScrollHandle, Window,
    canvas, div, point, prelude::*, px, size,
};
use vy_app::focus_path::FocusPath;
use vy_app::ui_list::{self, ListState};
use vy_app::widget::{ButtonVariant, ContainerKind, Gap, UiRole, WidgetNode};
use vy_core::PaneId;

use crate::theme::Theme;
use crate::ui::color::color as resolve_color;
use crate::view::EditorView;

/// Height of one row of a list, tree, or table widget.
pub const ROW_HEIGHT: f32 = 22.0;

/// Scroll state of the widget area of one UI pane.
///
/// The area itself is the scroll viewport. A panel can be taller than the pane
/// with any layout, not only with a list, so the whole tree scrolls; a focus
/// move then asks for the focused widget to be revealed.
pub struct PaneScroll {
    /// Offset of the area's viewport. The handle is shared with the element
    /// that tracks the scroll, so it keeps its offset across frames.
    pub handle: ScrollHandle,
    /// Set when widget focus moved. The next frame reveals the focused widget
    /// and clears the flag, so a wheel scroll is not undone by a redraw.
    pub reveal: Rc<Cell<bool>>,
}

impl PaneScroll {
    pub fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
            reveal: Rc::new(Cell::new(false)),
        }
    }
}

impl Default for PaneScroll {
    fn default() -> Self {
        Self::new()
    }
}

/// The pane being drawn, the widget that holds focus, and list state.
///
/// The same values reach every node in the recursion so a click can name the
/// widget it landed on, the focused widget can draw a ring, and a list can draw
/// its cursor and marks.
#[derive(Clone, Copy)]
struct RenderContext<'a> {
    pane_id: PaneId,
    focus_path: &'a FocusPath,
    list_states: &'a std::collections::BTreeMap<String, ListState>,
    /// Scroll state of the pane being drawn, when it has one.
    scroll: Option<&'a PaneScroll>,
}

/// Renders a custom declarative widget node into a GPUI element tree.
///
/// `focus_path` names the focused widget of this pane; an empty path draws no
/// focus ring. `list_states` supplies the cursor and marks of list, tree, and
/// table widgets, and `scroll` the viewport that keeps the focused widget in
/// view.
pub fn render_widget_node(
    theme: &Theme,
    node: &WidgetNode,
    pane_id: PaneId,
    focus_path: &FocusPath,
    list_states: &std::collections::BTreeMap<String, ListState>,
    scroll: Option<&PaneScroll>,
    cx: &mut Context<EditorView>,
) -> AnyElement {
    let context = RenderContext {
        pane_id,
        focus_path,
        list_states,
        scroll,
    };
    // The wrapper only gives the tree full width. Its height stays content
    // sized so the pane's scroll viewport has something to scroll.
    div()
        .flex()
        .flex_col()
        .w_full()
        .child(render_node(theme, node, &FocusPath::new(), context, cx))
        .into_any_element()
}

/// The scroll offset that brings `target` inside `viewport`, moving as little
/// as possible. `None` when it is already visible.
fn reveal_offset(
    viewport: Bounds<Pixels>,
    target: Bounds<Pixels>,
    offset: Point<Pixels>,
) -> Option<Point<Pixels>> {
    // The viewport is only known after the pane was painted once.
    if f32::from(viewport.size.height) <= 0.0 {
        return None;
    }
    let mut next = offset;
    if target.top() < viewport.top() {
        next.y += viewport.top() - target.top();
    } else if target.bottom() > viewport.bottom() {
        next.y -= target.bottom() - viewport.bottom();
    }
    (next != offset).then_some(next)
}

/// An invisible overlay that measures a widget and reveals it after a focus
/// move.
///
/// `row` names a fixed-height row inside the widget, which is how a list cursor
/// is revealed; `None` reveals the widget box itself. The offset is applied
/// before the next frame draws, because moving it in the middle of a frame
/// would leave the frame's hitboxes at the old positions.
fn reveal_overlay(view: EntityId, scroll: &PaneScroll, row: Option<usize>) -> AnyElement {
    let handle = scroll.handle.clone();
    let reveal = scroll.reveal.clone();
    canvas(
        move |bounds, window: &mut Window, _cx| {
            if !reveal.get() {
                return;
            }
            reveal.set(false);
            let target = match row {
                Some(index) => Bounds::new(
                    point(
                        bounds.origin.x,
                        bounds.origin.y + px(index as f32 * ROW_HEIGHT),
                    ),
                    size(bounds.size.width, px(ROW_HEIGHT)),
                ),
                None => bounds,
            };
            let Some(next) = reveal_offset(handle.bounds(), target, handle.offset()) else {
                return;
            };
            let handle = handle.clone();
            window.on_next_frame(move |_, cx| {
                handle.set_offset(next);
                // The offset only shows once the view draws again with the new
                // scroll position.
                cx.notify(view);
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

fn render_node(
    theme: &Theme,
    node: &WidgetNode,
    path: &FocusPath,
    context: RenderContext<'_>,
    cx: &mut Context<EditorView>,
) -> AnyElement {
    // Only nodes with an id extend the path, matching how `focus_path` walks.
    let path = match node.id() {
        Some(id) => path.joined(id),
        None => path.clone(),
    };
    let focused = !context.focus_path.is_empty() && &path == context.focus_path;

    match node {
        WidgetNode::Container {
            kind,
            gap,
            children,
            ..
        } => {
            let mut container = div().flex();
            match kind {
                ContainerKind::Column => {
                    container = container.flex_col();
                }
                ContainerKind::Row => {
                    container = container.flex_row().items_center();
                }
                ContainerKind::Stack => {
                    container = container.relative();
                }
            }

            match gap {
                Some(Gap::Sm) => {
                    container = container.gap_1();
                }
                Some(Gap::Md) => {
                    container = container.gap_2();
                }
                Some(Gap::Lg) => {
                    container = container.gap_4();
                }
                None => {}
            }

            for child in children {
                container = container.child(render_node(theme, child, &path, context, cx));
            }
            container.into_any_element()
        }
        WidgetNode::Text { text, color, .. } => {
            let mut text_el = div().child(text.clone());
            text_el = text_el.text_color(color.map_or(theme.foreground.default, |token| {
                resolve_color(theme, token)
            }));
            text_el.into_any_element()
        }
        WidgetNode::Button {
            id,
            label,
            command,
            variant,
        } => {
            let mut button = div()
                .id(gpui::SharedString::from(id.clone()))
                .w_full()
                .px_2()
                .py_0p5()
                .rounded_sm()
                .border_1()
                .border_color(gpui::rgba(0x00000000))
                .cursor_pointer()
                .child(label.clone());

            match variant {
                Some(ButtonVariant::Primary) => {
                    button = button
                        .bg(theme.statusline.info)
                        .text_color(theme.statusline.foreground);
                }
                Some(ButtonVariant::Danger) => {
                    button = button
                        .bg(theme.statusline.error)
                        .text_color(theme.statusline.foreground);
                }
                None => {
                    button = button
                        .text_color(theme.foreground.default)
                        .hover(|s| s.bg(theme.border.subtle));
                }
            }

            if focused {
                button = button.border_color(theme.border.focused);
            }

            let focus_path = path.clone();
            let pane_id = context.pane_id;
            button = button.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |view, _, _, cx| {
                    view.focus_widget(pane_id, &focus_path);
                    cx.notify();
                }),
            );

            if !command.is_empty() {
                let cmd = command.clone();
                button = button.on_click(cx.listener(move |this, _, _, cx| {
                    this.execute_command_string(&cmd);
                    cx.notify();
                }));
            }

            if focused && let Some(scroll) = context.scroll {
                button = button.child(reveal_overlay(cx.entity().entity_id(), scroll, None));
            }

            button.into_any_element()
        }
        WidgetNode::TextInput {
            value, placeholder, ..
        } => {
            let text = if value.is_empty() {
                placeholder.clone().unwrap_or_default()
            } else {
                value.clone()
            };
            let mut field = div()
                .px_2()
                .py_1()
                .bg(theme.background.surface)
                .border_1()
                .border_color(if focused {
                    theme.border.focused
                } else {
                    theme.border.subtle
                })
                .text_color(if value.is_empty() {
                    theme.foreground.muted
                } else {
                    theme.foreground.default
                });

            if node.focus_role() == Some(UiRole::TextInput) {
                let focus_path = path.clone();
                let pane_id = context.pane_id;
                field = field.on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, _, cx| {
                        view.focus_widget(pane_id, &focus_path);
                        cx.notify();
                    }),
                );
            }

            if focused && let Some(scroll) = context.scroll {
                field = field.child(reveal_overlay(cx.entity().entity_id(), scroll, None));
            }

            field.child(text).into_any_element()
        }
        WidgetNode::List { id, .. }
        | WidgetNode::Tree { id, .. }
        | WidgetNode::Table { id, .. } => {
            let rows = ui_list::rows(node, context.list_states.get(id).unwrap_or(&EMPTY_STATE));
            let state = context.list_states.get(id);
            let has_focus = focused;
            // Rows are a fixed height, so the pane can reveal the cursor row
            // from the widget's box alone.
            let cursor_row = state.and_then(|state| ui_list::cursor_row_index(node, state));

            let mut list = div()
                .flex()
                .flex_col()
                .w_full()
                .border_1()
                .border_color(if has_focus {
                    theme.border.focused
                } else {
                    gpui::rgba(0x00000000)
                })
                .rounded_sm();

            if rows.is_empty() {
                list = list
                    .px_2()
                    .py_1()
                    .text_color(theme.foreground.muted)
                    .child("no items");
            }

            for row in rows {
                let is_cursor = state.and_then(ListState::cursor) == Some(row.id.as_str());
                let is_marked = state.is_some_and(|state| state.is_selected(&row.id));

                let mut line = div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(ROW_HEIGHT))
                    .flex_shrink_0()
                    .px_2()
                    .w_full()
                    .overflow_hidden()
                    .whitespace_nowrap();

                if is_cursor && has_focus {
                    line = line
                        .bg(theme.list.active_background)
                        .text_color(theme.list.active_foreground);
                } else if is_marked {
                    line = line.bg(theme.border.subtle);
                } else {
                    line = line.text_color(theme.foreground.default);
                }

                if row.depth > 0 {
                    line = line.pl(px(12.0) * row.depth as f32);
                }
                if node.focus_role() == Some(UiRole::Tree) && row.has_children {
                    line = line.child(if row.collapsed { "▸" } else { "▾" });
                }

                line = line.child(row.label);
                if let Some(description) = row.description {
                    line = line.child(
                        div()
                            .text_xs()
                            .text_color(theme.foreground.muted)
                            .child(description),
                    );
                }
                list = list.child(line);
            }

            if focused && let Some(scroll) = context.scroll {
                list = list.child(reveal_overlay(cx.entity().entity_id(), scroll, cursor_row));
            }

            // A click focuses the list. Mapping a point to a row is still out of
            // scope: the rows are drawn by the host but have no hitboxes.
            let focus_path = path.clone();
            let pane_id = context.pane_id;
            list.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |view, _, _, cx| {
                    view.focus_widget(pane_id, &focus_path);
                    cx.notify();
                }),
            )
            .into_any_element()
        }
        WidgetNode::Separator { .. } => div()
            .w_full()
            .h(px(1.0))
            .bg(theme.border.subtle)
            .into_any_element(),
    }
}

/// List state used when a widget has no remembered state yet.
static EMPTY_STATE: ListState = ListState::empty();

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    #[test]
    fn reveal_moves_a_row_below_the_viewport_up_by_its_overlap() {
        let viewport = rect(0.0, 100.0, 200.0, 100.0);
        let target = rect(0.0, 180.0, 200.0, 40.0);
        let offset = point(px(0.0), px(0.0));

        assert_eq!(
            reveal_offset(viewport, target, offset),
            Some(point(px(0.0), px(-20.0)))
        );
    }

    #[test]
    fn reveal_moves_a_row_above_the_viewport_down_by_its_overlap() {
        let viewport = rect(0.0, 100.0, 200.0, 100.0);
        let target = rect(0.0, 60.0, 200.0, 20.0);
        let offset = point(px(0.0), px(-50.0));

        assert_eq!(
            reveal_offset(viewport, target, offset),
            Some(point(px(0.0), px(-10.0)))
        );
    }

    #[test]
    fn reveal_keeps_a_visible_row_in_place() {
        let viewport = rect(0.0, 100.0, 200.0, 100.0);
        let target = rect(0.0, 120.0, 200.0, 40.0);
        let offset = point(px(0.0), px(-30.0));

        assert_eq!(reveal_offset(viewport, target, offset), None);
    }

    #[test]
    fn reveal_waits_for_the_first_paint() {
        // The viewport is only measured once the pane was drawn.
        let viewport = rect(0.0, 0.0, 0.0, 0.0);
        let target = rect(0.0, 300.0, 200.0, 20.0);
        let offset = point(px(0.0), px(0.0));

        assert_eq!(reveal_offset(viewport, target, offset), None);
    }
}
