# imgui-painter

A rendering and styling toolkit for Dear ImGui that makes high-quality visuals as
easy as `PushStyleColor`, without replacing ImGui's widget, layout, or input
systems.

It restyles **stock** widgets. `ImGui::Button()` stays `ImGui::Button()` — same
ID, same layout, same input handling, same return value — it just gets painted
the way you asked.

```rust
let mut frame = painter.begin_frame();
unsafe {
    decorate_button(&mut frame, &material, || ui.button("Decorated Button"));
}
```

**What it is not:** a design system, a widget-set replacement, Qt, or CSS. It is
closer to a 2D rendering framework specialized for Dear ImGui than to "a styling
helper."

> **Status:** pre-release and unpublished. Consume it as a pinned git dependency
> (see [Installation](#installation)). The API is still developing.

## Why

Styling Dear ImGui past a certain point means hand-writing `ImDrawList` geometry,
and hand-written geometry doesn't compose. A shadow, a multi-stop gradient, a
gloss band, a bevel hairline, and three stacked borders are each individually
easy and collectively a mess of ordering bugs and magic numbers, rewritten per
widget.

imgui-painter makes that composition explicit and ordered instead:

```rust
canvas.rounded_rect(rect, radius);
canvas.add_shadow(&outer_shadow);
canvas.fill_gradient(&surface);
canvas.fill_band_gradient(top, gloss_end, &gloss);
canvas.fill_band_color(top, top + canvas.device_pixel(), highlight);
canvas.add_shadow(&inset_shadow);
canvas.add_border(&outer_border);
canvas.add_border_inset(canvas.device_pixel(), &inner_border);
```

No styling language, no cascade, no selector engine — just draw operations that
stack in the order you write them.

## Installation

```toml
[dependencies]
imgui-painter = { git = "https://github.com/samanshaiza004/imgui-painter", rev = "..." }
```

You also need a C++17 compiler, because the core is C++ compiled by
[`cc`](https://docs.rs/cc) at build time — the same mechanism `imgui-sys` uses for
cimgui. Xcode Command Line Tools on macOS, GCC/Clang on Linux, the Visual Studio
Build Tools on Windows.

### Compatibility

The painter core (geometry, gradients, shadows, borders, `Canvas`, `Session`) is
independent of any particular Dear ImGui version.

The **decorators** are not. `decorate_*` reconstructs stock widget chrome
geometry, and `recipes::apply_imgui_colors` names color roles that only exist in
newer ImGui. Both are pinned to:

| | Version |
|---|---|
| Dear ImGui | **1.91.9b** |
| imgui-rs / imgui-sys | 0.12, fork rev [`7a89260`](https://github.com/samanshaiza004/imgui-rs) |

Reconstructed part geometry is not a stable upstream contract: a source-compatible
ImGui bump can compile cleanly and silently move the stock widget away from the
painted rectangle. [`VERIFIED_IMGUI_SYS`](VERIFIED_IMGUI_SYS) records the revision
a human last validated, and CI fails when the resolved revision drifts from it.
See the [dependency-bump checklist](CONTRIBUTING.md#dependency-bump-checklist).

Because Cargo honors `[patch.crates-io]` only from the *consuming workspace root*,
a host that needs the decorators must apply the fork patch itself:

```toml
[patch.crates-io]
imgui = { git = "https://github.com/samanshaiza004/imgui-rs", rev = "7a89260c79ad1f9d4bfe81d6ca1b76ad38a6b3e3" }
imgui-sys = { git = "https://github.com/samanshaiza004/imgui-rs", rev = "7a89260c79ad1f9d4bfe81d6ca1b76ad38a6b3e3" }
```

This is also why the crate is not on crates.io yet — a published crate cannot
force that patch on its consumers.

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
whichever ImGui build the host already linked. Cargo resolves the adapter and
imgui-rs to one shared `imgui-sys` build, so there is never a second ImGui
instance or an ABI-layout guess.

That was a deliberate correction, not the first design. An earlier plan had the
core write directly into `ImDrawList`'s vertex/index buffers to avoid any ImGui
dependency at all. It was rejected because `ImDrawList`'s public *fields* are
stable to read, but its *invariants* — write-pointer bookkeeping, texture and
clip-rect stacking, large-mesh vertex-offset handling — are maintained by methods
like `PrimReserve`, are not part of any ABI guarantee, and have changed across
Dear ImGui versions. The core/adapter split gets the same "core has zero ImGui
dependency" property without taking on responsibility for invariants only Dear
ImGui itself is entitled to change.

The C ABI at `capi/imgui_painter_c.h` is the surface every language binding
compiles against; Rust is simply the first one.

## Comparison: imgui-painter vs. handwritten ImDrawList

`bindings/rust/benches/tessellation.rs` benchmarks `Session`-driven mesh
generation against `benches/handwritten.rs`, a from-scratch pure-Rust
tessellation with no FFI and no generic gradient dispatch — both producing the
same macOS-panel look (rounded rect + soft shadow + linear gradient fill + 1px
border). Run it with `cargo bench`; one measured run:

| | imgui-painter (`Session`) | handwritten `ImDrawList`-style Rust |
|---|---|---|
| Source lines for this one look | ~42 | ~213 |
| Tessellation | adaptive, error-bounded | fixed 8 segments/corner |
| Shadow rings | one `add_shadow` call | hand-rolled ring loop + falloff math |
| Gradient | generic 4-mode dispatch | inlined 2-stop linear lerp only |
| Mesh size (this look) | 433 vtx / 1260 idx | 553 vtx / 1620 idx |
| Time per mesh | ~6.96 µs (6.91–7.02) | ~8.78 µs (8.56–9.05) |

The adaptive formula picks fewer segments than a fixed per-corner count wherever
the shape's actual radii don't need more — smaller mesh *and* less time to
generate it, even after crossing an FFI boundary the handwritten version doesn't
have. This is not a universal result (a shape with large radii, against a fixed
implementation tuned lower, would tessellate faster and coarser); it is this one
look, which is also the one the visual gate validated as looking correct.

## Repo layout

```
include/imgui_painter.h   header-only C++ fluent wrapper over capi/
capi/imgui_painter_c.h    the C ABI — every language binding compiles against this
src/painter.cpp           the core: tessellation, gradients, shadows, borders
bindings/rust/            the Rust adapter + safe wrapper
bindings/rust/benches/    Session vs. handwritten tessellation benchmark
bindings/rust/tests/      includes a compile-check of the fluent C++ header
bindings/rust/examples/   basic usage + the painter_demo visual sandbox
book/                     the prose documentation (mdBook)
docs/                     design findings, case studies, screenshots
```

## Documentation

- **[The book](https://samanshaiza004.github.io/imgui-painter/)** — concepts,
  decorator anatomy, recipes, and the C ABI guide.
- **[API reference](https://samanshaiza004.github.io/imgui-painter/api/imgui_painter/)**
  — rustdoc.
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — quality bar, visual gate, and the
  dependency-bump checklist.
- **[CHANGELOG.md](CHANGELOG.md)** — the phase-by-phase development history.
- **[Resolver findings](docs/resolver-findings.md)** — widget anatomy evidence
  and the `item_paint` safety contract.

## Testing

Automated tests cover mesh geometry, lifecycle cleanup, composition invariants,
and a zero-allocation steady state. They do **not** cover final rasterized
appearance — that is what the human visual gate in
[CONTRIBUTING.md](CONTRIBUTING.md#the-visual-gate) is for.

```sh
cargo test
cargo run --example painter_demo
```

To prove the tree is self-contained, copy it anywhere outside a host workspace and
run those same commands. CI performs exactly this independent-copy check.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
