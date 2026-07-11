//! Proves the per-frame draw path allocates nothing on the Rust heap in
//! steady state — the "0 steady-state allocations/frame" metric, asserted,
//! not left to review. Its own test binary (one test, no parallel siblings)
//! so a process-wide counting allocator sees only this test's traffic.
//!
//! Scope, stated honestly: this covers the *Rust* accumulation + zero-copy
//! read path (`Session::begin` + `rounded_rect`/`fill_color`/`line` +
//! `with_raw_mesh`) — exactly what `Canvas` runs every frame minus the
//! `PrimReserve` submit, which needs a live ImGui context and rides the
//! visual gate. The mesh buffers themselves live in the C++ arena (their
//! own allocator, invisible to this counter) and don't realloc after
//! warm-up by construction (`ip_begin` clears without freeing). What this
//! catches is a regression that reintroduces a Rust-side per-frame
//! allocation — e.g. going back through `Session::end`'s owned-`Mesh`
//! `to_vec`, which is exactly what `Canvas` avoids.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use imgui_painter::{rgba, Rect, Session, Vec2};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn uv() -> Vec2 {
    Vec2 { x: 0.5, y: 0.5 }
}

/// One waveform's worth of geometry through the exact calls `Canvas` makes:
/// a background rect, 512 bars, a playhead line, two crosshair lines,
/// finished by the zero-copy raw read `Canvas` submits through.
fn draw_waveform(s: &mut Session) -> usize {
    let c = rgba(0x40, 0x80, 0xC0, 0xFF);
    s.begin(uv());
    s.rounded_rect(
        Rect {
            min: Vec2 { x: 0.0, y: 0.0 },
            max: Vec2 { x: 512.0, y: 80.0 },
        },
        0.0,
    );
    s.fill_color(c);
    for i in 0..512u32 {
        let x = i as f32;
        s.rounded_rect(
            Rect {
                min: Vec2 { x, y: 10.0 },
                max: Vec2 {
                    x: x + 0.5,
                    y: 70.0,
                },
            },
            0.0,
        );
        s.fill_color(c);
    }
    s.line(
        Vec2 { x: 256.0, y: 0.0 },
        Vec2 { x: 256.0, y: 80.0 },
        1.0,
        c,
    );
    s.line(
        Vec2 { x: 250.0, y: 40.0 },
        Vec2 { x: 262.0, y: 40.0 },
        1.0,
        c,
    );
    s.line(
        Vec2 { x: 256.0, y: 34.0 },
        Vec2 { x: 256.0, y: 46.0 },
        1.0,
        c,
    );
    s.with_raw_mesh(|vtx, idx| vtx.len() + idx.len())
}

#[test]
fn steady_state_frame_makes_no_rust_allocations() {
    let mut s = Session::new();

    // Warm up: the first frame grows the arena's buffers to their steady
    // size. After this, redrawing the same geometry must not touch the
    // Rust heap.
    let warm = draw_waveform(&mut s);
    assert!(warm > 0, "sanity: the frame produced geometry");

    let before = ALLOCS.load(Ordering::Relaxed);
    let steady = draw_waveform(&mut s);
    let after = ALLOCS.load(Ordering::Relaxed);

    assert_eq!(steady, warm, "same geometry, same mesh size");
    assert_eq!(
        after - before,
        0,
        "a steady-state frame must make no Rust-heap allocations (got {})",
        after - before
    );
}
