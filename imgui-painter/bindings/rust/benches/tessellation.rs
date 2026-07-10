// Benchmarks Session-driven mesh generation against an equivalent
// hand-written pure-Rust tessellation (benches/handwritten.rs), both
// producing the macOS-panel look from punks-standalone's painter_demo
// example. There's no live ImDrawList/GPU context available outside a real
// ImGui frame, so this measures Painter's abstraction/FFI overhead
// directly -- "is the convenience worth its cost" -- rather than something
// GPU submission time would answer anyway.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use imgui_painter::{
    rgba, Border, ColorStop, Gradient, GradientMode, Mesh, Rect, Session, Shadow, Vec2,
};

mod handwritten;

// Matches painter_demo.rs's BOX_W/BOX_H, so this measures the same shape
// the phase-1 visual gate already validated, not a synthetic one.
const POS: (f32, f32) = (0.0, 0.0);
const SIZE: (f32, f32) = (220.0, 90.0);

fn paint_macos_panel(session: &mut Session) -> Mesh {
    session.begin(Vec2 { x: 0.5, y: 0.5 });
    session.rounded_rect(
        Rect {
            min: Vec2 { x: POS.0, y: POS.1 },
            max: Vec2 {
                x: POS.0 + SIZE.0,
                y: POS.1 + SIZE.1,
            },
        },
        12.0,
    );
    session.add_shadow(&Shadow {
        offset: Vec2 { x: 0.0, y: 6.0 },
        blur: 24.0,
        spread: 2.0,
        color: rgba(0, 0, 0, 60),
        inset: false,
    });
    session.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: Vec2 { x: POS.0, y: POS.1 },
        to: Vec2 {
            x: POS.0,
            y: POS.1 + SIZE.1,
        },
        stops: vec![
            ColorStop {
                t: 0.0,
                color: rgba(248, 248, 250, 255),
            },
            ColorStop {
                t: 1.0,
                color: rgba(228, 228, 232, 255),
            },
        ],
    });
    session.add_border(&Border {
        thickness: 1.0,
        color: rgba(210, 210, 214, 255),
    });
    session.end()
}

fn bench_tessellation(c: &mut Criterion) {
    let mesh = paint_macos_panel(&mut Session::new());
    let (hw_vtx, hw_idx) = handwritten::tessellate_macos_panel(POS, SIZE);
    println!(
        "\nmacOS panel mesh size -- imgui-painter: {} vtx / {} idx; hand-written: {} vtx / {} idx\n",
        mesh.vertices.len(),
        mesh.indices.len(),
        hw_vtx.len(),
        hw_idx.len()
    );

    let mut group = c.benchmark_group("macos_panel_tessellation");
    group.bench_function("imgui_painter", |b| {
        let mut session = Session::new();
        b.iter(|| black_box(paint_macos_panel(&mut session)));
    });
    group.bench_function("handwritten", |b| {
        b.iter(|| black_box(handwritten::tessellate_macos_panel(POS, SIZE)));
    });
    group.finish();
}

criterion_group!(benches, bench_tessellation);
criterion_main!(benches);
