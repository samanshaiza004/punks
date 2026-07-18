# The compatibility contract

Two guarantees matter when integrating decorators: what imgui-painter promises
*you*, and what it needs from Dear ImGui.

## Guarantee: the last item is preserved

After a `decorate_*` call returns, the widget it wrapped is still ImGui's **last
item**, with:

- its **ID**,
- its **bounds** (`item_rect_min` / `item_rect_max` / `item_rect_size`),
- its **hovered** state (`is_item_hovered`),
- its **active** state (`is_item_active`),
- and its eligibility as a **drag/drop source or target**.

So this works, exactly as it would on an undecorated widget:

```rust
let clicked = unsafe {
    decorate_selectable(&mut frame, &material, selected, || {
        ui.selectable_config(&label).selected(selected).build()
    })
};

// All still valid — the decorator did not consume "last item".
if ui.is_item_hovered() { ui.tooltip_text("details"); }
let pressed_here = ui.is_item_active();
let rect = (ui.item_rect_min(), ui.item_rect_max());
```

This is a **public compatibility contract with an executable regression test**,
not an implementation accident. Hosts rely on it for tooltips, context menus,
right-click handling, and drag/drop attached immediately after a decorated call.

It is load-bearing in a specific way that is worth spelling out: press-origin
drag detection (`is_item_active` on the row that captured the mouse press, rather
than hover) is only possible *because* the decorator preserves it. Without that
guarantee, a host sweeping the cursor across decorated rows cannot distinguish
"the row I pressed on" from "a row I happened to cross."

## Requirement: an exact Dear ImGui version

`decorate_*` reconstructs stock widget chrome geometry — where the checkbox box
sits relative to its label, where the input frame ends, how the slider grab is
positioned. Those formulas mirror Dear ImGui's own internal layout code.

**That layout is not a stable upstream contract.** It is internal detail that
Dear ImGui is entitled to change in any release, including a patch release.

The consequence is specific and nasty: a source-compatible ImGui bump can
**compile cleanly** while silently moving the stock widget away from the painted
rectangle. Nothing errors. The paint is just in the wrong place.

The supported target is:

| | Version |
|---|---|
| Dear ImGui | **1.91.9b** |
| imgui-rs / imgui-sys | 0.12, fork rev `7a89260` |

A `debug_assert` checks `igGetVersion()` against `ANATOMY_IMGUI_VERSION` at
decoration time. Note that this is a *debug* assert: in release builds it is
compiled out, so a mismatched version degrades to silently wrong geometry rather
than a panic. Do not rely on it to catch a misconfiguration in production.

### Two kinds of version coupling

They fail differently, so it is worth distinguishing them:

- **Runtime** — the `decorate_*` geometry formulas. Wrong version compiles, then
  paints incorrectly.
- **Compile time** — `recipes::apply_imgui_colors` names color roles such as
  `ImGuiCol_NavCursor`, `ImGuiCol_TextLink`, and the `TabSelected`/`TabDimmed`
  family, which do not exist in older ImGui. Wrong version fails to build.

The painting core, the adapter, the style data types, and the recipe builders
have **no** version coupling at all. See
[Architecture](../concepts/architecture.md#where-version-coupling-lives).

## VERIFIED_IMGUI_SYS

Because a bad bump is silent, the project does not rely on noticing it.

`VERIFIED_IMGUI_SYS` at the repo root holds the imgui-rs fork revision that a
human last ran the full visual gate against. CI resolves `imgui-sys` fresh, with
no lockfile, extracts the resolved revision, and **fails the build** when it
differs.

Upstream point releases trip it too. That is the design, not a false positive.

The only correct response is to run the
[dependency-bump checklist](https://github.com/samanshaiza004/imgui-painter/blob/main/CONTRIBUTING.md#dependency-bump-checklist)
— re-verifying every widget's anatomy at 1×, 1.5×, and 2× — and *then* update the
file. Editing it to match a new revision without running the gate removes the
only protection against silently misplaced chrome.

## Why this is not on crates.io

Cargo honors `[patch.crates-io]` only from the **consuming workspace root**. A
published imgui-painter could not force its consumers onto the forked
`imgui-sys` that provides 1.91.9b; they would resolve stock `imgui-sys` 0.12,
which bundles Dear ImGui 1.89.2, and get wrong geometry with no error in release
builds.

Publishing is therefore deferred until upstream imgui-rs ships 1.91.x. Until
then, depend on it by git and apply the patch yourself, as shown in
[Getting started](../getting-started.md#adding-the-dependency).
