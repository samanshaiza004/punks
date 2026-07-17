//! ImGui-aware item decoration built on the renderer-agnostic painter core.
//!
//! Typed entry points bracket one stock ImGui widget, suppress only the
//! chrome imgui-painter replaces, and paint behind the widget while ImGui
//! keeps ownership of layout, input, navigation, text, and popup/tree state.

use imgui_sys as sys;

use crate::{Border, Canvas, Color, Frame, Rect, Shadow, Vec2};

const ANATOMY_COMPATIBILITY: &str =
    "Dear ImGui 1.91.9b / imgui-rs 0.12 fork 7a89260 / imgui-sys 0.12.0";
const ANATOMY_IMGUI_VERSION: &str = "1.91.9b";

const BUTTON_COLS: [sys::ImGuiCol; 3] = [
    sys::ImGuiCol_Button as _,
    sys::ImGuiCol_ButtonHovered as _,
    sys::ImGuiCol_ButtonActive as _,
];
const HEADER_COLS: [sys::ImGuiCol; 3] = [
    sys::ImGuiCol_Header as _,
    sys::ImGuiCol_HeaderHovered as _,
    sys::ImGuiCol_HeaderActive as _,
];
const FRAME_COLS: [sys::ImGuiCol; 3] = [
    sys::ImGuiCol_FrameBg as _,
    sys::ImGuiCol_FrameBgHovered as _,
    sys::ImGuiCol_FrameBgActive as _,
];
const SLIDER_COLS: [sys::ImGuiCol; 5] = [
    sys::ImGuiCol_FrameBg as _,
    sys::ImGuiCol_FrameBgHovered as _,
    sys::ImGuiCol_FrameBgActive as _,
    sys::ImGuiCol_SliderGrab as _,
    sys::ImGuiCol_SliderGrabActive as _,
];
const COMBO_COLS: [sys::ImGuiCol; 6] = [
    sys::ImGuiCol_FrameBg as _,
    sys::ImGuiCol_FrameBgHovered as _,
    sys::ImGuiCol_FrameBgActive as _,
    sys::ImGuiCol_Button as _,
    sys::ImGuiCol_ButtonHovered as _,
    sys::ImGuiCol_ButtonActive as _,
];

const TRANSPARENT: sys::ImVec4 = sys::ImVec4 {
    x: 0.0,
    y: 0.0,
    z: 0.0,
    w: 0.0,
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct ItemState {
    min: [f32; 2],
    max: [f32; 2],
    hovered: bool,
    active: bool,
    focused: bool,
    style_alpha: f32,
}

/// Fill colors for the interaction states shared by the current decorators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateColors {
    pub base: Color,
    pub hover: Color,
    pub active: Color,
}

impl StateColors {
    fn for_state(&self, state: &ItemState) -> Color {
        if state.active {
            self.active
        } else if state.hovered {
            self.hover
        } else {
            self.base
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectableVisualState {
    Pressed,
    Selected,
    Hovered,
    Idle,
}

fn selectable_visual_state(state: &ItemState, selected: bool) -> SelectableVisualState {
    if state.active {
        SelectableVisualState::Pressed
    } else if selected {
        SelectableVisualState::Selected
    } else if state.hovered {
        SelectableVisualState::Hovered
    } else {
        SelectableVisualState::Idle
    }
}

fn shade_color(color: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let scale = 1.0 - amount;
    let channel = |shift| (((color >> shift) & 0xff_u32) as f32 * scale).round() as u8;
    crate::rgba(
        channel(0),
        channel(8),
        channel(16),
        ((color >> 24) & 0xff) as u8,
    )
}

fn selectable_fill(material: &Material, state: &ItemState, selected: bool) -> Color {
    match selectable_visual_state(state, selected) {
        // `StateColors` deliberately remains small. Reuse Active for idle
        // selection, then darken it only while pressed so persistent rows do
        // not lose click feedback.
        SelectableVisualState::Pressed if selected => shade_color(material.fill.active, 0.12),
        SelectableVisualState::Pressed | SelectableVisualState::Selected => material.fill.active,
        SelectableVisualState::Hovered => material.fill.hover,
        SelectableVisualState::Idle => material.fill.base,
    }
}

/// The deliberately small appearance shared by today's typed decorators.
/// Multipart part styles and semantic states remain deferred until they have
/// a real public consumer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub radius: f32,
    pub fill: StateColors,
    pub border: Border,
    pub shadow: Option<Shadow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderStyle {
    pub track: Material,
    pub fill: Material,
    pub grab: Material,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComboStyle {
    pub frame: Material,
    pub arrow_region: Material,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeStyle {
    pub row: Material,
    pub disclosure: Material,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateColorSlot {
    Base,
    Hover,
    Active,
}

impl StateColorSlot {
    fn color(self, colors: &StateColors) -> Color {
        match self {
            Self::Base => colors.base,
            Self::Hover => colors.hover,
            Self::Active => colors.active,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SliderVisualState {
    Adjusting,
    Focused,
    Hovered,
    Idle,
}

fn slider_visual_state(state: &ItemState) -> SliderVisualState {
    if state.active {
        SliderVisualState::Adjusting
    } else if state.focused {
        SliderVisualState::Focused
    } else if state.hovered {
        SliderVisualState::Hovered
    } else {
        SliderVisualState::Idle
    }
}

fn slider_state_color_slot(state: SliderVisualState) -> StateColorSlot {
    match state {
        SliderVisualState::Idle => StateColorSlot::Base,
        SliderVisualState::Hovered | SliderVisualState::Focused => StateColorSlot::Hover,
        SliderVisualState::Adjusting => StateColorSlot::Active,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComboVisualState {
    Open,
    Pressed,
    Focused,
    Hovered,
    Idle,
}

fn combo_visual_state(state: &ItemState, popup_open: bool) -> ComboVisualState {
    if popup_open {
        ComboVisualState::Open
    } else if state.active {
        ComboVisualState::Pressed
    } else if state.focused {
        ComboVisualState::Focused
    } else if state.hovered {
        ComboVisualState::Hovered
    } else {
        ComboVisualState::Idle
    }
}

fn combo_state_color_slot(state: ComboVisualState) -> StateColorSlot {
    match state {
        ComboVisualState::Idle => StateColorSlot::Base,
        ComboVisualState::Hovered | ComboVisualState::Focused => StateColorSlot::Hover,
        ComboVisualState::Open | ComboVisualState::Pressed => StateColorSlot::Active,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeVisualState {
    Pressed,
    Selected,
    Focused,
    Hovered,
    Open,
    Idle,
}

/// Open ranks below Hovered/Focused: openness is already communicated by the
/// disclosure arrow, and letting it outrank hover made expanded rows feel
/// inert. It stays a named state (currently painting like Idle) so a future
/// distinct open treatment needs no re-plumbing.
fn tree_visual_state(state: &ItemState, selected: bool, open: bool) -> TreeVisualState {
    if state.active {
        TreeVisualState::Pressed
    } else if selected {
        TreeVisualState::Selected
    } else if state.focused {
        TreeVisualState::Focused
    } else if state.hovered {
        TreeVisualState::Hovered
    } else if open {
        TreeVisualState::Open
    } else {
        TreeVisualState::Idle
    }
}

fn tree_state_color_slot(state: TreeVisualState) -> StateColorSlot {
    match state {
        TreeVisualState::Idle | TreeVisualState::Open => StateColorSlot::Base,
        TreeVisualState::Hovered | TreeVisualState::Focused => StateColorSlot::Hover,
        TreeVisualState::Pressed | TreeVisualState::Selected => StateColorSlot::Active,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decorator {
    Button,
    Selectable,
    Checkbox,
    InputText,
    Slider,
    Combo,
    Tree,
}

impl Decorator {
    fn suppress_cols(self) -> &'static [sys::ImGuiCol] {
        match self {
            Self::Button => &BUTTON_COLS,
            Self::Selectable | Self::Tree => &HEADER_COLS,
            Self::Checkbox | Self::InputText => &FRAME_COLS,
            Self::Slider => &SLIDER_COLS,
            Self::Combo => &COMBO_COLS,
        }
    }

    /// ponytail: these formulas reproduce Dear ImGui 1.91.9b layout through
    /// public functions, but widget-part geometry is not an upstream contract.
    /// Upgrade path: the executable compatibility gate and visual gallery must
    /// be rerun on every imgui/imgui-sys bump.
    unsafe fn capture_chrome(self) -> Option<Rect> {
        let width = match self {
            Self::Button | Self::Selectable | Self::Tree => return None,
            Self::Checkbox => sys::igGetFrameHeight(),
            Self::InputText | Self::Slider | Self::Combo => sys::igCalcItemWidth(),
        };
        let height = sys::igGetFrameHeight();
        let mut min = sys::ImVec2 { x: 0.0, y: 0.0 };
        sys::igGetCursorScreenPos(&mut min);
        Some(rect(min.x, min.y, min.x + width, min.y + height))
    }
}

/// Allocation-free, private evidence of the parts the current widgets expose.
#[derive(Debug, Clone, Copy, PartialEq)]
enum WidgetAnatomy {
    Single {
        chrome: Rect,
    },
    Slider {
        frame: Rect,
        track: Rect,
        fill: Rect,
        grab: Rect,
    },
    Combo {
        frame: Rect,
        preview: Rect,
        arrow: Rect,
    },
    Tree {
        row: Rect,
        disclosure: Option<Rect>,
    },
}

impl WidgetAnatomy {
    fn chrome(self) -> Rect {
        match self {
            Self::Single { chrome } => chrome,
            Self::Slider { track, .. } => track,
            Self::Combo { frame, .. } => frame,
            Self::Tree { row, .. } => row,
        }
    }
}

struct StyleColorGuard {
    count: i32,
}

impl StyleColorGuard {
    unsafe fn push(cols: &[sys::ImGuiCol]) -> Self {
        for &col in cols {
            sys::igPushStyleColor_Vec4(col, TRANSPARENT);
        }
        Self {
            count: cols.len() as i32,
        }
    }
}

impl Drop for StyleColorGuard {
    fn drop(&mut self) {
        if self.count > 0 {
            unsafe { sys::igPopStyleColor(self.count) };
            self.count = 0;
        }
    }
}

struct ChannelSplitGuard {
    draw_list: *mut sys::ImDrawList,
    active: bool,
}

impl ChannelSplitGuard {
    unsafe fn split(draw_list: *mut sys::ImDrawList) -> Self {
        sys::ImDrawList_ChannelsSplit(draw_list, 3);
        sys::ImDrawList_ChannelsSetCurrent(draw_list, 1);
        Self {
            draw_list,
            active: true,
        }
    }

    unsafe fn background(&self) {
        sys::ImDrawList_ChannelsSetCurrent(self.draw_list, 0);
    }

    unsafe fn merge(mut self) {
        sys::ImDrawList_ChannelsMerge(self.draw_list);
        self.active = false;
    }
}

impl Drop for ChannelSplitGuard {
    fn drop(&mut self) {
        if self.active {
            unsafe { sys::ImDrawList_ChannelsMerge(self.draw_list) };
            self.active = false;
        }
    }
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect {
        min: Vec2 { x: x0, y: y0 },
        max: Vec2 { x: x1, y: y1 },
    }
}

fn item_rect(state: &ItemState) -> Rect {
    rect(state.min[0], state.min[1], state.max[0], state.max[1])
}

fn rect_width(value: Rect) -> f32 {
    value.max.x - value.min.x
}

fn rect_height(value: Rect) -> f32 {
    value.max.y - value.min.y
}

fn rect_is_valid(value: Rect) -> bool {
    [value.min.x, value.min.y, value.max.x, value.max.y]
        .into_iter()
        .all(f32::is_finite)
        && value.max.x >= value.min.x
        && value.max.y >= value.min.y
}

fn rect_contains(outer: Rect, inner: Rect) -> bool {
    const EPSILON: f32 = 0.5;
    inner.min.x >= outer.min.x - EPSILON
        && inner.min.y >= outer.min.y - EPSILON
        && inner.max.x <= outer.max.x + EPSILON
        && inner.max.y <= outer.max.y + EPSILON
}

fn normalized_linear(value: f32, min: f32, max: f32) -> f32 {
    debug_assert!(value.is_finite() && min.is_finite() && max.is_finite());
    if !value.is_finite() || !min.is_finite() || !max.is_finite() || min == max {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn slider_anatomy(
    frame: Rect,
    min: f32,
    max: f32,
    value: f32,
    grab_min_size: f32,
    framebuffer_scale: f32,
) -> WidgetAnatomy {
    const GRAB_PADDING: f32 = 2.0;
    let frame_width = rect_width(frame).max(0.0);
    let frame_height = rect_height(frame).max(0.0);
    let slider_size = (frame_width - GRAB_PADDING * 2.0).max(0.0);
    let grab_size = grab_min_size.max(0.0).min(slider_size);
    let usable_size = (slider_size - grab_size).max(0.0);
    let usable_min = frame.min.x + GRAB_PADDING + grab_size * 0.5;
    let grab_center = usable_min + usable_size * normalized_linear(value, min, max);
    let grab = rect(
        grab_center - grab_size * 0.5,
        (frame.min.y + GRAB_PADDING).min(frame.max.y),
        grab_center + grab_size * 0.5,
        (frame.max.y - GRAB_PADDING).max(frame.min.y),
    );

    let device_pixel = if framebuffer_scale.is_finite() && framebuffer_scale > 0.0 {
        1.0 / framebuffer_scale
    } else {
        1.0
    };
    let track_height = (frame_height * 0.25)
        .max(device_pixel * 2.0)
        .min(frame_height);
    let track_y = (frame.min.y + frame.max.y) * 0.5;
    let track = rect(
        (frame.min.x + GRAB_PADDING).min(frame.max.x),
        track_y - track_height * 0.5,
        (frame.max.x - GRAB_PADDING).max(frame.min.x),
        track_y + track_height * 0.5,
    );
    let fill = rect(
        track.min.x,
        track.min.y,
        grab_center.clamp(track.min.x, track.max.x),
        track.max.y,
    );

    WidgetAnatomy::Slider {
        frame,
        track,
        fill,
        grab,
    }
}

fn combo_anatomy(frame: Rect) -> WidgetAnatomy {
    let arrow_width = rect_height(frame).max(0.0).min(rect_width(frame).max(0.0));
    let split = frame.max.x - arrow_width;
    WidgetAnatomy::Combo {
        frame,
        preview: rect(frame.min.x, frame.min.y, split, frame.max.y),
        arrow: rect(split, frame.min.y, frame.max.x, frame.max.y),
    }
}

fn tree_anatomy(row: Rect, leaf: bool, disclosure_width: f32) -> WidgetAnatomy {
    let disclosure = (!leaf).then(|| {
        rect(
            row.min.x,
            row.min.y,
            (row.min.x + disclosure_width.max(0.0)).min(row.max.x),
            row.max.y,
        )
    });
    WidgetAnatomy::Tree { row, disclosure }
}

fn alpha_u8(color: Color) -> u8 {
    (color >> 24) as u8
}

fn with_style_alpha(color: Color, style_alpha: f32) -> Color {
    let alpha = style_alpha.clamp(0.0, 1.0);
    let resolved = ((alpha_u8(color) as f32 * alpha).round() as u8) as u32;
    (color & 0x00ff_ffff) | (resolved << 24)
}

fn resolved_border(border: Border, alpha: f32) -> Border {
    Border {
        color: with_style_alpha(border.color, alpha),
        ..border
    }
}

fn resolved_shadow(shadow: Shadow, alpha: f32) -> Shadow {
    Shadow {
        color: with_style_alpha(shadow.color, alpha),
        ..shadow
    }
}

fn paint_material(canvas: &mut Canvas<'_>, rect: Rect, material: &Material, state: &ItemState) {
    debug_assert!(rect_is_valid(rect));
    canvas.rounded_rect(rect, material.radius.min(rect_height(rect) * 0.5));
    if let Some(shadow) = material.shadow {
        canvas.add_shadow(&resolved_shadow(shadow, state.style_alpha));
    }
    canvas.fill_color(with_style_alpha(
        material.fill.for_state(state),
        state.style_alpha,
    ));
    canvas.add_border(&resolved_border(material.border, state.style_alpha));
}

fn paint_material_color(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    material: &Material,
    color: Color,
    style_alpha: f32,
) {
    debug_assert!(rect_is_valid(rect));
    canvas.rounded_rect(rect, material.radius.min(rect_height(rect) * 0.5));
    if let Some(shadow) = material.shadow {
        canvas.add_shadow(&resolved_shadow(shadow, style_alpha));
    }
    canvas.fill_color(with_style_alpha(color, style_alpha));
    canvas.add_border(&resolved_border(material.border, style_alpha));
}

fn paint_material_slot(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    material: &Material,
    slot: StateColorSlot,
    style_alpha: f32,
) {
    debug_assert!(rect_is_valid(rect));
    canvas.rounded_rect(rect, material.radius.min(rect_height(rect) * 0.5));
    if let Some(shadow) = material.shadow {
        canvas.add_shadow(&resolved_shadow(shadow, style_alpha));
    }
    canvas.fill_color(with_style_alpha(slot.color(&material.fill), style_alpha));
    canvas.add_border(&resolved_border(material.border, style_alpha));
}

fn paint_slider(
    canvas: &mut Canvas<'_>,
    anatomy: WidgetAnatomy,
    style: &SliderStyle,
    state: &ItemState,
    visual_state: SliderVisualState,
) {
    let WidgetAnatomy::Slider {
        track, fill, grab, ..
    } = anatomy
    else {
        unreachable!("slider painter requires slider anatomy")
    };
    let slot = slider_state_color_slot(visual_state);
    paint_material_slot(canvas, track, &style.track, slot, state.style_alpha);
    if rect_width(fill) > 0.0 {
        paint_material_slot(canvas, fill, &style.fill, slot, state.style_alpha);
    }
    paint_material_slot(canvas, grab, &style.grab, slot, state.style_alpha);
}

unsafe fn capture_item_state() -> ItemState {
    let mut min = sys::ImVec2 { x: 0.0, y: 0.0 };
    let mut max = sys::ImVec2 { x: 0.0, y: 0.0 };
    sys::igGetItemRectMin(&mut min);
    sys::igGetItemRectMax(&mut max);
    let style = &*sys::igGetStyle();
    ItemState {
        min: [min.x, min.y],
        max: [max.x, max.y],
        hovered: sys::igIsItemHovered(0),
        active: sys::igIsItemActive(),
        focused: sys::igIsItemFocused(),
        style_alpha: style.Alpha,
    }
}

unsafe fn item_paint<R>(
    frame: &mut Frame<'_>,
    decorator: Decorator,
    widget: impl FnOnce() -> R,
    paint: impl FnOnce(&R, &ItemState, Option<Rect>, &mut Canvas<'_>),
) -> R {
    debug_assert_eq!(
        std::ffi::CStr::from_ptr(sys::igGetVersion()).to_str().ok(),
        Some(ANATOMY_IMGUI_VERSION),
        "{ANATOMY_COMPATIBILITY}"
    );
    let draw_list = sys::igGetWindowDrawList();
    let captured = decorator.capture_chrome();
    let channels = ChannelSplitGuard::split(draw_list);
    let colors = StyleColorGuard::push(decorator.suppress_cols());

    let result = widget();
    let state = capture_item_state();
    drop(colors);

    channels.background();
    {
        let mut canvas = frame.canvas(draw_list);
        paint(&result, &state, captured, &mut canvas);
    }
    channels.merge();
    result
}

fn single_anatomy(
    decorator: Decorator,
    state: &ItemState,
    captured: Option<Rect>,
) -> WidgetAnatomy {
    let chrome = match decorator {
        Decorator::Button | Decorator::Selectable => item_rect(state),
        Decorator::Checkbox | Decorator::InputText => {
            captured.expect("multipart decorator captured chrome before submission")
        }
        _ => unreachable!("new multipart decorators resolve their own anatomy"),
    };
    debug_assert!(rect_is_valid(chrome));
    debug_assert!(rect_contains(item_rect(state), chrome));
    WidgetAnatomy::Single { chrome }
}

/// Decorate one stock ImGui Button while preserving its behavior and text.
///
/// # Safety
/// A live current ImGui context/window/frame and unsplit current draw list are
/// required on the context-owning thread.
///
/// # Correctness
/// The closure must submit exactly one stock Button. A mismatch produces wrong
/// pixels rather than memory unsafety.
pub unsafe fn decorate_button(
    frame: &mut Frame<'_>,
    material: &Material,
    widget: impl FnOnce() -> bool,
) -> bool {
    item_paint(
        frame,
        Decorator::Button,
        widget,
        |_, state, captured, canvas| {
            let anatomy = single_anatomy(Decorator::Button, state, captured);
            paint_material(canvas, anatomy.chrome(), material, state);
        },
    )
}

/// Decorate one stock ImGui Selectable while preserving its behavior and text.
///
/// Last-item ID, bounds, hover, active, and drag/drop attachment remain the
/// submitted Selectable's after this function returns. Persistent selection
/// paints below active interaction but above hover/base without changing
/// ImGui's own selected state or return value.
///
/// # Safety
/// A live current ImGui context/window/frame and unsplit current draw list are required.
///
/// # Correctness
/// The closure must submit exactly one stock Selectable.
pub unsafe fn decorate_selectable(
    frame: &mut Frame<'_>,
    material: &Material,
    selected: bool,
    widget: impl FnOnce() -> bool,
) -> bool {
    item_paint(
        frame,
        Decorator::Selectable,
        widget,
        |_, state, captured, canvas| {
            let anatomy = single_anatomy(Decorator::Selectable, state, captured);
            paint_material_color(
                canvas,
                anatomy.chrome(),
                material,
                selectable_fill(material, state, selected),
                state.style_alpha,
            );
        },
    )
}

/// Decorate one stock ImGui Checkbox, painting only its box.
///
/// # Safety
/// A live current ImGui context/window/frame and unsplit current draw list are required.
///
/// # Correctness
/// The closure must submit exactly one stock Checkbox.
pub unsafe fn decorate_checkbox(
    frame: &mut Frame<'_>,
    material: &Material,
    widget: impl FnOnce() -> bool,
) -> bool {
    item_paint(
        frame,
        Decorator::Checkbox,
        widget,
        |_, state, captured, canvas| {
            let anatomy = single_anatomy(Decorator::Checkbox, state, captured);
            paint_material(canvas, anatomy.chrome(), material, state);
        },
    )
}

/// Decorate one stock single-line ImGui InputText, excluding its visible label.
///
/// # Safety
/// A live current ImGui context/window/frame and unsplit current draw list are required.
///
/// # Correctness
/// The closure must submit exactly one single-line InputText.
pub unsafe fn decorate_input_text(
    frame: &mut Frame<'_>,
    material: &Material,
    widget: impl FnOnce() -> bool,
) -> bool {
    item_paint(
        frame,
        Decorator::InputText,
        widget,
        |_, state, captured, canvas| {
            let anatomy = single_anatomy(Decorator::InputText, state, captured);
            paint_material(canvas, anatomy.chrome(), material, state);
        },
    )
}

/// Decorate one horizontal, linear stock ImGui `f32` Slider.
///
/// paints track, completed fill, and grab (SliderGrab* now suppressed).
/// ImGui owns: formatted value text, label, navigation highlight.
///
/// # Safety
/// A live current ImGui context/window/frame and unsplit current draw list are required.
///
/// # Correctness
/// The closure must submit exactly one horizontal linear `f32` Slider using
/// this same `value`, `min`, and `max`. Dear ImGui exposes no stable post-item
/// metadata for checking the submitted range or flags, so a mismatch can
/// produce plausible but incorrect fill geometry.
pub unsafe fn decorate_slider_f32(
    frame: &mut Frame<'_>,
    style: &SliderStyle,
    min: f32,
    max: f32,
    value: &mut f32,
    widget: impl FnOnce(&mut f32) -> bool,
) -> bool {
    let (changed, _) = item_paint(
        frame,
        Decorator::Slider,
        || {
            let changed = widget(value);
            (changed, *value)
        },
        |(_, post_value), state, captured, canvas| {
            let frame_rect = captured.expect("slider frame captured before submission");
            let imgui_style = &*sys::igGetStyle();
            let io = &*sys::igGetIO();
            let anatomy = slider_anatomy(
                frame_rect,
                min,
                max,
                *post_value,
                imgui_style.GrabMinSize,
                io.DisplayFramebufferScale.x,
            );
            debug_assert!(
                rect_contains(item_rect(state), frame_rect),
                "Slider frame {frame_rect:?} must be contained by item {:?}",
                item_rect(state)
            );
            let visual_state = slider_visual_state(state);
            paint_slider(canvas, anatomy, style, state, visual_state);
        },
    );
    changed
}

/// Decorate one standard preview-and-arrow Combo and its popup contents.
///
/// paints frame and arrow-region background.
/// ImGui owns: preview text, arrow glyph, label, popup contents, navigation.
///
/// Suppressed parent-frame colors are restored before `contents` runs. The
/// Combo token remains alive for that closure, then is dropped internally so
/// Dear ImGui restores the parent window and last-item state before painting.
///
/// # Safety
/// A live current ImGui context/window/frame and unsplit parent draw list are required.
///
/// # Correctness
/// The closure must submit exactly one standard Combo. No-preview, no-arrow,
/// and custom-preview variants are outside this prototype.
pub unsafe fn decorate_combo<R, T>(
    frame: &mut Frame<'_>,
    style: &ComboStyle,
    begin: impl FnOnce() -> Option<R>,
    contents: impl FnOnce(&R) -> T,
) -> Option<T> {
    debug_assert_eq!(
        std::ffi::CStr::from_ptr(sys::igGetVersion()).to_str().ok(),
        Some(ANATOMY_IMGUI_VERSION),
        "{ANATOMY_COMPATIBILITY}"
    );
    let draw_list = sys::igGetWindowDrawList();
    let frame_rect = Decorator::Combo
        .capture_chrome()
        .expect("combo frame captured before submission");
    let channels = ChannelSplitGuard::split(draw_list);
    let colors = StyleColorGuard::push(Decorator::Combo.suppress_cols());

    let token = begin();
    drop(colors);
    let popup_open = token.is_some();
    let result = token.as_ref().map(contents);
    drop(token);

    // EndCombo restores the parent window and its backed-up LastItemData.
    let state = capture_item_state();
    debug_assert!(
        rect_contains(item_rect(&state), frame_rect),
        "Combo frame {frame_rect:?} must be contained by item {:?}",
        item_rect(&state)
    );
    let anatomy = combo_anatomy(frame_rect);
    let visual_state = combo_visual_state(&state, popup_open);
    let slot = combo_state_color_slot(visual_state);

    channels.background();
    {
        let mut canvas = frame.canvas(draw_list);
        let WidgetAnatomy::Combo { frame, arrow, .. } = anatomy else {
            unreachable!("combo painter requires combo anatomy")
        };
        paint_material_slot(&mut canvas, frame, &style.frame, slot, state.style_alpha);
        paint_material_slot(
            &mut canvas,
            arrow,
            &style.arrow_region,
            slot,
            state.style_alpha,
        );
    }
    channels.merge();
    result
}

/// Decorate one unframed, span-available-width stock TreeNode row.
/// Parent channels are merged before children are drawn through the token.
///
/// paints row and disclosure-slot background.
/// ImGui owns: disclosure arrow glyph, label, indentation, navigation.
///
/// # Safety
/// A live current ImGui context/window/frame and unsplit current draw list are required.
///
/// # Correctness
/// The closure must submit exactly one matching TreeNode. `selected` and `leaf`
/// must match its flags; non-leaves must use `SPAN_AVAIL_WIDTH | OPEN_ON_ARROW`,
/// while leaves must also use `LEAF | NO_TREE_PUSH_ON_OPEN`.
pub unsafe fn decorate_tree_node<R>(
    frame: &mut Frame<'_>,
    style: &TreeStyle,
    selected: bool,
    leaf: bool,
    widget: impl FnOnce() -> Option<R>,
) -> Option<R> {
    item_paint(
        frame,
        Decorator::Tree,
        widget,
        |result, state, _, canvas| {
            let anatomy = tree_anatomy(item_rect(state), leaf, sys::igGetTreeNodeToLabelSpacing());
            let visual_state = tree_visual_state(state, selected, result.is_some());
            let slot = tree_state_color_slot(visual_state);
            let WidgetAnatomy::Tree { row, disclosure } = anatomy else {
                unreachable!("tree painter requires tree anatomy")
            };
            paint_material_slot(canvas, row, &style.row, slot, state.style_alpha);
            if let Some(disclosure) = disclosure {
                paint_material_slot(
                    canvas,
                    disclosure,
                    &style.disclosure,
                    slot,
                    state.style_alpha,
                );
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgba;
    use std::ffi::CStr;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Mutex;

    const BASE: Color = 0x4000_0001;
    const HOVER: Color = 0x8000_0002;
    const ACTIVE: Color = 0xff00_0003;
    static IMGUI_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn state(hovered: bool, active: bool) -> ItemState {
        ItemState {
            min: [0.0, 0.0],
            max: [100.0, 20.0],
            hovered,
            active,
            focused: false,
            style_alpha: 1.0,
        }
    }

    fn focused_state(hovered: bool, active: bool, focused: bool) -> ItemState {
        ItemState {
            focused,
            ..state(hovered, active)
        }
    }

    #[test]
    fn bundled_imgui_matches_anatomy_compatibility() {
        let actual = unsafe { CStr::from_ptr(sys::igGetVersion()) };
        assert_eq!(actual.to_str().unwrap(), ANATOMY_IMGUI_VERSION);
        assert!(ANATOMY_COMPATIBILITY.contains(ANATOMY_IMGUI_VERSION));
    }

    #[test]
    fn state_colors_resolve_base_hover_and_active() {
        let colors = StateColors {
            base: BASE,
            hover: HOVER,
            active: ACTIVE,
        };
        assert_eq!(colors.for_state(&state(false, false)), BASE);
        assert_eq!(colors.for_state(&state(true, false)), HOVER);
        assert_eq!(colors.for_state(&state(true, true)), ACTIVE);
    }

    #[test]
    fn selectable_priority_is_pressed_selected_hovered_idle() {
        assert_eq!(
            selectable_visual_state(&state(true, true), true),
            SelectableVisualState::Pressed
        );
        assert_eq!(
            selectable_visual_state(&state(true, false), true),
            SelectableVisualState::Selected
        );
        assert_eq!(
            selectable_visual_state(&state(true, false), false),
            SelectableVisualState::Hovered
        );
        assert_eq!(
            selectable_visual_state(&state(false, false), false),
            SelectableVisualState::Idle
        );
    }

    #[test]
    fn selected_selectable_stays_active_colored_and_darkens_when_pressed() {
        let selected = rgba(100, 120, 140, 255);
        let material = Material {
            radius: 0.0,
            fill: StateColors {
                base: BASE,
                hover: HOVER,
                active: selected,
            },
            border: Border {
                thickness: 0.0,
                color: 0,
            },
            shadow: None,
        };
        assert_eq!(
            selectable_fill(&material, &state(false, false), true),
            selected
        );
        assert_eq!(
            selectable_fill(&material, &state(true, false), true),
            selected
        );
        assert_eq!(
            selectable_fill(&material, &state(true, true), true),
            shade_color(selected, 0.12)
        );
        assert_eq!(
            selectable_fill(&material, &state(true, false), false),
            HOVER
        );
    }

    #[test]
    fn slider_visual_state_priority_is_adjusting_focused_hovered_idle() {
        assert_eq!(
            slider_visual_state(&focused_state(true, true, true)),
            SliderVisualState::Adjusting
        );
        assert_eq!(
            slider_visual_state(&focused_state(true, false, true)),
            SliderVisualState::Focused
        );
        assert_eq!(
            slider_visual_state(&focused_state(true, false, false)),
            SliderVisualState::Hovered
        );
        assert_eq!(
            slider_visual_state(&focused_state(false, false, false)),
            SliderVisualState::Idle
        );
    }

    #[test]
    fn combo_visual_state_priority_is_open_pressed_focused_hovered_idle() {
        assert_eq!(
            combo_visual_state(&focused_state(true, true, true), true),
            ComboVisualState::Open
        );
        assert_eq!(
            combo_visual_state(&focused_state(true, true, true), false),
            ComboVisualState::Pressed
        );
        assert_eq!(
            combo_visual_state(&focused_state(true, false, true), false),
            ComboVisualState::Focused
        );
        assert_eq!(
            combo_visual_state(&focused_state(true, false, false), false),
            ComboVisualState::Hovered
        );
        assert_eq!(
            combo_visual_state(&focused_state(false, false, false), false),
            ComboVisualState::Idle
        );
    }

    #[test]
    fn tree_visual_state_priority_is_pressed_selected_focused_hovered_open_idle() {
        assert_eq!(
            tree_visual_state(&focused_state(true, true, true), true, true),
            TreeVisualState::Pressed
        );
        assert_eq!(
            tree_visual_state(&focused_state(true, false, true), true, true),
            TreeVisualState::Selected
        );
        assert_eq!(
            tree_visual_state(&focused_state(true, false, true), false, true),
            TreeVisualState::Focused
        );
        // The regression this order fixes: hovering an expanded row must show
        // hover feedback — Open no longer suppresses it.
        assert_eq!(
            tree_visual_state(&focused_state(true, false, false), false, true),
            TreeVisualState::Hovered
        );
        assert_eq!(
            tree_visual_state(&focused_state(false, false, false), false, true),
            TreeVisualState::Open
        );
        assert_eq!(
            tree_visual_state(&focused_state(false, false, false), false, false),
            TreeVisualState::Idle
        );
    }

    #[test]
    fn slider_visual_states_map_to_material_slots() {
        assert_eq!(
            slider_state_color_slot(SliderVisualState::Idle),
            StateColorSlot::Base
        );
        assert_eq!(
            slider_state_color_slot(SliderVisualState::Hovered),
            StateColorSlot::Hover
        );
        assert_eq!(
            slider_state_color_slot(SliderVisualState::Focused),
            StateColorSlot::Hover
        );
        assert_eq!(
            slider_state_color_slot(SliderVisualState::Adjusting),
            StateColorSlot::Active
        );
    }

    #[test]
    fn combo_visual_states_map_to_material_slots() {
        assert_eq!(
            combo_state_color_slot(ComboVisualState::Idle),
            StateColorSlot::Base
        );
        assert_eq!(
            combo_state_color_slot(ComboVisualState::Hovered),
            StateColorSlot::Hover
        );
        assert_eq!(
            combo_state_color_slot(ComboVisualState::Focused),
            StateColorSlot::Hover
        );
        assert_eq!(
            combo_state_color_slot(ComboVisualState::Pressed),
            StateColorSlot::Active
        );
        assert_eq!(
            combo_state_color_slot(ComboVisualState::Open),
            StateColorSlot::Active
        );
    }

    #[test]
    fn tree_visual_states_map_to_material_slots() {
        assert_eq!(
            tree_state_color_slot(TreeVisualState::Idle),
            StateColorSlot::Base
        );
        assert_eq!(
            tree_state_color_slot(TreeVisualState::Hovered),
            StateColorSlot::Hover
        );
        assert_eq!(
            tree_state_color_slot(TreeVisualState::Focused),
            StateColorSlot::Hover
        );
        assert_eq!(
            tree_state_color_slot(TreeVisualState::Pressed),
            StateColorSlot::Active
        );
        assert_eq!(
            tree_state_color_slot(TreeVisualState::Selected),
            StateColorSlot::Active
        );
        assert_eq!(
            tree_state_color_slot(TreeVisualState::Open),
            StateColorSlot::Base
        );
    }

    #[test]
    fn slider_anatomy_handles_min_mid_max_reversed_and_degenerate_ranges() {
        let frame = rect(0.0, 0.0, 100.0, 20.0);
        let center = |anatomy| match anatomy {
            WidgetAnatomy::Slider { grab, .. } => (grab.min.x + grab.max.x) * 0.5,
            _ => unreachable!(),
        };
        assert!(center(slider_anatomy(frame, 0.0, 1.0, 0.0, 10.0, 1.0)) < 10.0);
        assert_eq!(
            center(slider_anatomy(frame, 0.0, 1.0, 0.5, 10.0, 1.0)),
            50.0
        );
        assert!(center(slider_anatomy(frame, 0.0, 1.0, 1.0, 10.0, 1.0)) > 90.0);
        assert!(center(slider_anatomy(frame, 1.0, 0.0, 1.0, 10.0, 1.0)) < 10.0);
        assert!(center(slider_anatomy(frame, 1.0, 1.0, 1.0, 10.0, 1.0)).is_finite());
    }

    #[test]
    fn slider_anatomy_scales_with_logical_style_metrics() {
        let base = slider_anatomy(rect(0.0, 0.0, 100.0, 20.0), 0.0, 1.0, 0.5, 10.0, 1.0);
        let scaled = slider_anatomy(rect(0.0, 0.0, 200.0, 40.0), 0.0, 1.0, 0.5, 20.0, 1.0);
        let (base_grab, scaled_grab) = match (base, scaled) {
            (WidgetAnatomy::Slider { grab: a, .. }, WidgetAnatomy::Slider { grab: b, .. }) => {
                (a, b)
            }
            _ => unreachable!(),
        };
        assert_eq!(rect_width(scaled_grab), rect_width(base_grab) * 2.0);
        assert_eq!((scaled_grab.min.x + scaled_grab.max.x) * 0.5, 100.0);
    }

    #[test]
    fn slider_grab_padding_ignores_framebuffer_scale() {
        let frame = rect(0.0, 0.0, 100.0, 20.0);
        let grab = |scale| match slider_anatomy(frame, 0.0, 1.0, 0.0, 10.0, scale) {
            WidgetAnatomy::Slider { grab, .. } => grab,
            _ => unreachable!(),
        };
        assert_eq!(grab(1.0), grab(1.5));
        assert_eq!(grab(1.0), grab(2.0));
    }

    #[test]
    fn slider_track_minimum_uses_framebuffer_scale_only() {
        let frame = rect(0.0, 0.0, 100.0, 4.0);
        let height = |scale| match slider_anatomy(frame, 0.0, 1.0, 0.5, 2.0, scale) {
            WidgetAnatomy::Slider { track, .. } => rect_height(track),
            _ => unreachable!(),
        };
        assert_eq!(height(1.0), 2.0);
        assert!((height(2.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn combo_and_tree_anatomy_partition_without_escaping_the_item() {
        let frame = rect(2.0, 3.0, 102.0, 23.0);
        match combo_anatomy(frame) {
            WidgetAnatomy::Combo { preview, arrow, .. } => {
                assert_eq!(preview.max.x, arrow.min.x);
                assert_eq!(rect_width(arrow), 20.0);
                assert!(rect_contains(frame, preview));
                assert!(rect_contains(frame, arrow));
            }
            _ => unreachable!(),
        }
        match tree_anatomy(frame, false, 18.0) {
            WidgetAnatomy::Tree {
                disclosure: Some(disclosure),
                ..
            } => assert!(rect_contains(frame, disclosure)),
            _ => unreachable!(),
        }
        assert!(matches!(
            tree_anatomy(frame, true, 18.0),
            WidgetAnatomy::Tree {
                disclosure: None,
                ..
            }
        ));
    }

    #[test]
    fn style_alpha_is_applied_once_and_clamped() {
        let color = rgba(1, 2, 3, 200);
        assert_eq!(alpha_u8(with_style_alpha(color, 1.0)), 200);
        assert_eq!(alpha_u8(with_style_alpha(color, 0.5)), 100);
        assert_eq!(alpha_u8(with_style_alpha(color, 0.25)), 50);
        assert_eq!(alpha_u8(with_style_alpha(color, 0.0)), 0);
        assert_eq!(alpha_u8(with_style_alpha(color, 2.0)), 200);
    }

    #[test]
    fn decorators_suppress_the_expected_color_families() {
        assert_eq!(Decorator::Button.suppress_cols(), BUTTON_COLS);
        assert_eq!(Decorator::Selectable.suppress_cols(), HEADER_COLS);
        assert_eq!(Decorator::Checkbox.suppress_cols(), FRAME_COLS);
        assert_eq!(Decorator::InputText.suppress_cols(), FRAME_COLS);
        assert_eq!(Decorator::Slider.suppress_cols(), SLIDER_COLS);
        assert_eq!(Decorator::Combo.suppress_cols(), COMBO_COLS);
        assert_eq!(Decorator::Tree.suppress_cols(), HEADER_COLS);
    }

    #[test]
    fn finite_contained_geometry_guards_still_reject_bad_rects() {
        let outer = rect(0.0, 0.0, 100.0, 20.0);
        assert!(rect_is_valid(outer));
        assert!(rect_contains(outer, rect(2.0, 2.0, 20.0, 18.0)));
        assert!(!rect_contains(outer, rect(2.0, 2.0, 120.0, 18.0)));
        assert!(!rect_is_valid(rect(f32::NAN, 0.0, 10.0, 10.0)));
        assert!(!rect_is_valid(rect(10.0, 0.0, 5.0, 10.0)));
    }

    #[test]
    fn decoration_preserves_last_item_queries() {
        let _context_lock = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);
        context.io_mut().display_size = [640.0, 480.0];
        context.io_mut().delta_time = 1.0 / 60.0;
        context.fonts().build_rgba32_texture();

        let material = Material {
            radius: 2.0,
            fill: StateColors {
                base: rgba(20, 30, 40, 255),
                hover: rgba(30, 40, 50, 255),
                active: rgba(40, 50, 60, 255),
            },
            border: Border {
                thickness: 1.0,
                color: rgba(255, 255, 255, 30),
            },
            shadow: None,
        };
        let mut painter = crate::Painter::new();

        // Establish the window and an up state before the active frame.
        context.io_mut().add_mouse_pos_event([-100.0, -100.0]);
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, false);
        let mut button_center = [0.0, 0.0];
        {
            let ui = context.frame();
            ui.window("last-item contract")
                .position([0.0, 0.0], imgui::Condition::Always)
                .size([300.0, 120.0], imgui::Condition::Always)
                .title_bar(false)
                .movable(false)
                .build(|| {
                    ui.button("Contract button");
                    let min = ui.item_rect_min();
                    let max = ui.item_rect_max();
                    button_center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
                });
            context.render();
        }

        // 1.91's input queue may preserve move/down ordering across frames;
        // establish hover before submitting the press event.
        context.io_mut().add_mouse_pos_event(button_center);
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, false);
        {
            let ui = context.frame();
            ui.window("last-item contract")
                .position([0.0, 0.0], imgui::Condition::Always)
                .size([300.0, 120.0], imgui::Condition::Always)
                .title_bar(false)
                .movable(false)
                .build(|| {
                    ui.button("Contract button");
                });
            context.render();
        }

        context.io_mut().add_mouse_pos_event(button_center);
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, true);
        let ui = context.frame();
        let mut frame = painter.begin_frame();
        ui.window("last-item contract")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size([300.0, 120.0], imgui::Condition::Always)
            .title_bar(false)
            .movable(false)
            .build(|| {
                let mut inside_id = 0;
                let mut inside_min = sys::ImVec2 { x: 0.0, y: 0.0 };
                let mut inside_max = sys::ImVec2 { x: 0.0, y: 0.0 };
                let mut inside_hovered = false;
                let mut inside_active = false;
                unsafe {
                    decorate_button(&mut frame, &material, || {
                        let clicked = ui.button("Contract button");
                        inside_id = sys::igGetItemID();
                        sys::igGetItemRectMin(&mut inside_min);
                        sys::igGetItemRectMax(&mut inside_max);
                        inside_hovered = sys::igIsItemHovered(0);
                        inside_active = sys::igIsItemActive();
                        clicked
                    });

                    assert_eq!(sys::igGetItemID(), inside_id);
                    let mut after_min = sys::ImVec2 { x: 0.0, y: 0.0 };
                    let mut after_max = sys::ImVec2 { x: 0.0, y: 0.0 };
                    sys::igGetItemRectMin(&mut after_min);
                    sys::igGetItemRectMax(&mut after_max);
                    assert_eq!([after_min.x, after_min.y], [inside_min.x, inside_min.y]);
                    assert_eq!([after_max.x, after_max.y], [inside_max.x, inside_max.y]);
                    assert_eq!(sys::igIsItemHovered(0), inside_hovered);
                    assert_eq!(sys::igIsItemActive(), inside_active);
                    assert!(inside_hovered);
                    assert!(inside_active);
                }
            });
        drop(frame);
        context.render();
    }

    #[test]
    fn panic_restores_style_colors_and_draw_channels() {
        let _context_lock = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);
        context.io_mut().display_size = [640.0, 480.0];
        context.io_mut().delta_time = 1.0 / 60.0;
        context.fonts().build_rgba32_texture();

        let mut painter = crate::Painter::new();
        let ui = context.frame();
        let mut frame = painter.begin_frame();
        let material = Material {
            radius: 2.0,
            fill: StateColors {
                base: rgba(20, 30, 40, 255),
                hover: rgba(30, 40, 50, 255),
                active: rgba(40, 50, 60, 255),
            },
            border: Border {
                thickness: 1.0,
                color: rgba(255, 255, 255, 30),
            },
            shadow: None,
        };
        ui.window("panic cleanup").build(|| {
            let before = unsafe { *sys::igGetStyleColorVec4(sys::ImGuiCol_Button as _) };
            let panicked = catch_unwind(AssertUnwindSafe(|| unsafe {
                decorate_button(&mut frame, &material, || {
                    ui.button("panic");
                    panic!("intentional item-paint unwind")
                })
            }));
            assert!(panicked.is_err());
            let after = unsafe { *sys::igGetStyleColorVec4(sys::ImGuiCol_Button as _) };
            assert_eq!(before.x, after.x);
            assert_eq!(before.y, after.y);
            assert_eq!(before.z, after.z);
            assert_eq!(before.w, after.w);

            // A second split on the same draw list exercises the channel
            // guard: Dear ImGui asserts if the prior split was stranded.
            unsafe {
                decorate_button(&mut frame, &material, || ui.button("after panic"));
            }
        });
        drop(frame);
        context.render();
    }

    #[test]
    fn combo_restores_parent_colors_before_popup_contents() {
        let _context_lock = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);
        context.io_mut().display_size = [640.0, 480.0];
        context.io_mut().delta_time = 1.0 / 60.0;
        context.fonts().build_rgba32_texture();

        let material = Material {
            radius: 2.0,
            fill: StateColors {
                base: rgba(20, 30, 40, 255),
                hover: rgba(30, 40, 50, 255),
                active: rgba(40, 50, 60, 255),
            },
            border: Border {
                thickness: 1.0,
                color: rgba(255, 255, 255, 30),
            },
            shadow: None,
        };
        let combo_style = ComboStyle {
            frame: material,
            arrow_region: material,
        };
        let button_color = unsafe { *sys::igGetStyleColorVec4(sys::ImGuiCol_Button as _) };
        let mut painter = crate::Painter::new();
        let mut popup_contents_ran = false;

        // Resolve the current-version frame position instead of coupling the
        // interaction test to a historical window-padding/title-bar offset.
        context.io_mut().mouse_pos = [-100.0, -100.0];
        let mut combo_center = [0.0, 0.0];
        {
            let ui = context.frame();
            ui.window("combo lifecycle")
                .position([0.0, 0.0], imgui::Condition::Always)
                .size([400.0, 200.0], imgui::Condition::Always)
                .title_bar(false)
                .movable(false)
                .build(|| {
                    drop(ui.begin_combo("Mode", "Classic"));
                    let min = ui.item_rect_min();
                    let max = ui.item_rect_max();
                    combo_center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
                });
            context.render();
        }

        // Default ButtonBehavior opens on release. Three frames establish an
        // up -> down -> up transition over the fixed Combo frame.
        for mouse_down in [false, true, false] {
            context.io_mut().mouse_pos = combo_center;
            context.io_mut().mouse_down[0] = mouse_down;
            let ui = context.frame();
            let mut frame = painter.begin_frame();
            ui.window("combo lifecycle")
                .position([0.0, 0.0], imgui::Condition::Always)
                .size([400.0, 200.0], imgui::Condition::Always)
                .title_bar(false)
                .movable(false)
                .build(|| unsafe {
                    decorate_combo(
                        &mut frame,
                        &combo_style,
                        || ui.begin_combo("Mode", "Classic"),
                        |_token| {
                            popup_contents_ran = true;
                            let current = *sys::igGetStyleColorVec4(sys::ImGuiCol_Button as _);
                            assert_eq!(current.x, button_color.x);
                            assert_eq!(current.y, button_color.y);
                            assert_eq!(current.z, button_color.z);
                            assert_eq!(current.w, button_color.w);
                            ui.button("Popup button");
                            let mut input = String::new();
                            ui.input_text("Popup input", &mut input).build();
                        },
                    );
                });
            drop(frame);
            context.render();
        }

        assert!(
            popup_contents_ran,
            "the simulated click must open the Combo"
        );
    }
}
