//! Controlled waveform presentation for the application shell.
//!
//! The caller owns decoded audio, full-source peaks, duration, and playback
//! position. This module owns only presentation and returns seek intent. It is
//! deliberately private until a second real consumer earns a public crate.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    canvas, fill, point, px, size, AccessibleAction, App, Bounds, Context, ElementId, FocusHandle,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseExitEvent, MouseMoveEvent, Pixels, RenderOnce,
    Rgba, Role, Window,
};

use super::MainWindow;
use crate::theme;
use crate::{PlaybackStatus, WaveformPeaks};

/// Fixed height for the waveform strip -- the bottom transport strip's other
/// element (`transport.rs`'s controls) sizes itself around this.
pub(super) const WAVEFORM_HEIGHT: f32 = 120.0;

/// One min/max amplitude envelope for a time bucket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Peak {
    pub min: f32,
    pub max: f32,
}

/// Immutable, validated presentation data for a full-duration waveform.
#[derive(Clone, Debug)]
pub(crate) struct WaveformData {
    peaks: Arc<[Peak]>,
    duration: Duration,
}

impl WaveformData {
    pub(crate) fn new(peaks: Arc<[Peak]>, duration: Duration) -> Result<Self, WaveformDataError> {
        validate_peaks(&peaks)?;
        Ok(Self { peaks, duration })
    }

    fn is_operable(&self) -> bool {
        !self.peaks.is_empty() && !self.duration.is_zero()
    }
}

fn validate_peaks(peaks: &[Peak]) -> Result<(), WaveformDataError> {
    for (index, peak) in peaks.iter().enumerate() {
        if !peak.min.is_finite() || !peak.max.is_finite() {
            return Err(WaveformDataError::NonFinite {
                index,
                min: peak.min,
                max: peak.max,
            });
        }
        if peak.min > peak.max {
            return Err(WaveformDataError::ReversedBounds {
                index,
                min: peak.min,
                max: peak.max,
            });
        }
        if peak.min < -1.0 || peak.max > 1.0 {
            return Err(WaveformDataError::OutOfRange {
                index,
                min: peak.min,
                max: peak.max,
            });
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WaveformDataError {
    NonFinite { index: usize, min: f32, max: f32 },
    ReversedBounds { index: usize, min: f32, max: f32 },
    OutOfRange { index: usize, min: f32, max: f32 },
}

impl fmt::Display for WaveformDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { index, min, max } => {
                write!(f, "peak {index} contains a non-finite value ({min}, {max})")
            }
            Self::ReversedBounds { index, min, max } => {
                write!(f, "peak {index} has reversed bounds ({min}, {max})")
            }
            Self::OutOfRange { index, min, max } => {
                write!(f, "peak {index} is outside [-1, 1] ({min}, {max})")
            }
        }
    }
}

impl std::error::Error for WaveformDataError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeekKey {
    Left,
    Right,
    Home,
    End,
}

enum SeekRequest {
    Pointer(f32),
    Key { key: SeekKey, shift: bool },
    SetValue(f64),
}

fn effective_position(position: Duration, duration: Duration, operable: bool) -> Duration {
    if operable {
        position.min(duration)
    } else {
        Duration::ZERO
    }
}

fn time_for_fraction(fraction: f32, duration: Duration, operable: bool) -> Duration {
    if !operable || duration.is_zero() {
        return Duration::ZERO;
    }
    Duration::from_secs_f64(duration.as_secs_f64() * fraction.clamp(0.0, 1.0) as f64)
}

fn seek_after_key(
    position: Duration,
    duration: Duration,
    key: SeekKey,
    shift: bool,
) -> Option<Duration> {
    if duration.is_zero() {
        return None;
    }

    let position = position.min(duration);
    let step_fraction = if shift { 0.10 } else { 0.01 };
    let step = Duration::from_secs_f64(duration.as_secs_f64() * step_fraction);
    Some(match key {
        SeekKey::Left => position.saturating_sub(step),
        SeekKey::Right => position.saturating_add(step).min(duration),
        SeekKey::Home => Duration::ZERO,
        SeekKey::End => duration,
    })
}

fn seek_after_set_value(value: f64, duration: Duration, operable: bool) -> Option<Duration> {
    if !operable || !value.is_finite() {
        return None;
    }
    Some(Duration::from_secs_f64(
        value.clamp(0.0, duration.as_secs_f64()),
    ))
}

fn seek_after_request(
    position: Duration,
    duration: Duration,
    operable: bool,
    request: SeekRequest,
) -> Option<Duration> {
    match request {
        SeekRequest::Pointer(fraction) => Some(time_for_fraction(fraction, duration, operable)),
        SeekRequest::Key { key, shift } => seek_after_key(position, duration, key, shift),
        SeekRequest::SetValue(value) => seek_after_set_value(value, duration, operable),
    }
}

fn format_hover_time(time: Duration) -> String {
    let total_millis = time.as_millis();
    let total_seconds = total_millis / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;
    let tenths = (total_millis % 1_000) / 100;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}.{tenths}")
    } else {
        format!("{minutes}:{seconds:02}.{tenths}")
    }
}

/// Collapse source envelopes to one preserved-extrema envelope per visible
/// column. When there are fewer source peaks than columns, source envelopes
/// are repeated across their proportional spans; no values are interpolated.
fn aggregate_peaks(peaks: &[Peak], columns: usize) -> Vec<Peak> {
    if peaks.is_empty() || columns == 0 {
        return Vec::new();
    }

    (0..columns)
        .map(|column| {
            let start = column * peaks.len() / columns;
            let end = ((column + 1) * peaks.len() / columns).max(start + 1);
            let end = end.min(peaks.len());
            let mut envelope = peaks[start];
            for peak in &peaks[start + 1..end] {
                envelope.min = envelope.min.min(peak.min);
                envelope.max = envelope.max.max(peak.max);
            }
            envelope
        })
        .collect()
}

#[derive(Default)]
struct AggregationCache {
    peaks_identity: usize,
    columns: usize,
    envelopes: Option<Arc<[Peak]>>,
    #[cfg(test)]
    computations: usize,
}

impl AggregationCache {
    fn get(&mut self, peaks: &Arc<[Peak]>, columns: usize) -> Arc<[Peak]> {
        let identity = Arc::as_ptr(peaks).cast::<Peak>() as usize;
        if self.peaks_identity == identity && self.columns == columns {
            if let Some(envelopes) = &self.envelopes {
                return envelopes.clone();
            }
        }

        let envelopes: Arc<[Peak]> = aggregate_peaks(peaks, columns).into();
        self.peaks_identity = identity;
        self.columns = columns;
        self.envelopes = Some(envelopes.clone());
        #[cfg(test)]
        {
            self.computations += 1;
        }
        envelopes
    }
}

struct WaveformState {
    focus_handle: FocusHandle,
    cache: AggregationCache,
    hover_fraction: Option<f32>,
}

impl WaveformState {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_stop(true),
            cache: AggregationCache::default(),
            hover_fraction: None,
        }
    }
}

/// A controlled, single-lane waveform element.
type SeekListener = Arc<dyn Fn(Duration, &mut App)>;

#[derive(IntoElement)]
pub(crate) struct Waveform {
    data: WaveformData,
    id: ElementId,
    position: Duration,
    height: Pixels,
    vertical_padding: Pixels,
    background: Rgba,
    wave_color: Rgba,
    progress_color: Rgba,
    playhead_color: Rgba,
    on_seek: Option<SeekListener>,
}

impl Waveform {
    fn new(data: WaveformData) -> Self {
        Self {
            data,
            id: ElementId::Name("waveform".into()),
            position: Duration::ZERO,
            height: px(WAVEFORM_HEIGHT),
            vertical_padding: px(0.0),
            background: gpui::rgb(theme::SURFACE_INSET),
            wave_color: gpui::rgb(theme::WAVEFORM_BAR),
            progress_color: gpui::rgb(theme::SELECTION),
            playhead_color: gpui::rgb(theme::WAVEFORM_PLAYHEAD),
            on_seek: None,
        }
    }

    fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    fn position(mut self, position: Duration) -> Self {
        self.position = position;
        self
    }

    fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    fn vertical_padding(mut self, padding: Pixels) -> Self {
        self.vertical_padding = padding;
        self
    }

    fn background(mut self, color: Rgba) -> Self {
        self.background = color;
        self
    }

    fn wave_color(mut self, color: Rgba) -> Self {
        self.wave_color = color;
        self
    }

    fn progress_color(mut self, color: Rgba) -> Self {
        self.progress_color = color;
        self
    }

    fn playhead_color(mut self, color: Rgba) -> Self {
        self.playhead_color = color;
        self
    }

    fn on_seek(mut self, listener: impl Fn(Duration, &mut App) + 'static) -> Self {
        self.on_seek = Some(Arc::new(listener));
        self
    }
}

impl RenderOnce for Waveform {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state =
            window.use_keyed_state(self.id.clone(), cx, |_window, cx| WaveformState::new(cx));
        let focus_handle = state.read(cx).focus_handle.clone();
        let operable = self.data.is_operable();
        let duration = self.data.duration;
        let position = effective_position(self.position, duration, operable);
        let hover_fraction = state.read(cx).hover_fraction;
        let hover_time = hover_fraction
            .filter(|_| operable)
            .map(|fraction| format_hover_time(time_for_fraction(fraction, duration, true)));
        let bounds_cell = std::rc::Rc::new(std::cell::Cell::new(Bounds::default()));

        let data_for_prepaint = self.data.clone();
        let state_for_prepaint = state.clone();
        let bounds_for_move = bounds_cell.clone();
        let bounds_for_click = bounds_cell.clone();
        let on_seek_for_click = self.on_seek.clone();
        let state_for_move = state.clone();
        let state_for_exit = state.clone();
        let data_for_keys = self.data.clone();
        let on_seek_for_keys = self.on_seek.clone();
        let on_seek_for_a11y = self.on_seek.clone();
        let on_seek_for_decrement = self.on_seek.clone();
        let on_seek_for_set_value = self.on_seek.clone();
        let background = self.background;
        let wave_color = self.wave_color;
        let progress_color = self.progress_color;
        let playhead_color = self.playhead_color;
        let vertical_padding = self.vertical_padding;
        let id = self.id.clone();

        let mut root = gpui::div()
            .id(id)
            .debug_selector(|| "waveform".into())
            .relative()
            .h(self.height)
            .w_full()
            .rounded_md()
            .child(
                canvas(
                    move |bounds, _window, cx| {
                        bounds_cell.set(bounds);
                        let columns = drawable_columns(bounds.size.width);
                        let envelopes = state_for_prepaint.update(cx, |state, _cx| {
                            state.cache.get(&data_for_prepaint.peaks, columns)
                        });
                        WaveformPaintState { envelopes }
                    },
                    move |bounds, paint_state, window, _cx| {
                        window.paint_quad(fill(bounds, background));
                        if paint_state.envelopes.is_empty() || duration.is_zero() {
                            return;
                        }

                        let pad = f32::from(vertical_padding).max(0.0);
                        let height = (f32::from(bounds.size.height) - pad * 2.0).max(0.0);
                        if height <= 0.0 {
                            return;
                        }
                        let inner = Bounds::new(
                            point(bounds.left(), bounds.top() + px(pad)),
                            size(bounds.size.width, px(height)),
                        );
                        let column_width = inner.size.width / paint_state.envelopes.len() as f32;
                        let position_fraction = position.as_secs_f64() / duration.as_secs_f64();
                        let progress_x = inner.left() + inner.size.width * position_fraction as f32;

                        for (column, peak) in paint_state.envelopes.iter().enumerate() {
                            let x0 = inner.left() + column_width * column as f32;
                            let x1 = if column + 1 == paint_state.envelopes.len() {
                                inner.right()
                            } else {
                                inner.left() + column_width * (column + 1) as f32
                            };
                            let y_top = inner.top() + inner.size.height * (1.0 - peak.max) * 0.5;
                            let y_bottom = inner.top() + inner.size.height * (1.0 - peak.min) * 0.5;
                            paint_peak(
                                window,
                                x0,
                                x1.min(progress_x),
                                y_top,
                                y_bottom,
                                progress_color,
                            );
                            paint_peak(window, x0.max(progress_x), x1, y_top, y_bottom, wave_color);
                        }

                        if let Some(fraction) = hover_fraction {
                            let x = inner.left() + inner.size.width * fraction;
                            paint_peak(
                                window,
                                x,
                                (x + px(1.0)).min(inner.right()),
                                inner.top(),
                                inner.bottom(),
                                playhead_color.opacity(0.55),
                            );
                        }

                        let playhead_x = inner.left()
                            + inner.size.width * position_fraction.clamp(0.0, 1.0) as f32;
                        paint_peak(
                            window,
                            playhead_x,
                            (playhead_x + px(2.0)).min(inner.right()),
                            inner.top(),
                            inner.bottom(),
                            playhead_color,
                        );
                    },
                )
                .size_full(),
            );

        if let Some(hover_time) = hover_time {
            root = root.child(
                gpui::div()
                    .absolute()
                    .top_1()
                    .right_1()
                    .text_color(gpui::rgb(theme::WAVEFORM_TEXT))
                    .text_sm()
                    .child(hover_time),
            );
        }

        if !operable {
            return root;
        }

        root.track_focus(&focus_handle)
            .cursor_pointer()
            .focus_visible(|style| style.border_1().border_color(playhead_color))
            .role(Role::Slider)
            .aria_label("Waveform position")
            .aria_value(format_hover_time(position))
            .aria_numeric_value(position.as_secs_f64())
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(duration.as_secs_f64())
            .aria_numeric_value_step(duration.as_secs_f64() * 0.01)
            .aria_orientation(gpui::Orientation::Horizontal)
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                let bounds = bounds_for_move.get();
                let fraction = fraction_at(bounds, event.position);
                state_for_move.update(cx, |state, cx| {
                    if state.hover_fraction != Some(fraction) {
                        state.hover_fraction = Some(fraction);
                        cx.notify();
                    }
                });
            })
            .on_mouse_exit(move |_event: &MouseExitEvent, _window, cx| {
                state_for_exit.update(cx, |state, cx| {
                    if state.hover_fraction.take().is_some() {
                        cx.notify();
                    }
                });
            })
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    let bounds = bounds_for_click.get();
                    let fraction = fraction_at(bounds, event.position);
                    let Some(target) = seek_after_request(
                        position,
                        duration,
                        operable,
                        SeekRequest::Pointer(fraction),
                    ) else {
                        return;
                    };
                    window.focus(&focus_handle, cx);
                    if let Some(on_seek) = &on_seek_for_click {
                        on_seek(target, cx);
                    }
                },
            )
            .on_key_down(move |event: &KeyDownEvent, _window, cx| {
                let key = match event.keystroke.key.as_str() {
                    "left" => Some(SeekKey::Left),
                    "right" => Some(SeekKey::Right),
                    "home" => Some(SeekKey::Home),
                    "end" => Some(SeekKey::End),
                    _ => None,
                };
                let Some(key) = key else {
                    return;
                };
                let Some(target) = seek_after_request(
                    position,
                    data_for_keys.duration,
                    operable,
                    SeekRequest::Key {
                        key,
                        shift: event.keystroke.modifiers.shift,
                    },
                ) else {
                    return;
                };
                if let Some(on_seek) = &on_seek_for_keys {
                    on_seek(target, cx);
                    cx.stop_propagation();
                }
            })
            .on_a11y_action(AccessibleAction::Increment, move |_data, _window, cx| {
                if let Some(target) = seek_after_request(
                    position,
                    duration,
                    operable,
                    SeekRequest::Key {
                        key: SeekKey::Right,
                        shift: false,
                    },
                ) {
                    if let Some(on_seek) = &on_seek_for_a11y {
                        on_seek(target, cx);
                    }
                }
            })
            .on_a11y_action(AccessibleAction::Decrement, move |_data, _window, cx| {
                if let Some(target) = seek_after_request(
                    position,
                    duration,
                    operable,
                    SeekRequest::Key {
                        key: SeekKey::Left,
                        shift: false,
                    },
                ) {
                    if let Some(on_seek) = &on_seek_for_decrement {
                        on_seek(target, cx);
                    }
                }
            })
            .on_a11y_action(AccessibleAction::SetValue, move |data, _window, cx| {
                let Some(gpui::accesskit::ActionData::NumericValue(value)) = data else {
                    return;
                };
                if !value.is_finite() {
                    return;
                }
                if let Some(target) =
                    seek_after_request(position, duration, operable, SeekRequest::SetValue(*value))
                {
                    if let Some(on_seek) = &on_seek_for_set_value {
                        on_seek(target, cx);
                    }
                }
            })
    }
}

struct WaveformPaintState {
    envelopes: Arc<[Peak]>,
}

fn drawable_columns(width: Pixels) -> usize {
    f32::from(width).max(0.0).floor() as usize
}

fn fraction_at(bounds: Bounds<Pixels>, position: gpui::Point<Pixels>) -> f32 {
    let width = f32::from(bounds.size.width);
    if width <= 0.0 {
        return 0.0;
    }
    ((f32::from(position.x) - f32::from(bounds.left())) / width).clamp(0.0, 1.0)
}

fn paint_peak(
    window: &mut Window,
    x0: Pixels,
    x1: Pixels,
    y_top: Pixels,
    y_bottom: Pixels,
    color: Rgba,
) {
    if x1 <= x0 || y_bottom <= y_top {
        return;
    }
    window.paint_quad(fill(
        Bounds::from_corners(point(x0, y_top), point(x1, y_bottom)),
        color,
    ));
}

#[derive(Default)]
pub(super) struct BrowserWaveformCache {
    // Keep the adapted Arc stable while the caller updates duration or axis
    // values for the same peak allocation; the element cache keys on this Arc.
    key: Option<BrowserWaveformCacheKey>,
    peaks: Option<Arc<[Peak]>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BrowserWaveformCacheKey {
    peaks_identity: usize,
    peaks_len: usize,
    num_buckets: usize,
}

impl BrowserWaveformCache {
    fn get(
        &mut self,
        peaks: Option<&WaveformPeaks>,
        axis: Option<(f64, f64)>,
        duration: Option<Duration>,
    ) -> Option<WaveformData> {
        if duration.is_some_and(|duration| duration.is_zero()) {
            return WaveformData::new(Arc::from([]), Duration::ZERO).ok();
        }
        let key = BrowserWaveformCacheKey {
            peaks_identity: peaks
                .map(|peaks| std::ptr::from_ref(peaks) as usize)
                .unwrap_or_default(),
            peaks_len: peaks.map_or(0, |peaks| peaks.peaks.len()),
            num_buckets: peaks.map_or(0, |peaks| peaks.num_buckets),
        };
        let full_duration = duration.is_some_and(|duration| full_axis_matches(axis, duration));
        if self.key != Some(key) || (full_duration && self.peaks.is_none()) {
            self.key = Some(key);
            self.peaks = full_duration
                .then(|| browser_peaks_from_browser(peaks, axis, duration))
                .flatten();
        }
        if !full_duration {
            return None;
        }
        self.peaks
            .clone()
            .zip(duration)
            .map(|(peaks, duration)| WaveformData { peaks, duration })
    }
}

fn browser_peaks_from_browser(
    peaks: Option<&WaveformPeaks>,
    axis: Option<(f64, f64)>,
    duration: Option<Duration>,
) -> Option<Arc<[Peak]>> {
    let duration = duration?;
    if duration.is_zero() {
        return None;
    }

    if !full_axis_matches(axis, duration) {
        return None;
    }

    let peaks = peaks?;
    if peaks.num_buckets != peaks.peaks.len() {
        return None;
    }
    let peaks: Arc<[Peak]> = peaks
        .peaks
        .iter()
        .map(|&(min, max)| Peak { min, max })
        .collect::<Vec<_>>()
        .into();
    validate_peaks(&peaks).ok()?;
    Some(peaks)
}

fn full_axis_matches(axis: Option<(f64, f64)>, duration: Duration) -> bool {
    let Some((start, represented_duration)) = axis else {
        return false;
    };
    let expected = duration.as_secs_f64();
    let tolerance = expected.max(1.0) * 1e-6;
    start.abs() <= tolerance && (represented_duration - expected).abs() <= tolerance
}

#[cfg(test)]
fn waveform_data_from_browser(
    peaks: Option<&WaveformPeaks>,
    axis: Option<(f64, f64)>,
    duration: Option<Duration>,
) -> Option<WaveformData> {
    let duration = duration?;
    if duration.is_zero() {
        return WaveformData::new(Arc::from([]), duration).ok();
    }
    let peaks = browser_peaks_from_browser(peaks, axis, Some(duration))?;
    WaveformData::new(peaks, duration).ok()
}

impl MainWindow {
    pub(super) fn render_waveform(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let status = inner.playback_status();
        let (status_position, status_duration) = match &status {
            PlaybackStatus::Playing {
                position, duration, ..
            } => (*position, Some(*duration)),
            _ => (Duration::ZERO, None),
        };
        let duration = inner
            .current_track_info()
            .map(|track| track.source_duration)
            .or(status_duration);
        let position = status_position;
        let data = self
            .waveform_cache
            .get(inner.waveform_peaks(), inner.waveform_axis(), duration)
            .or_else(|| {
                duration.and_then(|duration| WaveformData::new(Arc::from([]), duration).ok())
            })
            .unwrap_or_else(|| WaveformData::new(Arc::from([]), Duration::ZERO).unwrap());

        let browser = self.browser.clone();
        Waveform::new(data)
            .id("main-waveform")
            .height(px(WAVEFORM_HEIGHT))
            .position(position)
            .vertical_padding(px(0.0))
            .background(gpui::rgb(theme::SURFACE_INSET))
            .wave_color(gpui::rgb(theme::WAVEFORM_BAR))
            .progress_color(gpui::rgb(theme::SELECTION))
            .playhead_color(gpui::rgb(theme::WAVEFORM_PLAYHEAD))
            .on_seek(move |target, cx| {
                browser.update(cx, |browser, cx| {
                    browser.inner.seek_to(target);
                    browser.ensure_playback_ticking(cx);
                    cx.notify();
                });
            })
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    fn data(peaks: &[(f32, f32)], duration: Duration) -> WaveformData {
        let peaks: Arc<[Peak]> = peaks
            .iter()
            .map(|&(min, max)| Peak { min, max })
            .collect::<Vec<_>>()
            .into();
        WaveformData::new(peaks, duration).unwrap()
    }

    #[test]
    fn data_rejects_malformed_peaks_with_indexed_errors() {
        let peaks: Arc<[Peak]> = vec![
            Peak { min: 0.0, max: 0.5 },
            Peak {
                min: f32::NAN,
                max: 0.5,
            },
        ]
        .into();
        assert!(matches!(
            WaveformData::new(peaks, Duration::from_secs(1)),
            Err(WaveformDataError::NonFinite { index: 1, .. })
        ));
    }

    #[test]
    fn empty_and_zero_duration_data_are_valid_inert_states() {
        assert!(!data(&[], Duration::from_secs(1)).is_operable());
        assert!(!data(&[(0.0, 1.0)], Duration::ZERO).is_operable());
        assert_eq!(
            effective_position(Duration::from_secs(3), Duration::ZERO, false),
            Duration::ZERO
        );
    }

    #[test]
    fn full_duration_mapping_and_position_clamping_are_consistent() {
        let duration = Duration::from_secs(100);
        assert_eq!(time_for_fraction(0.0, duration, true), Duration::ZERO);
        assert_eq!(
            time_for_fraction(0.25, duration, true),
            Duration::from_secs(25)
        );
        assert_eq!(time_for_fraction(2.0, duration, true), duration);
        assert_eq!(
            effective_position(Duration::from_secs(101), duration, true),
            duration
        );
    }

    #[test]
    fn keyboard_seek_uses_one_and_ten_percent_steps_and_clamps() {
        let duration = Duration::from_secs(100);
        assert_eq!(
            seek_after_key(Duration::from_secs(50), duration, SeekKey::Left, false),
            Some(Duration::from_secs(49))
        );
        assert_eq!(
            seek_after_key(Duration::from_secs(50), duration, SeekKey::Right, true),
            Some(Duration::from_secs(60))
        );
        assert_eq!(
            seek_after_key(Duration::from_secs(50), duration, SeekKey::Home, false),
            Some(Duration::ZERO)
        );
        assert_eq!(
            seek_after_key(Duration::from_secs(50), duration, SeekKey::End, false),
            Some(duration)
        );
        assert_eq!(
            seek_after_key(duration, duration, SeekKey::Right, true),
            Some(duration)
        );
    }

    #[test]
    fn aggregation_preserves_extrema_and_distributes_short_input() {
        let source = [
            Peak {
                min: -0.2,
                max: 0.3,
            },
            Peak {
                min: -0.9,
                max: 0.4,
            },
            Peak {
                min: -0.1,
                max: 0.8,
            },
            Peak {
                min: -0.7,
                max: 0.2,
            },
        ];
        assert_eq!(
            aggregate_peaks(&source, 2),
            vec![
                Peak {
                    min: -0.9,
                    max: 0.4
                },
                Peak {
                    min: -0.7,
                    max: 0.8
                },
            ]
        );

        let short = [Peak {
            min: -0.5,
            max: 0.5,
        }];
        assert_eq!(aggregate_peaks(&short, 3), vec![short[0]; 3]);
    }

    #[test]
    fn hover_time_uses_one_decimal_and_switches_at_one_hour() {
        assert_eq!(format_hover_time(Duration::from_millis(0)), "0:00.0");
        assert_eq!(
            format_hover_time(Duration::from_millis(3 * 60 * 1000 + 427)),
            "3:00.4"
        );
        assert_eq!(
            format_hover_time(Duration::from_secs(60 * 60 + 43 * 60 + 8)),
            "1:43:08.0"
        );
    }

    #[test]
    fn browser_adapter_withholds_windowed_peaks() {
        let peaks = WaveformPeaks {
            peaks: vec![(-0.5, 0.5); 4],
            num_buckets: 4,
        };
        let duration = Duration::from_secs(120);
        assert!(waveform_data_from_browser(
            Some(&peaks),
            Some((120.0, 120.0)),
            Some(Duration::from_secs(600)),
        )
        .is_none());
        assert!(waveform_data_from_browser(
            Some(&peaks),
            Some((0.0, 600.0)),
            Some(Duration::from_secs(600)),
        )
        .is_some());
        assert!(
            waveform_data_from_browser(Some(&peaks), Some((0.0, 120.0)), Some(duration)).is_some()
        );
    }

    #[test]
    fn aggregation_cache_reuses_position_and_duration_changes() {
        let peaks: Arc<[Peak]> = vec![
            Peak {
                min: -0.5,
                max: 0.5
            };
            4
        ]
        .into();
        let mut cache = AggregationCache::default();
        let _ = cache.get(&peaks, 2);
        let _ = effective_position(Duration::from_secs(2), Duration::from_secs(1), true);
        let _ = cache.get(&peaks, 2);
        assert_eq!(cache.computations, 1);

        let _ = cache.get(&peaks, 3);
        assert_eq!(cache.computations, 2);
        let replacement: Arc<[Peak]> = peaks.iter().copied().collect::<Vec<_>>().into();
        let _ = cache.get(&replacement, 3);
        assert_eq!(cache.computations, 3);
    }

    #[test]
    fn browser_adapter_cache_preserves_peak_allocation_identity() {
        let peaks = WaveformPeaks {
            peaks: vec![(-0.5, 0.5); 4],
            num_buckets: 4,
        };
        let duration = Duration::from_secs(10);
        let mut cache = BrowserWaveformCache::default();
        let first = cache
            .get(Some(&peaks), Some((0.0, 10.0)), Some(duration))
            .unwrap();
        let second = cache
            .get(Some(&peaks), Some((0.0, 10.0)), Some(duration))
            .unwrap();
        assert!(Arc::ptr_eq(&first.peaks, &second.peaks));
        let changed_duration = cache
            .get(
                Some(&peaks),
                Some((0.0, 20.0)),
                Some(Duration::from_secs(20)),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&first.peaks, &changed_duration.peaks));
    }

    #[test]
    fn accessible_set_value_uses_clamped_controlled_time() {
        let duration = Duration::from_secs(100);
        assert_eq!(
            seek_after_set_value(25.0, duration, true),
            Some(Duration::from_secs(25))
        );
        assert_eq!(
            seek_after_set_value(-1.0, duration, true),
            Some(Duration::ZERO)
        );
        assert_eq!(seek_after_set_value(101.0, duration, true), Some(duration));
        assert_eq!(seek_after_set_value(f64::NAN, duration, true), None);
        assert_eq!(seek_after_set_value(25.0, duration, false), None);
    }
}

#[cfg(test)]
mod gpui_tests {
    // GPUI's headless test context does not activate or expose an AccessKit
    // tree. Semantic role/value wiring is therefore kept in the element code
    // and checked through the pure seek reducer tests below; assistive
    // technology inspection remains a manual platform check.
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{point, Modifiers, MouseButton, Render, TestAppContext, Window};

    use super::*;

    struct TestWaveformView {
        data: WaveformData,
        position: Duration,
        seeks: Rc<RefCell<Vec<Duration>>>,
    }

    impl Render for TestWaveformView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let seeks = self.seeks.clone();
            Waveform::new(self.data.clone())
                .id("test-waveform")
                .position(self.position)
                .on_seek(move |time, _cx| seeks.borrow_mut().push(time))
        }
    }

    fn test_data() -> WaveformData {
        WaveformData::new(
            vec![
                Peak {
                    min: -1.0,
                    max: 1.0
                };
                8
            ]
            .into(),
            Duration::from_secs(100),
        )
        .unwrap()
    }

    #[gpui::test]
    fn click_focuses_waveform_and_keyboard_seeking_uses_same_output(cx: &mut TestAppContext) {
        let seeks = Rc::new(RefCell::new(Vec::new()));
        let (view, cx) = cx.add_window_view({
            let seeks = seeks.clone();
            move |_, _| TestWaveformView {
                data: test_data(),
                position: Duration::from_secs(50),
                seeks,
            }
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let bounds = cx.debug_bounds("waveform").unwrap();
        cx.simulate_mouse_move(bounds.center(), None::<MouseButton>, Modifiers::default());
        assert!(seeks.borrow().is_empty());
        let quarter = point(bounds.left() + bounds.size.width * 0.25, bounds.center().y);
        cx.simulate_click(quarter, gpui::Modifiers::default());
        assert_eq!(seeks.borrow().as_slice(), &[Duration::from_secs(25)]);

        // A controlled component only sees the new position after its caller
        // accepts the seek intent and renders it back.
        view.update(cx, |view, cx| {
            view.position = Duration::from_secs(25);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        cx.simulate_keystrokes("right shift-left");
        assert_eq!(
            seeks.borrow().as_slice(),
            &[
                Duration::from_secs(25),
                Duration::from_secs(26),
                Duration::from_secs(15),
            ]
        );
    }

    #[gpui::test]
    fn inert_waveform_never_focuses_or_emits_seek(cx: &mut TestAppContext) {
        let seeks = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let seeks = seeks.clone();
            move |_, _| TestWaveformView {
                data: WaveformData::new(Arc::from([]), Duration::from_secs(100)).unwrap(),
                position: Duration::from_secs(50),
                seeks,
            }
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let bounds = cx.debug_bounds("waveform").unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.simulate_keystrokes("right");
        cx.update(|window, cx| window.focus_next(cx));

        assert!(seeks.borrow().is_empty());
        cx.update(|window, cx| assert!(window.focused(cx).is_none()));
    }
}
