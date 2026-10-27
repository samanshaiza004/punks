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
        let has_selection = browser.inner.has_playable_selection();

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
                browser.ensure_playback_drivers(cx);
            }
            cx.notify();
        });
    }
}
