# imgui-painter

A rendering and styling toolkit for Dear ImGui that makes high-quality
visuals as easy as `PushStyleColor`, without replacing ImGui's widget,
layout, or input systems.

**Not** a design system, not Qt, not CSS, not a widget-set replacement.
Closer to a 2D rendering framework specialized for Dear ImGui than to "a
styling helper."

## Status: Phase 4B — Material + Decorator

The core, Rust adapter, tests, benchmarks, and examples live under this
directory and build independently from any host application. The crate is
still unpublished while its API develops.

Phase 1 (design doc §12 step 1 — the go/no-go gate for everything else) is
done: `Painter` only, `rounded_rect` + `fill_color` / `fill_gradient`
(linear, radial) / `add_shadow` (stackable) / `add_border` (with hairline
alpha compensation), validated by the three-looks visual gate below.

Phase 2 hardened that same low-level layer before moving up the design
doc's phase list: adaptive (error-bounded) rounded-rect tessellation,
Angular and Diamond gradients, benchmarks against equivalent hand-written
tessellation (see the [comparison table](#comparison-imgui-painter-vs-handwritten-imdrawlist)
below), and a header-only C++ fluent wrapper (`include/imgui_painter.h`)
alongside the Rust binding.

Phase 3 introduced the long-lived `Painter` → per-frame `Frame` → per-draw-list
`Canvas` ownership chain used by host applications.

Phase 4A **prototyped the item-paint bracket (§5)** — the mechanism that
restyles a *stock* `ImGui::Button()`/`Selectable()` with no wrapper widget:
a 3-channel draw-list split (Background → Widget → Overlay), the widget's own
frame colors (all interaction states) pushed transparent, and the decoration
painted behind the widget.

Phase 4B graduates that prototype into the ImGui-aware Rust adapter API:
`Material` holds the minimal shared radius/fill/border/shadow inputs,
`Decorator` maps Button and Selectable to their ImGui color slots, and
`item_paint` preserves the stock widget's layout, input, text, and return value.
The ImGui-free core remains unchanged.

Still deferred to Phase 5: `Resolver`, `Recipe`, themes, `PushMaterial`, the
ergonomic scope-guard API, additional widgets, typography, overlays, and the
Paint Debugger. A polished gallery is a later design milestone; the current
`painter_demo` remains a development sandbox.

Run the visual gate:

```
cargo run -p imgui-painter --example painter_demo
```

It renders three hand-built looks (a macOS-style panel, a Fluent-style
button, a GitHub-style button), each next to a plain-`ImDrawList` attempt at
the same look, so a human can judge whether Painter alone renders
convincingly. That judgment — not a test suite — is phase 1's actual pass
criterion; the automated tests below cover mesh-generation correctness,
not visual quality.

## Comparison: imgui-painter vs. handwritten ImDrawList

`bindings/rust/benches/tessellation.rs` benchmarks `Session`-driven mesh
generation against `benches/handwritten.rs`, a from-scratch pure-Rust
tessellation with no FFI and no generic gradient dispatch — both producing
the same macOS-panel look (rounded rect + soft shadow + linear gradient
fill + 1px border) from `painter_demo.rs`. Run it yourself with
`cargo bench -p imgui-painter`; one measured run on this machine:

| | imgui-painter (`Session`) | handwritten `ImDrawList`-style Rust |
|---|---|---|
| Source lines for this one look | ~42 ([`draw_macos_panel_painted`](bindings/rust/examples/painter_demo/main.rs)) | ~213 ([`benches/handwritten.rs`](bindings/rust/benches/handwritten.rs)) |
| Tessellation | adaptive, error-bounded (`CornerSegments`) | fixed 8 segments/corner |
| Shadow rings | one `add_shadow` call | hand-rolled ring loop + falloff math |
| Gradient | generic 4-mode `GradientT` dispatch | inlined 2-stop linear lerp only |
| Border ordering | handled by call order | handled by call order (same shape, rewritten) |
| Mesh size (this look) | 433 vtx / 1260 idx | 553 vtx / 1620 idx |
| Time per mesh | ~6.96 µs (6.91–7.02 µs) | ~8.78 µs (8.56–9.05 µs) |

The adaptive tessellation formula picks fewer segments than a fixed
per-corner count wherever the shape's actual radii don't need more —
smaller mesh *and* less time to generate it, even after crossing the
Rust↔C++ FFI boundary the handwritten version doesn't have to. This isn't a
universal result (a shape with large radii and a fixed-count implementation
tuned lower would tessellate faster and coarser); it's this one look, which
is also the one the visual gate already validated as looking correct.

## Architecture

```
imgui-painter core     (C++, compiled via cc; ZERO Dear ImGui / cimgui
                        dependency — pure math in, a generic vertex/index
                        mesh out)
        ↑ C API (capi/imgui_painter_c.h)
imgui-painter Rust adapter  (bindings/rust — copies the core's mesh into a
                        real ImDrawList through imgui-sys's own public
                        PrimReserve/PrimWriteVtx/PrimWriteIdx calls, never
                        by touching ImDrawList's internal buffers directly)
        ↑
host app (via imgui-rs/imgui-sys)
```

The core never links against Dear ImGui or cimgui — a host's adapter rides
whichever ImGui build the host app already linked. Cargo resolves the adapter
and imgui-rs 0.12 to one shared `imgui-sys` build, so there is never a second
ImGui instance or an ABI-layout guess.

This was a deliberate design decision, not the initial one: an earlier plan
had the core write directly into `ImDrawList`'s vertex/index buffers to
avoid any ImGui dependency at all. That was rejected because `ImDrawList`'s
public *fields* are stable to read but its *invariants* (write-pointer
bookkeeping, texture/clip-rect stacking, large-mesh vertex-offset handling)
are maintained by methods like `PrimReserve`, are not part of any ABI
guarantee, and have changed across Dear ImGui versions. The core/adapter
split gets the same "core has zero ImGui dependency" property without
taking on responsibility for invariants only Dear ImGui itself is entitled
to change.

## Repo layout

```
imgui-painter/
  include/imgui_painter.h   header-only C++ fluent wrapper
                             (Painter(rect).fill(...).shadow(...)
                             .border(...).draw(dl)) over capi/ — a
                             template draw() keeps it ImGui-dependency-free
                             too; see bindings/rust/tests/
                             fluent_header_mock.cpp for its compile-check
  capi/imgui_painter_c.h    the C ABI — every language binding compiles
                             against this
  capi/imgui_painter_c.cpp  ip_version() only; the real implementation
                             lives in src/
  src/painter.cpp           the core: tessellation, gradients, shadows,
                             borders — zero ImGui dependency
  bindings/rust/            the Rust adapter + safe wrapper (this phase's
                             only binding)
  bindings/rust/benches/    Session vs. handwritten-Rust tessellation
                             benchmark (cargo bench -p imgui-painter)
  bindings/rust/tests/      fluent_header_compiles.rs + the mock .cpp it
                             drives — proves include/imgui_painter.h
                             compiles standalone
  bindings/rust/examples/   basic usage + the painter_demo development sandbox
  README.md                 this file
```

## Building

`bindings/rust`'s `build.rs` compiles the C++ core with the [`cc`](https://docs.rs/cc)
crate — the same mechanism `imgui-sys` itself uses to compile cimgui. Needs a
C++17 compiler; the supported desktop platforms ship one
(GCC/Clang on Linux/macOS via `apt`/Xcode CLT, MSVC on Windows via the
Visual Studio Build Tools). `cargo test` also compiles
`include/imgui_painter.h` against a mock draw-list
(`bindings/rust/tests/fluent_header_compiles.rs`) to catch header rot;
`cargo bench -p imgui-painter` is separate (not part of the default test
run) and produces the numbers in the [comparison table](#comparison-imgui-painter-vs-handwritten-imdrawlist)
above.

To prove the tree is self-contained, copy `imgui-painter/` anywhere outside a
host workspace and run:

```
cd imgui-painter/bindings/rust
cargo build --examples
cargo test
```

The repository CI performs exactly this independent-copy check.

## Future repository split

Once later phases (Resolver, Recipe, themes, component library) are built
out here, this directory becomes its own repository: `include/`, `capi/`,
`src/` move as-is, `bindings/rust` becomes a published crate, and
`bindings/{c,zig,csharp,python}` land as separate binding crates against
the same `capi/imgui_painter_c.h` surface.
