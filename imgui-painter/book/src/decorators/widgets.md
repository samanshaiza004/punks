# Widget notes

Per-widget behavior, ownership boundaries, and the gotchas worth knowing before
you reach them.

## Button

```rust
decorate_button(&mut frame, &material, || ui.button("Save"))
```

The simplest case: the chrome rectangle is the item rectangle.

`SmallButton` needs no dedicated decorator — it goes through the same `ButtonEx`
path with zero vertical frame padding, and `decorate_button` covers it.

## Selectable

```rust
decorate_selectable(&mut frame, &material, is_selected, || {
    ui.selectable_config(&label).selected(is_selected).build()
})
```

Takes persistent selection explicitly. Priority: pressed, selected, hovered,
base. A pressed selected row derives a darker active fill so it keeps click
feedback while selected.

Note that ImGui's `Selectable` extends its own highlight by half of
`ItemSpacing.x` on each side. If a decorated row looks a couple of pixels wider
than you expect, that is stock behavior, not the decorator.

## Checkbox

The chrome rectangle is **the box only**, captured separately from the full item
so the fill does not land behind the label. ImGui still draws the check glyph.

There is no `CheckboxStyle` yet — the box uses `Material`. Styling the check
glyph itself is deferred.

## InputText

Single-line only. As with Checkbox, the chrome rectangle excludes the widget's
visible label.

Multi-line `InputTextMultiline` is not supported — it has a different internal
structure (a child window), and decorating it would need separate anatomy.

## Slider

```rust
decorate_slider_f32(&mut frame, &style, value, min, max, || { /* one slider */ })
```

Horizontal linear sliders. `SliderStyle` styles the frame, the filled portion,
and the grab independently, because they are three visually distinct surfaces.

**Gotcha:** the value, min, and max are passed to *both* the decorator and the
widget closure. If they disagree, the fill is drawn at a different fraction than
the widget reports — plausible-looking, entirely wrong output. Pass the same
values to both, ideally from the same variables.

Dragging, keyboard editing, and temporary (ctrl-click) input all remain stock
ImGui behavior.

## Combo

```rust
decorate_combo(&mut frame, &style, || ui.begin_combo(..), |_| { /* contents */ })
```

`ComboStyle` covers the closed field and the arrow area. ImGui keeps the entire
popup lifecycle — opening, sizing, positioning, closing, and selection.

**Limitation:** the contents closure cannot nest other decorators. The `Frame` is
mutably borrowed for the whole combo bracket, so decorated widgets *inside* an
open combo popup are not currently expressible. Stock widgets inside the popup
work normally.

## TreeNode

`TreeStyle` styles the row. Applies to unframed tree nodes; disclosure and
navigation remain stock.

The arrow is ImGui's. Leaf and non-leaf nodes have different disclosure
alignment, which is part of what the visual gate re-verifies on every dependency
bump.

## Not covered

No decorators exist for `CollapsingHeader`, tab bars, `ProgressBar`, or
scrollbars. Style those through stock ImGui colors — `recipes::apply_imgui_colors`
maps a `Palette` across every stock role, which in practice makes stock
`CollapsingHeader` chrome good enough that a dedicated decorator has not been
justified.

Focus rings, disabled-specific appearance, and icons are deferred; disabled
widgets currently inherit ImGui's style alpha.
