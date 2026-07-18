# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project does not
yet follow semantic versioning, because it has not had a release.

## [Unreleased]

Nothing yet.

## [0.1.0] — unreleased

The initial development line, built in phases. Each phase closed against a
human visual gate (`painter_demo` at 1×/1.5×/2×) in addition to automated tests;
they are recorded here because the reasoning behind several design decisions is
only legible as a sequence.

### The core rendering layer

- **Phase 1** — the go/no-go gate. `Painter` with `rounded_rect`, `fill_color`,
  `fill_gradient` (linear, radial), stackable `add_shadow`, and `add_border`
  with hairline alpha compensation.
- **Phase 2** — hardening: adaptive, error-bounded rounded-rect tessellation;
  Angular and Diamond gradient modes; benchmarks against equivalent hand-written
  tessellation; and the header-only C++ fluent wrapper (`include/imgui_painter.h`).
- **Phase 3** — the ownership chain hosts actually use: a long-lived `Painter`,
  a per-frame `Frame`, and a per-draw-list `Canvas`.
- **Phase 7** — rendering depth: inset shadows, band-clipped solid and gradient
  overlays, genuinely stacked inset borders, and device-scale hairlines, all
  composing in painter order. The ImGui-aware `Frame` path samples
  `DisplayFramebufferScale` automatically; direct C/C++/`Session` consumers set
  their host scale explicitly.

### The ImGui-aware decoration layer

- **Phase 4A** — prototyped the item-paint bracket: a three-channel draw-list
  split (Background → Widget → Overlay), the widget's own frame colors pushed
  transparent across all interaction states, and decoration painted behind a
  *stock* widget with no wrapper.
- **Phase 4B** — graduated it into the Rust adapter API. `Material` holds the
  shared radius/fill/border/shadow inputs; the stock widget keeps its layout,
  input, text, and return value. The ImGui-free core was unchanged.
- **Phase 5** — Checkbox and single-line InputText, without expanding `Material`.
  Their chrome rectangles are captured separately from the complete ImGui item,
  so Checkbox paint excludes its label and InputText paint excludes its visible
  label.
- **Phase 6** — replaced the public `item_paint`/`Decorator` pair with typed
  entry points (`decorate_button`, `decorate_selectable`, `decorate_checkbox`,
  `decorate_input_text`), making decorator/widget mismatch unrepresentable in
  normal use. The raw mechanism became private.
- **Phase 8** — Slider, Combo, and TreeNode. Their anatomy is centralized in a
  private, allocation-free enum.
- **Phase 9** — part-specific styling. Typed `SliderStyle`, `ComboStyle`, and
  `TreeStyle` give each reconstructed part an independent appearance, and
  private per-widget visual states replace the overloaded active bit.
  `recipes::Palette` (9 tokens) plus a recipe family (`raised_button`,
  `toolbar_button`, `inset_control`, `selected_row`, `browser_tree_row`,
  `parameter_slider`, `combo_field`, `panel`, `inset_panel`) reproduce
  reference desktop chrome.

### Host-integration APIs earned by application pressure

Two APIs were added only after a real application (punks2) demanded them twice:

- `recipes::apply_imgui_colors(&mut style.colors, &palette)` maps the compact
  palette across every stock ImGui color role without taking an `imgui-rs`
  dependency.
- `decorate_selectable(frame, material, selected, widget)` takes persistent
  selection explicitly. Priority is pressed interaction, then selected, then
  hovered, then base.

Decorators preserve the submitted widget as ImGui's last item — its ID, bounds,
hover, and active queries all survive the bracket. This is tested as a public
compatibility contract so tooltips, context menus, and drag/drop can be attached
immediately after a decorated call.

### Compatibility

Reconstructed widget geometry is explicitly compatible with **Dear ImGui 1.91.9b**
via imgui-rs 0.12 fork revision `7a89260`. Migrated from 1.89.2 to 1.91.9b, which
also brought upstream multi-select. See [CONTRIBUTING.md](CONTRIBUTING.md) for the
dependency-bump checklist that guards this.

### Still deferred

A public Resolver, `CheckboxStyle`, focus-ring styling, disabled-specific
appearance, icons, themes, `PushMaterial`, and typography.
