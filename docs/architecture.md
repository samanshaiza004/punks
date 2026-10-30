# Punks architecture

## Runtime boundaries

```text
Odin UI (Alicorn native host)
          │ commands / state / leased resources / wake sequence
          ▼
Punks-Caliber adapter (Rust)
          │
          ▼
Punks Core (Rust)
       ┌──┴──┐
       ▼     ▼
   Library  Audio
```

The Odin frontend owns layout, rendering, keyboard and pointer input, focus,
scroll position, and the cached state needed to draw its current view. It uses
Alicorn's procedural UI, keyed virtual list, native folder dialog, application
waker, and host lifecycle.

Punks Core is a headless Rust library. It owns filesystem navigation, search,
selection, playback commands and state, configuration, and background work.
It does not depend on a GUI toolkit, Alicorn, or native window handles. The
Caliber adapter translates versioned commands and publishes a compact state summary with a
bounded entry window. It does not implement audio playback.

`punks-audio` retains the existing decoders, resampling, preview cache,
playback engine, metadata readers, and analysis routines. `punks-library`
retains the SQLite library and filesystem reconciliation behavior. Filesystem
scans, decoding, hashing, analysis, and database work stay on Rust worker
threads. The audio callback uses prepared playback data and does not call
Caliber, the UI, or the database.

The Caliber application protocol is documented in
[`caliber-protocol.md`](caliber-protocol.md). SQLite compatibility and the
explicit handling of historical analysis results are documented in
[`database-migrations.md`](database-migrations.md).

## Current interface

The first native window supports opening a folder, navigating into and out of
directories, a bounded scrollable audio list, stable selection, filename
search, preview, stop, and seeking. It reports scan progress, playback state,
audio-device availability, and operation errors. The listening deck uses two
retained GPU layers for signed min/max peaks and the playhead, with direct
waveform seeking, pause/resume, Stop, and volume. Tags and metadata editing,
settings UI, waveform zoom/pan, and external drag-out remain future work.

The list uses 32 logical-pixel rows and the backend returns at most 128 entries
per immutable resource. The UI requests another window only when scrolling
reaches the returned range. Large directories therefore do not cross the
boundary as a complete list on every state update.

## Worker lifecycle and idle behavior

Caliber command handling and core state publication run on one Rust control
thread. The core owns and joins its background workers. When there is no
command or background completion, the control thread waits on a condition
variable. Playback status is refreshed at a bounded interval only while audio
is loading or playing. The Odin host has one blocking Caliber wake waiter; a
changed wake sequence invalidates the view. Shutdown stops and joins Rust work,
joins the Odin-owned waiter, then destroys the Caliber context.

## Build and platform coverage

The macOS arm64 build and frontend smoke runs are validated. The Windows x64
release executable also built and passed the no-audio end-to-end smoke on
2026-10-28 using SDL 3.4.2 on D3D12 and the pinned MSVC toolchain. Linux
remains unverified.
See the README for local prerequisites and commands.
