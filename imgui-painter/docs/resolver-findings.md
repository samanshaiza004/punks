# Resolver findings from Phase 5

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

## Phase 6 requirements

Phase 6 must solve only requirements demonstrated here:

1. Map each widget's anatomy into named parts rather than treating every item as
   one rectangle.
2. Allow part-specific appearance for checkbox box/checkmark/label and InputText
   frame/text/hint/caret/selection/focus treatment.
3. Distinguish pressed, focused/editing, selected, checked, mixed, hovered, and
   disabled semantics where a widget actually exposes them.
4. Make decorator/widget mismatches structurally harder through typed entry
   points or an equivalent checked API shape.
5. Define explicit ImGui-version compatibility coverage for reconstructed widget
   geometry.

This document intentionally defines no Resolver structs, traits, builders, or
Material expansion. Resolver, typed entry points, checked-state styling,
focus-ring styling, multiline InputText, Recipe, themes, `PushMaterial`,
typography, and scope guards remain deferred.

## Upstream contracts consulted

- [Rust Reference: `unsafe`](https://doc.rust-lang.org/stable/reference/unsafe-keyword.html)
- [Dear ImGui public API and item queries](https://github.com/ocornut/imgui/blob/master/imgui.h)
- [Dear ImGui widget implementations](https://github.com/ocornut/imgui/blob/master/imgui_widgets.cpp)
