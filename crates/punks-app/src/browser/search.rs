//! The Search bar: a real `gpui-component::input::Input` wired to
//! `SampleBrowser::search`/`clear_search`. `results.rs` already branches on
//! `SampleBrowser::is_in_search_mode()` (M3's grid, extended for search in
//! this milestone) to display search results instead of the browse
//! listing, so this file's only job is turning keystrokes into
//! `search()`/`clear_search()` calls -- no display logic lives here.

use gpui::prelude::*;
use gpui::{Context, Entity, Focusable, Window};
use gpui_component::h_flex;
use gpui_component::input::{Input, InputEvent, InputState};

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

    pub(super) fn render_search(&mut self, _cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("search")
            .p_2()
            .child(Input::new(&self.search_input))
    }
}

/// Verifies the wiring contract `new_search_input`'s subscription depends on
/// against a real `gpui-component::input::Input`/`InputState`, the same
/// test-only-entity pattern used for M3's `TestResultsView` and M4's
/// Button/Slider audit. No `SampleBrowser`/audio device involved.
///
/// Real keyboard-driven typing, and even plain Tab-focusing a rendered
/// `Input`, go through platform text-input/IME setup that `TestAppContext`'s
/// mock window doesn't implement -- both `cx.simulate_input("...")` and
/// `window.focus_next(cx)` panic with "Test Windows are not backed by a real
/// platform window" the moment a real `Input` is in the tree, confirmed by
/// checking `gpui-component`'s own `InputState` test suite
/// (`crates/base/src/input/base/state.rs`), which exercises text mutation
/// through `InputState`'s own methods rather than simulated keystrokes or
/// focus for exactly this reason. So, like M4's Slider audit,
/// `InputEvent::Change` is emitted directly here (the same event the real
/// typing path emits) to prove the subscription contract `new_search_input`
/// depends on is real. Tab-reachability for a real `Input` is **not**
/// verified this way -- see `docs/gpui-component-audit.md`'s M5 table.
#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{div, Render, TestAppContext};

    use super::*;

    struct SearchHarness {
        input: Entity<InputState>,
    }

    impl Render for SearchHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().tab_group().size_full().child(Input::new(&self.input))
        }
    }

    #[gpui::test]
    fn search_input_change_event_carries_the_typed_value(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
            SearchHarness { input }
        });
        let input = view.read_with(cx, |v, _| v.input.clone());

        let seen: Rc<Cell<Option<String>>> = Rc::new(Cell::new(None));
        let seen_for_sub = seen.clone();
        let input_for_sub = input.clone();
        cx.update(|_, cx| {
            cx.subscribe(&input, move |_input, event, cx| {
                if let InputEvent::Change = event {
                    seen_for_sub.set(Some(input_for_sub.read(cx).value().to_string()));
                }
            })
            .detach();
        });

        cx.update(|window, cx| {
            input.update(cx, |s, cx| {
                s.set_value("kick", window, cx);
                cx.emit(InputEvent::Change);
            });
        });
        cx.run_until_parked();

        assert_eq!(seen.take(), Some("kick".to_string()));
    }
}
