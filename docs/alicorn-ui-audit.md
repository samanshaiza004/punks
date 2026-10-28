# Alicorn UI API dogfood audit

Date: 2026-10-28. Punks is the first application dogfood for Alicorn v0.3.0.

## Current usage

The frontend now uses `alicorn:ui` for panels, rows, columns, buttons, text,
controlled fields, sliders, virtual-list scopes, layout styles, and text-buffer
creation, updates, and destruction. Those helpers match the layout structure:
one root column, horizontal header/navigation/transport rows, a fixed-height
preview column, and a retained virtualized browser list.

The search field uses `ui.text_field`, preserving its changed and submitted
events while giving the visible label to accessibility. The app's debounced
search and immediate submit behavior are unchanged. The list uses
`ui.virtual_list` with `focusable=true`; the helper exposes its scroll handle and
realized range while closing the retained scopes safely. Fixed row height
remains explicit because it is part of the virtualizer's contract.

The explicit native host lifecycle remains appropriate. Punks uses native
folder dialogs, scheduled debounce, backend wake publication, application
keyboard commands, and worker shutdown. The small `ui.run` wrapper does not
expose all of those native host services, so Punks must use `host.Run`, own
`begin_frame`/`end_frame`, and create the required `.Root` node.

## Resulting boundary

Direct `alicorn:runtime` calls remain at the custom host/frame boundary, for
host-callback invalidation and scroll restoration, and for runtime callback
signatures. Stable string IDs are passed directly as UI keys; numeric IDs use
`ui.key_u64`. The frontend retains the node handles needed for search focus and
list scrolling. It adds no retained sizing state or duplicate widget implementation.

The dogfood added two small `ui` conveniences: a text-buffer setter and an
optional `focusable` argument for the scoped virtual list. Both forward to the
existing runtime operations. The UI package tests cover buffer ownership and
focusability propagation.

Punks has no current layout that needs percentage sizing; fixed design metrics
and flex allocation express its present layout directly.