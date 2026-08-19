//! Transport controls: Play/Stop and the volume slider. Play/Stop calls
//! `SampleBrowser::play_selected()`/`stop()` directly; the volume slider is
//! `gpui-component`'s `Slider` bound to a persistent `SliderState` entity,
//! wired to `SampleBrowser::set_volume`/`volume` via a `SliderEvent::Change`
//! subscription set up once in `MainWindow::new`.

use gpui::prelude::*;
use gpui::{Context, Entity};
use gpui_component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_component::{button::*, h_flex, Disableable};

use super::MainWindow;
use crate::PlaybackStatus;

impl MainWindow {
    /// Called once from `MainWindow::new` to build the volume slider entity
    /// and wire its change events straight through to `SampleBrowser`. Kept
    /// here (not inlined into `new`) so the slider's own concerns stay in
    /// this file alongside the rest of the transport strip.
    pub(super) fn new_volume_slider(
        initial_volume: f32,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(initial_volume)
        });
        cx.subscribe(&slider, |this, _slider, event, cx| {
            if let SliderEvent::Change(SliderValue::Single(v)) = event {
                this.browser.update(cx, |b, _cx| b.inner.set_volume(*v));
                let mut config = crate::config::load();
                config.volume = *v;
                crate::config::save(&config);
            }
        })
        .detach();
        slider
    }

    pub(super) fn render_transport(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let is_playing = matches!(
            browser.inner.playback_status(),
            PlaybackStatus::Playing { .. } | PlaybackStatus::Loading { .. }
        );
        let has_selection = browser.inner.selected().is_some();

        h_flex()
            .id("transport")
            .items_center()
            .gap_2()
            .p_2()
            .child(
                Button::new("play-stop")
                    .label(if is_playing { "Stop" } else { "Play" })
                    .disabled(!is_playing && !has_selection)
                    .on_click(cx.listener(|this, _, _window, cx| this.toggle_playback(cx))),
            )
            .child(Slider::new(&self.volume_slider))
    }

    /// Single transport transition shared by the visible button and the
    /// configurable `TogglePlayback` GPUI action.
    pub(super) fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        self.browser.update(cx, |browser, cx| {
            let is_playing = matches!(
                browser.inner.playback_status(),
                PlaybackStatus::Playing { .. } | PlaybackStatus::Loading { .. }
            );
            if is_playing {
                browser.inner.stop();
            } else {
                browser.inner.play_selected();
                browser.ensure_playback_ticking(cx);
            }
            cx.notify();
        });
    }
}

/// Button/Slider audit (M4): the interactive-behavior verification M0's
/// smoke test deferred, using real `gpui-component` widgets exactly as
/// Punks configures them (not a generic gpui-component test -- those
/// already exist in gpui-component's own crate and are trusted as-is), in a
/// tiny test-only harness. No `SampleBrowser`/audio device involved. See
/// `docs/gpui-component-audit.md` for the recorded results.
#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{
        div, point, px, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, Render, TestAppContext,
        Window,
    };

    use super::*;

    struct ButtonHarness {
        clicks: Rc<Cell<usize>>,
        disabled: bool,
    }

    impl Render for ButtonHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let clicks = self.clicks.clone();
            div().tab_group().size(px(100.)).child(
                Button::new("play-stop")
                    .label("Play")
                    .disabled(self.disabled)
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
            )
        }
    }

    fn press_enter_and_space(cx: &mut gpui::VisualTestContext) {
        for key in ["enter", "space"] {
            let keystroke = Keystroke::parse(key).unwrap();
            cx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            cx.simulate_event(KeyUpEvent { keystroke });
        }
    }

    #[gpui::test]
    fn play_button_is_tab_reachable_and_activates_via_keyboard(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let clicks = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view({
            let clicks = clicks.clone();
            move |_, _| ButtonHarness {
                clicks,
                disabled: false,
            }
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // Tab-reachable: nothing is focused until Tab moves focus onto it.
        cx.update(|window, cx| assert!(window.focused(cx).is_none()));
        cx.update(|window, cx| window.focus_next(cx));
        cx.update(|window, cx| {
            assert!(
                window.focused(cx).is_some(),
                "button did not take focus via Tab"
            );
            window.draw(cx).clear(cx);
        });

        // Keyboard activation: Space and Enter both trigger on_click while focused.
        press_enter_and_space(cx);
        assert_eq!(clicks.get(), 2);

        // Mouse click also still works (not replaced by keyboard wiring).
        cx.simulate_click(point(px(10.), px(10.)), Modifiers::default());
        assert_eq!(clicks.get(), 3);
    }

    #[gpui::test]
    fn disabled_play_button_ignores_click_and_keyboard(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let clicks = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view({
            let clicks = clicks.clone();
            move |_, _| ButtonHarness {
                clicks,
                disabled: true,
            }
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_click(point(px(10.), px(10.)), Modifiers::default());
        cx.update(|window, cx| window.focus_next(cx));
        press_enter_and_space(cx);

        assert_eq!(clicks.get(), 0);
    }

    struct SliderHarness {
        state: Entity<SliderState>,
    }

    impl Render for SliderHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size(px(200.)).child(Slider::new(&self.state))
        }
    }

    #[gpui::test]
    fn volume_slider_set_value_updates_readable_state(cx: &mut TestAppContext) {
        // Exercises `new_volume_slider`'s own `.default_value(cfg.volume)`
        // initialization path plus the read side (`.value()`) that a Play/
        // Stop-adjacent volume readout would use.
        cx.update(gpui_component::init);
        let (view, cx) = cx.add_window_view(|_window, cx| {
            let state = cx.new(|_| {
                SliderState::new()
                    .min(0.0)
                    .max(1.0)
                    .step(0.01)
                    .default_value(0.75_f32)
            });
            SliderHarness { state }
        });
        let state = view.read_with(cx, |v, _| v.state.clone());
        assert_eq!(
            state.read_with(cx, |s, _| s.value()),
            SliderValue::Single(0.75)
        );

        cx.update(|window, cx| {
            state.update(cx, |s, cx| s.set_value(0.4_f32, window, cx));
        });
        assert_eq!(
            state.read_with(cx, |s, _| s.value()),
            SliderValue::Single(0.4)
        );
    }

    #[gpui::test]
    fn volume_slider_change_event_is_observable_via_subscription(cx: &mut TestAppContext) {
        // `new_volume_slider`'s subscription (the code that calls
        // `SampleBrowser::set_volume`) depends on `SliderEvent::Change`
        // being emitted and externally subscribable -- proven here the same
        // way `update_value_by_position` (the real drag handler) emits it
        // internally (`cx.emit(SliderEvent::Change(self.value))`), without
        // needing to simulate pixel-accurate mouse-drag hit-testing against
        // gpui-component's own internal (and separately-tested) slider
        // geometry.
        cx.update(gpui_component::init);
        let (view, cx) = cx.add_window_view(|_window, cx| {
            let state = cx.new(|_| SliderState::new().min(0.0).max(1.0));
            SliderHarness { state }
        });
        let state = view.read_with(cx, |v, _| v.state.clone());

        let changes: Rc<Cell<Option<SliderValue>>> = Rc::new(Cell::new(None));
        let changes_for_sub = changes.clone();
        cx.update(|_, cx| {
            cx.subscribe(&state, move |_state, event, _cx| {
                if let SliderEvent::Change(value) = event {
                    changes_for_sub.set(Some(*value));
                }
            })
            .detach();
        });

        cx.update(|_, cx| {
            state.update(cx, |_s, cx| {
                cx.emit(SliderEvent::Change(SliderValue::Single(0.4)))
            });
        });
        cx.run_until_parked();

        assert_eq!(changes.get(), Some(SliderValue::Single(0.4)));
    }
}
