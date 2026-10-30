# Waveform timeline

The listening deck uses signed min/max peaks rather than symmetric magnitude
bars. Prepared playback windows provide a 512-pair overview. The background
full-source analyzer retains 65,536 pairs in its versioned cache, giving the
waveform more detail as the visible time range narrows. Older 512-pair cache
entries are recomputed. A bounded GPU geometry surface draws up to 512 visible
envelope columns (roughly one per 1.5 logical units), aggregating only the
source buckets inside the current view. Positive-only samples stay above the
center; negative excursions stay below it. The combined channel envelope
preserves opposite-polarity stereo rather than averaging it away.

Zoom is bounded from 1x to 128x. The `-`, `Fit`, and `+` controls sit in the
waveform's reserved control band. Ctrl+wheel zooms around the pointer; wheel or
Shift+wheel pans horizontally. Click and drag seek within the visible interval,
and the footer's start/end labels track that interval. Zoom and pan rebuild only
the static visible peak geometry; moving the playhead continues to update its
small independent surface. Caliber publishes a changed waveform resource only
when source identity, axis, or peak resolution changes, not on ordinary
position or volume publications.

A second GPU surface draws progress and the playhead. Peak geometry changes only
when the waveform resource, allocated size, or theme changes. Ordinary playback
state publications still request an application description at the existing
backend cadence; this stage does not claim to eliminate those builds. Repeated
playhead updates do not replace the peak payload or upload another peak mesh.

An ordinary Alicorn slider supplies the timeline's pointer capture, keyboard
focus, and accessible numeric range beneath the custom presentation. There is
no separate visible Position slider. Click or drag anywhere on the waveform;
arrows adjust its position and Home/End reach the visible interval's bounds.
Space pauses or resumes; double-clicking the waveform toggles playback from the
clicked position. Seeking a paused or stopped recording leaves it paused. Stop
continues to reset the source position.

The visible interval comes from the Rust waveform axis, including its nonzero
origin when only a prepared source window is available. Scrubbing submits at
most 25 intermediate seeks per second, then always submits the final release.
A pending target stays visible while an out-of-window decode is loading.
Request IDs and the immutable resource handle prevent stale UI seeks from
applying after a different recording is adopted. Returning to the prepared
window cancels a superseded pending decode.

Caliber publishes peaks in a separate immutable waveform resource with a
version, analysis policy, file identity, source interval, and resource generation.
Position and volume updates keep the same handle. The frontend maps and copies
a changed resource into one of two bounded-lifetime arenas, then releases the
lease. Current resource owners are retired at shutdown; mapped reader leases
remain valid until released.

## Stable listening-deck slots

Playback state changes the contents of reserved bands. Desktop bands use a
100-unit waveform; the compact profile uses 72. Track information, the time
footer, transport, and volume reserve explicit heights derived from the active
text scale and theme control metrics. The deck's height is their sum plus
padding and gaps. Window/profile, text-scale, and theme-metric changes can
reallocate the deck; selecting, loading, seeking, playing, pausing, and errors
do not. The results viewport receives the remaining height.

The numeric time display uses monospace, nonwrapping text in its own grid cell.
A separate status cell always reserves 72 logical units times text scale, even
when empty. The compact footer omits the separate start/end labels and keeps
one current/total display. Long filenames and metadata use ellipsis with full
values available on hover. Loading a different source keeps the waveform
footprint and replaces old peaks with a placeholder. A pending seek in the
same source can continue displaying its cached peaks.

The old content-growth regression has been replaced by a state-transition
regression covering desktop and 350-unit component layouts, 140% text scale,
long filenames/durations/errors, stable region and control identities, retained
keyboard focus, single-line text, and child containment. This compact fixture
validates the deck component; it does not introduce a complete floating dock.

## Validation

- Seven Odin UI tests pass, including stable state-transition and narrow
  140%-text layouts, aligned paint and hit bounds, signed envelope geometry at 150% surface scale, independent
  playhead payloads, accessible range semantics, source-window mapping,
  coalesced drag capture, clamped release, and keyboard Home seeking.
- The unchanged Rust engine retains the validation from `8d0c791`: all 28
  Caliber/core/library tests pass. Resource stability, stale seek
  rejection, shutdown lease lifetime, and old peak-cache rejection are covered.
- 58 audio unit tests and two format-decode tests pass, including mono,
  opposite-polarity stereo through both extractors, short sources, paused seeks,
  and pending-window replacement. Three pre-existing WAV metadata-writing tests
  still fail with Windows temporary-file `Access is denied`; they were recorded
  and excluded from the subsequent targeted audio run.
- A native Windows Direct3D 12 audio smoke run passes on a generated six-second
  stereo recording, returns to idle, and exits cleanly. SDL reports 3.4.2 in
  this local Odin distribution. This is automated native evidence, not a manual
  interaction or screen-reader pass.

The Windows candidate is built with:

```powershell
.\tools\build.ps1 -Release -TestUi -OutputDirectory target/windows-x86_64/stable-deck-release
```

For native acceptance, try paused and playing seeks on short one-shots and long
recordings, drag beyond either edge, scrub rapidly then select another file,
and use Tab/arrows/Home/End/Space on the waveform. Resize at fractional DPI and
confirm the painted waveform still matches its hit region. The coarse/fine peak
levels remain bounded and do not yet request arbitrary source subranges.
Region selection remains a later stage.
