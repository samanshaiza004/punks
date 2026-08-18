# GPUI viability spike

**Question:** Can GPUI support the non-negotiable workflows Punks needs on macOS, Windows, and
Linux well enough that we should delete the existing imgui/winit/wgpu frontend?

**Scope of this spike:** one throwaway application at `spikes/gpui-viability/`, built and
manually verified against the real `punks-audio` crate (the renamed/merged successor of
`punks-playback` + `punks-analysis` — that rename happened elsewhere on this branch mid-spike;
see §7). Nothing in `punks-ui`, `punks-browser`, or `punks-standalone` was touched. This
document reports what was actually run and measured, not what should theoretically work.

**Environment reality check:** this spike was built and run on a single macOS (arm64) machine.
There was no Windows or Linux hardware available in this environment, and no real DAW to drag
into. macOS findings below are empirically verified (built, run, clicked, dragged). Windows and
Linux findings are **source-verified only** — read directly from GPUI's actual backend
implementations in the pinned commit, not run. Those are marked explicitly throughout; treat
them as "here is what the code does," not "here is what happened."

## 1. GPUI revision/version tested

- **Published crate:** `gpui` 0.2.2 on crates.io, published 2025-10-22 — the only version ever
  published standalone. Investigated first; **cannot** satisfy this spec (see §7.1).
- **Actually used:** `zed-industries/zed` git, pinned to commit
  `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` (HEAD of `main` as of 2026-08-17, the day this spike
  was built). Declared in [`spikes/gpui-viability/Cargo.toml`](../spikes/gpui-viability/Cargo.toml)
  as a `git = ..., rev = ...` dependency, isolated in its own Cargo workspace (see §7.4 for why).
- Rust edition 2024, rustc 1.97.1. Same toolchain family Punks already uses.

## 2. OS matrix

| OS | Built | Ran | Manually interacted with | Drag-out tested |
|---|---|---|---|---|
| macOS 26.5.2 (arm64) | ✅ | ✅ | ✅ (mouse, keyboard, real audio device) | ✅ **confirmed working** |
| Windows | ❌ not attempted | ❌ | ❌ | ❌ (source-read only, see §4) |
| Linux (Wayland) | ❌ not attempted | ❌ | ❌ | ❌ (source-read only, see §4) |
| Linux (X11) | ❌ not attempted | ❌ | ❌ | ❌ (source-read only, see §4) |

No Windows or Linux machine was available in this session. `gpui_windows` and `gpui_linux` are
real, substantial backends in the pinned commit (Win32 + DirectWrite; Wayland/X11 + cosmic-text) —
both actively used to ship Zed on those platforms — so "will it build and run at all" is low risk.
What's genuinely unverified is Windows/Linux-specific interaction correctness and, specifically,
external drag-out (§4).

## 3. Feature matrix

| Requirement | Status | Notes |
|---|---|---|
| Open native window, macOS | **Works** | `gpui_platform::application().run(...)`, `cx.open_window(...)`. |
| Clean startup/shutdown | **Works** | `cx.on_window_closed` → `cx.quit()` when last window closes; verified: closing the window ends the process with no zombie, no log errors. |
| No busy repaint while idle | **Works** | Render is `cx.notify()`-driven, not polled. Measured: >60s fully idle produced **zero** additional render-loop passes (see §6). |
| 10,000-row virtualized list | **Works** | `uniform_list`. Only the visible slice is ever built; confirmed via the same pattern GPUI's own test suite uses (`visible_range` assertions in `uniform_list.rs`'s internal tests). |
| Keyboard Up/Down navigation | **Works** | `UniformListScrollHandle::scroll_to_item(ix, ScrollStrategy::Nearest)`, verified visually scrolling through all 10k filtered rows. |
| Selection follows scroll | **Works** | Same mechanism; visually confirmed rows highlight and the viewport follows. |
| Text input, native keyboard editing | **Works, with real effort** | No batteries-included `TextInput` widget exists in GPUI. Must hand-implement `EntityInputHandler` (insert/delete/selection/IME marked-text) — this spike's version is ~250 lines, adapted from GPUI's own 778-line reference example (`examples/input.rs`). It's real native text editing (goes through the OS input method / IME composition path via `marked_range`), not a hand-rolled keymap — but it is not free. |
| Filtering 10k list stays responsive | **Works** | Typing in search re-filters 10,000 entries down to 530 ("kick") with no visible lag in manual testing. |
| Focus-search shortcut | **Works** | `cmd-f`/`ctrl-f` → `window.focus(&search_handle, cx)`. |
| Focus: Search → Results → Inspector | **Works, after a real footgun fix** | See §7.2 — `.tab_index()/.tab_stop()` on a `div()` is a silent no-op once you `.track_focus()` an externally-owned handle; the properties must be set on the `FocusHandle` itself. Cost real debugging time; verified working after the fix with screenshots. |
| Synthetic 512-bucket waveform, custom painting | **Works** | `canvas()` + `window.paint_quad`/`fill`. Background, waveform bars, playhead line, and a translucent selection-range overlay all painted directly — no renderer abstraction needed. |
| Mouse click → waveform time | **Works** | Bounds captured into a shared `Rc<Cell<Bounds<Pixels>>>` during the canvas prepaint pass, read back in the wrapping div's `on_mouse_down`. |
| Reuse existing playback engine (load/play/stop/audition) | **Works** | Wraps the real `punks-audio` `PlaybackEngine` unmodified. Verified: real WAV loaded, real audio played through the system output device, live position fed back into the UI via a `cx.spawn` timer loop (only running while something is actually playing — see §6), Stop and "audition another file" both confirmed working. |
| External OS drag-out — **hard gate** | **Works on macOS. Source-verified-only on Linux/Wayland. Missing on Windows and Linux/X11.** | See §4 — this is the headline finding. |
| Accessibility (role/name/focus/keyboard/state) | **API is real and source-verified; full independent AT verification not completed** | See §5. |

## 4. External drag findings (hard gate)

This was the single riskiest requirement, and where the spike spent the most effort.

**Public API used** (only exists on the unreleased `main` branch, not in crates.io 0.2.2):

```rust
div()
    .on_drag(path.clone(), |path, position, _, cx| cx.new(|_| DragGhost::new(path, position)))
    .external_drag_payload(|path: &PathBuf, _, _| {
        Some(ExternalDragPayload::Files(FileDragPaths::new([(path.clone(), false)])))
    })
```

`on_drag` starts GPUI's normal in-app drag (renders a ghost view that follows the cursor).
`external_drag_payload` is what's new: when the cursor leaves the window bounds, GPUI's window
layer calls `Window::promote_external_drag_to_platform`, which asks the platform backend
`can_start_external_drag()` / `start_external_drag(&payload)`. If the backend says yes, GPUI's
own ghost view is torn down and the OS takes over.

**macOS — confirmed working, end to end, with a real file.** `gpui_macos`'s `Window` implements
`NSDraggingSource`, and `start_external_drag` calls the real
`NSView::beginDraggingSessionWithItems:event:source:` with an `NSFilenamesPboardType` payload.
Verified manually in this session:
1. Wrapped the debug binary in a minimal `.app` bundle (see §7.3 — an unbundled binary can't be
   targeted for automation or get real window-manager/Dock identity; this is normal macOS
   packaging, not a GPUI issue).
2. Dragged the on-screen file row out of the spike window into a Finder window showing an empty
   target folder.
3. Confirmed both visually (Finder's icon view updated to show 1 item) and on disk:
   ```
   $ diff /var/folders/.../T/punks-gpui-spike/Sine_A_440Hz.wav /tmp/gpui-drag-drop-test/Sine_A_440Hz.wav
   (no output — byte-for-byte identical)
   ```

This is a real WAV file dragged out of a GPUI window into a real macOS app via the real system
drag-and-drop mechanism. Not a Finder-only trick, not a mock: the same NSPasteboard mechanism
any DAW's file well/browser would receive from.

**Linux/Wayland — source-verified, not run.** `gpui_linux/src/linux/wayland/window.rs` overrides
`can_start_external_drag`/`start_external_drag`, delegating to a real implementation in
`wayland/client.rs` that creates a `wl_data_source`, offers `text/uri-list`, sets
`DndAction::Copy | DndAction::Move`, and calls `wl_data_device::start_drag`. This is the correct,
standard Wayland DnD mechanism, fully wired up — not a stub. **Not run** (no Linux machine
available).

**Linux/X11 — missing.** `gpui_linux/src/linux/x11/window.rs` and `x11/client.rs` implement the
XDND protocol for **accepting** drops into the window (`XdndEnter`/`XdndPosition`/`XdndDrop`
handling is real and present) but never override `can_start_external_drag`/`start_external_drag`.
Confirmed by grep across both files — no match. Falls back to the platform trait's default,
which is a hardcoded `false`. **X11 cannot initiate an external drag today.** Per the spike's
ground rules, this would need a small private platform bridge implementing the source side of
XDND directly (the target side already exists as a model to follow) — not attempted here; no
X11 environment to build or test it against.

**Windows — missing.** `gpui_windows/src/window.rs` implements `IDropTarget`-style handling
(`DROPEFFECT`/`DROPEFFECT_COPY` etc. — confirmed present, for **accepting** drops) but has no
`can_start_external_drag`/`start_external_drag` override anywhere in the crate. Falls back to the
same default `false`. **Windows cannot initiate an external drag today.** This would need a small
private bridge around OLE drag-and-drop (`IDropSource` + `DoDragDrop`) — a well-trodden, bounded
Win32 API surface, but not attempted here; no Windows environment available.

**Bottom line on the hard gate:** the mechanism GPUI provides for this (`on_drag` +
`external_drag_payload`, promoted by the window layer to a platform call) is well-designed and
exactly the right shape — a real capability that belongs in GPUI, not a workaround. It is
**proven working on macOS** with a real file. It is **plausible and properly implemented on
Wayland** (unverified). It is **absent on X11 and Windows** and would need genuinely small,
scoped native glue on each (XDND source-side; OLE `IDropSource`) — consistent with "small private
OS glue is acceptable" in the spike's ground rules, but that glue does not exist yet and wasn't
built in this pass.

## 5. Accessibility findings

**What was tested vs. not**, stated plainly per the spike's own rule: do not claim support that
wasn't verified.

- **Source-verified:** GPUI has a real AccessKit integration (`accesskit`/`accesskit_consumer`/
  `accesskit_macos` are real dependencies of the pinned commit; **absent entirely** from the
  published 0.2.2 crate — see §7.1). There's a dedicated guide,
  `crates/gpui/src/_accessibility.rs`, documenting the model: elements get an accessibility node
  when they have both an `.id(...)` and a `.role(Role::...)`; `GlobalElementId`s become AccessKit
  node IDs; assistive-tech actions dispatch through `.on_a11y_action(AccessibleAction::X, ...)`;
  custom elements can expose synthetic children. This spike applies it to all four required
  elements:
  - **Search input:** `Role::TextInput`, `aria_label` reflecting current content, `tab_index(1)`.
  - **One result row (applied to all rows):** `Role::ListItem`, `aria_label` with name/duration/
    sample rate, `aria_position_in_set`/`aria_size_of_set`.
  - **Play button:** `Role::Button`, `aria_label`, reachable both by click and by a bound
    keyboard action (`space`) while the Inspector panel is focused. `.on_click()` is documented
    to auto-register an `AccessibleAction::Click` handler.
  - **Waveform:** `Role::Slider` (it behaves like a scrubber), `aria_label` reporting playhead
    position as a percentage, updated on click.
- **Runtime-verified, narrowly:** the running app logs real, live AccessKit diagnostics —
  `[gpui::window] Accessibility activated`, and, concretely,
  `[gpui::window::a11y] a11y: focused element (FocusId(2v1)) has no accessibility node (it has
  an id but no role); assistive technology will announce the whole window instead. Give it both
  an .id(...) and a .role(...) to expose it.` — fired for real, at runtime, when the initial
  root focus target (which deliberately has an `.id()` but no `.role()`, since it's not meant to
  be a real stop) was focused. This is GPUI proactively validating its own a11y completeness at
  runtime, which is a genuinely useful built-in DX signal, and it proves the AX layer is live and
  reactive in the running window — not just present in source.
- **Not verified:** an actual screen reader (VoiceOver) or a full AccessKit tree dump was **not**
  obtained in this session. Doing so needs Accessibility permission granted to whichever process
  drives the AX query (`osascript`/System Events, or a dedicated AX inspector) — that's a macOS
  security-settings change this spike deliberately did not make unilaterally. So: role, name,
  and the *intent* to expose focus/keyboard/state are all present and source-correct; independent
  confirmation that VoiceOver actually announces them correctly is an open item, not a claimed
  result.

## 6. Performance observations

- **10k-row list + filter:** typing a query that narrows 10,000 rows to 530 was visually instant
  in manual testing — no dropped-frame stutter, no perceptible input lag. This is exactly what
  `uniform_list` virtualization is for; nothing about it required a Punks-specific workaround.
- **Idle behavior:** the app renders only in response to `cx.notify()` (interaction, or the
  playback poll timer while something is actually playing). Measured directly from the app's own
  render-count log: over a clean, fully idle >60-second window (no mouse movement, no keypresses,
  nothing playing), **zero** additional render passes were logged. This is a real, measured
  confirmation of "no busy repaint loop while idle," not an assumption from reading docs.
- **Playback polling:** the moving playhead is driven by a `cx.spawn` loop that calls
  `cx.background_executor().timer(33ms).await` and exits the moment playback stops — so the only
  time the app polls on a timer is while audio is actually playing, matching the idle requirement.
- **What was *not* measured:** no frame-time histogram, no GPU profiler capture, no stress test
  beyond 10,000 items. The render-count-over-wall-clock log is a coarse proxy, not a rigorous
  benchmark; treat the responsiveness claims above as "felt right in manual testing," not
  "profiled and bounded."

## 7. GPUI API instability / dependency risks encountered

These were all found by actually building and running the spike, not by reading changelogs.

### 7.1 The published crate cannot do this spec at all
`gpui` 0.2.2 (crates.io, 2025-10-22) predates a since-in-progress crate split
(`gpui_platform`/`gpui_apple`/`gpui_macos`/`gpui_windows`/`gpui_linux`/`gpui_wgpu`/...). Checked
directly by downloading and extracting the published crate: **it has no `accesskit` dependency at
all**, and grepping its full source for `external_drag`/`ExternalDragPayload` returns nothing.
Both the accessibility system and the drag-out hard gate — half of this spec — **do not exist in
the only version of GPUI you can `cargo add` today.** Everything in this report required pinning
an unreleased git commit on `zed-industries/zed`'s `main` branch. That branch is, by GPUI's own
README, "still in active development... pre-1.0. There will often be breaking changes between
versions," and the Zed team has stated they don't have resources to support standalone use as a
stable dependency. This is the single largest risk in this report: real GO evidence, on top of a
target that could rename, restructure, or break at any commit, with no semver contract.

### 7.2 Silent tab-order footgun
Calling `.tab_index(N)` / `.tab_stop(true)` on a `div()` only takes effect when GPUI auto-creates
the focus handle for you (i.e., when you *don't* call `.track_focus()`). The moment you supply
your own externally-owned `FocusHandle` via `.track_focus(&handle)`, those div-level calls become
silent no-ops — confirmed by reading `elements/div.rs`: the auto-enrichment branch is gated on
`self.tracked_focus_handle.is_none()`. The tab-index/tab-stop values that actually matter live on
the `FocusHandle` struct itself (`impl FocusHandle { pub fn tab_index(...) }`), and must be set
there directly (`cx.focus_handle().tab_index(2).tab_stop(true)`). Getting this wrong produces
**no compiler error, no runtime warning, no visual difference** — the UI looks identical, `Tab`
still dispatches the action, `window.focus_next()` still runs — it just silently never lands on
that element. This cost real debugging time in this spike (confirmed by grep of gpui's own
`tab_stop.rs`/`window.rs`/`elements/div.rs` after `Tab` fired the action but moved no visible
focus) and would be very easy to ship broken in a real product without noticing.

### 7.3 `gpui_platform`'s cross-platform default is not actually cross-platform
The README's own recommended snippet — `gpui_platform = { version = "*" }` — is misleading:
`gpui_platform`'s *own* default features are `[]` (empty). `font-kit`/`wayland`/`x11` are not on
unless explicitly requested, contrary to what the README's phrasing implies. The failure mode is
silent and confusing: the app builds fine, opens a window fine, lays out the whole UI fine — it
just renders **zero glyphs**, everywhere (confirmed: search placeholder, all 10,000 row labels,
button text, all disappeared; layout boxes and colors were otherwise correct). No warning, no
error. Fixed by explicitly requesting `features = ["font-kit", "wayland", "x11"]` as the README's
prose (not its code sample) actually describes.

### 7.4 Dependency-graph friction
Pinning `gpui`/`gpui_platform` to a zed-industries/zed git rev, in the *same* Cargo workspace as
the existing Punks code, produced a real version conflict: `gpui_macos` requires `cocoa =0.26.0`
exactly, which conflicted with `punks-standalone`'s existing `drag` crate (pinned to
`cocoa ^0.26.0`, locked to 0.26.1) once both shared one resolved dependency graph. Worked around
by giving the spike its own, fully independent Cargo workspace (`[workspace]` in
`spikes/gpui-viability/Cargo.toml`) — appropriate for a spike, but a real production integration
would need to actually resolve this (most likely: dropping the old `drag` crate, since GPUI's own
drag mechanism would replace what it's used for).

### 7.5 macOS build needs the Metal Toolchain, not just Xcode CLT
First `cargo build` failed with `metal shader compilation failed: cannot execute tool 'metal' due
to missing Metal Toolchain; use: xcodebuild -downloadComponent MetalToolchain`. Xcode Command Line
Tools alone (what most CI images and fresh dev machines have) is **not** sufficient; a ~688MB
additional component download is required. This machine had it fetchable via
`xcodebuild -downloadComponent MetalToolchain` without further prompts, but that's a real,
non-obvious first-build tax that should be budgeted for in CI setup and onboarding docs if this
path is pursued.

### 7.6 Crate rename mid-session
Unrelated to GPUI itself, but worth recording: partway through this spike, `punks-playback` was
renamed/merged (with `punks-analysis`) into `punks-audio` by other work on this branch. Same
package version (0.2.0) and identical `PlaybackEngine`/`WaveformPeaks`/`PlaybackStatus` API, so
the spike's dependency was a one-line path update, not a rewrite — but it's a reminder that the
lower layers this spike leans on are themselves mid-refactor right now, independent of the GPUI
question.

### 7.7 No batteries-included text input
Already covered in §3, repeated here because it's a real cost, not a blocker: there is no stable,
reusable `TextInput` component in GPUI. Every app (including Zed itself, and this spike) hand-rolls
`EntityInputHandler`. This is a legitimate, well-designed low-level contract — not a placeholder —
but it's ~250-800 lines of code every consumer duplicates today.

## 8. Recommendation

### GO WITH EXPLICIT RISKS

**Why GO:** every non-negotiable capability in the spec was either proven working with real
evidence (window lifecycle, event-driven idle behavior, 10k-row virtualization with keyboard nav
and scroll-follow, custom-painted waveform with real mouse interaction, real playback-engine
reuse, and — the hard gate — actual OS-level file drag-out into Finder with a byte-identical file)
or is a small, bounded, previously-anticipated gap with a known shape (Windows/X11 drag-out via
OLE / XDND source-side glue — exactly the kind of "small private OS glue" the spike's own ground
rules called acceptable). Nothing GPUI does forced a workaround that felt like fighting the
framework; the low-level `Element`/`canvas`/`uniform_list` primitives are a genuinely good match
for a keyboard-first sample browser, and the accessibility model (AccessKit, roles, actions) is a
real, thought-out system, not an afterthought.

**Why "with explicit risks," not a plain GO:**
1. **The capability set this report validates does not exist in any published, versioned release
   of GPUI.** Everything here depends on an unreleased, actively-restructuring `main` branch with
   no semver contract and an explicit statement from the maintaining team that standalone use
   isn't a resourced priority (§7.1). Committing to GPUI today means committing to tracking that
   branch, budgeting time to absorb breaking changes, and accepting that a future upstream commit
   could remove or reshape the exact APIs this spike relied on.
2. **Windows and Linux/X11 external drag-out do not exist yet** and were not built or tested in
   this pass — they're real, scoped work items, not proven capabilities (§4).
3. **Two silent-failure footguns were hit in a few days of spike work** (tab-order no-op, missing
   glyph rendering) that produced no compiler or runtime error — just a broken-looking app. A
   production port should expect more of these and budget time for them.
4. **Windows and Linux were never actually run.** The source-level analysis here is thorough and
   specific (exact files, exact missing overrides), but "the code path exists and looks right" is
   not the same bar as "it built and a human clicked it," which is what macOS got in this report.

**If proceeding:** pin the git rev deliberately (already done here), track upstream for the
`gpui_platform` split's eventual crates.io release, prototype the Windows OLE and X11 XDND
drag-out bridges early (they're the biggest unknowns, not the biggest expected effort), and
budget real time for the font-kit-style silent-failure class of issue before treating any GPUI
integration as done.
