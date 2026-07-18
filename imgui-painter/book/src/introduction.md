# Introduction

imgui-painter is a rendering and styling toolkit for [Dear ImGui](https://github.com/ocornut/imgui).
It aims to make high-quality visuals as easy as `PushStyleColor`, without
replacing ImGui's widget, layout, or input systems.

## The problem

Dear ImGui's style system covers colors, rounding, spacing, and border sizes.
Past that, styling means writing `ImDrawList` geometry by hand.

Hand-written geometry does not compose. Consider one moderately rich button: a
drop shadow, a multi-stop base gradient, a translucent gloss band across the top
third, a one-physical-pixel bevel highlight, an inset shadow so the pressed state
reads as recessed, and two borders of different colors. Each of those is easy
alone. Together they become an ordering problem, a set of magic numbers, and a
block of code that has to be rewritten — and re-debugged — for the next widget.

Then the display scale changes and the hairlines go blurry.

## The approach

imgui-painter keeps the composition explicit and ordered, without introducing a
styling language:

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

Operations stack in the order you write them. There is no cascade, no selector
engine, and no inheritance to reason about — the reason the snippet above is
readable is precisely that it is not a stylesheet.

On top of that, a decoration layer restyles **stock** widgets:

```rust
unsafe {
    decorate_button(&mut frame, &material, || ui.button("Save"));
}
```

That is a real `ImGui::Button()`. It keeps its ID, its layout, its input
handling, its keyboard navigation, and its return value. imgui-painter suppresses
only the chrome it replaces and paints behind it.

## Non-goals

Being clear about what this is not is most of what keeps it small:

- **Not a widget library.** It adds no widgets. It restyles the ones ImGui has.
- **Not a design system or theme engine.** It has no opinion about your palette
  beyond an optional recipe helper.
- **Not CSS.** No cascade, no selectors, no stylesheet parsing.
- **It does not own layout, input, navigation, text, or popup/tree state.** ImGui
  keeps all of it. imgui-painter never renders text; hosts style typography
  through stock ImGui style APIs.

## How it is layered

The bottom layer is a C++ core with **zero** Dear ImGui dependency: pure math in,
a generic vertex/index mesh out. Above it sits a thin adapter that copies that
mesh into a real `ImDrawList`. See
[Architecture](concepts/architecture.md) for why that split exists and what it buys.

This also means the painting layer is version-agnostic while only the
widget-decoration layer is tied to a specific Dear ImGui release. That
distinction matters in practice, and it is covered in
[The compatibility contract](decorators/contract.md).
