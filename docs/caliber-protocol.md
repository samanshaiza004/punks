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
| `select` | `id`, `index`, `listing_revision`, `audition` | Select an entry and optionally preview it. |
| `enter` | `id`, `index`, `listing_revision`, `audition` | Enter a directory, or select a file. |
| `step` | `delta`, `audition` | Move selection by one or more rows. |
| `play` / `stop` | — | Start or stop the selected preview. |
| `seek` | `seconds` | Seek to a finite, non-negative position. |
| `search` | `query` | Search filenames; an empty query clears search. |
| `window` | `offset`, `listing_revision` | Request a bounded window for the current listing revision. |
| `create_library` | — | Initialize the library at the current location. |

Selection and window requests include the listing revision. The adapter checks
the revision and asset identity before using an index; stale or mismatched
requests become visible state errors and do not select a different file.

## State and resources

The state publication has `v: 1`, `ready`, `directory`, `query`, `searching`,
`listing_revision`, `total`, `offset`, `count`, `selected_index`, and `selected`
entry fields. `entries` contains a Caliber `{id, generation}` resource handle.
The state also reports playback (`idle`, `loading`, or `playing`), selected
filename, position and duration, audio availability and errors, the most recent
operation error, scan progress, `command_sequence`, and `last_request_id`.

The immutable entry resource contains `v`, `listing_revision`, `offset`,
`total`, and a bounded array of entries. Each entry includes its stable asset
identity, index, display name, path, format, size, and directory flag. The
frontend verifies the resource's version, revision, offset, and row count
against the state before using it.

The adapter retains the current resource and up to two prior resources while
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
