// Core tessellation: rounded rects, gradient fills, (later) shadow rings and
// borders. Zero Dear ImGui / cimgui dependency — see imgui_painter_c.h's
// header comment. Pure math in, a generic (ip_vertex, uint16_t index) mesh
// out.

#include "../capi/imgui_painter_c.h"

#include <algorithm>
#include <cmath>
#include <vector>

namespace {

constexpr float kPi = 3.14159265358979323846f;

// Max allowed sagitta (perpendicular deviation of a chord from the true
// arc) in pixels — the tessellation-quality knob for CornerSegments.
constexpr float kMaxSegmentError = 0.35f;

// Segments per 90-degree corner arc for a given radius, error-bounded: for
// chord/arc deviation up to kMaxSegmentError, the per-segment half-angle is
// theta/2 = acos(1 - error/radius) (standard sagitta-to-angle relation),
// and segments = ceil((PI/2) / theta). Segment count then scales with how
// large the arc actually renders (roughly sqrt(radius) for radius >>
// error), not linearly with radius — a plain linear-in-radius heuristic
// either over-tessellates small corners or, once clamped to a fixed
// ceiling, under-tessellates large ones (visible faceting on big rounded
// panels). Same goal as Dear ImGui's own `_CalcCircleAutoSegmentCount`,
// derived independently — the core still never calls into ImGui.
int CornerSegments(float radius) {
    if (radius <= 0.0f) {
        return 0;
    }
    // acos's argument must stay in [-1, 1]; clamp the error/radius ratio to
    // keep 1 - ratio there even for radii smaller than the error tolerance.
    const float ratio = std::clamp(kMaxSegmentError / radius, 0.0f, 1.9f);
    const float theta = 2.0f * std::acos(1.0f - ratio);
    const int segments = static_cast<int>(std::ceil((kPi * 0.5f) / theta));
    return std::clamp(segments, 4, 64);
}

struct Corner {
    float cx, cy;
    float start_angle;
};

ip_vec2 ArcPointAt(const Corner &c, float r, float angle) {
    return {c.cx + std::cos(angle) * r, c.cy + std::sin(angle) * r};
}

void AppendArc(const Corner &c, float r, int segments, std::vector<ip_vec2> &out) {
    // segments == 0 (the r <= 0 plain-rect path) degenerates to a single
    // point at the corner itself — guarded separately so the division below
    // never sees a zero denominator (0.0f / 0.0f is NaN, not 0, and that NaN
    // would otherwise survive being multiplied by r == 0 in ArcPointAt).
    if (segments <= 0) {
        out.push_back(ArcPointAt(c, r, c.start_angle));
        return;
    }
    for (int i = 0; i <= segments; ++i) {
        const float t = c.start_angle +
                         (kPi * 0.5f) * (static_cast<float>(i) / static_cast<float>(segments));
        out.push_back(ArcPointAt(c, r, t));
    }
}

// Append `subdivisions - 1` extra points strictly between `a` and `b` (the
// caller already emitted `a`; the next arc/edge emits `b`). `subdivisions
// <= 1` appends nothing — a plain edge from `a` straight to `b`.
void AppendEdgeSubdivisions(ip_vec2 a, ip_vec2 b, int subdivisions, std::vector<ip_vec2> &out) {
    for (int s = 1; s < subdivisions; ++s) {
        const float f = static_cast<float>(s) / static_cast<float>(subdivisions);
        out.push_back({a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f});
    }
}

// Append the outline of a rounded rect (clockwise, starting at the top-left
// corner's arc) to `out`. `radius <= 0` degenerates each corner's arc to a
// single point, so this same code path also produces a plain rectangle.
//
// `segments` is explicit (not derived from `radius` here) so two outlines
// that must tessellate to matching point counts — a border's outer and
// inner ring — can share one value regardless of their different radii; see
// `RoundedRectOutlineAuto` for the "just pick something reasonable" case.
//
// `edge_subdivisions` adds extra colinear points along each of the 4
// straight edges between corners — irrelevant for a solid fill (1, the
// default: no extra points) but needed for a gradient fill, where a fan
// triangle spanning too much of the shape can miss a color-stop's slope
// change (see EvalGradient / kGradientEdgeSubdivisions below).
void RoundedRectOutline(ip_rect rect, float radius, int segments, int edge_subdivisions,
                         std::vector<ip_vec2> &out) {
    const float w = rect.max.x - rect.min.x;
    const float h = rect.max.y - rect.min.y;
    const float r = std::clamp(radius, 0.0f, std::min(w, h) * 0.5f);

    // Centers of the four corner arcs, in outline (clockwise) order:
    // top-left, top-right, bottom-right, bottom-left. Angles are measured
    // from +x, clockwise (screen space, +y down). At r == 0 each "arc"
    // degenerates to the corner point itself, regardless of angle.
    const Corner corners[4] = {
        {rect.min.x + r, rect.min.y + r, kPi},
        {rect.max.x - r, rect.min.y + r, -kPi * 0.5f},
        {rect.max.x - r, rect.max.y - r, 0.0f},
        {rect.min.x + r, rect.max.y - r, kPi * 0.5f},
    };

    for (int i = 0; i < 4; ++i) {
        AppendArc(corners[i], r, segments, out);
        const Corner &next = corners[(i + 1) % 4];
        const ip_vec2 next_start = ArcPointAt(next, r, next.start_angle);
        AppendEdgeSubdivisions(out.back(), next_start, edge_subdivisions, out);
    }
}

// Convenience over `RoundedRectOutline` for the common case (a single
// outline, no matching-point-count requirement): picks a segment count from
// the shape's clamped radius via `CornerSegments`.
void RoundedRectOutlineAuto(ip_rect rect, float radius, int edge_subdivisions,
                             std::vector<ip_vec2> &out) {
    const float w = rect.max.x - rect.min.x;
    const float h = rect.max.y - rect.min.y;
    const float r = std::clamp(radius, 0.0f, std::min(w, h) * 0.5f);
    RoundedRectOutline(rect, radius, CornerSegments(r), edge_subdivisions, out);
}

// Fan-triangulate a convex polygon (the rounded-rect outline always is)
// around its centroid. `color_at(point)` supplies each vertex's (and the
// centroid's) color — a constant closure for a solid fill, gradient
// evaluation for `ip_fill_gradient`.
template <typename ColorAt>
void FillConvexFan(const std::vector<ip_vec2> &poly, ColorAt color_at, ip_vec2 uv,
                    std::vector<ip_vertex> &vtx, std::vector<uint16_t> &idx) {
    if (poly.size() < 3) {
        return;
    }
    ip_vec2 centroid{0.0f, 0.0f};
    for (const auto &p : poly) {
        centroid.x += p.x;
        centroid.y += p.y;
    }
    centroid.x /= static_cast<float>(poly.size());
    centroid.y /= static_cast<float>(poly.size());

    const uint16_t base = static_cast<uint16_t>(vtx.size());
    vtx.push_back({centroid, uv, color_at(centroid)});
    for (const auto &p : poly) {
        vtx.push_back({p, uv, color_at(p)});
    }
    for (uint16_t i = 0; i < poly.size(); ++i) {
        const uint16_t a = base + 1 + i;
        const uint16_t b = base + 1 + static_cast<uint16_t>((i + 1) % poly.size());
        idx.push_back(base);
        idx.push_back(a);
        idx.push_back(b);
    }
}

ip_color LerpColor(ip_color a, ip_color b, float t) {
    t = std::clamp(t, 0.0f, 1.0f);
    auto lerp_channel = [t](uint32_t ca, uint32_t cb) -> uint32_t {
        return static_cast<uint32_t>(static_cast<float>(ca) +
                                      (static_cast<float>(cb) - static_cast<float>(ca)) * t);
    };
    const uint32_t r = lerp_channel(a & 0xFF, b & 0xFF);
    const uint32_t g = lerp_channel((a >> 8) & 0xFF, (b >> 8) & 0xFF);
    const uint32_t bch = lerp_channel((a >> 16) & 0xFF, (b >> 16) & 0xFF);
    const uint32_t al = lerp_channel((a >> 24) & 0xFF, (b >> 24) & 0xFF);
    return r | (g << 8) | (bch << 16) | (al << 24);
}

// Gradient parameter `t` at `p`, per mode. A degenerate axis/radius
// (`from == to`) evaluates to a constant t == 0 for every mode rather than
// dividing by zero — consistent behavior across all four, not a special
// case per mode.
float GradientT(const ip_gradient &grad, ip_vec2 p) {
    switch (grad.mode) {
    case IP_GRADIENT_RADIAL: {
        const float radius = std::hypot(grad.to.x - grad.from.x, grad.to.y - grad.from.y);
        if (radius <= 0.0f) {
            return 0.0f;
        }
        return std::hypot(p.x - grad.from.x, p.y - grad.from.y) / radius;
    }
    case IP_GRADIENT_ANGULAR: {
        // Sweep/conic gradient: t is the angle from `from` to `p`, offset
        // by the from->to axis's own angle (so `to`'s direction is the
        // sweep's t == 0 start), normalized over one full turn. Needs an
        // explicit from==to guard: atan2(0, 0) only makes `axis_angle`
        // well-defined (0, not NaN) — `point_angle` still varies with `p`'s
        // actual position, so without this check a degenerate axis would
        // sweep normally instead of collapsing to a constant t == 0 like
        // Linear/Radial/Diamond do.
        if (grad.to.x == grad.from.x && grad.to.y == grad.from.y) {
            return 0.0f;
        }
        const float axis_angle = std::atan2(grad.to.y - grad.from.y, grad.to.x - grad.from.x);
        const float point_angle = std::atan2(p.y - grad.from.y, p.x - grad.from.x);
        float delta = std::fmod(point_angle - axis_angle, 2.0f * kPi);
        if (delta < 0.0f) {
            delta += 2.0f * kPi;
        }
        return delta / (2.0f * kPi);
    }
    case IP_GRADIENT_DIAMOND: {
        // Concentric diamond iso-lines, scaled per-axis by the from->to
        // box (the standard "diamond gradient" convention) so `to` still
        // means "this is the t == 1 edge", consistent with Linear/Radial.
        // Each axis independently falls back to 0 (not NaN/Inf) if its own
        // extent is degenerate, so a from==to point degenerates to t == 0
        // like every other mode.
        const float ax = std::fabs(grad.to.x - grad.from.x);
        const float ay = std::fabs(grad.to.y - grad.from.y);
        const float dx = std::fabs(p.x - grad.from.x);
        const float dy = std::fabs(p.y - grad.from.y);
        const float tx = ax > 0.0f ? dx / ax : 0.0f;
        const float ty = ay > 0.0f ? dy / ay : 0.0f;
        return std::max(tx, ty);
    }
    case IP_GRADIENT_LINEAR:
    default: {
        // Linear, and the fallback for any future/unrecognized mode value.
        const float ax = grad.to.x - grad.from.x;
        const float ay = grad.to.y - grad.from.y;
        const float len_sq = ax * ax + ay * ay;
        if (len_sq <= 0.0f) {
            return 0.0f;
        }
        return ((p.x - grad.from.x) * ax + (p.y - grad.from.y) * ay) / len_sq;
    }
    }
}

// ponytail: fixed subdivision count rather than analytic stop-boundary
// insertion — visually smooth for a handful of stops (this phase's target),
// but a triangle can still span a stop's slope "kink" for gradients with
// many closely-spaced stops. Upgrade path: subdivide edges at each stop's
// projected position along the gradient axis instead of a fixed count.
//
// ponytail: same fixed-subdivision limitation applies to IP_GRADIENT_ANGULAR's
// hard wrap seam (t approx 1 meets t approx 0) — a single fan triangle
// straddling that seam interpolates straight across it instead of wrapping,
// showing a visible color snap on close inspection. Not fixed this pass;
// same upgrade path (an explicit extra outline point exactly at the seam
// angle) as the multi-stop case above.
constexpr int kGradientEdgeSubdivisions = 16;

ip_color EvalGradient(const ip_gradient &grad, ip_vec2 p) {
    if (grad.stop_count <= 0) {
        return 0;
    }
    if (grad.stop_count == 1) {
        return grad.stops[0].color;
    }

    const float t = std::clamp(GradientT(grad, p), 0.0f, 1.0f);
    const int last = grad.stop_count - 1;
    if (t <= grad.stops[0].t) {
        return grad.stops[0].color;
    }
    if (t >= grad.stops[last].t) {
        return grad.stops[last].color;
    }
    // Stops are assumed sorted ascending by `t` (documented contract).
    for (int i = 0; i < last; ++i) {
        const ip_color_stop &s0 = grad.stops[i];
        const ip_color_stop &s1 = grad.stops[i + 1];
        if (t >= s0.t && t <= s1.t) {
            const float span = s1.t - s0.t;
            const float local_t = span > 0.0f ? (t - s0.t) / span : 0.0f;
            return LerpColor(s0.color, s1.color, local_t);
        }
    }
    return grad.stops[last].color;
}

// Scale `color`'s alpha channel by `factor` (clamped to [0, 1]). Used for
// shadow ring falloff and hairline-border alpha compensation.
ip_color ScaleAlpha(ip_color color, float factor) {
    factor = std::clamp(factor, 0.0f, 1.0f);
    const uint32_t rgb = color & 0x00FFFFFF;
    const uint32_t alpha = (color >> 24) & 0xFF;
    const uint32_t scaled = static_cast<uint32_t>(static_cast<float>(alpha) * factor);
    return rgb | (scaled << 24);
}

// Build a hollow ring (outer outline minus inner outline, connected by a
// quad strip) — the shape a border stroke needs, as opposed to the solid
// fan `FillConvexFan` produces. Outer and inner outlines are tessellated
// with the *same* `segments` (see `RoundedRectOutline`'s doc comment) so
// they zip together 1:1.
void StrokeRing(ip_rect rect, float radius, float thickness, ip_color color, ip_vec2 uv,
                std::vector<ip_vertex> &vtx, std::vector<uint16_t> &idx) {
    const float w = rect.max.x - rect.min.x;
    const float h = rect.max.y - rect.min.y;
    const float outer_r = std::clamp(radius, 0.0f, std::min(w, h) * 0.5f);
    const int segments = CornerSegments(outer_r);

    std::vector<ip_vec2> outer;
    RoundedRectOutline(rect, outer_r, segments, /*edge_subdivisions=*/1, outer);

    // Shrink inward by `thickness` on all sides. Guard the case where
    // `thickness` exceeds half the shape's extent (a border thicker than
    // the shape) by collapsing the inner rect to a centered point rather
    // than emitting inverted (min > max) geometry.
    ip_rect inner_rect{
        {rect.min.x + thickness, rect.min.y + thickness},
        {rect.max.x - thickness, rect.max.y - thickness},
    };
    if (inner_rect.min.x > inner_rect.max.x) {
        inner_rect.min.x = inner_rect.max.x = (rect.min.x + rect.max.x) * 0.5f;
    }
    if (inner_rect.min.y > inner_rect.max.y) {
        inner_rect.min.y = inner_rect.max.y = (rect.min.y + rect.max.y) * 0.5f;
    }
    const float inner_r = std::max(outer_r - thickness, 0.0f);
    std::vector<ip_vec2> inner;
    RoundedRectOutline(inner_rect, inner_r, segments, /*edge_subdivisions=*/1, inner);

    const size_t n = std::min(outer.size(), inner.size());
    if (n < 2) {
        return;
    }
    const uint16_t base = static_cast<uint16_t>(vtx.size());
    for (size_t i = 0; i < n; ++i) {
        vtx.push_back({outer[i], uv, color});
        vtx.push_back({inner[i], uv, color});
    }
    const uint16_t un = static_cast<uint16_t>(n);
    for (uint16_t i = 0; i < un; ++i) {
        const uint16_t i0 = base + i * 2;
        const uint16_t i1 = base + i * 2 + 1;
        const uint16_t j0 = base + ((i + 1) % un) * 2;
        const uint16_t j1 = base + ((i + 1) % un) * 2 + 1;
        idx.push_back(i0);
        idx.push_back(j0);
        idx.push_back(i1);
        idx.push_back(i1);
        idx.push_back(j0);
        idx.push_back(j1);
    }
}

} // namespace

struct ip_ctx {
    ip_vec2 white_uv{0.0f, 0.0f};
    std::vector<ip_vertex> vtx;
    std::vector<uint16_t> idx;

    ip_rect shape_rect{};
    float shape_radius = 0.0f;
    bool has_shape = false;
};

ip_ctx *ip_ctx_create(void) { return new ip_ctx(); }

void ip_ctx_destroy(ip_ctx *ctx) { delete ctx; }

void ip_begin(ip_ctx *ctx, ip_vec2 white_pixel_uv) {
    ctx->white_uv = white_pixel_uv;
    ctx->vtx.clear();
    ctx->idx.clear();
    ctx->has_shape = false;
}

void ip_rounded_rect(ip_ctx *ctx, ip_rect rect, float radius) {
    ctx->shape_rect = rect;
    ctx->shape_radius = radius;
    ctx->has_shape = true;
}

void ip_fill_color(ip_ctx *ctx, ip_color color) {
    if (!ctx->has_shape) {
        return;
    }
    std::vector<ip_vec2> outline;
    RoundedRectOutlineAuto(ctx->shape_rect, ctx->shape_radius, /*edge_subdivisions=*/1, outline);
    FillConvexFan(
        outline, [color](ip_vec2) { return color; }, ctx->white_uv, ctx->vtx, ctx->idx);
}

void ip_fill_gradient(ip_ctx *ctx, const ip_gradient *gradient) {
    if (!ctx->has_shape || gradient == nullptr || gradient->stop_count == 0) {
        return;
    }
    std::vector<ip_vec2> outline;
    RoundedRectOutlineAuto(ctx->shape_rect, ctx->shape_radius, kGradientEdgeSubdivisions, outline);
    const ip_gradient grad = *gradient;
    FillConvexFan(
        outline, [&grad](ip_vec2 p) { return EvalGradient(grad, p); }, ctx->white_uv, ctx->vtx,
        ctx->idx);
}

void ip_line(ip_ctx *ctx, ip_vec2 a, ip_vec2 b, float thickness, ip_color color) {
    // A straight segment as a `thickness`-wide quad: offset each endpoint by
    // half the thickness along the segment's perpendicular, emit two
    // triangles. Independent of the current shape (unlike ip_fill_*), so it
    // composes freely alongside fills in one accumulation.
    //
    // ponytail: no anti-aliasing, unlike Dear ImGui's own AddLine (which
    // feathers the edge). Pixel-identical for axis-aligned integer-width
    // lines (all punks2 uses today: playhead + crosshair, 1px); a visible
    // hard edge for diagonal lines. Upgrade path: a feathered-edge stroke
    // (an inner opaque quad flanked by alpha-ramp edge quads) if a diagonal
    // consumer ever appears.
    const float dx = b.x - a.x;
    const float dy = b.y - a.y;
    const float len = std::sqrt(dx * dx + dy * dy);
    if (len <= 0.0f) {
        return;
    }
    const float half = thickness * 0.5f;
    const float nx = -dy / len * half;
    const float ny = dx / len * half;

    const uint16_t base = static_cast<uint16_t>(ctx->vtx.size());
    ctx->vtx.push_back({{a.x + nx, a.y + ny}, ctx->white_uv, color});
    ctx->vtx.push_back({{b.x + nx, b.y + ny}, ctx->white_uv, color});
    ctx->vtx.push_back({{b.x - nx, b.y - ny}, ctx->white_uv, color});
    ctx->vtx.push_back({{a.x - nx, a.y - ny}, ctx->white_uv, color});
    ctx->idx.push_back(base);
    ctx->idx.push_back(base + 1);
    ctx->idx.push_back(base + 2);
    ctx->idx.push_back(base);
    ctx->idx.push_back(base + 2);
    ctx->idx.push_back(base + 3);
}

void ip_add_shadow(ip_ctx *ctx, const ip_shadow *shadow) {
    if (!ctx->has_shape || shadow == nullptr) {
        return;
    }
    const float spread = shadow->spread;
    const float blur = std::max(shadow->blur, 0.0f);
    // Ring count is the quality knob: more rings for a bigger blur radius,
    // clamped to a sane range. blur <= 0 collapses to a single hard-edged
    // ring at exactly `spread` — no falloff to approximate.
    const int ring_count =
        blur > 0.0f ? std::clamp(static_cast<int>(blur / 2.0f) + 3, 3, 12) : 1;

    // Outermost (largest, weakest) ring first, innermost (smallest,
    // strongest) last, so the more-opaque rings paint on top of the softer
    // ones — the standard "nested translucent shapes" soft-shadow trick.
    for (int i = ring_count - 1; i >= 0; --i) {
        const float f = ring_count > 1
                             ? static_cast<float>(i) / static_cast<float>(ring_count - 1)
                             : 0.0f;
        const float expand = spread + blur * f;
        // Quadratic falloff: a cheap, visually reasonable stand-in for a
        // true Gaussian without doing any actual convolution.
        const float falloff = (1.0f - f) * (1.0f - f);

        const ip_rect ring_rect{
            {ctx->shape_rect.min.x - expand + shadow->offset.x,
             ctx->shape_rect.min.y - expand + shadow->offset.y},
            {ctx->shape_rect.max.x + expand + shadow->offset.x,
             ctx->shape_rect.max.y + expand + shadow->offset.y},
        };
        const float ring_radius = ctx->shape_radius + expand;
        const ip_color ring_color = ScaleAlpha(shadow->color, falloff);

        std::vector<ip_vec2> outline;
        RoundedRectOutlineAuto(ring_rect, ring_radius, /*edge_subdivisions=*/1, outline);
        FillConvexFan(
            outline, [ring_color](ip_vec2) { return ring_color; }, ctx->white_uv, ctx->vtx,
            ctx->idx);
    }
}

void ip_add_border(ip_ctx *ctx, const ip_border *border) {
    if (!ctx->has_shape || border == nullptr || border->thickness <= 0.0f) {
        return;
    }
    // Hairline compensation: sub-pixel geometry rasterizes unreliably
    // without MSAA, so thicknesses under 1px are drawn at a full 1px but
    // with proportionally reduced alpha instead — the same trick many 2D
    // vector renderers use to approximate a true hairline.
    const float draw_thickness = std::max(border->thickness, 1.0f);
    const float alpha_scale = border->thickness < 1.0f ? border->thickness : 1.0f;
    const ip_color draw_color = ScaleAlpha(border->color, alpha_scale);
    StrokeRing(ctx->shape_rect, ctx->shape_radius, draw_thickness, draw_color, ctx->white_uv,
               ctx->vtx, ctx->idx);
}

ip_mesh ip_end(ip_ctx *ctx) {
    return ip_mesh{
        ctx->vtx.data(),
        static_cast<int32_t>(ctx->vtx.size()),
        ctx->idx.data(),
        static_cast<int32_t>(ctx->idx.size()),
    };
}
