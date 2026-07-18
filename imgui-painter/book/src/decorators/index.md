# How decoration works

A decorator restyles a **stock** ImGui widget. There is no wrapper widget, no
reimplementation, and no fork of ImGui's logic.

```rust
unsafe {
    decorate_button(&mut frame, &material, || ui.button("Save"));
}
```

That is a real `ImGui::Button()`. It keeps its ID, layout, input handling,
keyboard navigation, disabled state, and return value.

## The bracket

Each `decorate_*` call brackets the widget submission with three steps:

1. **Split the draw list into channels.** Background, Widget, Overlay. This lets
   imgui-painter paint *behind* a widget that has not been submitted yet, which
   is otherwise impossible in immediate mode — you do not know the widget's
   rectangle until after it is submitted.
2. **Suppress the chrome it replaces.** The widget's own frame colors are pushed
   transparent across *all* interaction states, so ImGui still draws its text,
   check glyph, and grab, but not the box behind them.
3. **Paint into the Background channel**, then merge.

The result is a widget that ImGui fully owns, wearing chrome that imgui-painter
drew.

## What gets replaced, and what does not

imgui-painter replaces the **frame chrome**: the filled box, its border, its
shadow. ImGui keeps everything else — text, check marks, slider grabs, tree
arrows, popup lifecycle, and all input.

imgui-painter never renders text. Style typography through stock ImGui style
APIs (`push_style_color(StyleColor::Text, ..)`). The `Palette` type includes
`text` and `text_muted` tokens purely so hosts can keep typography coherent with
the chrome, applied by the host.

## Chrome rectangle vs. item rectangle

For several widgets, the chrome is not the whole item.

A `Checkbox` is a box **plus a label**; a single-line `InputText` is a field
**plus an optional visible label**. Painting the full item rectangle would put
the fill behind the label text too, which is wrong.

So the chrome rectangle is captured separately from the complete ImGui item.
Checkbox paint excludes its label; InputText paint excludes its visible label.
This is why the decorators are typed per widget rather than being one generic
function — the anatomy genuinely differs.

## Typed entry points

There is one entry point per widget:

| Function | Widget |
|---|---|
| `decorate_button` | `Button`, `SmallButton` |
| `decorate_selectable` | `Selectable` |
| `decorate_checkbox` | `Checkbox` |
| `decorate_input_text` | single-line `InputText` |
| `decorate_slider_f32` | horizontal linear `SliderFloat` |
| `decorate_combo` | `BeginCombo`/`EndCombo` |
| `decorate_tree_node` | `TreeNode` |

An earlier design exposed a generic `item_paint` plus a `Decorator` enum you
selected yourself. That made decorator/widget mismatch representable — you could
decorate a `Checkbox` with `Decorator::Button` and get plausible-looking wrong
output. The typed entry points make that unrepresentable in normal use; the raw
mechanism is now private.

## Style inputs

`Material` carries the shared inputs — `radius`, `fill` (a `StateColors` triple
of base/hover/active), `border`, and an optional `shadow`.

Widgets with multiple independently-styleable parts take a richer type instead:
`SliderStyle`, `ComboStyle`, and `TreeStyle`. A slider's frame, fill, and grab
are three different surfaces, and collapsing them into one `Material` would mean
they could never differ.

## Selection is explicit

`decorate_selectable` takes persistent selection as a parameter:

```rust
decorate_selectable(&mut frame, &material, is_selected, || {
    ui.selectable_config(&label).selected(is_selected).build()
});
```

Priority is: pressed interaction, then selected, then hovered, then base. Idle
selection reuses the `Material`'s active token; a *pressed* selected row derives
a darker active fill so rows do not lose click feedback while selected.

This parameter exists because an early host had to maintain a parallel
"selected row" `Material` and swap it manually, which meant selection state was
tracked in two places. Passing it in deletes that duplication.

## Safety

The decorators are `unsafe` because they call into `imgui-sys` and require:

1. a current ImGui frame and window, and
2. a closure that submits **exactly one** stock widget item.

Submitting zero or two items breaks the channel-split and chrome-capture logic.
This is a real precondition, not a formality — which is why it is `unsafe` rather
than returning a `Result`.
