# Recipes and palettes

`recipes` is an optional convenience layer. It turns a small set of palette
tokens into `Material`s and painted surfaces, so a host does not hand-author
every gradient and border.

Nothing else depends on it. If you want full control, build `Material`s directly.

## Palette

Nine tokens describe a chrome scheme:

```rust
Palette {
    surface,         // the default panel/body surface
    surface_raised,  // raised elements: buttons, toolbars
    surface_inset,   // recessed elements: fields, wells
    border_light,    // top bevels and highlights
    border_dark,     // outlines and bottom shading
    accent,          // the one attention color
    selection,       // selected/active state
    text,            // primary text
    text_muted,      // secondary text
}
```

`text` and `text_muted` are in here even though **imgui-painter never renders
text**. They exist so hosts keep typography coherent with the chrome, applied
through stock ImGui style APIs. The crate links only `imgui-sys` and deliberately
owns no `imgui-rs` helper for it.

## Building materials

```rust
let palette = my_palette();
let button   = recipes::raised_button(&palette);
let toolbar  = recipes::toolbar_button(&palette);
let field    = recipes::inset_control(&palette);
let row      = recipes::selected_row(&palette);
let tree     = recipes::browser_tree_row(&palette);
let slider   = recipes::parameter_slider(&palette);
let combo    = recipes::combo_field(&palette);
```

And two that paint directly rather than returning a style:

```rust
recipes::panel(&mut canvas, rect, &palette);        // raised surface
recipes::inset_panel(&mut canvas, rect, &palette);  // recessed well
```

## Bridging to stock ImGui colors

```rust
recipes::apply_imgui_colors(&mut style.colors, &palette);
```

This maps the nine tokens across **every** stock ImGui color role — collapsing
headers, tables, scrollbars, navigation, popup chrome, plots, tabs, and text.

Without it, a host ends up maintaining two parallel palettes: one for
imgui-painter chrome and a hand-written one for everything ImGui still draws
itself. They drift. This call is what keeps decorated and undecorated surfaces
looking like the same application.

> **Version note:** this function names color roles including
> `ImGuiCol_NavCursor`, `ImGuiCol_TextLink`, and the `TabSelected`/`TabDimmed`
> family, which do not exist in older Dear ImGui. It is a **compile-time**
> version dependency — see [the compatibility contract](decorators/contract.md).

## Known caveat: hover on light palettes

Recipe hover derivation lifts the fill about 10% toward white (roughly one
surface step). That was validated on a dark palette, where it reads clearly.

**On light palettes it is close to imperceptible.** A host using a light scheme
should override hover explicitly — mixing toward the selection token works well:

```rust
let mut material = recipes::toolbar_button(&palette);
material.fill.hover = mix(palette.surface_raised, palette.selection, 0.18);
```

A contrast-aware (rather than fixed-tint) hover derivation is a known
improvement that has not been made yet. Until then, treat recipe hover as a
starting point on light schemes rather than a finished value.

## When to stop using recipes

Recipes cover conventional desktop chrome. Once a host wants something
specific — a multi-stop gloss, a particular bevel, a layered rack look — build
the `Material` directly or paint with a `Canvas`. The recipes are a shortcut, and
outgrowing them is expected rather than a problem.
