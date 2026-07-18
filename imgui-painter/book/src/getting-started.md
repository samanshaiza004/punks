# Getting started

## Requirements

- A **C++17 compiler**. The core is C++ compiled by [`cc`](https://docs.rs/cc) at
  build time — the same mechanism `imgui-sys` uses for cimgui.
  - macOS: Xcode Command Line Tools (`xcode-select --install`)
  - Linux: GCC or Clang, plus `libgtk-3-dev` if you want to build the examples
  - Windows: Visual Studio Build Tools
- A host already using `imgui-rs` / `imgui-sys` 0.12.

## Adding the dependency

The crate is not on crates.io yet, so depend on it by git:

```toml
[dependencies]
imgui-painter = { git = "https://github.com/samanshaiza004/imgui-painter", rev = "..." }
```

If you use the widget decorators, your workspace root must also pin the imgui-rs
fork that provides the supported Dear ImGui version:

```toml
[patch.crates-io]
imgui = { git = "https://github.com/samanshaiza004/imgui-rs", rev = "7a89260c79ad1f9d4bfe81d6ca1b76ad38a6b3e3" }
imgui-sys = { git = "https://github.com/samanshaiza004/imgui-rs", rev = "7a89260c79ad1f9d4bfe81d6ca1b76ad38a6b3e3" }
```

This has to live in **your** workspace root. Cargo ignores `[patch]` sections
declared by dependencies, so imgui-painter cannot apply it on your behalf. See
[The compatibility contract](decorators/contract.md) for why the pin exists.

After adding it, confirm you have exactly one ImGui:

```sh
cargo tree -d | grep imgui-sys
```

Two `imgui-sys` builds means two Dear ImGui instances, which produces garbage
rendering or a crash. This is the single most important thing to verify when
integrating.

## Decorating your first widget

Create one long-lived `Painter` at startup, then per frame:

```rust
use imgui_painter::{decorate_button, rgba, Border, Material, StateColors};

let material = Material {
    radius: 5.0,
    fill: StateColors {
        base: rgba(45, 108, 223, 255),
        hover: rgba(62, 128, 240, 255),
        active: rgba(35, 88, 190, 255),
    },
    border: Border {
        thickness: 1.0,
        color: rgba(255, 255, 255, 48),
    },
    shadow: None,
};

let mut frame = painter.begin_frame();
// SAFETY: this runs inside the current ImGui window and frame, and the
// closure issues exactly one stock widget item.
unsafe {
    decorate_button(&mut frame, &material, || ui.button("Decorated Button"));
}
```

`decorate_button` returns whatever the closure returns, so the usual `if
ui.button(..)` control flow still works:

```rust
let clicked = unsafe {
    decorate_button(&mut frame, &material, || ui.button("Save"))
};
if clicked {
    save();
}
```

### Why `unsafe`

The decorators call into `imgui-sys` and rely on being inside a live ImGui frame
and window. The contract you are upholding is:

1. There is a current ImGui frame and window.
2. The closure submits **exactly one** stock widget item.

Breaking either is undefined behavior, which is why the call is `unsafe` rather
than merely fallible.

## Painting directly

Not everything is a widget. For panels, strips, and backgrounds, get a `Canvas`
and paint shapes yourself:

```rust
let mut frame = painter.begin_frame();
unsafe {
    let draw_list = imgui::sys::igGetWindowDrawList();
    let mut canvas = frame.canvas(draw_list);
    canvas.rounded_rect(rect, 3.0);
    canvas.fill_color(rgba(40, 44, 52, 255));
    canvas.add_border(&Border { thickness: 1.0, color: rgba(0, 0, 0, 90) });
}
```

Paint parent surfaces **before** submitting the child widgets that sit on them —
draw order is submission order. See [Geometry and composition](concepts/geometry.md).

## Running the demo

The visual sandbox is the fastest way to see what the toolkit can do:

```sh
cargo run --example painter_demo
```

It renders hand-built looks, decorated stock widgets, a layered-chrome state row,
a Slider/Combo/TreeNode gallery, and a recipe rack. It also accepts a demo-only
logical UI scale, which is how compatibility screenshots are produced:

```sh
IMGUI_PAINTER_DEMO_UI_SCALE=1.5 cargo run --example painter_demo
```
