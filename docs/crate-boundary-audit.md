# Crate-boundary audit

This audit is based on the current checkout before the consolidation. The
checkout still contains the ImGui UI (`punks-ui` and the ImGui renderer in
`punks-standalone`); no GPUI application source is present. The boundary
decision below therefore evaluates the code that exists and preserves its
behavior while removing artificial crate seams. A GPUI port, if it is brought
into this checkout later, belongs inside `punks-app` rather than creating a
toolkit-neutral layer.

## Decision

The resulting graph is:

```text
punks-audio ─┐
             ├─> punks-app ─> punks-standalone
punks-library ─┘
```

`punks-core` is not retained. Its config is application state, its directory
listing types are only used by the browser UI, and its recursive walker is
only borrowed by the library as a convenience. The library owns its own scan
input; the application owns its browse listing and preferences.

## Current crates

### `punks-core` — DELETE

- **Invariant protected:** filesystem listing/search behavior and JSON
  preferences.
- **Consumers:** `punks-browser` uses listing/search and re-exports the types;
  `punks-ui` uses config; `punks-library` uses the recursive walker. There is
  no consumer outside this Punks workspace.
- **Dependencies isolated:** `serde`, `serde_json`, `dirs`, and `log` are
  currently bundled behind the crate even though config is application-owned.
- **Would merging materially worsen tests or ownership?** No. Config and
  browse listing tests move beside `punks-app`; library scan tests already
  belong beside SQLite reconciliation and can use a local scanner.
- **Independent outside Punks today?** No. The boundary exists because the
  old browser and UI were split, not because these are shared domain values
  between independent products.
- **Classification:** **DELETE**. Keep only the code in the owner that has a
  real consumer.

### `punks-analysis` — MERGE

- **Invariant protected:** pure time-domain feature calculations and their
  pipeline version.
- **Consumers:** the browser analysis worker and one browser integration test.
  After the browser/UI merge there is one application consumer.
- **Dependencies isolated:** it is std-only, while audio has Symphonia,
  CPAL, Rubato, Lofty, and the decode cache.
- **Would merging materially worsen tests or ownership?** No. The algorithms
  remain a private module with their existing unit tests. A separate crate
  would not provide a second release or product boundary today.
- **Independent outside Punks today?** No. There is no second consumer and no
  independent distribution or release cadence.
- **Classification:** **MERGE** into `punks-audio`. Analysis is part of the
  audio pipeline already and the requested boundary is responsibility-based,
  not dependency-aesthetic.

### `punks-library` — KEEP

- **Invariant protected:** SQLite schema/migrations, asset identity across
  moves, user-data preservation, generated-cache invalidation, and query/cache
  semantics.
- **Consumers:** the application and its library/analysis integration tests.
- **Dependencies isolated:** `rusqlite` and `sha2` stay out of the application
  and audio crates; SQLite connection ownership remains testable without a UI
  or audio device.
- **Would merging materially worsen tests or ownership?** Yes. Combining it
  with the app would mix SQLite transactions and cache ownership into the UI
  state machine; combining it with audio would mix storage with file decoding
  and metadata writes.
- **Independent outside Punks today?** It is a concrete, separately testable
  storage responsibility with a distinct dependency graph. Its boundary is
  justified by data-integrity ownership, not by a future DAW claim.
- **Classification:** **KEEP**.

### `punks-playback` — MERGE/RENAME

- **Invariant protected:** decode, preview playback, real-time callback
  safety, resampling, waveform extraction, and metadata read/modify/write.
- **Consumers:** the browser orchestration and playback metadata tests.
- **Dependencies isolated:** Symphonia, CPAL, Rubato, Lofty, and LRU cache
  implementation are isolated from SQLite and the UI.
- **Would merging materially worsen tests or ownership?** No if it remains a
  library crate. Its modules already share the audio lifecycle and error
  boundary; only the misleading playback-only name is removed.
- **Independent outside Punks today?** Yes as a concrete audio subsystem that
  can compile and test without the app. No future host/audio-sink API is
  introduced.
- **Classification:** **MERGE** `punks-analysis` into it and rename it to
  `punks-audio`.

### `punks-browser` — DELETE/MERGE

- **Invariant protected:** tabs, selection, search, library orchestration,
  commands, health checks, worker coordination, and transport state.
- **Consumers:** only `punks-ui` and `punks-standalone`; its public exports
  are immediately re-exported for that pair. It is the application's domain
  object, not a second product.
- **Dependencies isolated:** none that deserve a crate boundary; it already
  depends on core, analysis, library, and playback and bridges all of them.
- **Would merging materially worsen tests or ownership?** No. The pure state
  helpers and integration tests move into `punks-app`; SQLite and audio remain
  lower-crate owners.
- **Independent outside Punks today?** No. The only rationale is the stale
  “embed the browser in a future DAW” claim.
- **Classification:** **MERGE** into `punks-app`; remove the crate.

### `punks-ui` — DELETE/MERGE

- **Invariant protected:** the current toolkit-specific panel, settings,
  inspector, metadata editor, selection presentation, and theme.
- **Consumers:** only `punks-standalone`.
- **Dependencies isolated:** ImGui, `rfd`, and the external painter are
  isolated from browser/audio/library code, but that is no longer useful once
  the UI is the application itself. The current checkout is still ImGui; a
  later GPUI implementation should replace this module in `punks-app` rather
  than preserve a UI abstraction crate.
- **Would merging materially worsen tests or ownership?** No. The panel and
  theme are one application surface and already depend directly on browser
  APIs and config.
- **Independent outside Punks today?** No.
- **Classification:** **MERGE** into `punks-app`; remove the crate.

### `punks-standalone` — KEEP

- **Invariant protected:** process startup, native window/event loop, GPU
  renderer, clipboard, drag-out, and application shutdown.
- **Consumers:** it is the executable entry point and has no library consumer.
- **Dependencies isolated:** Winit/WGPU/renderer/platform integration stay out
  of the application-domain library, audio, and SQLite crates.
- **Would merging materially worsen tests or ownership?** Yes. It would force
  platform/GPU setup into app-domain tests and make headless state tests
  compile against the executable shell.
- **Independent outside Punks today?** Yes as the platform entry point, even
  though it is not a reusable library.
- **Classification:** **KEEP** as the executable entry point.

## Resulting ownership

- `punks-audio`: decode, playback, resampling, waveforms, metadata, and pure
  analysis algorithms.
- `punks-library`: SQLite-backed roots/assets, reconciliation, tags, facts,
  overrides, generated analysis persistence, and waveform cache persistence.
- `punks-app`: config, directory browsing, tabs, selection, commands, health,
  workers, inspector, transport, and the application UI.
- `punks-standalone`: native executable/window/renderer shell.

No compatibility re-exports of the deleted crate paths, future DAW interfaces,
generic service traits, or toolkit-neutral UI crate are retained.
