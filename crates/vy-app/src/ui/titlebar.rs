//! Custom titlebar rendering using GPUI.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Bounds, Decorations, MouseButton, PathBuilder, Pixels, Point, Rgba, SharedString, Window,
    canvas, div, point, prelude::*, px,
};

use crate::theme::Theme;

/// Height of the custom titlebar.
pub const TITLEBAR_HEIGHT: f32 = 28.0;

/// Width of a window control button (minimize/maximize/close).
const WINDOW_BUTTON_WIDTH: f32 = 40.0;

/// Size of the box the window control icons are drawn in.
const WINDOW_ICON_SIZE: f32 = 16.0;

/// Pointer travel, in pixels, before a titlebar press starts a window move.
/// Moving the window immediately would hand the pointer to the compositor and
/// the titlebar would never see the second click of a double click.
const DRAG_THRESHOLD: f64 = 3.0;

/// Left inset reserved on macOS for the native traffic light buttons.
#[cfg(target_os = "macos")]
const MAC_TRAFFIC_LIGHT_INSET: f32 = 78.0;

/// Renders the custom window titlebar.
///
/// `press` is shared with the view so a drag can be told apart from a click
/// across frames.
pub fn render_titlebar(
    theme: &Theme,
    title: impl Into<SharedString>,
    is_dirty: bool,
    window: &mut Window,
    press: Rc<Cell<Option<Point<Pixels>>>>,
    on_double_click: impl Fn(&mut Window) + 'static,
) -> impl IntoElement {
    let title: SharedString = title.into();
    let dirty_indicator = if is_dirty { " ●" } else { "" };
    let full_title = format!("{title}{dirty_indicator}");

    let press_on_down = press.clone();
    let press_on_move = press.clone();
    let press_on_up = press.clone();

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
        .on_mouse_down(MouseButton::Left, move |event, window, _cx| {
            if event.click_count == 2 {
                press_on_down.set(None);
                on_double_click(window);
                return;
            }
            press_on_down.set(Some(event.position));
        })
        .on_mouse_move(move |event, window, _cx| {
            if event.pressed_button != Some(MouseButton::Left) {
                press_on_move.set(None);
                return;
            }
            let Some(press) = press_on_move.get() else {
                return;
            };
            if (event.position - press).magnitude() > DRAG_THRESHOLD {
                press_on_move.set(None);
                window.start_window_move();
            }
        })
        .on_mouse_up(MouseButton::Left, move |_event, _window, _cx| {
            press_on_up.set(None);
        })
        .on_mouse_down(MouseButton::Right, |event, window, _| {
            // With client-side decorations there is no native titlebar, so the
            // window menu (move/resize/close) is our responsibility.
            window.show_window_menu(event.position);
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

    // macOS and Windows draw their own window buttons next to this titlebar.
    // Linux windows with client-side decorations have none, so draw them here.
    let shows_window_buttons = matches!(window.window_decorations(), Decorations::Client { .. });

    let title = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .overflow_hidden()
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.titlebar.foreground)
                .child(full_title),
        );

    let titlebar = titlebar.child(title);
    if shows_window_buttons {
        titlebar.child(render_window_buttons(theme))
    } else {
        titlebar
    }
}

#[derive(Clone, Copy, Debug)]
enum WindowButtonIcon {
    Minimize,
    Maximize,
    Close,
}

fn render_window_buttons(theme: &Theme) -> impl IntoElement {
    let foreground = theme.titlebar.foreground;
    let hover_background = theme.background.surface;

    div()
        .flex()
        .flex_row()
        .items_center()
        .h_full()
        .child(window_button(
            "vy-titlebar-minimize",
            foreground,
            hover_background,
            WindowButtonIcon::Minimize,
            |window| window.minimize_window(),
        ))
        .child(window_button(
            "vy-titlebar-maximize",
            foreground,
            hover_background,
            WindowButtonIcon::Maximize,
            |window| window.zoom_window(),
        ))
        .child(window_button(
            "vy-titlebar-close",
            foreground,
            hover_background,
            WindowButtonIcon::Close,
            |window| window.remove_window(),
        ))
}

fn window_button(
    id: &'static str,
    foreground: Rgba,
    hover_background: Rgba,
    icon: WindowButtonIcon,
    on_press: impl Fn(&mut Window) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(WINDOW_BUTTON_WIDTH))
        .h_full()
        .hover(move |style| style.bg(hover_background))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            // Keep the titlebar from starting a window move instead.
            cx.stop_propagation();
            on_press(window);
        })
        .on_click(|_event, _window, cx| cx.stop_propagation())
        .child(window_icon(icon, foreground))
}

fn window_icon(icon: WindowButtonIcon, foreground: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _cx| {
            let Some(path) = icon_path(icon, bounds) else {
                return;
            };
            window.paint_path(path, foreground);
        },
    )
    .size(px(WINDOW_ICON_SIZE))
}

fn icon_path(icon: WindowButtonIcon, bounds: Bounds<Pixels>) -> Option<gpui::Path<Pixels>> {
    let center = bounds.center();
    let mut path = PathBuilder::stroke(px(1.0));

    match icon {
        WindowButtonIcon::Minimize => {
            let half = px(5.0);
            path.move_to(point(center.x - half, center.y));
            path.line_to(point(center.x + half, center.y));
        }
        WindowButtonIcon::Maximize => {
            let half = px(4.5);
            let top_left = point(center.x - half, center.y - half);
            path.move_to(top_left);
            path.line_to(point(center.x + half, center.y - half));
            path.line_to(point(center.x + half, center.y + half));
            path.line_to(point(center.x - half, center.y + half));
            path.line_to(top_left);
        }
        WindowButtonIcon::Close => {
            let half = px(4.0);
            path.move_to(point(center.x - half, center.y - half));
            path.line_to(point(center.x + half, center.y + half));
            path.move_to(point(center.x + half, center.y - half));
            path.line_to(point(center.x - half, center.y + half));
        }
    }

    path.build().ok()
}
