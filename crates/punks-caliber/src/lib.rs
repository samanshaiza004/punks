use caliber_ffi::*;
use punks_core::{FileEntry, PlaybackStatus, PunksConfig, SampleBrowser};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const PROTOCOL: u32 = 1;
pub const MAX_COMMAND: usize = 16 * 1024;
pub const MAX_RESOURCE: usize = 1024 * 1024;
pub const WINDOW: usize = 128;
const MAX_WAVEFORM_PEAKS: usize = 65_536;
#[derive(Default)]
struct Signal {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl Signal {
    fn notify(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.1 = true;
        self.changed.notify_one();
    }
    fn stop(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.0 = true;
        self.changed.notify_one();
    }
    fn wait(&self, active: bool) -> bool {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while !s.0 && !s.1 {
            if active {
                s = self
                    .changed
                    .wait_timeout(s, Duration::from_millis(33))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
                break;
            } else {
                s = self.changed.wait(s).unwrap_or_else(|e| e.into_inner());
            }
        }
        s.1 = false;
        !s.0
    }
}
#[repr(C)]
pub struct PunksSession {
    context: *mut CaliberContext,
    signal: Arc<Signal>,
    worker: Option<JoinHandle<()>>,
    stopped: bool,
}
// CaliberContext is Send+Sync; the session retains it until its worker and the
// caller-owned wake waiter are joined. No other operation may race destruction.
#[derive(Clone, Copy)]
struct Transport(*mut CaliberContext);
unsafe impl Send for Transport {}
impl Transport {
    fn state(&self, bytes: &[u8]) -> Result<u64, CaliberStatus> {
        let mut revision = 0;
        let status = unsafe {
            caliber_context_publish_state(
                self.0,
                PROTOCOL,
                bytes.as_ptr(),
                bytes.len(),
                &mut revision,
            )
        };
        if status == CaliberStatus::Ok {
            Ok(revision)
        } else {
            Err(status)
        }
    }
    fn resource(&self, bytes: &[u8]) -> Result<Resource, CaliberStatus> {
        let mut r = Resource::default();
        let status = unsafe {
            caliber_context_publish_resource(
                self.0,
                bytes.as_ptr(),
                bytes.len(),
                &mut r.id,
                &mut r.generation,
            )
        };
        if status == CaliberStatus::Ok {
            Ok(r)
        } else {
            Err(status)
        }
    }
    fn retire(&self, r: Resource) {
        if r.id != 0 {
            unsafe { caliber_context_release_resource(self.0, r.id, r.generation) };
        }
    }
    fn take(&self, bytes: &mut [u8]) -> Result<usize, CaliberStatus> {
        let mut len = 0;
        let status = unsafe {
            caliber_context_take_command(self.0, bytes.as_mut_ptr(), bytes.len(), &mut len)
        };
        if status == CaliberStatus::Ok {
            Ok(len)
        } else {
            Err(status)
        }
    }
}
fn guarded(f: impl FnOnce() -> CaliberStatus) -> CaliberStatus {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(CaliberStatus::Internal)
}

/// Flags: 1 disables audio; 2 disables persisted configuration restoration.
/// The frontend owns exactly one Caliber wake waiter and joins it after stop,
/// before destroy. The returned transport is borrowed from this session.
///
/// # Safety
/// `out` and `out_context` must be valid, aligned, writable pointers to storage
/// for their respective pointer types. They must not overlap.
#[no_mangle]
pub unsafe extern "C" fn punks_create(
    version: u32,
    flags: u32,
    out: *mut *mut PunksSession,
    out_context: *mut *mut CaliberContext,
) -> CaliberStatus {
    guarded(|| {
        if out.is_null() || out_context.is_null() || flags & !3 != 0 {
            return CaliberStatus::InvalidArgument;
        }
        if version != PROTOCOL {
            return CaliberStatus::UnsupportedVersion;
        }
        let config = CaliberContextConfig {
            max_command_bytes: MAX_COMMAND,
            max_publication_bytes: 64 * 1024,
            max_resource_bytes: 1024 * 1024,
            max_resources: 12,
            max_pending_commands: 64,
            ..Default::default()
        };
        let mut context = std::ptr::null_mut();
        let status = unsafe { caliber_context_create(&config, &mut context) };
        if status != CaliberStatus::Ok {
            return status;
        }
        let signal = Arc::new(Signal::default());
        let control = signal.clone();
        let transport = Transport(context);
        let worker = std::thread::Builder::new()
            .name("punks-control".into())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| run(transport, control, flags)));
                if result.is_err() {
                    let _ = transport
                        .state(br#"{"v":1,"ready":false,"error":"Backend worker failed"}"#);
                }
            });
        let worker = match worker {
            Ok(w) => w,
            Err(_) => {
                unsafe { caliber_context_destroy(context) };
                return CaliberStatus::Internal;
            }
        };
        let session = Box::new(PunksSession {
            context,
            signal,
            worker: Some(worker),
            stopped: false,
        });
        unsafe {
            *out_context = context;
            *out = Box::into_raw(session)
        };
        CaliberStatus::Ok
    })
}
/// Dispatch copies bytes through Caliber, then signals the backend owner.
///
/// # Safety
/// `session` must be a live pointer returned by [`punks_create`], with no
/// concurrent stop or destroy. When `len` is nonzero, `data` must point to at
/// least `len` readable bytes. A null `data` pointer is accepted only for a
/// zero-length message.
#[no_mangle]
pub unsafe extern "C" fn punks_dispatch(
    session: *const PunksSession,
    data: *const u8,
    len: usize,
) -> CaliberStatus {
    guarded(|| {
        if session.is_null() {
            return CaliberStatus::InvalidArgument;
        }
        let s = unsafe { &*session };
        if s.stopped {
            return CaliberStatus::Stopped;
        }
        let status = unsafe { caliber_context_dispatch(s.context, data, len) };
        if status == CaliberStatus::Ok {
            s.signal.notify();
        }
        status
    })
}
/// Stops/joins all Rust work, then permanently stops the Caliber wake waiter.
///
/// # Safety
/// `session` must be a live pointer returned by [`punks_create`]. The caller
/// must prevent concurrent dispatch and destroy calls for the duration.
#[no_mangle]
pub unsafe extern "C" fn punks_stop(session: *mut PunksSession) -> CaliberStatus {
    guarded(|| {
        if session.is_null() {
            return CaliberStatus::InvalidArgument;
        }
        let s = unsafe { &mut *session };
        if !s.stopped {
            s.signal.stop();
            if let Some(w) = s.worker.take() {
                let _ = w.join();
            }
            s.stopped = true;
        }
        unsafe { caliber_context_stop_wake_waiters(s.context) }
    })
}
/// Caller must stop and join its foreign waiter and cease transport calls first.
/// Caliber state/resource leases remain releasable after destruction.
///
/// # Safety
/// `session` must be a live, stopped pointer returned by [`punks_create`]. No
/// call may still access the session or its Caliber context, and all foreign
/// wake waiters must already be joined. The pointer must not be used again.
#[no_mangle]
pub unsafe extern "C" fn punks_destroy(session: *mut PunksSession) -> CaliberStatus {
    guarded(|| {
        if session.is_null() {
            return CaliberStatus::InvalidArgument;
        }
        if !unsafe { &*session }.stopped {
            return CaliberStatus::Unavailable;
        }
        let s = unsafe { Box::from_raw(session) };
        unsafe { caliber_context_destroy(s.context) };
        CaliberStatus::Ok
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    v: u32,
    op: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    index: usize,
    #[serde(default)]
    listing_revision: u64,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    delta: i32,
    #[serde(default)]
    seconds: f64,
    #[serde(default)]
    waveform: Resource,
    #[serde(default)]
    value: f32,
    #[serde(default)]
    audition: bool,
    #[serde(default)]
    request_id: u64,
    #[serde(default)]
    tab_id: u64,
    #[serde(default)]
    tab_target_id: u64,
    #[serde(default)]
    after: bool,
}
#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    pub id: u64,
    pub generation: u64,
}
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Entry {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub path: String,
    pub format: String,
    pub size: u64,
    pub directory: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct EntryWindow {
    pub v: u32,
    pub listing_revision: u64,
    pub offset: usize,
    pub total: usize,
    pub entries: Vec<Entry>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FolderWindow {
    pub v: u32,
    pub listing_revision: u64,
    pub offset: usize,
    pub total: usize,
    pub entries: Vec<Entry>,
}
#[derive(Serialize)]
struct EntryWindowBytes<'a> {
    v: u32,
    listing_revision: u64,
    offset: usize,
    total: usize,
    entries: &'a [Entry],
}
fn encode_window(
    revision: u64,
    offset: usize,
    total: usize,
    rows: &mut Vec<Entry>,
) -> Result<Vec<u8>, CaliberStatus> {
    loop {
        let bytes = serde_json::to_vec(&EntryWindowBytes {
            v: PROTOCOL,
            listing_revision: revision,
            offset,
            total,
            entries: rows,
        })
        .map_err(|_| CaliberStatus::Internal)?;
        if bytes.len() <= MAX_RESOURCE {
            return Ok(bytes);
        }
        if rows.pop().is_none() {
            return Err(CaliberStatus::LimitExceeded);
        }
    }
}
fn encode_folder_window(
    revision: u64,
    offset: usize,
    total: usize,
    rows: &mut Vec<Entry>,
) -> Result<Vec<u8>, CaliberStatus> {
    loop {
        let bytes = serde_json::to_vec(&EntryWindowBytes {
            v: PROTOCOL,
            listing_revision: revision,
            offset,
            total,
            entries: rows,
        })
        .map_err(|_| CaliberStatus::Internal)?;
        if bytes.len() <= MAX_RESOURCE {
            return Ok(bytes);
        }
        if rows.pop().is_none() {
            return Err(CaliberStatus::LimitExceeded);
        }
    }
}
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Playback {
    pub status: String,
    pub file: String,
    pub position: f64,
    pub duration: f64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub v: u32,
    pub ready: bool,
    pub directory: String,
    pub can_navigate_up: bool,
    pub query: String,
    pub searching: bool,
    pub listing_revision: u64,
    pub total: usize,
    pub offset: usize,
    pub count: usize,
    pub entries: Resource,
    pub folders: Resource,
    pub folder_count: usize,
    pub folder_offset: usize,
    pub breadcrumbs: Vec<String>,
    pub tabs: Vec<TabView>,
    pub active_tab: u64,
    pub selected_index: i64,
    pub selected: Entry,
    pub playback: Playback,
    pub volume: f32,
    pub audio_available: bool,
    pub audio_error: String,
    pub error: String,
    pub scanning: bool,
    pub scan_done: usize,
    pub scan_total: usize,
    pub command_sequence: u64,
    pub last_request_id: u64,
    pub waveform_file: String,
    pub waveform_start: f64,
    pub waveform_duration: f64,
    pub waveform: Resource,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WaveformWindow {
    pub v: u32,
    pub analysis_version: u32,
    pub channel_policy: String,
    pub file: String,
    pub start: f64,
    pub duration: f64,
    pub peak_resolution: u32,
    pub peaks: Vec<[i16; 2]>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct TabView {
    pub id: u64,
    pub title: String,
}
fn text(s: &str, max: usize) -> String {
    let mut n = s.len().min(max);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    s[..n].to_owned()
}
fn entries(b: &SampleBrowser) -> &[FileEntry] {
    if b.is_in_search_mode() {
        b.search_results().unwrap_or(&[])
    } else {
        b.entries()
    }
}
fn selected(b: &SampleBrowser) -> Option<usize> {
    if b.is_in_search_mode() {
        b.search_selected()
    } else {
        b.selected()
    }
}
fn entry(b: &SampleBrowser, e: &FileEntry, index: usize) -> Entry {
    Entry {
        id: b.asset_identity(&e.path),
        index,
        name: text(&e.name, 1024),
        path: text(&e.path.to_string_lossy(), 4096),
        format: e.extension.clone(),
        size: e.size_bytes,
        directory: e.is_directory,
    }
}
#[derive(Default)]
struct Publication {
    window_key: String,
    epoch: u64,
    key: String,
    offset: usize,
    resource: Resource,
    count: usize,
    old: Vec<Resource>,
    last: Vec<u8>,
    sequence: u64,
    request: u64,
    error: String,
    folder_window_key: String,
    folder_offset: usize,
    folder_resource: Resource,
    folder_count: usize,
    tab_ids: Vec<u64>,
    tab_offsets: HashMap<u64, usize>,
    next_tab_id: u64,
    active_tab_id: u64,
    waveform_resource: Resource,
    waveform_key: Option<(String, u64, u64, usize, usize)>,
}
impl Publication {
    fn active_tab(&self, b: &SampleBrowser) -> u64 {
        self.tab_ids.get(b.active_tab()).copied().unwrap_or(0)
    }
    fn sync_tabs(&mut self, b: &SampleBrowser) {
        if self.next_tab_id == 0 {
            self.next_tab_id = 1;
        }
        while self.tab_ids.len() < b.tab_count() {
            let id = self.next_tab_id;
            self.next_tab_id = self.next_tab_id.wrapping_add(1).max(1);
            self.tab_ids.push(id);
        }
        while self.tab_ids.len() > b.tab_count() {
            if let Some(id) = self.tab_ids.pop() {
                self.tab_offsets.remove(&id);
            }
        }
    }
    fn key(&self, b: &SampleBrowser) -> String {
        format!(
            "{}:{:?}:{}:{}:{}:{}:{}",
            self.active_tab(b),
            b.current_directory(),
            b.browse_revision(),
            b.search_revision(),
            b.search_query(),
            b.is_in_search_mode(),
            entries(b).len()
        )
    }
    fn refresh(&mut self, b: &SampleBrowser) {
        self.sync_tabs(b);
        let active_tab = self.active_tab(b);
        let key = self.key(b);
        if key != self.key {
            if active_tab != self.active_tab_id {
                self.offset = self.tab_offsets.get(&active_tab).copied().unwrap_or(0);
            } else {
                self.offset = 0;
            }
            self.folder_offset = 0;
            self.active_tab_id = active_tab;
            self.key = key;
            self.epoch = self.epoch.wrapping_add(1);
            self.last.clear();
        }
    }
    fn publish(&mut self, t: Transport, b: &SampleBrowser) -> Result<(), CaliberStatus> {
        self.refresh(b);
        let data = entries(b);
        self.offset = self.offset.min(data.len().saturating_sub(1));
        self.tab_offsets.insert(self.active_tab(b), self.offset);
        let window_key = format!("{}:{}", self.epoch, self.offset);
        // The resource is replaced only when the listing or requested window changes.
        let needs_resource = self.resource.id == 0 || self.old_window_key() != window_key;
        let mut fresh = None;
        if needs_resource {
            let mut rows = data
                .iter()
                .enumerate()
                .skip(self.offset)
                .take(WINDOW)
                .map(|(i, e)| entry(b, e, i))
                .collect::<Vec<_>>();
            let bytes = encode_window(self.epoch, self.offset, data.len(), &mut rows)?;
            self.count = rows.len();
            fresh = Some(t.resource(&bytes)?);
        }
        // The sidebar remains a navigation surface while a recursive search
        // is active, so derive child folders from the underlying directory
        // listing rather than the current search projection.
        let all_folders = b
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_directory)
            .collect::<Vec<_>>();
        self.folder_count = all_folders.len();
        self.folder_offset = self.folder_offset.min(self.folder_count.saturating_sub(1));
        let folder_window_key = format!("{}:{}", self.epoch, self.folder_offset);
        let needs_folder_resource =
            self.folder_resource.id == 0 || self.folder_window_key != folder_window_key;
        let mut fresh_folders = None;
        if needs_folder_resource {
            let mut rows = all_folders
                .iter()
                .skip(self.folder_offset)
                .take(WINDOW)
                .map(|(index, item)| entry(b, item, *index))
                .collect::<Vec<_>>();
            let bytes =
                encode_folder_window(self.epoch, self.folder_offset, self.folder_count, &mut rows)?;
            fresh_folders = Some(t.resource(&bytes)?);
        }
        let mut playback = Playback {
            status: "idle".into(),
            ..Default::default()
        };
        match b.playback_status() {
            PlaybackStatus::Idle => {}
            PlaybackStatus::Loading { file } => {
                playback.status = "loading".into();
                playback.file = text(&file.to_string_lossy(), 4096);
            }
            PlaybackStatus::Playing {
                file,
                position,
                duration,
            } => {
                playback.status = "playing".into();
                playback.file = text(&file.to_string_lossy(), 4096);
                playback.position = position.as_secs_f64();
                playback.duration = duration.as_secs_f64();
            }
            PlaybackStatus::Paused {
                file,
                position,
                duration,
            } => {
                playback.status = "paused".into();
                playback.file = text(&file.to_string_lossy(), 4096);
                playback.position = position.as_secs_f64();
                playback.duration = duration.as_secs_f64();
            }
        }
        if playback.duration == 0.0 {
            playback.duration = b
                .current_track_info()
                .map(|i| i.source_duration.as_secs_f64())
                .unwrap_or(0.0);
        }
        let sel = selected(b);
        let current = sel
            .and_then(|i| data.get(i).map(|e| entry(b, e, i)))
            .unwrap_or_default();
        let active_tab = self.active_tab(b);
        let tabs = self
            .tab_ids
            .iter()
            .enumerate()
            .map(|(index, id)| TabView {
                id: *id,
                title: text(&b.tab_title(index), 128),
            })
            .collect();
        let source_peaks = b.waveform_peaks();
        let peak_resolution = source_peaks
            .map(|peaks| peaks.peaks.len().min(MAX_WAVEFORM_PEAKS))
            .unwrap_or_default();
        let (waveform_start, waveform_duration) = b.waveform_axis().unwrap_or((0.0, 0.0));
        let waveform_file = b
            .current_file()
            .map(|path| text(&path.to_string_lossy(), 4096))
            .unwrap_or_default();
        let waveform_key = (
            waveform_file.clone(),
            waveform_start.to_bits(),
            waveform_duration.to_bits(),
            peak_resolution,
            source_peaks
                .map(|peaks| peaks.peaks.as_ptr() as usize)
                .unwrap_or(0),
        );
        let fresh_waveform = if self.waveform_key.as_ref() != Some(&waveform_key) {
            let waveform_peaks = source_peaks
                .map(|peaks| {
                    peaks
                        .peaks
                        .iter()
                        .take(MAX_WAVEFORM_PEAKS)
                        .map(|(low, high)| {
                            [
                                (low.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16,
                                (high.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16,
                            ]
                        })
                        .collect()
                })
                .unwrap_or_default();
            let waveform_bytes = serde_json::to_vec(&WaveformWindow {
                v: PROTOCOL,
                analysis_version: 3,
                channel_policy: "combined_envelope".into(),
                file: waveform_file.clone(),
                start: waveform_start,
                duration: waveform_duration,
                peak_resolution: peak_resolution as u32,
                peaks: waveform_peaks,
            })
            .map_err(|_| CaliberStatus::Internal)?;
            if waveform_bytes.len() > MAX_RESOURCE {
                return Err(CaliberStatus::Internal);
            }
            Some(t.resource(&waveform_bytes)?)
        } else {
            None
        };
        let progress = b.scan_progress();
        let state = State {
            v: PROTOCOL,
            ready: true,
            directory: b
                .current_directory()
                .map(|p| text(&p.to_string_lossy(), 4096))
                .unwrap_or_default(),
            can_navigate_up: b.can_navigate_up(),
            query: text(b.search_query(), 1024),
            searching: b.is_searching(),
            listing_revision: self.epoch,
            total: data.len(),
            offset: self.offset,
            count: self.count,
            entries: fresh.unwrap_or(self.resource),
            folders: fresh_folders.unwrap_or(self.folder_resource),
            folder_count: self.folder_count,
            folder_offset: self.folder_offset,
            breadcrumbs: b.breadcrumbs().into_iter().map(|s| text(&s, 128)).collect(),
            tabs,
            active_tab,
            selected_index: sel.map(|i| i as i64).unwrap_or(-1),
            selected: current,
            playback,
            volume: b.volume(),
            audio_available: b.audio_available(),
            audio_error: b.audio_error().unwrap_or_default().to_owned(),
            error: text(
                if self.error.is_empty() {
                    b.last_error().unwrap_or_default()
                } else {
                    &self.error
                },
                4096,
            ),
            scanning: progress.is_some(),
            scan_done: progress.map(|p| p.0).unwrap_or(0),
            scan_total: progress.map(|p| p.1).unwrap_or(0),
            command_sequence: self.sequence,
            last_request_id: self.request,
            waveform_file,
            waveform_start,
            waveform_duration,
            waveform: fresh_waveform.unwrap_or(self.waveform_resource),
        };
        let bytes = serde_json::to_vec(&state).map_err(|_| CaliberStatus::Internal)?;
        if bytes != self.last {
            if let Err(e) = t.state(&bytes) {
                if let Some(r) = fresh {
                    t.retire(r);
                }
                if let Some(r) = fresh_folders {
                    t.retire(r);
                }
                if let Some(r) = fresh_waveform {
                    t.retire(r);
                }
                return Err(e);
            }
            self.last = bytes;
        }
        if let Some(r) = fresh {
            if self.resource.id != 0 {
                self.old.push(self.resource);
            }
            self.resource = r;
            self.window_key = window_key;
        }
        if let Some(r) = fresh_folders {
            if self.folder_resource.id != 0 {
                self.old.push(self.folder_resource);
            }
            self.folder_resource = r;
            self.folder_window_key = folder_window_key;
        }
        if let Some(r) = fresh_waveform {
            if self.waveform_resource.id != 0 {
                self.old.push(self.waveform_resource);
            }
            self.waveform_resource = r;
            self.waveform_key = Some(waveform_key);
        }
        while self.old.len() > 4 {
            t.retire(self.old.remove(0));
        }
        Ok(())
    }
    fn old_window_key(&self) -> String {
        self.window_key.clone()
    }
    fn apply(&mut self, b: &mut SampleBrowser, c: Command) -> Result<(), String> {
        self.request = c.request_id;
        if c.v != PROTOCOL {
            return Err(format!("Unsupported Punks protocol {}", c.v));
        }
        if c.path.len() > 4096 || c.query.len() > 1024 || c.id.len() > 8192 {
            return Err("Command field exceeds protocol limit".into());
        }
        self.refresh(b);
        let tab_management = matches!(
            c.op.as_str(),
            "tab_new" | "tab_close" | "tab_switch" | "tab_reorder"
        );
        if !tab_management && c.tab_id != 0 && c.tab_id != self.active_tab(b) {
            return Err("Command belongs to an inactive tab".into());
        }
        match c.op.as_str() {
            "open" => {
                let path = Path::new(&c.path)
                    .canonicalize()
                    .map_err(|e| e.to_string())?;
                b.open_directory(&path).map_err(|e| e.to_string())?;
            }
            "up" => {
                b.navigate_up().map_err(|e| e.to_string())?;
            }
            "breadcrumb" => {
                b.navigate_to_breadcrumb(c.index)
                    .map_err(|e| e.to_string())?;
            }
            "enter_folder" => {
                if c.listing_revision != self.epoch {
                    return Err(format!(
                        "Stale folder listing revision {} (current {})",
                        c.listing_revision, self.epoch
                    ));
                }
                let item = b
                    .entries()
                    .get(c.index)
                    .ok_or("Folder selection is out of bounds")?;
                if !item.is_directory || b.asset_identity(&item.path) != c.id {
                    return Err("Folder identity changed".into());
                }
                b.navigate_into(c.index).map_err(|e| e.to_string())?;
            }
            "select" | "enter" => {
                if c.listing_revision != self.epoch {
                    return Err(format!(
                        "Stale listing revision {} (current {})",
                        c.listing_revision, self.epoch
                    ));
                }
                let item = entries(b)
                    .get(c.index)
                    .ok_or("Selection is out of bounds")?;
                if b.asset_identity(&item.path) != c.id {
                    return Err("Selection identity changed".into());
                }
                let path = item.path.clone();
                let is_dir = item.is_directory;
                if c.op == "enter" && is_dir {
                    b.open_directory(&path).map_err(|e| e.to_string())?;
                } else {
                    if b.is_in_search_mode() {
                        b.select_search_result(c.index);
                    } else {
                        b.select(c.index);
                    }
                    if c.audition && !is_dir {
                        b.play_selected();
                    }
                }
            }
            "step" => {
                let len = entries(b).len();
                if len > 0 {
                    let index = if let Some(i) = selected(b) {
                        (i as i64 + c.delta as i64).clamp(0, len as i64 - 1) as usize
                    } else {
                        0
                    };
                    if b.is_in_search_mode() {
                        b.select_search_result(index);
                    } else {
                        b.select(index);
                    }
                    self.offset = index.saturating_sub(WINDOW / 2);
                    self.tab_offsets.insert(self.active_tab(b), self.offset);
                    if c.audition {
                        b.play_selected();
                    }
                }
            }
            "play" => b.play_selected(),
            "pause" => b.pause(),
            "resume" => b.resume(),
            "stop" => b.stop(),
            "volume" => {
                if !c.value.is_finite() || !(0.0..=1.0).contains(&c.value) {
                    return Err("Volume must be a finite value from 0 to 1".into());
                }
                b.set_volume(c.value);
            }
            "seek" => {
                if c.waveform.id != 0
                    && (c.waveform != self.waveform_resource
                        || b.waveform_axis().is_none()
                        || b.current_file()
                            .is_none_or(|p| p.to_string_lossy() != c.path))
                {
                    return Err("Stale waveform seek".into());
                }
                if !c.seconds.is_finite() || !(0.0..=1e9).contains(&c.seconds) {
                    return Err("Invalid seek position".into());
                }
                b.seek_to(Duration::from_secs_f64(c.seconds));
            }
            "search" => {
                if c.query.is_empty() {
                    b.clear_search();
                } else {
                    b.search(&c.query);
                }
            }
            "window" => {
                if c.listing_revision != self.epoch {
                    return Err(format!(
                        "Stale window revision {} (current {})",
                        c.listing_revision, self.epoch
                    ));
                }
                self.offset = c.offset.min(entries(b).len().saturating_sub(1));
                self.tab_offsets.insert(self.active_tab(b), self.offset);
            }
            "folder_window" => {
                if c.listing_revision != self.epoch {
                    return Err(format!(
                        "Stale folder window revision {} (current {})",
                        c.listing_revision, self.epoch
                    ));
                }
                self.folder_offset = c.offset.min(
                    b.entries()
                        .iter()
                        .filter(|item| item.is_directory)
                        .count()
                        .saturating_sub(1),
                );
            }
            "tab_new" => {
                let start = b.current_directory().map(|path| path.to_path_buf());
                b.new_tab(start.as_deref());
                self.sync_tabs(b);
                self.offset = 0;
            }
            "tab_close" => {
                let index = self
                    .tab_ids
                    .iter()
                    .position(|id| *id == c.tab_id)
                    .ok_or("Unknown tab identity")?;
                if b.tab_count() <= 1 {
                    return Err("Punks must keep at least one tab open".into());
                }
                b.close_tab(index);
                self.tab_ids.remove(index);
                self.tab_offsets.remove(&c.tab_id);
                self.offset = self
                    .tab_offsets
                    .get(&self.active_tab(b))
                    .copied()
                    .unwrap_or(0);
            }
            "tab_switch" => {
                let index = self
                    .tab_ids
                    .iter()
                    .position(|id| *id == c.tab_id)
                    .ok_or("Unknown tab identity")?;
                self.tab_offsets.insert(self.active_tab(b), self.offset);
                b.switch_tab(index);
                self.offset = self.tab_offsets.get(&c.tab_id).copied().unwrap_or(0);
            }
            "tab_reorder" => {
                let from = self
                    .tab_ids
                    .iter()
                    .position(|id| *id == c.tab_id)
                    .ok_or("Unknown source tab identity")?;
                let target = self
                    .tab_ids
                    .iter()
                    .position(|id| *id == c.tab_target_id)
                    .ok_or("Unknown target tab identity")?;
                let mut to = target + if c.after { 1 } else { 0 };
                if from < to {
                    to -= 1;
                }
                to = to.min(self.tab_ids.len().saturating_sub(1));
                if from != to {
                    b.reorder_tab(from, to);
                    let id = self.tab_ids.remove(from);
                    self.tab_ids.insert(to, id);
                }
            }
            "create_library" => b.init_library(),
            _ => return Err(format!("Unknown command {}", c.op)),
        }
        Ok(())
    }
}
fn run(t: Transport, signal: Arc<Signal>, flags: u32) {
    let mut cfg = if flags & 2 != 0 {
        PunksConfig::default()
    } else {
        punks_core::config::load()
    };
    let s = signal.clone();
    let wake = Arc::new(move || s.notify());
    let mut browser = if flags & 1 != 0 {
        SampleBrowser::without_audio(&cfg, wake)
    } else {
        SampleBrowser::with_waker(&cfg, wake)
    }
    .expect("core initialization");
    let mut p = Publication::default();
    let _ = p.publish(t, &browser);
    let mut bytes = [0u8; MAX_COMMAND];
    let mut volume_save_deadline = None;
    loop {
        let active = volume_save_deadline.is_some()
            || matches!(
                browser.playback_status(),
                PlaybackStatus::Playing { .. } | PlaybackStatus::Loading { .. }
            );
        if !signal.wait(active) {
            break;
        }
        let mut taken = 0;
        while taken < 64 {
            let len = match t.take(&mut bytes) {
                Ok(n) => n,
                Err(CaliberStatus::Unavailable) => break,
                Err(e) => {
                    p.error = format!("Caliber command error: {e:?}");
                    break;
                }
            };
            taken += 1;
            p.sequence += 1;
            p.error = match serde_json::from_slice::<Command>(&bytes[..len]) {
                Ok(c) => {
                    let volume_command = c.op == "volume";
                    let update_config = matches!(
                        c.op.as_str(),
                        "open"
                            | "up"
                            | "enter"
                            | "breadcrumb"
                            | "tab_new"
                            | "tab_close"
                            | "tab_switch"
                            | "tab_reorder"
                    );
                    let result = p.apply(&mut browser, c);
                    if result.is_ok() && volume_command && flags & 2 == 0 {
                        volume_save_deadline = Some(Instant::now() + Duration::from_millis(350));
                    }
                    if result.is_ok() && update_config && flags & 2 == 0 {
                        browser.update_config(&mut cfg);
                        cfg.volume = browser.volume();
                        punks_core::config::save(&cfg);
                        volume_save_deadline = None;
                    }
                    result.err().unwrap_or_default()
                }
                Err(e) => format!("Invalid command: {e}"),
            };
        }
        if taken == 64 {
            signal.notify();
        }
        browser.poll();
        if volume_save_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            browser.update_config(&mut cfg);
            cfg.volume = browser.volume();
            punks_core::config::save(&cfg);
            volume_save_deadline = None;
        }
        if let Err(e) = p.publish(t, &browser) {
            eprintln!("Punks publication failed: {e:?}");
        }
    }
    if volume_save_deadline.is_some() && flags & 2 == 0 {
        browser.update_config(&mut cfg);
        cfg.volume = browser.volume();
        punks_core::config::save(&cfg);
    }
    drop(browser);
    for r in p
        .old
        .into_iter()
        .chain([p.resource, p.folder_resource, p.waveform_resource])
    {
        t.retire(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_resources_fit_caliber_limit_even_with_expensive_json_escapes() {
        let mut rows = (0..WINDOW)
            .map(|index| Entry {
                id: format!("asset-{index}"),
                index,
                name: "\u{1}".repeat(1024),
                path: "\u{1}".repeat(4096),
                ..Entry::default()
            })
            .collect::<Vec<_>>();

        let bytes = encode_window(4, 128, 10_000, &mut rows).unwrap();
        assert!(!rows.is_empty());
        assert!(rows.len() < WINDOW);
        assert!(bytes.len() <= MAX_RESOURCE);

        let parsed: EntryWindow = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.listing_revision, 4);
        assert_eq!(parsed.offset, 128);
        assert_eq!(parsed.total, 10_000);
        assert_eq!(parsed.entries.len(), rows.len());
    }
}
