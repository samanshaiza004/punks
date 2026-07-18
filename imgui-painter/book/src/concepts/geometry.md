# Geometry and composition

Painting is a sequence of operations against the `Canvas`'s current shape. You
set a shape, then apply fills, shadows, and borders to it, in order.

```rust
canvas.rounded_rect(rect, radius);   // set the shape
canvas.add_shadow(&outer_shadow);    // then compose onto it
canvas.fill_gradient(&surface);
canvas.add_border(&outer_border);
```

## Shapes

`rounded_rect(rect, radius)` is the workhorse. A radius of `0.0` gives a plain
rectangle, so there is no separate call for it.

Tessellation is **adaptive and error-bounded**: the segment count per corner is
derived from the corner's actual radius against a flatness tolerance, rather than
being a fixed constant. A 2px corner does not get the same 8 segments as a 20px
corner. This produces smaller meshes than a fixed-count approach at equal or
better visual quality — see [Benchmarks](../appendix/benchmarks.md).

## Fills

- `fill_color(color)` — flat fill.
- `fill_gradient(&gradient)` — a multi-stop gradient in one of four modes:
  **Linear**, **Radial**, **Angular**, and **Diamond**. Stops are `(t, color)`
  pairs with `t` in `0.0..=1.0`.
- `fill_band_color(from_y, to_y, color)` and
  `fill_band_gradient(from_y, to_y, &gradient)` — fills clipped to a horizontal
  band, still respecting the shape's rounded corners.

Bands are what make gloss and bevel effects possible without a second shape. A
gloss highlight across the top third of a button is a band, not a separate
rounded rect that you have to keep aligned with the first one.

## Shadows

`add_shadow(&shadow)` takes an offset, blur, spread, color, and an `inset` flag.

- **Outer** shadows (`inset: false`) read as elevation.
- **Inset** shadows (`inset: true`) read as recession, and are what makes a
  pressed state look genuinely pushed in rather than merely darker.

Shadows stack. Calling `add_shadow` twice applies both, in order.

## Borders

- `add_border(&border)` — a border on the current shape's edge.
- `add_border_inset(distance, &border)` — a border inset by `distance`, which is
  how you stack multiple visually distinct outlines.

Borders honor **hairline alpha compensation**. A border thinner than one physical
pixel cannot be drawn thinner, so instead its alpha is scaled proportionally. A
0.5px border becomes a 1px border at half alpha, which is visually what a
sub-pixel line should look like — and, importantly, it stays consistent across
display scales.

## Device pixels

```rust
let hairline = canvas.device_pixel();
canvas.fill_band_color(rect.min.y + hairline, rect.min.y + hairline * 2.0, highlight);
```

`device_pixel()` is one physical pixel in logical units. Use it anywhere you mean
"the thinnest crisp line", rather than hard-coding `1.0` — on a 2× display, `1.0`
logical unit is two physical pixels, and a bevel drawn that way looks twice as
heavy as intended.

## Composition is ordered, not declarative

The full layered-chrome idiom looks like this:

```rust
canvas.rounded_rect(rect, radius);
canvas.add_shadow(&outer_shadow);                                    // elevation
canvas.fill_gradient(&surface);                                      // base
canvas.fill_band_gradient(top, gloss_end, &gloss);                   // gloss
canvas.fill_band_color(top, top + canvas.device_pixel(), highlight); // bevel
canvas.add_shadow(&inset_shadow);                                    // depth
canvas.add_border(&outer_border);                                    // outline
canvas.add_border_inset(canvas.device_pixel(), &inner_border);       // inner outline
```

Every line is a draw operation whose effect depends on the ones before it. That
is the whole model. There is no state to push and pop, no cascade to resolve, and
nothing that happens implicitly between two adjacent calls — which is what makes
this readable at all compared to the equivalent hand-written geometry.
