# punks redesign findings (imgui-painter as a real application dependency)

The punks2 "Neon Live" redesign (slices R1–R3b, the drag-gesture fix, and the
final typography/depth completion pass)
rebuilt the app's chrome on imgui-painter: theme foundation, ~40 decorated
button/row/field/checkbox sites, recipe-driven materials, and a measured
per-row decoration cost. This document answers two questions: (1) can punks
reach the reference's dense, coherent desktop-tool look; (2) is imgui-painter
pleasant enough that a developer could achieve it without understanding the
library's internals. Entries are classified; not every inconvenience is
something imgui-painter must absorb.

## imgui-painter API/DX findings

- Resolved: `recipes::apply_imgui_colors` now maps the compact `Palette` onto
  every stock ImGui color role through a statically-sized `imgui-sys` array.
  Punks keeps only application metrics and semantic destructive red locally;
  collapsing headers, tables, scrollbars, navigation, popup chrome, plots, and
  text no longer require a second hand-maintained palette.
- Ambient frame access is absent: decorating buttons in nested sidebar,
  inspector, and modal draw helpers required threading the mutable `Frame`
  through seven private signatures that otherwise did not need painter state —
  mechanical parameter plumbing through the UI call tree.
- Resolved/documented: decorators preserve last-item queries after the bracket (`is_item_hovered`,
  `is_item_active`, `item_rect_*`, drag-drop attachment). punks relies on this
  for tooltips, drag-drop, right-click handling, and the drag-out fix — it is
  now a public compatibility contract with an executable ID/rect/hover/active
  regression test. Drag/drop attachment remains in the human gate.
- Recipe hover derivation (a ~10% tint toward white / one surface step) is
  imperceptible on light palettes — the recipes were visually validated on a
  dark rack. punks overrides `fill.hover` toward its selection token in
  theme.rs. Recipes could accept a hover intent or derive hover perceptually
  (contrast-aware) instead of by fixed tint.
- Resolved: `decorate_selectable` accepts persistent selection explicitly.
  Priority is pressed interaction, selected, hovered, base. Idle selection
  reuses the Material active token; a selected press derives a darker active
  fill so rows do not lose click feedback. Punks deleted its parallel
  selected-row Material and uses one row recipe.
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

- Popup sizing/lifecycle remains with ImGui. Punks paints the explicit current
  popup rectangle as the first user draw operation, then submits stock popup
  contents. An inset edge works inside the popup clip; an external painter
  shadow would be clipped, so none is faked or pushed into the library.
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

## Completion-pass evidence

- The broad workspace, sidebar, results well, Inspector, waveform, transport,
  and popup bodies all use Canvas or existing `panel`/`inset_panel` recipes.
  No Punks-side vertex/index mutation or hand-written tessellation was needed.
- Surface hierarchy composes cleanly but still requires the host to know flow
  geometry: paint parent rectangles first, then submit transparent child
  windows. That repeated ordering is real DX pressure, but a public flow-strip
  or pane abstraction remains unearned without a second application consumer.
- Inter Regular 4.1 replaces ProggyClean at 13 logical pixels. Explicit Latin,
  Greek/Cyrillic, punctuation, arrow, and geometric-symbol ranges fix the
  missing transport/Inspector glyphs while tightening vertical rhythm. The OFL
  license ships beside the font.
- The waveform is the deliberate identity risk: an orange multi-stop clip
  surface with a semi-opaque dark min/max envelope, inset edge, dark outline,
  blue playhead, and small inter-bucket gaps. Keeping those layers in one
  Canvas preserves batching without collapsing dense files into an opaque
  analyzer block.
- The palette bridge makes stock CollapsingHeader chrome sufficient for the
  Inspector: pale raised idle headers, restrained hover, blue active state.
  A dedicated CollapsingHeader decorator is still not justified.
- Mutable `Frame` plumbing and weak generic light-theme hover derivation remain
  the two repeated library-level frictions. Both stay documented rather than
  acquiring speculative APIs in this pass.

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
- Completion pass (real library, constrained 800x632 window, 10 decorated
  rows/frame): idle browsing settled around 1.84-2.05 ms and 95-116
  allocations/frame. Auditioning files with the layered waveform visible
  generally measured 1.84-2.81 ms and 115-125 allocations/frame, with
  occasional 3.44 ms / 138-allocation samples while selection, decoding, or
  window activity changed. These are debug-build observations without a hard
  machine-dependent threshold; they are not directly comparable to the
  earlier small smoke-directory baseline. The completion pass remained
  responsive while exercising long filenames, a narrow Inspector, pane
  scrollbars, and the new broad layered surfaces.

## Verdicts

1. **The look is achievable.** The full application — tabs, toolbar,
   breadcrumbs, transport, sidebar, inspector, modals, popups, fields, rows,
   waveform — carries the reference's light glossy chrome with zero raw
   draw-list geometry in application code (CI-enforced), across four
   screenshot gates.
2. **DX is workable and two top gaps are closed.** The decorator transforms were
   mechanical enough to delegate site-by-site; the recurring friction was
   Palette↔ImGui-style hand-sync, mutable-Frame plumbing, light-palette hover
   derivation, and Selectable selection parity. The palette and selection APIs
   are now proven by Punks; Frame plumbing and generalized hover policy remain
   open. None forced a bypass.
3. **Acceptance criterion met:** no visual treatment required bypassing
   imgui-painter, and stock ImGui styling remains only where ownership
   deliberately stays with ImGui (popup chrome, scrollbars, headers, glyphs,
   the error line).
