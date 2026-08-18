# Dear ImGui 1.91.9b migration findings

## Compatibility target

- Dear ImGui: `1.91.9b` (non-docking is authoritative for Punks).
- imgui-rs API: `0.12.0`, maintained fork revision
  `7a89260c79ad1f9d4bfe81d6ca1b76ad38a6b3e3`.
- cimgui generation source: `c561c6a6e54d24d45b44c33ab058b5c1e2327949`.
- `ImTextureID` remains pointer-compatible in the generated master, docking,
  FreeType, and docking-FreeType variants.

The originally proposed cimgui revision
`98e6ff7051df19b76854bc3eb3cea2798f8d3bc5` cannot parse Dear ImGui 1.91.9b.
Using it would require stale generated layouts or manual binding edits, both
more dangerous than advancing cimgui. The selected revision is the first
tested pre-1.92 generator that emits all four existing fork-CI variants.

The fork keeps the published imgui-rs 0.12 high-level cursor enum unchanged.
Dear ImGui 1.91's raw Wait/Progress cursor values remain available in
`imgui-sys`; exposing them in the high-level enum broke the exhaustive match
in published `imgui-winit-support 0.13` and would have forced a second fork.

## Painter audit

The exact 1.91.9b Checkbox, single-line InputText, horizontal linear Slider,
standard Combo, and unframed TreeNode implementations were compared with the
private anatomy formulas. Their scoped geometry remains equivalent. The
compatibility tests now use queued mouse events because 1.91 preserves input
event ordering more strictly than the old direct-IO-field test setup.

The ImGui palette bridge now maps all 56 roles, including selected/dimmed tabs,
tab overlines, `TextLink`, and the renamed `NavCursor` role. Splitter cleanup,
Combo parent last-item restoration, popup color restoration, last-item query
preservation, clipping, and panic cleanup remain executable tests.

## Native selection ownership

Punks keeps one sorted application-owned vector per browse/search view. Begin
and End requests apply in order to the same mutable candidate, which is
committed once only if the displayed-list revision is unchanged. Rebuilt search
results and replaced browse listings clear index selection and move to a new ID
scope. Duplicate labels receive index IDs without path-string allocations.

Browse directories participate in layout/navigation but are rejected from
selection requests and batch paths. The active browse-or-search selection feeds
batch actions and drag-out. A selected row drags every selected file; an
unselected row drags only itself; empty list space remains available to native
box selection.

## Remaining manual gates

- Ctrl/Cmd-click, Shift-click, Shift+Arrow, Ctrl/Cmd+A, and Escape in both views.
- Box selection with vertical/horizontal clipping and autoscroll.
- Selected/unselected row drag ownership and multi-file OS payloads.
- Filtering, navigation, tab switching, empty lists, popups, Combo, TreeNode,
  scrolling, clipboard, and drag-out.
- 1×, 1.5×, and 2× painter screenshots plus Punks browse/settings screenshots.
- Frozen-table selection if tables are introduced; early 1.91 clipping behavior
  remains a mandatory future compatibility gate.

Dear ImGui 1.92 is intentionally excluded. Its font and renderer texture
protocol migration remains a separate milestone.
