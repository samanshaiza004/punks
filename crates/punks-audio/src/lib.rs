use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use lru::LruCache;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

mod analysis;
mod decode;
pub mod metadata;
pub mod peaks;
mod resample;

pub use analysis::{
    amp_to_dbfs, pipeline_version, run_all, AnalysisContext, AnalysisReport, AudioBuffer,
};
pub use decode::{compute_source_peaks, decode_file, AudioMetadata, DecodedAudio};
pub use metadata::{
    resolve, Backend, Capability, Field, Metadata, MetadataBackend, MetadataError, MetadataSource,
    ResolvedMetadata, Sourced,
};
pub use peaks::WaveformPeaks;

/// Container-level info about the currently loaded track: free-text metadata,
/// its true source length, and whether only a preview window was decoded.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub metadata: AudioMetadata,
    pub source_sample_rate: u32,
    pub source_duration: Duration,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub enum PlaybackStatus {
    Idle,
    Loading {
        file: PathBuf,
    },
    Playing {
        file: PathBuf,
        position: Duration,
        duration: Duration,
    },
}

#[derive(Debug)]
pub enum PlaybackError {
    DecodeError(String),
    DeviceError(String),
    UnsupportedFormat,
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaybackError::DecodeError(e) => write!(f, "decode error: {e}"),
            PlaybackError::DeviceError(e) => write!(f, "device error: {e}"),
            PlaybackError::UnsupportedFormat => write!(f, "unsupported audio format"),
        }
    }
}

impl std::error::Error for PlaybackError {}

struct SharedState {
    /// Raw pointer to the control-owned published buffer. The callback only
    /// borrows this pointer between `callback_users` increment/decrement;
    /// `PlaybackEngine` retains the owning `Box` until that count is zero.
    published: AtomicPtr<PublishedAudio>,
    volume: AtomicU32,
    callback_users: AtomicUsize,
    acknowledged_generation: AtomicU64,
    stream_failed: AtomicBool,
}

struct DecodeTarget {
    sample_rate: AtomicU32,
    channels: AtomicUsize,
}

#[derive(Clone)]
struct PreparedAudio {
    samples: Vec<f32>,
    total_frames: usize,
    file: PathBuf,
    peaks: WaveformPeaks,
    info: TrackInfo,
    /// Where in the source this buffer starts (0 for a from-the-start preview).
    window_start: Duration,
}

/// The published sample object visible to the CPAL callback. Its sample data
/// and metadata are immutable; only its per-buffer atomics change during
/// playback. Its `Arc` is cloned and dropped only by the control side.
struct PublishedAudio {
    generation: u64,
    audio: Arc<PreparedAudio>,
    cursor: AtomicUsize,
    playing: AtomicBool,
}

impl SharedState {
    fn new() -> Self {
        Self {
            published: AtomicPtr::new(std::ptr::null_mut()),
            volume: AtomicU32::new(1.0f32.to_bits()),
            callback_users: AtomicUsize::new(0),
            acknowledged_generation: AtomicU64::new(0),
            stream_failed: AtomicBool::new(false),
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: &Arc<SharedState>,
) -> Result<cpal::Stream, PlaybackError> {
    let callback_shared = Arc::clone(shared);
    let error_shared = Arc::clone(shared);
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                audio_callback(data, &callback_shared);
            },
            move |_err| {
                // CPAL invokes this outside the data callback. Keep it to an
                // atomic fault signal; control-side `poll()` owns recovery and
                // user-visible error creation.
                error_shared.stream_failed.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;
    stream
        .play()
        .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;
    Ok(stream)
}

/// A "latest wins" single-slot mailbox: `send` replaces whatever is waiting
/// (if anything), `recv` blocks until a value is available. This coalesces
/// rapid requests into a single persistent worker thread — if several `send`s
/// land before the worker is free, only the most recent one is ever received.
/// Reused by both the decode worker (here) and the browser's peaks worker.
pub struct RequestSlot<T> {
    slot: Mutex<Option<T>>,
    cv: Condvar,
}

impl<T> RequestSlot<T> {
    pub fn new() -> Self {
        RequestSlot {
            slot: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    pub fn send(&self, value: T) {
        let mut guard = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(value);
        self.cv.notify_one();
    }

    pub fn recv(&self) -> T {
        let mut guard = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(value) = guard.take() {
                return value;
            }
            guard = self.cv.wait(guard).unwrap_or_else(|e| e.into_inner());
        }
    }
}

impl<T> Default for RequestSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}

const CACHE_CAPACITY: usize = 10;
/// Length of an on-demand window decoded when scrubbing past the loaded region
/// of a long file. Matches the initial preview window for a consistent feel.
const WINDOW: Duration = Duration::from_secs(120);

/// What the decode worker should produce.
enum DecodeRequest {
    /// From-the-start preview (or the whole file, if short).
    Full(PathBuf),
    /// A bounded window starting `start` into the source (scrub target).
    Window {
        path: PathBuf,
        start: Duration,
    },
    Shutdown,
}

impl DecodeRequest {
    fn path(&self) -> &Path {
        match self {
            DecodeRequest::Full(p) => p,
            DecodeRequest::Window { path, .. } => path,
            DecodeRequest::Shutdown => unreachable!("shutdown has no path"),
        }
    }
    fn window(&self) -> Option<Duration> {
        match self {
            DecodeRequest::Full(_) => None,
            DecodeRequest::Window { start, .. } => Some(*start),
            DecodeRequest::Shutdown => None,
        }
    }
}

/// A decode we're awaiting: its request id (to drop results superseded by a
/// later request) and the file, for `status()`'s Loading state.
struct PendingReq {
    id: u64,
    file: PathBuf,
}

pub struct PlaybackEngine {
    shared: Arc<SharedState>,
    stream: Option<cpal::Stream>,
    device_sample_rate: u32,
    device_channels: u16,
    current_file: Option<PathBuf>,
    current_peaks: Option<WaveformPeaks>,
    /// Full-source waveform (path it belongs to + peaks), computed in the
    /// background for long files. Preferred over `current_peaks` when it's for
    /// the current file; lets the waveform show the whole source, not just the
    /// loaded window.
    current_full_peaks: Option<(PathBuf, WaveformPeaks)>,
    current_info: Option<TrackInfo>,
    /// Where the loaded buffer starts in the source (0 unless a window was
    /// decoded on demand).
    current_window_start: Duration,
    active_buffer: Option<Box<PublishedAudio>>,
    retired_buffer: Option<Box<PublishedAudio>>,
    pending_buffer: Option<Arc<PreparedAudio>>,
    next_buffer_generation: u64,
    /// The decode we're awaiting, if any.
    pending: Option<PendingReq>,
    next_request_id: u64,
    cache: LruCache<PathBuf, Arc<PreparedAudio>>,
    decode_request: Arc<RequestSlot<(u64, DecodeRequest)>>,
    decode_target: Arc<DecodeTarget>,
    decode_result_rx: Option<mpsc::Receiver<(u64, Result<PreparedAudio, PlaybackError>)>>,
    decode_thread: Option<std::thread::JoinHandle<()>>,
    recovery_attempted: bool,
    pending_stream_error: Option<PlaybackError>,
}

impl PlaybackEngine {
    pub fn new() -> Result<Self, PlaybackError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| PlaybackError::DeviceError("no output device found".into()))?;

        let supported_config = device
            .default_output_config()
            .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;

        if supported_config.sample_format() != SampleFormat::F32 {
            return Err(PlaybackError::UnsupportedFormat);
        }

        let sample_rate = supported_config.sample_rate();
        let channels = supported_config.channels();

        let config: StreamConfig = supported_config.into();

        let shared = Arc::new(SharedState::new());
        let stream = build_stream(&device, &config, &shared)?;

        // One persistent decode worker for the engine's lifetime, instead of a
        // thread per play() call. Rapid navigation (holding W/S) now coalesces
        // into a single in-flight decode via RequestSlot rather than spawning
        // and fully decoding a thread per keypress.
        let decode_request = Arc::new(RequestSlot::<(u64, DecodeRequest)>::new());
        let decode_target = Arc::new(DecodeTarget {
            sample_rate: AtomicU32::new(sample_rate),
            channels: AtomicUsize::new(channels as usize),
        });
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let decode_thread;
        {
            let decode_request = Arc::clone(&decode_request);
            let decode_target = Arc::clone(&decode_target);
            decode_thread = std::thread::spawn(move || loop {
                let (id, req) = decode_request.recv();
                if matches!(&req, DecodeRequest::Shutdown) {
                    break;
                }
                let target_channels = decode_target.channels.load(Ordering::Acquire);
                let target_rate = decode_target.sample_rate.load(Ordering::Acquire);
                let result =
                    decode_and_prepare(req.path(), target_channels, target_rate, req.window());
                // This bounded send may wait for control-side `poll()`, never
                // the CPAL callback. `Drop` disconnects the receiver before
                // joining, which wakes this send during shutdown.
                if result_tx.send((id, result)).is_err() {
                    break;
                }
            });
        }

        Ok(PlaybackEngine {
            shared,
            stream: Some(stream),
            device_sample_rate: sample_rate,
            device_channels: channels,
            current_file: None,
            current_peaks: None,
            current_full_peaks: None,
            current_info: None,
            current_window_start: Duration::ZERO,
            active_buffer: None,
            retired_buffer: None,
            pending_buffer: None,
            next_buffer_generation: 0,
            pending: None,
            next_request_id: 0,
            cache: LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).unwrap()),
            decode_request,
            decode_target,
            decode_result_rx: Some(result_rx),
            decode_thread: Some(decode_thread),
            recovery_attempted: false,
            pending_stream_error: None,
        })
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        id
    }

    fn publish_now(&mut self, audio: Arc<PreparedAudio>) {
        self.next_buffer_generation = self.next_buffer_generation.wrapping_add(1);
        let published = Box::new(PublishedAudio {
            generation: self.next_buffer_generation,
            audio: Arc::clone(&audio),
            cursor: AtomicUsize::new(0),
            playing: AtomicBool::new(true),
        });
        let old = self.active_buffer.replace(published);
        let new_ptr = self
            .active_buffer
            .as_ref()
            .map_or(std::ptr::null_mut(), |buffer| {
                (&**buffer) as *const PublishedAudio as *mut PublishedAudio
            });

        // The new Box is already owned by `active_buffer` before publication.
        // The old Box stays alive in this stack variable until it is either
        // retired or dropped after the callback-user count proves safety.
        self.shared.published.store(new_ptr, Ordering::Release);

        if let Some(old) = old {
            if self.shared.callback_users.load(Ordering::SeqCst) == 0 {
                drop(old);
            } else {
                debug_assert!(self.retired_buffer.is_none());
                self.retired_buffer = Some(old);
            }
        }

        self.current_file = Some(audio.file.clone());
        self.current_peaks = Some(audio.peaks.clone());
        self.current_info = Some(audio.info.clone());
        self.current_window_start = audio.window_start;
    }

    /// Drop a superseded published buffer only after every callback that could
    /// have loaded its pointer has finished. A callback increments the count
    /// before loading the pointer; a callback starting after the publication
    /// sees the new pointer, so a zero count is the retirement acknowledgement.
    fn advance_handoff(&mut self) {
        if self.retired_buffer.is_some() {
            if self.shared.callback_users.load(Ordering::SeqCst) != 0 {
                return;
            }
            self.retired_buffer.take();
        }

        if let Some(audio) = self.pending_buffer.take() {
            self.publish_now(audio);
        }
    }

    fn commit(&mut self, audio: &Arc<PreparedAudio>) {
        self.pending_buffer = Some(Arc::clone(audio));
        self.advance_handoff();
    }

    fn stop_stream_and_clear_handoff(&mut self) {
        self.set_active_playing(false);
        drop(self.stream.take());
        while self.shared.callback_users.load(Ordering::SeqCst) != 0 {
            std::thread::yield_now();
        }

        // No future callback can run after the stream is dropped and all
        // in-flight callbacks have acknowledged. The control side may now
        // clear the published pointer and destroy every owned buffer.
        self.shared
            .published
            .store(std::ptr::null_mut(), Ordering::SeqCst);
        self.active_buffer = None;
        self.retired_buffer = None;
        self.pending_buffer = None;
    }

    fn request_full_decode(&mut self, path: PathBuf) {
        let id = self.alloc_id();
        self.pending = Some(PendingReq {
            id,
            file: path.clone(),
        });
        self.decode_request.send((id, DecodeRequest::Full(path)));
    }

    fn set_active_playing(&self, playing: bool) {
        if let Some(active) = self.active_buffer.as_ref() {
            active.playing.store(playing, Ordering::SeqCst);
        }
    }

    fn restart_stream(&mut self) -> Result<(), PlaybackError> {
        let current_file = self.current_file.clone();
        self.stop_stream_and_clear_handoff();

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| PlaybackError::DeviceError("no output device found".into()))?;
        let supported_config = device
            .default_output_config()
            .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;
        if supported_config.sample_format() != SampleFormat::F32 {
            return Err(PlaybackError::UnsupportedFormat);
        }

        self.device_sample_rate = supported_config.sample_rate();
        self.device_channels = supported_config.channels();
        self.decode_target
            .sample_rate
            .store(self.device_sample_rate, Ordering::Release);
        self.decode_target
            .channels
            .store(self.device_channels as usize, Ordering::Release);
        self.stream = Some(build_stream(
            &device,
            &supported_config.into(),
            &self.shared,
        )?);

        // Prepared samples are normalized to the device format by the decode
        // worker. Re-decode after any stream restart instead of reusing a
        // buffer prepared for a device that may have a different rate/channel
        // layout. Clearing the cache also prevents a stale-format cache hit.
        self.cache.clear();
        self.current_peaks = None;
        self.current_info = None;
        if let Some(path) = current_file {
            self.request_full_decode(path);
        }
        Ok(())
    }

    fn poll_stream_failure(&mut self) -> Option<PlaybackError> {
        if !self.shared.stream_failed.swap(false, Ordering::AcqRel) {
            return None;
        }

        self.set_active_playing(false);
        if self.recovery_attempted {
            return Some(PlaybackError::DeviceError(
                "audio stream failed again; press Play to retry".into(),
            ));
        }

        self.recovery_attempted = true;
        match self.restart_stream() {
            Ok(()) => {
                self.recovery_attempted = false;
                None
            }
            Err(error) => {
                self.pending = None;
                Some(error)
            }
        }
    }

    /// Begin loading and playing a file. If the file was recently decoded it
    /// is served from an in-memory cache and playback starts immediately.
    /// Otherwise the request is handed to the persistent decode worker and
    /// this returns immediately. Call [`poll`] each frame to check for
    /// completion and commit the audio buffer.
    pub fn play(&mut self, path: &Path) {
        self.set_active_playing(false);
        self.pending_buffer = None;
        self.recovery_attempted = false;
        self.pending_stream_error = None;

        if self.stream.is_none() {
            if let Err(error) = self.restart_stream() {
                self.pending = None;
                self.pending_stream_error = Some(error);
                return;
            }
        }

        let path_buf = path.to_path_buf();
        // New clip: drop the previous file's full-source waveform.
        self.current_full_peaks = None;

        if let Some(cached) = self.cache.get(&path_buf) {
            let cached = Arc::clone(cached);
            self.pending = None;
            self.commit(&cached);
            return;
        }

        self.current_peaks = None;
        self.current_info = None;

        // If a decode is already in flight, this replaces the queued request —
        // RequestSlot coalesces to the latest — so rapid navigation collapses
        // into a single decode instead of spawning a thread per keypress.
        self.request_full_decode(path_buf);
    }

    pub fn poll(&mut self) -> Option<PlaybackError> {
        if let Some(error) = self.poll_stream_failure() {
            return Some(error);
        }

        self.advance_handoff();
        if self.pending.is_none() {
            return self.pending_stream_error.take();
        }

        let result_rx = self
            .decode_result_rx
            .as_ref()
            .expect("decode result receiver lives until PlaybackEngine::drop");
        loop {
            match result_rx.try_recv() {
                Ok((id, result)) => {
                    // A result for a request superseded by a later play()/seek
                    // (or abandoned for a cache hit) — discard and keep draining.
                    if self.pending.as_ref().map(|p| p.id) != Some(id) {
                        continue;
                    }
                    return match result {
                        Ok(audio) => {
                            self.pending = None;
                            let arc = Arc::new(audio);
                            // Previews/windows of long files are large and
                            // re-auditioned rarely; keep them out of the cache
                            // so it stays full of small one-shots.
                            if !arc.info.truncated {
                                self.cache.put(arc.file.clone(), Arc::clone(&arc));
                            }
                            self.commit(&arc);
                            None
                        }
                        Err(e) => {
                            self.pending = None;
                            Some(e)
                        }
                    };
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    return Some(PlaybackError::DecodeError(
                        "decode worker terminated unexpectedly".into(),
                    ));
                }
            }
        }
    }

    pub fn stop(&mut self) {
        self.set_active_playing(false);
        self.pending = None;
        self.pending_buffer = None;
        // Keep current_file / current_info (and the decoded buffer) so the clip
        // stays loaded and scrubbable after Stop — seek_fraction can resume it,
        // and status() correctly reports Playing once it does. A new play()
        // overwrites them, so nothing goes stale.
    }

    pub fn status(&self) -> PlaybackStatus {
        if let Some(p) = &self.pending {
            return PlaybackStatus::Loading {
                file: p.file.clone(),
            };
        }

        if !self
            .active_buffer
            .as_ref()
            .is_some_and(|buffer| buffer.playing.load(Ordering::Relaxed))
        {
            return PlaybackStatus::Idle;
        }

        match &self.current_file {
            Some(file) => {
                let cursor = self
                    .active_buffer
                    .as_ref()
                    .map(|buffer| buffer.cursor.load(Ordering::Relaxed))
                    .unwrap_or(0);
                let total = self.loaded_total_frames();
                let channels = self.device_channels as usize;
                let frame = cursor.checked_div(channels).unwrap_or(0);
                let rate = self.device_sample_rate as f64;
                // Position and duration are SOURCE-relative: the loaded buffer
                // may be a window starting at `current_window_start` into a
                // longer source, so the playhead and time readout track the
                // whole file, not just the buffer.
                let within = frame as f64 / rate;
                let position = self.current_window_start + Duration::from_secs_f64(within);
                let duration = self
                    .current_info
                    .as_ref()
                    .map(|i| i.source_duration)
                    .unwrap_or_else(|| Duration::from_secs_f64(total as f64 / rate));

                PlaybackStatus::Playing {
                    file: file.clone(),
                    position,
                    duration,
                }
            }
            None => PlaybackStatus::Idle,
        }
    }

    pub fn current_file(&self) -> Option<&Path> {
        self.current_file.as_deref()
    }

    /// True when the full-source waveform for the current file is available and
    /// shown (so the waveform axis spans the whole source, not just a window).
    fn showing_full(&self) -> bool {
        matches!(
            (&self.current_full_peaks, &self.current_file),
            (Some((p, _)), Some(f)) if p == f
        )
    }

    pub fn waveform_peaks(&self) -> Option<&WaveformPeaks> {
        if let (Some((p, peaks)), Some(f)) = (&self.current_full_peaks, &self.current_file) {
            if p == f {
                return Some(peaks);
            }
        }
        self.current_peaks.as_ref()
    }

    /// The `(start, duration)` region of the SOURCE, in seconds, that the
    /// waveform returned by [`waveform_peaks`] represents. The UI maps the
    /// playhead and scrub clicks against this. `None` when nothing is loaded.
    pub fn waveform_axis(&self) -> Option<(f64, f64)> {
        let info = self.current_info.as_ref()?;
        if self.showing_full() {
            Some((0.0, info.source_duration.as_secs_f64().max(1e-9)))
        } else {
            let total = self.loaded_total_frames();
            if total == 0 {
                return None;
            }
            let loaded = (total as f64 / self.device_sample_rate as f64).max(1e-9);
            Some((self.current_window_start.as_secs_f64(), loaded))
        }
    }

    pub fn current_info(&self) -> Option<&TrackInfo> {
        self.current_info.as_ref()
    }

    fn loaded_total_frames(&self) -> usize {
        self.active_buffer
            .as_ref()
            .map(|buffer| buffer.audio.total_frames)
            .unwrap_or(0)
    }

    /// Attach a full-source waveform for `path`, if it's still the current file
    /// (a scan that finished after the user moved on is dropped).
    pub fn set_full_peaks(&mut self, path: &Path, peaks: WaveformPeaks) {
        if self.current_file.as_deref() == Some(path) {
            self.current_full_peaks = Some((path.to_path_buf(), peaks));
        }
    }

    /// Seek to `target` seconds into the SOURCE and play from there. If the
    /// loaded buffer already covers `target`, this just moves the cursor
    /// (instant). Otherwise it requests an on-demand window decode at `target`
    /// (scrubbing a long file past its loaded region) — playback resumes when
    /// that window commits via [`poll`].
    ///
    /// `&mut self` because the out-of-window case enqueues a decode. The
    /// in-window fast path is atomics only; its cursor store is published by
    /// the `Release` store of that buffer's playing flag, which the callback
    /// loads with `Acquire`, so a seek never reads a stale position.
    pub fn seek_to(&mut self, target: Duration) {
        let total = self.loaded_total_frames();
        if total == 0 {
            return;
        }
        let rate = self.device_sample_rate as f64;
        let loaded = total as f64 / rate;
        let ws = self.current_window_start.as_secs_f64();
        let t = target.as_secs_f64();

        if t >= ws && t < ws + loaded {
            let within = t - ws;
            let frame = ((within * rate) as usize).min(total - 1);
            let channels = self.device_channels.max(1) as usize;
            if let Some(active) = self.active_buffer.as_ref() {
                active.cursor.store(frame * channels, Ordering::SeqCst);
                active.playing.store(true, Ordering::Release);
            }
            return;
        }

        let Some(file) = self.current_file.clone() else {
            return;
        };
        self.set_active_playing(false);
        self.pending_buffer = None;
        let id = self.alloc_id();
        self.pending = Some(PendingReq {
            id,
            file: file.clone(),
        });
        self.decode_request.send((
            id,
            DecodeRequest::Window {
                path: file,
                start: target,
            },
        ));
    }

    pub fn set_volume(&self, v: f32) {
        self.shared
            .volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }
}

impl Drop for PlaybackEngine {
    fn drop(&mut self) {
        self.stop_stream_and_clear_handoff();
        // Disconnect a possibly-full result mailbox before joining the worker;
        // a worker blocked in its bounded send then exits instead of deadlocking
        // shutdown.
        self.decode_result_rx.take();
        self.decode_request
            .send((u64::MAX, DecodeRequest::Shutdown));
        if let Some(thread) = self.decode_thread.take() {
            let _ = thread.join();
        }
    }
}

fn decode_and_prepare(
    path: &Path,
    target_channels: usize,
    target_rate: u32,
    window: Option<Duration>,
) -> Result<PreparedAudio, PlaybackError> {
    let decoded = match window {
        None => decode::decode_file(path)?,
        Some(start) => decode::decode_window(path, start, WINDOW)?,
    };

    let waveform_peaks = peaks::compute_peaks(
        &decoded.interleaved,
        decoded.channels as usize,
        peaks::DEFAULT_NUM_BUCKETS,
    );

    let samples = adapt_channels(
        &decoded.interleaved,
        decoded.channels as usize,
        target_channels,
    );

    let samples = if decoded.sample_rate != target_rate {
        resample::resample(&samples, target_channels, decoded.sample_rate, target_rate)?
    } else {
        samples
    };

    let total_frames = samples.len() / target_channels;

    let info = TrackInfo {
        source_sample_rate: decoded.sample_rate,
        source_duration: decoded.source_duration,
        truncated: decoded.truncated,
        metadata: decoded.metadata,
    };

    Ok(PreparedAudio {
        samples,
        total_frames,
        file: path.to_path_buf(),
        peaks: waveform_peaks,
        info,
        window_start: decoded.window_start,
    })
}

fn audio_callback(data: &mut [f32], shared: &SharedState) {
    // This counter is the callback's lifetime acknowledgement. It is updated
    // before the pointer load and after the last sample read, so control-side
    // retirement can never destroy a buffer that this invocation can touch.
    shared.callback_users.fetch_add(1, Ordering::SeqCst);

    let ptr = shared.published.load(Ordering::Acquire);
    if ptr.is_null() {
        data.fill(0.0);
        shared.callback_users.fetch_sub(1, Ordering::SeqCst);
        return;
    }

    // SAFETY: `PlaybackEngine` publishes only heap-stable `PublishedAudio`
    // pointers and retains their owning Box until `callback_users` reaches
    // zero after this invocation. The callback never clones/drops the Arc.
    let published = unsafe { &*ptr };
    if !published.playing.load(Ordering::Acquire) {
        data.fill(0.0);
        shared.callback_users.fetch_sub(1, Ordering::SeqCst);
        return;
    }
    let samples = &published.audio.samples;
    let cursor = published.cursor.load(Ordering::Relaxed);
    let remaining = samples.len().saturating_sub(cursor);
    let to_copy = remaining.min(data.len());
    let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));

    for (dst, &src) in data[..to_copy]
        .iter_mut()
        .zip(&samples[cursor..cursor + to_copy])
    {
        *dst = src * volume;
    }

    if to_copy < data.len() {
        data[to_copy..].fill(0.0);
        published.playing.store(false, Ordering::Relaxed);
    }

    published.cursor.store(cursor + to_copy, Ordering::Relaxed);
    shared
        .acknowledged_generation
        .store(published.generation, Ordering::Release);
    shared.callback_users.fetch_sub(1, Ordering::SeqCst);
}

fn adapt_channels(samples: &[f32], from: usize, to: usize) -> Vec<f32> {
    if from == to || from == 0 || to == 0 {
        return samples.to_vec();
    }

    let num_frames = samples.len() / from;
    let mut out = Vec::with_capacity(num_frames * to);
    let inv_from = 1.0 / from as f32;

    for frame in 0..num_frames {
        let base = frame * from;
        if from > to {
            // Downmix: sum all source channels to mono, then write to every
            // output channel. (L+R)/2 for stereo→mono; correct for all counts.
            let mono: f32 = (0..from).map(|ch| samples[base + ch]).sum::<f32>() * inv_from;
            for _ in 0..to {
                out.push(mono);
            }
        } else {
            // Upmix: copy available channels, replicate last one for the rest.
            for ch in 0..to {
                out.push(samples[base + ch.min(from - 1)]);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn published(samples: &[f32], generation: u64) -> Box<PublishedAudio> {
        Box::new(PublishedAudio {
            generation,
            audio: Arc::new(PreparedAudio {
                samples: samples.to_vec(),
                total_frames: samples.len(),
                file: PathBuf::from("test.wav"),
                peaks: WaveformPeaks {
                    peaks: Vec::new(),
                    num_buckets: 0,
                },
                info: TrackInfo {
                    metadata: AudioMetadata::default(),
                    source_sample_rate: 48_000,
                    source_duration: Duration::from_secs(1),
                    truncated: false,
                },
                window_start: Duration::ZERO,
            }),
            cursor: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
        })
    }

    fn publish_for_test(shared: &SharedState, buffer: &PublishedAudio) {
        let ptr = buffer as *const PublishedAudio as *mut PublishedAudio;
        shared.published.store(ptr, Ordering::Release);
        buffer.playing.store(true, Ordering::Release);
    }

    #[test]
    fn request_slot_coalesces_to_latest() {
        // Several sends before anyone reads: only the last one should surface.
        let slot = RequestSlot::new();
        for value in 0..100 {
            slot.send(value);
        }
        assert_eq!(slot.recv(), 99);
    }

    #[test]
    fn request_slot_recv_blocks_until_send() {
        let slot = Arc::new(RequestSlot::new());
        let reader = Arc::clone(&slot);
        let handle = std::thread::spawn(move || reader.recv());
        std::thread::sleep(Duration::from_millis(20));
        slot.send(42);
        assert_eq!(handle.join().unwrap(), 42);
    }

    #[test]
    fn callback_reads_immutable_buffer_applies_volume_and_acknowledges() {
        let shared = SharedState::new();
        let buffer = published(&[0.25, -0.5, 1.0], 7);
        publish_for_test(&shared, &buffer);
        shared.volume.store(0.5f32.to_bits(), Ordering::Relaxed);

        let mut output = [0.0; 3];
        audio_callback(&mut output, &shared);

        assert_eq!(output, [0.125, -0.25, 0.5]);
        assert_eq!(shared.callback_users.load(Ordering::SeqCst), 0);
        assert_eq!(shared.acknowledged_generation.load(Ordering::Acquire), 7);
        assert!(buffer.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn callback_zero_fills_tail_and_stops_on_short_buffer() {
        let shared = SharedState::new();
        let buffer = published(&[1.0, 2.0], 1);
        publish_for_test(&shared, &buffer);

        let mut output = [0.0; 4];
        audio_callback(&mut output, &shared);

        assert_eq!(output, [1.0, 2.0, 0.0, 0.0]);
        assert_eq!(buffer.cursor.load(Ordering::Relaxed), 2);
        assert!(!buffer.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn callback_buffer_replacement_is_observed_by_next_invocation() {
        let shared = SharedState::new();
        let first = published(&[1.0, 1.0], 1);
        let second = published(&[2.0, 2.0], 2);
        publish_for_test(&shared, &first);

        let mut output = [0.0; 2];
        audio_callback(&mut output, &shared);
        assert_eq!(output, [1.0, 1.0]);

        first.cursor.store(0, Ordering::Release);
        publish_for_test(&shared, &second);
        audio_callback(&mut output, &shared);
        assert_eq!(output, [2.0, 2.0]);
        assert_eq!(shared.acknowledged_generation.load(Ordering::Acquire), 2);
    }

    #[test]
    fn channel_adaptation_handles_mono_stereo_and_upmix() {
        assert_eq!(adapt_channels(&[1.0, -1.0, 0.5, 0.5], 2, 1), [0.0, 0.5]);
        assert_eq!(
            adapt_channels(&[1.0, 2.0, 3.0, 4.0], 2, 3),
            [1.0, 2.0, 2.0, 3.0, 4.0, 4.0]
        );
    }
}
