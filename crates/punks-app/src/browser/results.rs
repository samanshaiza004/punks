//! The Results pane: the width-adaptive multi-column grid, row/box-select
//! interaction, keyboard navigation, and drag-out. Relocated out of
//! `browser/mod.rs`; built on `selection.rs`'s pure reducers, which this
//! file commits through `SampleBrowser::set_browse_selection` -- never a
//! parallel selection store (see `selection.rs`'s module doc).

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    canvas, fill, point, px, Bounds, Context, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollStrategy, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme};

use super::selection::{self, ClickModifiers, GridGeometry};
use super::{MainWindow, INSPECTOR_WIDTH, MIN_LIST_ROWS, SIDEBAR_WIDTH};
use crate::actions::{Confirm, CursorDown, CursorUp};
use crate::filesystem::FileEntry;
use crate::theme;
use crate::SampleBrowser;

/// Fixed row height: box-select's geometry math needs a *known* row height
/// to convert pixels to indices, so rows are sized explicitly rather than
/// left to content (M2's approach for a single, unselected column).
const ROW_HEIGHT: f32 = 28.0;

fn pixels_to_f32(p: Point<Pixels>) -> (f32, f32) {
    (f32::from(p.x), f32::from(p.y))
}

/// Commits a selection through whichever of `SampleBrowser`'s two parallel
/// selection stores (browse vs. text-search results, `search.rs`, M5) is
/// currently displayed -- both already exist on `SampleBrowser` and already
/// do their own filter/sort/dedup/clamp; this just picks the right one so
/// `results.rs` doesn't duplicate that logic per call site.
fn commit_selection(
    inner: &mut SampleBrowser,
    in_search: bool,
    selection: Vec<usize>,
    cursor: Option<usize>,
) {
    if in_search {
        inner.set_search_selection(selection, cursor);
    } else {
        inner.set_browse_selection(selection, cursor);
    }
}

/// Plays row `index` of whichever result set is currently displayed.
/// `play_selected()` only knows about the browse cursor, so search-mode
/// audition resolves the path from `search_results()` and calls
/// `play_file()` directly instead.
fn audition(inner: &mut SampleBrowser, in_search: bool, index: usize) {
    if in_search {
        if let Some(path) = inner
            .search_results()
            .and_then(|r| r.get(index))
            .map(|e| e.path.clone())
        {
            inner.play_file(&path);
        }
    } else {
        inner.play_selected();
    }
}

impl MainWindow {
    /// Space available to the grid: viewport width minus the fixed Sidebar/
    /// Inspector panes and their borders. Approximate (doesn't account for
    /// gpui-component's own internal flex rounding) -- good enough to drive
    /// `column_count`, and self-corrects on resize since a resize forces a
    /// re-render. Needs visual confirmation once screen access is available
    /// (see docs/gpui-component-audit.md's open items).
    fn grid_avail_width(&self, window: &Window) -> f32 {
        let viewport_width = f32::from(window.viewport_size().width);
        let inspector = if self.inspector_visible {
            INSPECTOR_WIDTH + 1.0
        } else {
            0.0
        };
        (viewport_width - SIDEBAR_WIDTH - 1.0 - inspector).max(selection::MIN_COLUMN_WIDTH)
    }

    fn grid_cols(&self, window: &Window) -> usize {
        selection::column_count(self.grid_avail_width(window), selection::MIN_COLUMN_WIDTH)
    }

    pub(super) fn render_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let in_search = inner.is_in_search_mode();
        let real_entries: Vec<FileEntry> = if in_search {
            inner.search_results().unwrap_or(&[]).to_vec()
        } else {
            inner.entries().to_vec()
        };
        let real_count = real_entries.len();
        // Search results are a flat, already-filtered file list -- padding
        // it with synthetic rows to force virtualization scale (like the
        // browse listing) would just be noise on top of real query results.
        let row_count = if in_search {
            real_count
        } else {
            real_count.max(MIN_LIST_ROWS)
        };
        let current_selection: Vec<usize> = if in_search {
            inner.search_selection().to_vec()
        } else {
            inner.selection().to_vec()
        };
        let cursor = if in_search {
            inner.search_selected()
        } else {
            inner.selected()
        };
        let revision = if in_search {
            inner.search_revision()
        } else {
            inner.browse_revision()
        };

        let avail_width = self.grid_avail_width(window);
        let cols = selection::column_count(avail_width, selection::MIN_COLUMN_WIDTH);
        let gutters = selection::COLUMN_GUTTER * (cols.saturating_sub(1)) as f32;
        let col_width = (avail_width - gutters) / cols as f32;
        let geometry = GridGeometry {
            cols,
            row_height: ROW_HEIGHT,
            col_width: col_width + selection::COLUMN_GUTTER,
        };
        let grid_rows = row_count.div_ceil(cols);

        let grid_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
        let grid_bounds_for_canvas = grid_bounds.clone();
        let grid_bounds_for_move = grid_bounds;

        let entries_for_list = real_entries.clone();
        let entries_for_move = real_entries.clone();
        let selection_for_list = current_selection.clone();

        v_flex()
            .id("results")
            .key_context("ResultsPanel")
            .track_focus(&self.results_focus)
            .flex_1()
            .h_full()
            .relative()
            .on_action(cx.listener(Self::cursor_up))
            .on_action(cx.listener(Self::cursor_down))
            .on_action(cx.listener(Self::confirm))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    // A row's own on_mouse_down calls stop_propagation, so
                    // reaching here means the press landed on empty grid
                    // background: start a box-select gesture and clear the
                    // previous selection (mirrors "click empty space clears
                    // selection").
                    let in_search = this.browser.read(cx).inner.is_in_search_mode();
                    let revision = if in_search {
                        this.browser.read(cx).inner.search_revision()
                    } else {
                        this.browser.read(cx).inner.browse_revision()
                    };
                    this.gesture.begin_background(revision);
                    this.box_select_start = Some(event.position);
                    this.box_select_current = Some(event.position);
                    this.browser.update(cx, |b, cx| {
                        commit_selection(&mut b.inner, in_search, Vec::new(), None);
                        cx.notify();
                    });
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    if !this.gesture.is_box_selecting() {
                        return;
                    }
                    let Some(start) = this.box_select_start else {
                        return;
                    };
                    this.box_select_current = Some(event.position);

                    let bounds = grid_bounds_for_move.get();
                    let scroll_offset = this.scroll_handle.0.borrow().base_handle.offset();
                    let (bx, by) = pixels_to_f32(bounds.origin);
                    let (sx, sy) = pixels_to_f32(scroll_offset);
                    let (startx, starty) = pixels_to_f32(start);
                    let (curx, cury) = pixels_to_f32(event.position);
                    let a = (startx - bx - sx, starty - by - sy);
                    let b = (curx - bx - sx, cury - by - sy);

                    let is_directory = |i: usize| {
                        entries_for_move
                            .get(i)
                            .is_some_and(|e: &FileEntry| e.is_directory)
                    };
                    let candidate =
                        selection::reduce_box_select(geometry, a, b, real_count, is_directory);

                    let in_search = this.browser.read(cx).inner.is_in_search_mode();
                    let current_revision = if in_search {
                        this.browser.read(cx).inner.search_revision()
                    } else {
                        this.browser.read(cx).inner.browse_revision()
                    };
                    // Stale-revision guard: a directory change (or a search
                    // re-run) mid-drag discards the in-progress box-select
                    // instead of partially committing it against a listing
                    // that no longer matches.
                    if let Some(candidate) = selection::selection_if_revision_matches(
                        candidate,
                        this.gesture.started_revision,
                        current_revision,
                    ) {
                        this.browser.update(cx, |b, cx| {
                            commit_selection(&mut b.inner, in_search, candidate, None);
                            cx.notify();
                        });
                    } else {
                        this.gesture.end();
                        this.box_select_start = None;
                        this.box_select_current = None;
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _window, cx| {
                    this.gesture.end();
                    this.box_select_start = None;
                    this.box_select_current = None;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _window, cx| {
                    this.gesture.end();
                    this.box_select_start = None;
                    this.box_select_current = None;
                    cx.notify();
                }),
            )
            .child(
                gpui::uniform_list(
                    "results-grid",
                    grid_rows,
                    cx.processor(
                        move |this: &mut Self, range: std::ops::Range<usize>, _window, cx| {
                            range
                                .map(|vrow| {
                                    this.render_grid_row(
                                        vrow,
                                        cols,
                                        col_width,
                                        real_count,
                                        &entries_for_list,
                                        &selection_for_list,
                                        cursor,
                                        revision,
                                        in_search,
                                        cx,
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                    ),
                )
                .track_scroll(&self.scroll_handle)
                .h_full(),
            )
            .child(
                // Box-select rectangle overlay: a translucent quad painted
                // between press-origin and the current point.
                //
                // ponytail: box-select doesn't auto-scroll past the viewport
                // edge yet. Upgrade path: nudge `self.scroll_handle` in the
                // on_mouse_move handler above when `event.position` is near
                // `grid_bounds`'s top/bottom edge, before computing `b`.
                canvas(
                    move |bounds, _window, _cx| {
                        grid_bounds_for_canvas.set(bounds);
                    },
                    {
                        let start = self.box_select_start;
                        let current = self.box_select_current;
                        move |_bounds, _prepaint, window, _cx| {
                            let (Some(start), Some(current)) = (start, current) else {
                                return;
                            };
                            let rect = Bounds::from_corners(
                                point(start.x.min(current.x), start.y.min(current.y)),
                                point(start.x.max(current.x), start.y.max(current.y)),
                            );
                            window.paint_quad(fill(rect, gpui::rgba(0x3d78c840)));
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                gpui::div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .text_color(gpui::rgb(theme::TEXT_MUTED))
                    .child(format!(
                        "{real_count} real / {row_count} total rows, {cols} cols"
                    )),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_grid_row(
        &mut self,
        vrow: usize,
        cols: usize,
        col_width: f32,
        real_count: usize,
        real_entries: &[FileEntry],
        selection: &[usize],
        cursor: Option<usize>,
        revision: u64,
        in_search: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .id(("grid-row", vrow))
            .gap(px(selection::COLUMN_GUTTER))
            .children((0..cols).map(|c| {
                let ix = vrow * cols + c;
                self.render_cell(
                    ix,
                    col_width,
                    real_count,
                    real_entries,
                    selection,
                    cursor,
                    revision,
                    in_search,
                    cx,
                )
            }))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_cell(
        &mut self,
        ix: usize,
        col_width: f32,
        real_count: usize,
        real_entries: &[FileEntry],
        selection: &[usize],
        cursor: Option<usize>,
        revision: u64,
        in_search: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let real_entry = real_entries.get(ix).filter(|_| ix < real_count);
        let label = match real_entry {
            Some(e) => format!("{}{}", if e.is_directory { "[dir] " } else { "" }, e.name),
            None => format!("(synthetic row {ix})"),
        };
        let is_synthetic = real_entry.is_none();
        let is_selected = Some(ix) == cursor || selection.binary_search(&ix).is_ok();

        let cell = gpui::div()
            .id(("row", ix))
            .w(px(col_width))
            .h(px(ROW_HEIGHT))
            .px_2()
            .py_1()
            .when(is_selected, |el| el.bg(cx.theme().list_active))
            .when(is_synthetic, |el| el.opacity(0.35))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    let browser = this.browser.read(cx);
                    // Search results are files only -- `set_search_selection`
                    // doesn't filter directories out because there are none.
                    let is_directory = !in_search
                        && browser
                            .inner
                            .entries()
                            .get(ix)
                            .is_some_and(|e| e.is_directory);
                    let current_selection: Vec<usize> = if in_search {
                        browser.inner.search_selection().to_vec()
                    } else {
                        browser.inner.selection().to_vec()
                    };
                    let modifiers = ClickModifiers {
                        shift: event.modifiers.shift,
                        toggle: event.modifiers.control || event.modifiers.platform,
                    };
                    let outcome = selection::reduce_click(
                        &current_selection,
                        this.gesture.anchor,
                        ix,
                        modifiers,
                        |i| i == ix && is_directory,
                    );
                    this.gesture.begin_row(ix, revision);
                    this.gesture.anchor = outcome.anchor;
                    this.browser.update(cx, |b, cx| {
                        commit_selection(
                            &mut b.inner,
                            in_search,
                            outcome.selection,
                            outcome.cursor,
                        );
                        if outcome.audition {
                            audition(&mut b.inner, in_search, ix);
                            b.ensure_playback_ticking(cx);
                        }
                        cx.notify();
                    });
                }),
            )
            .child(label);

        // Files (not directories, not synthetic padding) drag out to other
        // apps -- whole selection if the pressed row is itself selected,
        // otherwise just that row (`selection::drag_payload`, ported
        // verbatim from the old ImGui frontend).
        match real_entry.filter(|e| !e.is_directory) {
            Some(e) => {
                let is_row_selected = selection.binary_search(&ix).is_ok();
                let selected_paths = self.browser.read(cx).inner.selection_paths();
                let paths = selection::drag_payload(is_row_selected, &selected_paths, &e.path);
                crate::platform::drag::draggable(cell, crate::platform::drag::DragPaths(paths))
            }
            None => cell,
        }
    }

    fn cursor_up(&mut self, _: &CursorUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(-1, window, cx);
    }

    fn cursor_down(&mut self, _: &CursorDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(1, window, cx);
    }

    fn move_cursor(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let next = self.browser.update(cx, |b, cx| {
            let in_search = b.inner.is_in_search_mode();
            let len = if in_search {
                b.inner.search_results().map_or(0, <[_]>::len)
            } else {
                b.inner.entries().len()
            };
            let current = if in_search {
                b.inner.search_selected()
            } else {
                b.inner.selected()
            };
            let next = selection::nav_cursor(current, delta, len);
            if let Some(next) = next {
                if in_search {
                    b.inner.select_search_result(next);
                } else {
                    b.inner.select(next);
                }
                audition(&mut b.inner, in_search, next);
                b.ensure_playback_ticking(cx);
            }
            cx.notify();
            next
        });
        if let Some(next) = next {
            let cols = self.grid_cols(window).max(1);
            self.scroll_handle
                .scroll_to_item(next / cols, ScrollStrategy::Nearest);
        }
    }

    fn confirm(&mut self, _: &Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        self.browser.update(cx, |b, cx| {
            let in_search = b.inner.is_in_search_mode();
            if in_search {
                // Search results are files only -- confirm always auditions,
                // never navigates.
                if let Some(index) = b.inner.search_selected() {
                    audition(&mut b.inner, true, index);
                    b.ensure_playback_ticking(cx);
                }
            } else if let Some(index) = b.inner.selected() {
                let is_directory = b.inner.entries().get(index).is_some_and(|e| e.is_directory);
                if is_directory {
                    if let Err(e) = b.inner.navigate_into(index) {
                        log::warn!("navigate into failed: {e}");
                    }
                    b.ensure_polling(cx);
                } else {
                    b.inner.play_selected();
                    b.ensure_playback_ticking(cx);
                }
            }
            cx.notify();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::selection::SelectionGesture;
    use super::*;
    use gpui::{div, MouseButton, TestAppContext};

    #[derive(Clone)]
    struct TestRow {
        is_directory: bool,
    }

    /// Structurally mirrors the real `render_cell`/click-handler wiring
    /// (same `selection.rs` reducers, same click-modifier translation) but
    /// backed by fake in-memory rows -- never a real `Browser`/
    /// `SampleBrowser`, so these tests need no audio device and are safe on
    /// a headless runner. A flat (non-virtualized, single-column) row list
    /// is enough to exercise the dispatch logic; it doesn't need to
    /// reproduce the real grid's virtualization to prove the reducers wire
    /// up correctly.
    struct TestResultsView {
        rows: Vec<TestRow>,
        gesture: SelectionGesture,
        selection: Vec<usize>,
        cursor: Option<usize>,
        audition_log: Vec<usize>,
        focus_handle: gpui::FocusHandle,
    }

    impl TestResultsView {
        fn handle_click(&mut self, ix: usize, modifiers: ClickModifiers) {
            let is_directory = |i: usize| self.rows.get(i).is_some_and(|r| r.is_directory);
            let outcome = selection::reduce_click(
                &self.selection,
                self.gesture.anchor,
                ix,
                modifiers,
                is_directory,
            );
            self.gesture.anchor = outcome.anchor;
            self.selection = outcome.selection;
            self.cursor = outcome.cursor;
            if outcome.audition {
                self.audition_log.push(ix);
            }
        }

        fn cursor_up(&mut self, _: &CursorUp, _window: &mut Window, cx: &mut Context<Self>) {
            self.cursor = selection::nav_cursor(self.cursor, -1, self.rows.len());
            cx.notify();
        }

        fn cursor_down(&mut self, _: &CursorDown, _window: &mut Window, cx: &mut Context<Self>) {
            self.cursor = selection::nav_cursor(self.cursor, 1, self.rows.len());
            cx.notify();
        }
    }

    impl Render for TestResultsView {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("test-results")
                .key_context("ResultsPanel")
                .track_focus(&self.focus_handle)
                .on_action(cx.listener(Self::cursor_up))
                .on_action(cx.listener(Self::cursor_down))
                .size_full()
                .children((0..self.rows.len()).map(|ix| {
                    div()
                        .id(("row", ix))
                        .debug_selector(move || format!("row{ix}"))
                        .h(px(20.0))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                this.handle_click(
                                    ix,
                                    ClickModifiers {
                                        shift: event.modifiers.shift,
                                        toggle: event.modifiers.control || event.modifiers.platform,
                                    },
                                );
                                cx.notify();
                            }),
                        )
                }))
        }
    }

    fn rows(n: usize) -> Vec<TestRow> {
        (0..n)
            .map(|_| TestRow {
                is_directory: false,
            })
            .collect()
    }

    fn new_test_view(
        cx: &mut TestAppContext,
        n: usize,
    ) -> (gpui::Entity<TestResultsView>, &mut gpui::VisualTestContext) {
        cx.add_window_view(|window, cx| {
            let focus_handle = cx.focus_handle();
            window.focus(&focus_handle, cx);
            TestResultsView {
                rows: rows(n),
                gesture: SelectionGesture::default(),
                selection: Vec::new(),
                cursor: None,
                audition_log: Vec::new(),
                focus_handle,
            }
        })
    }

    #[gpui::test]
    fn plain_click_selects_and_logs_audition(cx: &mut TestAppContext) {
        let (view, cx) = new_test_view(cx, 5);

        let bounds = cx.debug_bounds("row2").expect("row2 should have rendered");
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());

        view.read_with(cx, |view, _| {
            assert_eq!(view.selection, vec![2]);
            assert_eq!(view.cursor, Some(2));
            assert_eq!(view.audition_log, vec![2]);
        });
    }

    #[gpui::test]
    fn modifier_click_extends_selection_without_auditioning(cx: &mut TestAppContext) {
        let (view, cx) = new_test_view(cx, 5);

        let bounds0 = cx.debug_bounds("row0").unwrap();
        cx.simulate_click(bounds0.center(), gpui::Modifiers::default());

        let bounds3 = cx.debug_bounds("row3").unwrap();
        cx.simulate_click(
            bounds3.center(),
            gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );

        view.read_with(cx, |view, _| {
            assert_eq!(view.selection, vec![0, 1, 2, 3]);
            // Shift-click never logs an audition -- only the audition_log
            // entry from the plain click at row 0 is present.
            assert_eq!(view.audition_log, vec![0]);
        });
    }

    #[gpui::test]
    fn cursor_up_and_down_move_the_cursor(cx: &mut TestAppContext) {
        let (view, cx) = new_test_view(cx, 5);

        cx.dispatch_action(CursorDown);
        view.read_with(cx, |view, _| assert_eq!(view.cursor, Some(0)));

        cx.dispatch_action(CursorDown);
        view.read_with(cx, |view, _| assert_eq!(view.cursor, Some(1)));

        cx.dispatch_action(CursorUp);
        view.read_with(cx, |view, _| assert_eq!(view.cursor, Some(0)));
    }
}
