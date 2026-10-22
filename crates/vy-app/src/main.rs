//! Skeleton of `vy-app`.
//!
//! On startup it launches `vy-server` as a child process and opens the main editor window.

pub mod theme;
mod ui;
mod view;

use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_platform::application;
use theme::Theme;
use view::EditorView;

fn main() {
    application().run(|cx: &mut App| {
        gpui_base::init(cx);
        cx.set_global(Theme::dark());

        let bounds = Bounds::centered(None, size(px(1000.0), px(700.0)), cx);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| cx.new(EditorView::new),
            )
            .expect("could not open the window");

        window
            .update(cx, |view, window, cx| {
                view.focus_handle.focus(window, cx);
            })
            .ok();

        cx.activate(true);
    });
}
