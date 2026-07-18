# Contributing to imgui-painter

## Building

The Rust crate's `build.rs` compiles the C++ core with the [`cc`](https://docs.rs/cc)
crate — the same mechanism `imgui-sys` uses to compile cimgui. You need a C++17
compiler: Clang via Xcode Command Line Tools on macOS, GCC/Clang via `apt` on
Linux, MSVC via the Visual Studio Build Tools on Windows. Linux additionally
needs `libgtk-3-dev` for the `winit`/`wgpu` dev-dependencies used by the examples.

```sh
cargo build
cargo test
cargo build --examples
```

## The quality bar

Every change must pass all of these, in order:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --no-default-features
cargo build --examples
```

`--no-default-features` is not a formality. It disables the `decorators` feature
and is the executable proof that the painter core stays independent of any
particular Dear ImGui version. If a change makes the core fail to compile without
decorators, the layering has been broken — fix the layering, don't relax the test.

Automated tests cover mesh geometry, lifecycle cleanup, and composition
invariants. They do **not** cover final rasterized appearance, which is why the
visual gate below exists and cannot be skipped.

## The visual gate

```sh
cargo run --example painter_demo
```

The demo renders three hand-built looks (a macOS-style panel, a Fluent-style
button, a GitHub-style button), stock Button/Selectable/Checkbox/InputText
widgets, the layered-chrome state row, the Slider/Combo/TreeNode gallery, and
the Ableton-inspired recipe rack.

A human must run it at all three scales:

```sh
IMGUI_PAINTER_DEMO_UI_SCALE=1.0 cargo run --example painter_demo
IMGUI_PAINTER_DEMO_UI_SCALE=1.5 cargo run --example painter_demo
IMGUI_PAINTER_DEMO_UI_SCALE=2.0 cargo run --example painter_demo
```

`IMGUI_PAINTER_DEMO_UI_SCALE` is a demo-only *logical* scale. Framebuffer scale
stays at the real host value so renderer/scissor coordinates remain valid and
physical-pixel behavior is exercised independently.

Confirm at every scale that:

- inner shadows stay clipped,
- bevel and gloss bands follow rounded corners,
- stacked borders remain visually distinct,
- pressed chrome reads as inset,
- focus reads as focus rather than as hover,
- hairlines stay crisp at the current display scale.

And that these remain **stock ImGui behavior**, unchanged by decoration:

- Slider dragging and keyboard/temporary input,
- Combo popup selection, including stock Button/InputText chrome inside the popup,
- TreeNode disclosure and navigation.

## Dependency-bump checklist

Reconstructed part geometry is **not a stable upstream contract**. It can
silently desynchronize on a source-compatible bump — the code still compiles, the
paint just lands in the wrong place. This checklist is what stands between that
and a release.

The authoritative compatibility target is **Dear ImGui 1.91.9b** through imgui-rs
fork revision `7a89260c79ad1f9d4bfe81d6ca1b76ad38a6b3e3`.

On any `imgui` / `imgui-sys` source or version bump:

1. Rerun the full visual gate at 1×, 1.5×, and 2×.
2. Re-verify the original four widgets (Button, Selectable, Checkbox, InputText).
3. Re-verify Slider frame/fill/grab alignment and temporary input.
4. Re-verify Combo popup lifecycle with both visible and hidden labels.
5. Re-verify non-leaf and leaf TreeNode disclosure alignment.
6. Re-verify disabled alpha and physical-pixel hairlines.
7. Refresh the Slider formula tests and the screenshots in `docs/screenshots/`.
8. Update `ANATOMY_COMPATIBILITY` and `ANATOMY_IMGUI_VERSION` in
   `bindings/rust/src/item_paint.rs`.
9. Update [`VERIFIED_IMGUI_SYS`](VERIFIED_IMGUI_SYS).

### VERIFIED_IMGUI_SYS is a CI gate, not a note

[`VERIFIED_IMGUI_SYS`](VERIFIED_IMGUI_SYS) holds the imgui-rs fork revision that a
human last ran the checklist against. CI resolves `imgui-sys` fresh, extracts the
resolved revision, and **fails the build** when it differs. Upstream point
releases trip it too — that is the intended behavior, not a false positive.

The only correct way to make that failure go away is to run the checklist above
and then update the file. Editing the file to match a new revision without
running the gate defeats the entire mechanism.

## Architecture rules

These are the invariants the design depends on. Breaking one is a design change,
not an implementation detail.

1. **The C++ core never links Dear ImGui or cimgui.** Pure math in, a generic
   vertex/index mesh out. If a change to `src/painter.cpp` needs an ImGui type,
   it belongs in the adapter instead.
2. **The adapter writes only through public prim APIs** — `PrimReserve`,
   `PrimWriteVtx`, `PrimWriteIdx` — and never touches `ImDrawList`'s internal
   buffers. `ImDrawList`'s public *fields* are stable to read, but its
   *invariants* (write-pointer bookkeeping, texture/clip-rect stacking,
   large-mesh vertex-offset handling) are maintained by those methods, are not
   covered by any ABI guarantee, and have changed across Dear ImGui versions.
3. **One `imgui-sys` build.** Cargo must resolve the adapter and the host's
   imgui-rs to a single `imgui-sys`, or there are two ImGui instances. Check with
   `cargo tree -d | grep imgui-sys` after any dependency change.
4. **Decorators preserve the last item.** After a `decorate_*` call returns, the
   submitted widget must still be ImGui's last item, with its ID, bounds, hover,
   and active queries intact. This is a public compatibility contract with an
   executable regression test; hosts attach tooltips, context menus, and
   drag/drop immediately after decorated calls.
5. **The steady state allocates nothing on the Rust side.** Guarded by
   `bindings/rust/tests/zero_alloc.rs`.

## Documentation

Prose documentation lives in `book/` (mdBook) and is published to GitHub Pages
alongside rustdoc. Build it locally with:

```sh
cargo install mdbook
mdbook serve book
```

If a change alters the public API, the compatibility contract, or the anatomy
formulas, update the book in the same change — those pages are the reason the
reasoning survives.
