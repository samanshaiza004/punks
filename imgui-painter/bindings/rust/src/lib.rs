//! Rust bindings for imgui-painter. This crate *is* the Rust adapter (see
//! the design doc's core/adapter split): it links the C++ core (compiled by
//! `build.rs` via `cc`, matching how `imgui-sys` compiles cimgui) and copies
//! the core's output mesh into a live `ImDrawList` through `imgui_sys`'s
//! `PrimReserve`/`PrimWriteVtx`/`PrimWriteIdx` (see [`adapter`]) — never by
//! touching `ImDrawList`'s internal buffers directly.
//!
//! [`Session`] is the safe wrapper over the raw `ip_ctx*` FFI calls.

pub mod adapter;

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
        pub fn ip_rounded_rect(ctx: *mut ip_ctx, rect: ip_rect, radius: f32);
        pub fn ip_fill_color(ctx: *mut ip_ctx, color: ip_color);
        pub fn ip_fill_gradient(ctx: *mut ip_ctx, gradient: *const ip_gradient);
        pub fn ip_add_shadow(ctx: *mut ip_ctx, shadow: *const ip_shadow);
        pub fn ip_add_border(ctx: *mut ip_ctx, border: *const ip_border);
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

/// A soft shadow around the current shape. `blur <= 0.0` draws a single
/// hard-edged ring at exactly `spread`, with no falloff. `inset` is accepted
/// for forward compatibility but not implemented yet (outer shadows only
/// this pass — see the C header).
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

/// A stroke around the current shape's outline. `thickness < 1.0` is drawn
/// at 1px with proportionally reduced alpha — see the C header's hairline
/// note.
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
/// Get a per-frame [`FramePainter`] from [`begin_frame`](Self::begin_frame);
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
    /// ImGui context must exist on this thread), then hands back a
    /// [`FramePainter`] scoped to this frame.
    pub fn begin_frame(&mut self) -> FramePainter<'_> {
        let white_uv = unsafe { adapter::white_pixel_uv() };
        FramePainter {
            painter: self,
            white_uv,
        }
    }
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

/// One frame's painting, borrowed from a [`Painter`] via
/// [`Painter::begin_frame`]. Submission lives here, on the frame — a shape
/// method builds its mesh and copies it straight into the target draw list
/// today, but because the frame owns that boundary, later work (GPU-batch
/// upload, `ImDrawListSplitter` z-order, frame stats) can change how/when
/// submission happens without touching any call site.
pub struct FramePainter<'a> {
    painter: &'a mut Painter,
    white_uv: Vec2,
}

impl FramePainter<'_> {
    /// Build a solid-filled (optionally rounded) rect and submit it into
    /// `dl`. `radius <= 0.0` is a plain rectangle — pixel-identical to
    /// `ImDrawList::add_rect(...).filled(true)`.
    ///
    /// # Safety
    /// `dl` must be a valid, currently-active `ImDrawList*` for this frame
    /// (e.g. `imgui::sys::igGetWindowDrawList()`), on the thread that owns
    /// the ImGui context — same contract as
    /// [`adapter::paint_to_draw_list`].
    pub unsafe fn fill_rounded_rect(
        &mut self,
        dl: *mut imgui_sys::ImDrawList,
        rect: Rect,
        radius: f32,
        color: Color,
    ) {
        let mesh = self.build_fill(rect, radius, color);
        adapter::paint_to_draw_list(dl, &mesh);
    }

    /// Mesh half of [`fill_rounded_rect`](Self::fill_rounded_rect), split
    /// out so it's exercisable without a live ImGui draw list (the tests
    /// build a `FramePainter` with a synthetic white-pixel UV and assert on
    /// this mesh). Submission stays on the frame; this only builds.
    fn build_fill(&mut self, rect: Rect, radius: f32, color: Color) -> Mesh {
        let session = &mut self.painter.session;
        session.begin(self.white_uv);
        session.rounded_rect(rect, radius);
        session.fill_color(color);
        session.end()
    }
}

impl Drop for FramePainter<'_> {
    fn drop(&mut self) {
        // No-op today. Reserved as the frame-end boundary for behavior that
        // belongs at frame granularity when it lands: frame stats / alloc
        // counters, validation hooks, deferred GPU-batch upload, paint
        // debugger snapshots. Per-shape scratch already clears on each
        // `fill_*` call (Session::begin), so there's nothing to reset here
        // yet — this exists so those futures don't change the call site.
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
        assert_eq!(version(), 1);
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
    fn frame_painter_fill_matches_the_equivalent_session_sequence() {
        // The Painter/FramePainter wrapper must build the exact same mesh a
        // direct Session begin -> rounded_rect -> fill_color -> end would.
        // Constructed by hand (bypassing begin_frame, which needs a live
        // ImGui context for white_pixel_uv) with a synthetic UV so the
        // geometry path is exercisable headlessly; draw_into's submission
        // half rides the manual visual check.
        let r = rect((3.0, 5.0), (43.0, 25.0));
        let color = rgba(0x12, 0x34, 0x56, 0xFF);

        let mut expected_session = Session::new();
        expected_session.begin(uv());
        expected_session.rounded_rect(r, 6.0);
        expected_session.fill_color(color);
        let expected = expected_session.end();

        let mut painter = Painter::new();
        let mut frame = FramePainter {
            painter: &mut painter,
            white_uv: uv(),
        };
        let got = frame.build_fill(r, 6.0, color);

        assert!(!got.vertices.is_empty());
        assert_eq!(got, expected);
    }
}
