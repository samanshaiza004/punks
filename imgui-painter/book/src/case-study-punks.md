# Case study: punks2

[punks2](https://github.com/samanshaiza004/punks2) is a keyboard-first sample
browser for musicians. It was the first real application built on imgui-painter,
and the first honest test of whether the toolkit is pleasant to use by someone
who is not its author.

The full findings, including per-frame allocation measurements, live in
[`docs/punks-redesign-findings.md`](https://github.com/samanshaiza004/imgui-painter/blob/main/docs/punks-redesign-findings.md).
This chapter is the summary worth reading before adopting the library.

## What was built

An entire application's chrome — tabs, toolbar, breadcrumbs, transport, sidebar,
inspector, modals, popups, fields, rows, and a waveform display — across roughly
40 decorated widget sites, plus painted panel surfaces.

## Did it work?

Yes, and with a specific, checkable result: **zero raw `ImDrawList` geometry in
application code**, CI-enforced by a grep that fails the build if product code
calls `add_rect`, `add_line`, and friends directly.

That gate is the real finding. It means every visual treatment the application
wanted was expressible through the toolkit. Where it was not immediately
expressible, the answer was to improve the library rather than to hand-roll
geometry in the app — and the library never had to be bypassed.

## What the application drove into the library

Two APIs exist because punks2 needed them twice:

- **`recipes::apply_imgui_colors`.** Before it, punks2 maintained a second,
  hand-written palette for everything ImGui still drew itself (collapsing
  headers, scrollbars, tables, popup chrome). The two drifted constantly. One
  call deleted the duplicate.
- **`decorate_selectable(.., selected, ..)`.** Before it, punks2 kept a parallel
  "selected row" `Material` and swapped it manually, tracking selection in two
  places.

Both are cases where repeated application pressure — not speculation — justified
the API.

## What stayed hard

Honest friction, still open:

- **Mutable `Frame` plumbing.** Decorating widgets inside nested draw helpers
  meant threading `&mut Frame` through several private function signatures that
  otherwise had no interest in painter state. There is no ambient frame access.
- **Light-palette hover.** The recipe hover derivation was validated on a dark
  palette and is nearly invisible on light ones. punks2 overrides
  `fill.hover` toward its selection token. See
  [Recipes](recipes.md#known-caveat-hover-on-light-palettes).
- **Flow geometry is the host's problem.** Building nested surfaces means
  painting parent rectangles first, then submitting transparent child regions. It
  composes correctly, but the host has to know the ordering. A public
  pane/strip abstraction remains unearned without a second consumer.
- **`decorate_combo` cannot nest decorators**, because the `Frame` is mutably
  borrowed for the whole bracket.

## A bug the contract prevented

During the redesign, box-selection across rows fired a native OS file-drag from
every row the cursor crossed — a flickering copy cursor and a blanked UI.

The fix was to detect drags by **press origin** (`is_item_active`, true only for
the row that captured the press) rather than by hover. That fix is only available
because decorators preserve the widget's last-item queries — see
[the compatibility contract](decorators/contract.md#guarantee-the-last-item-is-preserved).
Without that guarantee, the application could not have distinguished the pressed
row from a crossed one.

It is a good illustration of why that contract is tested rather than assumed.

## Performance

Measured with a counting global allocator on debug builds, decorating roughly ten
rows per frame in a real library:

- Idle browsing settled around **1.84–2.05 ms** per draw and **95–116**
  allocations per frame.
- With audio playing and the layered waveform visible, **1.84–2.81 ms** and
  **115–125** allocations.

Per-row decoration cost was within noise. In one comparison the decorated build
allocated *fewer* times per frame than the undecorated baseline, because
decorators ride the zero-allocation canvas path while the widgets they replaced
did not.

These are debug-build observations on one machine, not a benchmark — but they
were enough to establish that decoration is not the thing to optimize first.
