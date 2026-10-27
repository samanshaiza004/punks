//! The Search bar: a real `gpui-component::input::Input` wired to
//! `SampleBrowser::search`/`clear_search`. `results.rs` already branches on
//! `SampleBrowser::is_in_search_mode()` to display search results instead
//! of the browse listing, so this file's only job is turning keystrokes into
//! `search()`/`clear_search()` calls -- no display logic lives here.

use gpui::prelude::*;
use gpui::{Context, Entity, Focusable, Window};
use gpui_component::button::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{h_flex, Sizable};

use super::MainWindow;
use crate::actions::FocusSearch;

impl MainWindow {
    /// Called once from `MainWindow::new` (which is why it needs `window`
    /// directly, unlike most of this file's siblings -- `InputState::new`
    /// requires it at construction time).
    pub(super) fn new_search_input(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
        cx.subscribe_in(&input, window, |this, input, event, _window, cx| {
            if let InputEvent::Change = event {
                let query = input.read(cx).value().to_string();
                this.browser.update(cx, |b, cx| {
                    if query.is_empty() {
                        b.inner.clear_search();
                    } else {
                        b.inner.search(&query);
                    }
                    b.ensure_polling(cx);
                    cx.notify();
                });
            }
        })
        .detach();
        input
    }

    pub(super) fn focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
    }

    pub(super) fn render_search(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("search")
            .p_2()
            .child(Input::new(&self.search_input))
            .child(
                Button::new("settings")
                    .label("Settings")
                    .small()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.open_settings(cx);
                    })),
            )
    }
}
