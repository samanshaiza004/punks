# Building Punks

## GPUI toolchain

The GPUI frontend is intentionally pinned to the exact Zed revision
`8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`. The `gpui-component` fork is pinned to
`4cf3cddc0609a220aed0323678fe5241fdbfd7d6` and pins its GPUI dependencies to the same Zed
revision. Do not update either revision as part of an unrelated change.

`gpui_platform` must keep the explicit `font-kit`, `wayland`, and `x11` features. Its defaults
are empty; removing these features produces a build that can open a window but renders no text.

## macOS

Use a current Xcode installation with the Metal Toolchain available. If Xcode reports a missing
Metal Toolchain, install it with:

```text
xcodebuild -downloadComponent MetalToolchain
```

## Platform verification

The macOS GPUI path and external drag-out were runtime-verified in the viability spike. The
Wayland, Windows OLE, and X11 XDND paths require runtime checks on their respective systems with
a real file drop target; source compilation and runtime behavior are not interchangeable claims.

The M6 private bridges are intentionally narrow: they carry file lists only and do not add a
cross-platform drag configuration layer. Keep the old platform-specific behavior intact when
changing them, and record any new OS evidence in `docs/gpui-viability.md`.

For a Linux/Wayland verification session, use a real compositor session and run the normal
standalone binary from that session:

```text
cargo build --release -p punks-standalone
RUST_LOG=info ./target/release/punks-standalone
```

Record the compositor, `WAYLAND_DISPLAY`, build target, font rendering, keyboard behavior,
audio playback, and a file drag into a real Wayland drop target. A source build without a live
Wayland session is not runtime verification.

CI also runs the dependency-only `gpui_component_smoke` example under a headless Weston
compositor in `.github/workflows/wayland-smoke.yml`. That proves GPUI can initialize and render
through the Wayland backend without audio hardware; it does not replace a manual external-drag
test against a real Wayland drop target.
