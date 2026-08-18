//! The application shell: Sidebar | Results | Inspector, wired to a real
//! [`SampleBrowser`] through [`Browser`]. M2 scope only -- selection
//! semantics, the waveform/transport strip, and the inspector's detail
//! sections are later milestones; this proves the state/render boundary
//! (a real `SampleBrowser` driving a virtualized GPUI list) works first.

pub mod state;

pub use state::Browser;

use std::ops::Range;

use gpui::prelude::*;
use gpui::{uniform_list, Context, Entity, Window};
use gpui_component::{button::*, h_flex, v_flex, ActiveTheme};

use crate::actions::{OpenFolder, ToggleInspector};
use crate::filesystem::FileEntry;
use crate::platform::drag::{draggable, DragPaths};
use crate::theme;
use crate::LibraryState;

/// Results below this are padded with synthetic rows so the list actually
/// exercises `uniform_list`'s virtualization at the scale the spike proved
/// (10k rows) rather than trivially rendering a handful of real entries.
/// Synthetic rows are visually and structurally distinct from real ones
/// (see `render_row`) -- never conflated with real `SampleBrowser` data.
const MIN_LIST_ROWS: usize = 1000;

pub struct MainWindow {
    browser: Entity<Browser>,
    inspector_visible: bool,
    // Render-count diagnostics, gated the same way the old ImGui frontend's
    // `UiPerfProbe` was (`PUNKS_UI_PERF=1`) -- used to verify GPUI stays
    // event-driven (zero renders while genuinely idle), same method the
    // viability spike used.
    perf_enabled: bool,
    render_count: u64,
    perf_started: std::time::Instant,
    last_perf_log: std::time::Instant,
}

impl MainWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Read once here, matching the "cfg is read once by the caller"
        // contract `SampleBrowser::new`'s doc comment describes.
        let cfg = crate::config::load();
        let last_directory = cfg.last_directory.clone();
        let browser = cx.new(|_cx| {
            let mut browser =
                Browser::new(&cfg).expect("failed to initialize audio engine for M2 shell");
            if let Some(dir) = last_directory {
                match browser.inner.open_directory(&dir) {
                    Ok(()) => log::info!(
                        "restored last directory {dir:?}: {} entries",
                        browser.inner.entries().len()
                    ),
                    Err(e) => log::warn!("failed to restore last directory {dir:?}: {e}"),
                }
            }
            browser
        });
        cx.observe(&browser, |_this, _browser, cx| cx.notify())
            .detach();

        let now = std::time::Instant::now();
        Self {
            browser,
            inspector_visible: true,
            perf_enabled: std::env::var_os("PUNKS_UI_PERF").is_some_and(|v| v == "1"),
            render_count: 0,
            perf_started: now,
            last_perf_log: now,
        }
    }

    fn open_folder(&mut self, _: &OpenFolder, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        self.browser.update(cx, |browser, cx| {
            match browser.inner.open_directory(&path) {
                Ok(()) => {
                    let mut cfg = crate::config::load();
                    cfg.last_directory = Some(path);
                    crate::config::save(&cfg);
                }
                Err(e) => log::warn!("failed to open directory {path:?}: {e}"),
            }
            browser.ensure_polling(cx);
            cx.notify();
        });
    }

    fn toggle_inspector(
        &mut self,
        _: &ToggleInspector,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inspector_visible = !self.inspector_visible;
        cx.notify();
    }

    fn select_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.browser.update(cx, |browser, cx| {
            browser.inner.select(index);
            cx.notify();
        });
    }

    fn render_row(
        &mut self,
        ix: usize,
        real_count: usize,
        real_entries: &[FileEntry],
        selected: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let real_entry = real_entries.get(ix).filter(|_| ix < real_count);
        let label = match real_entry {
            Some(e) => format!("{}{}", if e.is_directory { "[dir] " } else { "" }, e.name),
            None => format!("(synthetic row {ix})"),
        };
        let is_synthetic = real_entry.is_none();

        let row = h_flex()
            .id(("row", ix))
            .px_2()
            .py_1()
            .when(Some(ix) == selected, |el| el.bg(cx.theme().list_active))
            .when(is_synthetic, |el| el.opacity(0.35))
            .cursor_pointer()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _window, cx| this.select_row(ix, cx)),
            )
            .child(label);

        // Files (not directories, not synthetic padding) drag out to other
        // apps -- the same mechanism the viability spike proved end-to-end
        // on macOS, now exercised by a real result row instead of a
        // standalone placeholder.
        match real_entry.filter(|e| !e.is_directory) {
            Some(e) => draggable(row, DragPaths(vec![e.path.clone()])),
            None => row,
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.perf_enabled {
            self.render_count += 1;
            if self.last_perf_log.elapsed() > std::time::Duration::from_secs(2) {
                let elapsed = self.perf_started.elapsed().as_secs_f64();
                log::info!(
                    "perf: {} renders in {:.2}s ({:.2}/s)",
                    self.render_count,
                    elapsed,
                    self.render_count as f64 / elapsed.max(0.001)
                );
                self.last_perf_log = std::time::Instant::now();
            }
        }

        let browser = self.browser.read(cx);
        let inner = &browser.inner;

        let library_state = inner.library_state();
        let tags = inner.library_tags().to_vec();
        let current_dir = inner
            .current_directory()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "No folder open".into());
        let real_entries: Vec<FileEntry> = inner.entries().to_vec();
        let selected = inner.selected();
        let selected_summary = selected
            .and_then(|i| real_entries.get(i))
            .map(|e| format!("{}  |  {} bytes", e.name, e.size_bytes))
            .unwrap_or_else(|| "No selection".into());

        let row_count = real_entries.len().max(MIN_LIST_ROWS);
        let real_count = real_entries.len();

        h_flex()
            .id("main-window")
            .key_context("MainWindow")
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::toggle_inspector))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                // Sidebar: real library/tag state.
                v_flex()
                    .id("sidebar")
                    .w(gpui::px(220.))
                    .h_full()
                    .p_3()
                    .gap_2()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(Button::new("open-folder").label("Open Folder...").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.open_folder(&OpenFolder, window, cx)
                        }),
                    ))
                    .child(current_dir)
                    .child(match library_state {
                        LibraryState::NotALibrary => "Library: not attached".to_string(),
                        LibraryState::Scanning => "Library: scanning...".to_string(),
                        LibraryState::Ready => "Library: ready".to_string(),
                    })
                    .child("Tags:")
                    .children(
                        tags.into_iter()
                            .map(|t| format!("  {} ({})", t.name, t.count)),
                    ),
            )
            .child(
                // Results: real entries, padded with synthetic rows so the
                // list genuinely exercises virtualization at scale.
                v_flex()
                    .id("results")
                    .flex_1()
                    .h_full()
                    .child(
                        uniform_list(
                            "results-list",
                            row_count,
                            cx.processor(
                                move |this: &mut Self, range: Range<usize>, _window, cx| {
                                    range
                                        .map(|ix| {
                                            this.render_row(
                                                ix,
                                                real_count,
                                                &real_entries,
                                                selected,
                                                cx,
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .h_full(),
                    )
                    .child(
                        gpui::div()
                            .text_color(gpui::rgb(theme::TEXT_MUTED))
                            .child(format!("{real_count} real / {row_count} total rows")),
                    ),
            )
            .when(self.inspector_visible, |el| {
                el.child(
                    v_flex()
                        .id("inspector")
                        .w(gpui::px(280.))
                        .h_full()
                        .p_3()
                        .gap_2()
                        .border_l_1()
                        .border_color(cx.theme().border)
                        .child("Inspector")
                        .child(selected_summary),
                )
            })
    }
}
