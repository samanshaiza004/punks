//! Wraps [`SampleBrowser`] (pure domain logic, unchanged) as a GPUI
//! [`gpui::Entity`], and bridges its background-worker polling into GPUI's
//! event-driven render model.

use std::time::Duration;

use gpui::Context;

use crate::{BrowserError, PunksConfig, SampleBrowser};

/// The application's one [`SampleBrowser`], plus the bookkeeping needed to
/// drive its `poll()` from GPUI without busy-looping.
pub struct Browser {
    pub inner: SampleBrowser,
    poll_driver_running: bool,
}

impl Browser {
    pub fn new(cfg: &PunksConfig) -> Result<Self, BrowserError> {
        Ok(Self {
            inner: SampleBrowser::new(cfg)?,
            poll_driver_running: false,
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
}
