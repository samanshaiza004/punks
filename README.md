# punks

A modular sample browser for musicians, built in Rust.

## What it does

- Browse directories of audio files (WAV, FLAC, MP3, OGG) with breadcrumb navigation
- Production-sound aware: reads Broadcast Wave (`bext`) description and start timecode,
  and plays RF64 (>4 GB) field recordings
- Preview-play through your default audio device — click a file, or use keyboard
  navigation (W/S or arrow keys) to step through and auto-play
- Long files (> 2 min) preview a bounded window instead of loading whole, so hours-long
  recordings open instantly and stay memory-bounded
- Instant replay from an in-memory decode cache when you revisit a sample
- Volume control for previews, persisted across sessions
- Recursive filename search from the current directory
- Opt-in per-folder library (SQLite in a `.punks` folder you create explicitly):
  tag samples, filter by tags (AND), tags survive file renames and moves
- Waveform visualizer with a playhead
- Remappable keybinds and a configurable samples folder via the Settings modal
- Restores the exact directory you left off in on next launch
- Drag a sample out of the browser into another application (macOS/Windows)

## Building

```
cargo build --release -p punks-standalone
```

### Requirements

- Rust 1.84+ (stable)
- macOS, Linux, or Windows
- On Linux: ALSA and GTK3 development libraries (`libasound2-dev libgtk-3-dev` on Debian/Ubuntu —
  `cpal` needs ALSA, `drag`'s Linux backend needs GTK3)

### Cross-platform builds

- **macOS**: native — `cargo build --release -p punks-standalone`.
- **Linux, from macOS**: use [`cross`](https://github.com/cross-rs/cross) (Docker-based), since
  `cpal` and `drag` need ALSA/GTK3 headers macOS doesn't have:
  `cross build --release --target x86_64-unknown-linux-gnu -p punks-standalone`. The stock `cross`
  image doesn't ship GTK3 dev headers, so this needs a custom image (or run
  `apt-get install libgtk-3-dev` in a `Cross.toml` pre-build step) — CI is the simpler path.
- **Windows, from macOS**: not worth fighting locally — the `wgpu` + MSVC toolchain doesn't
  cross-compile cleanly. Windows builds run natively on GitHub Actions instead, alongside macOS and
  Linux, in [`.github/workflows/release.yml`](.github/workflows/release.yml) on every `v*` tag push.

## Running

```
cargo run -p punks-standalone
```

Click **Browse...** to open a directory, then click any file to preview it.

## Architecture

The workspace has three library crates and one executable:

```
punks-audio ─┐
             ├─> punks-app ─> punks-standalone
punks-library┘
```

- **`punks-audio`** owns decoding, preview playback, resampling, waveform
  generation, audio metadata, and the pure analysis algorithms.
- **`punks-library`** owns SQLite roots/assets, reconciliation, tags, facts,
  overrides, and generated-data caches.
- **`punks-app`** owns preferences, browsing, tabs, selection, commands,
  workers, inspector, transport, and the application UI.
- **`punks-standalone`** is the native executable shell and renderer.

These are internal application boundaries, not a promise of a reusable browser
API. The full boundary rationale is in [the crate-boundary audit](docs/crate-boundary-audit.md).

## License

[MIT](LICENSE)
