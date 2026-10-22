//! Skeleton of `vy-app`.
//!
//! On startup it launches `vy-server` as a child process and opens the main editor window.

mod ui;
mod view;

use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use view::EditorView;

fn main() {
    Application::new().run(|cx: &mut App| {
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
            .update(cx, |view, window, _cx| {
                view.focus_handle.focus(window);
            })
            .ok();

        cx.activate(true);
    });
}
