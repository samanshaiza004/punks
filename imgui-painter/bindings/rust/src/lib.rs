//! Rust bindings for imgui-painter. This crate *is* the Rust adapter (see
//! the design doc's core/adapter split): it links the C++ core (compiled by
//! `build.rs` via `cc`, matching how `imgui-sys` compiles cimgui) and copies
//! the core's output mesh into a live `ImDrawList` through `imgui_sys`'s
//! `PrimReserve`/`PrimWriteVtx`/`PrimWriteIdx` (see [`adapter`]) — never by
//! touching `ImDrawList`'s internal buffers directly.
//!
//! [`Session`] is the safe wrapper over the raw `ip_ctx*` FFI calls.

pub mod adapter;
mod item_paint;

pub use item_paint::{
    decorate_button, decorate_checkbox, decorate_combo, decorate_input_text, decorate_selectable,
    decorate_slider_f32, decorate_tree_node, Material, StateColors,
};

mod ffi {
    #![allow(non_camel_case_types)]
    use std::os::raw::c_int;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ip_vec2 {
        pub x: f32,
        pub y: f32,
    }

    pub type ip_color = u32;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ip_rect {
        pub min: ip_vec2,
        pub max: ip_vec2,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ip_vertex {
        pub pos: ip_vec2,
        pub uv: ip_vec2,
        pub col: ip_color,
    }

    #[repr(C)]
    pub struct ip_mesh {
        pub vtx: *const ip_vertex,
        pub vtx_count: c_int,
        pub idx: *const u16,
        pub idx_count: c_int,
    }

    #[repr(C)]
    pub struct ip_ctx {
        _private: [u8; 0],
    }

    /// Matches `ip_gradient_mode`'s C values exactly (a plain C enum's
    /// underlying type is `int`, i.e. `i32`, on every platform this builds
    /// for).
    #[repr(i32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ip_gradient_mode {
        Linear = 0,
        Radial = 1,
        Angular = 2,
        Diamond = 3,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ip_color_stop {
        pub t: f32,
        pub color: ip_color,
    }

    #[repr(C)]
    pub struct ip_gradient {
        pub mode: ip_gradient_mode,
        pub from: ip_vec2,
        pub to: ip_vec2,
        pub stops: *const ip_color_stop,
        pub stop_count: c_int,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ip_shadow {
        pub offset: ip_vec2,
        pub blur: f32,
        pub spread: f32,
        pub color: ip_color,
        pub inset: bool,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ip_border {
        pub thickness: f32,
        pub color: ip_color,
    }

    extern "C" {
        pub fn ip_version() -> i32;
        pub fn ip_ctx_create() -> *mut ip_ctx;
        pub fn ip_ctx_destroy(ctx: *mut ip_ctx);
        pub fn ip_begin(ctx: *mut ip_ctx, white_pixel_uv: ip_vec2);
        pub fn ip_set_pixel_scale(ctx: *mut ip_ctx, scale: f32);
        pub fn ip_rounded_rect(ctx: *mut ip_ctx, rect: ip_rect, radius: f32);
        pub fn ip_fill_color(ctx: *mut ip_ctx, color: ip_color);
        pub fn ip_fill_gradient(ctx: *mut ip_ctx, gradient: *const ip_gradient);
        pub fn ip_fill_band_color(ctx: *mut ip_ctx, y0: f32, y1: f32, color: ip_color);
        pub fn ip_fill_band_gradient(
            ctx: *mut ip_ctx,
            y0: f32,
            y1: f32,
            gradient: *const ip_gradient,
        );
        pub fn ip_line(ctx: *mut ip_ctx, a: ip_vec2, b: ip_vec2, thickness: f32, color: ip_color);
        pub fn ip_add_shadow(ctx: *mut ip_ctx, shadow: *const ip_shadow);
        pub fn ip_add_border(ctx: *mut ip_ctx, border: *const ip_border);
        pub fn ip_add_border_inset(ctx: *mut ip_ctx, inset: f32, border: *const ip_border);
        pub fn ip_end(ctx: *mut ip_ctx) -> ip_mesh;
    }
}

/// The C ABI version imgui-painter was built against.
pub fn version() -> i32 {
    unsafe { ffi::ip_version() }
}

/// A 2D point, in whatever coordinate space the caller is drawing in
/// (screen-space pixels when painting into a real `ImDrawList`).
pub type Vec2 = ffi::ip_vec2;

/// Packed RGBA, one byte per channel, R in the lowest byte — matches Dear
/// ImGui's `ImU32` packing, so it can be handed to a real draw list as-is.
pub type Color = u32;

/// Pack R, G, B, A into a [`Color`]. This is the whole point of this
/// function existing: hand-packed hex literals for this layout are easy to
/// get backwards (R is the *lowest* byte here, not the ARGB convention many
/// readers expect from a glance at a hex literal) — a bug that bit this
/// crate's own tests once already. Prefer this over writing the hex by
/// hand.
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

/// One tessellated vertex. Field order matches `ImDrawVert` (pos, uv, col)
/// so [`adapter::paint_to_draw_list`]'s copy is a straight field copy.
pub type Vertex = ffi::ip_vertex;

/// An owned copy of one paint session's output — safe to hold onto after the
/// [`Session`] that produced it is gone (unlike the raw `ip_mesh`, whose
/// buffers live only until the next `ip_begin`/`ip_ctx_destroy`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// Which axis a [`Gradient`]'s `from`/`to` describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientMode {
    /// Color is `stops[0]` at `from`, `stops[last]` at `to`, interpolated
    /// by projecting each point onto the `from`->`to` axis.
    Linear,
    /// `from` is the center, `to` sets the radius (its distance from
    /// `from`); color is `stops[0]` at the center, `stops[last]` at that
    /// radius and beyond.
    Radial,
    /// Sweep/conic gradient: color sweeps around `from`, starting in
    /// `to`'s direction. Has an unavoidable hard seam where the sweep
    /// wraps back to its start — see the C header's `IP_GRADIENT_ANGULAR`
    /// doc comment for the known (documented, not fixed) artifact there.
    Angular,
    /// Concentric diamond iso-lines, scaled per-axis by the `from`->`to`
    /// box so `to` still means "this is the t = 1 edge".
    Diamond,
}

impl From<GradientMode> for ffi::ip_gradient_mode {
    fn from(m: GradientMode) -> Self {
        match m {
            GradientMode::Linear => ffi::ip_gradient_mode::Linear,
            GradientMode::Radial => ffi::ip_gradient_mode::Radial,
            GradientMode::Angular => ffi::ip_gradient_mode::Angular,
            GradientMode::Diamond => ffi::ip_gradient_mode::Diamond,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStop {
    /// Position along the gradient. Stops must be given in ascending `t`
    /// order — the core assumes it and doesn't sort.
    pub t: f32,
    pub color: Color,
}

/// A multi-stop gradient. What `from`/`to` mean depends on [`mode`](Self::mode)
/// — see [`GradientMode`]'s per-variant docs.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub mode: GradientMode,
    pub from: Vec2,
    pub to: Vec2,
    pub stops: Vec<ColorStop>,
}

/// A soft shadow around or inside the current shape. `blur <= 0.0` draws a
/// single hard-edged ring at exactly `spread`, with no falloff.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub offset: Vec2,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub inset: bool,
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow {
            offset: Vec2 { x: 0.0, y: 0.0 },
            blur: 12.0,
            spread: 0.0,
            color: rgba(0, 0, 0, 120), // semi-transparent black
            inset: false,
        }
    }
}

/// A stroke around the current shape's outline. Thickness below one device
/// pixel is compensated using [`Session::set_pixel_scale`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub thickness: f32,
    pub color: Color,
}

/// A paint session: set a shape, apply one or more effects to it, get the
/// resulting mesh. Owns one `ip_ctx` for its lifetime — cheap to create,
/// reasonable to keep around and reuse via [`Session::begin`] per frame.
pub struct Session {
    ctx: *mut ffi::ip_ctx,
}

impl Session {
    pub fn new() -> Self {
        let ctx = unsafe { ffi::ip_ctx_create() };
        assert!(!ctx.is_null(), "imgui-painter: ip_ctx_create returned null");
        Session { ctx }
    }

    /// Start a new session: clears any previous output and records the UV
    /// all emitted vertices sample (a solid-white texel in the host's font
    /// atlas — see the module docs on why fills need no separate untextured
    /// draw path).
    pub fn begin(&mut self, white_pixel_uv: Vec2) {
        unsafe { ffi::ip_begin(self.ctx, white_pixel_uv) };
    }

    /// Set logical-to-device pixel scale so direct `Session` consumers can
    /// render crisp hairline borders on scaled displays. The ImGui-aware
    /// [`Frame`] path samples this automatically.
    pub fn set_pixel_scale(&mut self, scale: f32) {
        unsafe { ffi::ip_set_pixel_scale(self.ctx, scale) };
    }

    /// Set the shape subsequent fill/shadow/border calls apply to.
    /// `radius <= 0.0` is a plain rectangle.
    pub fn rounded_rect(&mut self, rect: Rect, radius: f32) {
        let rect = ffi::ip_rect {
            min: rect.min,
            max: rect.max,
        };
        unsafe { ffi::ip_rounded_rect(self.ctx, rect, radius) };
    }

    /// Tessellate the current shape as a solid fill.
    pub fn fill_color(&mut self, color: Color) {
        unsafe { ffi::ip_fill_color(self.ctx, color) };
    }

    /// Tessellate the current shape with a multi-stop gradient fill. An
    /// empty `stops` is a no-op; a single stop fills solid with that color.
    pub fn fill_gradient(&mut self, gradient: &Gradient) {
        let stops: Vec<ffi::ip_color_stop> = gradient
            .stops
            .iter()
            .map(|s| ffi::ip_color_stop {
                t: s.t,
                color: s.color,
            })
            .collect();
        let raw = ffi::ip_gradient {
            mode: gradient.mode.into(),
            from: gradient.from,
            to: gradient.to,
            stops: stops.as_ptr(),
            stop_count: stops.len() as std::os::raw::c_int,
        };
        // SAFETY: `raw.stops` points into `stops`, which outlives this call.
        unsafe { ffi::ip_fill_gradient(self.ctx, &raw) };
    }

    /// Fill the current shape only within an absolute horizontal band. This
    /// is the Phase 7 layer stack and painter_demo primitive for gloss,
    /// highlights, shades, and bevels.
    pub fn fill_band_color(&mut self, y0: f32, y1: f32, color: Color) {
        unsafe { ffi::ip_fill_band_color(self.ctx, y0, y1, color) };
    }

    /// Gradient-fill the current shape only within an absolute horizontal
    /// band for the Phase 7 layer stack and painter_demo.
    pub fn fill_band_gradient(&mut self, y0: f32, y1: f32, gradient: &Gradient) {
        let stops: Vec<ffi::ip_color_stop> = gradient
            .stops
            .iter()
            .map(|s| ffi::ip_color_stop {
                t: s.t,
                color: s.color,
            })
            .collect();
        let raw = ffi::ip_gradient {
            mode: gradient.mode.into(),
            from: gradient.from,
            to: gradient.to,
            stops: stops.as_ptr(),
            stop_count: stops.len() as std::os::raw::c_int,
        };
        unsafe { ffi::ip_fill_band_gradient(self.ctx, y0, y1, &raw) };
    }

    /// Rasterize a soft shadow around the current shape and append it to
    /// the mesh. Paint order follows call order — call this before a
    /// `fill_*` to put the shadow behind the fill. Stackable: call it more
    /// than once for layered shadows.
    pub fn add_shadow(&mut self, shadow: &Shadow) {
        let raw = ffi::ip_shadow {
            offset: shadow.offset,
            blur: shadow.blur,
            spread: shadow.spread,
            color: shadow.color,
            inset: shadow.inset,
        };
        unsafe { ffi::ip_add_shadow(self.ctx, &raw) };
    }

    /// Stroke the current shape's outline.
    pub fn add_border(&mut self, border: &Border) {
        let raw = ffi::ip_border {
            thickness: border.thickness,
            color: border.color,
        };
        unsafe { ffi::ip_add_border(self.ctx, &raw) };
    }

    /// Stroke an outline inset from the current shape's outer edge. Stack
    /// borders by calling this with increasing inset values.
    pub fn add_border_inset(&mut self, inset: f32, border: &Border) {
        let raw = ffi::ip_border {
            thickness: border.thickness,
            color: border.color,
        };
        unsafe { ffi::ip_add_border_inset(self.ctx, inset, &raw) };
    }

    /// Append a straight `thickness`-px segment from `a` to `b`. Independent
    /// of the current shape (unlike `fill_*`), so it composes with fills in
    /// one accumulation. Not anti-aliased — pixel-exact for axis-aligned
    /// integer-width lines, a hard edge for diagonals (see the core's
    /// ponytail).
    pub fn line(&mut self, a: Vec2, b: Vec2, thickness: f32, color: Color) {
        unsafe { ffi::ip_line(self.ctx, a, b, thickness, color) };
    }

    /// Read the accumulated mesh as borrowed slices, without copying it into
    /// an owned [`Mesh`] — the zero-allocation read path [`Canvas`] submits
    /// through. The slices are valid only for `f`'s duration (they borrow the
    /// context's arena, invalidated by the next `begin`); this ends the
    /// current accumulation, same as [`end`](Self::end).
    pub fn with_raw_mesh<R>(&mut self, f: impl FnOnce(&[Vertex], &[u16]) -> R) -> R {
        let raw = unsafe { ffi::ip_end(self.ctx) };
        // SAFETY: ip_end returns pointers into the ctx's buffers, valid until
        // the next begin/Drop; the slices don't escape `f`.
        let vtx: &[Vertex] = if raw.vtx.is_null() || raw.vtx_count <= 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(raw.vtx, raw.vtx_count as usize) }
        };
        let idx: &[u16] = if raw.idx.is_null() || raw.idx_count <= 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(raw.idx, raw.idx_count as usize) }
        };
        f(vtx, idx)
    }

    /// Finish the session and copy out the accumulated mesh.
    pub fn end(&mut self) -> Mesh {
        let raw = unsafe { ffi::ip_end(self.ctx) };
        // SAFETY: ip_end returns pointers into buffers owned by `self.ctx`,
        // valid for at least this call's duration; copy them out immediately
        // since the next `begin`/`Drop` invalidates them.
        let vertices = if raw.vtx.is_null() || raw.vtx_count == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(raw.vtx, raw.vtx_count as usize) }.to_vec()
        };
        let indices = if raw.idx.is_null() || raw.idx_count == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(raw.idx, raw.idx_count as usize) }.to_vec()
        };
        Mesh { vertices, indices }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { ffi::ip_ctx_destroy(self.ctx) };
    }
}

/// Long-lived owner of a paint context, held in application/UI state and
/// reused every frame. Cheap to construct; the underlying `ip_ctx` is an
/// arena whose scratch buffers grow once and are cleared (not dropped) per
/// shape, so there's no steady-state allocation after warm-up.
///
/// Get a per-frame [`Frame`] from [`begin_frame`](Self::begin_frame);
/// the borrow it takes makes starting two frames on one `Painter`
/// concurrently a compile error, not a runtime rule.
pub struct Painter {
    session: Session,
}

impl Painter {
    pub fn new() -> Self {
        Painter {
            session: Session::new(),
        }
    }

    /// Begin a frame. Samples the host's white-pixel UV once (an active
    /// ImGui context must exist on this thread), then hands back a [`Frame`]
    /// scoped to this frame; open a [`Canvas`] per draw list from it.
    pub fn begin_frame(&mut self) -> Frame<'_> {
        let white_uv = unsafe { adapter::white_pixel_uv() };
        // Dear ImGui desktop backends use a uniform framebuffer scale. Keep
        // the C core scalar and sample X here; a non-uniform backend would
        // need a two-axis core API before it could promise exact hairlines.
        let pixel_scale = unsafe {
            let io = &*imgui_sys::igGetIO();
            io.DisplayFramebufferScale.x
        };
        Frame {
            painter: self,
            white_uv,
            pixel_scale,
        }
    }
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

/// One imgui frame's painting, borrowed from a [`Painter`] via
/// [`Painter::begin_frame`]. Hand out a [`Canvas`] per draw list with
/// [`canvas`](Self::canvas); each `Canvas` accumulates shapes and submits
/// once on `Drop`.
///
/// `Frame` is the only scope that spans *everything drawn this imgui frame*,
/// across every draw list. That makes it the reserved home for frame-level
/// behavior — primitive/vertex counters, allocation stats, validation
/// hooks, cross-canvas mesh batching, GPU-upload coordination — none of
/// which belong to a single `Canvas`. None of it is built yet (current hosts
/// draw into one window = one draw list, so there's no consumer); it lives in
/// `ip_frame_begin`/`ip_frame_end` as reserved hooks.
///
/// Invariant, total across the chain: a `Canvas` never outlives its `Frame`;
/// a `Frame` never outlives its `Painter`; the `Painter` owns all memory.
/// [`begin_frame`](Painter::begin_frame)'s `&mut Painter` borrow makes two
/// concurrent frames uncompilable; [`canvas`](Self::canvas)'s `&mut Frame`
/// borrow makes two concurrent canvases uncompilable — matching the arena's
/// single-accumulator reality, enforced by the borrow checker, not a rule.
pub struct Frame<'a> {
    painter: &'a mut Painter,
    white_uv: Vec2,
    pixel_scale: f32,
}

impl Frame<'_> {
    /// Open a [`Canvas`] targeting `dl`. The canvas accumulates every shape
    /// drawn on it into one mesh and submits it into `dl` when it drops.
    ///
    /// # Safety
    /// `dl` must be a valid, currently-active `ImDrawList*` (e.g.
    /// `imgui::sys::igGetWindowDrawList()`) that stays valid until the
    /// returned `Canvas` drops, on the thread owning the ImGui context —
    /// same contract as [`adapter::paint_raw`]. The `&mut self` borrow keeps
    /// only one canvas open at a time.
    pub unsafe fn canvas(&mut self, dl: *mut imgui_sys::ImDrawList) -> Canvas<'_> {
        self.painter.session.begin(self.white_uv);
        let pixel_scale = if self.pixel_scale.is_finite() && self.pixel_scale > 0.0 {
            self.pixel_scale
        } else {
            1.0
        };
        self.painter.session.set_pixel_scale(pixel_scale);
        Canvas {
            session: &mut self.painter.session,
            dl,
            pixel_scale,
        }
    }
}

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        // No-op today (see the type doc): the reserved home for frame-end
        // behavior — stats, validation, cross-canvas batch flush, GPU
        // upload. Per-canvas scratch clears on each `canvas()`; there's
        // nothing frame-level to reset yet. Exists so those futures don't
        // change any call site.
    }
}

/// Draws into exactly one [`ImDrawList`](imgui_sys::ImDrawList). Every shape
/// (`fill_rect`, `line`) accumulates into one mesh; the whole mesh submits
/// once, on `Drop` — so 512 waveform bars are one reserve + one copy, not
/// 512. Borrowed from a [`Frame`] via [`Frame::canvas`]; the app describes
/// geometry and never touches the draw list, `PrimReserve`, or a `Mesh`.
pub struct Canvas<'a> {
    session: &'a mut Session,
    dl: *mut imgui_sys::ImDrawList,
    pixel_scale: f32,
}

impl Canvas<'_> {
    /// One physical device pixel expressed in the current ImGui logical
    /// coordinate space. Useful for exact bevel strips and stacked hairlines.
    pub fn device_pixel(&self) -> f32 {
        1.0 / self.pixel_scale
    }

    /// Set the shape subsequent styling-depth operations apply to.
    pub fn rounded_rect(&mut self, rect: Rect, radius: f32) {
        self.session.rounded_rect(rect, radius);
    }

    /// Fill the current shape with one color.
    pub fn fill_color(&mut self, color: Color) {
        self.session.fill_color(color);
    }

    /// Fill the current shape with a multi-stop gradient.
    pub fn fill_gradient(&mut self, gradient: &Gradient) {
        self.session.fill_gradient(gradient);
    }

    /// Fill an absolute horizontal band clipped to the current shape.
    pub fn fill_band_color(&mut self, y0: f32, y1: f32, color: Color) {
        self.session.fill_band_color(y0, y1, color);
    }

    /// Gradient-fill an absolute horizontal band clipped to the current shape.
    pub fn fill_band_gradient(&mut self, y0: f32, y1: f32, gradient: &Gradient) {
        self.session.fill_band_gradient(y0, y1, gradient);
    }

    /// Add an outer or inset shadow to the current shape.
    pub fn add_shadow(&mut self, shadow: &Shadow) {
        self.session.add_shadow(shadow);
    }

    /// Stroke the current shape's outer outline.
    pub fn add_border(&mut self, border: &Border) {
        self.session.add_border(border);
    }

    /// Stroke an outline inset from the current shape's outer edge.
    pub fn add_border_inset(&mut self, inset: f32, border: &Border) {
        self.session.add_border_inset(inset, border);
    }

    /// A solid-filled (optionally rounded) rect. `radius <= 0.0` is a plain
    /// rectangle — pixel-identical to `ImDrawList::add_rect(...).filled(true)`.
    pub fn fill_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.rounded_rect(rect, radius);
        self.fill_color(color);
    }

    /// A straight `thickness`-px segment from `a` to `b`. Not anti-aliased —
    /// pixel-exact for axis-aligned integer-width lines (playhead, crosshair),
    /// a hard edge for diagonals (see [`Session::line`]).
    pub fn line(&mut self, a: Vec2, b: Vec2, thickness: f32, color: Color) {
        self.session.line(a, b, thickness, color);
    }
}

impl Drop for Canvas<'_> {
    fn drop(&mut self) {
        // Zero-copy submit: read the arena's accumulated mesh and write it
        // straight into the bound draw list — no owned `Mesh`, so no per-frame
        // allocation. `Drop` can't return `Result`, but PrimReserve/
        // PrimWriteVtx don't fail, so there's nothing to surface.
        let dl = self.dl;
        // SAFETY: `dl` upheld valid by the caller of `Frame::canvas` for this
        // Canvas's whole lifetime; the raw slices don't escape the closure.
        self.session
            .with_raw_mesh(|vtx, idx| unsafe { adapter::paint_raw(dl, vtx, idx) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uv() -> Vec2 {
        Vec2 { x: 0.5, y: 0.5 }
    }

    fn rect(min: (f32, f32), max: (f32, f32)) -> Rect {
        Rect {
            min: Vec2 { x: min.0, y: min.1 },
            max: Vec2 { x: max.0, y: max.1 },
        }
    }

    // Per-channel near-equality: atan2-based gradients (Angular) can land a
    // hair off an exact stop boundary (e.g. t == 0.49999997 instead of 0.5)
    // purely from floating-point rounding in the angle math, which the
    // truncating lerp in LerpColor then turns into an off-by-one channel
    // value. Comparing colors exactly at a stop boundary is testing float
    // rounding, not gradient correctness — this tolerance is the fix.
    fn color_close(actual: Color, expected: Color, tolerance: i32) -> bool {
        (0..4).all(|shift| {
            let a = ((actual >> (shift * 8)) & 0xFF) as i32;
            let e = ((expected >> (shift * 8)) & 0xFF) as i32;
            (a - e).abs() <= tolerance
        })
    }

    #[test]
    fn version_is_reachable_through_ffi() {
        assert_eq!(version(), 2);
    }

    #[test]
    fn rgba_packs_r_in_the_lowest_byte() {
        // Regression: an earlier hand-packed test literal got this backwards
        // (treated the top byte as R, ARGB-style) and silently passed
        // because the test it broke happened to compare against an equally
        // wrong expected value. Pin the byte order explicitly so that
        // mistake can't recur unnoticed.
        assert_eq!(rgba(0xAA, 0xBB, 0xCC, 0xDD), 0xDDCCBBAA);
        assert_eq!(rgba(0xFF, 0x00, 0x00, 0x00), 0x000000FF); // pure R, alpha 0
        assert_eq!(rgba(0x00, 0x00, 0x00, 0xFF), 0xFF000000); // opaque black
    }

    #[test]
    fn radius_zero_never_produces_nan_positions() {
        // Regression: AppendArc's segment-angle division (i / segments) was
        // 0.0 / 0.0 at radius <= 0 (segments == 0) before it was guarded,
        // producing NaN vertex positions that earlier tests didn't catch
        // because they only asserted vertex *counts*, not that the
        // positions were finite. Cover every effect at radius 0, not just
        // solid fill.
        let finite = |mesh: &Mesh| {
            mesh.vertices
                .iter()
                .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite())
        };
        let r = rect((0.0, 0.0), (50.0, 50.0));

        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(r, 0.0);
        s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF));
        assert!(finite(&s.end()));

        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(r, 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Linear,
            from: Vec2 { x: 0.0, y: 0.0 },
            to: Vec2 { x: 50.0, y: 0.0 },
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0xFF, 0x00, 0x00, 0xFF),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0x00, 0x00, 0xFF, 0xFF),
                },
            ],
        });
        assert!(finite(&s.end()));

        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(r, 0.0);
        s.add_shadow(&Shadow {
            offset: Vec2 { x: 0.0, y: 0.0 },
            blur: 8.0,
            spread: 4.0,
            color: rgba(0, 0, 0, 120),
            inset: false,
        });
        assert!(finite(&s.end()));

        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(r, 0.0);
        s.add_border(&Border {
            thickness: 2.0,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
        });
        assert!(finite(&s.end()));
    }

    #[test]
    fn solid_fill_produces_a_closed_triangle_fan() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (100.0, 50.0)), 8.0);
        s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF));
        let mesh = s.end();

        assert!(!mesh.vertices.is_empty());
        // Fan triangulation: 3 indices per triangle.
        assert_eq!(mesh.indices.len() % 3, 0);
        // Every index must address a real vertex.
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
        // Every vertex carries the UV set at begin() and the fill color.
        assert!(mesh.vertices.iter().all(|v| v.uv == uv()));
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.col == rgba(0xFF, 0xFF, 0xFF, 0xFF)));
    }

    #[test]
    fn no_shape_means_no_output() {
        let mut s = Session::new();
        s.begin(uv());
        s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF)); // no rounded_rect() call first
        let mesh = s.end();
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn begin_clears_the_previous_session() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (10.0, 10.0)), 0.0);
        s.fill_color(rgba(0xFF, 0x00, 0x00, 0xFF));
        assert!(!s.end().vertices.is_empty());

        s.begin(uv()); // no shape/fill this time
        let mesh = s.end();
        assert!(mesh.vertices.is_empty());
    }

    #[test]
    fn zero_radius_still_fills_a_plain_rect() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (20.0, 10.0)), 0.0);
        s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF));
        let mesh = s.end();
        // 4 outline points + 1 centroid.
        assert_eq!(mesh.vertices.len(), 5);
        assert_eq!(mesh.indices.len(), 4 * 3);
    }

    #[test]
    fn larger_radius_tessellates_more_vertices_than_smaller() {
        let mesh_of = |radius: f32| {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((0.0, 0.0), (200.0, 200.0)), radius);
            s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF));
            s.end()
        };
        let small = mesh_of(2.0);
        let large = mesh_of(64.0);
        assert!(large.vertices.len() > small.vertices.len());
    }

    #[test]
    fn segment_count_grows_sublinearly_with_radius() {
        // Regression for the error-bounded tessellation formula (replacing
        // a plain linear-in-radius heuristic): segment count should track
        // roughly sqrt(radius), not radius itself, so a 10x larger radius
        // must NOT produce anywhere near 10x the vertices. Use radii well
        // clear of both the small-radius floor and the large-radius
        // ceiling clamps so the sqrt relationship is actually visible (not
        // masked by clamping), on a rect big enough that the corner radius
        // itself is never the constraint.
        let mesh_of = |radius: f32| {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((0.0, 0.0), (600.0, 600.0)), radius);
            s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF));
            s.end()
        };
        let small = mesh_of(20.0);
        let large = mesh_of(200.0); // 10x the radius
        assert!(
            large.vertices.len() > small.vertices.len(),
            "must still grow"
        );
        let ratio = large.vertices.len() as f64 / small.vertices.len() as f64;
        assert!(
            ratio < 5.0,
            "expected sublinear (~sqrt) growth for a 10x radius increase, got {ratio}x vertices \
             ({} -> {})",
            small.vertices.len(),
            large.vertices.len()
        );
    }

    #[test]
    fn linear_gradient_endpoints_hit_exact_stop_colors() {
        // A plain (radius 0) rect with a horizontal axis exactly spanning
        // its x-extent: every left-edge vertex (x=0) projects to t=0, every
        // right-edge vertex (x=100) projects to t=1 — both are clamped to
        // the endpoint stop's color exactly, not interpolated.
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (100.0, 50.0)), 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Linear,
            from: Vec2 { x: 0.0, y: 25.0 },
            to: Vec2 { x: 100.0, y: 25.0 },
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0x11, 0x11, 0x11, 0x11),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0x99, 0x99, 0x99, 0x99),
                },
            ],
        });
        let mesh = s.end();

        assert!(mesh
            .vertices
            .iter()
            .filter(|v| v.pos.x == 0.0)
            .all(|v| v.col == rgba(0x11, 0x11, 0x11, 0x11)));
        assert!(mesh
            .vertices
            .iter()
            .filter(|v| v.pos.x == 100.0)
            .all(|v| v.col == rgba(0x99, 0x99, 0x99, 0x99)));
        // At least one vertex actually sits at each end (the test would be
        // vacuously true otherwise).
        assert!(mesh.vertices.iter().any(|v| v.pos.x == 0.0));
        assert!(mesh.vertices.iter().any(|v| v.pos.x == 100.0));
    }

    #[test]
    fn degenerate_gradient_axis_falls_back_to_first_stop() {
        // from == to: the axis has zero length, so GradientT can't divide by
        // it — every vertex must land on the first stop's color rather than
        // panicking or dividing by zero into garbage.
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (40.0, 40.0)), 4.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Linear,
            from: Vec2 { x: 20.0, y: 20.0 },
            to: Vec2 { x: 20.0, y: 20.0 },
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0xAA, 0xAA, 0xAA, 0xAA),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0xBB, 0xBB, 0xBB, 0xBB),
                },
            ],
        });
        let mesh = s.end();
        assert!(!mesh.vertices.is_empty());
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.col == rgba(0xAA, 0xAA, 0xAA, 0xAA)));
    }

    #[test]
    fn degenerate_axis_falls_back_to_first_stop_for_angular_and_diamond() {
        // Same degenerate contract as Linear/Radial (see the test above), now
        // for Angular (explicit from==to guard) and Diamond (per-axis
        // zero-extent guard) — must still land on stops[0], not
        // NaN/Inf/garbage.
        for mode in [GradientMode::Angular, GradientMode::Diamond] {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((0.0, 0.0), (40.0, 40.0)), 4.0);
            s.fill_gradient(&Gradient {
                mode,
                from: Vec2 { x: 20.0, y: 20.0 },
                to: Vec2 { x: 20.0, y: 20.0 },
                stops: vec![
                    ColorStop {
                        t: 0.0,
                        color: rgba(0xAA, 0xAA, 0xAA, 0xAA),
                    },
                    ColorStop {
                        t: 1.0,
                        color: rgba(0xBB, 0xBB, 0xBB, 0xBB),
                    },
                ],
            });
            let mesh = s.end();
            assert!(!mesh.vertices.is_empty());
            assert!(
                mesh.vertices
                    .iter()
                    .all(|v| v.col == rgba(0xAA, 0xAA, 0xAA, 0xAA)),
                "mode {mode:?} did not degenerate to the first stop"
            );
        }
    }

    #[test]
    fn angular_gradient_sweeps_from_the_to_direction() {
        // Axis points +x (to is directly right of from), so t == 0 exactly
        // in that direction; sweeping 180 degrees around lands at t == 0.5,
        // which atan2's own rounding lands a hair off (e.g. 0.49999997) —
        // exact enough to identify the vertex position but not exact enough
        // for a bit-exact color match, hence color_close below.
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((-50.0, -50.0), (50.0, 50.0)), 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Angular,
            from: Vec2 { x: 0.0, y: 0.0 },
            to: Vec2 { x: 1.0, y: 0.0 },
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0xFF, 0x00, 0x00, 0xFF),
                },
                ColorStop {
                    t: 0.5,
                    color: rgba(0x00, 0xFF, 0x00, 0xFF),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0x00, 0x00, 0xFF, 0xFF),
                },
            ],
        });
        let mesh = s.end();

        // The point straight in the `to` direction (+x from `from`) is the
        // sweep's t == 0 start.
        assert!(mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: 50.0, y: 0.0 } && v.col == rgba(0xFF, 0x00, 0x00, 0xFF)));
        // Directly opposite (-x) is exactly halfway around the sweep.
        assert!(mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: -50.0, y: 0.0 }
                && color_close(v.col, rgba(0x00, 0xFF, 0x00, 0xFF), 1)));
    }

    #[test]
    fn angular_gradient_seam_is_a_hard_wrap_not_a_blend() {
        // The documented seam artifact's *cause*: t == 0 and t just under 1
        // are geometrically adjacent (both right next to the `to`
        // direction) but land on opposite ends of the stop range — not a
        // bug in GradientT, just what "sweep gradient" means. Pin the
        // colors on both sides of the seam so a future change to the wrap
        // math would be caught here.
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((-50.0, -50.0), (50.0, 50.0)), 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Angular,
            from: Vec2 { x: 0.0, y: 0.0 },
            to: Vec2 { x: 1.0, y: 0.0 },
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0xFF, 0x00, 0x00, 0xFF),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0x00, 0x00, 0xFF, 0xFF),
                },
            ],
        });
        let mesh = s.end();
        // Exactly at the sweep start: t == 0.
        assert!(mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: 50.0, y: 0.0 } && v.col == rgba(0xFF, 0x00, 0x00, 0xFF)));
    }

    #[test]
    fn diamond_gradient_uses_per_axis_max_norm() {
        // from->to spans (10, 20): a point 10 right of `from` maxes out the
        // x-axis ratio (10/10 == 1.0) even though it hasn't moved in y at
        // all, and a point 20 down maxes out the y-axis ratio the same
        // way — both clamp to the last stop exactly, which is the "diamond"
        // (per-axis max, not Euclidean distance) shape's whole point.
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (10.0, 20.0)), 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Diamond,
            from: Vec2 { x: 0.0, y: 0.0 },
            to: Vec2 { x: 10.0, y: 20.0 },
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0xFF, 0x00, 0x00, 0xFF),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0x00, 0x00, 0xFF, 0xFF),
                },
            ],
        });
        let mesh = s.end();

        // (0, 0) is both `from` and a rect corner -> t == 0 exactly.
        assert!(mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: 0.0, y: 0.0 } && v.col == rgba(0xFF, 0x00, 0x00, 0xFF)));
        // Corner (10, 0): dx/ax = 10/10 = 1.0, dy/ay = 0/20 = 0 -> t = 1.0
        // (max, not a Euclidean blend of the two ratios).
        assert!(mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: 10.0, y: 0.0 } && v.col == rgba(0x00, 0x00, 0xFF, 0xFF)));
        // Corner (0, 20): dx/ax = 0, dy/ay = 20/20 = 1.0 -> t = 1.0 too,
        // reached via the *other* axis saturating instead.
        assert!(mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: 0.0, y: 20.0 } && v.col == rgba(0x00, 0x00, 0xFF, 0xFF)));
    }

    #[test]
    fn degenerate_zero_size_rect_does_not_panic() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((5.0, 5.0), (5.0, 5.0)), 10.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Radial,
            from: Vec2 { x: 5.0, y: 5.0 },
            to: Vec2 { x: 25.0, y: 5.0 },
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0xFF, 0x00, 0x00, 0x00),
                },
            ],
        });
        let mesh = s.end();
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
    }

    #[test]
    fn empty_stops_is_a_no_op() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (10.0, 10.0)), 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Linear,
            from: Vec2 { x: 0.0, y: 0.0 },
            to: Vec2 { x: 10.0, y: 0.0 },
            stops: vec![],
        });
        let mesh = s.end();
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn single_stop_gradient_fills_solid() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (10.0, 10.0)), 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Radial,
            from: Vec2 { x: 5.0, y: 5.0 },
            to: Vec2 { x: 5.0, y: 5.0 },
            stops: vec![ColorStop {
                t: 0.0,
                color: rgba(0x42, 0x42, 0x42, 0x42),
            }],
        });
        let mesh = s.end();
        assert!(!mesh.vertices.is_empty());
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.col == rgba(0x42, 0x42, 0x42, 0x42)));
    }

    #[test]
    fn gradient_fill_tessellates_more_vertices_than_solid_fill() {
        // Edge subdivision (needed so multi-stop gradients don't miss a
        // stop's slope change within one triangle) means a gradient fill's
        // mesh is strictly larger than a solid fill of the same shape.
        let mesh_of_solid = {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((0.0, 0.0), (100.0, 100.0)), 8.0);
            s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF));
            s.end()
        };
        let mesh_of_gradient = {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((0.0, 0.0), (100.0, 100.0)), 8.0);
            s.fill_gradient(&Gradient {
                mode: GradientMode::Linear,
                from: Vec2 { x: 0.0, y: 0.0 },
                to: Vec2 { x: 100.0, y: 0.0 },
                stops: vec![
                    ColorStop {
                        t: 0.0,
                        color: rgba(0xFF, 0x00, 0x00, 0xFF),
                    },
                    ColorStop {
                        t: 1.0,
                        color: rgba(0x00, 0x00, 0xFF, 0xFF),
                    },
                ],
            });
            s.end()
        };
        assert!(mesh_of_gradient.vertices.len() > mesh_of_solid.vertices.len());
    }

    #[test]
    fn radial_gradient_center_and_far_corner_hit_stop_colors() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (100.0, 100.0)), 0.0);
        s.fill_gradient(&Gradient {
            mode: GradientMode::Radial,
            from: Vec2 { x: 50.0, y: 50.0 },
            to: Vec2 { x: 50.0, y: 0.0 }, // radius 50
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0x11, 0x11, 0x11, 0x11),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0x99, 0x99, 0x99, 0x99),
                },
            ],
        });
        let mesh = s.end();
        // The centroid vertex sits at the rect's center, distance 0 from
        // `from` -> exactly the first stop's color.
        assert_eq!(mesh.vertices[0].pos, Vec2 { x: 50.0, y: 50.0 });
        assert_eq!(mesh.vertices[0].col, rgba(0x11, 0x11, 0x11, 0x11));
        // Every corner is ~70.7 px from center — past the 50px radius, so t
        // clamps to 1.0 and gets exactly the last stop's color.
        assert!(mesh
            .vertices
            .iter()
            .filter(|v| v.pos.x == 0.0 || v.pos.x == 100.0)
            .all(|v| v.col == rgba(0x99, 0x99, 0x99, 0x99)));
    }

    #[test]
    fn band_fill_clips_vertices_and_accepts_inverted_endpoints() {
        let mesh_of = |y0: f32, y1: f32| {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((10.0, 20.0), (90.0, 80.0)), 12.0);
            s.fill_band_color(y0, y1, rgba(0xFF, 0xFF, 0xFF, 0xFF));
            s.end()
        };
        let forward = mesh_of(35.0, 55.0);
        let inverted = mesh_of(55.0, 35.0);
        assert!(!forward.vertices.is_empty());
        assert!(forward
            .vertices
            .iter()
            .all(|v| v.pos.y >= 35.0 - 0.001 && v.pos.y <= 55.0 + 0.001));
        assert_eq!(forward, inverted);
    }

    #[test]
    fn band_outside_shape_is_empty_and_full_height_matches_fill_bounds() {
        let shape = rect((10.0, 20.0), (90.0, 80.0));
        let bounds = |mesh: &Mesh| {
            mesh.vertices.iter().fold(
                (
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ),
                |(min_x, min_y, max_x, max_y), v| {
                    (
                        min_x.min(v.pos.x),
                        min_y.min(v.pos.y),
                        max_x.max(v.pos.x),
                        max_y.max(v.pos.y),
                    )
                },
            )
        };

        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(shape, 12.0);
        s.fill_band_color(100.0, 120.0, rgba(0xFF, 0xFF, 0xFF, 0xFF));
        assert!(s.end().vertices.is_empty());

        let plain = {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(shape, 12.0);
            s.fill_color(rgba(0xFF, 0xFF, 0xFF, 0xFF));
            s.end()
        };
        let band = {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(shape, 12.0);
            s.fill_band_color(20.0, 80.0, rgba(0xFF, 0xFF, 0xFF, 0xFF));
            s.end()
        };
        assert_eq!(bounds(&plain), bounds(&band));
    }

    #[test]
    fn gradient_band_clips_to_the_requested_interval() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (40.0, 40.0)), 4.0);
        s.fill_band_gradient(
            8.0,
            12.0,
            &Gradient {
                mode: GradientMode::Linear,
                from: Vec2 { x: 0.0, y: 8.0 },
                to: Vec2 { x: 40.0, y: 8.0 },
                stops: vec![
                    ColorStop {
                        t: 0.0,
                        color: rgba(0xFF, 0, 0, 0xFF),
                    },
                    ColorStop {
                        t: 1.0,
                        color: rgba(0, 0, 0xFF, 0xFF),
                    },
                ],
            },
        );
        let mesh = s.end();
        assert!(!mesh.vertices.is_empty());
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.pos.y >= 8.0 - 0.001 && v.pos.y <= 12.0 + 0.001));
    }

    #[test]
    fn inset_shadow_stays_inside_and_hard_band_reaches_only_spread() {
        let shape = rect((10.0, 20.0), (90.0, 80.0));
        let spread = 6.0;
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(shape, 0.0);
        s.add_shadow(&Shadow {
            offset: Vec2 { x: 0.0, y: 0.0 },
            blur: 0.0,
            spread,
            color: rgba(0, 0, 0, 0xFF),
            inset: true,
        });
        let mesh = s.end();
        assert!(!mesh.vertices.is_empty());
        assert!(mesh.vertices.iter().all(|v| {
            v.pos.x >= shape.min.x - 0.5
                && v.pos.x <= shape.max.x + 0.5
                && v.pos.y >= shape.min.y - 0.5
                && v.pos.y <= shape.max.y + 0.5
        }));
        assert!(mesh.vertices.iter().all(|v| {
            let edge_distance = (v.pos.x - shape.min.x)
                .min(shape.max.x - v.pos.x)
                .min(v.pos.y - shape.min.y)
                .min(shape.max.y - v.pos.y);
            edge_distance <= spread + 0.001
        }));
    }

    #[test]
    fn blurred_offset_inset_shadow_stays_inside_shape() {
        let shape = rect((10.0, 20.0), (90.0, 80.0));
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(shape, 8.0);
        s.add_shadow(&Shadow {
            offset: Vec2 { x: 5.0, y: -3.0 },
            blur: 12.0,
            spread: 2.0,
            color: rgba(0, 0, 0, 0xFF),
            inset: true,
        });
        let mesh = s.end();
        assert!(!mesh.vertices.is_empty());
        assert!(mesh.vertices.iter().all(|v| {
            v.pos.x >= shape.min.x - 0.5
                && v.pos.x <= shape.max.x + 0.5
                && v.pos.y >= shape.min.y - 0.5
                && v.pos.y <= shape.max.y + 0.5
        }));
    }

    #[test]
    fn inset_shadow_without_shape_emits_nothing() {
        let mut s = Session::new();
        s.begin(uv());
        s.add_shadow(&Shadow {
            inset: true,
            ..Shadow::default()
        });
        assert!(s.end().vertices.is_empty());
    }

    #[test]
    fn hard_edged_shadow_is_a_single_uniform_ring() {
        // blur <= 0 collapses to one ring with no falloff (f == 0, so the
        // quadratic falloff is exactly 1.0) — every vertex keeps the
        // shadow's alpha unscaled.
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 0.0);
        s.add_shadow(&Shadow {
            offset: Vec2 { x: 0.0, y: 0.0 },
            blur: 0.0,
            spread: 10.0,
            color: rgba(0, 0, 0, 0xFF), // opaque black
            inset: false,
        });
        let mesh = s.end();
        assert!(!mesh.vertices.is_empty());
        assert!(mesh.vertices.iter().all(|v| v.col == rgba(0, 0, 0, 0xFF)));
    }

    #[test]
    fn blurred_shadow_produces_more_vertices_and_varied_alpha_than_hard_edged() {
        let mesh_of = |blur: f32| {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 4.0);
            s.add_shadow(&Shadow {
                offset: Vec2 { x: 0.0, y: 0.0 },
                blur,
                spread: 4.0,
                color: rgba(0, 0, 0, 0xFF), // opaque black
                inset: false,
            });
            s.end()
        };
        let hard = mesh_of(0.0);
        let blurred = mesh_of(20.0);
        // More rings -> more vertices than the single hard-edged ring.
        assert!(blurred.vertices.len() > hard.vertices.len());
        // And the rings carry genuinely different alphas (falloff), not one
        // flat color repeated.
        let distinct_alphas: std::collections::HashSet<u32> =
            blurred.vertices.iter().map(|v| v.col >> 24).collect();
        assert!(distinct_alphas.len() > 1);
    }

    #[test]
    fn shadow_offset_translates_the_ring() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 0.0);
        s.add_shadow(&Shadow {
            offset: Vec2 { x: 10.0, y: 20.0 },
            blur: 0.0,
            spread: 0.0,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            inset: false,
        });
        let mesh = s.end();
        // spread == 0 and blur == 0: the single ring is exactly the base
        // rect translated by `offset` — its top-left corner lands at
        // (0+10, 0+20), not at the base shape's own (0, 0).
        assert!(mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: 10.0, y: 20.0 }));
        assert!(!mesh
            .vertices
            .iter()
            .any(|v| v.pos == Vec2 { x: 0.0, y: 0.0 }));
    }

    #[test]
    fn stacked_shadows_accumulate_more_vertices_than_one() {
        let mesh_with = |calls: usize| {
            let mut s = Session::new();
            s.begin(uv());
            s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 4.0);
            for _ in 0..calls {
                s.add_shadow(&Shadow::default());
            }
            s.end()
        };
        assert!(mesh_with(2).vertices.len() > mesh_with(1).vertices.len());
    }

    #[test]
    fn border_is_a_hollow_ring_with_exact_vertex_and_index_counts() {
        // Plain rect (radius 0): 4 outer + 4 inner points, zipped into 4
        // quads (2 triangles each) — no shared centroid, unlike a fill.
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 0.0);
        s.add_border(&Border {
            thickness: 2.0,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
        });
        let mesh = s.end();
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.indices.len(), 4 * 6);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
    }

    #[test]
    fn hairline_border_scales_alpha_instead_of_geometry() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 0.0);
        s.add_border(&Border {
            thickness: 0.5,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
        });
        let mesh = s.end();
        // alpha 0xFF scaled by 0.5 (float-truncated in the core) == 0x7F;
        // the RGB channels are untouched.
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.col == rgba(0xFF, 0xFF, 0xFF, 0x7F)));
    }

    #[test]
    fn pixel_scale_makes_half_logical_pixel_a_crisp_device_pixel() {
        let mesh_at = |scale: f32| {
            let mut s = Session::new();
            s.begin(uv());
            s.set_pixel_scale(scale);
            s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 0.0);
            s.add_border(&Border {
                thickness: 0.5,
                color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            });
            s.end()
        };
        assert!(mesh_at(2.0)
            .vertices
            .iter()
            .all(|v| v.col == rgba(0xFF, 0xFF, 0xFF, 0xFF)));
        assert!(mesh_at(1.0)
            .vertices
            .iter()
            .all(|v| v.col == rgba(0xFF, 0xFF, 0xFF, 0x7F)));
    }

    #[test]
    fn inset_borders_stack_on_distinct_outlines() {
        let outer_color = rgba(0x11, 0x22, 0x33, 0xFF);
        let inner_color = rgba(0xDD, 0xEE, 0xFF, 0xFF);
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (50.0, 30.0)), 4.0);
        s.add_border(&Border {
            thickness: 1.0,
            color: outer_color,
        });
        s.add_border_inset(
            1.0,
            &Border {
                thickness: 1.0,
                color: inner_color,
            },
        );
        let mesh = s.end();

        let min_x_for = |color| {
            mesh.vertices
                .iter()
                .filter(|v| v.col == color)
                .map(|v| v.pos.x)
                .fold(f32::INFINITY, f32::min)
        };
        assert!((min_x_for(outer_color) - 0.0).abs() < 0.001);
        assert!((min_x_for(inner_color) - 1.0).abs() < 0.001);
    }

    #[test]
    fn invalid_border_geometry_is_ignored() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (50.0, 30.0)), 4.0);
        s.add_border_inset(
            -1.0,
            &Border {
                thickness: 1.0,
                color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            },
        );
        s.add_border(&Border {
            thickness: f32::NAN,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
        });
        assert!(s.end().vertices.is_empty());
    }

    #[test]
    fn begin_resets_pixel_scale() {
        let mut s = Session::new();
        s.begin(uv());
        s.set_pixel_scale(2.0);
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (50.0, 50.0)), 0.0);
        s.add_border(&Border {
            thickness: 0.5,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
        });
        assert!(s
            .end()
            .vertices
            .iter()
            .all(|v| v.col == rgba(0xFF, 0xFF, 0xFF, 0x7F)));
    }

    #[test]
    fn border_thicker_than_shape_does_not_panic() {
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (10.0, 10.0)), 2.0);
        s.add_border(&Border {
            thickness: 100.0,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
        });
        let mesh = s.end();
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
    }

    #[test]
    fn no_shape_means_no_shadow_or_border_output() {
        let mut s = Session::new();
        s.begin(uv());
        s.add_shadow(&Shadow::default());
        s.add_border(&Border {
            thickness: 1.0,
            color: rgba(0xFF, 0xFF, 0xFF, 0xFF),
        });
        let mesh = s.end();
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn line_produces_a_one_pixel_wide_quad_with_the_expected_span() {
        // A 1px vertical segment: the quad must span ±0.5px perpendicular
        // (so 1px wide) over the segment's length. `Canvas`'s Drop-submit
        // needs a live draw list, so the geometry is exercised at the
        // Session level it composes; submission rides the visual gate.
        let color = rgba(0xFF, 0x00, 0x00, 0xFF);
        let mut s = Session::new();
        s.begin(uv());
        s.line(
            Vec2 { x: 10.0, y: 5.0 },
            Vec2 { x: 10.0, y: 25.0 },
            1.0,
            color,
        );
        s.with_raw_mesh(|vtx, idx| {
            assert_eq!(vtx.len(), 4, "a line is one quad");
            assert_eq!(idx.len(), 6, "one quad is two triangles");
            let xs: Vec<f32> = vtx.iter().map(|v| v.pos.x).collect();
            let ys: Vec<f32> = vtx.iter().map(|v| v.pos.y).collect();
            assert!(
                xs.iter().all(|&x| x == 9.5 || x == 10.5),
                "1px wide: {xs:?}"
            );
            assert!(
                ys.iter().all(|&y| y == 5.0 || y == 25.0),
                "spans a->b: {ys:?}"
            );
            assert!(vtx.iter().all(|v| v.col == color));
        });
    }

    #[test]
    fn multiple_shapes_accumulate_into_one_mesh() {
        // The single-submit property `Canvas` relies on: several fills + a
        // line drawn between one begin and one read land in ONE mesh (not
        // one per shape). Two plain rects (5 vtx / 12 idx each) + one line
        // (4 vtx / 6 idx) = 14 vtx / 30 idx combined.
        let c = rgba(0x20, 0x40, 0x60, 0xFF);
        let mut s = Session::new();
        s.begin(uv());
        s.rounded_rect(rect((0.0, 0.0), (20.0, 10.0)), 0.0);
        s.fill_color(c);
        s.rounded_rect(rect((30.0, 0.0), (50.0, 10.0)), 0.0);
        s.fill_color(c);
        s.line(Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 0.0, y: 10.0 }, 1.0, c);
        s.with_raw_mesh(|vtx, idx| {
            assert_eq!(vtx.len(), 5 + 5 + 4);
            assert_eq!(idx.len(), 12 + 12 + 6);
        });
    }

    #[test]
    fn with_raw_mesh_matches_the_owned_mesh_copy() {
        // The zero-copy read path and the owned-Mesh path must see identical
        // geometry — so `Canvas`'s no-alloc submit draws the same thing the
        // tested `Session::end` path would.
        let c = rgba(0x11, 0x22, 0x33, 0xFF);
        let mut raw_session = Session::new();
        raw_session.begin(uv());
        raw_session.rounded_rect(rect((1.0, 2.0), (9.0, 8.0)), 2.0);
        raw_session.fill_color(c);
        let (raw_v, raw_i) = raw_session.with_raw_mesh(|vtx, idx| (vtx.to_vec(), idx.to_vec()));

        let mut owned_session = Session::new();
        owned_session.begin(uv());
        owned_session.rounded_rect(rect((1.0, 2.0), (9.0, 8.0)), 2.0);
        owned_session.fill_color(c);
        let owned = owned_session.end();

        assert_eq!(raw_v, owned.vertices);
        assert_eq!(raw_i, owned.indices);
    }
}
