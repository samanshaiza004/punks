# Painter, Frame, Canvas

Three types form an ownership chain that matches how an immediate-mode host
actually runs: one long-lived object, one per frame, one per draw list.

```
Painter   long-lived    owns the native painter context
   └─ Frame   per frame    knows this frame's framebuffer scale
        └─ Canvas  per draw list   the thing you actually paint with
```

## Painter

Create one at startup and keep it for the life of the application. It owns the
native context behind the C ABI, so creating one per frame would mean allocating
and freeing native state 60+ times a second for no reason.

## Frame

```rust
let mut frame = painter.begin_frame();
```

A `Frame` is scoped to a single ImGui frame. Its job is to know the current
**framebuffer scale**, which is what makes hairlines land on physical pixels
instead of blurring across two of them.

The ImGui-aware path samples `DisplayFramebufferScale` automatically. Direct
C/C++/`Session` consumers set their host scale explicitly, because there is no
ImGui to ask.

`Frame` is borrowed mutably by decorators, which is why hosts with deep call
trees end up threading `&mut Frame` through their draw functions.

## Canvas

```rust
let draw_list = imgui::sys::igGetWindowDrawList();
let mut canvas = frame.canvas(draw_list);
```

A `Canvas` binds a `Frame` to one specific `ImDrawList`. All painting happens
through it. Because it is tied to a draw list, painting into a different window
means getting a different `Canvas`.

`canvas.device_pixel()` returns the size of one physical pixel in logical units —
use it for hairlines and bevels rather than hard-coding `1.0`.

## Draw order is submission order

Immediate mode has no z-index. Whatever you submit first is behind whatever you
submit next.

For a panel with widgets on it, that means:

1. Paint the panel surface with a `Canvas`.
2. *Then* submit the widgets.

Hosts building nested surfaces (a window containing panes containing rows) end up
painting parent rectangles first, then submitting transparent child regions. That
ordering requirement is real and is the main structural thing to plan for when
adopting the toolkit.

## Zero allocations in the steady state

Once running, the Rust side of a frame allocates nothing. Meshes are written into
reused buffers and copied into the draw list; there is no per-frame `Vec` churn
behind the painting API.

This is enforced, not aspirational: `bindings/rust/tests/zero_alloc.rs` installs a
process-wide counting global allocator and asserts a zero steady-state
allocation count per frame. If a change introduces a per-frame allocation, that
test fails.

The caveats are documented in the test itself — it measures the painting path's
steady state, not application code that happens to allocate its own labels.
