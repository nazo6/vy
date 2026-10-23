//! Native widget tree rendering for `ui` views.
//!
//! Defined in `docs/ui.md` and `docs/adr/0005-native-plugin-ui.md`.

use gpui::{Div, div, prelude::*, px};
use vy_app::widget::{ButtonVariant, ContainerKind, Gap, WidgetNode};

use crate::theme::Theme;
use crate::ui::color::color as resolve_color;

/// Renders a custom declarative widget node into a GPUI element tree.
pub fn render_widget_node(theme: &Theme, node: &WidgetNode) -> Div {
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
                container = container.child(render_widget_node(theme, child));
            }
            container
        }
        WidgetNode::Text { text, color, .. } => {
            let mut text_el = div().child(text.clone());
            text_el = text_el.text_color(color.map_or(theme.foreground.default, |token| {
                resolve_color(theme, token)
            }));
            text_el
        }
        WidgetNode::Button {
            label,
            command: _,
            variant,
            ..
        } => {
            let mut button = div()
                .px_3()
                .py_1()
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
                        .bg(theme.list.active_background)
                        .text_color(theme.foreground.default);
                }
            }
            button
        }
        WidgetNode::TextInput {
            value, placeholder, ..
        } => {
            let text = if value.is_empty() {
                placeholder.clone().unwrap_or_default()
            } else {
                value.clone()
            };
            div()
                .px_2()
                .py_1()
                .bg(theme.background.surface)
                .border_1()
                .border_color(theme.border.subtle)
                .text_color(if value.is_empty() {
                    theme.foreground.muted
                } else {
                    theme.foreground.default
                })
                .child(text)
        }
        WidgetNode::Separator { .. } => div().w_full().h(px(1.0)).bg(theme.border.subtle),
    }
}
