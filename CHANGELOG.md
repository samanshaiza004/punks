# Changelog

## v0.2.0 — analysis pipeline, editable metadata, fact overrides (pre-1.0, unstable)

Workspace crates are all `0.2.0` and unstable — expect breaking changes before `1.0`.

### Features

- **Analyzer registry + background analysis pipeline**: `punks-analysis` gained a trait-based
  analyzer contract and registry; a global background worker in `punks-browser` runs analyzers
  per-file through a versioned, per-asset job queue and orchestrates results back to the UI
- **Duration analyzer**: length and peak level (dBFS) now shown for the selected file
- **Filename analysis**: a `Filename` analyzer parses instrument/BPM/key straight from filenames
  into typed `Fact`s, shown as a facts line in the browser
- **Fact overrides**: correct or supply facts (instrument/BPM/key) per-file via an override popup;
  overrides are tri-state (set / cleared / explicitly marked absent via an N/A button) and take
  precedence over analysis-derived values
- **Facts-based filtering**: filter the library by BPM/key/instrument, not just tags
- **Value provenance**: every resolved metadata field now carries where it came from — embedded
  file, user override, analysis, or project — via a new `MetadataSource`/`Sourced<T>` model, so
  the UI can label each value with its origin instead of presenting everything as equally
  authoritative
- **Metadata backend abstraction**: a `Metadata`/`Field`/`Capability`/`MetadataBackend` layer
  replaces ad-hoc per-format code — a native WAV/BWF writer plus a Lofty-backed reader/writer for
  FLAC/MP3/OGG, gated per-field by what each format can actually store
- **Editable embedded description**: the BWF `bext` description field is directly editable in the
  UI and saves back into the file atomically (new `write_atomically` primitive: write to a sibling
  temp file, fsync, rename — never touches the original file in place)
- **Multi-select + batch tagging**: select multiple files in the browser and apply a tag to all of
  them at once
- **Real OS clipboard for imgui**: Ctrl+C/Ctrl+V in text fields (e.g. the error line, description
  editor) now reach the real system clipboard instead of imgui's internal no-op fallback

### Fixes

- Nested-library resolution now picks the longest matching library path instead of the first
  match, fixing incorrect library selection when libraries are nested inside one another
- Error-job retry semantics in the analysis queue are now consistent: errored jobs no longer get
  silently reclaimed/retried outside the intended pipeline-version-bump path
- Library schema migrations are now serialized, fixing a race on concurrent first-open
- Assorted UI fixes across the analysis/metadata rollout (priority queue ordering, filename
  analysis display, library-backed bug fixes)

### Removed

- Deleted dead metadata API surface left over after the Backend rewire

### Internal

- WAV metadata test coverage: foreign-chunk-survives-edit, round-trip, capability, and atomicity
  tests guard the new Backend abstraction
- Integration tests for the analysis pipeline in `punks-browser`

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
