//! The application shell: Sidebar | Results | Inspector, wired to a real
//! [`SampleBrowser`] through [`Browser`]. Selection semantics live in
//! [`selection`] (pure reducers) and [`results`] (the GPUI wiring that
//! commits them); [`sidebar`] holds the library/tag pane. This file owns
//! composition/layout and the small amount of action wiring that doesn't
//! belong to any one pane.

#[cfg(test)]
mod component_audit;
pub mod inspector;
pub mod results;
pub mod search;
pub mod selection;
mod settings;
pub mod sidebar;
pub mod state;
pub mod transport;
mod waveform;

pub use state::Browser;

use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, Point, Window};
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState};
use gpui_component::slider::SliderState;
use gpui_component::{h_flex, v_flex, ActiveTheme, Sizable};

use crate::actions::{OpenFolder, Redo, ShowSettings, ToggleInspector, TogglePlayback, Undo};

/// Results below this are padded with synthetic rows so the list actually
/// exercises `uniform_list`'s virtualization at the scale the spike proved
/// (10k rows) rather than trivially rendering a handful of real entries.
/// Synthetic rows are visually and structurally distinct from real ones --
/// never conflated with real `SampleBrowser` data.
pub(crate) const MIN_LIST_ROWS: usize = 1000;

/// Pane widths, shared between `sidebar.rs`'s/this file's inspector layout
/// and `results.rs`'s available-width calculation for the grid's column
/// count -- kept as one source of truth so the two stay consistent.
pub(crate) const SIDEBAR_WIDTH: f32 = 220.0;
pub(crate) const INSPECTOR_WIDTH: f32 = 280.0;

pub struct MainWindow {
    browser: Entity<Browser>,
    inspector_visible: bool,
    settings_visible: bool,
    settings_inputs: settings::KeybindInputs,
    settings_error: Option<String>,
    settings_saved: bool,
    // Render-count diagnostics, gated the same way the old ImGui frontend's
    // `UiPerfProbe` was (`PUNKS_UI_PERF=1`) -- used to verify GPUI stays
    // event-driven (zero renders while genuinely idle), same method the
    // viability spike used.
    perf_enabled: bool,
    render_count: u64,
    perf_started: std::time::Instant,
    last_perf_log: std::time::Instant,

    // Results pane state -- see `results.rs` for the methods that own it.
    // Transient gesture state (never committed selection data -- see
    // `selection.rs`'s module doc for why it lives separately from
    // `SampleBrowser`).
    gesture: selection::SelectionGesture,
    // Box-select's press-origin and current-drag point, in window-space
    // pixels. GPUI-specific (not portable/testable data), unlike
    // `SelectionGesture`'s fields -- see `results.rs`.
    box_select_start: Option<Point<gpui::Pixels>>,
    box_select_current: Option<Point<gpui::Pixels>>,
    scroll_handle: gpui::UniformListScrollHandle,
    results_focus: FocusHandle,

    // Transport strip state -- see `transport.rs`/`waveform.rs`.
    volume_slider: Entity<SliderState>,
    waveform_cache: waveform::BrowserWaveformCache,

    // Search bar state -- see `search.rs`.
    search_input: Entity<InputState>,

    // Inspector pane state -- see `inspector.rs`.
    description_input: Entity<InputState>,
    // The path `description_input` was last seeded from, so switching the
    // inspected file re-seeds the field instead of leaking one file's
    // in-progress edit onto the next.
    description_seeded_for: Option<std::path::PathBuf>,
}

impl MainWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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

        let volume_slider = Self::new_volume_slider(cfg.volume, cx);
        let search_input = Self::new_search_input(window, cx);
        let description_input = Self::new_description_input(window, cx);
        let settings_inputs = settings::KeybindInputs::new(window, cx, &cfg.keybinds);

        let now = std::time::Instant::now();
        Self {
            browser,
            inspector_visible: cfg.inspector_visible,
            settings_visible: false,
            settings_inputs,
            settings_error: None,
            settings_saved: false,
            perf_enabled: std::env::var_os("PUNKS_UI_PERF").is_some_and(|v| v == "1"),
            render_count: 0,
            perf_started: now,
            last_perf_log: now,
            gesture: selection::SelectionGesture::default(),
            box_select_start: None,
            box_select_current: None,
            scroll_handle: gpui::UniformListScrollHandle::new(),
            results_focus: cx.focus_handle(),
            volume_slider,
            waveform_cache: waveform::BrowserWaveformCache::default(),
            search_input,
            description_input,
            description_seeded_for: None,
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
        let mut cfg = crate::config::load();
        cfg.inspector_visible = self.inspector_visible;
        crate::config::save(&cfg);
        cx.notify();
    }

    fn toggle_playback_action(
        &mut self,
        _: &TogglePlayback,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_playback(cx);
    }

    fn undo_action(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        self.browser.update(cx, |browser, cx| {
            browser.inner.undo();
            cx.notify();
        });
    }

    fn redo_action(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        self.browser.update(cx, |browser, cx| {
            browser.inner.redo();
            cx.notify();
        });
    }

    fn show_settings(&mut self, _: &ShowSettings, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(cx);
    }

    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_visible = true;
        self.settings_error = None;
        self.settings_saved = false;
        cx.notify();
    }

    fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_visible = false;
        self.settings_error = None;
        self.settings_saved = false;
        cx.notify();
    }

    fn apply_settings(&mut self, cx: &mut Context<Self>) {
        let draft = self.settings_inputs.read(cx);
        let mut cfg = crate::config::load();
        if let Err(error) = settings::apply_functional_keybinds(&mut cfg, &draft) {
            self.settings_error = Some(error.into());
            self.settings_saved = false;
            cx.notify();
            return;
        }
        cfg.volume = self.browser.read(cx).inner.volume();
        cfg.inspector_visible = self.inspector_visible;
        crate::config::save(&cfg);
        self.settings_error = None;
        self.settings_saved = true;
        cx.notify();
    }

    fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.settings_error.clone();
        let saved = self.settings_saved;
        let row = |label: &'static str, input: &Entity<InputState>| {
            h_flex()
                .items_center()
                .gap_2()
                .child(label)
                .child(Input::new(input).flex_1())
        };

        v_flex()
            .id("settings-panel")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(gpui::px(390.0))
            .p_4()
            .gap_2()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child("Settings")
                    .child(
                        Button::new("settings-close")
                            .label("Close")
                            .small()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.close_settings(cx);
                            })),
                    ),
            )
            .child("Keyboard shortcuts")
            .child("Changes apply after restart.")
            .child(row("Navigate up", &self.settings_inputs.navigate_up))
            .child(row("Navigate down", &self.settings_inputs.navigate_down))
            .child(row("Navigate back", &self.settings_inputs.navigate_back))
            .child(row("Confirm", &self.settings_inputs.confirm))
            .child(row("Play / Stop", &self.settings_inputs.play_stop))
            .child(row(
                "Toggle Inspector",
                &self.settings_inputs.toggle_inspector,
            ))
            .child(row("Undo", &self.settings_inputs.undo))
            .child(row("Redo", &self.settings_inputs.redo))
            .when_some(error, |el, error| {
                el.child(gpui::div().text_color(cx.theme().danger).child(error))
            })
            .when(saved, |el| {
                el.child("Saved. Restart Punks to activate new shortcuts.")
            })
            .child(
                Button::new("settings-apply")
                    .label("Apply")
                    .small()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.apply_settings(cx);
                    })),
            )
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        // Results is the default initial focus target. This only fires on
        // the very first frame (before anything has focus) -- once Search
        // or any other element takes focus, `window.focused(cx)` is `Some`,
        // so this never fights a later focus change (e.g. typing into
        // Search, which re-renders on every keystroke via its own
        // `InputEvent::Change` subscription).
        if window.focused(cx).is_none() {
            window.focus(&self.results_focus, cx);
        }

        let inspector_visible = self.inspector_visible;

        v_flex()
            .id("main-window")
            .key_context("MainWindow")
            .relative()
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::toggle_inspector))
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::toggle_playback_action))
            .on_action(cx.listener(Self::undo_action))
            .on_action(cx.listener(Self::redo_action))
            .on_action(cx.listener(Self::show_settings))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_search(cx))
            .child(
                h_flex()
                    .id("panes")
                    .flex_1()
                    .child(self.render_sidebar(cx))
                    .child(self.render_results(window, cx))
                    .when(inspector_visible, |el| {
                        el.child(self.render_inspector(window, cx))
                    }),
            )
            .child(
                v_flex()
                    .id("bottom-strip")
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(self.render_waveform(cx))
                    .child(self.render_transport(cx)),
            )
            .when(self.settings_visible, |el| {
                el.child(self.render_settings(cx))
            })
    }
}
