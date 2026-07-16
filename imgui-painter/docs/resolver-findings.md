# Resolver findings from Phases 5 and 8

Phase 5 reused one `Material` across four stock Dear ImGui widgets. Its purpose
was to collect evidence for Phase 6, not to design a Resolver in advance.

## Widget evidence

| Widget | Complete ImGui item | Painted chrome | Stock foreground retained | Suppressed colors | Pressure on `Material` |
|---|---|---|---|---|---|
| Button | Frame and label | Complete item rectangle | Label, navigation highlight, interaction, return value | `Button`, `ButtonHovered`, `ButtonActive` | Its `active` color means pressed/held. |
| Selectable | Selectable row | Complete item rectangle | Label, navigation highlight, interaction, return value | `Header`, `HeaderHovered`, `HeaderActive` | Row selection and activation are not represented separately. |
| Checkbox | Box and label | Pre-captured `GetFrameHeight()` square | Checkmark, mixed indicator, label, navigation highlight, interaction, return value | `FrameBg`, `FrameBgHovered`, `FrameBgActive` | It has multiple parts, and checked/mixed state is absent from `Material`. Effects that fit a button may intrude on the adjacent label. |
| InputText | Frame and visible label | Pre-captured `CalcItemWidth()` by frame-height rectangle | Text, hint, selection, caret, clipping, navigation highlight, editing, return value | `FrameBg`, `FrameBgHovered`, `FrameBgActive` | Focus/editing is overloaded onto `active`; text, hint, caret, selection, and focus treatment are distinct parts. |

## Decorator matching is a correctness contract

`item_paint` remains `unsafe` because callers must provide a live current ImGui
context on its owning thread, use the current frame and window draw list, and
avoid an existing channel split. Those obligations protect raw FFI and draw-list
invariants.

Matching the `Decorator` to the single stock widget emitted by the closure is a
separate correctness obligation. A mismatch is not expected to cause memory
unsafety. It normally suppresses the wrong ImGui color family, leaving stock
chrome visible while custom chrome is painted behind it. The result is quietly
wrong or double-painted pixels.

Dear ImGui exposes generic last-item identity, bounds, flags, and interaction
queries, but no stable public widget-type tag that can enforce this match.
Aspect-ratio, item-ID, and size guesses would both reject valid layouts and miss
real mismatches, so Phase 5 deliberately adds no heuristic assertion. This is
pressure for a structurally safer typed Phase 6 API.

## Chrome geometry is version-coupled

Checkbox and InputText chrome rectangles are reconstructed before widget
submission from public layout functions:

- Checkbox: cursor screen position plus a `GetFrameHeight()` square.
- Single-line InputText: cursor screen position plus `CalcItemWidth()` and
  `GetFrameHeight()`.

The functions are public, but the assumption that these formulas reproduce a
specific widget's internal chrome is not a stable public contract. Dear ImGui
does not expose the geometry of individual widget parts. An ImGui or imgui-sys
upgrade can therefore compile cleanly while silently moving the stock widget
away from the captured paint rectangle.

Every ImGui/imgui-sys dependency bump must rerun the `painter_demo` visual gate,
including Checkbox label exclusion and InputText label, hint, caret, selection,
clipboard, hover, and focus behavior. Automated assertions cover finite,
non-negative, contained geometry; they cannot prove visual alignment with
upstream widget internals.

## Shared state has different meanings

`ItemState::active` is not one visual concept:

- Button: pressed/held.
- Selectable: currently activated by interaction, not persistent selection.
- Checkbox: pressed/held; checked and mixed are separate states.
- InputText: focused and editing, potentially for many frames.

Using one `StateColors::active` value for all four is useful evidence, not a
finished semantic model. In particular, a pressed-button color may be unsuitable
as a persistent InputText focus treatment.

## Phase 8 breadth evidence

Phase 8 added three anatomy classes while leaving `Material` unchanged:

| Widget | Private anatomy | Stock foreground/behavior retained | Suppressed colors | New pressure |
|---|---|---|---|---|
| Slider (`f32`, horizontal, linear) | Frame, thin track, completed fill, reconstructed grab | Grab, formatted value, label, navigation, drag/keyboard/temp-input behavior | `FrameBg*` | Track/fill/grab need separate appearance. Active conflates drag, keyboard adjustment, and temporary input. |
| Combo (standard preview + arrow) | Frame, preview region, arrow region | Preview text, arrow glyph, label, navigation, popup and selection | `FrameBg*`, `Button*` | Popup-open is persistent state distinct from pressed/focused; arrow region wants its own appearance. |
| TreeNode (unframed span row) | Row and optional disclosure slot | Arrow, label, indentation, navigation, children and open state | `Header*` | Selected/open/pressed are distinct; leaf rows remove disclosure but may preserve label spacing; icons need a content contract. |

### Slider configuration is duplicated

`decorate_slider_f32` and its stock-widget closure both describe the value,
range, and slider mode. Dear ImGui exposes no stable post-item metadata carrying
the submitted range, orientation, or flags. A closure can therefore submit a
different slider while the decorator reconstructs geometry from its declared
arguments; the resulting fill may look plausible while being wrong. Matching
the same value/range and a horizontal linear `f32` Slider remains an explicit
caller correctness contract, not a structurally enforced guarantee.

### Parent item and token lifecycle are separate stages

`BeginCombo()` switches the current window to its popup before returning an open
token. Combo parent state is therefore captured after popup completion, when
`EndCombo()` restores the parent window's backed-up last-item data. The
decorator owns two explicit stages: transparent parent-frame colors surround
only `BeginCombo`, then ordinary style colors are restored before caller popup
contents run. It drops the token, captures the restored parent state, paints
through the captured parent draw list, and finally merges the parent channels.

An open TreeNode does not switch windows: it establishes indentation/ID state
and returns its token while the parent item remains current, so its state is
captured immediately after the stock call and parent channels merge before
children run. Private RAII guards pop suppressed colors and merge channels
during unwinding so a panic cannot strand either stack.

### Logical scale is not framebuffer scale

Widget geometry—font/frame height, `GrabMinSize`, item width, and the Slider's
upstream-fixed 2.0-unit grab padding—lives in Dear ImGui's logical coordinate
space. Framebuffer scale only converts logical geometry to physical pixels;
imgui-painter uses it for hairlines and the custom track's two-physical-pixel
minimum. Tests vary those inputs independently so a crisp raster result cannot
hide incorrect logical anatomy.

### Style alpha is integration state, not a Material variant

The item decorator multiplies custom fill, border, and shadow alpha by the
current ImGui style alpha exactly once. `BeginDisabled` therefore dims painter
chrome consistently with stock text/foreground without pretending `Material`
has a dedicated disabled semantic. A future part-style model may still need a
real disabled appearance rather than alpha alone.

### Executable compatibility boundary

Private constants pin anatomy reconstruction to Dear ImGui 1.89.2, imgui-rs
0.12, and imgui-sys 0.12.0. Tests compare `igGetVersion()` with that pin, while
independent-build CI compares the freshly resolved imgui-sys package with
`VERIFIED_IMGUI_SYS`. Updating either dependency requires rerunning Slider
formula tests, Combo lifecycle, TreeNode leaf/disclosure alignment, and the
1×/1.5×/2× visual gate.

## Resolver verdict and future requirements

The repeated bracket, geometry validation, and state capture now justify a
**private anatomy-resolution boundary**. They do not justify a public Resolver:
the geometry is upstream-version-coupled, and no application/custom-widget
consumer currently needs to provide or extend anatomy.

A future public part-style model must solve only requirements demonstrated here:

1. Map each widget's anatomy into named parts rather than treating every item as
   one rectangle.
2. Allow part-specific appearance for checkbox box/checkmark/label and InputText
   frame/text/hint/caret/selection/focus treatment.
3. Distinguish pressed, focused/editing, selected, checked, mixed, hovered, and
   disabled semantics where a widget actually exposes them.
4. Give Slider track/fill/grab, Combo frame/arrow, and Tree row/disclosure/icon
   distinct appearance without turning `Material` into one universal bag.
5. Make duplicated widget configuration (especially Slider range/flags and
   Tree leaf/selected flags) structurally harder to mismatch where possible.
6. Keep explicit ImGui-version compatibility coverage for every reconstructed
   part.

This document intentionally defines no public Resolver structs, traits,
builders, or Material expansion. Public Resolver, part styles, checked/focus
styling, multiline InputText, Combo variants, generic sliders, icons, Recipe,
themes, `PushMaterial`, and typography remain deferred.

## Upstream contracts consulted

- [Rust Reference: `unsafe`](https://doc.rust-lang.org/stable/reference/unsafe-keyword.html)
- [Dear ImGui public API and item queries](https://github.com/ocornut/imgui/blob/master/imgui.h)
- [Dear ImGui widget implementations](https://github.com/ocornut/imgui/blob/master/imgui_widgets.cpp)
