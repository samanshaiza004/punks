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
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable};

use super::selection::{self, ClickModifiers, GridGeometry};
use super::{MainWindow, INSPECTOR_WIDTH, SIDEBAR_WIDTH};
use crate::actions::{Confirm, CursorDown, CursorUp};
use crate::filesystem::FileEntry;
use crate::SampleBrowser;

/// Fixed row height lets box-selection convert pixel positions to indices.
const ROW_HEIGHT: f32 = 28.0;

fn grid_row_count(entry_count: usize, columns: usize) -> usize {
    entry_count.div_ceil(columns)
}

fn pixels_to_f32(p: Point<Pixels>) -> (f32, f32) {
    (f32::from(p.x), f32::from(p.y))
}

/// Commits the selection to the active browse or search result list.
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
/// Search-mode audition resolves the path from `search_results()` and calls
/// `play_file()` directly; browse-mode audition uses the active browse
/// selection through `play_selected()`.
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
    fn render_location(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let breadcrumbs = inner.breadcrumbs();
        let can_navigate_up = inner.can_navigate_up();

        let mut row = h_flex()
            .id("results-location")
            .items_center()
            .gap_1()
            .flex_1()
            .overflow_hidden();
        if breadcrumbs.is_empty() {
            row = row.child(
                gpui::div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No folder open"),
            );
        } else {
            row = row.child(
                Button::new("navigate-up")
                    .label("↑")
                    .ghost()
                    .small()
                    .disabled(!can_navigate_up)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.navigate_back(window, cx);
                    })),
            );
            let last = breadcrumbs.len() - 1;
            for (level, crumb) in breadcrumbs.into_iter().enumerate() {
                if level > 0 {
                    row = row.child(
                        gpui::div()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground)
                            .child("/"),
                    );
                }
                if level < last {
                    row = row.child(
                        Button::new(("breadcrumb", level as u64))
                            .label(crumb)
                            .ghost()
                            .small()
                            .flex_shrink_0()
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.browser.update(cx, |browser, cx| {
                                    if let Err(error) = browser.inner.navigate_to_breadcrumb(level)
                                    {
                                        log::warn!("breadcrumb navigation failed: {error}");
                                    }
                                    browser.ensure_polling(cx);
                                    cx.notify();
                                });
                            })),
                    );
                } else {
                    row = row.child(
                        gpui::div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .whitespace_nowrap()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(crumb),
                    );
                }
            }
        }

        v_flex().flex_shrink_0().px_2().py_1().child(row)
    }

    /// Space available to the grid: viewport width minus the fixed Sidebar/
    /// Inspector panes and their borders. Approximate (doesn't account for
    /// gpui-component's own internal flex rounding) -- good enough to drive
    /// `column_count`, and self-corrects on resize since a resize forces a
    /// re-render.
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
        let grid_rows = grid_row_count(real_count, cols);

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
            .child(self.render_location(cx))
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
    }

    #[allow(clippy::too_many_arguments)]
    fn render_grid_row(
        &mut self,
        vrow: usize,
        cols: usize,
        col_width: f32,
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
        real_entries: &[FileEntry],
        selection: &[usize],
        cursor: Option<usize>,
        revision: u64,
        in_search: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(real_entry) = real_entries.get(ix) else {
            // `grid_rows` is derived from the same real-entry count, so this
            // is only a defensive empty slot if the listing changes between
            // render snapshots. Never manufacture production result data.
            return gpui::div()
                .id(("row-empty", ix))
                .w(px(col_width))
                .h(px(ROW_HEIGHT));
        };
        let label = format!(
            "{}{}",
            if real_entry.is_directory {
                "[dir] "
            } else {
                ""
            },
            real_entry.name
        );
        let is_selected = Some(ix) == cursor || selection.binary_search(&ix).is_ok();

        let cell = gpui::div()
            .id(("row", ix))
            .w(px(col_width))
            .h(px(ROW_HEIGHT))
            .px_2()
            .py_1()
            .when(is_selected, |el| el.bg(cx.theme().list_active))
            // Hover is presentation-only. Keep it off selected cells so the
            // established selection color remains unambiguous while the
            // pointer moves across the grid.
            .when(!is_selected, |el| {
                el.hover(|el| el.bg(cx.theme().tokens.list_hover))
            })
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
                            b.ensure_playback_drivers(cx);
                        }
                        cx.notify();
                    });
                }),
            )
            .child(label);

        // Files (not directories) drag out to other
        // apps -- whole selection if the pressed row is itself selected,
        // otherwise just that row (`selection::drag_payload`, ported
        // verbatim from the old ImGui frontend).
        if !real_entry.is_directory {
            let is_row_selected = selection.binary_search(&ix).is_ok();
            let selected_paths = self.browser.read(cx).inner.selection_paths();
            let paths = selection::drag_payload(is_row_selected, &selected_paths, &real_entry.path);
            crate::platform::drag::draggable(cell, crate::platform::drag::DragPaths(paths))
        } else {
            cell
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
                b.ensure_playback_drivers(cx);
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
                    b.ensure_playback_drivers(cx);
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
                    b.ensure_playback_drivers(cx);
                }
            }
            cx.notify();
        });
    }
}
