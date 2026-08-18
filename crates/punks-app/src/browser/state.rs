//! Wraps [`SampleBrowser`] (pure domain logic, unchanged) as a GPUI
//! [`gpui::Entity`], and bridges its background-worker polling into GPUI's
//! event-driven render model.

use std::time::Duration;

use gpui::Context;

use crate::{BrowserError, PlaybackStatus, PunksConfig, SampleBrowser};

/// The application's one [`SampleBrowser`], plus the bookkeeping needed to
/// drive its `poll()` from GPUI without busy-looping.
pub struct Browser {
    pub inner: SampleBrowser,
    poll_driver_running: bool,
    playback_ticker_running: bool,
}

impl Browser {
    pub fn new(cfg: &PunksConfig) -> Result<Self, BrowserError> {
        Ok(Self {
            inner: SampleBrowser::new(cfg)?,
            poll_driver_running: false,
            playback_ticker_running: false,
        })
    }

    /// Starts polling `inner` on a short timer if it isn't already running.
    /// Call this after any action that might kick off background work
    /// (navigate, search, scan, tag/library operations, ...). The driver
    /// stops itself once `SampleBrowser::poll` reports nothing is pending —
    /// see that method's doc comment for exactly what it tracks.
    ///
    /// `cx.notify()` is called only on ticks where `poll()` reports
    /// `changed`, never unconditionally: an unconditional per-tick notify
    /// would just recreate the continuous low-rate render loop this whole
    /// bridge exists to avoid.
    pub fn ensure_polling(&mut self, cx: &mut Context<Self>) {
        if self.poll_driver_running {
            return;
        }
        self.poll_driver_running = true;

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

                let still_pending = this.update(cx, |this, cx| {
                    let outcome = this.inner.poll();
                    if outcome.changed {
                        cx.notify();
                    }
                    if !outcome.pending {
                        this.poll_driver_running = false;
                    }
                    outcome.pending
                });

                match still_pending {
                    Ok(true) => continue,
                    // `Ok(false)`: settled. `Err`: the entity (window) is
                    // gone, nothing left to poll for.
                    Ok(false) | Err(_) => break,
                }
            }
        })
        .detach();
    }

    /// Starts a ~33ms presentation timer while audio is genuinely playing,
    /// separate from [`Self::ensure_polling`]'s 100ms background-completion
    /// poller: that one drains five worker channels and only notifies on a
    /// real change, which would mean ~10 FPS waveform motion if reused here.
    /// `SampleBrowser::playback_status()` is a lock-free atomic read (no
    /// channel drain, no side effect -- see its doc comment), so this timer
    /// can call it every tick and notify unconditionally while playing,
    /// without touching analysis/search/scan/health/peaks at all. Call this
    /// the instant Play begins (or a row is auditioned); it self-terminates
    /// the moment `playback_status()` is no longer `Playing`/`Loading`.
    pub fn ensure_playback_ticking(&mut self, cx: &mut Context<Self>) {
        if self.playback_ticker_running {
            return;
        }
        self.playback_ticker_running = true;

        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(33))
                .await;

            let still_playing = this.update(cx, |this, cx| {
                let playing = matches!(
                    this.inner.playback_status(),
                    PlaybackStatus::Playing { .. } | PlaybackStatus::Loading { .. }
                );
                if playing {
                    cx.notify();
                } else {
                    this.playback_ticker_running = false;
                }
                playing
            });

            match still_playing {
                Ok(true) => continue,
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }
}
