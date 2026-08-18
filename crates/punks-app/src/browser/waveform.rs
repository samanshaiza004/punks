//! Real-data waveform display: peaks from `SampleBrowser::waveform_peaks()`,
//! a playhead line driven by `playback_status()`, and click-to-seek.
//! Structurally identical to the viability spike's proven waveform
//! (`spikes/gpui-viability/src/main.rs`) -- background, bars, playhead,
//! click-to-seek -- except painting real audio data instead of synthetic
//! buckets.
//!
//! // ponytail: the spike's waveform also demoed a synthetic
//! // selection-range overlay (a fixed 10% highlight from the click point).
//! // Punks has no real audio-region-selection feature yet to back that with
//! // actual data, so it's left out here rather than inventing state with no
//! // product requirement. Upgrade path: once a region-selection feature
//! // exists on `SampleBrowser`, paint its range the same way the spike did.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{canvas, fill, point, px, size, Bounds, Context, MouseButton, MouseDownEvent, Pixels};

use super::MainWindow;
use crate::theme;
use crate::PlaybackStatus;

/// Fixed height for the waveform strip -- the bottom transport strip's other
/// element (`transport.rs`'s controls) sizes itself around this.
pub(super) const WAVEFORM_HEIGHT: f32 = 120.0;

impl MainWindow {
    pub(super) fn render_waveform(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let peaks = inner.waveform_peaks().cloned();
        let status = inner.playback_status();
        let position = match &status {
            PlaybackStatus::Playing { position, .. } => Some(*position),
            _ => None,
        };
        let duration = inner
            .current_track_info()
            .map(|t| t.source_duration)
            .or(match &status {
                PlaybackStatus::Playing { duration, .. } => Some(*duration),
                _ => None,
            });

        let bounds_cell: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
        let bounds_for_click = bounds_cell.clone();

        gpui::div()
            .id("waveform")
            .h(px(WAVEFORM_HEIGHT))
            .w_full()
            .rounded_md()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    let Some(duration) = duration else {
                        return;
                    };
                    let bounds = bounds_for_click.get();
                    if bounds.size.width <= px(0.) {
                        return;
                    }
                    let frac =
                        ((event.position.x - bounds.left()) / bounds.size.width).clamp(0.0, 1.0);
                    let target = Duration::from_secs_f64(duration.as_secs_f64() * frac as f64);
                    this.browser.update(cx, |b, cx| {
                        b.inner.seek_to(target);
                        b.ensure_playback_ticking(cx);
                        cx.notify();
                    });
                }),
            )
            .child(
                canvas(
                    move |bounds, _window, _cx| {
                        bounds_cell.set(bounds);
                    },
                    move |bounds, _prepaint, window, _cx| {
                        window.paint_quad(fill(bounds, gpui::rgb(theme::SURFACE_INSET)));

                        let Some(peaks) = &peaks else {
                            return;
                        };
                        if peaks.num_buckets == 0 {
                            return;
                        }
                        let mid_y = bounds.top() + bounds.size.height * 0.5;
                        let half_h = bounds.size.height * 0.5;
                        let bucket_w = bounds.size.width / peaks.num_buckets as f32;
                        for (i, (min, max)) in peaks.peaks.iter().enumerate() {
                            let x = bounds.left() + bucket_w * i as f32;
                            let y_top = mid_y - half_h * *max;
                            let y_bot = mid_y - half_h * *min;
                            window.paint_quad(fill(
                                Bounds::from_corners(
                                    point(x, y_top),
                                    point(x + bucket_w.max(px(1.)), y_bot),
                                ),
                                gpui::rgb(theme::WAVEFORM_BAR),
                            ));
                        }

                        if let (Some(position), Some(duration)) = (position, duration) {
                            if duration.as_secs_f64() > 0.0 {
                                let frac = (position.as_secs_f64() / duration.as_secs_f64())
                                    .clamp(0.0, 1.0)
                                    as f32;
                                let playhead_x = bounds.left() + bounds.size.width * frac;
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(playhead_x, bounds.top()),
                                        size(px(2.), bounds.size.height),
                                    ),
                                    gpui::rgb(theme::WAVEFORM_PLAYHEAD),
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}
