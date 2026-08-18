//! GPUI viability spike for Punks. Ugly on purpose: this proves capability, it
//! does not port the product. See docs/gpui-viability.md for the write-up.
//!
//! Layout: Search (left, top) -> Results (left, uniform_list, 10k rows) ->
//! Inspector (right: selected entry, synthetic waveform, transport, OS drag-out).
//! Tab cycles focus across exactly those three regions.

use std::cell::Cell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, CursorStyle, Div, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, ExternalDragPayload, FileDragPaths,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, IntoElement, KeyBinding, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, Role,
    ScrollStrategy, ShapedLine, SharedString, Style, TextRun, UTF16Selection,
    UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions, canvas, div, fill,
    point, prelude::*, px, relative, rgb, rgba, size, uniform_list,
};

use punks_audio::{PlaybackEngine, PlaybackStatus, WaveformPeaks};

const ENTRY_COUNT: usize = 10_000;
const WAVEFORM_BUCKETS: usize = 512;

// ---------------------------------------------------------------------------
// Fake library data
// ---------------------------------------------------------------------------

struct Entry {
    name: SharedString,
    duration_secs: f32,
    sample_rate: u32,
    fake_path: SharedString,
}

/// Deterministic pseudo-random fake entries. No `rand` dependency: this is a
/// spike, and a tiny xorshift is plenty for "10,000 plausible-looking rows".
fn xorshift(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn gen_entries(n: usize) -> Vec<Entry> {
    const PREFIXES: &[&str] = &[
        "Kick",
        "Snare",
        "HiHat",
        "Clap",
        "Tom",
        "Perc",
        "Vocal_Chop",
        "Pad",
        "Bass",
        "Riser",
        "Impact",
        "FX",
        "Field_Rec",
        "Foley_Door",
        "Foley_Glass",
        "Synth_Lead",
        "Guitar_Pluck",
        "Rain",
        "Wind",
        "Crowd",
    ];
    const SUFFIXES: &[&str] = &[
        "Warm", "Bright", "Deep", "Tight", "Loose", "Dirty", "Clean", "Wide", "Mono", "Layered",
    ];

    let mut seed = 0x9E3779B97F4A7C15u64;
    (0..n)
        .map(|i| {
            seed = xorshift(seed.wrapping_add(i as u64));
            let prefix = PREFIXES[(seed as usize) % PREFIXES.len()];
            seed = xorshift(seed);
            let suffix = SUFFIXES[(seed as usize) % SUFFIXES.len()];
            seed = xorshift(seed);
            let duration_secs = 0.15 + (seed % 8000) as f32 / 1000.0;
            seed = xorshift(seed);
            let sample_rate = if seed.is_multiple_of(5) {
                48_000
            } else {
                44_100
            };
            let name = format!("{prefix}_{suffix}_{:04}.wav", i);
            let fake_path = format!("/samples/{prefix}/{name}");
            Entry {
                name: name.into(),
                duration_secs,
                sample_rate,
                fake_path: fake_path.into(),
            }
        })
        .collect()
}

/// A minimal PCM16 mono WAV, written by hand (no `hound`). Real file, real
/// bytes on disk -- exercised through the real `punks-playback` decode path.
fn write_sine_wav(
    path: &Path,
    freq_hz: f32,
    seconds: f32,
    sample_rate: u32,
) -> std::io::Result<()> {
    let num_frames = (seconds * sample_rate as f32) as u32;
    let mut data = Vec::with_capacity(num_frames as usize * 2);
    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        let s = (t * freq_hz * std::f32::consts::TAU).sin() * 0.4;
        data.extend_from_slice(&((s * i16::MAX as f32) as i16).to_le_bytes());
    }
    let byte_rate = sample_rate * 2;
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fmt.extend_from_slice(&1u16.to_le_bytes()); // mono
    fmt.extend_from_slice(&sample_rate.to_le_bytes());
    fmt.extend_from_slice(&byte_rate.to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes()); // block align
    fmt.extend_from_slice(&16u16.to_le_bytes()); // bits/sample

    let mut body = Vec::new();
    body.extend_from_slice(b"WAVE");
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(&data);

    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    std::fs::write(path, out)
}

fn synthetic_peaks() -> WaveformPeaks {
    let mut peaks = Vec::with_capacity(WAVEFORM_BUCKETS);
    for i in 0..WAVEFORM_BUCKETS {
        let t = i as f32 / WAVEFORM_BUCKETS as f32;
        let envelope = (1.0 - (t - 0.5).abs() * 1.6).clamp(0.05, 1.0);
        let wobble = (t * 40.0).sin() * 0.15;
        let max = (envelope + wobble).clamp(0.0, 1.0);
        peaks.push((-max * 0.85, max));
    }
    WaveformPeaks {
        peaks,
        num_buckets: WAVEFORM_BUCKETS,
    }
}

// ---------------------------------------------------------------------------
// Search input: hand-rolled per GPUI's `EntityInputHandler`. There is no
// batteries-included stable text widget in GPUI as of this spike; this is
// the sanctioned pattern (adapted from gpui's own `examples/input.rs`),
// trimmed to insert/backspace/left/right/select-all/home/end + IME marked
// text. This is real native text editing (goes through the OS input method),
// not a hand-matched keymap.
// ---------------------------------------------------------------------------

actions!(
    spike,
    [
        FocusSearch,
        Tab,
        TabPrev,
        SelectNext,
        SelectPrev,
        PlaySelected,
        StopPlayback,
        AuditionOther,
        SiLeft,
        SiRight,
        SiSelectLeft,
        SiSelectRight,
        SiSelectAll,
        SiHome,
        SiEnd,
        SiBackspace,
        SiDelete,
        SiPaste,
        SiCopy,
        SiCut,
        Quit,
    ]
);

struct SearchInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl SearchInput {
    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| offset + i)
            .unwrap_or(self.content.len())
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

impl EntityInputHandler for SearchInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.marked_range = if !new_text.is_empty() {
            Some(range.start..range.start + new_text.len())
        } else {
            None
        };
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|nr| nr.start + range.start..nr.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct SearchTextElement {
    input: Entity<SearchInput>,
}

struct SearchPrepaint {
    line: Option<ShapedLine>,
    cursor: Option<gpui::PaintQuad>,
    selection: Option<gpui::PaintQuad>,
}

impl IntoElement for SearchTextElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SearchTextElement {
    type RequestLayoutState = ();
    type PrepaintState = SearchPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), rgba(0xffffff55).into())
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);

        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    rgb(0x89b4fa),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x89b4fa55),
                )),
                None,
            )
        };
        SearchPrepaint {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().unwrap();
        line.paint(
            bounds.origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .unwrap();
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for SearchInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        div()
            .id("search")
            .role(Role::TextInput)
            .aria_label(if self.content.is_empty() {
                SharedString::from("Search")
            } else {
                SharedString::from(format!("Search: {}", self.content))
            })
            .key_context("SearchInput")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(|this, _: &SiBackspace, window, cx| {
                if this.selected_range.is_empty() {
                    let prev = this.previous_boundary(this.cursor_offset());
                    this.select_to(prev, cx);
                }
                this.replace_text_in_range(None, "", window, cx);
            }))
            .on_action(cx.listener(|this, _: &SiDelete, window, cx| {
                if this.selected_range.is_empty() {
                    let next = this.next_boundary(this.cursor_offset());
                    this.select_to(next, cx);
                }
                this.replace_text_in_range(None, "", window, cx);
            }))
            .on_action(cx.listener(|this, _: &SiLeft, _window, cx| {
                if this.selected_range.is_empty() {
                    let prev = this.previous_boundary(this.cursor_offset());
                    this.move_to(prev, cx);
                } else {
                    this.move_to(this.selected_range.start, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SiRight, _window, cx| {
                if this.selected_range.is_empty() {
                    let next = this.next_boundary(this.cursor_offset());
                    this.move_to(next, cx);
                } else {
                    this.move_to(this.selected_range.end, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SiSelectLeft, _window, cx| {
                let prev = this.previous_boundary(this.cursor_offset());
                this.select_to(prev, cx);
            }))
            .on_action(cx.listener(|this, _: &SiSelectRight, _window, cx| {
                let next = this.next_boundary(this.cursor_offset());
                this.select_to(next, cx);
            }))
            .on_action(cx.listener(|this, _: &SiSelectAll, _window, cx| {
                this.move_to(0, cx);
                this.select_to(this.content.len(), cx);
            }))
            .on_action(cx.listener(|this, _: &SiHome, _window, cx| this.move_to(0, cx)))
            .on_action(cx.listener(|this, _: &SiEnd, _window, cx| {
                let len = this.content.len();
                this.move_to(len, cx)
            }))
            .on_action(cx.listener(|this, _: &SiPaste, window, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    this.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SiCopy, _window, cx| {
                if !this.selected_range.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        this.content[this.selected_range.clone()].to_string(),
                    ));
                }
            }))
            .on_action(cx.listener(|this, _: &SiCut, window, cx| {
                if !this.selected_range.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        this.content[this.selected_range.clone()].to_string(),
                    ));
                    this.replace_text_in_range(None, "", window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.is_selecting = true;
                    if event.modifiers.shift {
                        let ix = this.index_for_mouse_position(event.position);
                        this.select_to(ix, cx);
                    } else {
                        let ix = this.index_for_mouse_position(event.position);
                        this.move_to(ix, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _window, _cx| this.is_selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _window, _cx| this.is_selecting = false),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_selecting {
                    let ix = this.index_for_mouse_position(event.position);
                    this.select_to(ix, cx);
                }
            }))
            .bg(rgb(0x313244))
            .rounded_md()
            .border_2()
            .border_color(if focused {
                rgb(0x89b4fa)
            } else {
                rgb(0x313244)
            })
            .line_height(px(20.))
            .text_size(px(16.))
            .child(
                div()
                    .h(px(20. + 8. * 2.))
                    .w_full()
                    .p(px(8.))
                    .child(SearchTextElement { input: cx.entity() }),
            )
    }
}

impl Focusable for SearchInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ---------------------------------------------------------------------------
// Drag ghost: the floating label that follows the cursor during an in-app
// drag gesture, before/in addition to the OS taking over for the external
// portion of the drag.
// ---------------------------------------------------------------------------

struct DragGhost {
    label: SharedString,
    position: Point<Pixels>,
}

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.position.x - px(60.))
            .pt(self.position.y - px(14.))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x89b4fa))
                    .text_color(rgb(0x1e1e2e))
                    .text_xs()
                    .shadow_md()
                    .child(self.label.clone()),
            )
    }
}

// ---------------------------------------------------------------------------
// Root view
// ---------------------------------------------------------------------------

struct Spike {
    entries: Rc<Vec<Entry>>,
    filtered: Vec<u32>,
    selected: usize,
    scroll_handle: UniformListScrollHandle,
    search: Entity<SearchInput>,
    root_focus: FocusHandle,
    results_focus: FocusHandle,
    inspector_focus: FocusHandle,
    waveform: Rc<WaveformPeaks>,
    waveform_bounds: Rc<Cell<Bounds<Pixels>>>,
    playhead: f32,
    selection_range: Option<(f32, f32)>,
    wav_a: PathBuf,
    wav_b: PathBuf,
    playback: Option<PlaybackEngine>,
    status_text: String,
    is_polling: bool,
    last_error: Option<String>,
    render_count: u64,
    started_at: Instant,
    last_perf_log: Instant,
}

impl Spike {
    fn refilter(&mut self, query: &str) {
        let q = query.to_lowercase();
        self.filtered = if q.is_empty() {
            (0..self.entries.len() as u32).collect()
        } else {
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.name.to_lowercase().contains(&q))
                .map(|(i, _)| i as u32)
                .collect()
        };
        self.selected = 0;
        self.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.filtered
            .get(self.selected)
            .map(|&ix| &self.entries[ix as usize])
    }

    fn on_focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.search.read(cx).focus_handle.clone();
        window.focus(&handle, cx);
    }

    fn on_tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("on_tab fired");
        window.focus_next(cx);
        cx.notify();
    }

    fn on_tab_prev(&mut self, _: &TabPrev, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("on_tab_prev fired");
        window.focus_prev(cx);
        cx.notify();
    }

    fn on_select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        log::info!("on_select_next fired");
        if self.filtered.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        self.scroll_handle
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn on_select_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.selected.saturating_sub(1);
        self.scroll_handle
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn play_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(engine) = self.playback.as_mut() else {
            self.last_error = Some("no playback engine (audio device init failed)".into());
            cx.notify();
            return;
        };
        engine.play(&path);
        self.start_polling(cx);
        cx.notify();
    }

    fn on_play_selected(&mut self, _: &PlaySelected, _window: &mut Window, cx: &mut Context<Self>) {
        let path = self.wav_a.clone();
        self.play_path(path, cx);
    }

    fn on_audition_other(
        &mut self,
        _: &AuditionOther,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self.wav_b.clone();
        self.play_path(path, cx);
    }

    fn on_stop(&mut self, _: &StopPlayback, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(engine) = self.playback.as_mut() {
            engine.stop();
        }
        self.status_text = "stopped".into();
        cx.notify();
    }

    /// Poll the playback engine on a timer, but only while something is
    /// actually playing -- this is the "no busy loop while idle" rule
    /// applied to the one thing in this spike that legitimately needs
    /// periodic updates (the moving playhead).
    fn start_polling(&mut self, cx: &mut Context<Self>) {
        if self.is_polling {
            return;
        }
        self.is_polling = true;
        cx.spawn(async move |this, cx| {
            loop {
                let should_continue = this
                    .update(cx, |this, cx| {
                        if let Some(engine) = this.playback.as_mut() {
                            if let Some(err) = engine.poll() {
                                this.last_error = Some(err.to_string());
                            }
                            match engine.status() {
                                PlaybackStatus::Playing {
                                    position, duration, ..
                                } => {
                                    this.status_text = format!(
                                        "playing {:.2}s / {:.2}s",
                                        position.as_secs_f32(),
                                        duration.as_secs_f32()
                                    );
                                    if duration.as_secs_f32() > 0.0 {
                                        this.playhead = (position.as_secs_f32()
                                            / duration.as_secs_f32())
                                        .clamp(0.0, 1.0);
                                    }
                                    cx.notify();
                                    true
                                }
                                PlaybackStatus::Loading { .. } => {
                                    this.status_text = "loading...".into();
                                    cx.notify();
                                    true
                                }
                                PlaybackStatus::Idle => {
                                    this.status_text = "idle".into();
                                    this.is_polling = false;
                                    cx.notify();
                                    false
                                }
                            }
                        } else {
                            this.is_polling = false;
                            false
                        }
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
            }
        })
        .detach();
    }

    fn render_row(&mut self, ix: usize, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let entry_ix = self.filtered[ix];
        let entry = &self.entries[entry_ix as usize];
        let selected = ix == self.selected;
        div()
            .id(("row", ix))
            .role(Role::ListItem)
            .aria_label(SharedString::from(format!(
                "{}, {:.2} seconds, {} Hz",
                entry.name, entry.duration_secs, entry.sample_rate
            )))
            .aria_position_in_set(ix + 1)
            .aria_size_of_set(self.filtered.len())
            .flex()
            .flex_row()
            .justify_between()
            .px_2()
            .py_1()
            .when(selected, |d| d.bg(rgb(0x45475a)))
            .hover(|d| d.bg(rgb(0x313244)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.selected = ix;
                cx.notify();
            }))
            .child(
                div()
                    .text_color(rgb(0xcdd6f4))
                    .overflow_hidden()
                    .child(entry.name.clone()),
            )
            .child(
                div()
                    .text_color(rgb(0x9399b2))
                    .child(format!("{:.1}s", entry.duration_secs)),
            )
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_count += 1;
        if self.last_perf_log.elapsed() > Duration::from_secs(2) {
            let elapsed = self.started_at.elapsed().as_secs_f64();
            log::info!(
                "perf: {} renders in {:.2}s ({:.1}/s)",
                self.render_count,
                elapsed,
                self.render_count as f64 / elapsed.max(0.001)
            );
            self.last_perf_log = Instant::now();
        }

        let filtered_count = self.filtered.len();
        let waveform = self.waveform.clone();
        let waveform_bounds_write = self.waveform_bounds.clone();
        let waveform_bounds_read = self.waveform_bounds.clone();
        let playhead = self.playhead;
        let selection_range = self.selection_range;

        let selected_summary = self
            .selected_entry()
            .map(|e| {
                format!(
                    "{}  |  {:.2}s  |  {} Hz  |  {}",
                    e.name, e.duration_secs, e.sample_rate, e.fake_path
                )
            })
            .unwrap_or_else(|| "No selection".into());

        div()
            .id("root")
            .key_context("Spike")
            .track_focus(&self.root_focus)
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_tab_prev))
            .size_full()
            .flex()
            .flex_row()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .font_family("Helvetica")
            .child(
                // Left: search + results
                div()
                    .id("results-panel")
                    .w(px(420.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .p_3()
                    .gap_2()
                    .border_r_1()
                    .border_color(rgb(0x313244))
                    .child(self.search.clone())
                    .child(
                        div()
                            .id("results")
                            .role(Role::List)
                            .aria_label(SharedString::from(format!(
                                "Results, {filtered_count} items"
                            )))
                            .key_context("ResultsPanel")
                            .track_focus(&self.results_focus)
                            .on_action(cx.listener(Self::on_select_next))
                            .on_action(cx.listener(Self::on_select_prev))
                            .flex_1()
                            .when(self.results_focus.is_focused(window), |d| {
                                d.border_2().border_color(rgb(0x89b4fa))
                            })
                            .child(
                                uniform_list(
                                    "results-list",
                                    filtered_count,
                                    cx.processor(
                                        |this: &mut Self, range: Range<usize>, _window, cx| {
                                            range
                                                .map(|ix| this.render_row(ix, cx))
                                                .collect::<Vec<_>>()
                                        },
                                    ),
                                )
                                .track_scroll(&self.scroll_handle)
                                .h_full(),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x6c7086))
                            .child(format!("{filtered_count} / {} entries", self.entries.len())),
                    ),
            )
            .child(
                // Right: inspector
                div()
                    .id("inspector")
                    .role(Role::Group)
                    .aria_label("Inspector")
                    .key_context("Inspector")
                    .track_focus(&self.inspector_focus)
                    .on_action(cx.listener(Self::on_play_selected))
                    .on_action(cx.listener(Self::on_stop))
                    .on_action(cx.listener(Self::on_audition_other))
                    .when(self.inspector_focus.is_focused(window), |d| {
                        d.border_2().border_color(rgb(0x89b4fa))
                    })
                    .flex_1()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_3()
                    .child(div().text_lg().child(selected_summary))
                    .child(
                        // Waveform: synthetic 512-bucket custom painting.
                        div()
                            .id("waveform")
                            .role(Role::Slider)
                            .aria_label(SharedString::from(format!(
                                "Waveform, playhead at {:.0}%",
                                playhead * 100.0
                            )))
                            .cursor_pointer()
                            .h(px(160.))
                            .w_full()
                            .rounded_md()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                    let bounds = waveform_bounds_read.get();
                                    if bounds.size.width > px(0.) {
                                        let frac = ((event.position.x - bounds.left())
                                            / bounds.size.width)
                                            .clamp(0.0, 1.0);
                                        this.playhead = frac;
                                        this.selection_range = Some((frac, (frac + 0.1).min(1.0)));
                                        cx.notify();
                                    }
                                }),
                            )
                            .child(
                                canvas(
                                    move |bounds, _window, _cx| {
                                        waveform_bounds_write.set(bounds);
                                    },
                                    move |bounds, _prepaint, window, _cx| {
                                        window.paint_quad(fill(bounds, rgb(0x11111b)));

                                        if let Some((s, e)) = selection_range {
                                            let sel_bounds = Bounds::from_corners(
                                                point(
                                                    bounds.left() + bounds.size.width * s,
                                                    bounds.top(),
                                                ),
                                                point(
                                                    bounds.left() + bounds.size.width * e,
                                                    bounds.bottom(),
                                                ),
                                            );
                                            window.paint_quad(fill(sel_bounds, rgba(0x89b4fa33)));
                                        }

                                        let mid_y = bounds.top() + bounds.size.height * 0.5;
                                        let half_h = bounds.size.height * 0.5;
                                        let bucket_w =
                                            bounds.size.width / waveform.num_buckets as f32;
                                        for (i, (min, max)) in waveform.peaks.iter().enumerate() {
                                            let x = bounds.left() + bucket_w * i as f32;
                                            let y_top = mid_y - half_h * *max;
                                            let y_bot = mid_y - half_h * *min;
                                            window.paint_quad(fill(
                                                Bounds::from_corners(
                                                    point(x, y_top),
                                                    point(x + bucket_w.max(px(1.)), y_bot),
                                                ),
                                                rgb(0x89b4fa),
                                            ));
                                        }

                                        let playhead_x =
                                            bounds.left() + bounds.size.width * playhead;
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(playhead_x, bounds.top()),
                                                size(px(2.), bounds.size.height),
                                            ),
                                            rgb(0xf38ba8),
                                        ));
                                    },
                                )
                                .size_full(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                transport_button("play", "Play (space)").on_click(cx.listener(
                                    |this, _, _window, cx| {
                                        let path = this.wav_a.clone();
                                        this.play_path(path, cx);
                                    },
                                )),
                            )
                            .child(transport_button("stop", "Stop").on_click(cx.listener(
                                |this, _, window, cx| this.on_stop(&StopPlayback, window, cx),
                            )))
                            .child(transport_button("audition", "Audition other (a)").on_click(
                                cx.listener(|this, _, _window, cx| {
                                    let path = this.wav_b.clone();
                                    this.play_path(path, cx);
                                }),
                            )),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x9399b2))
                            .child(self.status_text.clone()),
                    )
                    .when_some(self.last_error.clone(), |d, err| {
                        d.child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xf38ba8))
                                .child(format!("error: {err}")),
                        )
                    })
                    .child(
                        div()
                            .mt_4()
                            .pt_3()
                            .border_t_1()
                            .border_color(rgb(0x313244))
                            .child(div().text_sm().text_color(rgb(0x9399b2)).child(
                                "Drag this out to Finder / Explorer / a file manager / a DAW:",
                            ))
                            .child({
                                let wav_a = self.wav_a.clone();
                                let wav_a_for_payload = wav_a.clone();
                                let label: SharedString = format!("{}", wav_a.display()).into();
                                div()
                                    .id("drag-source")
                                    .role(Role::Button)
                                    .aria_label(SharedString::from(format!(
                                        "Drag {} to another application",
                                        wav_a
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("file")
                                    )))
                                    .mt_1()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(0x45475a))
                                    .cursor(CursorStyle::OpenHand)
                                    .child(format!("\u{1F3B5} {}", label))
                                    .on_drag(
                                        wav_a.clone(),
                                        |path: &PathBuf, position, _window, cx| {
                                            cx.new(|_| DragGhost {
                                                label: path
                                                    .file_name()
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or("file")
                                                    .to_string()
                                                    .into(),
                                                position,
                                            })
                                        },
                                    )
                                    .external_drag_payload(move |_path: &PathBuf, _window, _cx| {
                                        Some(ExternalDragPayload::Files(FileDragPaths::new([(
                                            wav_a_for_payload.clone(),
                                            false,
                                        )])))
                                    })
                            }),
                    ),
            )
    }
}

fn transport_button(id: &'static str, label: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(0x45475a))
        .hover(|d| d.bg(rgb(0x585b70)))
        .cursor_pointer()
        .child(label)
}

fn run() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let tmp_dir = std::env::temp_dir().join("punks-gpui-spike");
    std::fs::create_dir_all(&tmp_dir).expect("create spike temp dir");
    let wav_a = tmp_dir.join("Sine_A_440Hz.wav");
    let wav_b = tmp_dir.join("Sine_B_880Hz.wav");
    write_sine_wav(&wav_a, 440.0, 1.5, 44_100).expect("write wav a");
    write_sine_wav(&wav_b, 880.0, 1.5, 44_100).expect("write wav b");

    gpui_platform::application().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("cmd-f", FocusSearch, None),
            KeyBinding::new("ctrl-f", FocusSearch, None),
            KeyBinding::new("tab", Tab, None),
            KeyBinding::new("shift-tab", TabPrev, None),
            KeyBinding::new("down", SelectNext, Some("ResultsPanel")),
            KeyBinding::new("up", SelectPrev, Some("ResultsPanel")),
            KeyBinding::new("space", PlaySelected, Some("Inspector")),
            KeyBinding::new("s", StopPlayback, Some("Inspector")),
            KeyBinding::new("a", AuditionOther, Some("Inspector")),
            KeyBinding::new("backspace", SiBackspace, Some("SearchInput")),
            KeyBinding::new("delete", SiDelete, Some("SearchInput")),
            KeyBinding::new("left", SiLeft, Some("SearchInput")),
            KeyBinding::new("right", SiRight, Some("SearchInput")),
            KeyBinding::new("shift-left", SiSelectLeft, Some("SearchInput")),
            KeyBinding::new("shift-right", SiSelectRight, Some("SearchInput")),
            KeyBinding::new("cmd-a", SiSelectAll, Some("SearchInput")),
            KeyBinding::new("home", SiHome, Some("SearchInput")),
            KeyBinding::new("end", SiEnd, Some("SearchInput")),
            KeyBinding::new("cmd-v", SiPaste, Some("SearchInput")),
            KeyBinding::new("cmd-c", SiCopy, Some("SearchInput")),
            KeyBinding::new("cmd-x", SiCut, Some("SearchInput")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());

        // Clean shutdown: quit once the last window closes (see gpui's own
        // examples/on_window_close_quit.rs for this exact pattern).
        cx.on_window_closed(|cx, _window_id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let entries = Rc::new(gen_entries(ENTRY_COUNT));
        let playback = match PlaybackEngine::new() {
            Ok(engine) => Some(engine),
            Err(err) => {
                log::warn!("playback engine init failed: {err}");
                None
            }
        };

        let bounds = Bounds::centered(None, size(px(1100.0), px(720.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| {
                let search = cx.new(|cx| SearchInput {
                    // `.tab_index()`/`.tab_stop()` only matter here, on the
                    // FocusHandle itself -- calling them on the `div()` that
                    // `.track_focus()`s an externally-owned handle is a
                    // silent no-op (see gpui elements/div.rs: the interactivity
                    // layer only applies its own tab_index/tab_stop when it
                    // auto-creates the handle, which it doesn't when you
                    // supply one). Confirmed by reading gpui's source after a
                    // Tab keypress fired the action but moved no focus.
                    focus_handle: cx.focus_handle().tab_index(1).tab_stop(true),
                    content: "".into(),
                    placeholder: "Search 10,000 samples...".into(),
                    selected_range: 0..0,
                    selection_reversed: false,
                    marked_range: None,
                    last_layout: None,
                    last_bounds: None,
                    is_selecting: false,
                });

                let root_focus = cx.focus_handle();
                let results_focus = cx.focus_handle().tab_index(2).tab_stop(true);
                let inspector_focus = cx.focus_handle().tab_index(3).tab_stop(true);
                window.focus(&root_focus, cx);

                let filtered = (0..entries.len() as u32).collect();
                cx.new(|cx| {
                    // The parent observes the child: idiomatic GPUI data flow, and
                    // sidesteps the chicken-and-egg problem of SearchInput wanting a
                    // handle back to Spike before Spike exists.
                    cx.observe(&search, |spike: &mut Spike, search, cx| {
                        let query = search.read(cx).content.to_string();
                        spike.refilter(&query);
                        cx.notify();
                    })
                    .detach();

                    Spike {
                        entries,
                        filtered,
                        selected: 0,
                        scroll_handle: UniformListScrollHandle::new(),
                        search,
                        root_focus,
                        results_focus,
                        inspector_focus,
                        waveform: Rc::new(synthetic_peaks()),
                        waveform_bounds: Rc::new(Cell::new(Bounds::default())),
                        playhead: 0.0,
                        selection_range: None,
                        wav_a,
                        wav_b,
                        playback,
                        status_text: "idle".into(),
                        is_polling: false,
                        last_error: None,
                        render_count: 0,
                        started_at: Instant::now(),
                        last_perf_log: Instant::now(),
                    }
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}

fn main() {
    run();
}
