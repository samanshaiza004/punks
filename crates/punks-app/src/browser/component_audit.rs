//! M5 remaining-component audit (Checkbox, Select, Tooltip, Popover,
//! Dialog -- Button/Slider were finalized in M4, Input in `search.rs`).
//! Test-only entities wrapping real `gpui-component` widgets, same pattern
//! as M3's `TestResultsView` and M4's Button/Slider audit. No production
//! code lives here -- this whole module is `#[cfg(test)]`-gated from
//! `browser/mod.rs`, since none of these five components have a real Punks
//! consumer yet (see `docs/gpui-component-audit.md`'s M5 table for what
//! that means for coverage depth on Select/Tooltip/Dialog specifically).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{div, point, px, Context, Render, TestAppContext, Window};
use gpui_component::button::*;
use gpui_component::checkbox::Checkbox;
use gpui_component::popover::Popover;

struct CheckboxHarness {
    checked: Rc<Cell<bool>>,
    clicks: Rc<Cell<usize>>,
}

impl Render for CheckboxHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let checked = self.checked.clone();
        let clicks = self.clicks.clone();
        div().size(px(100.)).child(
            Checkbox::new("audit-checkbox")
                .checked(self.checked.get())
                .on_click(move |next, _window, _cx| {
                    checked.set(*next);
                    clicks.set(clicks.get() + 1);
                }),
        )
    }
}

#[gpui::test]
fn checkbox_click_toggles_and_reports_the_next_state(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let checked = Rc::new(Cell::new(false));
    let clicks = Rc::new(Cell::new(0));
    let (_view, cx) = cx.add_window_view({
        let checked = checked.clone();
        let clicks = clicks.clone();
        move |_, _| CheckboxHarness { checked, clicks }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    cx.simulate_click(point(px(10.), px(10.)), Default::default());
    assert!(checked.get());
    assert_eq!(clicks.get(), 1);

    cx.simulate_click(point(px(10.), px(10.)), Default::default());
    assert!(!checked.get());
    assert_eq!(clicks.get(), 2);
}

struct PopoverHarness {
    changes: Rc<RefCell<Vec<bool>>>,
}

impl Render for PopoverHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let changes = self.changes.clone();
        Popover::new("audit-popover")
            .trigger(
                Button::new("audit-popover-trigger")
                    .label("Open")
                    .size(px(100.)),
            )
            .content(|_, _, _| {
                div()
                    .debug_selector(|| "audit-popover-content".into())
                    .size(px(40.))
            })
            .on_open_change(move |open, _, _| changes.borrow_mut().push(*open))
    }
}

#[gpui::test]
fn popover_opens_on_trigger_click_and_dismisses_on_outside_click(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (_view, cx) = cx.add_window_view({
        let changes = changes.clone();
        move |_, _| PopoverHarness { changes }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // Click the trigger (top-left of the window, where the harness's only
    // button is): the popover opens.
    cx.simulate_click(point(px(20.), px(20.)), Default::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("audit-popover-content").is_some(),
        "popover content did not render after trigger click"
    );

    // Click well outside both the trigger and the popover content: it dismisses.
    cx.simulate_click(point(px(300.), px(300.)), Default::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("audit-popover-content").is_none(),
        "popover content still rendered after an outside click"
    );
    assert!(changes.borrow().first().copied() == Some(true));
    assert!(changes.borrow().last().copied() == Some(false));
}
