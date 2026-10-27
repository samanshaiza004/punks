//! GPUI application/window bootstrap. `punks-standalone` calls [`run`] and
//! nothing else -- window lifecycle, rendering, and platform drag-out all
//! live in this crate now, not in the executable shell.

use gpui::prelude::*;
use gpui::{px, size, App, Bounds, KeyBinding, WindowBounds, WindowOptions};
use gpui_component::Root;

use crate::actions::{
    Confirm, CursorDown, CursorUp, FocusSearch, NavigateBack, Redo, ShowSettings, ToggleInspector,
    TogglePlayback, Undo,
};
use crate::browser::MainWindow;
use crate::theme;

/// App-wide keybindings. The Results-pane ones are read from the persisted
/// config so a user's remapped keys (`config.rs`'s `Keybinds`) take effect
/// immediately at startup. The eight functional bindings are deliberately
/// split between the results context and the main-window context. Confirm is
/// always additionally bound to Enter
/// regardless of the configured key, matching the old ImGui frontend's
/// D/Enter/KeypadEnter equivalence (GPUI reports the numeric-keypad Enter as
/// the same "enter" key name as the main Enter on this platform, so one
/// binding covers both). `FocusSearch` isn't (yet) user-configurable --
/// `config.rs`'s `Keybinds` doesn't have a slot for it; so it's a fixed
/// cross-platform binding (cmd-F on
/// macOS, ctrl-F elsewhere) with no key-context restriction, so it works
/// from any pane.
fn bind_keys(cx: &mut App) {
    let keybinds = crate::config::load().keybinds;
    cx.bind_keys([
        KeyBinding::new(
            &keybinds.navigate_up.to_lowercase(),
            CursorUp,
            Some("ResultsPanel"),
        ),
        KeyBinding::new(
            &keybinds.navigate_down.to_lowercase(),
            CursorDown,
            Some("ResultsPanel"),
        ),
        KeyBinding::new(
            &keybinds.navigate_back.to_lowercase(),
            NavigateBack,
            Some("MainWindow"),
        ),
        KeyBinding::new(
            &keybinds.confirm.to_lowercase(),
            Confirm,
            Some("ResultsPanel"),
        ),
        KeyBinding::new("enter", Confirm, Some("ResultsPanel")),
        KeyBinding::new(
            &keybinds.play_stop.to_lowercase(),
            TogglePlayback,
            Some("MainWindow"),
        ),
        KeyBinding::new(
            &keybinds.toggle_inspector.to_lowercase(),
            ToggleInspector,
            Some("MainWindow"),
        ),
        KeyBinding::new(&keybinds.undo.to_lowercase(), Undo, Some("MainWindow")),
        KeyBinding::new(&keybinds.redo.to_lowercase(), Redo, Some("MainWindow")),
        KeyBinding::new("secondary-f", FocusSearch, None),
        KeyBinding::new("secondary-,", ShowSettings, None),
    ]);
}

/// The GPUI entry point. `punks-standalone`'s `main()` calls this and
/// nothing else.
pub fn run() {
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_component::init(cx);
        theme::apply_neon_theme(cx);
        bind_keys(cx);

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
                let view = cx.new(|cx| MainWindow::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open Punks window");
        })
        .detach();

        cx.activate(true);
    });
}
