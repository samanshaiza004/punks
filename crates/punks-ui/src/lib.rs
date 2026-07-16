use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use imgui::Key;
use punks_browser::{
    Capability, Fact, Field, HealthIssue, HealthIssueKind, LibraryState, Metadata, MetadataSource,
    PlaybackStatus, SampleBrowser, TagCount,
};
use punks_core::config::{Keybinds, PunksConfig};

mod theme;

pub use theme::apply_theme;

/// One editable metadata field's panel state, seeded from the file once per
/// selection (see `seed_metadata_editor`).
#[derive(Default, Clone)]
struct MetaField {
    /// The edit buffer (keywords are shown as a comma-separated list).
    buf: String,
    /// The value `buf` was seeded from — `buf != seed` means "edited",
    /// which gates Save so a value shown only because the field loaded blank
    /// can't overwrite real content.
    seed: String,
    writable: bool,
    max_len: Option<usize>,
    /// Where the shown value came from, for the provenance badge (today
    /// always `Embedded`; the panel is shaped for Override/Analysis/Project
    /// if a metadata-override layer ever lands).
    source: Option<MetadataSource>,
}

impl MetaField {
    fn dirty(&self) -> bool {
        self.buf != self.seed
    }
}

/// Per-file metadata editor state for the Metadata panel.
#[derive(Default)]
struct MetadataEditor {
    path: Option<PathBuf>,
    description: MetaField,
    keywords: MetaField,
    category: MetaField,
    creator: MetaField,
}

/// Short label for a value's provenance badge.
fn source_label(source: MetadataSource) -> &'static str {
    match source {
        MetadataSource::Embedded => "Embedded",
        MetadataSource::Override => "Override",
        MetadataSource::Analysis => "Analysis",
        MetadataSource::Project => "Project",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BrowserAction {
    NavigateUp,
    NavigateDown,
    NavigateBack,
    Confirm,
    NewTab,
    CloseTab,
    PrevTab,
    NextTab,
    Undo,
    Redo,
    TogglePlayback,
    ToggleInspector,
}

const CAPTURABLE_KEYS: &[(Key, &str)] = &[
    (Key::A, "A"),
    (Key::B, "B"),
    (Key::C, "C"),
    (Key::D, "D"),
    (Key::E, "E"),
    (Key::F, "F"),
    (Key::G, "G"),
    (Key::H, "H"),
    (Key::I, "I"),
    (Key::J, "J"),
    (Key::K, "K"),
    (Key::L, "L"),
    (Key::M, "M"),
    (Key::N, "N"),
    (Key::O, "O"),
    (Key::P, "P"),
    (Key::Q, "Q"),
    (Key::R, "R"),
    (Key::S, "S"),
    (Key::T, "T"),
    (Key::U, "U"),
    (Key::V, "V"),
    (Key::W, "W"),
    (Key::X, "X"),
    (Key::Y, "Y"),
    (Key::Z, "Z"),
    (Key::Enter, "Enter"),
    (Key::Space, "Space"),
    (Key::UpArrow, "UpArrow"),
    (Key::DownArrow, "DownArrow"),
    (Key::LeftArrow, "LeftArrow"),
    (Key::RightArrow, "RightArrow"),
    (Key::Tab, "Tab"),
];

fn parse_key(s: &str) -> Option<Key> {
    CAPTURABLE_KEYS
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(s))
        .map(|(k, _)| *k)
}

fn key_name(k: Key) -> &'static str {
    CAPTURABLE_KEYS
        .iter()
        .find(|(key, _)| *key == k)
        .map(|(_, name)| *name)
        .unwrap_or("?")
}

fn keybind_field_mut(keybinds: &mut Keybinds, action: BrowserAction) -> &mut String {
    match action {
        BrowserAction::NavigateUp => &mut keybinds.navigate_up,
        BrowserAction::NavigateDown => &mut keybinds.navigate_down,
        BrowserAction::NavigateBack => &mut keybinds.navigate_back,
        BrowserAction::Confirm => &mut keybinds.confirm,
        BrowserAction::NewTab => &mut keybinds.new_tab,
        BrowserAction::CloseTab => &mut keybinds.close_tab,
        BrowserAction::PrevTab => &mut keybinds.prev_tab,
        BrowserAction::NextTab => &mut keybinds.next_tab,
        BrowserAction::Undo => &mut keybinds.undo,
        BrowserAction::Redo => &mut keybinds.redo,
        BrowserAction::TogglePlayback => &mut keybinds.play_stop,
        BrowserAction::ToggleInspector => &mut keybinds.toggle_inspector,
    }
}

fn keybind_field(keybinds: &Keybinds, action: BrowserAction) -> &str {
    match action {
        BrowserAction::NavigateUp => &keybinds.navigate_up,
        BrowserAction::NavigateDown => &keybinds.navigate_down,
        BrowserAction::NavigateBack => &keybinds.navigate_back,
        BrowserAction::Confirm => &keybinds.confirm,
        BrowserAction::NewTab => &keybinds.new_tab,
        BrowserAction::CloseTab => &keybinds.close_tab,
        BrowserAction::PrevTab => &keybinds.prev_tab,
        BrowserAction::NextTab => &keybinds.next_tab,
        BrowserAction::Undo => &keybinds.undo,
        BrowserAction::Redo => &keybinds.redo,
        BrowserAction::TogglePlayback => &keybinds.play_stop,
        BrowserAction::ToggleInspector => &keybinds.toggle_inspector,
    }
}

const KEYBIND_ACTIONS: &[(BrowserAction, &str)] = &[
    (BrowserAction::NavigateUp, "Navigate up"),
    (BrowserAction::NavigateDown, "Navigate down"),
    (BrowserAction::NavigateBack, "Back"),
    (BrowserAction::Confirm, "Confirm / Play"),
    (BrowserAction::NewTab, "New tab"),
    (BrowserAction::CloseTab, "Close tab"),
    (BrowserAction::PrevTab, "Previous tab"),
    (BrowserAction::NextTab, "Next tab"),
    (BrowserAction::Undo, "Undo"),
    (BrowserAction::Redo, "Redo"),
    (BrowserAction::TogglePlayback, "Play / Stop"),
    (BrowserAction::ToggleInspector, "Toggle inspector"),
];

// text_muted.
const TAB_CLOSE_TEXT: [f32; 4] = [0.22745, 0.28235, 0.36078, 1.0];

// selection darkened 20%.
const DIR_TEXT_COLOR: [f32; 4] = [0.18510, 0.37647, 0.65255, 1.0];

// Left rail: library status + tag filters live here; tags on rows are pills.
const SIDEBAR_WIDTH: f32 = 180.0;
/// Width of the drag strip between Results and the Inspector.
const SPLITTER_WIDTH: f32 = 6.0;
/// Height of the waveform strip. Shared so the panel can reserve exactly the
/// right room below the file list for it + the metadata lines + transport row.
const WAVEFORM_HEIGHT: f32 = 64.0;
// surface.
const SIDEBAR_BG: [f32; 4] = [0.77255, 0.82745, 0.88627, 1.0];
// surface_inset.
const PILL_BG: [f32; 4] = [0.68235, 0.74902, 0.82353, 1.0];
// text.
const PILL_TEXT: [f32; 4] = [0.06275, 0.09020, 0.13333, 1.0];

// File list lays out entries in width-adaptive columns; each column is at least
// this wide, so wide windows show 2+ columns and narrow ones collapse to 1.
const MIN_COLUMN_WIDTH: f32 = 300.0;
const COLUMN_GUTTER: f32 = 8.0;

fn column_count(avail_width: f32) -> usize {
    ((avail_width / MIN_COLUMN_WIDTH).floor() as usize).max(1)
}

/// Scroll the child window (must be the current window) so row `row` is
/// visible, nudging the minimum distance rather than re-centering — so it
/// doesn't fight a scroll position the user set deliberately.
fn scroll_row_into_view(ui: &imgui::Ui, row: usize, row_stride: f32) {
    let row_top = row as f32 * row_stride;
    let row_bottom = row_top + row_stride;
    let view_top = ui.scroll_y();
    let view_bottom = view_top + ui.window_size()[1];
    if row_top < view_top {
        ui.set_scroll_y(row_top);
    } else if row_bottom > view_bottom {
        ui.set_scroll_y(row_bottom - ui.window_size()[1]);
    }
}

/// Display a `Fact` value as a plain string (integers without a trailing `.0`).
fn fact_display(f: &Fact) -> String {
    match f {
        Fact::Text(s) => s.clone(),
        Fact::Real(v) if v.fract() == 0.0 => format!("{}", *v as i64),
        Fact::Real(v) => format!("{v}"),
        Fact::Blob(_) => "<blob>".into(),
    }
}

/// One-line human-readable description of a metadata health issue, for the
/// sidebar health panel. `{:?}` on `Field` prints its bare variant name
/// (`Description`, `Category`, …) since it's a data-less enum, so it doubles
/// as the display label without a separate lookup table.
fn health_issue_summary(issue: &HealthIssue) -> String {
    match &issue.kind {
        HealthIssueKind::CacheDrift { embedded, cached } => {
            format!(
                "{:?} changed since last scan (file: \"{embedded}\", cached: \"{cached}\")",
                issue.field
            )
        }
        HealthIssueKind::OverrideDrift {
            embedded,
            overridden,
        } => format!(
            "{:?} disagrees with override (file: \"{embedded}\", override: \"{overridden}\")",
            issue.field
        ),
    }
}

/// Uppercase the first character (canonical instruments are stored lowercase).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Track length for the analysis readout. Sample libraries are mostly sub-second
/// one-shots, so show tenths under a minute (`0.3s`, `12.4s`) and `m:ss` above.
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let total = secs as u64;
        format!("{}:{:02}", total / 60, total % 60)
    }
}

/// bext TimeReference (sample count) as `hh:mm:ss.mmm` start timecode.
fn format_timecode(samples: u64, sample_rate: u32) -> String {
    if sample_rate == 0 {
        return "--".into();
    }
    let total_ms = (samples as f64 / sample_rate as f64 * 1000.0) as u64;
    let ms = total_ms % 1000;
    let s = (total_ms / 1000) % 60;
    let m = (total_ms / 60_000) % 60;
    let h = total_ms / 3_600_000;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

fn relative_parent(root: Option<&Path>, file_path: &Path) -> String {
    let parent = match file_path.parent() {
        Some(p) => p,
        None => return String::new(),
    };
    if let Some(root) = root {
        if let Ok(rel) = parent.strip_prefix(root) {
            let s = rel.to_string_lossy();
            if s.is_empty() {
                return ".".into();
            }
            return s.into_owned();
        }
    }
    parent
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub struct BrowserPanel {
    prefs: PunksConfig,
    rebinding: Option<BrowserAction>,
    /// Warning shown when a rebind was refused because the chosen key is
    /// already bound to another action. Cleared when a new rebind starts.
    rebind_conflict: Option<String>,
    search_buf: String,
    last_typed_query: String,
    query_change_time: Instant,
    last_searched_query: String,
    volume: f32,
    /// Tracks the active tab between frames so the search box can be reloaded
    /// from the newly active tab's stored query when the user switches tabs.
    last_active_tab: usize,
    /// Last mouse-x we seeked to during a waveform drag, so a held-still cursor
    /// lets audio play forward instead of re-seeking every frame. `None` when
    /// not scrubbing.
    scrub_last_x: Option<f32>,
    /// Set when W/S keyboard nav moves the selection; consumed on the next
    /// list render to scroll the newly selected row into view if it isn't.
    scroll_to_selected: bool,
    /// Sidebar "new tag" input buffer.
    new_tag_buf: String,
    /// "New tag" buffer inside the per-file tag popup.
    popup_tag_buf: String,
    /// File the tag-editor popup is editing (set by the Inspector, or by
    /// right-click where that works).
    tag_popup_path: Option<PathBuf>,
    open_tag_editor: bool,
    /// Tag pending delete confirmation (set by right-click in the sidebar).
    tag_delete_target: Option<(i64, String)>,
    open_tag_delete: bool,
    /// Request to open the "delete library" confirmation popup.
    open_delete_library: bool,
    /// File the override popup is editing, and its per-metric edit buffers
    /// (seeded from the current override when the popup opens).
    override_popup_path: Option<PathBuf>,
    override_bufs: HashMap<&'static str, String>,
    open_override_editor: bool,
    /// Mirrors `browser.last_error()` into an owned buffer so it can be shown in
    /// a read-only (but selectable/copyable) input widget — plain `Text` widgets
    /// can't be selected or copied in imgui.
    error_buf: String,
    /// Metadata editor state (Description/Keywords/Category/Creator), reseeded
    /// whenever the loaded file changes so switching clips doesn't leak the
    /// previous one's unsaved edit into the new one. Shown read-only in the
    /// Inspector's Metadata section; edited in a modal (see `open_metadata_editor`).
    editor: MetadataEditor,
    /// Request to open the metadata editor modal (set by the Inspector's
    /// Actions/Metadata buttons, consumed at panel scope with the other popups).
    open_metadata_editor: bool,
    /// BPM range filter edit buffers, resynced from `browser.fact_filter()`
    /// on tab switch (like `search_buf`) so they don't leak one tab's
    /// in-progress edit into another.
    bpm_min: f32,
    bpm_max: f32,
    #[cfg(debug_assertions)]
    row_decorations: std::cell::Cell<u32>,
}

/// The correctable detected facts the override popup edits, as
/// `(metric, label, numeric?)`.
const OVERRIDE_FIELDS: &[(&str, &str, bool)] = &[
    ("instrument", "Instrument", false),
    ("key", "Key", false),
    ("bpm", "Tempo", true),
];

impl BrowserPanel {
    pub fn new() -> Self {
        let prefs = punks_core::config::load();
        let volume = prefs.volume;
        BrowserPanel {
            prefs,
            rebinding: None,
            rebind_conflict: None,
            search_buf: String::new(),
            last_typed_query: String::new(),
            query_change_time: Instant::now(),
            last_searched_query: String::new(),
            volume,
            last_active_tab: 0,
            scrub_last_x: None,
            scroll_to_selected: false,
            new_tag_buf: String::new(),
            popup_tag_buf: String::new(),
            tag_popup_path: None,
            open_tag_editor: false,
            tag_delete_target: None,
            open_tag_delete: false,
            open_delete_library: false,
            override_popup_path: None,
            override_bufs: HashMap::new(),
            open_override_editor: false,
            error_buf: String::new(),
            editor: MetadataEditor::default(),
            open_metadata_editor: false,
            bpm_min: 0.0,
            bpm_max: 0.0,
            #[cfg(debug_assertions)]
            row_decorations: std::cell::Cell::new(0),
        }
    }

    /// The config loaded at construction, for callers (e.g. the app shell)
    /// that need to share it with other components instead of loading it a
    /// second time.
    pub fn prefs(&self) -> &PunksConfig {
        &self.prefs
    }

    /// Probe-only counter consumed by punks-standalone's PUNKS_UI_PERF probe;
    /// counts decorated clipped-list rows painted in the most recent draw.
    #[cfg(debug_assertions)]
    pub fn row_decorations_last_frame(&self) -> u32 {
        self.row_decorations.get()
    }

    pub fn draw(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
        on_drag_file: Option<&mut dyn FnMut(&Path)>,
    ) {
        #[cfg(debug_assertions)]
        self.row_decorations.set(0);

        browser.poll();
        let palette = theme::neon_palette();
        let tab_active_material = theme::tab_active_material();
        let tab_inactive_material = theme::tab_inactive_material();
        let toolbar_material = theme::toolbar_material();
        let raised_material = theme::raised_material();
        let inset_material = theme::inset_field_material();
        let volume_slider_style = theme::volume_slider_style();

        // When the active tab changes, reload the search box from that tab's
        // stored query and resync the debounce trackers so we don't re-issue a
        // search for text the tab already has results for.
        if self.last_active_tab != browser.active_tab() {
            self.search_buf = browser.search_query().to_string();
            self.last_typed_query = self.search_buf.clone();
            self.last_searched_query = self.search_buf.clone();
            self.query_change_time = Instant::now();
            let f = browser.fact_filter();
            self.bpm_min = f.bpm_min.unwrap_or(0.0);
            self.bpm_max = f.bpm_max.unwrap_or(0.0);
            self.last_active_tab = browser.active_tab();
        }

        // Persist the open tab set (directories + which one is active), so the
        // browser restores the same tabs across launches. One check here
        // covers every path that changes it — navigation (keyboard, click,
        // breadcrumb, Up, Browse), tab open/close/switch/reorder — and fires
        // at most once per actual change. `last_directory` rides along as the
        // active tab's directory: it's the pre-tabs fallback field, so (like
        // before) it only ever moves forward and is never clobbered with
        // `None` when the active tab is temporarily blank.
        let tab_dirs = browser.tab_directories();
        let active_idx = browser.active_tab();
        let current_dir = browser.current_directory().map(Path::to_path_buf);
        let tabs_changed = self.prefs.tabs != tab_dirs || self.prefs.active_tab != active_idx;
        let dir_changed = current_dir.is_some() && self.prefs.last_directory != current_dir;
        if tabs_changed || dir_changed {
            self.prefs.tabs = tab_dirs;
            self.prefs.active_tab = active_idx;
            if let Some(dir) = current_dir {
                self.prefs.last_directory = Some(dir);
            }
            punks_core::config::save(&self.prefs);
        }

        // --- Tab bar: switch / drag-reorder / close / new ------------------
        let mut switch_to: Option<usize> = None;
        let mut close_idx: Option<usize> = None;
        let mut reorder: Option<(usize, usize)> = None;
        let mut open_new_tab = false;
        let tab_count = browser.tab_count();
        let active_tab = browser.active_tab();

        for i in 0..tab_count {
            if i > 0 {
                ui.same_line();
            }
            let tab_material = if i == active_tab {
                tab_active_material
            } else {
                tab_inactive_material
            };

            let title = browser.tab_title(i);
            let active_text = (i == active_tab).then(|| {
                ui.push_style_color(
                    imgui::StyleColor::Text,
                    theme::color_f32(palette.border_light),
                )
            });
            // SAFETY: the closure submits exactly one stock button in the active window.
            let clicked = unsafe {
                imgui_painter::decorate_button(frame, &tab_material, || {
                    ui.button(format!("{title}##tab{i}"))
                })
            };
            if let Some(color) = active_text {
                color.pop();
            }
            if clicked {
                switch_to = Some(i);
            }

            // Drag this tab as a reorder source; payload is its index.
            if let Some(tooltip) = ui
                .drag_drop_source_config("TAB_REORDER")
                .flags(imgui::DragDropFlags::SOURCE_NO_PREVIEW_TOOLTIP)
                .begin_payload(i)
            {
                tooltip.end();
            }
            // Accept a dropped tab: the dragged tab lands at this index.
            if let Some(target) = ui.drag_drop_target() {
                if let Some(Ok(payload)) =
                    target.accept_payload::<usize, _>("TAB_REORDER", imgui::DragDropFlags::empty())
                {
                    reorder = Some((payload.data, i));
                }
                target.pop();
            }

            // Close glyph, attached flush to the label so the two read as one
            // tab. Hidden on the only tab so one always remains.
            if tab_count > 1 {
                ui.same_line_with_spacing(0.0, 0.0);
                let ctext = ui.push_style_color(imgui::StyleColor::Text, TAB_CLOSE_TEXT);
                // SAFETY: the closure submits exactly one stock button in the active window.
                let clicked = unsafe {
                    imgui_painter::decorate_button(frame, &tab_material, || {
                        ui.button(format!("\u{00d7}##closetab{i}"))
                    })
                };
                if clicked {
                    close_idx = Some(i);
                }
                ctext.pop();
            }
        }
        ui.same_line();
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("+##newtab"))
        } {
            open_new_tab = true;
        }
        ui.separator();

        // Apply deferred tab actions after the loop (mutating shifts indices).
        if let Some((from, to)) = reorder {
            browser.reorder_tab(from, to);
        }
        if let Some(i) = close_idx {
            browser.close_tab(i);
        }
        if open_new_tab {
            let start = browser.current_directory().map(|p| p.to_path_buf());
            browser.new_tab(start.as_deref());
        }
        if let Some(i) = switch_to {
            browser.switch_tab(i);
        }

        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Browse..."))
        } {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Err(e) = browser.open_directory(&path) {
                    log::error!("failed to open directory: {e}");
                } else {
                    // last_directory is persisted centrally at the top of draw().
                    self.search_buf.clear();
                    self.last_typed_query.clear();
                    self.last_searched_query.clear();
                }
            }
        }

        if browser.can_navigate_up() {
            ui.same_line();
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("^  Up"))
            } {
                if let Err(e) = browser.navigate_up() {
                    log::error!("navigate_up failed: {e}");
                }
            }
        }

        ui.same_line();
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Settings"))
        } {
            ui.open_popup("Settings##modal");
        }

        // Inspector toggle: hiding the right pane gives Results the full width.
        // The state is persisted (see the tab/volume save path below).
        ui.same_line();
        // SAFETY: the closure submits exactly one stock button in the active window.
        let inspector_clicked = unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || {
                ui.button(if self.prefs.inspector_visible {
                    "Inspector \u{25b8}"
                } else {
                    "Inspector \u{25c2}"
                })
            })
        };
        if inspector_clicked {
            self.prefs.inspector_visible = !self.prefs.inspector_visible;
            punks_core::config::save(&self.prefs);
        }

        self.draw_settings_modal(ui, browser, frame);

        let crumbs = browser.breadcrumbs();
        if !crumbs.is_empty() {
            ui.separator();
            for (i, crumb) in crumbs.iter().enumerate() {
                if i > 0 {
                    ui.same_line();
                    ui.text_disabled(">");
                    ui.same_line();
                }
                if i < crumbs.len() - 1 {
                    // SAFETY: the closure submits exactly one stock SmallButton in the active window.
                    let clicked = unsafe {
                        imgui_painter::decorate_button(frame, &toolbar_material, || {
                            ui.small_button(format!("{}##crumb{}", crumb, i))
                        })
                    };
                    if clicked {
                        if let Err(e) = browser.navigate_to_breadcrumb(i) {
                            log::error!("breadcrumb nav failed: {e}");
                        }
                    }
                } else {
                    ui.text(crumb);
                }
            }
        }

        ui.separator();

        let avail = ui.content_region_avail();
        // Reserve room below the body row for only what stays *pinned*: the
        // waveform + the transport row (Play/Stop + volume) + a line of slack for
        // the error line. The Inspector's metadata/facts now scroll inside their
        // own pane, so their length no longer figures into this reservation —
        // that's what keeps the transport on-screen no matter how much metadata a
        // file has. Derived from font metrics so it survives a font-size change.
        let line_h = ui.text_line_height_with_spacing();
        // The error line is a read-only input (frame-height, not text-line-height)
        // so its text is selectable/copyable.
        let reserved = WAVEFORM_HEIGHT + ui.frame_height_with_spacing() + line_h;
        let body_height = (avail[1] - reserved).max(120.0);
        let mut drag_requested: Option<PathBuf> = None;
        let mut search_focused = false;
        let mut in_search = false;

        let up_key = parse_key(&self.prefs.keybinds.navigate_up).unwrap_or(Key::W);
        let down_key = parse_key(&self.prefs.keybinds.navigate_down).unwrap_or(Key::S);
        let back_key = parse_key(&self.prefs.keybinds.navigate_back).unwrap_or(Key::A);
        let conf_key = parse_key(&self.prefs.keybinds.confirm).unwrap_or(Key::D);

        // Ctrl/Cmd+F jumps straight to the search box (browser-style). Detected
        // at panel scope, applied inside the content child right before the input.
        let focus_search = ui.is_window_focused()
            && (ui.io().key_ctrl || ui.io().key_super)
            && ui.is_key_pressed_no_repeat(Key::F);

        // Reseed the metadata editor from the newly-selected file (once per
        // selection change) before anything reads it this frame — the Inspector
        // summary below and the editor modal at panel scope both depend on it.
        seed_metadata_editor(browser, &mut self.editor);

        // Three-pane body row: Sidebar | Results | Inspector. Results is the
        // star and gets whatever width the other two don't take; the Inspector
        // is fixed-width (drag the splitter to resize) and disappears entirely
        // when toggled off, handing its space back to Results. Widths are
        // computed up front because a child window needs its width set before it
        // is drawn, and same_line panes can't be measured after the fact.
        let spacing = ui.clone_style().item_spacing[0];
        let full_w = ui.content_region_avail()[0];
        let inspector_w = if self.prefs.inspector_visible {
            self.prefs.inspector_width
        } else {
            0.0
        };
        // Same_line gaps between panes: sidebar|content always; plus
        // content|splitter and splitter|inspector when the inspector is shown.
        let (splitter_w, gap_count) = if self.prefs.inspector_visible {
            (SPLITTER_WIDTH, 3.0)
        } else {
            (0.0, 1.0)
        };
        let content_w =
            (full_w - SIDEBAR_WIDTH - splitter_w - inspector_w - gap_count * spacing).max(160.0);

        // Left rail: library status + tag filters. Tag display on rows stays
        // inline (pills); the sidebar is only for filtering and library setup.
        let sidebar_bg = ui.push_style_color(imgui::StyleColor::ChildBg, SIDEBAR_BG);
        ui.child_window("sidebar")
            .size([SIDEBAR_WIDTH, body_height])
            .build(|| {
                self.draw_sidebar(ui, browser, frame);
            });
        sidebar_bg.pop();

        ui.same_line();
        ui.child_window("content")
            .size([content_w, body_height])
            .build(|| {
                let w = ui.content_region_avail()[0];
                if focus_search {
                    ui.set_keyboard_focus_here();
                }
                ui.set_next_item_width(w);
                // SAFETY: the closure submits exactly one stock single-line InputText.
                unsafe {
                    imgui_painter::decorate_input_text(frame, &inset_material, || {
                        ui.input_text("##search", &mut self.search_buf)
                            .hint("Search...")
                            .build()
                    })
                };
                let sf = ui.is_item_active();
                search_focused = sf;

                if self.search_buf != self.last_typed_query {
                    self.last_typed_query = self.search_buf.clone();
                    self.query_change_time = Instant::now();
                }
                if self.query_change_time.elapsed() >= SEARCH_DEBOUNCE
                    && self.last_typed_query != self.last_searched_query
                {
                    self.last_searched_query = self.last_typed_query.clone();
                    if self.last_searched_query.is_empty() {
                        browser.clear_search();
                    } else {
                        browser.search(&self.last_searched_query);
                    }
                }

                if browser.is_searching() && !browser.is_in_search_mode() {
                    ui.text_disabled("Searching...");
                }

                let is = browser.is_in_search_mode();
                in_search = is;

                ui.child_window("file_list").size([0.0, 0.0]).build(|| {
                    if is {
                        self.draw_search_results(
                            ui,
                            browser,
                            frame,
                            &mut drag_requested,
                            sf,
                            up_key,
                            down_key,
                            back_key,
                        );
                    } else {
                        self.draw_browse_list(
                            ui,
                            browser,
                            frame,
                            &mut drag_requested,
                            sf,
                            up_key,
                            down_key,
                            back_key,
                            conf_key,
                        );
                    }
                });
            });

        if self.prefs.inspector_visible {
            // Splitter: an invisible hit strip whose horizontal drag resizes the
            // Inspector. Dragging left widens it (the boundary moves left), so we
            // subtract the mouse delta. Width persists on release.
            ui.same_line();
            ui.invisible_button("##inspector_splitter", [splitter_w, body_height]);
            if ui.is_item_hovered() || ui.is_item_active() {
                ui.set_mouse_cursor(Some(imgui::MouseCursor::ResizeEW));
            }
            if ui.is_item_active() {
                self.prefs.inspector_width =
                    (self.prefs.inspector_width - ui.io().mouse_delta[0]).clamp(180.0, 600.0);
            }
            if ui.is_item_deactivated() {
                punks_core::config::save(&self.prefs);
            }

            ui.same_line();
            let inspector_bg = ui.push_style_color(imgui::StyleColor::ChildBg, SIDEBAR_BG);
            ui.child_window("inspector")
                .size([inspector_w, body_height])
                .build(|| {
                    self.draw_inspector(ui, browser, frame);
                });
            inspector_bg.pop();
        }

        if let Some(path) = drag_requested.as_deref() {
            if let Some(on_drag_file) = on_drag_file {
                on_drag_file(path);
            }
            return;
        }

        ui.separator();

        // Panel-level keys (same focus gating as nav): the configurable
        // play/stop bind toggles playback; the tab / undo / redo / inspector
        // binds do the rest.
        if ui.is_window_focused() && !search_focused {
            let count = browser.tab_count();
            let active = browser.active_tab();
            let pressed =
                |bind: &str| parse_key(bind).is_some_and(|k| ui.is_key_pressed_no_repeat(k));
            if pressed(&self.prefs.keybinds.play_stop) {
                match browser.playback_status() {
                    PlaybackStatus::Playing { .. } | PlaybackStatus::Loading { .. } => {
                        browser.stop();
                    }
                    PlaybackStatus::Idle => {
                        if in_search {
                            if let Some(idx) = browser.search_selected() {
                                if let Some(e) = browser.search_results().and_then(|r| r.get(idx)) {
                                    let path = e.path.clone();
                                    browser.play_file(&path);
                                }
                            }
                        } else {
                            browser.play_selected();
                        }
                    }
                }
            } else if pressed(&self.prefs.keybinds.next_tab) {
                browser.switch_tab((active + 1) % count);
            } else if pressed(&self.prefs.keybinds.prev_tab) {
                browser.switch_tab((active + count - 1) % count);
            } else if pressed(&self.prefs.keybinds.new_tab) {
                let start = browser.current_directory().map(|p| p.to_path_buf());
                browser.new_tab(start.as_deref());
            } else if pressed(&self.prefs.keybinds.close_tab) {
                browser.close_tab(browser.active_tab());
            } else if pressed(&self.prefs.keybinds.undo) {
                browser.undo();
            } else if pressed(&self.prefs.keybinds.redo) {
                browser.redo();
            } else if pressed(&self.prefs.keybinds.toggle_inspector) {
                self.prefs.inspector_visible = !self.prefs.inspector_visible;
                punks_core::config::save(&self.prefs);
            }
        }

        draw_waveform_widget(ui, browser, frame, &mut self.scrub_last_x);

        // Pinned transport, directly below the waveform: Play/Stop on the left,
        // volume on the right edge. Always visible (never scrolls) so playback
        // becomes muscle memory — the metadata/facts that used to live here have
        // moved into the Inspector, which is exactly what frees this row to stay
        // put no matter how much a file carries.
        // Z-order contract: capture the strip rect first, paint its inset-panel
        // background before any transport widget is submitted, then submit the
        // widgets on top.
        let [strip_x, strip_y] = ui.cursor_screen_pos();
        let strip_width = ui.content_region_avail()[0];
        const TRANSPORT_PADDING: f32 = 4.0;
        let strip_rect = imgui_painter::Rect {
            min: imgui_painter::Vec2 {
                x: strip_x,
                y: strip_y - TRANSPORT_PADDING,
            },
            max: imgui_painter::Vec2 {
                x: strip_x + strip_width,
                y: strip_y + ui.frame_height() + TRANSPORT_PADDING,
            },
        };
        // SAFETY: the window draw list is the active one this frame; the Canvas
        // submits and releases the raw pointer when it drops at the block's end.
        unsafe {
            let dl = imgui::sys::igGetWindowDrawList();
            let mut canvas = frame.canvas(dl);
            imgui_painter::recipes::inset_panel(&mut canvas, strip_rect, &palette);
        }

        let playing = !matches!(browser.playback_status(), PlaybackStatus::Idle);
        // SAFETY: the closure submits exactly one stock button in the active window.
        let play_clicked = unsafe {
            imgui_painter::decorate_button(frame, &raised_material, || {
                ui.button(if playing {
                    "\u{25a0} Stop"
                } else {
                    "\u{25b6} Play"
                })
            })
        };
        if play_clicked {
            if playing {
                browser.stop();
            } else if in_search {
                // Mirror the Space-key path above: play the highlighted search
                // result, or the browse selection.
                if let Some(idx) = browser.search_selected() {
                    if let Some(e) = browser.search_results().and_then(|r| r.get(idx)) {
                        let path = e.path.clone();
                        browser.play_file(&path);
                    }
                }
            } else {
                browser.play_selected();
            }
        }

        ui.same_line();
        let transport_x = ui.cursor_pos()[0];
        let transport_y = ui.cursor_pos()[1];
        let panel_width = ui.content_region_avail()[0];
        const VOLUME_SLIDER_WIDTH: f32 = 120.0;

        ui.set_cursor_pos([
            transport_x + (panel_width - VOLUME_SLIDER_WIDTH).max(0.0),
            transport_y,
        ]);
        ui.set_next_item_width(VOLUME_SLIDER_WIDTH);
        let mut vol = self.volume;
        // SAFETY: the closure submits exactly one stock f32 slider with the matching range/value.
        let changed = unsafe {
            imgui_painter::decorate_slider_f32(
                frame,
                &volume_slider_style,
                0.0,
                1.0,
                &mut vol,
                |v| {
                    ui.slider_config("##volume", 0.0_f32, 1.0_f32)
                        .display_format("")
                        .build(v)
                },
            )
        };
        let hovered = ui.is_item_hovered();
        let committed = ui.is_item_deactivated_after_edit();
        if changed {
            self.volume = vol;
            browser.set_volume(self.volume);
        }
        if hovered {
            ui.tooltip_text(format!("Volume: {}%", (self.volume * 100.0).round() as i32));
        }
        if committed {
            self.prefs.volume = self.volume;
            punks_core::config::save(&self.prefs);
        }

        if let Some(err) = browser.last_error() {
            // A read-only InputText (not plain Text) so the message can be
            // selected and copied — needed for anything longer than fits on
            // screen, like a full db-error string.
            self.error_buf.clear();
            self.error_buf.push_str(err);
            let w = ui.content_region_avail()[0];
            let color = ui.push_style_color(imgui::StyleColor::Text, [1.0, 0.3, 0.3, 1.0]);
            ui.set_next_item_width(w);
            ui.input_text("##last_error", &mut self.error_buf)
                .read_only(true)
                .build();
            color.pop();
        }

        // Tag popups live at panel scope so open_popup/popup share an ID
        // stack; rows and the sidebar only set the request flags.
        if self.open_tag_editor {
            self.open_tag_editor = false;
            ui.open_popup("##tag_editor");
        }
        if self.open_tag_delete {
            self.open_tag_delete = false;
            ui.open_popup("##tag_delete");
        }
        if self.open_override_editor {
            self.open_override_editor = false;
            // Seed each edit field from the current override value (blank if
            // none, or if the metric is marked absent — nothing to edit there).
            if let Some(path) = self.override_popup_path.clone() {
                for (metric, _, _) in OVERRIDE_FIELDS {
                    let seed = match browser.override_state(&path, metric) {
                        Some(Some(f)) => fact_display(&f),
                        Some(None) | None => String::new(),
                    };
                    self.override_bufs.insert(metric, seed);
                }
            }
            ui.open_popup("##override_editor");
        }
        if self.open_metadata_editor {
            self.open_metadata_editor = false;
            ui.open_popup("Edit Metadata##metadata_editor");
        }

        self.draw_override_popup(ui, browser, frame);

        // Give the editor modal a comfortable fixed width the first time it
        // appears (its fields size to the modal, not the other way round). No
        // safe `set_next_window_size` on `Ui` in this imgui version, so call the
        // C API directly; height 0 lets it auto-fit the fields.
        unsafe {
            imgui::sys::igSetNextWindowSize(
                imgui::sys::ImVec2 { x: 440.0, y: 0.0 },
                imgui::Condition::Appearing as i32,
            );
        }
        draw_metadata_editor_modal(ui, browser, &mut self.editor, frame);

        ui.popup("##tag_editor", || {
            if let Some(path) = self.tag_popup_path.clone() {
                // If the row this popup was opened for is part of a live
                // multi-selection, tag edits apply to the whole marked set —
                // the batch-tag consumer that exercises `selection_paths`.
                // Opening the popup for a row outside any selection (the
                // common case) edits just that one file, as before.
                let selection = browser.selection_paths();
                let batch = selection.len() > 1 && selection.contains(&path);
                let paths: &[PathBuf] = if batch {
                    &selection
                } else {
                    std::slice::from_ref(&path)
                };

                let fname = if batch {
                    format!("{} files selected", paths.len())
                } else {
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                };
                ui.text_disabled(&fname);
                ui.separator();

                // In batch mode a tag is "checked" only if every selected file
                // already has it — an unchecked box may still be mixed.
                let current = browser.tag_ids_for_path(&path);
                let tags: Vec<TagCount> = browser.library_tags().to_vec();
                if tags.is_empty() {
                    ui.text_disabled("No tags yet.");
                }
                for t in &tags {
                    let mut has = if batch {
                        paths
                            .iter()
                            .all(|p| browser.tag_ids_for_path(p).contains(&t.id))
                    } else {
                        current.contains(&t.id)
                    };
                    // SAFETY: the closure submits exactly one stock Checkbox.
                    if unsafe {
                        imgui_painter::decorate_checkbox(frame, &toolbar_material, || {
                            ui.checkbox(format!("{}##ptag{}", t.name, t.id), &mut has)
                        })
                    } {
                        if batch {
                            if has {
                                browser.assign_tag_batch(paths, t.id);
                            } else {
                                browser.unassign_tag_batch(paths, t.id);
                            }
                        } else if has {
                            browser.assign_tag(&path, t.id);
                        } else {
                            browser.unassign_tag(&path, t.id);
                        }
                    }
                }

                ui.separator();
                ui.set_next_item_width(150.0);
                // SAFETY: the closure submits exactly one stock single-line InputText.
                let entered = unsafe {
                    imgui_painter::decorate_input_text(frame, &inset_material, || {
                        ui.input_text("##popup_new_tag", &mut self.popup_tag_buf)
                            .hint("New tag...")
                            .enter_returns_true(true)
                            .build()
                    })
                };
                ui.same_line();
                // SAFETY: the closure submits exactly one stock button in the active window.
                let add = unsafe {
                    imgui_painter::decorate_button(frame, &toolbar_material, || {
                        ui.small_button("Add##popup_add_tag")
                    })
                };
                if (entered || add) && !self.popup_tag_buf.trim().is_empty() {
                    let name = self.popup_tag_buf.trim().to_string();
                    if batch {
                        browser.create_and_assign_tag_batch(paths, &name);
                    } else {
                        browser.create_and_assign_tag(&path, &name);
                    }
                    self.popup_tag_buf.clear();
                }
            }
        });

        ui.popup("##tag_delete", || {
            if let Some((id, name)) = self.tag_delete_target.clone() {
                if ui
                    .selectable_config(format!("Delete tag \"{name}\""))
                    .build()
                {
                    browser.delete_tag(id);
                    self.tag_delete_target = None;
                }
            }
        });
    }

    fn draw_sidebar(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
    ) {
        let toolbar_material = theme::toolbar_material();
        let raised_material = theme::raised_material();
        let row_material = theme::row_material();
        let row_selected_material = theme::row_selected_material();
        ui.spacing();
        ui.text_disabled("LIBRARY");
        ui.separator();

        match browser.library_state() {
            LibraryState::NotALibrary => {
                if browser.current_directory().is_some() {
                    ui.text_wrapped("Not a library. Create one to tag and filter samples.");
                    ui.spacing();
                    // SAFETY: the closure submits exactly one stock button in the active window.
                    if unsafe {
                        imgui_painter::decorate_button(frame, &raised_material, || {
                            ui.button("Create library")
                        })
                    } {
                        browser.init_library();
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text(
                            "Adds a .punks folder to this root.\nNothing is written until you click.",
                        );
                    }
                } else {
                    ui.text_disabled("Open a folder first.");
                }
            }
            LibraryState::Scanning => {
                ui.text_disabled("Scanning library…");
                match browser.scan_progress() {
                    Some((_, 0)) | None => {
                        // Still walking the tree (count unknown): indeterminate
                        // sweep driven by the frame clock.
                        let f = (ui.time() as f32).fract();
                        imgui::ProgressBar::new(f)
                            .size([-1.0, 14.0])
                            .overlay_text("Reading files…")
                            .build(ui);
                    }
                    Some((done, total)) => {
                        let frac = done as f32 / total as f32;
                        imgui::ProgressBar::new(frac)
                            .size([-1.0, 14.0])
                            .overlay_text(format!("{done} / {total}"))
                            .build(ui);
                    }
                }
                self.draw_delete_library(ui, browser, frame);
            }
            LibraryState::Ready => {
                ui.spacing();
                if let Some(desc) = browser.undo_description() {
                    // SAFETY: the closure submits exactly one stock button in the active window.
                    if unsafe {
                        imgui_painter::decorate_button(frame, &toolbar_material, || {
                            ui.button(format!("Undo: {desc}##undo_btn"))
                        })
                    } {
                        browser.undo();
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text(format!("({})", self.prefs.keybinds.undo));
                    }
                } else {
                    ui.text_disabled("Nothing to undo");
                }
                if let Some(desc) = browser.redo_description() {
                    // SAFETY: the closure submits exactly one stock button in the active window.
                    if unsafe {
                        imgui_painter::decorate_button(frame, &toolbar_material, || {
                            ui.button(format!("Redo: {desc}##redo_btn"))
                        })
                    } {
                        browser.redo();
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text(format!("({})", self.prefs.keybinds.redo));
                    }
                }

                ui.spacing();
                ui.text_disabled("TAGS");
                ui.set_next_item_width(-1.0);
                let entered = ui
                    .input_text("##new_tag", &mut self.new_tag_buf)
                    .hint("New tag...")
                    .enter_returns_true(true)
                    .build();
                if entered && !self.new_tag_buf.trim().is_empty() {
                    let name = self.new_tag_buf.trim().to_string();
                    browser.create_tag(&name);
                    self.new_tag_buf.clear();
                }
                ui.spacing();

                let filter: Vec<i64> = browser.tag_filter().to_vec();
                let tags: Vec<TagCount> = browser.library_tags().to_vec();
                if tags.is_empty() {
                    ui.text_disabled("No tags yet.");
                }
                for t in &tags {
                    let selected = filter.contains(&t.id);
                    let label = format!("{}  ({})##sbtag{}", t.name, t.count, t.id);
                    let material = if selected {
                        &row_selected_material
                    } else {
                        &row_material
                    };
                    // SAFETY: the closure submits exactly one stock Selectable in the active window.
                    if unsafe {
                        imgui_painter::decorate_selectable(frame, material, || {
                            ui.selectable_config(&label).selected(selected).build()
                        })
                    } {
                        browser.toggle_tag_filter(t.id);
                    }
                    if ui.is_item_clicked_with_button(imgui::MouseButton::Right) {
                        self.tag_delete_target = Some((t.id, t.name.clone()));
                        self.open_tag_delete = true;
                    }
                }
                if !filter.is_empty() {
                    ui.spacing();
                    // SAFETY: the closure submits exactly one stock button in the active window.
                    let clear_clicked = unsafe {
                        imgui_painter::decorate_button(frame, &toolbar_material, || {
                            ui.small_button("Clear filter")
                        })
                    };
                    if clear_clicked {
                        browser.clear_tag_filter();
                    }
                }
                self.draw_fact_filters(ui, browser, frame);
                self.draw_health_panel(ui, browser, frame);
                self.draw_delete_library(ui, browser, frame);
            }
        }
    }

    /// Sidebar metadata health-check panel: an on-demand scan comparing each
    /// asset's embedded metadata against the library's cached copy and any
    /// user override, surfacing missing/drifted fields. Read-only — this
    /// never writes anything; it's a diagnostic, not a fix.
    fn draw_health_panel(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
    ) {
        let toolbar_material = theme::toolbar_material();
        ui.spacing();
        ui.separator();
        ui.text_disabled("METADATA HEALTH");
        ui.spacing();

        let running = browser.health_check_running();
        if running {
            ui.text_disabled("Checking…");
        } else {
            // SAFETY: the closure submits exactly one stock button in the active window.
            let check_clicked = unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || {
                    ui.button("Check library health")
                })
            };
            if check_clicked {
                browser.check_library_health();
            }
        }

        match browser.health_issues() {
            None => {
                if !running {
                    ui.text_disabled("Not checked yet.");
                }
            }
            Some([]) => ui.text_disabled("No issues found."),
            Some(issues) => {
                ui.spacing();
                ui.text(format!("{} issue(s) found:", issues.len()));
                ui.child_window("health_issues")
                    .size([-1.0, 150.0])
                    .border(true)
                    .build(|| {
                        for issue in issues {
                            let name = issue
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            ui.text_wrapped(format!("{name} — {}", health_issue_summary(issue)));
                            if ui.is_item_hovered() {
                                ui.tooltip_text(issue.path.to_string_lossy());
                            }
                        }
                    });
            }
        }
    }

    /// Sidebar facet filters over the correctable facts (instrument/key/bpm
    /// range) — the same "click to constrain the file list" pattern as the
    /// tag filter above it, ANDed with it. Values come from `FactFacets`,
    /// recomputed from the in-memory analysis caches (no SQL), so this is
    /// zero-cost per frame beyond reading an already-built `Vec`.
    fn draw_fact_filters(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
    ) {
        let toolbar_material = theme::toolbar_material();
        let inset_material = theme::inset_field_material();
        let Some(facets) = browser.library_fact_facets().cloned() else {
            return;
        };
        if facets.instruments.is_empty() && facets.keys.is_empty() && facets.bpm_range.is_none() {
            return;
        }
        let filter = browser.fact_filter().clone();

        ui.spacing();
        ui.separator();
        ui.text_disabled("FACTS");

        if !facets.instruments.is_empty() {
            ui.spacing();
            ui.text_disabled("Instrument");
            for (name, count) in &facets.instruments {
                let selected = filter.instrument.as_deref() == Some(name.as_str());
                let label = format!("{}  ({})##facinst{}", capitalize(name), count, name);
                if ui.selectable_config(&label).selected(selected).build() {
                    browser.set_instrument_filter(if selected { None } else { Some(name.clone()) });
                }
            }
        }

        if !facets.keys.is_empty() {
            ui.spacing();
            ui.text_disabled("Key");
            for (name, count) in &facets.keys {
                let selected = filter.key.as_deref() == Some(name.as_str());
                let label = format!("{name}  ({count})##fackey{name}");
                if ui.selectable_config(&label).selected(selected).build() {
                    browser.set_key_filter(if selected { None } else { Some(name.clone()) });
                }
            }
        }

        if let Some((lo, hi)) = facets.bpm_range {
            ui.spacing();
            ui.text_disabled(format!("BPM ({lo:.0}-{hi:.0})"));
            let mut active = filter.bpm_min.is_some() || filter.bpm_max.is_some();
            // SAFETY: the closure submits exactly one stock Checkbox.
            if unsafe {
                imgui_painter::decorate_checkbox(frame, &toolbar_material, || {
                    ui.checkbox("Filter by BPM##bpmfilter", &mut active)
                })
            } {
                if active {
                    self.bpm_min = filter.bpm_min.unwrap_or(lo);
                    self.bpm_max = filter.bpm_max.unwrap_or(hi);
                    browser.set_bpm_filter(Some(self.bpm_min), Some(self.bpm_max));
                } else {
                    browser.set_bpm_filter(None, None);
                }
            }
            if active {
                ui.set_next_item_width(70.0);
                // SAFETY: the closure submits exactly one stock single-line InputText.
                let mut changed = unsafe {
                    imgui_painter::decorate_input_text(frame, &inset_material, || {
                        ui.input_float("Min##bpmmin", &mut self.bpm_min).build()
                    })
                };
                ui.same_line();
                ui.set_next_item_width(70.0);
                // SAFETY: the closure submits exactly one stock single-line InputText.
                changed |= unsafe {
                    imgui_painter::decorate_input_text(frame, &inset_material, || {
                        ui.input_float("Max##bpmmax", &mut self.bpm_max).build()
                    })
                };
                if changed {
                    browser.set_bpm_filter(Some(self.bpm_min), Some(self.bpm_max));
                }
            }
        }

        if !filter.is_empty() {
            ui.spacing();
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || {
                    ui.small_button("Clear fact filter")
                })
            } {
                browser.clear_fact_filter();
            }
        }
    }

    /// The right-hand Inspector: contextual info about the selected file,
    /// ordered by what users reach for most — Selection, then Actions, then
    /// Facts / Tags, with Metadata last (a ~5%-of-time concern that shouldn't
    /// crowd the reading flow). Sections default open/closed each launch; that
    /// state is deliberately *not* persisted (see `PunksConfig`). Future
    /// sections (Embedded metadata, Health, Preview FX) slot in here once they
    /// have real content to show.
    fn draw_inspector(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
    ) {
        let toolbar_material = theme::toolbar_material();
        ui.spacing();
        let Some(path) = browser.current_file().map(Path::to_path_buf) else {
            ui.text_disabled("No file selected");
            return;
        };

        // Selection.
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ui.text_wrapped(&name);
        ui.separator();

        // Actions: the verbs a user reaches for on a selected sound. Override /
        // Edit Metadata open their modals via request flags (consumed at panel
        // scope with the other popups); the rest act immediately.
        let playing = !matches!(browser.playback_status(), PlaybackStatus::Idle);
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || {
                ui.button(if playing { "Stop" } else { "Play" })
            })
        } {
            if playing {
                browser.stop();
            } else {
                browser.play_file(&path);
            }
        }
        ui.same_line();
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Reveal"))
        } {
            reveal_in_file_manager(&path);
        }
        ui.same_line();
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Copy Path"))
        } {
            ui.set_clipboard_text(path.to_string_lossy());
        }
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Override"))
        } {
            self.override_popup_path = Some(path.clone());
            self.open_override_editor = true;
        }
        ui.same_line();
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Edit Metadata"))
        } {
            self.open_metadata_editor = true;
        }
        ui.separator();

        // Facts (open by default): the read-only analysis readout — what the
        // sound *is* (resolved user⇒analysis) plus its levels, and the file's
        // start timecode when present.
        if ui.collapsing_header("Facts", imgui::TreeNodeFlags::DEFAULT_OPEN) {
            if let Some(a) = browser.current_analysis() {
                let mut facts: Vec<String> = Vec::new();
                if let Some(Fact::Text(s)) = browser.current_resolved("instrument") {
                    facts.push(capitalize(&s));
                }
                if let Some(Fact::Real(b)) = browser.current_resolved("bpm") {
                    facts.push(format!("{} BPM", b as u32));
                }
                if let Some(Fact::Text(s)) = browser.current_resolved("key") {
                    facts.push(s);
                }
                if facts.is_empty() {
                    ui.text_disabled("no detected facts");
                } else {
                    ui.text_disabled(facts.join("   \u{b7}   "));
                }
                // Levels: peak in dB (audio people don't think in linear
                // amplitude); peak stays stored linear.
                ui.text_disabled(format!(
                    "{}   \u{b7}   Peak {:.1} dB",
                    format_duration(a.duration),
                    punks_browser::amp_to_dbfs(a.peak),
                ));
                ui.text_disabled(format!(
                    "RMS {:.1} dBFS   \u{b7}   ZCR {:.3}",
                    a.rms_dbfs, a.zcr
                ));
                if let Some(info) = browser.current_track_info() {
                    if let Some(tc) = info.metadata.time_reference.filter(|&t| t > 0) {
                        ui.text_disabled(format!(
                            "TC {}",
                            format_timecode(tc, info.source_sample_rate)
                        ));
                    }
                }
            } else if browser.current_analysis_pending() {
                // Animated so a slow/queued analysis reads as "working", not
                // frozen. ASCII dots (the pixel font lacks a `…` glyph).
                let dots = (ui.time() * 2.0) as usize % 4;
                ui.text_disabled(format!("analyzing{}", ".".repeat(dots)));
            } else {
                ui.text_disabled("no analysis yet");
            }
        }

        // Tags (open by default): the selected file's tags, edited through the
        // existing per-file tag popup.
        if ui.collapsing_header("Tags", imgui::TreeNodeFlags::DEFAULT_OPEN) {
            let ids = browser.tag_ids_for_path(&path);
            if ids.is_empty() {
                ui.text_disabled("No tags");
            } else {
                let tags = browser.library_tags();
                let names: Vec<&str> = ids
                    .iter()
                    .filter_map(|id| tags.iter().find(|t| t.id == *id).map(|t| t.name.as_str()))
                    .collect();
                ui.text_wrapped(names.join(", "));
            }
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Edit Tags"))
            } {
                self.tag_popup_path = Some(path.clone());
                self.open_tag_editor = true;
            }
        }

        // Metadata (closed by default): a low-weight read-only summary; the
        // editor opens as a modal, so it never sits as always-live edit fields.
        if ui.collapsing_header("Metadata", imgui::TreeNodeFlags::empty()) {
            metadata_summary(ui, &self.editor);
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || {
                    ui.button("Edit Metadata...")
                })
            } {
                self.open_metadata_editor = true;
            }
        }
    }

    /// Testing convenience pinned to the bottom of the library sidebar: wipe
    /// the current `.punks` store, behind a confirmation popup.
    fn draw_delete_library(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
    ) {
        let toolbar_material = theme::toolbar_material();
        ui.spacing();
        ui.separator();
        let danger = ui.push_style_color(imgui::StyleColor::Text, [0.90, 0.45, 0.45, 1.0]);
        // SAFETY: the closure submits exactly one stock button in the active window.
        let clicked = unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || {
                ui.small_button("Delete library")
            })
        };
        danger.pop();
        if ui.is_item_hovered() {
            ui.tooltip_text("Deletes the .punks folder for this root (testing).");
        }
        if clicked {
            self.open_delete_library = true;
        }
        if self.open_delete_library {
            self.open_delete_library = false;
            ui.open_popup("##delete_library");
        }
        ui.popup("##delete_library", || {
            ui.text("Delete this library's .punks store?");
            ui.text_disabled("Tags and cached waveforms are lost. Irreversible.");
            ui.spacing();
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Delete"))
            } {
                browser.delete_active_library();
                ui.close_current_popup();
            }
            ui.same_line();
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Cancel"))
            } {
                ui.close_current_popup();
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_search_results(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
        drag_requested: &mut Option<PathBuf>,
        search_focused: bool,
        up_key: Key,
        down_key: Key,
        back_key: Key,
    ) {
        // `A` exits search — tree-style nav that must work regardless of whether
        // there are results yet (before the count-based early returns below,
        // which would otherwise leave you unable to key out of a no-match /
        // still-searching state).
        if ui.is_window_focused() && !search_focused && ui.is_key_pressed_no_repeat(back_key) {
            self.search_buf.clear();
            self.last_typed_query.clear();
            self.last_searched_query.clear();
            browser.clear_search();
            return;
        }

        let count = match browser.search_results() {
            Some(r) if !r.is_empty() => r.len(),
            Some(_) => {
                ui.text_disabled("No results.");
                return;
            }
            None => {
                ui.text_disabled("Searching...");
                return;
            }
        };

        // Clone root so we don't hold a borrow on browser during keyboard
        // handling or the clipper loop.
        let root: Option<PathBuf> = browser.current_directory().map(|p| p.to_path_buf());

        // List navigation over results (W/S). `A` was already handled above.
        // `is_key_pressed` (not `_no_repeat`) applies imgui's normal key-repeat,
        // so holding W/S steps rapidly through results instead of moving once.
        if ui.is_window_focused() && !search_focused {
            if ui.is_key_pressed(up_key) {
                let idx = browser
                    .search_selected()
                    .map_or(count - 1, |i| i.saturating_sub(1));
                browser.select_search_result(idx);
                if let Some(e) = browser.search_results().and_then(|r| r.get(idx)) {
                    let path = e.path.clone();
                    browser.play_file(&path);
                }
                self.scroll_to_selected = true;
            }
            if ui.is_key_pressed(down_key) {
                let idx = browser
                    .search_selected()
                    .map_or(0, |i| (i + 1).min(count - 1));
                browser.select_search_result(idx);
                if let Some(e) = browser.search_results().and_then(|r| r.get(idx)) {
                    let path = e.path.clone();
                    browser.play_file(&path);
                }
                self.scroll_to_selected = true;
            }
        }

        let selected = browser.search_selected();
        let mut click_action: Option<(usize, PathBuf)> = None;
        let row_material = theme::row_material();
        let row_selected_material = theme::row_selected_material();

        // Width-adaptive columns; the clipper iterates rows of `cols` items so
        // only visible rows allocate label strings.
        let avail_w = ui.content_region_avail()[0];
        let cols = column_count(avail_w);
        let col_w = avail_w / cols as f32;
        let num_rows = count.div_ceil(cols);
        let row_stride = ui.text_line_height_with_spacing();

        if std::mem::take(&mut self.scroll_to_selected) {
            if let Some(sel) = selected {
                scroll_row_into_view(ui, sel / cols, row_stride);
            }
        }

        let clip = imgui::ListClipper::new(num_rows as i32)
            .items_height(row_stride)
            .begin(ui);
        'rows: for row in clip.iter() {
            for c in 0..cols {
                let i = row as usize * cols + c;
                if i >= count {
                    break;
                }
                if c > 0 {
                    ui.same_line_with_pos(c as f32 * col_w);
                }
                // Extract owned data in a short block so the borrow on browser
                // ends before we call any mutable method.
                let (label, path) = {
                    let results = browser.search_results().unwrap();
                    let e = &results[i];
                    let parent_hint = relative_parent(root.as_deref(), &e.path);
                    let label = format!("{}  ({})##sresult{}", e.name, parent_hint, i);
                    (label, e.path.clone())
                };

                let sel_w = col_w - COLUMN_GUTTER;
                let is_selected = selected == Some(i);
                let material = if is_selected {
                    &row_selected_material
                } else {
                    &row_material
                };
                // SAFETY: the closure submits exactly one stock Selectable in the active window.
                let clicked = unsafe {
                    imgui_painter::decorate_selectable(frame, material, || {
                        ui.selectable_config(&label)
                            .selected(is_selected)
                            .size([sel_w.max(40.0), 0.0])
                            .build()
                    })
                };
                #[cfg(debug_assertions)]
                self.row_decorations.set(self.row_decorations.get() + 1);
                // Capture the row's hover/click/rect state while the selectable
                // is still ImGui's last item.
                let row_hovered = ui.is_item_hovered();
                let row_right_clicked = ui.is_item_clicked_with_button(imgui::MouseButton::Right);
                let row_rect = (ui.item_rect_min(), ui.item_rect_max());

                if row_right_clicked {
                    self.tag_popup_path = Some(path.clone());
                    self.open_tag_editor = true;
                }
                let names = browser.tag_names_for_path(&path);
                if !names.is_empty() {
                    draw_tag_pills(ui, &names, frame, row_rect.0, row_rect.1);
                }
                if row_hovered
                    && ui.is_mouse_dragging_with_threshold(imgui::MouseButton::Left, -1.0)
                {
                    *drag_requested = Some(path);
                    break 'rows;
                }
                if clicked {
                    click_action = Some((i, path));
                }
            }
        }

        if let Some((i, path)) = click_action {
            browser.select_search_result(i);
            browser.play_file(&path);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_browse_list(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
        drag_requested: &mut Option<PathBuf>,
        search_focused: bool,
        up_key: Key,
        down_key: Key,
        back_key: Key,
        conf_key: Key,
    ) {
        // Keyboard nav runs first — and BEFORE any empty-directory early return
        // — decoupled by what each key acts on. `A` (back) is tree navigation:
        // it moves up the directory stack and needs no list selection, so it
        // must work even in an empty folder. `W`/`S`/`D` are list navigation:
        // they self-initialize the selection and no-op when there's nothing to
        // act on.
        if ui.is_window_focused() && !search_focused {
            let entry_count = browser.entries().len();
            let selected = browser.selected();

            if ui.is_key_pressed_no_repeat(back_key) {
                if let Err(e) = browser.navigate_up() {
                    log::error!("navigate_up failed: {e}");
                }
            } else if entry_count > 0 {
                // `else if`: `A` may have just changed the directory; don't also
                // move a selection within the now-different listing this frame.
                if ui.is_key_pressed(up_key) {
                    // First press with no selection lands on the last item.
                    // `is_key_pressed` (not `_no_repeat`) applies imgui's normal
                    // key-repeat (io.key_repeat_delay/_rate): a tap moves one row,
                    // holding rapidly steps through the list like an OS text
                    // cursor instead of moving once.
                    let idx = selected.map_or(entry_count - 1, |i| i.saturating_sub(1));
                    browser.select(idx);
                    browser.play_selected();
                    self.scroll_to_selected = true;
                }
                if ui.is_key_pressed(down_key) {
                    // First press with no selection lands on the first item.
                    let idx = selected.map_or(0, |i| (i + 1).min(entry_count - 1));
                    browser.select(idx);
                    browser.play_selected();
                    self.scroll_to_selected = true;
                }
                let confirm = ui.is_key_pressed_no_repeat(conf_key)
                    || ui.is_key_pressed_no_repeat(Key::Enter)
                    || ui.is_key_pressed_no_repeat(Key::KeypadEnter);
                if confirm {
                    if let Some(i) = selected {
                        let is_dir = browser.entries().get(i).map(|e| e.is_directory);
                        if is_dir == Some(true) {
                            if let Err(e) = browser.navigate_into(i) {
                                log::error!("navigate_into failed: {e}");
                            }
                        } else if is_dir == Some(false) {
                            browser.play_selected();
                        }
                    }
                }
            }
        }

        // Re-read selected AND entry_count: navigate_into/navigate_up above may
        // have swapped in a different (shorter or longer) directory listing
        // mid-frame. Indexing browser.entries() below with the stale count
        // caused an out-of-bounds panic when the new directory had fewer
        // entries than the one just left.
        let selected = browser.selected();
        let entry_count = browser.entries().len();
        let mut click_action: Option<(usize, bool, PathBuf)> = None;
        let row_material = theme::row_material();
        let row_selected_material = theme::row_selected_material();

        if entry_count == 0 {
            if browser.current_directory().is_some() {
                ui.text_disabled("Empty directory.");
            } else {
                ui.text_disabled("No folder open. Click Browse to get started.");
            }
            return;
        }

        // Lay out entries in width-adaptive columns to use horizontal space.
        // The clipper iterates rows of `cols` items, so off-screen rows are
        // skipped and label strings are allocated only for visible items.
        let avail_w = ui.content_region_avail()[0];
        let cols = column_count(avail_w);
        let col_w = avail_w / cols as f32;
        let num_rows = entry_count.div_ceil(cols);
        let row_stride = ui.text_line_height_with_spacing();

        // W/S nav can select a row outside the visible viewport (the whole
        // point of the columns+clipper is to not draw off-screen rows), so it
        // has to be scrolled into view explicitly rather than relying on the
        // clipper to happen to show it.
        if std::mem::take(&mut self.scroll_to_selected) {
            if let Some(sel) = selected {
                scroll_row_into_view(ui, sel / cols, row_stride);
            }
        }

        let clip = imgui::ListClipper::new(num_rows as i32)
            .items_height(row_stride)
            .begin(ui);
        'rows: for row in clip.iter() {
            for c in 0..cols {
                let i = row as usize * cols + c;
                if i >= entry_count {
                    break;
                }
                if c > 0 {
                    ui.same_line_with_pos(c as f32 * col_w);
                }

                // Extract owned data in a short block so the immutable borrow on
                // browser ends before we call any mutable method.
                let (label, is_dir, path) = {
                    let e = &browser.entries()[i];
                    let label = if e.is_directory {
                        format!("> {}##entry{}", e.name, i)
                    } else {
                        format!("{}##entry{}", e.name, i)
                    };
                    (label, e.is_directory, e.path.clone())
                };

                let is_selected = selected == Some(i) || browser.selection().contains(&i);
                let material = if is_selected {
                    &row_selected_material
                } else {
                    &row_material
                };
                let sel_w = col_w - COLUMN_GUTTER;
                let size = [sel_w.max(40.0), 0.0];
                let clicked = if is_dir {
                    let color = ui.push_style_color(imgui::StyleColor::Text, DIR_TEXT_COLOR);
                    // SAFETY: the closure submits exactly one stock Selectable in the active window.
                    let clicked = unsafe {
                        imgui_painter::decorate_selectable(frame, material, || {
                            ui.selectable_config(&label)
                                .selected(is_selected)
                                .size(size)
                                .build()
                        })
                    };
                    color.pop();
                    clicked
                } else {
                    // SAFETY: the closure submits exactly one stock Selectable in the active window.
                    unsafe {
                        imgui_painter::decorate_selectable(frame, material, || {
                            ui.selectable_config(&label)
                                .selected(is_selected)
                                .size(size)
                                .build()
                        })
                    }
                };
                #[cfg(debug_assertions)]
                self.row_decorations.set(self.row_decorations.get() + 1);
                // Capture the row's hover/click/rect state while the selectable
                // is still ImGui's last item.
                let row_hovered = ui.is_item_hovered();
                let row_right_clicked = ui.is_item_clicked_with_button(imgui::MouseButton::Right);
                let row_rect = (ui.item_rect_min(), ui.item_rect_max());

                if !is_dir {
                    // Right-click opens the tag editor where the platform
                    // delivers it (works on Windows; unreliable on some macOS
                    // trackpad configurations); the Inspector is the primary
                    // path on every platform.
                    if row_right_clicked {
                        self.tag_popup_path = Some(path.clone());
                        self.open_tag_editor = true;
                    }
                    let names = browser.tag_names_for_path(&path);
                    if !names.is_empty() {
                        draw_tag_pills(ui, &names, frame, row_rect.0, row_rect.1);
                    }
                }
                if !is_dir
                    && row_hovered
                    && ui.is_mouse_dragging_with_threshold(imgui::MouseButton::Left, -1.0)
                {
                    *drag_requested = Some(path);
                    break 'rows;
                }
                if clicked {
                    click_action = Some((i, is_dir, path));
                }
            }
        }

        // Apply click after the loop — avoids holding an immutable borrow
        // on browser.entries() while calling mutable browser methods.
        // Shift/Ctrl(Cmd)-click adjust the marked set for batch operations
        // (see `selection`/`selection_paths`) without opening/playing anything
        // — multi-selecting isn't "open this file". A directory always ignores
        // modifiers: navigating into it is the only sensible click behavior.
        if let Some((i, is_dir, _)) = click_action {
            if is_dir {
                browser.select(i);
                if let Err(e) = browser.navigate_into(i) {
                    log::error!("navigate_into failed: {e}");
                }
            } else if ui.io().key_shift {
                browser.range_select(i);
            } else if ui.io().key_ctrl || ui.io().key_super {
                browser.toggle_select(i);
            } else {
                browser.select(i);
                browser.play_selected();
            }
        }
    }

    /// The override editor for the target file: each correctable metric shows its
    /// detected value (read-only) beside an editable override with Set / Clear.
    /// Setting writes a user override that patches — never replaces — the detected
    /// fact; Clear reverts to detected.
    fn draw_override_popup(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
    ) {
        let toolbar_material = theme::toolbar_material();
        let raised_material = theme::raised_material();
        ui.popup("##override_editor", || {
            let Some(path) = self.override_popup_path.clone() else {
                return;
            };
            let fname = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.text_disabled(&fname);
            ui.separator();

            for &(metric, label, numeric) in OVERRIDE_FIELDS {
                let detected = browser
                    .detected_fact(&path, metric)
                    .map(|f| fact_display(&f))
                    .unwrap_or_else(|| "\u{2014}".into()); // em dash = none
                let marked_absent = matches!(browser.override_state(&path, metric), Some(None));

                ui.text(label);
                ui.same_line();
                ui.text_disabled(format!("(detected: {detected})"));
                if marked_absent {
                    ui.same_line();
                    ui.text_colored([0.9, 0.7, 0.3, 1.0], "marked N/A");
                }

                // Confine the &mut into override_bufs to this block so the Set /
                // N/A / Clear branches below can borrow browser + override_bufs
                // freely.
                let entered = {
                    let buf = self.override_bufs.entry(metric).or_default();
                    ui.set_next_item_width(140.0);
                    ui.input_text(format!("##ov_{metric}"), buf).build();
                    buf.trim().to_string()
                };
                ui.same_line();
                // SAFETY: the closure submits exactly one stock button in the active window.
                let set = unsafe {
                    imgui_painter::decorate_button(frame, &raised_material, || {
                        ui.small_button(format!("Set##set_{metric}"))
                    })
                };
                if set && !entered.is_empty() {
                    if numeric {
                        if let Ok(n) = entered.parse::<f64>() {
                            browser.set_override(&path, metric, Fact::Real(n));
                        }
                    } else {
                        browser.set_override(&path, metric, Fact::Text(entered));
                    }
                }
                ui.same_line();
                // SAFETY: the closure submits exactly one stock button in the active window.
                if unsafe {
                    imgui_painter::decorate_button(frame, &toolbar_material, || {
                        ui.small_button(format!("N/A##na_{metric}"))
                    })
                } {
                    // Explicitly "this metric doesn't apply" — different from
                    // Clear, which instead reveals the detected guess again.
                    browser.mark_absent(&path, metric);
                    self.override_bufs.insert(metric, String::new());
                }
                ui.same_line();
                // SAFETY: the closure submits exactly one stock button in the active window.
                if unsafe {
                    imgui_painter::decorate_button(frame, &toolbar_material, || {
                        ui.small_button(format!("Clear##clr_{metric}"))
                    })
                } {
                    browser.clear_override(&path, metric);
                    self.override_bufs.insert(metric, String::new());
                }
            }
        });
    }

    fn draw_settings_modal(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        frame: &mut imgui_painter::Frame<'_>,
    ) {
        let toolbar_material = theme::toolbar_material();
        let modal = ui
            .modal_popup_config("Settings##modal")
            .save_settings(false)
            .always_auto_resize(true);

        if let Some(_token) = modal.begin_popup() {
            ui.text("Samples folder");
            let dir_label = self
                .prefs
                .last_directory
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "(none)".into());
            ui.text_disabled(&dir_label);
            ui.same_line();
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || {
                    ui.button("Browse##settings")
                })
            } {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    // last_directory is persisted centrally at the top of draw().
                    if let Err(e) = browser.open_directory(&path) {
                        log::error!("failed to open directory: {e}");
                    }
                }
            }

            ui.separator();
            ui.text("Keybinds");
            ui.spacing();

            for &(action, label) in KEYBIND_ACTIONS {
                let is_rebinding = self.rebinding == Some(action);
                let current = keybind_field(&self.prefs.keybinds, action);
                let btn_label = if is_rebinding {
                    format!("Press any key...##{label}")
                } else {
                    let display = parse_key(current).map(key_name).unwrap_or(current);
                    format!("[ {display} ]##{label}")
                };

                ui.text(label);
                ui.same_line_with_pos(180.0);
                // SAFETY: the closure submits exactly one stock button in the active window.
                let clicked = unsafe {
                    imgui_painter::decorate_button(frame, &toolbar_material, || {
                        ui.button(&btn_label)
                    })
                };
                if clicked && !is_rebinding {
                    self.rebinding = Some(action);
                    self.rebind_conflict = None;
                }
            }

            if let Some(action) = self.rebinding {
                for &(key, name) in CAPTURABLE_KEYS {
                    if ui.is_key_pressed_no_repeat(key) {
                        // Refuse a key already bound to a different action —
                        // two actions on one key silently shadow each other.
                        let conflict = KEYBIND_ACTIONS.iter().find(|(a, _)| {
                            *a != action
                                && keybind_field(&self.prefs.keybinds, *a)
                                    .eq_ignore_ascii_case(name)
                        });
                        if let Some((_, other)) = conflict {
                            self.rebind_conflict =
                                Some(format!("{name} is already bound to \"{other}\""));
                        } else {
                            *keybind_field_mut(&mut self.prefs.keybinds, action) = name.to_string();
                            self.rebind_conflict = None;
                            punks_core::config::save(&self.prefs);
                        }
                        self.rebinding = None;
                        break;
                    }
                }
            }

            if let Some(msg) = &self.rebind_conflict {
                ui.text_colored([1.0, 0.4, 0.4, 1.0], msg);
            }

            ui.separator();
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || {
                    ui.button("Reset to defaults")
                })
            } {
                self.prefs.keybinds = Keybinds::default();
                self.rebind_conflict = None;
                punks_core::config::save(&self.prefs);
            }
            ui.same_line();
            // SAFETY: the closure submits exactly one stock button in the active window.
            if unsafe {
                imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Close"))
            } {
                self.rebinding = None;
                ui.close_current_popup();
            }
        }
    }
}

impl Default for BrowserPanel {
    fn default() -> Self {
        Self::new()
    }
}

// surface_inset.
const WAVEFORM_BG: [f32; 4] = [0.68235, 0.74902, 0.82353, 1.0];
// accent.
const WAVEFORM_BAR: [f32; 4] = [0.94118, 0.56863, 0.22745, 1.0];
// text at 0.9 alpha.
const WAVEFORM_PLAYHEAD: [f32; 4] = [0.06275, 0.09020, 0.13333, 0.9];
// text at 0.85 alpha.
const WAVEFORM_TEXT: [f32; 4] = [0.06275, 0.09020, 0.13333, 0.85];
// text at 0.35 alpha for the hover/scrub crosshair.
const WAVEFORM_HOVER: [f32; 4] = [0.06275, 0.09020, 0.13333, 0.35];

fn color_u32(c: [f32; 4]) -> u32 {
    let r = (c[0] * 255.0) as u32;
    let g = (c[1] * 255.0) as u32;
    let b = (c[2] * 255.0) as u32;
    let a = (c[3] * 255.0) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

/// Right-aligned tag pills inside a file row's rect, drawn over the
/// selectable. Stops before eating the space reserved for the file name.
fn draw_tag_pills(
    ui: &imgui::Ui,
    names: &[String],
    frame: &mut imgui_painter::Frame<'_>,
    rect_min: [f32; 2],
    rect_max: [f32; 2],
) {
    let bg = color_u32(PILL_BG);
    let fg = color_u32(PILL_TEXT);
    let y0 = rect_min[1] + 1.0;
    let y1 = rect_max[1] - 1.0;
    // Keep the left part of the row readable for the name.
    let min_x = rect_min[0] + 160.0;
    let mut x = rect_max[0] - 4.0;

    // Lay pills out right-to-left, collecting each one's position so the
    // backgrounds go through one Canvas (submitted first) and the labels
    // draw on top afterward (imgui owns fonts).
    let mut pills: Vec<(f32, f32, &str)> = Vec::new(); // (x, width, name)
    for name in names.iter().rev() {
        let w = ui.calc_text_size(name)[0] + 10.0;
        if x - w < min_x {
            break;
        }
        x -= w;
        pills.push((x, w, name));
        x -= 4.0;
    }
    if pills.is_empty() {
        return;
    }

    // Pill backgrounds — one Canvas, one submit for the whole row's pills.
    // SAFETY: the window draw list is the active one this frame; the Canvas
    // submits and releases the raw pointer when it drops at the block's end.
    unsafe {
        let dl = imgui::sys::igGetWindowDrawList();
        let mut canvas = frame.canvas(dl);
        for &(px, w, _) in &pills {
            canvas.fill_rect(
                imgui_painter::Rect {
                    min: imgui_painter::Vec2 { x: px, y: y0 },
                    max: imgui_painter::Vec2 { x: px + w, y: y1 },
                },
                4.0,
                bg,
            );
        }
    }

    // Labels on top, on imgui's own draw list.
    let draw = ui.get_window_draw_list();
    for &(px, _, name) in &pills {
        draw.add_text([px + 5.0, y0 + 1.0], fg, name);
    }
}

fn draw_waveform_widget(
    ui: &imgui::Ui,
    browser: &mut SampleBrowser,
    frame: &mut imgui_painter::Frame<'_>,
    scrub_last_x: &mut Option<f32>,
) {
    let [cx, cy] = ui.cursor_screen_pos();
    let w = ui.content_region_avail()[0];
    const H: f32 = WAVEFORM_HEIGHT;

    // Interactive hit area (replaces the passive dummy) for hover + scrub.
    let clicked = ui.invisible_button("##waveform", [w, H]);
    let hovered = ui.is_item_hovered();
    let active = ui.is_item_active();
    // (start, duration) in SOURCE seconds that the shown waveform spans. The
    // playhead, hover label and scrub all map against this, so everything is
    // consistent whether we're showing the whole-file waveform or just a
    // loaded window while it computes.
    let axis = browser.waveform_axis();
    let mouse_x = ui.io().mouse_pos[0];

    // imgui's own draw list, for text only — imgui-painter owns fills and
    // strokes. `draw` stays live across the Canvas blocks below (each opens
    // the raw window draw list, which is the same underlying list); this is
    // the same interleave Stage 1 proved safe.
    let draw = ui.get_window_draw_list();

    let bg = color_u32(WAVEFORM_BG);
    let bar_color = color_u32(WAVEFORM_BAR);
    let playhead_color = color_u32(WAVEFORM_PLAYHEAD);
    let text_color = color_u32(WAVEFORM_TEXT);

    // Geometry helpers so the Canvas calls below read cleanly.
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| imgui_painter::Rect {
        min: imgui_painter::Vec2 { x: x0, y: y0 },
        max: imgui_painter::Vec2 { x: x1, y: y1 },
    };
    let pt = |x: f32, y: f32| imgui_painter::Vec2 { x, y };

    // Background + every bar in ONE Canvas — up to ~512 bars submitted as a
    // single reserve+copy, not one per bar. Drawn first (behind everything).
    // SAFETY: the window draw list is the active one this frame; the Canvas
    // submits and releases the raw pointer when it drops at the block's end.
    let has_peaks = unsafe {
        let dl = imgui::sys::igGetWindowDrawList();
        let mut canvas = frame.canvas(dl);
        canvas.fill_rect(rect(cx, cy, cx + w, cy + H), 0.0, bg);
        if let Some(peaks) = browser.waveform_peaks() {
            let bar_w = (w / peaks.num_buckets as f32).max(1.0);
            let mid_y = cy + H / 2.0;
            let half_h = H / 2.0;
            for (i, &(lo, hi)) in peaks.peaks.iter().enumerate() {
                let x = cx + i as f32 * bar_w;
                let y_top = mid_y - hi * half_h;
                let y_bot = (mid_y - lo * half_h).max(y_top + 1.0);
                canvas.fill_rect(rect(x, y_top, x + bar_w - 0.5, y_bot), 0.0, bar_color);
            }
            true
        } else {
            false
        }
    };

    match browser.playback_status() {
        PlaybackStatus::Playing {
            file,
            position,
            duration,
        } => {
            // Playhead position mapped onto the displayed axis (source-relative).
            // Its own Canvas at this exact z-point, so the label text below
            // stays over it, matching the pre-conversion draw order.
            if let Some((start, dur)) = axis {
                let t = (((position.as_secs_f64() - start) / dur) as f32).clamp(0.0, 1.0);
                let px = cx + t * w;
                unsafe {
                    let dl = imgui::sys::igGetWindowDrawList();
                    let mut canvas = frame.canvas(dl);
                    canvas.line(pt(px, cy), pt(px, cy + H), 1.0, playhead_color);
                }
            }
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let pos_s = position.as_secs();
            let dur_s = duration.as_secs();
            draw.add_text(
                [cx + 4.0, cy + 2.0],
                text_color,
                format!(
                    "{}  {}:{:02} / {}:{:02}",
                    name,
                    pos_s / 60,
                    pos_s % 60,
                    dur_s / 60,
                    dur_s % 60,
                ),
            );
        }
        PlaybackStatus::Loading { file } => {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            draw.add_text(
                [cx + 4.0, cy + H / 2.0 - 7.0],
                text_color,
                format!("Loading: {name}..."),
            );
        }
        PlaybackStatus::Idle => {
            if !has_peaks {
                draw.add_text(
                    [cx + 4.0, cy + H / 2.0 - 7.0],
                    color_u32([0.5, 0.5, 0.5, 0.7]),
                    "Idle",
                );
            }
        }
    }

    // Hover / scrub crosshair + time label at the mouse position.
    if let Some((start, dur)) = axis {
        if hovered || active {
            let mx = mouse_x.clamp(cx, cx + w);
            let hover = color_u32(WAVEFORM_HOVER);
            let mid = cy + H / 2.0;
            // Both crosshair strokes in one Canvas, submitted here so the
            // hover label below stays on top (pre-conversion draw order).
            unsafe {
                let dl = imgui::sys::igGetWindowDrawList();
                let mut canvas = frame.canvas(dl);
                canvas.line(pt(mx, cy), pt(mx, cy + H), 1.0, hover);
                canvas.line(pt(mx - 4.0, mid), pt(mx + 4.0, mid), 1.0, hover);
            }
            let frac = ((mx - cx) / w).clamp(0.0, 1.0) as f64;
            let t = (start + frac * dur) as u64;
            let label = format!("{}:{:02}", t / 60, t % 60);
            let lx = (mx + 4.0).clamp(cx + 2.0, cx + w - 36.0);
            draw.add_text([lx, cy + 2.0], color_u32(WAVEFORM_TEXT), label);
            ui.set_mouse_cursor(Some(imgui::MouseCursor::ResizeEW));
        }
    }

    // Click seeks once; drag follows the cursor. Re-seek only when the cursor
    // moved >= 1px since the last seek, so a held-still cursor lets audio play
    // forward instead of re-triggering the same grain every frame.
    if let Some((start, dur)) = axis {
        if active || clicked {
            let mx = mouse_x.clamp(cx, cx + w);
            if scrub_last_x.is_none_or(|lx| (mx - lx).abs() >= 1.0) {
                let frac = ((mx - cx) / w).clamp(0.0, 1.0) as f64;
                let target = (start + frac * dur).max(0.0);
                browser.seek_to(Duration::from_secs_f64(target));
                *scrub_last_x = Some(mx);
            }
        } else {
            *scrub_last_x = None;
        }
    } else {
        *scrub_last_x = None;
    }
}

/// Seed one [`MetaField`] from a resolved value (if any) and its capability —
/// called once per field on file-selection change, never per frame.
fn seed_metadata_field(
    field: &mut MetaField,
    value: Option<(String, MetadataSource)>,
    capability: Capability,
    max_len: Option<usize>,
) {
    let (buf, source) = match value {
        Some((v, s)) => (v, Some(s)),
        None => (String::new(), None),
    };
    field.seed = buf.clone();
    field.buf = buf;
    field.source = source;
    field.writable = capability.can_write();
    field.max_len = max_len;
}

/// One editable metadata row: label + provenance badge, then the input
/// itself (greyed out with a tooltip when the backend can't write it), then
/// a truncation warning if the edited value would be cut on save.
fn draw_metadata_field(
    ui: &imgui::Ui,
    label: &str,
    id: &str,
    hint: &str,
    field: &mut MetaField,
    frame: &mut imgui_painter::Frame<'_>,
    inset_material: &imgui_painter::Material,
) {
    ui.text_disabled(label);
    ui.same_line();
    let badge = field.source.map(source_label).unwrap_or("—");
    ui.text_disabled(format!("[{badge}]"));

    let w = ui.content_region_avail()[0];
    ui.set_next_item_width(w.max(80.0));
    // SAFETY: the closure submits exactly one stock single-line InputText.
    unsafe {
        imgui_painter::decorate_input_text(frame, inset_material, || {
            ui.input_text(id, &mut field.buf)
                .hint(hint)
                .read_only(!field.writable)
                .build()
        })
    };
    let hovered = ui.is_item_hovered();
    if !field.writable {
        if hovered {
            ui.tooltip_text(
                "Not writable for this file (inside a library only; RF64 is read-only)",
            );
        }
    } else if field.dirty() {
        if let Some(max) = field.max_len {
            if field.buf.len() > max {
                ui.text_colored(
                    [0.9, 0.7, 0.3, 1.0],
                    format!("will truncate to {max} bytes"),
                );
            }
        }
    }
}

/// Reveal `path` in the OS file manager, selecting the file where the platform
/// supports it. Best-effort: a spawn failure is logged, never surfaced — this
/// is a convenience, not a data operation.
// ponytail: Linux opens the containing folder (xdg-open has no portable
// "select this file"); macOS/Windows select the file itself. Upgrade path: a
// per-desktop-environment selection call if users ask for it.
fn reveal_in_file_manager(path: &Path) {
    use std::process::Command;
    let spawned = if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else {
        let dir = path.parent().unwrap_or(path);
        Command::new("xdg-open").arg(dir).spawn()
    };
    if let Err(e) = spawned {
        log::warn!("reveal {path:?} failed: {e}");
    }
}

/// Reseed the metadata editor from the file whenever the selection changes.
/// Idempotent within a frame (guarded on the path) so the Inspector summary
/// and the editor modal can both rely on it having run. Provenance is captured
/// here (today always `Embedded`) via `read_resolved_metadata`.
fn seed_metadata_editor(browser: &mut SampleBrowser, editor: &mut MetadataEditor) {
    let current_path = browser.current_file().map(Path::to_path_buf);
    if editor.path == current_path {
        return;
    }
    editor.path = current_path.clone();
    let Some(path) = current_path.as_deref() else {
        *editor = MetadataEditor::default();
        return;
    };
    let resolved = browser.read_resolved_metadata(path).unwrap_or_default();
    let (cap, max) = browser.metadata_field_status(path, Field::Description);
    seed_metadata_field(
        &mut editor.description,
        resolved.description.map(|s| (s.value, s.source)),
        cap,
        max,
    );
    let (cap, max) = browser.metadata_field_status(path, Field::Keywords);
    seed_metadata_field(
        &mut editor.keywords,
        resolved.keywords.map(|s| (s.value.join(", "), s.source)),
        cap,
        max,
    );
    let (cap, max) = browser.metadata_field_status(path, Field::Category);
    seed_metadata_field(
        &mut editor.category,
        resolved.category.map(|s| (s.value, s.source)),
        cap,
        max,
    );
    let (cap, max) = browser.metadata_field_status(path, Field::Creator);
    seed_metadata_field(
        &mut editor.creator,
        resolved.creator.map(|s| (s.value, s.source)),
        cap,
        max,
    );
}

/// Read-only, low-visual-weight metadata summary for the Inspector — one line
/// per field showing the file's committed value (or `—`). Editing happens in
/// the modal opened by the caller's button, not here (metadata is a ~5%-of-time
/// concern; it shouldn't sit as always-live edit fields in the reading flow).
fn metadata_summary(ui: &imgui::Ui, editor: &MetadataEditor) {
    if editor.path.is_none() {
        ui.text_disabled("No file selected");
        return;
    }
    let row = |label: &str, f: &MetaField| {
        let val = if f.seed.is_empty() {
            "—"
        } else {
            f.seed.as_str()
        };
        ui.text_disabled(format!("{label}: {val}"));
    };
    row("Description", &editor.description);
    row("Keywords", &editor.keywords);
    row("Category", &editor.category);
    row("Creator", &editor.creator);
}

/// The provenance-aware metadata editor, drawn inside a modal so it stays out
/// of the primary flow until asked for. Description/Keywords/Category/Creator,
/// each editable (or read-only, per `Capability`) with a source badge; one Save
/// writes every dirty field in a single undoable `set_metadata` call. Assumes
/// `seed_metadata_editor` has already run this frame.
fn draw_metadata_editor_modal(
    ui: &imgui::Ui,
    browser: &mut SampleBrowser,
    editor: &mut MetadataEditor,
    frame: &mut imgui_painter::Frame<'_>,
) {
    let toolbar_material = theme::toolbar_material();
    let raised_material = theme::raised_material();
    let inset_material = theme::inset_field_material();
    ui.modal_popup("Edit Metadata##metadata_editor", || {
        let Some(path) = editor.path.clone() else {
            ui.close_current_popup();
            return;
        };
        draw_metadata_field(
            ui,
            "Description",
            "##meta-description",
            "Description...",
            &mut editor.description,
            frame,
            &inset_material,
        );
        draw_metadata_field(
            ui,
            "Keywords",
            "##meta-keywords",
            "Keywords, comma-separated...",
            &mut editor.keywords,
            frame,
            &inset_material,
        );
        draw_metadata_field(
            ui,
            "Category",
            "##meta-category",
            "Category...",
            &mut editor.category,
            frame,
            &inset_material,
        );
        draw_metadata_field(
            ui,
            "Creator",
            "##meta-creator",
            "Creator...",
            &mut editor.creator,
            frame,
            &inset_material,
        );

        ui.separator();
        let dirty = editor.description.dirty()
            || editor.keywords.dirty()
            || editor.category.dirty()
            || editor.creator.dirty();
        // SAFETY: the closure submits exactly one stock button in the active window.
        let save = unsafe {
            imgui_painter::decorate_button(frame, &raised_material, || ui.button("Save"))
        };
        if save && dirty {
            let m = Metadata {
                description: editor
                    .description
                    .dirty()
                    .then(|| editor.description.buf.trim().to_string()),
                keywords: if editor.keywords.dirty() {
                    editor
                        .keywords
                        .buf
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    Vec::new()
                },
                category: editor
                    .category
                    .dirty()
                    .then(|| editor.category.buf.trim().to_string()),
                creator: editor
                    .creator
                    .dirty()
                    .then(|| editor.creator.buf.trim().to_string()),
            };
            browser.set_metadata(&path, m);
            // Force a reseed next frame from what was actually written (picks up
            // any backend truncation instead of echoing raw input).
            editor.path = None;
            ui.close_current_popup();
        }
        ui.same_line();
        // SAFETY: the closure submits exactly one stock button in the active window.
        if unsafe {
            imgui_painter::decorate_button(frame, &toolbar_material, || ui.button("Cancel"))
        } {
            // Discard in-progress edits: reseed from the file next frame.
            editor.path = None;
            ui.close_current_popup();
        }
    });
}
