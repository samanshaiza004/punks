//! Pure selection-gesture logic for the Results grid.
//!
//! [`crate::SampleBrowser`] (`TabState`) already owns the committed cursor
//! (`selected()`), the marked set (`selection()`), and the listing revision
//! (`browse_revision()`/`search_revision()`), with a commit API
//! (`set_browse_selection`/`set_search_selection`) that already filters
//! directories out of the batch set, sorts, dedups, and clamps the cursor.
//! Nothing here duplicates that -- everything below is either a pure
//! reducer (takes the browser's *current* selection/cursor plus one input
//! event, returns the *next* selection/cursor for the caller to commit) or
//! [`SelectionGesture`], which holds only the transient, mid-gesture state
//! that genuinely has no home in `SampleBrowser` because it isn't committed
//! data: which element a mouse-down landed on, the Shift-range anchor, and
//! the revision a gesture started at (so a directory change mid-gesture
//! discards it instead of partially committing).
//!
//! Every function here is plain data in, plain data out -- no GPUI, no
//! window, no audio device. `results.rs` wires GPUI mouse/keyboard events to
//! these reducers and commits their output through `SampleBrowser`.

use std::path::{Path, PathBuf};

/// Old ImGui results view laid entries out in width-adaptive columns; each
/// column is at least this wide, so wide windows show 2+ columns and narrow
/// ones collapse to 1. Ported verbatim (`ui.rs`'s `MIN_COLUMN_WIDTH`).
pub const MIN_COLUMN_WIDTH: f32 = 300.0;
/// Ported verbatim (`ui.rs`'s `COLUMN_GUTTER`).
pub const COLUMN_GUTTER: f32 = 8.0;

/// Ported verbatim (`ui.rs`'s `column_count`), generalized to take the
/// minimum column width as a parameter instead of reading the module
/// constant directly, so it composes with [`COLUMN_GUTTER`] at the call site
/// without hidden coupling.
pub fn column_count(avail_width: f32, min_column_width: f32) -> usize {
    ((avail_width / min_column_width).floor() as usize).max(1)
}

/// Where a gesture's mouse-down landed. Distinguishing these is what old
/// ImGui's `is_item_active()` per-item state gave for free; GPUI has no
/// equivalent, so [`SelectionGesture`] tracks it by hand. A press on a row
/// is drag/click territory; a press on empty list background starts a
/// box-select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureOrigin {
    Row(usize),
    Background,
}

/// Transient state for one in-progress selection gesture. Never holds
/// committed selection data -- see the module doc for why.
#[derive(Debug, Default, Clone)]
pub struct SelectionGesture {
    /// Anchor row for Shift-range-select (first row of the current run).
    pub anchor: Option<usize>,
    /// Where the current mouse-down landed.
    pub press_origin: Option<GestureOrigin>,
    /// `SampleBrowser::browse_revision()`/`search_revision()` captured when
    /// the gesture began; checked against the current revision at commit
    /// time via [`selection_if_revision_matches`].
    pub started_revision: u64,
}

impl SelectionGesture {
    pub fn begin_row(&mut self, index: usize, revision: u64) {
        self.press_origin = Some(GestureOrigin::Row(index));
        self.started_revision = revision;
    }

    pub fn begin_background(&mut self, revision: u64) {
        self.press_origin = Some(GestureOrigin::Background);
        self.started_revision = revision;
    }

    pub fn end(&mut self) {
        self.press_origin = None;
    }

    pub fn is_box_selecting(&self) -> bool {
        matches!(self.press_origin, Some(GestureOrigin::Background))
    }
}

/// Which modifier keys were held for a click. `toggle` is Ctrl **or** Cmd --
/// both are the platform's "add/remove this one item" convention, and the
/// old ImGui binding treated them identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClickModifiers {
    pub shift: bool,
    pub toggle: bool,
}

/// Result of one [`reduce_click`] call: the next selection/cursor/anchor to
/// commit, and whether this click should audition (play) the clicked file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickOutcome {
    pub selection: Vec<usize>,
    pub cursor: Option<usize>,
    pub anchor: Option<usize>,
    pub audition: bool,
}

/// Reduce a click on row `index` against the browser's *current* selection
/// and gesture anchor. Ports the semantics ImGui's `MultiSelect` computed
/// internally:
/// - Plain click: selects only `index`, auditions (directories collapse to
///   an empty selection the same way `SampleBrowser::select` does; audition
///   is gated on it being a file -- `play_selected` no-ops on directories
///   too, so this is defense in depth, not the only guard).
/// - Shift+click: range-selects from the current anchor (or `index` itself
///   if there's no anchor yet) through `index`, extending the existing
///   selection rather than replacing it. Never auditions.
/// - Ctrl/Cmd+click: toggles `index` into/out of the existing selection.
///   Never auditions.
///
/// `is_directory` excludes directory indices from the *committed* selection
/// in the shift/toggle branches -- directories stay cursor-only, matching
/// old ImGui's `selectable` guard on `apply_selection_requests`.
pub fn reduce_click(
    current_selection: &[usize],
    anchor: Option<usize>,
    index: usize,
    modifiers: ClickModifiers,
    is_directory: impl Fn(usize) -> bool,
) -> ClickOutcome {
    if modifiers.shift {
        let anchor_index = anchor.unwrap_or(index);
        let (lo, hi) = if anchor_index <= index {
            (anchor_index, index)
        } else {
            (index, anchor_index)
        };
        let mut selection: Vec<usize> = current_selection.to_vec();
        for i in lo..=hi {
            if !is_directory(i) && !selection.contains(&i) {
                selection.push(i);
            }
        }
        selection.sort_unstable();
        selection.dedup();
        return ClickOutcome {
            selection,
            cursor: Some(index),
            anchor: Some(anchor_index),
            audition: false,
        };
    }

    if modifiers.toggle {
        let mut selection: Vec<usize> = current_selection.to_vec();
        selection.sort_unstable();
        if !is_directory(index) {
            match selection.binary_search(&index) {
                Ok(pos) => {
                    selection.remove(pos);
                }
                Err(pos) => selection.insert(pos, index),
            }
        }
        return ClickOutcome {
            selection,
            cursor: Some(index),
            anchor: Some(index),
            audition: false,
        };
    }

    let is_dir = is_directory(index);
    ClickOutcome {
        selection: if is_dir { Vec::new() } else { vec![index] },
        cursor: Some(index),
        anchor: Some(index),
        audition: !is_dir,
    }
}

/// Grid geometry needed to map a pixel rectangle to row indices, mirroring
/// the old clipper's `row * cols + col` addressing. `col_width` is the
/// *stride* between column starts (rendered cell width plus any gutter
/// between cells) -- the hit-test boundary between columns, not necessarily
/// the cell's own painted width.
#[derive(Debug, Clone, Copy)]
pub struct GridGeometry {
    pub cols: usize,
    pub row_height: f32,
    pub col_width: f32,
}

/// Which grid indices a rectangle between `a` and `b` intersects. Both
/// points must already be in content space (scroll offset added in by the
/// caller) -- this is pure arithmetic against `geometry`, no per-row
/// hit-testing. Order of `a`/`b` doesn't matter (a drag can go in any
/// direction). Bounded to `[0, row_count)`.
pub fn indices_in_rect(
    geometry: GridGeometry,
    a: (f32, f32),
    b: (f32, f32),
    row_count: usize,
) -> Vec<usize> {
    if geometry.cols == 0 || geometry.row_height <= 0.0 || geometry.col_width <= 0.0 {
        return Vec::new();
    }
    let (x0, x1) = (a.0.min(b.0).max(0.0), a.0.max(b.0).max(0.0));
    let (y0, y1) = (a.1.min(b.1).max(0.0), a.1.max(b.1).max(0.0));

    let col_lo = ((x0 / geometry.col_width).floor() as usize).min(geometry.cols - 1);
    let col_hi = ((x1 / geometry.col_width).floor() as usize).min(geometry.cols - 1);
    let row_lo = (y0 / geometry.row_height).floor() as usize;
    let row_hi = (y1 / geometry.row_height).floor() as usize;

    let mut indices = Vec::new();
    for row in row_lo..=row_hi {
        for col in col_lo..=col_hi {
            let index = row * geometry.cols + col;
            if index < row_count {
                indices.push(index);
            }
        }
    }
    indices
}

/// Box-select: [`indices_in_rect`] filtered the same way [`reduce_click`]'s
/// shift/toggle branches filter -- directories are never part of the
/// committed batch set.
///
pub fn reduce_box_select(
    geometry: GridGeometry,
    a: (f32, f32),
    b: (f32, f32),
    row_count: usize,
    is_directory: impl Fn(usize) -> bool,
) -> Vec<usize> {
    indices_in_rect(geometry, a, b, row_count)
        .into_iter()
        .filter(|&i| !is_directory(i))
        .collect()
}

/// Clamp-move the keyboard cursor by `delta` rows (W/S nav). `None`
/// (nothing selected yet) steps from row 0 forward or the last row
/// backward, matching a natural "start browsing" first press. Clamps at
/// bounds rather than wrapping.
pub fn nav_cursor(current: Option<usize>, delta: isize, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let start = match current {
        Some(i) => i as isize,
        None if delta >= 0 => -1,
        None => len as isize,
    };
    let next = (start + delta).clamp(0, len as isize - 1);
    Some(next as usize)
}

/// Files a native drag carries from a row: the whole selection when the
/// pressed row is itself selected, otherwise just that row. Ported verbatim
/// from `ui.rs`'s `drag_payload` (framework-agnostic already).
pub fn drag_payload(
    is_selected: bool,
    selected_paths: &[PathBuf],
    row_path: &Path,
) -> Vec<PathBuf> {
    if is_selected && !selected_paths.is_empty() {
        selected_paths.to_vec()
    } else {
        vec![row_path.to_path_buf()]
    }
}

/// A directory change mid-gesture discards the whole in-progress selection
/// rather than partially committing it. Ported verbatim from `ui.rs`'s
/// `selection_if_revision_matches`.
pub fn selection_if_revision_matches(
    mut candidate: Vec<usize>,
    started_revision: u64,
    current_revision: u64,
) -> Option<Vec<usize>> {
    (started_revision == current_revision).then(|| {
        candidate.sort_unstable();
        candidate.dedup();
        candidate
    })
}
