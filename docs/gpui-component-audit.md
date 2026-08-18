# gpui-component audit

Tracks whether/how [`gpui-component`](https://github.com/longbridge/gpui-component) is used for
commodity UI controls in the GPUI production rewrite (see the milestone plan referenced from the
PR, and `docs/gpui-viability.md` for the underlying GPUI viability spike). Started at M0
(dependency resolution + cost measurement); grows through later milestones as real usage exists
to audit specific components against.

## M0 — Dependency resolution

### Revision pair

| Crate | Source | Commit |
|---|---|---|
| `gpui` / `gpui_platform` | `zed-industries/zed` | `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` (frozen by the viability spike, unchanged) |
| `gpui-component` | `samanshaiza004/gpui-component` (fork of `longbridge/gpui-component`) | `4cf3cddc0609a220aed0323678fe5241fdbfd7d6` |

### Why a fork was needed

`gpui-component`'s own `Cargo.toml` depends on `gpui`/`gpui_platform`/`gpui_web`/`gpui_macros`/
`reqwest_client` via `git = "https://github.com/zed-industries/zed"` with **no `rev`/`branch`/
`tag`** — an unqualified reference that Cargo resolves to whatever the repository's default
branch HEAD is at resolution time. Punks' own dependency on `gpui`/`gpui_platform` pins an exact
`rev`. These are two different Cargo `SourceId`s for the same underlying crate, and Cargo will
not unify them:

```text
error: failed to select a version for `cocoa`.
    ... required by package `gpui v0.2.2 (https://github.com/zed-industries/zed#aa371861)`
    ... which satisfies git dependency `gpui` of package `gpui-component ...`
  previously selected package `cocoa v0.26.1`
    ... which satisfies dependency `cocoa = "^0.26.0"` (locked to 0.26.1) of package `drag v2.1.0`
```

The visible symptom was a `cocoa` version conflict (`gpui_macos` needs exactly `cocoa 0.26.0`;
`gpui-component`'s independently-resolved `gpui` landed on a different upstream commit that
disagreed), but the real problem — confirmed via `cargo tree -i gpui` — was **two different
`gpui` git commits in one dependency graph** (`8b1497db` from Punks' own pin, `aa371861` from
gpui-component's unpinned reference, which had already moved forward on zed's `main` in the time
between the spike and this work).

Cargo's `[patch]` mechanism cannot fix this: `[patch."https://github.com/zed-industries/zed"]
gpui = { git = "...", rev = "8b1497db..." }` fails with `patch for 'gpui' points to the same
source, but patches must point to different sources` — `[patch]` requires the replacement to be a
genuinely different source (e.g. a fork), not the same repository pinned to a different ref.

So, per the task brief's own stated preference ("prefer a small fork/patch over moving Punks'
GPUI baseline"): forked `gpui-component` to `samanshaiza004/gpui-component` (matching this
repo's existing fork convention — `imgui`/`imgui-painter` are already forked under the same
account for an analogous reason) and made a five-line change to its root `Cargo.toml`, adding
`rev = "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc"` to its `gpui`, `gpui_platform`, `gpui_web`,
`gpui_macros`, and `reqwest_client` workspace dependencies (all from the same zed monorepo, pinned
together for internal consistency). No other changes. With both sides now referencing the
identical `git URL + rev`, Cargo unifies them into one source automatically — no `[patch]` needed
at all.

After that, `cocoa`/`dispatch2` still needed a plain `cargo update` (not piecemeal `-p --precise`,
which failed because Cargo's incremental single-package update doesn't consider multi-package
combined solutions atomically) to let the resolver find `cocoa 0.26.0` + `dispatch2 0.3.1`
together, which satisfy both `gpui_macos`'s exact requirement and `drag`'s/`rfd`'s looser `^0.3.x`
ranges. Both are the only versions of `cocoa`/`dispatch2` now used by anything gpui-related; a
second `cocoa 0.25.0` also exists in the lockfile for an unrelated, semver-incompatible caller
(harmless multi-version coexistence, not a conflict).

**Verification:** `cargo tree -i gpui` and `cargo tree -i gpui_platform` both show exactly one
resolved package with one source URL+rev across the whole graph — Punks' own path dependency and
`gpui-component`'s transitive dependency both resolve to the same node.

### Dependency-footprint measurement

All measurements on this machine (macOS 26.5.2, arm64, rustc 1.97.1), clean builds
(`cargo clean` / `rm -rf target/release` before each), "before" captured via `git stash` back to
the pre-M0 commit and restored afterward.

| Metric | Before | After | Delta |
|---|---|---|---|
| Packages in `Cargo.lock` | 520 | 1042 | +522 (~2x) |
| Unique crate names in `cargo tree` | — | 522 | — |
| Clean `cargo build --workspace` (debug) | 80.55s | 195.01s | +142% |
| Clean `cargo build --release -p punks-standalone` | 90.45s | 355.89s | +293% |
| `punks-standalone` release binary size | 14MB | 15MB* | +1MB* |
| Incremental rebuild (touch one `punks-app` file, debug) | — | 6.05s | — |

\* `punks-standalone` doesn't actually call into `gpui`/`gpui-component` yet (that's M1) —
`gpui-component`'s dependencies are declared but unreferenced from any real code path, so the
linker strips them from the final binary via dead-code elimination. The 14MB→15MB delta is
**not representative** of the real future cost. A more honest floor estimate: the M0 smoke-test
example (`crates/punks-app/examples/gpui_component_smoke.rs`, which genuinely renders a real
`Button` and `Input`) links to a **20MB** release binary — a +43% size increase over the current
14MB imgui-based `punks-standalone`, for a UI with exactly one button and one text field. Expect
the real app's eventual binary size to land somewhere above 20MB as more components get used, not
below it.

**What's driving the cost:** `gpui-component`'s `crates/ui` pulls substantially more than the
control widgets it exposes — confirmed present in the dependency graph: `ropey` (rope text
buffer, for its code-editor-capable text input), `syntect` (syntax highlighting), `markdown`/
`html5ever`/`xml5ever`/`markup5ever` (markdown/HTML rendering support, presumably for its
markdown-table/rich-text features), `lsp-types` (language server protocol types — not used by
anything a sample browser needs), `notify` (filesystem watching), plus its own `smol`-based async
runtime pieces. None of this is required by the Input/Button/Checkbox/Select/Slider/Tooltip/
Popover/Dialog surface Punks actually wants.

**Assessment:** the cost is real but not disproportionate enough to abandon the "commodity
controls by default" approach this early — build-time cost is a one-time-per-clean-build tax
(incremental rebuilds, the actual day-to-day dev-loop cost, are fast at 6s), and the binary-size
cost (+6MB for a minimal UI) is not alarming for a desktop audio tool. If later milestones' real
usage stays confined to the control types actually needed and the size/build cost keeps growing
disproportionately as more of `gpui-component`'s surface gets pulled in, revisit whether a
narrower fork (stripping the markdown/LSP/syntax-highlighting modules Punks will never use) is
worth the added fork-maintenance burden. Not revisited now — noted for later milestones to watch,
not acted on preemptively.

### Button + Input smoke test

`crates/punks-app/examples/gpui_component_smoke.rs`: a real `gpui_component::input::Input` (via
`InputState` + change-event subscription) and a real `gpui_component::button::Button` with a
click handler, wrapped in `gpui-component`'s required `Root` view, rendered in a GPUI window built
against the pinned rev pair above.

- **Compiles:** yes, cleanly, both debug and release (`cargo build -p punks-app --example
  gpui_component_smoke`, and `--release`), no warnings from the example's own code.
- **Runs:** yes — launched (wrapped in a minimal `.app` bundle, since an unbundled binary has no
  window-manager identity, same finding as the earlier GPUI viability spike), stayed alive with
  no crash and no error/panic in logs (`RUST_LOG=info`) over several seconds, using window-
  system-consistent CPU/memory (small, non-zero, growing slightly — consistent with a real
  render loop, not a stall).
- **Interactive verification (click, focus, keyboard typing into the Input): not performed.**
  The user declined the screen-access grant needed to drive/observe this app via computer-use
  automation. This is recorded honestly rather than inferred from "it didn't crash" — process
  liveness confirms the window opened and is rendering, but does **not** confirm click handling,
  focus, or keyboard input actually work correctly. If this matters before later milestones lean
  further on `gpui-component`'s interactive controls, it should be verified directly (either via
  a future computer-use session with access granted, or manual testing).

### Theme integration approach

Not decided in M0 — deferred to M1 per the plan, where it gets discovered empirically through
real `Button`/`Input` usage rather than chosen in the abstract. This example currently uses
`gpui-component`'s own default theme (no Neon Live palette wiring yet); M1's theme work picks up
from here.

## Later milestones (not yet audited)

Full per-component PASS/PARTIAL/FAIL table (visual fit, keyboard operation, focus behavior,
accessibility role/name/state, disabled/read-only behavior, clipboard/IME, macOS/Windows/Linux
behavior) across Checkbox, Select, Slider, Tooltip, Popover, Dialog — populated once M5's
inspector/search/settings work gives real usage to audit against.
