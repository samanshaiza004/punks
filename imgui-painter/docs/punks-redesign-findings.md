# punks redesign findings (imgui-painter as a real application dependency)

The punks2 "Neon Live" redesign (slices R1–R3b, plus the drag-gesture fix)
rebuilt the app's chrome on imgui-painter: theme foundation, ~40 decorated
button/row/field/checkbox sites, recipe-driven materials, and a measured
per-row decoration cost. This document answers two questions: (1) can punks
reach the reference's dense, coherent desktop-tool look; (2) is imgui-painter
pleasant enough that a developer could achieve it without understanding the
library's internals. Entries are classified; not every inconvenience is
something imgui-painter must absorb.

## imgui-painter API/DX gaps

- No bridge between `recipes::Palette` tokens and stock ImGui style colors:
  every non-decorated widget (collapsing headers, progress bar, scrollbars,
  popup chrome, check marks, text) is hand-synced in punks-ui's
  `theme::apply_theme`. The duplication is the single largest DX cost.
- Ambient frame access is absent: decorating buttons in nested sidebar,
  inspector, and modal draw helpers required threading the mutable `Frame`
  through seven private signatures that otherwise did not need painter state —
  mechanical parameter plumbing through the UI call tree.
- Decorators preserve last-item queries after the bracket (`is_item_hovered`,
  `is_item_active`, `item_rect_*`, drag-drop attachment). punks relies on this
  for tooltips, drag-drop, right-click handling, and the drag-out fix — it is
  an undocumented guarantee that deserves a documented contract.
- Recipe hover derivation (a ~10% tint toward white / one surface step) is
  imperceptible on light palettes — the recipes were visually validated on a
  dark rack. punks overrides `fill.hover` toward its selection token in
  theme.rs. Recipes could accept a hover intent or derive hover perceptually
  (contrast-aware) instead of by fixed tint.
- Parity gap: `decorate_selectable` lacks a persistent-selection parameter —
  `Selectable.active` means activation interaction, not selection — unlike
  `decorate_tree_node`'s selected flag. punks swaps materials per row (the
  tab-bar pattern). Recommend a `selected: bool` parameter for parity.
- No helper to paint a background strip/row behind a run of widgets in window
  flow; the manual rect-capture idiom (cursor pos + known height, paint via
  Canvas before submitting widgets) is required each time (transport strip).
- `decorate_combo`'s contents closure cannot nest other decorators: the
  `Frame` is mutably borrowed for the combo's whole bracket.
- `decorate_slider_f32` duplicates value/min/max between the decorator
  arguments and the widget closure; a mismatch produces plausible wrong fill.

## Widget-breadth gaps

- No decorators for CollapsingHeader, tab-shaped controls, ProgressBar, or
  scrollbars; these remain stock-styled via `apply_theme`.
- Confirmed non-gap: SmallButton needs no dedicated decorator —
  `decorate_button` covers it (same ButtonEx path, zero vertical padding in
  the pinned ImGui), verified at the R2a visual gate on the breadcrumbs.

## Missing renderer capability

- Knob and segmented-LCD-style readouts from the reference have no
  imgui-painter equivalent; punks does not currently need them, so this is
  noted, not requested.

## Dear ImGui / upstream limitations

- The official multi-select API (`BeginMultiSelect`/`EndMultiSelect`,
  `ImGuiMultiSelectFlags_BoxSelect*`) landed in Dear ImGui 1.91.0. punks pins
  1.89.2 (the anatomy/version gate) and imgui-rs 0.12 does not wrap it, so
  upstream box-select is unavailable without the full dependency-bump
  procedure. Multi-select remains ctrl/cmd-click + shift-click; drag-from-row
  is reserved for native drag-out. A hand-rolled rubber-band select is
  possible app-side but competes with the drag-out gesture on dense lists —
  deferred as a product decision.

## punks-specific design choices (not library defects)

- Light popup/window chrome via stock ImGui styling in `apply_theme` —
  ownership of popup chrome deliberately remains with ImGui.
- The custom tab bar remains button-based (decorated) rather than asking
  imgui-painter for a tab widget.
- Per-row `+` tag buttons removed — the Inspector Tags section owns tag
  editing; fewer per-row widgets also reduced per-row decoration cost.
- The error line stays a stock read-only input (red text on light) — no
  decoration needed for a rarely-visible diagnostic surface.
- Drag-select glitch (found at the R3 gate) was a pre-existing punks gesture
  bug, not an imgui-painter defect: hover-based drag-out detection fired a
  native OS drag session from every row a pressed cursor crossed, every frame,
  while repeated early returns blanked the lower UI. Fixed with press-origin
  detection (`is_item_active`) plus a one-per-gesture latch — which the
  preserved-last-item guarantee above makes possible on decorated rows.

## Performance evidence

Methodology: `PUNKS_UI_PERF=1` on debug builds logs average draw milliseconds,
allocations/frame (counting global allocator, debug only), and decorated
rows/frame at a one-second cadence.

- Baseline (R3a, stock rows, small smoke directory, ~20 visible rows):
  avg draw 0.66–0.91 ms typical, ~128–142 allocs/frame, 0 decorated rows.
- After (R3b, same environment, 7 decorated rows/frame): avg draw 0.79–1.0 ms
  typical (occasional 1.8 ms samples during window activity), ~120–133
  allocs/frame — *lower* than baseline: removing the per-row + buttons freed
  more allocations than decoration added, and the decorators ride the
  zero-alloc Canvas path. Per-row decoration cost at this scale is within
  noise.
- Open: real-library / large-search-result numbers on the user's machine
  (run the app with `PUNKS_UI_PERF=1` on a big folder and scroll).

## Verdicts

1. **The look is achievable.** The full application — tabs, toolbar,
   breadcrumbs, transport, sidebar, inspector, modals, popups, fields, rows,
   waveform — carries the reference's light glossy chrome with zero raw
   draw-list geometry in application code (CI-enforced), across four
   screenshot gates.
2. **DX is workable but has a clear top-four.** The decorator transforms were
   mechanical enough to delegate site-by-site; the recurring friction was
   (a) the Palette↔ImGui-style hand-sync, (b) mutable-Frame plumbing,
   (c) light-palette hover derivation, and (d) the `decorate_selectable`
   selection-parity gap. None forced a bypass.
3. **Acceptance criterion met:** no visual treatment required bypassing
   imgui-painter, and stock ImGui styling remains only where ownership
   deliberately stays with ImGui (popup chrome, scrollbars, headers, glyphs,
   the error line).
