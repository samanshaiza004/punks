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

use crate::{Mesh, Vec2, Vertex};

/// Append `mesh` to `draw_list`'s current command — the owned-[`Mesh`]
/// convenience over [`paint_raw`]. Used by the benchmark and owned-mesh
/// tests; the per-frame draw path ([`crate::Canvas`]) goes through
/// [`paint_raw`] to avoid the owned copy.
///
/// # Safety
/// Same as [`paint_raw`].
pub unsafe fn paint_to_draw_list(draw_list: *mut sys::ImDrawList, mesh: &Mesh) {
    paint_raw(draw_list, &mesh.vertices, &mesh.indices);
}

/// Append `vertices`/`indices` to `draw_list`'s current command, inheriting
/// its active clip rect and texture binding (that's what `PrimReserve` sets
/// up). Borrows the slices — no owned `Mesh`, so the per-frame draw path can
/// submit straight from the arena with zero allocation.
///
/// # Safety
/// `draw_list` must be a valid, currently-active `ImDrawList*` (e.g. from
/// `igGetWindowDrawList()`), on the thread that owns the ImGui context for
/// the current frame. The slices must stay valid for the call.
pub unsafe fn paint_raw(draw_list: *mut sys::ImDrawList, vertices: &[Vertex], indices: &[u16]) {
    if vertices.is_empty() || indices.is_empty() {
        return;
    }
    sys::ImDrawList_PrimReserve(draw_list, indices.len() as i32, vertices.len() as i32);
    // Indices are session-local (0-based); the draw list's vertex buffer is
    // shared across everything drawn this frame, so rebase against its
    // current write offset before writing ours in.
    let base = (*draw_list)._VtxCurrentIdx as u16;
    for &i in indices {
        sys::ImDrawList_PrimWriteIdx(draw_list, base.wrapping_add(i));
    }
    for v in vertices {
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
