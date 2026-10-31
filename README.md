# Punks

Punks is a local audio browser built around its Rust audio and library code. The
standalone desktop frontend is written in Odin with Alicorn; Caliber connects
the frontend to the headless Rust application.

The current interface opens a folder, navigates audio files, searches names,
selects files, and previews or stops playback. It reads WAV (including RF64 and
Broadcast Wave), FLAC, MP3, and OGG through the existing Rust audio engine.
The workbench has folder navigation, per-tab browsing and search, a virtualized
file table, and a persistent waveform and listening deck.
Long-file previews remain bounded. Click or drag the signed waveform to seek;
Tab focuses it for arrow-key and Home/End seeking. Space pauses or resumes,
and Stop resets the position. Drop a WAV, FLAC, MP3, or OGG file from your file
manager onto the window to audition it without changing the active browsing
folder. Drag an audio result out to another application to copy its file there
(currently supported by the Windows native host). Library tagging controls,
settings, and waveform zoom/pan remain future work.

## Build

Initialize the pinned frontend and ABI dependencies:

```sh
git submodule update --init --recursive
```

Install a Rust toolchain, Odin, and SDL3. The native build was validated with
Odin `dev-2026-09-nightly:a2fb372` and SDL3 from Homebrew on macOS. On macOS:

```sh
brew install sdl3
./tools/build.sh
```

Set `ODIN` to the Odin executable if it is not on `PATH`. Set `SDL3_PREFIX` if
SDL3 is installed outside Homebrew's default location. The build creates
`target/debug/punks` and packages a macOS app at
`target/debug/punks-package/Punks.app`. Add `--release` to build optimized
artifacts.

On Windows x64, install a complete Odin distribution, Rust MSVC toolchain, and
Visual Studio Build Tools with the x64 C++ workload and Windows SDK. Build with:

    .\tools\build.ps1

Add `-Release` to build optimized artifacts. The helper builds the Caliber DLL and
Odin frontend, then stages a runnable folder at
`target/windows-x86_64/debug` (or `release`). Keep `punks.exe`, `SDL3.dll`, and
`punks_caliber.dll` together when running the application. Pass `-Odin` if the
compiler is not available on PATH.

On macOS, install SDL3 3.4.18 and use `./tools/build.sh`. On Linux, install SDL3
development files and the system audio dependencies required by CPAL, then use
tools/build.sh. Linux remains unvalidated in this development environment.
## Run

Open the macOS app bundle, or launch the executable directly:

```sh
open target/debug/punks-package/Punks.app
target/debug/punks --folder /path/to/samples
```

`--no-audio` starts the browser without opening an audio device. `--folder`
opens a folder at launch; the Open Folder button uses the native folder picker.
Use the search field for filename search, Up/Down to change selection, Return
to enter a selected folder, and Space to toggle preview playback. Search is
debounced while typing. Escape clears it.

## Validate

On Windows, `./tools/build.ps1 -Release -TestUi` also runs the headless frontend
regressions for viewport filling, compact and expanded layouts, resizing, and
single-label result rows with accessible names.

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./tools/build.sh
```

## Architecture

```text
Alicorn/Odin frontend
        │ versioned JSON commands, immutable resources, wakeups
        ▼
Punks-Caliber adapter
        ▼
Punks Core ────────┐
   │               │
Library          Audio
```

`punks-core` owns browsing, search, selection, playback control, configuration,
and worker lifecycle without depending on a GUI toolkit. `punks-library`
owns SQLite identities, metadata, tags, and analysis data. `punks-audio` owns
decoding, playback, resampling, waveform generation, and analysis. The Odin
frontend owns layout, focus, scrolling, and presentation state. The protocol,
resource lifetime, and shutdown rules are described in
[`docs/caliber-protocol.md`](docs/caliber-protocol.md). The wider design and
database repair are documented in [`docs/architecture.md`](docs/architecture.md)
and [`docs/database-migrations.md`](docs/database-migrations.md).

## License

[MIT](LICENSE)
