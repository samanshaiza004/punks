# punks devlog #1 — Rebuilding a sample browser in Rust

*Draft / outline — 2026-06-22*

## What punks is

punks is a modular **sample browser** for musicians: open a folder of audio
files, step through them, hear each one instantly, and drag the one you want
into your DAW. It's a small, fast, native desktop app with a deliberately short
dependency graph.

## Goals

- **Instant, frictionless auditioning.** Click or keyboard-walk through a folder
  and hear samples with no perceptible delay.
- **Real-time-safe audio.** The output path never blocks, never allocates on the
  audio thread, and never glitches under load.
- **Clear ownership.** Audio, persistence, application state, and the native
  executable shell have separate responsibilities and tests.
- **Small and boring.** Smallest correct solution; few dependencies, few
  abstractions, no runtime to ship.
- **Cross-platform.** macOS, Linux, Windows from one codebase.

## How it's made

A Rust workspace of three library crates plus one executable:

```
punks-audio ─┐
             ├─> punks-app ─> punks-standalone
punks-library┘
```

- **`punks-audio`** — the audio engine. `cpal` for native output
  (CoreAudio / WASAPI / ALSA), `symphonia` for decoding WAV/FLAC/MP3/OGG,
  `rubato` for sample-rate conversion, an `lru` decode cache, and the pure
  filename/time-domain analysis algorithms. The audio callback reads only an
  already-published immutable buffer plus atomics: no lock, allocation, wait,
  decode, I/O, or logging is permitted. Decoding happens on a background
  thread; control-side publication and callback acknowledgement keep buffer
  ownership off the callback thread. See `docs/audio-realtime-contract.md`.
- **`punks-library`** — SQLite-backed roots/assets, reconciliation, tags,
  facts, overrides, and disposable analysis/waveform caches.
- **`punks-app`** — preferences, directory history, selection, threaded search,
  tabs, commands, health checks, worker orchestration, and the application UI.
- **`punks-standalone`** — the shell: a `winit` window, `wgpu` + `imgui-wgpu`
  render loop, and native drag-out (`drag`) so you can drag a sample straight
  into another app.

## Why it's an upgrade from the original Electron codebase

punks is a ground-up rewrite of an earlier Electron version. The move to native
Rust is mostly about what an *audio* app needs:

- **No garbage collector in the audio path.** A GC pause during playback is an
  audible glitch. Rust lets the output callback be fully lock-free and
  allocation-free — something that's structurally hard in a JS/Electron runtime.
- **Direct hardware audio.** `cpal` talks to the OS audio APIs in-process,
  instead of going through Web Audio and a Chromium sandbox.
- **No runtime to ship.** No bundled Chromium + Node. The result is a single
  small native binary with fast startup and low memory, instead of a
  hundreds-of-megabytes app that boots a browser to draw a list.
- **In-process decoding.** `symphonia` decodes on a worker thread with no IPC
  bridge between a renderer and a main process.
- **Small, enforced boundaries.** The Rust crate layering keeps audio and
  persistence testable without a window or audio device, while application
  orchestration stays with the UI that consumes it. There is no unused
  toolkit-neutral or future-host layer.

*(Honest caveat: this section is the rationale for going native, grounded in the
current architecture — not a feature-by-feature diff against the old app.)*

## Log — what's been done so far

**Foundation**
- Layered workspace; audio engine and analysis; SQLite library; application UI;
  winit/wgpu standalone shell.
- Waveform visualizer with playhead; remappable keybinds; configurable samples
  folder; native drag-out.

**Volume + persistence**
- Working preview volume slider, persisted across sessions.
- Fixed directory persistence to restore the *exact* subdirectory you left off
  in (previously only saved the folder you picked, not where you navigated).

**Audit & hardening pass**
- Deleted dead/reserved code; refreshed the README; committed `Cargo.lock`;
  added a `fmt + clippy + test` CI workflow (Linux + macOS).
- Audio `Release`/`Acquire` ordering on the buffer swap; poisoned-lock recovery;
  search errors logged instead of swallowed; proper stereo→mono downmix
  (average, not truncate).
- Performance: switched the file list to an imgui `ListClipper`, eliminating a
  per-frame allocation of the entire listing.

**Tabs**
- Full multi-tab navigation: each tab has its own history, selection, and search
  state; one global playback engine shared across tabs. Drag-to-reorder, a
  custom tab bar (imgui can't read its native tab reorder back), close with
  min-one-tab, and next/prev/new/close keybinds.

**Decode robustness**
- Handle Ogg-Vorbis-in-WAV files (WAVE format tag `0x674f`, from the Vorbis ACM
  codec) by extracting the inner Ogg stream from the `data` chunk — a real
  format that shows up in older sample packs.

**UI polish**
- Browser-like tabs (active accent vs. muted inactive, attached close glyph);
  width-adaptive multi-column file list; a calmer dark theme.

**GPUI settings and drag-out follow-up**
- Added a restart-applied settings panel for the eight functional keybinds. The
  four dormant tab keybinds remain persisted but hidden and unregistered until
  visible tabs return.
- Replaced the Windows `drag` stopgap with a private OLE `CF_HDROP` source and
  added a private X11 XDND source bridge. Wayland remains on GPUI's path.
- Windows, X11, and Wayland runtime verification remains outstanding on this
  macOS-only host; see `docs/gpui-viability.md`.

## What's next

**Near term**
- Tab persistence across launches (restore the open set, not just one folder).
- A single reusable decode worker ("latest request wins") instead of spawning a
  thread per keypress during fast scrolling.
- Broader format coverage (sibling Ogg-in-WAV tags; surface genuinely
  unsupported codecs clearly).

**Browser features**
- Metadata/tags, favorites, and BPM/key detection.
- Waveform zoom, loop region, and quick trim.

The crate boundaries are intentionally limited to responsibilities with real
consumers today. A future host or DAW integration can earn a new boundary when
that second consumer exists.
