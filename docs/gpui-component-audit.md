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

## M4 — Button, Slider

M0's smoke test proved Button/Input compiled and ran, but never proved interactive
keyboard/focus behavior (screen access was declined twice in this engagement). M4 ships a real
Play/Stop `Button` and a real volume `Slider` (`crates/punks-app/src/browser/transport.rs`), so
this is where that debt comes due. Verified via the same test-only-entity `#[gpui::test]`
pattern used for M3's `TestResultsView`: a minimal harness wrapping the *real*
`gpui-component::button::Button`/`slider::Slider` configured exactly as `transport.rs` uses
them, with no `SampleBrowser`/audio device involved
(`crates/punks-app/src/browser/transport.rs`'s `#[cfg(test)] mod tests`).

| Component | Visual fit | Keyboard | Focus | A11y | Disabled | Notes |
|---|---|---|---|---|---|---|
| Button | PASS | PASS | PASS | PASS | PASS | `play_button_is_tab_reachable_and_activates_via_keyboard`: `window.focus_next` reaches it, Space and Enter each trigger `on_click` once (2 keyboard activations logged), and a plain mouse click still fires a third time — keyboard wiring doesn't replace or double-fire alongside mouse. `disabled_play_button_ignores_click_and_keyboard`: with `.disabled(true)`, neither a mouse click nor Space/Enter increments the click counter. A11y: `RUST_LOG=warn` over both tests printed no "focused element has no accessibility node" warning (the diagnostic the viability spike originally found) — `Button` sets `Role::Button`/`Role::Link` and a focus handle internally, so this is expected, not a lucky pass. |
| Slider | PASS | N/A* | — | — | N/A* | `volume_slider_set_value_updates_readable_state`: `.default_value(v)` seeds `SliderState::value()` correctly (this is what `new_volume_slider` uses to seed from `cfg.volume`), and `set_value` updates it. `volume_slider_change_event_is_observable_via_subscription`: `SliderEvent::Change` is observable via `cx.subscribe`, which is exactly what `new_volume_slider`'s `SampleBrowser::set_volume` wiring depends on. *Real drag-to-change (pixel-accurate mouse-down/move hit-testing through `SliderTrack`'s internal bounds) is gpui-component's own internal concern — not re-tested here, trusted as a maintained dependency the same way Button's click-dispatch internals are; `update_value_by_position` (`#[doc(hidden)]` but `pub`) is the exact method that path calls, and it emits the same `SliderEvent::Change` these tests already prove is observable. Keyboard-driven slider adjustment (arrow keys) and disabled-slider rendering are not exercised — Punks' volume slider is never disabled, and keyboard-adjustable sliders aren't a documented Punks requirement; noted as an open item if that changes. |

**Assessment:** both components are safe to build on. No unexplained gaps for Punks' actual
usage (a Play/Stop toggle button and a single continuous volume slider).

## M5 — Input, Checkbox, Select, Tooltip, Popover, Dialog

Button and Slider were finalized in M4. M5 ships real Input usage (`search.rs`'s search bar,
`inspector.rs`'s description field) and audits the remaining five components, most of which
have no real Punks consumer yet — see the Notes column for exactly what each row's evidence is
and isn't.

| Component | Evidence | Notes |
|---|---|---|
| Input | PARTIAL | Real consumers: `search.rs` (search bar → `SampleBrowser::search`/`clear_search`) and `inspector.rs` (description field → `set_description`). `search_input_change_event_carries_the_typed_value` (`browser/search.rs`) proves the `InputEvent::Change` subscription contract the wiring depends on. **Not verified**: real keyboard-driven typing and even plain `Tab`-focusing a rendered `Input` both panic under `TestAppContext` ("Test Windows are not backed by a real platform window") — confirmed by checking `gpui-component`'s own `InputState` test suite, which avoids rendering+keystroke-simulating an `Input` for the same reason (it mutates `InputState` directly instead). This is a platform-IME limitation of the test harness itself, not a defect found in `Input`; genuine interactive typing/focus needs either a real windowed run or `VisualTestAppContext` (real platform rendering), neither available in this engagement (screen access declined). |
| Checkbox | PASS (isolated) | No real Punks consumer yet. `checkbox_click_toggles_and_reports_the_next_state` (`browser/component_audit.rs`) proves click toggles `checked` state via the same external-callback pattern (`on_click(\|next, window, cx\| ...)`) a real usage would use. Keyboard activation/focus/disabled not separately re-tested — structurally the same `Disableable`/focus-handle plumbing as Button (already proven in M4), not re-verified in isolation to avoid duplicating that coverage. |
| Popover | PASS (isolated) | No real Punks consumer yet. `popover_opens_on_trigger_click_and_dismisses_on_outside_click` (`browser/component_audit.rs`): clicking a real `Button` trigger opens the popover (`content` renders, confirmed via `debug_bounds`), and a click outside both trigger and content dismisses it (content stops rendering, `on_open_change` fires `true` then `false`). This is the exact `.trigger(...).content(...).on_open_change(...)` pattern a real "assign new tag" or "edit override" popup would use. Escape-to-dismiss specifically was not separately exercised (outside-click was used instead, since it's the more common dismiss path and was the one already covered by prior art in gpui-component's own equivalent test). |
| Select | NOT VERIFIED | No real Punks consumer, and no isolated audit test written — `Select`/`SelectState` are generic over a `SearchableListDelegate`, requiring more harness setup than the time budget for this milestone covered. Flagged as an open item: audit before the first real Select usage (e.g. an instrument/key picker) lands. |
| Tooltip | NOT VERIFIED | Same as Select — no real consumer, no isolated audit test. `Tooltip::new(text)` is a simple hover-content wrapper; likely low-risk, but not independently confirmed. Flagged as an open item. |
| Dialog | NOT VERIFIED | Same as Select/Tooltip — no real consumer. `dialog/` requires window-level `open_dialog`-style machinery (distinct from Popover's simpler anchored-overlay model) that wasn't explored deeply enough this milestone to audit responsibly. Flagged as an open item: audit before the first real Dialog usage (e.g. a delete-library confirmation) lands. |

**Assessment:** Input/Checkbox/Popover have real evidence (Input: real usage + a targeted
subscription-contract test, given the IME test-harness limitation; Checkbox/Popover: isolated
but real-pattern tests). Select/Tooltip/Dialog are genuinely unverified — reported honestly as
open items rather than assumed safe, since none has shipped in real Punks code yet to force the
issue.

### Re-measured dependency footprint (vs. M0's baseline)

Same machine (macOS 26.5.2, arm64, rustc 1.97.1), clean builds (`rm -rf target` before each) —
same methodology as M0's measurement.

| Metric | M0 baseline | M5 (now) | Delta |
|---|---|---|---|
| Packages in `Cargo.lock` | 1042 | 989 | −53 (resolver settled slightly smaller, not a regression) |
| Clean `cargo build --workspace` (debug) | 195.01s | 232s (3m 52s) | +37s (~+19%) — M1–M5's own code, plus real usage now compiling more of `gpui-component`'s `ui` crate (Button/Input/Slider/Checkbox/Popover/uniform_list/canvas) that M0's minimal smoke test never touched |
| Clean `cargo build --release -p punks-standalone` | 355.89s | 249s (4m 09s wall, but under the 355.89s M0 figure) | −107s — faster despite more code; plausibly machine/thermal/scheduling variance between runs rather than a real structural improvement, reported as measured rather than assumed |
| `punks-standalone` release binary size | 14MB (pre-GPUI) / 20MB (M0's minimal smoke example, unrepresentative) | 26.6MB (27,903,232 bytes) | First measurement against the *real* app — M0 flagged its own 20MB number as a floor, not representative; 26.6MB is real usage (Button, Input ×2, Slider, uniform_list, canvas, Checkbox/Popover in test code only) and lands where M0 predicted ("somewhere above 20MB") |

**Assessment, updated:** the package-count and (apparent) release-build-time numbers didn't grow
disproportionately — the debug build time did, tracking real code growth (five new milestones of
`punks-app` source plus more of `gpui-component`'s surface actually compiling), not the dependency
graph itself widening. Nothing here changes M0's original call: the cost is real but not
disproportionate enough to abandon "commodity controls by default." Revisit if Select/Tooltip/
Dialog's eventual real usage (or a further growth in `gpui-component` surface) pushes debug build
time meaningfully past this milestone's ~4-minute clean-build floor.
