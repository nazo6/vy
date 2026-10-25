//! Skeleton of `vy-app`.
//!
//! On startup it launches `vy-server` as a child process and opens the main editor window.

pub mod focus;
pub mod theme;
mod ui;
mod view;

use gpui::{
    App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, prelude::*, px, size,
};
use gpui_platform::application;
use theme::Theme;
use view::EditorView;
use vy_app::config::Config;
use vy_app::connection::{CliOptions, startup_for};
use vy_app::paths::Paths;

fn main() {
    let options = CliOptions::parse();

    let startup = match startup_for(&options, &Paths::from_env()) {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("vy: {error}");
            std::process::exit(2);
        }
    };

    let (initial_config, _) = Config::load(&Paths::from_env(), startup.local_workspace.as_deref());
    let initial_theme = if initial_config.theme() == "light" {
        Theme::light()
    } else {
        Theme::dark()
    };

    application().run(move |cx: &mut App| {
        gpui_base::init(cx);
        cx.set_global(initial_theme);

        let bounds = Bounds::centered(None, size(px(1000.0), px(700.0)), cx);
        let view_startup = startup.clone();

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Vy".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(9.0), px(8.0))),
                    }),
                    ..Default::default()
                },
                move |_window, cx| cx.new(|cx| EditorView::new(cx, view_startup.clone())),
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
