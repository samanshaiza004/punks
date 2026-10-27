# Foundation audit

Audit date: 2026-10-27. Starting Punks commit: `8e03643`.

## Sources and baseline

- Punks was inspected in this checkout. Its earlier cleanup removed the Rust
  tests. The GPUI executable still depends on the domain code in `punks-app`.
- Caliber was inspected at `4814a5161809f37d07f8456b81988013f038867a`.
  Its two crates passed all 27 tests using a temporary target directory.
- Alicorn was inspected at `9062358a68f324dc3250d4c6420faf95c6fc568c`.
  Its runtime passed all 250 tests. The native async-list example compiled
  with the installed Odin compiler and the existing AccessKit archive.
- Development platform: macOS arm64, Rust 1.97.1, Odin
  `dev-2026-09-nightly:a2fb372`.
- The original workspace test invocation could not resolve GPUI because Cargo
  could not create its cached Zed checkout under the initial filesystem policy.
  An isolated copy of the unchanged library crate then failed compilation on
  the five undefined constants `SCHEMA_V2` through `SCHEMA_V6`.
- The current `SCHEMA_V1` is actually the v6 override-table rebuild. It cannot
  initialize an empty database because `fact_overrides` does not exist.
- Historical migrations exist in a separate local Punks checkout at commit
  `891768530f0b9b3c725b5985f415c1489a5eb75a`. Its library source is unchanged
  relative to that commit. This is evidence for versions 1–6, not permission
  to infer arbitrary unknown schemas.

## Existing boundaries

`punks-audio` already owns decoding, RF64/BWF parsing, bounded preview windows,
resampling, decode caching, waveform computation, metadata and deterministic
analysis. Its CPAL callback reads immutable prepared audio. These mechanisms
will be retained.

`punks-library` owns SQLite, asset identities, reconciliation, tags, overrides,
analysis jobs and persistent waveform files. The missing migration definitions
prevent it from building. Historical v3/v4 migrations discard generated results;
the repaired path must preserve those records instead. Job claiming also reads
and updates in separate statements, allowing competing connections to claim
the same pending work.

Most of `punks-app/src/lib.rs`, plus `command`, `config`, `filesystem` and
`health`, is already independent of GPUI. The crate couples that code to GPUI
only through its module declarations and dependencies. Worker handles are
mostly detached, asynchronous completion has no native wake path, directory
and database operations run on the caller, and construction currently fails
when no audio device is available. Those are the lifecycle changes needed for
a frontend that can sleep when idle.

GPUI composition, actions, theme, platform drag bridges, focus, selection
gestures and viewport geometry belong to presentation. They do not belong in
the extracted core.

## Caliber boundary

The actual `CaliberApiV1` supplies bounded command copying, coherent state
leases, immutable resource leases, generation checks, resource retirement and
a blocking wake-sequence wait. The ABI uses platform `usize` for lengths.
Both state and mapped-resource leases keep their allocation alive after the
context is destroyed; consumers must release each lease exactly once.

Only one Caliber wake waiter may be active per context. The frontend will own
that waiter and wake Alicorn through `Application_Waker`. A small Punks dispatch
entry point will enqueue through the existing Caliber operation and signal the
backend owner; it will not define a second transport. Backend mutation runs on
one control thread. Shutdown stops backend workers, stops and joins the foreign
waiter, and only then destroys the context. No generic Caliber operation runs
from the real-time callback.

The application protocol will be versioned JSON: coarse commands, a small state
summary and a bounded immutable entry window. The frontend retains scroll,
focus and rendering state. Stale listing revisions are rejected; resource-map
races cause a coherent state reread rather than dereferencing retired data.

## Smallest native application

Alicorn already provides procedural `ui` authoring, controlled text fields,
keyed fixed-row virtual lists, native asynchronous folder dialogs, keyboard
callbacks, explicit invalidation and a thread-safe application waker. Its
`host.Run` owns the SDL native lifecycle. A single window can therefore exercise
open/navigation, bounded rows, selection, search, preview and stop without
changes to Alicorn or a new renderer. Playback publication needs a deadline
only while playback/loading is active; completed worker results supply wakes.

## Implementation sequence

1. Repair migrations and add preservation, rollback and reconciliation tests.
2. Extract `punks-core`, retaining the Rust audio/library code, and give its
   workers explicit wake and shutdown ownership.
3. Add the Punks Caliber adapter with bounded publications and contract tests.
4. Build the native Odin window and validate an end-to-end vertical slice.
5. Remove the obsolete GPUI implementation after that validation and update
   build instructions, architecture, protocol and validation records.

The old frontend remains available during the vertical-slice work. Advanced
comparison, acoustic exploration, segmentation and DAW integration are excluded.
