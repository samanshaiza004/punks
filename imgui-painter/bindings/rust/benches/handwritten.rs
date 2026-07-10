// A hand-written pure-Rust tessellation of the same macOS-panel look
// `tessellation.rs` benchmarks through `Session` — no FFI, no generic
// gradient-evaluation dispatch (the linear lerp below is inlined directly,
// not routed through anything resembling `GradientT`'s mode switch). This
// is the "what would I write by hand" comparison point for measuring
// Painter's abstraction/FFI overhead, not a library of its own — duplicating
// painter.cpp's algorithm shape here (concentric shadow rings, fan-fill,
// stroked border ring) is intentional, since that's what a developer
// reaching for `ImDrawList` directly would end up writing anyway.

use imgui_painter::{rgba, Color, Vec2, Vertex};

const SEGMENTS_PER_CORNER: usize = 8;

fn corner_arc(cx: f32, cy: f32, r: f32, start: f32, out: &mut Vec<(f32, f32)>) {
    for i in 0..=SEGMENTS_PER_CORNER {
        let t = start + std::f32::consts::FRAC_PI_2 * (i as f32 / SEGMENTS_PER_CORNER as f32);
        out.push((cx + t.cos() * r, cy + t.sin() * r));
    }
}

fn rounded_rect_outline(min: (f32, f32), max: (f32, f32), radius: f32) -> Vec<(f32, f32)> {
    let r = radius
        .min((max.0 - min.0) * 0.5)
        .min((max.1 - min.1) * 0.5)
        .max(0.0);
    let mut out = Vec::new();
    corner_arc(min.0 + r, min.1 + r, r, std::f32::consts::PI, &mut out);
    corner_arc(
        max.0 - r,
        min.1 + r,
        r,
        -std::f32::consts::FRAC_PI_2,
        &mut out,
    );
    corner_arc(max.0 - r, max.1 - r, r, 0.0, &mut out);
    corner_arc(
        min.0 + r,
        max.1 - r,
        r,
        std::f32::consts::FRAC_PI_2,
        &mut out,
    );
    out
}

fn lerp_channel(a: u32, b: u32, t: f32) -> u32 {
    (a as f32 + (b as f32 - a as f32) * t) as u32
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let r = lerp_channel(a & 0xFF, b & 0xFF, t);
    let g = lerp_channel((a >> 8) & 0xFF, (b >> 8) & 0xFF, t);
    let bl = lerp_channel((a >> 16) & 0xFF, (b >> 16) & 0xFF, t);
    let al = lerp_channel((a >> 24) & 0xFF, (b >> 24) & 0xFF, t);
    r | (g << 8) | (bl << 16) | (al << 24)
}

fn scale_alpha(color: Color, factor: f32) -> Color {
    let rgb = color & 0x00FF_FFFF;
    let a = (color >> 24) & 0xFF;
    let scaled = (a as f32 * factor.clamp(0.0, 1.0)) as u32;
    rgb | (scaled << 24)
}

fn fill_fan(
    outline: &[(f32, f32)],
    uv: Vec2,
    color_at: impl Fn(f32, f32) -> Color,
    vtx: &mut Vec<Vertex>,
    idx: &mut Vec<u16>,
) {
    if outline.len() < 3 {
        return;
    }
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    for &(x, y) in outline {
        cx += x;
        cy += y;
    }
    cx /= outline.len() as f32;
    cy /= outline.len() as f32;

    let base = vtx.len() as u16;
    vtx.push(Vertex {
        pos: Vec2 { x: cx, y: cy },
        uv,
        col: color_at(cx, cy),
    });
    for &(x, y) in outline {
        vtx.push(Vertex {
            pos: Vec2 { x, y },
            uv,
            col: color_at(x, y),
        });
    }
    let n = outline.len() as u16;
    for i in 0..n {
        let a = base + 1 + i;
        let b = base + 1 + (i + 1) % n;
        idx.push(base);
        idx.push(a);
        idx.push(b);
    }
}

fn stroke_ring(
    outer: &[(f32, f32)],
    inner: &[(f32, f32)],
    color: Color,
    uv: Vec2,
    vtx: &mut Vec<Vertex>,
    idx: &mut Vec<u16>,
) {
    let n = outer.len().min(inner.len());
    if n < 2 {
        return;
    }
    let base = vtx.len() as u16;
    for i in 0..n {
        vtx.push(Vertex {
            pos: Vec2 {
                x: outer[i].0,
                y: outer[i].1,
            },
            uv,
            col: color,
        });
        vtx.push(Vertex {
            pos: Vec2 {
                x: inner[i].0,
                y: inner[i].1,
            },
            uv,
            col: color,
        });
    }
    let un = n as u16;
    for i in 0..un {
        let i0 = base + i * 2;
        let i1 = base + i * 2 + 1;
        let j0 = base + (i + 1) % un * 2;
        let j1 = base + (i + 1) % un * 2 + 1;
        idx.push(i0);
        idx.push(j0);
        idx.push(i1);
        idx.push(i1);
        idx.push(j0);
        idx.push(j1);
    }
}

pub fn tessellate_macos_panel(pos: (f32, f32), size: (f32, f32)) -> (Vec<Vertex>, Vec<u16>) {
    let uv = Vec2 { x: 0.5, y: 0.5 };
    let min = pos;
    let max = (pos.0 + size.0, pos.1 + size.1);
    let radius = 12.0f32;

    let mut vtx = Vec::new();
    let mut idx = Vec::new();

    // Shadow: same "concentric rings, quadratic falloff, outermost first"
    // shape as painter.cpp's ip_add_shadow, hand-coded directly instead of
    // going through a generic ring-builder.
    let offset = (0.0f32, 6.0f32);
    let blur = 24.0f32;
    let spread = 2.0f32;
    let shadow_color = rgba(0, 0, 0, 60);
    let ring_count = ((blur / 2.0) as i32 + 3).clamp(3, 12);
    for i in (0..ring_count).rev() {
        let f = if ring_count > 1 {
            i as f32 / (ring_count - 1) as f32
        } else {
            0.0
        };
        let expand = spread + blur * f;
        let falloff = (1.0 - f) * (1.0 - f);
        let ring_min = (min.0 - expand + offset.0, min.1 - expand + offset.1);
        let ring_max = (max.0 + expand + offset.0, max.1 + expand + offset.1);
        let ring_radius = radius + expand;
        let ring_color = scale_alpha(shadow_color, falloff);
        let outline = rounded_rect_outline(ring_min, ring_max, ring_radius);
        fill_fan(&outline, uv, |_, _| ring_color, &mut vtx, &mut idx);
    }

    // Fill: linear gradient, top to bottom, two stops.
    let top = rgba(248, 248, 250, 255);
    let bottom = rgba(228, 228, 232, 255);
    let fill_outline = rounded_rect_outline(min, max, radius);
    fill_fan(
        &fill_outline,
        uv,
        |_, y| {
            let t = (y - min.1) / size.1;
            lerp_color(top, bottom, t)
        },
        &mut vtx,
        &mut idx,
    );

    // Border: 1px stroked ring.
    let border_color = rgba(210, 210, 214, 255);
    let thickness = 1.0f32;
    let outer = rounded_rect_outline(min, max, radius);
    let inner_min = (min.0 + thickness, min.1 + thickness);
    let inner_max = (max.0 - thickness, max.1 - thickness);
    let inner_radius = (radius - thickness).max(0.0);
    let inner = rounded_rect_outline(inner_min, inner_max, inner_radius);
    stroke_ring(&outer, &inner, border_color, uv, &mut vtx, &mut idx);

    (vtx, idx)
}
