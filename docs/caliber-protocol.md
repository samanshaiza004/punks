# Punks Caliber protocol

The application protocol is version 1 and uses UTF-8 JSON. The Caliber C ABI
remains the boundary for command copying, state publication, immutable
resources, leases, resource retirement, and wakeups. The Odin frontend does
not call the core directly.

## Limits

| Item | Limit |
| --- | ---: |
| Command payload | 16 KiB |
| State publication | 64 KiB |
| Entry resource | 1 MiB |
| Live Caliber resources | 8 |
| Pending commands | 64 |
| Rows in a requested window | 128 |
| Path field | 4 KiB |
| Search query | 1 KiB |
| Asset identity | 8 KiB |

The adapter reduces the returned row count if JSON escaping makes a resource
larger than its byte limit. It never publishes a full unbounded directory in
one resource.

## Commands

Each command carries `v: 1` and an `op` string. Optional fields default to
empty values or zero. Unknown fields and unsupported protocol versions are
rejected.

| Operation | Fields | Behavior |
| --- | --- | --- |
| `open` | `path` | Open a directory. |
| `up` | — | Navigate to the parent directory. |
| `breadcrumb` | `index` | Navigate to a visible ancestor in the active tab's breadcrumb history. |
| `select` | `id`, `index`, `listing_revision`, `audition` | Select an entry and optionally preview it. |
| `enter` | `id`, `index`, `listing_revision`, `audition` | Enter a result row directory, or select a file. |
| `enter_folder` | `id`, `index`, `listing_revision` | Enter a direct child folder from the navigation sidebar, including during search. |
| `step` | `delta`, `audition` | Move selection by one or more rows. |
| `play` / `stop` | — | Start or stop the selected preview. |
| `seek` | `seconds` | Seek to a finite, non-negative position. |
| `search` | `query` | Search filenames; an empty query clears search. |
| `window` | `offset`, `listing_revision` | Request a bounded window for the current listing revision. |
| `folder_window` | `offset`, `listing_revision` | Request a bounded window of direct child folders for the active directory. |
| `tab_new` | — | Open a tab at the current directory. |
| `tab_close` | `tab_id` | Close a tab by its session-stable identity. One tab always remains open. |
| `tab_switch` | `tab_id` | Activate a tab by its session-stable identity. |
| `tab_reorder` | `tab_id`, `tab_target_id`, `after` | Move a tab before or after another tab without changing its identity. |
| `create_library` | — | Initialize the library at the current location. |

Every command may include `tab_id`. Commands targeting the active browsing view
are rejected when that ID no longer names the active tab. This prevents delayed
frontend actions from applying to another tab after a switch. Tab IDs are
session-local and are not persisted.

Selection and window requests include the listing revision. The adapter checks
the revision and asset identity before using an index; stale or mismatched
requests become visible state errors and do not select a different file.

## State and resources

The state publication has `v: 1`, `ready`, `directory`, `can_navigate_up`, `query`, `searching`,
`listing_revision`, `total`, `offset`, `count`, `selected_index`, and `selected`
entry fields. `entries` contains a Caliber `{id, generation}` resource handle.
`folders` is a second bounded resource containing only the active directory's
direct child folders. Its entries retain their index in the current listing so
navigation is checked against the same revision and asset identity. `tabs`,
`active_tab`, and `breadcrumbs` provide the current tab strip and location
navigation. Tab ordering, search text, selection, and directory state remain
owned by `punks-core`.
The state also reports playback (`idle`, `loading`, or `playing`), selected
filename, position and duration, audio availability and errors, the most recent
operation error, scan progress, `command_sequence`, and `last_request_id`.
It also includes a bounded 512-bucket signed waveform snapshot for the loaded
playback file. Peak values are quantized to signed 16-bit values; the waveform
axis start and duration identify the source interval represented by the peaks.

The immutable entry resource contains `v`, `listing_revision`, `offset`,
`total`, and a bounded array of entries. Each entry includes its stable asset
identity, index, display name, path, format, size, and directory flag. The
frontend verifies the resource's version, revision, offset, and row count
against the state before using it.

The adapter retains the current entry and folder resources and up to four prior
resources while
publishing replacements. It retires older resources through Caliber. A mapped
view lease remains valid until released even after retirement or context
destruction; the frontend releases every view immediately after copying it into
one of two local arenas. State-publication leases are also released exactly
once. A map that races retirement causes the frontend to reread state.

## Wakeups and shutdown

Commands are copied through `caliber_context_dispatch`; the backend actor is
then signaled. Core worker completions use the same actor signal. When idle,
the actor sleeps without periodic polling. While playback loads or runs, it
updates status on a 33 ms schedule. Caliber increments its wake sequence when
state changes, and the frontend's single blocking waiter calls Alicorn's
thread-safe application wake function.

Shutdown order is:

1. Stop and join the Punks actor and core workers.
2. Stop Caliber wake waits and join the frontend's waiter thread.
3. Destroy the Punks session and Caliber context.

No Caliber function is called from the real-time audio callback. Caliber ABI
version 1 and Punks protocol version 1 are checked independently at startup.
