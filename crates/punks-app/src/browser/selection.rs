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
//! window, no audio device -- so it's covered by ordinary `#[test]`s.
//! `results.rs` is what wires GPUI mouse/keyboard events to these reducers
//! and commits their output through `SampleBrowser`.

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
/// // ponytail: box-select doesn't auto-scroll past the viewport edge yet.
/// // Upgrade path: results.rs's on_mouse_move handler nudges the
/// // UniformListScrollHandle when the current point is near the viewport
/// // edge, before calling this function with the (now-scrolled) content
/// // coordinates.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_click_on_file_selects_alone_and_auditions() {
        let outcome = reduce_click(&[5, 6], Some(6), 2, ClickModifiers::default(), |_| false);
        assert_eq!(outcome.selection, vec![2]);
        assert_eq!(outcome.cursor, Some(2));
        assert_eq!(outcome.anchor, Some(2));
        assert!(outcome.audition);
    }

    #[test]
    fn plain_click_on_directory_selects_alone_without_auditioning() {
        let outcome = reduce_click(&[], None, 3, ClickModifiers::default(), |i| i == 3);
        assert!(outcome.selection.is_empty());
        assert_eq!(outcome.cursor, Some(3));
        assert!(!outcome.audition);
    }

    /// Mirrors `ui.rs`'s old `begin_and_end_requests_compose_on_one_candidate`:
    /// successive gesture reducer calls compose onto one running selection
    /// rather than each starting fresh.
    #[test]
    fn shift_then_toggle_click_compose_on_current_selection() {
        let is_dir = |_: usize| false;

        let after_plain = reduce_click(&[], None, 0, ClickModifiers::default(), is_dir);
        assert_eq!(after_plain.selection, vec![0]);
        assert_eq!(after_plain.anchor, Some(0));

        let shift = ClickModifiers {
            shift: true,
            toggle: false,
        };
        let after_shift =
            reduce_click(&after_plain.selection, after_plain.anchor, 3, shift, is_dir);
        assert_eq!(after_shift.selection, vec![0, 1, 2, 3]);
        assert!(!after_shift.audition);

        let toggle = ClickModifiers {
            shift: false,
            toggle: true,
        };
        let after_toggle = reduce_click(
            &after_shift.selection,
            after_shift.anchor,
            2,
            toggle,
            is_dir,
        );
        assert_eq!(after_toggle.selection, vec![0, 1, 3]);
        assert!(!after_toggle.audition);
    }

    /// Mirrors `ui.rs`'s old `select_all_clear_all_and_ranges_are_bounded`:
    /// a rect larger than the grid still only selects in-bounds indices, and
    /// a plain click afterward clears the multi-selection back down to one row.
    #[test]
    fn box_select_bounds_to_row_count_and_plain_click_clears_it() {
        let is_dir = |_: usize| false;
        let geometry = GridGeometry {
            cols: 2,
            row_height: 20.0,
            col_width: 100.0,
        };
        let all = reduce_box_select(geometry, (0.0, 0.0), (5000.0, 5000.0), 9, is_dir);
        assert_eq!(all, (0..9).collect::<Vec<_>>());

        let cleared = reduce_click(&all, None, 4, ClickModifiers::default(), is_dir);
        assert_eq!(cleared.selection, vec![4]);
    }

    /// Mirrors `ui.rs`'s old `browse_request_application_excludes_directories`.
    #[test]
    fn directories_are_excluded_from_committed_multi_selection() {
        let directories = [false, true, false, true, false];
        let is_dir = |i: usize| directories[i];
        let geometry = GridGeometry {
            cols: 5,
            row_height: 20.0,
            col_width: 100.0,
        };
        let selection = reduce_box_select(geometry, (0.0, 0.0), (500.0, 20.0), 5, is_dir);
        assert_eq!(selection, vec![0, 2, 4]);

        // Toggle-clicking a directory is a no-op on the committed set -- it
        // can still become the keyboard cursor (`ClickOutcome::cursor` is
        // set unconditionally), just never part of the marked set.
        let toggle = ClickModifiers {
            shift: false,
            toggle: true,
        };
        let outcome = reduce_click(&[], None, 1, toggle, is_dir);
        assert!(outcome.selection.is_empty());
        assert_eq!(outcome.cursor, Some(1));
    }

    #[test]
    fn indices_in_rect_maps_pixel_rect_to_grid_indices() {
        let geometry = GridGeometry {
            cols: 3,
            row_height: 20.0,
            col_width: 100.0,
        };
        // Spans columns 1..=2 of rows 0..=1 -- indices 1,2 (row 0) and 4,5
        // (row 1) at `row * cols + col` indexing.
        let indices = indices_in_rect(geometry, (110.0, 5.0), (250.0, 25.0), 9);
        assert_eq!(indices, vec![1, 2, 4, 5]);
    }

    #[test]
    fn indices_in_rect_is_direction_independent() {
        let geometry = GridGeometry {
            cols: 3,
            row_height: 20.0,
            col_width: 100.0,
        };
        let forward = indices_in_rect(geometry, (110.0, 5.0), (250.0, 25.0), 9);
        let backward = indices_in_rect(geometry, (250.0, 25.0), (110.0, 5.0), 9);
        assert_eq!(forward, backward);
    }

    #[test]
    fn column_count_is_width_adaptive_and_never_zero() {
        assert_eq!(column_count(1000.0, MIN_COLUMN_WIDTH), 3);
        assert_eq!(column_count(50.0, MIN_COLUMN_WIDTH), 1);
    }

    #[test]
    fn nav_cursor_clamps_at_bounds_and_seeds_from_empty_selection() {
        assert_eq!(nav_cursor(None, 1, 5), Some(0));
        assert_eq!(nav_cursor(None, -1, 5), Some(4));
        assert_eq!(nav_cursor(Some(0), -1, 5), Some(0));
        assert_eq!(nav_cursor(Some(4), 1, 5), Some(4));
        assert_eq!(nav_cursor(Some(2), 1, 5), Some(3));
        assert_eq!(nav_cursor(None, 1, 0), None);
    }

    /// Ported verbatim from `ui.rs`'s
    /// `changed_dataset_revision_discards_stale_index_requests`.
    #[test]
    fn changed_dataset_revision_discards_stale_index_requests() {
        assert_eq!(
            selection_if_revision_matches(vec![3, 1, 3], 7, 7),
            Some(vec![1, 3])
        );
        assert_eq!(selection_if_revision_matches(vec![1, 3], 7, 8), None);
    }

    /// Ported verbatim from `ui.rs`'s `unselected_row_drag_carries_only_that_row`.
    #[test]
    fn unselected_row_drag_carries_only_that_row() {
        let row = PathBuf::from("row.wav");
        let selected = vec![PathBuf::from("selected.wav")];
        assert_eq!(drag_payload(false, &selected, &row), vec![row]);
    }

    /// Ported verbatim from `ui.rs`'s `selected_row_drag_carries_all_selected_paths`.
    #[test]
    fn selected_row_drag_carries_all_selected_paths() {
        let row = PathBuf::from("row.wav");
        let selected = vec![PathBuf::from("first.wav"), PathBuf::from("second.wav")];
        assert_eq!(drag_payload(true, &selected, &row), selected);
    }

    /// Ported verbatim from `ui.rs`'s `selected_row_drag_never_carries_an_empty_payload`.
    #[test]
    fn selected_row_drag_never_carries_an_empty_payload() {
        let row = PathBuf::from("row.wav");
        assert_eq!(drag_payload(true, &[], &row), vec![row]);
    }
}
