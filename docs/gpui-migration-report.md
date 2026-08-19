# GPUI migration report

## Current architecture

`punks-app` owns application orchestration and the GPUI window. `punks-audio` owns CPAL,
decoding, prepared buffers, and playback state. `punks-library` owns SQLite and persistent
library data. Commands and undo/redo remain in `punks-app`; background decode, analysis, and
waveform work remain off the UI and callback paths. `punks-standalone` only initializes logging
and calls `punks_app::run()`.

## Components

| Control | Status | Evidence / decision |
|---|---|---|
| Button | Used | Play/Stop, Settings, Apply, and Close; M4 app-level tests pass. |
| Input | Used | Search, inspector description, and eight settings fields; M5 integration evidence plus settings tests. |
| Slider | Used | Volume control; event and state tests pass. |
| Checkbox / Popover | Isolated audit only | No current production consumer. |
| Select / Tooltip / Dialog | Deferred | Audit on the first real consumer; none is advertised by M6. |

## Platform integration

| Platform | Implementation | Verification |
|---|---|---|
| macOS | GPUI `on_drag` + `external_drag_payload` | TESTED in the viability spike |
| Wayland | GPUI upstream source path | SOURCE-VERIFIED ONLY; runtime environment unavailable here |
| Windows | Private OLE `IDataObject` / `IDropSource` / `DoDragDrop` bridge | NOT TESTED; no Windows target/runtime here |
| X11 | Private bounded XDND source bridge | NOT TESTED; no Linux/X11 runtime here |
| Clipboard | GPUI/component/native input behavior | NOT independently runtime-tested in this M6 pass |
| Dialogs | `rfd` native dialogs | Existing path retained; not re-audited in M6 |

The four tab keybind fields remain in the persisted config for compatibility, but are not shown
in Settings and are not registered until visible tab UI returns. Box-select auto-scroll remains
deferred as planned.

## Deletions and additions

The old Windows `drag` dependency and stopgap were removed. The executable no longer owns the
old renderer/event loop. Added modules are `browser/settings.rs`, `platform/windows_drag.rs`,
and `platform/x11_drag.rs`; the GPUI frontend uses direct GPUI for specialized browser,
waveform, selection, and inspector surfaces.

## Verification

Passed on the available macOS host:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo build --release -p punks-standalone
cargo tree -d
cargo tree -i gpui
cargo tree -i gpui_platform
```

The host has no Wayland compositor, Linux target, Windows target, or audio output device, so
those runtime gates remain explicitly NOT TESTED rather than being inferred from source.
The repository now has a headless Weston launch gate in
`.github/workflows/wayland-smoke.yml`; a passing CI run will establish runtime GPUI/Wayland
initialization, but manual external-drag verification still needs a real Wayland drop target.
