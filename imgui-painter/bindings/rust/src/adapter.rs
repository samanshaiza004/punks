//! Copies a finished [`Mesh`] into a live Dear ImGui `ImDrawList`, through
//! `imgui_sys`'s public draw-list primitives (`PrimReserve`/`PrimWriteVtx`/
//! `PrimWriteIdx`) — the same C API cimgui exposes to any C caller. Never
//! touches `ImDrawList`'s internal buffers directly (see the crate's module
//! docs and the design doc's core/adapter split).
//!
//! Depends on `imgui-sys` at the same "0.12" version punks-ui's `imgui`
//! crate pulls in, so Cargo resolves them to one shared build — this rides
//! the host app's existing ImGui instance rather than linking a second copy.

use imgui_sys as sys;

use crate::{Mesh, Vec2};

/// Append `mesh` to `draw_list`'s current command, inheriting its active
/// clip rect and texture binding (that's what `PrimReserve` sets up).
///
/// # Safety
/// `draw_list` must be a valid, currently-active `ImDrawList*` (e.g. from
/// `ui.get_window_draw_list()` via [`white_pixel_uv`]'s sibling call, or
/// `igGetWindowDrawList()` directly), and this must run on the thread that
/// owns the ImGui context for the current frame.
pub unsafe fn paint_to_draw_list(draw_list: *mut sys::ImDrawList, mesh: &Mesh) {
    if mesh.vertices.is_empty() || mesh.indices.is_empty() {
        return;
    }
    sys::ImDrawList_PrimReserve(
        draw_list,
        mesh.indices.len() as i32,
        mesh.vertices.len() as i32,
    );
    // Indices are session-local (0-based); the draw list's vertex buffer is
    // shared across everything drawn this frame, so rebase against its
    // current write offset before writing ours in.
    let base = (*draw_list)._VtxCurrentIdx as u16;
    for &i in &mesh.indices {
        sys::ImDrawList_PrimWriteIdx(draw_list, base + i);
    }
    for v in &mesh.vertices {
        let pos = sys::ImVec2 {
            x: v.pos.x,
            y: v.pos.y,
        };
        let uv = sys::ImVec2 {
            x: v.uv.x,
            y: v.uv.y,
        };
        sys::ImDrawList_PrimWriteVtx(draw_list, pos, uv, v.col);
    }
}

/// The UV of the host's font-atlas all-white texel — pass to
/// [`crate::Session::begin`] so fills/gradients need no separate untextured
/// draw path.
///
/// # Safety
/// Must be called with an active ImGui context on the current thread (same
/// requirement as any other `igGet*` call).
pub unsafe fn white_pixel_uv() -> Vec2 {
    let mut uv = sys::ImVec2 { x: 0.0, y: 0.0 };
    sys::igGetFontTexUvWhitePixel(&mut uv);
    Vec2 { x: uv.x, y: uv.y }
}
