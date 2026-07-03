use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

pub use punks_analysis::{amp_to_dbfs, AnalysisReport};
pub use punks_core::config::PunksConfig;
pub use punks_core::{DirListing, FileEntry, ScanError, SUPPORTED_EXTENSIONS};
pub use punks_library::{LibraryError, ScanSummary, TagCount};
pub use punks_playback::{AudioMetadata, PlaybackError, PlaybackStatus, TrackInfo, WaveformPeaks};

use punks_analysis::{AnalysisContext, AudioBuffer};
use punks_library::Library;
use punks_playback::{decode_file, PlaybackEngine, RequestSlot};

/// How many buckets a full-source waveform is computed at (matches the preview
/// waveform's resolution).
const WAVEFORM_BUCKETS: usize = 512;

/// Compute (or load from cache) the full-source waveform for `path`, cache it
/// under the file's library if any, and return it. Runs on the peaks worker.
fn compute_and_cache_peaks(path: &Path) -> Option<WaveformPeaks> {
    let root = path.parent().and_then(Library::find_root);
    let lib = root.as_deref().and_then(|r| Library::open(r).ok());

    if let Some(lib) = &lib {
        if let Ok(Some(pairs)) = lib.load_waveform(path) {
            let num_buckets = pairs.len();
            return Some(WaveformPeaks {
                peaks: pairs,
                num_buckets,
            });
        }
    }
    match punks_playback::compute_source_peaks(path, WAVEFORM_BUCKETS) {
        Ok(peaks) => {
            if let Some(lib) = &lib {
                if let Err(e) = lib.store_waveform(path, &peaks.peaks) {
                    log::warn!("waveform cache write: {e}");
                }
            }
            Some(peaks)
        }
        Err(e) => {
            log::warn!("full-source peaks for {}: {e}", path.display());
            None
        }
    }
}

/// Whether the active tab's directory tree has a library attached.
/// Browsing always works; library features (tags, filters) light up only when
/// the user has deliberately created a `.punks` root (or one already exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryState {
    /// Plain browsing; the user may explicitly create a library here.
    NotALibrary,
    /// A library is attached and its background scan is still running.
    Scanning,
    /// A library is attached and its caches are loaded.
    Ready,
}

#[derive(Debug)]
pub enum BrowserError {
    Scan(ScanError),
    Playback(PlaybackError),
    NoSelection,
}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowserError::Scan(e) => write!(f, "scan error: {e}"),
            BrowserError::Playback(e) => write!(f, "playback error: {e}"),
            BrowserError::NoSelection => write!(f, "no file selected"),
        }
    }
}

impl std::error::Error for BrowserError {}

impl From<ScanError> for BrowserError {
    fn from(e: ScanError) -> Self {
        BrowserError::Scan(e)
    }
}

impl From<PlaybackError> for BrowserError {
    fn from(e: PlaybackError) -> Self {
        BrowserError::Playback(e)
    }
}

/// One tab's navigation context: its own directory history, selection, and
/// search. Playback is global and lives on `SampleBrowser`, not here.
#[derive(Default)]
struct TabState {
    history: Vec<PathBuf>,
    listing: Option<DirListing>,
    selected: Option<usize>,
    /// Committed search text, so a tab restores its query when reactivated.
    search_query: String,
    /// The DISPLAY list for results view: text results after the tag filter,
    /// or a library-wide tag query when only tags are active.
    search_results: Option<Vec<FileEntry>>,
    /// Unfiltered text-search output, kept so the tag filter can be toggled
    /// without re-running the search.
    raw_results: Option<Vec<FileEntry>>,
    search_rx: Option<mpsc::Receiver<Vec<FileEntry>>>,
    search_selected: Option<usize>,
    /// Index into SampleBrowser::libraries when this tab is inside a library
    /// root. Tag IDs are meaningful only relative to that library and are
    /// never persisted outside the session.
    library_idx: Option<usize>,
    /// AND tag filter (active-library tag ids).
    tag_filter: Vec<i64>,
}

/// An opened library plus in-memory display caches, so per-frame UI reads
/// (pills, sidebar counts) never touch SQLite. Caches reload after scans and
/// tag mutations.
struct LibraryContext {
    root: PathBuf,
    lib: Library,
    tags: Vec<TagCount>,
    /// Every non-missing asset, as ready-to-display entries (absolute paths).
    assets: Vec<FileEntry>,
    /// Absolute path -> tag ids.
    asset_tags: HashMap<PathBuf, Vec<i64>>,
    /// Absolute path -> analysis results, filled asynchronously by the worker.
    analysis: HashMap<PathBuf, AnalysisReport>,
    scanning: bool,
    scan_rx: Option<mpsc::Receiver<Result<ScanSummary, LibraryError>>>,
    scan_progress: Option<Arc<ScanProgress>>,
}

/// Live progress of a background scan, shared with the scan thread.
/// `total == 0` means the directory tree is still being walked (count unknown).
#[derive(Default)]
struct ScanProgress {
    total: std::sync::atomic::AtomicUsize,
    done: std::sync::atomic::AtomicUsize,
}

impl LibraryContext {
    fn reload(&mut self) {
        match self.lib.list_tags() {
            Ok(t) => self.tags = t,
            Err(e) => log::warn!("library tag reload: {e}"),
        }
        match self.lib.list_assets() {
            Ok(assets) => {
                self.assets = assets
                    .iter()
                    .map(|a| {
                        let path = self.root.join(&a.relative_path);
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let extension = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        FileEntry {
                            path,
                            name,
                            extension,
                            size_bytes: a.size,
                            is_directory: false,
                        }
                    })
                    .collect();
            }
            Err(e) => log::warn!("library asset reload: {e}"),
        }
        match self.lib.all_asset_tags() {
            Ok(pairs) => {
                let mut map: HashMap<PathBuf, Vec<i64>> = HashMap::new();
                for (rel, tag_id) in pairs {
                    map.entry(self.root.join(rel)).or_default().push(tag_id);
                }
                self.asset_tags = map;
            }
            Err(e) => log::warn!("library asset-tag reload: {e}"),
        }
        match self.lib.all_analysis() {
            Ok(rows) => {
                self.analysis = rows
                    .into_iter()
                    .map(|(path, metrics)| (path, AnalysisReport::from_metrics(&metrics)))
                    .collect();
            }
            Err(e) => log::warn!("library analysis reload: {e}"),
        }
    }
}

fn spawn_scan(
    root: PathBuf,
) -> (
    mpsc::Receiver<Result<ScanSummary, LibraryError>>,
    Arc<ScanProgress>,
) {
    use std::sync::atomic::Ordering;
    let progress = Arc::new(ScanProgress::default());
    let (tx, rx) = mpsc::channel();
    let prog = Arc::clone(&progress);
    std::thread::spawn(move || {
        // The scan gets its own connection (WAL journal) so the UI thread's
        // reads never block on it.
        let result = Library::open(&root).and_then(|mut lib| {
            let files = punks_library::scan_files(&root)?;
            prog.total.store(files.len(), Ordering::Relaxed);
            let summary = lib
                .reconcile_with_progress(&files, |done| prog.done.store(done, Ordering::Relaxed))?;
            // Queue analysis for everything present at the current pipeline
            // version (idempotent). The worker drains it after the scan resolves.
            lib.enqueue_all(punks_analysis::pipeline_version())?;
            Ok(summary)
        });
        let _ = tx.send(result);
    });
    (rx, progress)
}

/// The global background analysis worker: one app-lifetime thread that drains a
/// library's job queue whenever asked. Fed a durable FIFO of roots (every queued
/// root is drained, unlike the latest-wins peaks `RequestSlot`); reports each
/// finished asset back so the UI can fill it in. Owns its own `Library`
/// connection per drain, so it never contends with the UI thread's handle beyond
/// WAL's normal write serialization.
fn spawn_analysis_worker() -> (mpsc::Sender<PathBuf>, mpsc::Receiver<PathBuf>) {
    let (drain_tx, drain_rx) = mpsc::channel::<PathBuf>();
    let (done_tx, done_rx) = mpsc::channel::<PathBuf>();
    std::thread::spawn(move || {
        while let Ok(root) = drain_rx.recv() {
            let Ok(mut lib) = Library::open(&root) else {
                continue;
            };
            // Requeue anything a prior run left mid-flight before draining.
            if let Err(e) = lib.reset_running_jobs() {
                log::warn!("analysis reset ({}): {e}", root.display());
            }
            loop {
                match lib.claim_next_pending() {
                    Ok(Some(path)) => {
                        let t = std::time::Instant::now();
                        match decode_file(&path) {
                            Ok(d) => {
                                // Bounded window (decode_file caps long files), so
                                // worker memory stays bounded. The context also
                                // carries the *true* source length so Duration is
                                // correct even when `audio` is a preview window.
                                let ctx = AnalysisContext {
                                    audio: AudioBuffer::new(
                                        &d.interleaved,
                                        d.sample_rate,
                                        d.channels,
                                    ),
                                    source_duration: d.source_duration,
                                };
                                let report = punks_analysis::run_all(&ctx);
                                let dur = t.elapsed().as_millis() as u32;
                                match lib.store_analysis(&path, &report.metrics(), dur) {
                                    Ok(()) => {
                                        if done_tx.send(path).is_err() {
                                            return; // receiver dropped: app closing
                                        }
                                    }
                                    Err(e) => log::warn!("analysis store {path:?}: {e}"),
                                }
                            }
                            Err(e) => {
                                if let Err(e2) = lib.fail_analysis(&path, &e.to_string()) {
                                    log::warn!("analysis fail-mark {path:?}: {e2}");
                                }
                            }
                        }
                    }
                    Ok(None) => break, // drained
                    Err(e) => {
                        log::warn!("analysis claim ({}): {e}", root.display());
                        break;
                    }
                }
            }
        }
    });
    (drain_tx, done_rx)
}

pub struct SampleBrowser {
    tabs: Vec<TabState>,
    active_tab: usize,
    playback: PlaybackEngine,
    last_error: Option<String>,
    libraries: Vec<LibraryContext>,
    /// Persistent background worker computing full-source waveforms (latest
    /// request wins, so scrolling past files doesn't queue up work).
    peaks_request: Arc<RequestSlot<PathBuf>>,
    peaks_result_rx: mpsc::Receiver<(PathBuf, Option<WaveformPeaks>)>,
    /// The file we've most recently asked for full peaks, to avoid re-requesting
    /// every frame. Reset when a new clip is played.
    peaks_requested_for: Option<PathBuf>,
    /// Global background analysis worker: send a library root to drain its job
    /// queue; receive each finished asset's path to refresh its cached results.
    analysis_drain_tx: mpsc::Sender<PathBuf>,
    analysis_done_rx: mpsc::Receiver<PathBuf>,
}

/// Max analysis completions folded into caches per frame, so a fast worker
/// finishing thousands of files can't stall a frame. Leftovers arrive next frame.
const ANALYSIS_DRAIN_PER_FRAME: usize = 32;

impl SampleBrowser {
    /// `cfg` is read once by the caller (see `BrowserPanel::prefs`) rather than
    /// loaded again here, so a single app startup only touches disk once for
    /// config instead of once per component that needs it.
    pub fn new(cfg: &PunksConfig) -> Result<Self, BrowserError> {
        let playback = PlaybackEngine::new()?;

        // Persistent peaks worker: one full-source waveform computation at a
        // time, latest request wins.
        let peaks_request = Arc::new(RequestSlot::<PathBuf>::new());
        let (peaks_tx, peaks_result_rx) = mpsc::channel();
        {
            let peaks_request = Arc::clone(&peaks_request);
            std::thread::spawn(move || loop {
                let path = peaks_request.recv();
                let peaks = compute_and_cache_peaks(&path);
                if peaks_tx.send((path, peaks)).is_err() {
                    break; // receiver dropped
                }
            });
        }

        let (analysis_drain_tx, analysis_done_rx) = spawn_analysis_worker();

        let mut browser = SampleBrowser {
            tabs: vec![TabState::default()],
            active_tab: 0,
            playback,
            last_error: None,
            libraries: Vec::new(),
            peaks_request,
            peaks_result_rx,
            peaks_requested_for: None,
            analysis_drain_tx,
            analysis_done_rx,
        };

        browser.playback.set_volume(cfg.volume);

        let (valid_tabs, active_idx) = restore_tab_plan(&cfg.tabs, cfg.active_tab);
        if valid_tabs.is_empty() {
            // No persisted tab set (fresh install, a config saved before tabs
            // existed, or every saved directory has since vanished): fall
            // back to the single last-opened directory, exactly as before
            // tabs were persisted.
            if let Some(dir) = cfg.last_directory.as_deref().filter(|p| p.is_dir()) {
                let _ = browser.open_directory(dir);
            }
        } else {
            let _ = browser.open_directory(&valid_tabs[0]);
            for dir in &valid_tabs[1..] {
                browser.new_tab(Some(dir));
            }
            browser.switch_tab(active_idx);
        }

        Ok(browser)
    }

    fn active(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    fn active_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    pub fn poll(&mut self) {
        if let Some(err) = self.playback.poll() {
            self.last_error = Some(err.to_string());
        }

        // Drain every tab's search channel, not just the active one, so a
        // search started in a tab still resolves while another tab is focused.
        let mut rebuild: Vec<usize> = Vec::new();
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if let Some(rx) = &tab.search_rx {
                match rx.try_recv() {
                    Ok(results) => {
                        tab.raw_results = Some(results);
                        tab.search_rx = None;
                        rebuild.push(i);
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        tab.raw_results = Some(Vec::new());
                        tab.search_rx = None;
                        rebuild.push(i);
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        for i in rebuild {
            self.rebuild_tab_results(i);
        }

        // Drain library scans the same way; on completion, reload the display
        // caches and refresh any tab whose results depend on that library.
        let mut scanned: Vec<usize> = Vec::new();
        let mut scan_errors: Vec<String> = Vec::new();
        for (i, ctx) in self.libraries.iter_mut().enumerate() {
            let Some(rx) = &ctx.scan_rx else { continue };
            match rx.try_recv() {
                Ok(result) => {
                    ctx.scan_rx = None;
                    ctx.scanning = false;
                    ctx.scan_progress = None;
                    match result {
                        Ok(s) => {
                            log::info!(
                                "library scan {}: {} added, {} moved, {} modified, {} missing, {} unchanged",
                                ctx.root.display(), s.added, s.moved, s.modified, s.missing, s.unchanged
                            );
                            ctx.reload();
                            scanned.push(i);
                        }
                        Err(e) => scan_errors.push(e.to_string()),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    ctx.scan_rx = None;
                    ctx.scanning = false;
                    ctx.scan_progress = None;
                    scan_errors.push("scan thread terminated unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        for e in scan_errors {
            log::error!("library scan: {e}");
            self.last_error = Some(format!("library scan: {e}"));
        }
        if !scanned.is_empty() {
            for i in 0..self.tabs.len() {
                if self.tabs[i]
                    .library_idx
                    .is_some_and(|li| scanned.contains(&li))
                {
                    self.rebuild_tab_results(i);
                }
            }
            // Jobs were enqueued during the scan; kick the worker to drain them.
            for &i in &scanned {
                let _ = self.analysis_drain_tx.send(self.libraries[i].root.clone());
            }
        }

        // Fold in finished analysis, capped per frame so a fast worker can't
        // stall a frame. Each completion refreshes just that asset's cache.
        for _ in 0..ANALYSIS_DRAIN_PER_FRAME {
            match self.analysis_done_rx.try_recv() {
                Ok(path) => self.apply_analysis_done(&path),
                Err(_) => break,
            }
        }

        self.poll_full_peaks();
    }

    /// Kick off / receive the full-source waveform for the current clip. A long
    /// file only shows its preview peaks until this fills in the whole shape,
    /// which is also what makes the entire source scrubbable.
    fn poll_full_peaks(&mut self) {
        // Request once per truncated clip. Short clips already decode whole, so
        // their preview peaks are the full waveform — no scan needed.
        if let (Some(info), Some(file)) =
            (self.playback.current_info(), self.playback.current_file())
        {
            if info.truncated && self.peaks_requested_for.as_deref() != Some(file) {
                self.peaks_requested_for = Some(file.to_path_buf());
                self.peaks_request.send(file.to_path_buf());
            }
        }

        // Apply any finished waveform to the engine (dropped if it no longer
        // matches the current file — the user moved on).
        while let Ok((path, peaks)) = self.peaks_result_rx.try_recv() {
            if let Some(peaks) = peaks {
                self.playback.set_full_peaks(&path, peaks);
            }
        }
    }

    pub fn open_directory(&mut self, path: &Path) -> Result<(), BrowserError> {
        let listing = punks_core::list_directory(path)?;
        {
            let tab = self.active_mut();
            tab.history = vec![path.to_path_buf()];
            tab.listing = Some(listing);
            tab.selected = None;
            // Browse is a fresh start for this tab: drop the tag filter too.
            tab.tag_filter.clear();
        }
        self.last_error = None;
        self.clear_search();
        self.attach_library_for_active();
        Ok(())
    }

    pub fn navigate_into(&mut self, index: usize) -> Result<(), BrowserError> {
        let path = {
            let entry = self.entries().get(index).ok_or(BrowserError::NoSelection)?;
            if !entry.is_directory {
                return Err(BrowserError::NoSelection);
            }
            entry.path.clone()
        };

        let listing = punks_core::list_directory(&path)?;
        let tab = self.active_mut();
        tab.history.push(path);
        tab.listing = Some(listing);
        tab.selected = None;
        self.attach_library_for_active();
        Ok(())
    }

    pub fn navigate_up(&mut self) -> Result<(), BrowserError> {
        if self.active().history.len() <= 1 {
            return Ok(());
        }
        let path = {
            let tab = self.active_mut();
            tab.history.pop();
            tab.history.last().unwrap().clone()
        };
        let listing = punks_core::list_directory(&path)?;
        let tab = self.active_mut();
        tab.listing = Some(listing);
        tab.selected = None;
        self.attach_library_for_active();
        Ok(())
    }

    pub fn navigate_to_breadcrumb(&mut self, level: usize) -> Result<(), BrowserError> {
        if level >= self.active().history.len() {
            return Ok(());
        }
        let path = {
            let tab = self.active_mut();
            tab.history.truncate(level + 1);
            tab.history.last().unwrap().clone()
        };
        let listing = punks_core::list_directory(&path)?;
        let tab = self.active_mut();
        tab.listing = Some(listing);
        tab.selected = None;
        self.attach_library_for_active();
        Ok(())
    }

    pub fn entries(&self) -> &[FileEntry] {
        self.active()
            .listing
            .as_ref()
            .map(|l| l.entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_directory(&self) -> Option<&Path> {
        self.active().history.last().map(PathBuf::as_path)
    }

    pub fn breadcrumbs(&self) -> Vec<String> {
        self.active()
            .history
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string_lossy().into_owned())
            })
            .collect()
    }
    pub fn can_navigate_up(&self) -> bool {
        self.active().history.len() > 1
    }

    pub fn select(&mut self, index: usize) {
        if index < self.entries().len() {
            self.active_mut().selected = Some(index);
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.active().selected
    }

    pub fn play_selected(&mut self) {
        let index = match self.active().selected {
            Some(i) => i,
            None => return,
        };
        let path = match self.entries().get(index) {
            Some(entry) if !entry.is_directory => entry.path.clone(),
            _ => return,
        };

        self.last_error = None;
        self.peaks_requested_for = None;
        self.playback.play(&path);
    }

    pub fn play_file(&mut self, path: &Path) {
        self.last_error = None;
        self.peaks_requested_for = None;
        self.playback.play(path);
    }

    pub fn stop(&mut self) {
        self.playback.stop();
    }

    pub fn playback_status(&self) -> PlaybackStatus {
        self.playback.status()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn waveform_peaks(&self) -> Option<&WaveformPeaks> {
        self.playback.waveform_peaks()
    }

    /// Container metadata + preview info for the current track (global, like
    /// playback). `None` when nothing is loaded.
    pub fn current_track_info(&self) -> Option<&TrackInfo> {
        self.playback.current_info()
    }

    /// The most-specific library owning `path`, if any (longest matching root, so
    /// a nested library wins over its parent).
    fn library_for_path(&self, path: &Path) -> Option<&LibraryContext> {
        self.libraries
            .iter()
            .filter(|c| path.starts_with(&c.root))
            .max_by_key(|c| c.root.as_os_str().len())
    }

    /// Analysis results for the loaded track, once the worker has computed them.
    /// `None` outside a library or before results land.
    pub fn current_analysis(&self) -> Option<AnalysisReport> {
        let path = self.playback.current_file()?;
        self.library_for_path(path)?.analysis.get(path).copied()
    }

    /// Whether the loaded track's analysis is still in flight (queued or running),
    /// so the UI can show an "analyzing…" placeholder until [`current_analysis`]
    /// fills in. Reads one indexed job row for the single loaded file.
    pub fn current_analysis_pending(&self) -> bool {
        let Some(path) = self.playback.current_file() else {
            return false;
        };
        let Some(ctx) = self.library_for_path(path) else {
            return false;
        };
        if ctx.analysis.contains_key(path) {
            return false;
        }
        matches!(
            ctx.lib.job_status(path).ok().flatten().as_deref(),
            Some("pending") | Some("running")
        )
    }

    /// Fold one finished analysis into its library's cache, refreshing just that
    /// asset so the next draw reflects it. Picks the most-specific owning library.
    fn apply_analysis_done(&mut self, path: &Path) {
        let Some(i) = self
            .libraries
            .iter()
            .enumerate()
            .filter(|(_, c)| path.starts_with(&c.root))
            .max_by_key(|(_, c)| c.root.as_os_str().len())
            .map(|(i, _)| i)
        else {
            return;
        };
        let ctx = &mut self.libraries[i];
        match ctx.lib.analysis_metrics(path) {
            Ok(metrics) => {
                ctx.analysis
                    .insert(path.to_path_buf(), AnalysisReport::from_metrics(&metrics));
            }
            Err(e) => log::warn!("analysis refresh {path:?}: {e}"),
        }
    }

    /// The `(start, duration)` in source seconds the displayed waveform spans,
    /// or `None` when nothing is loaded. The UI maps the playhead and scrub
    /// clicks against this.
    pub fn waveform_axis(&self) -> Option<(f64, f64)> {
        self.playback.waveform_axis()
    }

    /// Seek to `target` seconds into the source and play from there (decoding
    /// an on-demand window if it's past the loaded region).
    pub fn seek_to(&mut self, target: Duration) {
        self.playback.seek_to(target);
    }

    pub fn set_volume(&self, v: f32) {
        self.playback.set_volume(v);
    }

    pub fn volume(&self) -> f32 {
        self.playback.volume()
    }

    pub fn search(&mut self, query: &str) {
        let root = match self.current_directory() {
            Some(p) => p.to_path_buf(),
            None => return,
        };
        let query = query.to_string();
        let (tx, rx) = mpsc::channel();
        let thread_query = query.clone();
        std::thread::spawn(move || {
            let results = punks_core::search_directory(&root, &thread_query, SUPPORTED_EXTENSIONS)
                .unwrap_or_else(|e| {
                    log::warn!("search in {}: {e}", root.display());
                    Vec::new()
                });
            let _ = tx.send(results);
        });
        let tab = self.active_mut();
        tab.search_rx = Some(rx);
        tab.search_results = None;
        tab.raw_results = None;
        tab.search_selected = None;
        tab.search_query = query;
    }

    /// Clear the text search. An active tag filter stays; the results view
    /// falls back to the library-wide tag query if one is set.
    pub fn clear_search(&mut self) {
        let tab = self.active_mut();
        tab.search_results = None;
        tab.raw_results = None;
        tab.search_rx = None;
        tab.search_selected = None;
        tab.search_query = String::new();
        let i = self.active_tab;
        self.rebuild_tab_results(i);
    }

    pub fn is_searching(&self) -> bool {
        self.active().search_rx.is_some()
    }

    pub fn is_in_search_mode(&self) -> bool {
        self.active().search_results.is_some() || self.active().search_rx.is_some()
    }

    pub fn search_results(&self) -> Option<&[FileEntry]> {
        self.active().search_results.as_deref()
    }

    pub fn search_selected(&self) -> Option<usize> {
        self.active().search_selected
    }

    pub fn select_search_result(&mut self, index: usize) {
        let valid = self
            .active()
            .search_results
            .as_ref()
            .is_some_and(|r| index < r.len());
        if valid {
            self.active_mut().search_selected = Some(index);
        }
    }

    // --- Library / tags -----------------------------------------------------

    /// Rebuild the active display list from (raw text results) AND (tag
    /// filter). Called on state changes only — never per frame.
    fn rebuild_tab_results(&mut self, tab_idx: usize) {
        let filter = self.tabs[tab_idx].tag_filter.clone();
        let lib_idx = self.tabs[tab_idx].library_idx;

        let display: Option<Vec<FileEntry>> = match (lib_idx, filter.is_empty()) {
            (_, true) | (None, _) => self.tabs[tab_idx].raw_results.clone(),
            (Some(li), false) => {
                let ctx = &self.libraries[li];
                let passes = |path: &Path| {
                    ctx.asset_tags
                        .get(path)
                        .is_some_and(|ids| filter.iter().all(|t| ids.contains(t)))
                };
                let base: Vec<FileEntry> = match &self.tabs[tab_idx].raw_results {
                    // Text search active: AND the tag filter into its results.
                    Some(raw) => raw.iter().filter(|e| passes(&e.path)).cloned().collect(),
                    // Tags only: a library-wide query shown as results.
                    None => ctx
                        .assets
                        .iter()
                        .filter(|e| passes(&e.path))
                        .cloned()
                        .collect(),
                };
                Some(base)
            }
        };

        let tab = &mut self.tabs[tab_idx];
        if tab.library_idx.is_none() {
            // A filter without a library is meaningless.
            tab.tag_filter.clear();
        }
        tab.search_results = display;
        match &tab.search_results {
            Some(r) if !r.is_empty() => {
                if let Some(s) = tab.search_selected {
                    if s >= r.len() {
                        tab.search_selected = Some(r.len() - 1);
                    }
                }
            }
            _ => tab.search_selected = None,
        }
    }

    /// Bind the active tab to the library owning its current directory, if
    /// any. Attaching an EXISTING library is automatic (its presence is proof
    /// of a prior deliberate choice); creating one is never implicit — see
    /// [`init_library`](Self::init_library).
    fn attach_library_for_active(&mut self) {
        let dir = self.current_directory().map(Path::to_path_buf);
        let idx = dir.as_deref().and_then(|d| self.find_or_open_library(d));
        if self.active().library_idx != idx {
            let tab = self.active_mut();
            tab.library_idx = idx;
            tab.tag_filter.clear();
            let i = self.active_tab;
            self.rebuild_tab_results(i);
        }
    }

    fn find_or_open_library(&mut self, dir: &Path) -> Option<usize> {
        if let Some(i) = self.libraries.iter().position(|c| dir.starts_with(&c.root)) {
            return Some(i);
        }
        let root = Library::find_root(dir)?;
        match Library::open(&root) {
            Ok(lib) => Some(self.push_library(lib)),
            Err(e) => {
                log::warn!("failed to open library at {}: {e}", root.display());
                None
            }
        }
    }

    /// Register an opened library: load whatever a previous session stored so
    /// tags show immediately, then freshen against the disk in the background.
    fn push_library(&mut self, lib: Library) -> usize {
        let root = lib.root().to_path_buf();
        let mut ctx = LibraryContext {
            root: root.clone(),
            lib,
            tags: Vec::new(),
            assets: Vec::new(),
            asset_tags: HashMap::new(),
            analysis: HashMap::new(),
            scanning: true,
            scan_rx: None,
            scan_progress: None,
        };
        ctx.reload();
        let (rx, progress) = spawn_scan(root);
        ctx.scan_rx = Some(rx);
        ctx.scan_progress = Some(progress);
        self.libraries.push(ctx);
        self.libraries.len() - 1
    }

    pub fn library_state(&self) -> LibraryState {
        match self.active().library_idx {
            None => LibraryState::NotALibrary,
            Some(i) if self.libraries[i].scanning => LibraryState::Scanning,
            Some(_) => LibraryState::Ready,
        }
    }

    /// Live scan progress `(done, total)` for the active library, or `None` when
    /// not scanning. `total == 0` means the tree is still being walked.
    pub fn scan_progress(&self) -> Option<(usize, usize)> {
        use std::sync::atomic::Ordering;
        let ctx = &self.libraries[self.active().library_idx?];
        let p = ctx.scan_progress.as_ref()?;
        ctx.scanning.then(|| {
            (
                p.done.load(Ordering::Relaxed),
                p.total.load(Ordering::Relaxed),
            )
        })
    }

    /// Explicitly create a library rooted at the current directory. The only
    /// path that ever writes a `.punks` folder — must stay behind a clearly
    /// labelled user action.
    pub fn init_library(&mut self) {
        if self.active().library_idx.is_some() {
            return;
        }
        let Some(dir) = self.current_directory().map(Path::to_path_buf) else {
            return;
        };
        match Library::create(&dir) {
            Ok(lib) => {
                let idx = self.push_library(lib);
                self.active_mut().library_idx = Some(idx);
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// Delete the active tab's library store (`.punks/`) from disk. A testing
    /// convenience — destructive and irreversible; the UI gates it behind a
    /// confirmation.
    pub fn delete_active_library(&mut self) {
        let Some(li) = self.active().library_idx else {
            return;
        };
        let root = self.libraries[li].root.clone();
        // Drop the LibraryContext (closing its SQLite connection and the scan
        // receiver) before touching the folder on disk.
        self.libraries.remove(li);
        // Re-point tabs, same shift logic as closing a tab: those inside the
        // removed library detach; those after it shift down one.
        for tab in &mut self.tabs {
            match tab.library_idx {
                Some(x) if x == li => {
                    tab.library_idx = None;
                    tab.tag_filter.clear();
                }
                Some(x) if x > li => tab.library_idx = Some(x - 1),
                _ => {}
            }
        }
        // ponytail: on Windows a mid-flight scan/peaks worker connection can
        // block this delete; fine on unix (unlink-while-open). Upgrade path:
        // quiesce workers before removing.
        if let Err(e) = std::fs::remove_dir_all(root.join(".punks")) {
            self.last_error = Some(format!("delete library: {e}"));
        }
        for i in 0..self.tabs.len() {
            self.rebuild_tab_results(i);
        }
    }

    /// Tags of the active tab's library, with usage counts (empty if none).
    pub fn library_tags(&self) -> &[TagCount] {
        self.active()
            .library_idx
            .map(|i| self.libraries[i].tags.as_slice())
            .unwrap_or(&[])
    }

    pub fn tag_filter(&self) -> &[i64] {
        &self.active().tag_filter
    }

    pub fn toggle_tag_filter(&mut self, tag_id: i64) {
        let tab = self.active_mut();
        if let Some(pos) = tab.tag_filter.iter().position(|&t| t == tag_id) {
            tab.tag_filter.remove(pos);
        } else {
            tab.tag_filter.push(tag_id);
        }
        let i = self.active_tab;
        self.rebuild_tab_results(i);
    }

    pub fn clear_tag_filter(&mut self) {
        self.active_mut().tag_filter.clear();
        let i = self.active_tab;
        self.rebuild_tab_results(i);
    }

    /// Tag names for a file, for inline pill display (cache lookup, no DB).
    pub fn tag_names_for_path(&self, path: &Path) -> Vec<String> {
        let Some(li) = self.active().library_idx else {
            return Vec::new();
        };
        let ctx = &self.libraries[li];
        let Some(ids) = ctx.asset_tags.get(path) else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|id| ctx.tags.iter().find(|t| t.id == *id))
            .map(|t| t.name.clone())
            .collect()
    }

    pub fn tag_ids_for_path(&self, path: &Path) -> Vec<i64> {
        self.active()
            .library_idx
            .and_then(|li| self.libraries[li].asset_tags.get(path).cloned())
            .unwrap_or_default()
    }

    pub fn create_tag(&mut self, name: &str) {
        let Some(li) = self.active().library_idx else {
            return;
        };
        if let Err(e) = self.libraries[li].lib.create_tag(name) {
            self.last_error = Some(e.to_string());
        }
        self.after_tag_mutation(li);
    }

    pub fn assign_tag(&mut self, path: &Path, tag_id: i64) {
        let Some(li) = self.active().library_idx else {
            return;
        };
        if let Err(e) = self.libraries[li].lib.assign_tag(path, tag_id) {
            self.last_error = Some(e.to_string());
        }
        self.after_tag_mutation(li);
    }

    pub fn unassign_tag(&mut self, path: &Path, tag_id: i64) {
        let Some(li) = self.active().library_idx else {
            return;
        };
        if let Err(e) = self.libraries[li].lib.remove_tag(path, tag_id) {
            self.last_error = Some(e.to_string());
        }
        self.after_tag_mutation(li);
    }

    pub fn create_and_assign_tag(&mut self, path: &Path, name: &str) {
        let Some(li) = self.active().library_idx else {
            return;
        };
        match self.libraries[li].lib.create_tag(name) {
            Ok(tag) => {
                if let Err(e) = self.libraries[li].lib.assign_tag(path, tag.id) {
                    self.last_error = Some(e.to_string());
                }
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
        self.after_tag_mutation(li);
    }

    pub fn delete_tag(&mut self, tag_id: i64) {
        let Some(li) = self.active().library_idx else {
            return;
        };
        if let Err(e) = self.libraries[li].lib.delete_tag(tag_id) {
            self.last_error = Some(e.to_string());
        }
        for tab in &mut self.tabs {
            tab.tag_filter.retain(|&t| t != tag_id);
        }
        self.after_tag_mutation(li);
    }

    fn after_tag_mutation(&mut self, li: usize) {
        self.libraries[li].reload();
        for i in 0..self.tabs.len() {
            if self.tabs[i].library_idx == Some(li) && !self.tabs[i].tag_filter.is_empty() {
                self.rebuild_tab_results(i);
            }
        }
    }

    // --- Tab management ---------------------------------------------------

    /// Create a new tab and make it active. `start` selects its initial
    /// directory: `Some(dir)` opens that directory in the new tab, `None`
    /// leaves it blank. The caller owns the policy (clone current / blank /
    /// last-saved) so it can be made pref-driven later.
    pub fn new_tab(&mut self, start: Option<&Path>) {
        self.tabs.push(TabState::default());
        self.active_tab = self.tabs.len() - 1;
        if let Some(dir) = start {
            let _ = self.open_directory(dir);
        }
    }

    /// Close the tab at `index`. No-op if only one tab remains.
    pub fn close_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        self.active_tab = adjust_active_after_close(self.active_tab, index, self.tabs.len());
    }

    /// Make `index` the active tab (no-op if out of range).
    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    /// Move the tab at `from` to position `to`, keeping the same logical tab
    /// active. Used by drag-to-reorder.
    pub fn reorder_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active_tab = adjust_active_after_reorder(self.active_tab, from, to);
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_tab(&self) -> usize {
        self.active_tab
    }

    /// Title for the tab at `index`: its current directory's name, falling
    /// back to the full path, or "New Tab" when no folder is open.
    pub fn tab_title(&self, index: usize) -> String {
        match self.tabs.get(index).and_then(|t| t.history.last()) {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned()),
            None => "New Tab".to_string(),
        }
    }

    /// The active tab's committed search text (for the UI search box to
    /// restore on tab switch).
    pub fn search_query(&self) -> &str {
        &self.active().search_query
    }

    /// Each tab's current directory, in tab order, blank tabs omitted. Used
    /// by the UI to detect changes and persist the open tab set.
    pub fn tab_directories(&self) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .filter_map(|t| t.history.last().cloned())
            .collect()
    }
}

/// Which of `saved_tabs` to restore, and which one was active: keeps only
/// paths that still exist as directories (a moved/deleted folder is silently
/// dropped rather than restored as an error), preserving order, and clamps
/// `saved_active` into range. Pure (does the filesystem check but nothing
/// else) so it's testable without constructing a `SampleBrowser`, which needs
/// a real audio device.
fn restore_tab_plan(saved_tabs: &[PathBuf], saved_active: usize) -> (Vec<PathBuf>, usize) {
    let valid: Vec<PathBuf> = saved_tabs.iter().filter(|p| p.is_dir()).cloned().collect();
    if valid.is_empty() {
        return (Vec::new(), 0);
    }
    let active = saved_active.min(valid.len() - 1);
    (valid, active)
}

/// Active-tab index after removing the tab at `removed`. `new_len` is the tab
/// count *after* removal (>= 1). Closing a tab left of the active one shifts it
/// down; closing the active tab focuses the tab that slid into its slot,
/// clamped to the last tab.
fn adjust_active_after_close(active: usize, removed: usize, new_len: usize) -> usize {
    if active > removed {
        active - 1
    } else if active == removed {
        active.min(new_len - 1)
    } else {
        active
    }
}

/// Active-tab index after moving a tab from `from` to `to`, keeping the same
/// logical tab focused.
fn adjust_active_after_reorder(active: usize, from: usize, to: usize) -> usize {
    if active == from {
        to
    } else {
        let mut a = active;
        if from < a {
            a -= 1;
        }
        if to <= a {
            a += 1;
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::{adjust_active_after_close, adjust_active_after_reorder, restore_tab_plan};
    use std::path::PathBuf;

    /// A couple of real temp directories plus a path that doesn't exist, for
    /// exercising restore_tab_plan's filesystem check.
    struct TempDirs {
        base: PathBuf,
        a: PathBuf,
        b: PathBuf,
        vanished: PathBuf,
    }
    impl TempDirs {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "punks2_tabrestore_{}_{tag}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let a = base.join("a");
            let b = base.join("b");
            std::fs::create_dir_all(&a).unwrap();
            std::fs::create_dir_all(&b).unwrap();
            TempDirs {
                vanished: base.join("gone"),
                base,
                a,
                b,
            }
        }
    }
    impl Drop for TempDirs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn restore_tab_plan_skips_vanished_and_clamps_active() {
        let d = TempDirs::new("skip");
        // Saved order was [a, gone, b]; "gone" no longer exists on disk.
        let saved = vec![d.a.clone(), d.vanished.clone(), d.b.clone()];
        // saved_active=2 pointed at `b` in the original 3-entry list; after
        // dropping the vanished middle entry there are only 2 valid dirs, so
        // it clamps to the last one (still `b`, index 1).
        let (tabs, active) = restore_tab_plan(&saved, 2);
        assert_eq!(tabs, vec![d.a.clone(), d.b.clone()]);
        assert_eq!(active, 1);
    }

    #[test]
    fn restore_tab_plan_all_vanished_is_empty() {
        let (tabs, active) = restore_tab_plan(&[PathBuf::from("/does/not/exist")], 5);
        assert!(tabs.is_empty());
        assert_eq!(active, 0);
    }

    #[test]
    fn restore_tab_plan_in_range_active_is_unchanged() {
        let d = TempDirs::new("inrange");
        let saved = vec![d.a.clone(), d.b.clone()];
        let (tabs, active) = restore_tab_plan(&saved, 0);
        assert_eq!(tabs, saved);
        assert_eq!(active, 0);
    }

    #[test]
    fn close_left_of_active_shifts_down() {
        // [0,1,2,3], active=2, close 0 -> [1,2,3], active follows to 1
        assert_eq!(adjust_active_after_close(2, 0, 3), 1);
    }

    #[test]
    fn close_right_of_active_keeps_index() {
        // active=1, close 3 -> active unchanged
        assert_eq!(adjust_active_after_close(1, 3, 3), 1);
    }

    #[test]
    fn close_active_focuses_right_neighbor() {
        // [0,1,2], active=1, close 1 -> [0,2], focus the tab now at index 1
        assert_eq!(adjust_active_after_close(1, 1, 2), 1);
    }

    #[test]
    fn close_active_last_clamps() {
        // [0,1,2], active=2, close 2 -> [0,1], clamp to last index 1
        assert_eq!(adjust_active_after_close(2, 2, 2), 1);
    }

    #[test]
    fn reorder_moves_active_with_it() {
        // active tab dragged from 2 to 0 -> active is now 0
        assert_eq!(adjust_active_after_reorder(2, 2, 0), 0);
    }

    #[test]
    fn reorder_non_active_left_to_right_past_active() {
        // [a,b,c,d], active=1(b), move a:0->3 => [b,c,d,a], b now at 0
        assert_eq!(adjust_active_after_reorder(1, 0, 3), 0);
    }

    #[test]
    fn reorder_non_active_right_to_left_before_active() {
        // [a,b,c,d], active=1(b), move d:3->0 => [d,a,b,c], b now at 2
        assert_eq!(adjust_active_after_reorder(1, 3, 0), 2);
    }
}
