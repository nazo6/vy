//! Native widget tree rendering for `ui` views.
//!
//! Defined in `docs/specs/ui.md` and `docs/adr/0005-native-plugin-ui.md`.

use gpui::{AnyElement, Context, IntoElement, div, prelude::*, px};
use vy_app::focus_path::FocusPath;
use vy_app::ui_list::{self, ListState};
use vy_app::widget::{ButtonVariant, ContainerKind, Gap, UiRole, WidgetNode};
use vy_core::PaneId;

use crate::theme::Theme;
use crate::ui::color::color as resolve_color;
use crate::view::EditorView;

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
}

/// Renders a custom declarative widget node into a GPUI element tree.
///
/// `focus_path` names the focused widget of this pane; an empty path draws no
/// focus ring. `list_states` supplies the cursor and marks of list, tree, and
/// table widgets.
pub fn render_widget_node(
    theme: &Theme,
    node: &WidgetNode,
    pane_id: PaneId,
    focus_path: &FocusPath,
    list_states: &std::collections::BTreeMap<String, ListState>,
    cx: &mut Context<EditorView>,
) -> AnyElement {
    let context = RenderContext {
        pane_id,
        focus_path,
        list_states,
    };
    render_node(theme, node, &FocusPath::new(), context, cx)
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
                button = button.border_1().border_color(theme.border.focused);
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

            field.child(text).into_any_element()
        }
        WidgetNode::List { id, .. }
        | WidgetNode::Tree { id, .. }
        | WidgetNode::Table { id, .. } => {
            let rows = ui_list::rows(node, context.list_states.get(id).unwrap_or(&EMPTY_STATE));
            let state = context.list_states.get(id);
            let has_focus = focused;

            let mut list = div().flex().flex_col().w_full();
            if has_focus {
                list = list
                    .border_1()
                    .border_color(theme.border.focused)
                    .rounded_sm();
            }

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
                    .h(px(22.0))
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
                if row.collapsed {
                    line = line.child("▸");
                } else if node.focus_role() == Some(UiRole::Tree) && !row.collapsed {
                    // A tree row that is not collapsed may still be a leaf; the
                    // marker is only cosmetic.
                    line = line.child("▾");
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

            // A click focuses the list; the host's row hit test is out of scope
            // until the widget gains real scrolling.
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
