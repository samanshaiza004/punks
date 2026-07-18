# The C ABI

`capi/imgui_painter_c.h` is the surface every language binding compiles against.
Rust is simply the first binding, not a privileged one.

Because the core links no Dear ImGui and no cimgui, a binding needs only a C FFI
and a way to hand the resulting mesh to whatever draw list its host uses.

## The shape of it

```c
ip_ctx *ctx = ip_ctx_create();
ip_set_pixel_scale(ctx, 2.0f);

ip_begin(ctx);
ip_rounded_rect(ctx, rect, 5.0f);
ip_add_shadow(ctx, &shadow);
ip_fill_gradient(ctx, &gradient);
ip_add_border(ctx, &border);
ip_mesh mesh;
ip_end(ctx, &mesh);

/* copy mesh.vertices / mesh.indices into your host draw list */

ip_ctx_destroy(ctx);
```

### Functions

| | |
|---|---|
| `ip_version` | ABI version integer |
| `ip_ctx_create` / `ip_ctx_destroy` | context lifetime |
| `ip_set_pixel_scale` | device pixel ratio for hairlines |
| `ip_begin` / `ip_end` | open a build, emit an `ip_mesh` |
| `ip_rounded_rect`, `ip_line` | shapes |
| `ip_fill_color`, `ip_fill_gradient` | fills |
| `ip_fill_band_color`, `ip_fill_band_gradient` | band-clipped fills |
| `ip_add_shadow` | outer and inset shadows |
| `ip_add_border`, `ip_add_border_inset` | borders |

### Types

`ip_vec`, `ip_rect`, `ip_color_stop`, `ip_gradient`, `ip_gradient_mode`,
`ip_shadow`, `ip_border`, `ip_vertex`, `ip_mesh` — all `#[repr(C)]`-compatible
plain data.

## The C++ fluent wrapper

`include/imgui_painter.h` is a header-only fluent wrapper over the same C API:

```cpp
Painter(rect).fill(gradient).shadow(shadow).border(border).draw(dl);
```

Its `draw()` is a **template**, which is what keeps the header itself free of any
ImGui dependency — it will write into anything that exposes the expected
prim-writing interface. That is verified, not assumed: a compile-check test
builds the header against a mock draw list
(`bindings/rust/tests/fluent_header_compiles.rs` and its `.cpp` mock), so header
rot is caught by CI rather than by a downstream consumer.

## Writing a new binding

The contract a binding must uphold:

1. **One context per long-lived painter.** Creating and destroying per frame
   works but wastes native allocations.
2. **Set the pixel scale** from the host's framebuffer scale, or hairlines will
   be wrong on HiDPI displays. There is no ImGui to ask, so the host must tell it.
3. **Copy the mesh through the host draw list's public API.** For Dear ImGui that
   means `PrimReserve` / `PrimWriteVtx` / `PrimWriteIdx` — never a direct write
   into internal buffers. See
   [Architecture](concepts/architecture.md#the-adapter-writes-only-through-public-prim-apis)
   for why this is not negotiable.
4. **Treat `ip_mesh` as borrowed** until the next `ip_begin` on that context.

Widget decoration is *not* part of the C ABI. It is inherently ImGui-version- and
language-specific — it depends on reading ImGui's internal item state — so each
binding implements it against its own ImGui bindings, or omits it entirely and
exposes only painting.

Planned bindings (`bindings/{c,zig,csharp,python}`) all target this same header.
