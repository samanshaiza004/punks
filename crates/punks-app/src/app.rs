//! GPUI application/window bootstrap. `punks-standalone` calls [`run`] and
//! nothing else -- window lifecycle, rendering, and platform drag-out all
//! live in this crate now, not in the executable shell.

use gpui::prelude::*;
use gpui::{px, size, App, Bounds, WindowBounds, WindowOptions};
use gpui_component::Root;

use crate::browser::MainWindow;
use crate::theme;

/// The GPUI entry point. `punks-standalone`'s `main()` calls this and
/// nothing else.
pub fn run() {
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_component::init(cx);
        theme::apply_neon_theme(cx);

        cx.on_window_closed(|cx, _window_id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1100.0), px(700.0)),
                cx,
            ))),
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(MainWindow::new);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open Punks window");
        })
        .detach();

        cx.activate(true);
    });
}
