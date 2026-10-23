//! Custom titlebar rendering using GPUI.

use gpui::{ClickEvent, MouseDownEvent, SharedString, Window, div, prelude::*, px};

use crate::theme::Theme;

/// Height of the custom titlebar.
pub const TITLEBAR_HEIGHT: f32 = 28.0;

/// Left inset reserved on macOS for the native traffic light buttons.
#[cfg(target_os = "macos")]
const MAC_TRAFFIC_LIGHT_INSET: f32 = 78.0;

/// Renders the custom window titlebar.
pub fn render_titlebar(
    theme: &Theme,
    title: impl Into<SharedString>,
    is_dirty: bool,
    _window: &mut Window,
    on_drag: impl Fn(&MouseDownEvent, &mut Window) + 'static,
    on_double_click: impl Fn(&ClickEvent, &mut Window) + 'static,
) -> impl IntoElement {
    let title: SharedString = title.into();
    let dirty_indicator = if is_dirty { " ●" } else { "" };
    let full_title = format!("{title}{dirty_indicator}");

    let mut titlebar = div()
        .id("vy-titlebar")
        .h(px(TITLEBAR_HEIGHT))
        .w_full()
        .bg(theme.titlebar.background)
        .border_b_1()
        .border_color(theme.titlebar.border)
        .flex()
        .flex_row()
        .items_center()
        .text_size(px(12.0))
        .text_color(theme.titlebar.foreground)
        .on_mouse_down(gpui::MouseButton::Left, move |event, window, _| {
            on_drag(event, window);
        })
        .on_click(move |event, window, _| {
            if event.click_count() == 2 {
                on_double_click(event, window);
            }
        });

    #[cfg(target_os = "macos")]
    {
        // When not in fullscreen, inset content from the left so macOS traffic light buttons are unobstructed.
        if !window.is_fullscreen() {
            titlebar = titlebar.pl(px(MAC_TRAFFIC_LIGHT_INSET));
        } else {
            titlebar = titlebar.pl(px(12.0));
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        titlebar = titlebar.pl(px(12.0));
    }

    titlebar.child(
        div().flex().flex_row().items_center().gap(px(6.0)).child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.titlebar.foreground)
                .child(full_title),
        ),
    )
}
