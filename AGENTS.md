# AGENTS.md — Operating manual for working on punks2

Read this before writing any code. When this file and your instincts disagree, this file wins.

## 1. What this project is

punks2 is a keyboard-first sample browser for musicians and production-sound people, in Rust.
Browse folders → audition instantly → tag, correct, and describe what you hear. It handles the
formats real sample work involves: plain WAV/BWF, RF64 field recordings >4 GB, FLAC/MP3/OGG,
and the metadata professionals rely on (bext description, timecode).

**What it is not:** a DAW, a file manager, a cloud service, a plugin, or a Sononym clone. It does
not sync, upload, watch folders, or auto-organize. It never modifies audio _content_.

**Long-term vision:** the user's knowledge about their sounds — tags, corrections, descriptions —
outlives any database, because files stay authoritative and everything else is either user data
kept safe or a cache that can be rebuilt from the files.

## 2. Architectural principles (with why)

1. **Strict crate DAG** — `core → {analysis, library, playback} → browser → ui → standalone`.
   Lower crates never know upper crates exist. _Why:_ every crate is testable without a GUI or
   an audio device, and features land as vertical slices without cross-contamination.
   - `punks-core`: filesystem walking + config. No workspace deps.
   - `punks-analysis`: pure DSP + filename parsing. **Zero dependencies, std only.** No I/O.
   - `punks-library`: SQLite storage. Never names an analyzer; stores opaque typed facts.
   - `punks-playback`: decode, audio out, file metadata read/write.
   - `punks-browser`: the headless application. The ONLY crate the UI talks to.
   - `punks-ui`: one imgui panel. No SQLite, no decoding, no file writes — it calls browser methods.
2. **Three classes of data** — every schema/API decision starts here:
   - **User data** (tags, overrides, descriptions the user typed): never regenerated, never
     dropped, migrated additively, survives rename/move via asset identity.
   - **Generated data** (analysis facts, job queue, waveform cache): disposable; DROP+CREATE
     migrations are fine; deleting it only forces recompute.
   - **The files themselves**: authoritative. The DB is a rebuildable index of them.
3. **Files are sacred.** Two invariants, non-negotiable:
   - Every file write goes through `write_atomically` (sibling temp, same extension, fsync,
     rename; original untouched on any failure).
   - **Punks never discards metadata it doesn't understand.** Metadata writes are
     read-modify-write; unknown chunks/tags/pictures survive byte-for-byte.
4. **Capability, not format checks.** UI asks `backend.capability(Field).can_write()`, never
   `if extension == "wav"`. _Why:_ formats diverge; conditionals rot.
5. **The build is the smallest correct solution.** See §3.

## 3. Engineering rules

Before writing code, in order: does this need to exist? → does std solve it? → does the platform?
→ does an existing dependency? → can it be simpler? → only then write it.

- Prefer deletion over addition; boring over clever; fewer files/deps/abstractions.
- No new dependency if existing tools suffice. Current deps are the budget, not the floor.
- No trait, generic, builder, or wrapper with a single implementation unless the second
  implementation is already scheduled (e.g. `MetadataBackend` earned its trait: Wave + Lofty).
- Enum dispatch over `dyn` when the set of implementations is closed.
- When two solutions are equally simple, choose the more correct/robust one.
- Never cut corners on: security, data integrity, input validation at trust boundaries,
  error handling that prevents data loss, accessibility, platform quirks, explicit requirements.

**Intentional shortcuts** are marked in code:

```
// ponytail: <simplification>. <limitation / failure mode>. Upgrade path: <path>.
```

Example (real, from punks-library):

```rust
// ponytail: O(files x rows) linear scans; fine for tens of thousands of assets.
// Upgrade to prebuilt indices if libraries grow past that.
```

A ponytail is a promise that the ceiling is known. No unmarked shortcuts.

## 4. Mistakes weaker models make here — and the rule that prevents each

1. **Reaching for a crate std already covers** (walkdir, anyhow, thiserror, tokio…).
   _Why it happens:_ training bias toward popular crates. _Rule:_ new dependency requires a
   written justification against std. _Example:_ every error type here is a hand-written enum
   with a `Display` impl — no thiserror.
2. **Editing files in place.** _Why:_ it's fewer lines. _Rule:_ all file writes go through
   `write_atomically`; a metadata write that can't be atomic is refused (RF64 is read-only for
   exactly this reason). _Example:_ the bext writer streams a full spliced copy, never seeks
   into the original.
3. **"Fixing" a migration by regenerating a user table.** _Why:_ DROP+CREATE is easy. _Rule:_
   user tables (tags, fact_overrides) migrate via rename/copy/drop inside the one IMMEDIATE
   transaction in `open_at`; only Generated tables may DROP+CREATE. _Example:_ SCHEMA_V6.
4. **Touching SQLite or decoding in the draw loop.** _Why:_ it's where the data is needed.
   _Rule:_ per-frame reads come from `LibraryContext` caches; mutations go through browser
   methods that update caches. One documented exception: `job_status` point query.
5. **Making state transitions untestable.** _Why:_ `SampleBrowser` looks like the natural home.
   _Rule:_ `SampleBrowser` needs a real audio device and cannot be unit-tested — extract pure
   functions for logic (see `selection_after_toggle`, `restore_tab_plan`,
   `adjust_active_after_close`) and test those.
6. **Swallowing worker errors or unwrapping them.** _Rule:_ workers `log::warn!` and continue;
   user-initiated paths set `last_error` so the UI shows it. Panics are for programmer bugs only.
7. **Format conditionals in the UI** (`if wav`). _Rule:_ ask `capability()`.
8. **Adding speculative fields/options/config.** _Rule:_ flag it as a TODO task instead of
   building it. If it must exist as shape (a schema column, an enum), it needs a comment naming
   the concrete future feature, and it must be trivially deletable.
9. **Blocking the UI thread with file-sized I/O.** _Rule:_ anything proportional to file size
   belongs on a worker (decode, peaks, scan, analysis already are).
10. **Churn:** renaming/moving working code while doing a feature. _Rule:_ smallest diff that is
    correct; refactors are their own task.

## 5. Quality bar — every change, measurable

A change is "done" only when ALL of these pass, in this order:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p punks-standalone
```

Additional acceptance criteria:

- Non-trivial logic ships with one minimal runnable verification (a small test or an assert
  self-check). No new test frameworks or fixtures unless requested.
- Any change to a metadata writer MUST extend the permanent **"foreign metadata survives"**
  test category: editing one field leaves unknown chunks/tags/pictures byte-identical.
- No new `pub` item without a consumer (tests don't count as the consumer).
- Worker/thread additions state their shutdown story, even if it's a documented ponytail.
- Report outcomes honestly: failing tests are reported with output, skipped steps are named.

## 6. Decision framework when uncertain

- **Blocked on a fact you can check** (does this API exist? does the test pass?) → check it.
- **Blocked on product intent** → state the blocker in one line, ask at most ONE question.
- **Otherwise** → proceed with the best reasonable assumption and say what you assumed.
- **Stop** when the quality bar passes and the task's acceptance criteria are met. Do not
  gold-plate, do not refactor adjacent code.
- **Defer** (create a flagged TODO, don't build) anything speculative: undo/Command system,
  health validator, additional metadata fields — the pattern is established.
- **Simplify** whenever a reviewer would need the comment to understand the cleverness.
- **Delete** anything with no consumer. Assume every new file, dependency, abstraction, hook,
  utility, class, and component is guilty until proven necessary.

## 7. Repository conventions

**Naming.** Crates are `punks-*`. No `Manager`, `Service`, `Util`, `Helper` types. Types name
the _information_ (`Metadata`), not the mechanism; backends name the _storage_ (`WaveBackend`).
Pure transition functions read as `noun_after_event` (`selection_after_toggle`).

**Testing.** std-only. Temp dirs/files named with pid + nanos (see `TempRoot`); clean up with
`Drop` or best-effort `remove_file`. Tests state _why_ in a sentence when the point is a
negative (guards matter as much as matches — see the filename-parser tests). Integration tests
exercise the public API exactly as the real caller does.

**Dependencies.** Each Cargo.toml is intentionally minimal; punks-analysis stays at zero.
Version bumps and new deps are their own commits.

**Architecture.** New capabilities land as vertical slices: library API → browser orchestration
→ UI, each verifiable alone. Vocabulary bridges (analysis report ↔ library facts) live in
punks-browser only — the two lower crates never learn each other's types.

**Database.** One SQLite DB per library root at `.punks/library.db`, WAL, `busy_timeout`,
foreign keys ON. Migrations are numbered `SCHEMA_Vn` constants applied in one IMMEDIATE
transaction (concurrent first-open is a tested scenario). Relative paths, `/`-separated, so a
root can relocate across OSes. `WITHOUT ROWID` for junction/queue tables. CHECK constraints
encode "exactly one typed column set".

**UI.** One imgui panel; immediate mode. Per-frame reads from caches only. Lists use
`ListClipper` + width-adaptive columns. Popups are opened via request flags at panel scope (ID
stack). Anything destructive sits behind a confirmation and a tooltip that says what will be
written where. Layout heights derive from font metrics, not pixel guesses. Errors show in a
read-only, copyable input.

**Background workers.** Two shapes, choose deliberately:

- `RequestSlot` (latest-wins, one persistent thread) for work where only the newest request
  matters: decode, peaks.
- `mpsc` FIFO (durable, every message processed) for work that must not be dropped: analysis
  drains, with a priority lane checked before every claim.
  Workers own their SQLite connections. Results return over channels; the UI thread folds them in
  during `poll()`, capped per frame.

**Threading.** UI thread owns all state mutation. The audio callback uses only atomics plus a
control-owned published buffer; it never takes a lock. Each published buffer owns its cursor and
playing atomics, and the control side retires the old owner only after callback acknowledgement.
Read `docs/audio-realtime-contract.md` before touching the handoff or callback.

**Metadata.** The file is authoritative; the DB description is a cache of it. All reads/writes
go through `Backend::for_path` → `MetadataBackend`. WAV/BWF uses our own writer; everything
else lofty. RF64 is write-refused. Per-field `Capability` gates the UI. Never discard what you
don't understand.

**Persistence.** Config is JSON via serde in `dirs::config_dir()`, all fields `#[serde(default)]`
so old configs always load; unknown futures degrade, never fail. Waveform cache is a versioned
magic-header binary with a size+mtime validity stamp — any mismatch means recompute, never
trust.

**Errors.** One error enum per crate, `Display` + `std::error::Error` by hand, `From` impls for
the boundaries it wraps. Errors carry the path (`{path:?}: {e}`). No `anyhow`/`thiserror`.

**Logging.** `log` crate only (env_logger in the binary). `warn!` = recoverable/background,
`error!` = user-visible failure. Never `println!`. Log messages name the operation and path.

**Public APIs.** `punks-browser` re-exports everything the UI needs — the UI imports from
`punks_browser` and `punks_core::config` only. Crate roots re-export their public surface.
Doc comments explain _why_ and name their consumers; internals are `pub(crate)` the moment the
public consumer disappears.

## 8. Communication

Answer first. Concise, direct, minimal words. No preamble, no praise, no restating the request,
no narrating reasoning. Longer explanations only when complexity or the user demands them.

Good:

```
Done. File patched.
Need repo path.
2 bugs: null case, off-by-one.
No. Breaks cache.
```

Preserve: facts, constraints, decisions, open issues.
Remove: fluff, repetition, dead ends.

When blocked: one-line blocker, at most one question, otherwise proceed on the best assumption.
Use the smallest useful tool; reuse results you already have; summarize tool output, never dump it.
