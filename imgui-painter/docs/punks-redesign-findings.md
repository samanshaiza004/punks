# punks redesign findings (imgui-painter as a real application dependency)

This document answers two questions: (1) can punks reach the reference's
dense, coherent desktop-tool look; (2) is imgui-painter pleasant enough that a
developer could achieve it without understanding the library's internals.
Entries are classified; not every inconvenience is something imgui-painter
must absorb.

## imgui-painter API/DX gaps

- No bridge between `recipes::Palette` tokens and stock ImGui style colors:
  every non-decorated widget (collapsing headers, progress bar, scrollbars,
  popup chrome, check marks, text) is hand-synced in punks-ui's
  `theme::apply_theme`.
- No helper to paint a background strip/row behind a run of widgets in window
  flow; the manual rect-capture idiom (cursor pos + known height, paint via
  Canvas before submitting widgets) is required each time.
- `decorate_combo`'s contents closure cannot nest other decorators: the
  `Frame` is mutably borrowed for the combo's whole bracket.
- `decorate_slider_f32` duplicates value/min/max between the decorator
  arguments and the widget closure; a mismatch produces plausible wrong fill.

## Widget-breadth gaps

- No decorators for CollapsingHeader, tab-shaped controls, ProgressBar, or
  scrollbars; these remain stock-styled via `apply_theme`.

## Missing renderer capability

- Knob and segmented-LCD-style readouts from the reference have no
  imgui-painter equivalent; punks does not currently need them, so this is
  noted, not requested.

## Dear ImGui / upstream limitations

- (populated during slices)

## punks-specific design choices (not library defects)

- Light popup/window chrome via stock ImGui styling in `apply_theme` —
  ownership of popup chrome deliberately remains with ImGui.
- The custom tab bar remains button-based (decorated in R2a) rather than
  asking imgui-painter for a tab widget.

## Performance evidence

- Baseline (R3a, stock rows, debug build, small smoke directory ~20 visible
  rows): avg draw 0.66–0.91 ms typical, ~128–142 allocs/frame, decorated
  rows/frame 0.
- After (R3b, decorated rows, same environment, 7 decorated rows/frame):
  avg draw 0.79–1.0 ms typical (occasional 1.8 ms samples during window
  activity), ~120–133 allocs/frame — *lower* than baseline: removing the
  per-row + buttons freed more allocations than decoration added, and the
  decorators ride the zero-alloc Canvas path. Per-row decoration cost at this
  scale is within noise. Real-library / large-search numbers are captured by
  the user at the R4 gate.

## Observed during R1

- Nothing beyond the seeded entries.

## Observed during R2a

- Widget breadth: SmallButton decorates via `decorate_button` (ButtonEx path,
  zero vertical padding) — confirmed at the R2a visual gate: breadcrumb
  geometry and chrome align.
- API/DX: decorators preserve last-item queries after the bracket — relied on
  for drag-drop/tooltips, worth documenting in imgui-painter's docs.
- API/DX: recipe hover derivation (a ~10% tint toward white / one surface
  step) is imperceptible on light palettes — the recipes were visually
  validated on a dark rack. punks overrides `fill.hover` toward its selection
  token in theme.rs. Recipes could accept a hover intent or derive hover
  perceptually (contrast-aware) instead of by fixed tint.

## Observed during R2b

- API/DX: decorating buttons in nested sidebar, inspector, and modal draw
  helpers required threading the mutable `Frame` through seven private
  signatures that otherwise did not need painter state. Ambient frame access
  is absent, so decoration adds mechanical parameter plumbing through the UI
  call tree.

## Observed during R3a

- punks-specific design choice: per-row `+` tag buttons removed — the Inspector
  Tags section owns tag editing; fewer per-row widgets also reduces future
  per-row decoration cost.
- Performance evidence: set `PUNKS_UI_PERF=1` in debug builds to log average
  draw milliseconds, allocations per frame, and decorated rows per frame at a
  one-second cadence. Baseline (stock rows) and after (decorated rows) numbers
  will be recorded at the R3 gates.

## Observed during R3b

- API/DX (parity gap): `decorate_selectable` lacks a persistent-selection
  parameter — `Selectable.active` means activation interaction, not selection
  — unlike `decorate_tree_node`'s selected flag. punks swaps materials per row
  (tab-bar pattern). Recommend a `selected: bool` parameter for parity.
- Nothing further.

## Observed at the R3 gate (drag-select glitch)

- Not an imgui-painter defect: a pre-existing punks gesture bug. Drag-out
  detection was hover-based (`row_hovered && is_mouse_dragging`), so sweeping
  the cursor across rows — a box-select gesture — fired a new native OS drag
  session from every row crossed, every frame (flickering copy cursor), and
  each trigger early-returned out of `draw`, blanking the waveform/transport.
  Fixed with press-origin detection (`is_item_active`) plus a one-per-gesture
  latch. Decorated rows preserve `is_item_active` after the bracket, which the
  fix relies on.
- Dear ImGui / upstream limitation: the official multi-select API
  (`BeginMultiSelect`/`EndMultiSelect`, `ImGuiMultiSelectFlags_BoxSelect*`)
  landed in Dear ImGui 1.91.0. punks pins 1.89.2 (the anatomy/version gate)
  and imgui-rs 0.12 does not wrap it, so upstream box-select is unavailable
  without the full dependency-bump procedure. Multi-select remains
  ctrl/cmd-click + shift-click; drag-from-row is reserved for native drag-out.
  A hand-rolled rubber-band select is possible app-side if wanted, but it
  competes with the drag-out gesture on dense lists and is deferred as a
  product decision.
