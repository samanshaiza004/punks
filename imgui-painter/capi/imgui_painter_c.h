/* imgui-painter C API — the FFI surface every language binding compiles
 * against. Mirrors the core 1:1; include/imgui_painter.h wraps this in a
 * fluent Painter type for direct C++ consumers (see its own header comment
 * — the Rust adapter, bindings/rust, was phase 1's proof of the core/adapter
 * split; this is phase 2's binding for C++ callers of the same core).
 *
 * Zero Dear ImGui / cimgui dependency by design: every type here is a plain
 * value type imgui-painter owns. A host embedding this against a real
 * ImDrawList (e.g. imgui-painter's Rust adapter) copies the resulting
 * `ip_mesh` in through its host's own public draw-list API
 * (PrimReserve/PrimWriteVtx/PrimWriteIdx) — this library never assumes
 * anything about how its output gets rendered.
 */
#ifndef IMGUI_PAINTER_C_H
#define IMGUI_PAINTER_C_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Bumped whenever the C ABI changes. */
int32_t ip_version(void);

typedef struct {
    float x, y;
} ip_vec2;

/* Packed RGBA, one byte per channel, R in the lowest byte — the same packing
 * Dear ImGui's ImU32 uses, so an adapter can hand these through unchanged. */
typedef uint32_t ip_color;

typedef struct {
    ip_vec2 min, max;
} ip_rect;

/* One tessellated output vertex. Field order/layout matches ImDrawVert
 * exactly (pos, uv, col) so an ImGui adapter's copy is a straight field
 * copy — a convenience for that one adapter, not a dependency in this
 * direction: this header still declares its own type rather than including
 * an ImGui header. */
typedef struct {
    ip_vec2 pos;
    ip_vec2 uv;
    ip_color col;
} ip_vertex;

/* Buffers are owned by the ip_ctx and valid until the next ip_begin (or
 * ip_ctx_destroy) — copy out anything you need to keep past that. */
typedef struct {
    const ip_vertex *vtx;
    int32_t vtx_count;
    const uint16_t *idx;
    int32_t idx_count;
} ip_mesh;

typedef struct ip_ctx ip_ctx;

ip_ctx *ip_ctx_create(void);
void ip_ctx_destroy(ip_ctx *ctx);

/* Start a new paint session: clears the output mesh and records the UV all
 * emitted vertices sample (a solid-white texel in the host's font atlas, so
 * flat-color/gradient fills need no separate "untextured" draw path). */
void ip_begin(ip_ctx *ctx, ip_vec2 white_pixel_uv);

/* Set the shape subsequent fill/shadow/border calls apply to. `radius <= 0`
 * is a plain rectangle. Replaces any shape set earlier in this session. */
void ip_rounded_rect(ip_ctx *ctx, ip_rect rect, float radius);

/* Tessellate the current shape as a solid fill and append it to the mesh. */
void ip_fill_color(ip_ctx *ctx, ip_color color);

typedef enum {
    IP_GRADIENT_LINEAR = 0,
    IP_GRADIENT_RADIAL = 1,
    /* Sweep/conic gradient: t is the angle from `from` to the point being
     * evaluated, offset so `to`'s direction is the sweep's t == 0 start,
     * normalized over one full turn. Has an unavoidable hard seam where
     * t wraps from ~1 back to 0 — see painter.cpp's kGradientEdgeSubdivisions
     * comment for the known (documented, not fixed this pass) triangle
     * artifact right at that seam. */
    IP_GRADIENT_ANGULAR = 2,
    /* Concentric diamond iso-lines, scaled per-axis by the from->to box
     * (t == max(|dx|/|to.x-from.x|, |dy|/|to.y-from.y|)) so `to` still
     * means "this is the t == 1 edge", consistent with Linear/Radial. */
    IP_GRADIENT_DIAMOND = 3,
} ip_gradient_mode;

/* `t` positions are expected ascending across a gradient's `stops` array;
 * behavior for an unsorted array is unspecified. */
typedef struct {
    float t;
    ip_color color;
} ip_color_stop;

typedef struct {
    ip_gradient_mode mode;
    /* Linear: axis endpoints — color is stops[0] at `from`, stops[last] at
     * `to`, interpolated by projection onto the from->to axis.
     * Radial: `from` is the center; `to` sets the radius (its distance from
     * `from`) — color is stops[0] at the center, stops[last] at that radius
     * and beyond. */
    ip_vec2 from, to;
    const ip_color_stop *stops;
    int32_t stop_count;
} ip_gradient;

/* Tessellate the current shape with a multi-stop gradient fill (`stop_count
 * == 0` is a no-op; `== 1` fills solid with that one stop's color). */
void ip_fill_gradient(ip_ctx *ctx, const ip_gradient *gradient);

/* Append a straight segment from `a` to `b`, `thickness` px wide, as a quad.
 * Independent of the current shape, so it composes with fills in one
 * accumulation. Not anti-aliased (see painter.cpp's ponytail): pixel-exact
 * for axis-aligned integer-width lines, a hard edge for diagonals. */
void ip_line(ip_ctx *ctx, ip_vec2 a, ip_vec2 b, float thickness, ip_color color);

typedef struct {
    ip_vec2 offset;
    float blur;
    float spread;
    ip_color color;
    /* Accepted for forward ABI compatibility; not implemented yet (outer
     * shadows only this pass) — ip_add_shadow ignores it. */
    bool inset;
} ip_shadow;

typedef struct {
    float thickness;
    ip_color color;
} ip_border;

/* Rasterize a soft shadow around the current shape and append it to the
 * mesh. Paint order follows call order: call ip_add_shadow before ip_fill_*
 * to put the shadow behind the fill. Stackable — call it more than once for
 * layered shadows. */
void ip_add_shadow(ip_ctx *ctx, const ip_shadow *shadow);

/* Stroke the current shape's outline. Thickness below 1px is drawn at 1px
 * with proportionally reduced alpha (a hairline approximation — real
 * sub-pixel geometry rasterizes unreliably without MSAA). */
void ip_add_border(ip_ctx *ctx, const ip_border *border);

/* Finish the session and return the accumulated mesh. */
ip_mesh ip_end(ip_ctx *ctx);

#ifdef __cplusplus
}
#endif

#endif /* IMGUI_PAINTER_C_H */
