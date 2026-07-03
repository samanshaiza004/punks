# Changelog

## v0.1.0 — first release (pre-1.0, unstable)

Initial release of punks2, a modular sample browser for musicians. Workspace crates
(`punks-core`, `punks-analysis`, `punks-library`, `punks-playback`, `punks-browser`, `punks-ui`,
`punks-standalone`) are all `0.1.0` and unstable — expect breaking changes before `1.0`.

### Features

- Browse directories of audio files (WAV, FLAC, MP3, OGG) with breadcrumb navigation and keyboard
  nav (W/S / arrow keys, auto-play on step)
- Production-sound aware: reads Broadcast Wave (`bext`) description/timecode, plays RF64 (>4 GB)
  field recordings, and previews long files (> 2 min) as a bounded, memory-safe window
- Instant replay from an in-memory decode cache; single reusable decode worker
- Interactive waveform visualizer: hover crosshair, click/drag scrub, source-relative axis
- Volume control for previews, persisted across sessions
- Recursive filename search from the current directory
- Multi-tab browsing with persistence across restarts
- Opt-in per-folder library (SQLite in a `.punks` folder): tag samples, filter by tags (AND),
  tags survive renames/moves; background scan with progress bar and reconciliation
- `punks-analysis`: dependency-free time-domain audio features (RMS, peak, zero-crossing rate)
  with a versioned analysis job queue in the library, ready for a future scheduler
- Remappable keybinds and configurable samples folder via Settings modal
- Restores the last directory on next launch
- Drag a sample out of the browser into another application (macOS/Windows)

### Distribution

- macOS and Linux: native builds; Linux cross-builds from macOS via `cross` (see README)
- Windows: built natively in CI (GitHub Actions matrix), not locally cross-compiled
- CI runs fmt/clippy/test on macOS, Linux, and Windows on every push and PR
