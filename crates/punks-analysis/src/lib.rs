//! Time-domain audio feature analysis: RMS, peak, zero-crossing rate, duration.
//!
//! Pure DSP with no dependencies — no file I/O, no SQLite, no playback. Callers
//! decode audio elsewhere and hand a borrowed [`AnalysisContext`] (the audio plus
//! cheaply-known source facts) to a static `Analyzer::analyze`. Each analyzer type
//! *is* its own output.
//!
//! This crate owns the *algorithms*. The orchestrator runs [`run_all`] on a
//! buffer to get one [`AnalysisReport`], then hands its flattened
//! [`AnalysisReport::metrics`] to storage — storage never names an analyzer.
//! [`pipeline_version`] folds every analyzer's `VERSION` so persisted results
//! invalidate when any algorithm changes.
//!
//! Deliberately tiny: no FFT/STFT/windowing/chroma/onset/tempo/key. Shared
//! spectral infra should wait for a second real need.

/// A borrowed view of interleaved f32 PCM plus its rate and channel layout.
/// The minimal wrapper analyzers operate on.
pub struct AudioBuffer<'a> {
    /// Interleaved samples: `[ch0, ch1, …, ch0, ch1, …]`.
    pub samples: &'a [f32],
    pub sample_rate: u32,
    pub channels: u16,
}

impl<'a> AudioBuffer<'a> {
    pub fn new(samples: &'a [f32], sample_rate: u32, channels: u16) -> Self {
        AudioBuffer {
            samples,
            sample_rate,
            channels,
        }
    }

    /// Number of frames (samples per channel).
    pub fn frames(&self) -> usize {
        self.samples.len() / (self.channels.max(1) as usize)
    }
}

/// Everything an analyzer reads. Two roles:
///
/// 1. **Immutable inputs** — the decoded `audio` plus cheaply-known facts about
///    the source (`source_duration`) that analyzers must not recompute.
/// 2. **Shared intermediate representations** (future) — lazily-computed DSP
///    artifacts cached here so dependent analyzers reuse one computation instead
///    of each recomputing it. When Tempo needs an STFT and Key needs the same
///    spectrogram, the context computes it once (Tempo → needs STFT ┐, Key →
///    reuses ┘). Added when the first spectral analyzer lands, e.g.
///    `spectrum: OnceCell<Spectrum>`, `spectrogram: OnceCell<Spectrogram>`,
///    `chromagram: OnceCell<Chromagram>`. Not implemented yet — this is the shape
///    the future MIR library grows into, not a bag of arbitrary metadata.
pub struct AnalysisContext<'a> {
    pub audio: AudioBuffer<'a>,
    /// True source length, even when `audio` is a bounded preview window of a
    /// longer file. From the decoder — not derivable from `audio`.
    pub source_duration: std::time::Duration,
}

/// A feature analyzer. `analyze` is static — there's no analyzer state to
/// configure — and returns the analyzer's own result type.
///
/// - `ID` is the stable string key its metrics are stored under.
/// - `VERSION` bumps whenever the algorithm changes; folded into
///   [`pipeline_version`] so persisted results invalidate.
/// - `DEPENDS_ON` lists the `ID`s of analyzers whose output this one needs. It's
///   empty for every current analyzer; the field exists so a future scheduler
///   can topologically order the registry (e.g. key detection → chromagram)
///   without touching the worker, which only ever calls [`run_all`].
///
/// Infallible for now — RMS/Peak/ZCR/Duration are mathematically total. When the
/// first fallible analyzer arrives (Tempo, whose requested spectrum may be
/// absent), give the trait `type Error = Infallible` so only it carries a real
/// error type, rather than making every analyzer return `Result`.
pub trait Analyzer {
    const ID: &'static str;
    const VERSION: u32;
    const DEPENDS_ON: &'static [&'static str] = &[];
    type Output;
    fn analyze(ctx: &AnalysisContext) -> Self::Output;
}

/// Linear amplitude below which dBFS is floored to stay finite (≈ -180 dBFS),
/// so silence stores a real number rather than `-inf`.
const DBFS_FLOOR_AMP: f32 = 1e-9;

/// Linear amplitude → dBFS (`20·log10`), floored finite so silence is a real
/// number, not `-inf`. Callers store linear and display in dB with this.
pub fn amp_to_dbfs(amp: f32) -> f32 {
    20.0 * amp.max(DBFS_FLOOR_AMP).log10()
}

/// Root-mean-square level over the whole buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rms {
    /// Linear RMS amplitude.
    pub value: f32,
    /// The same level in dBFS (`20·log10(value)`), floored finite.
    pub dbfs: f32,
}

impl Analyzer for Rms {
    const ID: &'static str = "rms";
    const VERSION: u32 = 1;
    type Output = Rms;

    fn analyze(ctx: &AnalysisContext) -> Rms {
        let samples = ctx.audio.samples;
        let n = samples.len();
        if n == 0 {
            return Rms {
                value: 0.0,
                dbfs: amp_to_dbfs(0.0),
            };
        }
        // Accumulate in f64 so long buffers don't lose precision.
        let sum_sq: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
        let value = (sum_sq / n as f64).sqrt() as f32;
        Rms {
            value,
            dbfs: amp_to_dbfs(value),
        }
    }
}

/// Sample peak: the largest absolute sample value. Not an oversampled
/// inter-sample true peak.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peak {
    pub value: f32,
}

impl Analyzer for Peak {
    const ID: &'static str = "peak";
    const VERSION: u32 = 1;
    type Output = Peak;

    fn analyze(ctx: &AnalysisContext) -> Peak {
        let value = ctx
            .audio
            .samples
            .iter()
            .fold(0.0_f32, |m, &x| m.max(x.abs()));
        Peak { value }
    }
}

/// Zero-crossing rate: the fraction of adjacent same-channel sample pairs whose
/// sign differs, in `[0, 1]`. Computed per channel (not across the interleaved
/// stream, which would count spurious crossings between channels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zcr {
    pub value: f32,
}

impl Analyzer for Zcr {
    const ID: &'static str = "zcr";
    const VERSION: u32 = 1;
    type Output = Zcr;

    fn analyze(ctx: &AnalysisContext) -> Zcr {
        let samples = ctx.audio.samples;
        let ch = ctx.audio.channels.max(1) as usize;
        let frames = samples.len() / ch;
        if frames < 2 {
            return Zcr { value: 0.0 };
        }
        let mut crossings: u64 = 0;
        for c in 0..ch {
            let mut prev_neg = samples[c] < 0.0;
            for f in 1..frames {
                let neg = samples[f * ch + c] < 0.0;
                if neg != prev_neg {
                    crossings += 1;
                }
                prev_neg = neg;
            }
        }
        let pairs = (frames - 1) as u64 * ch as u64;
        Zcr {
            value: crossings as f32 / pairs as f32,
        }
    }
}

/// Source length. Reads `source_duration` from the context — the *true* length,
/// so a long file previewed as a bounded window still reports its full duration.
/// Holds the strong `std::time::Duration` type; only flattened to seconds at the
/// storage boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Duration {
    pub value: std::time::Duration,
}

impl Analyzer for Duration {
    const ID: &'static str = "duration";
    const VERSION: u32 = 1;
    type Output = Duration;

    fn analyze(ctx: &AnalysisContext) -> Duration {
        Duration {
            value: ctx.source_duration,
        }
    }
}

/// The registered analyzers as `(id, version)`, in run order. The single place
/// that knows the analyzer set; [`run_all`] and [`pipeline_version`] read it.
/// When an analyzer gains a non-empty `DEPENDS_ON`, sort this topologically.
const PIPELINE: &[(&str, u32)] = &[
    (Rms::ID, Rms::VERSION),
    (Peak::ID, Peak::VERSION),
    (Zcr::ID, Zcr::VERSION),
    (Duration::ID, Duration::VERSION),
];

/// One asset's analysis result. A typed carrier so analyzers stay type-safe,
/// with [`metrics`](Self::metrics)/[`from_metrics`](Self::from_metrics) as the
/// only seam to storage — the DB schema can evolve without the analyzers, and
/// storage never names a field. Grows by adding fields (e.g. `bpm: Option<f32>`,
/// `key: Option<Key>`) plus a line in each of the two methods; nothing else moves.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnalysisReport {
    /// Linear RMS amplitude.
    pub rms: f32,
    /// RMS in dBFS (floored finite).
    pub rms_dbfs: f32,
    /// Sample peak (max |x|), linear. Display in dB via [`amp_to_dbfs`].
    pub peak: f32,
    /// Zero-crossing rate in `[0, 1]`.
    pub zcr: f32,
    /// True source length (the strong type; flattened to seconds only in storage).
    pub duration: std::time::Duration,
}

impl AnalysisReport {
    /// Flatten to `(metric, value)` pairs for opaque storage.
    pub fn metrics(&self) -> Vec<(&'static str, f64)> {
        vec![
            ("rms", self.rms as f64),
            ("rms_dbfs", self.rms_dbfs as f64),
            ("peak", self.peak as f64),
            ("zcr", self.zcr as f64),
            ("duration", self.duration.as_secs_f64()),
        ]
    }

    /// Rebuild from stored pairs; a metric absent from `rows` stays at its
    /// default (0 now; `None` once optional fields exist).
    pub fn from_metrics(rows: &[(String, f64)]) -> Self {
        let raw = |k: &str| rows.iter().find(|(m, _)| m == k).map(|(_, v)| *v);
        let get = |k: &str| raw(k).map(|v| v as f32).unwrap_or_default();
        AnalysisReport {
            rms: get("rms"),
            rms_dbfs: get("rms_dbfs"),
            peak: get("peak"),
            zcr: get("zcr"),
            // Guard against negative/NaN so `from_secs_f64` can't panic.
            duration: std::time::Duration::from_secs_f64(
                raw("duration")
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or(0.0),
            ),
        }
    }
}

/// Run every analyzer over `ctx` and assemble one [`AnalysisReport`]. Because the
/// whole report is produced in a single call, a caller can never persist a
/// partially-analyzed asset. Gains topological ordering + an intermediate-artifact
/// map when the first analyzer declares a dependency; callers are unaffected.
pub fn run_all(ctx: &AnalysisContext) -> AnalysisReport {
    let rms = Rms::analyze(ctx);
    let peak = Peak::analyze(ctx);
    let zcr = Zcr::analyze(ctx);
    let duration = Duration::analyze(ctx);
    AnalysisReport {
        rms: rms.value,
        rms_dbfs: rms.dbfs,
        peak: peak.value,
        zcr: zcr.value,
        duration: duration.value,
    }
}

/// A single version for the whole analyzer set: any analyzer added, removed, or
/// version-bumped changes this, so storage can requeue stale jobs by comparing a
/// plain `u32`. (FNV-ish fold over each `(id, version)`.)
pub fn pipeline_version() -> u32 {
    let mut v: u32 = 0;
    for (id, ver) in PIPELINE {
        for b in id.bytes() {
            v = v.wrapping_mul(31).wrapping_add(b as u32);
        }
        v = v.wrapping_mul(31).wrapping_add(*ver);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration as StdDuration;

    fn sine(freq: f32, sample_rate: u32, amp: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|n| amp * (std::f32::consts::TAU * freq * n as f32 / sample_rate as f32).sin())
            .collect()
    }

    /// Context whose `source_duration` matches the buffer (the whole-file case).
    fn ctx<'a>(samples: &'a [f32], sr: u32, ch: u16) -> AnalysisContext<'a> {
        let frames = samples.len() / ch.max(1) as usize;
        AnalysisContext {
            audio: AudioBuffer::new(samples, sr, ch),
            source_duration: StdDuration::from_secs_f64(frames as f64 / sr as f64),
        }
    }

    #[test]
    fn silence_is_zero_everywhere() {
        let s = vec![0.0f32; 1000];
        let c = ctx(&s, 48_000, 1);
        assert_eq!(Rms::analyze(&c).value, 0.0);
        assert_eq!(Peak::analyze(&c).value, 0.0);
        assert_eq!(Zcr::analyze(&c).value, 0.0);
        // Silence floors to a finite, very low dBFS (not -inf).
        assert!(Rms::analyze(&c).dbfs < -150.0 && Rms::analyze(&c).dbfs.is_finite());
    }

    #[test]
    fn constant_half_is_the_known_rms_fixture() {
        // A DC signal of 0.5 has RMS exactly 0.5 and peak 0.5; no sign changes.
        let s = vec![0.5f32; 512];
        let c = ctx(&s, 48_000, 1);
        assert!((Rms::analyze(&c).value - 0.5).abs() < 1e-6);
        assert!((Rms::analyze(&c).dbfs - (20.0 * 0.5_f32.log10())).abs() < 1e-4);
        assert_eq!(Peak::analyze(&c).value, 0.5);
        assert_eq!(Zcr::analyze(&c).value, 0.0);
    }

    #[test]
    fn sine_has_expected_rms_peak_and_zcr() {
        let sr = 48_000;
        let freq = 1000.0;
        let amp = 0.8;
        let s = sine(freq, sr, amp, sr as usize); // 1 second
        let c = ctx(&s, sr, 1);

        // RMS of a sine is amp / sqrt(2).
        let expected_rms = amp / std::f32::consts::SQRT_2;
        assert!((Rms::analyze(&c).value - expected_rms).abs() < 1e-3);
        // Peak ≈ amp.
        assert!((Peak::analyze(&c).value - amp).abs() < 1e-3);
        // ZCR ≈ 2·freq/sample_rate (two crossings per cycle).
        let expected_zcr = 2.0 * freq / sr as f32;
        assert!((Zcr::analyze(&c).value - expected_zcr).abs() < 1e-3);
    }

    #[test]
    fn alternating_sign_is_full_zcr() {
        let s: Vec<f32> = (0..256)
            .map(|n| if n % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let c = ctx(&s, 48_000, 1);
        assert!((Zcr::analyze(&c).value - 1.0).abs() < 1e-6);
        assert_eq!(Peak::analyze(&c).value, 1.0);
        assert!((Rms::analyze(&c).value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zcr_is_per_channel_not_across_interleave() {
        // Two channels, each a constant with no sign change within the channel:
        // L = +1, R = -1 interleaved. A naive flat walk would see a crossing on
        // every step; the per-channel computation must see none.
        let s = vec![1.0f32, -1.0, 1.0, -1.0, 1.0, -1.0];
        let c = ctx(&s, 48_000, 2);
        assert_eq!(Zcr::analyze(&c).value, 0.0);
    }

    #[test]
    fn duration_is_source_not_buffer() {
        // The analyzer must report the *source* length even when the buffer is a
        // short preview window of a much longer file (the truncated-decode case).
        let s = vec![0.0f32; 4]; // tiny buffer
        let long = StdDuration::from_secs(3600);
        let c = AnalysisContext {
            audio: AudioBuffer::new(&s, 48_000, 1),
            source_duration: long,
        };
        assert_eq!(Duration::analyze(&c).value, long);
        assert_eq!(run_all(&c).duration, long);
    }

    #[test]
    fn amp_to_dbfs_floors_silence_finite() {
        assert!(amp_to_dbfs(0.0) < -150.0 && amp_to_dbfs(0.0).is_finite());
        assert!((amp_to_dbfs(1.0) - 0.0).abs() < 1e-4); // full scale = 0 dBFS
        assert!((amp_to_dbfs(0.5) - (20.0 * 0.5_f32.log10())).abs() < 1e-4);
    }

    #[test]
    fn version_is_readable() {
        assert_eq!(Rms::VERSION, 1);
        assert_eq!(Peak::VERSION, 1);
        assert_eq!(Zcr::VERSION, 1);
        assert_eq!(Duration::VERSION, 1);
    }

    #[test]
    fn run_all_fills_the_whole_report() {
        let s = sine(1000.0, 48_000, 0.8, 48_000);
        let c = ctx(&s, 48_000, 1);
        let r = run_all(&c);
        assert!((r.rms - 0.8 / std::f32::consts::SQRT_2).abs() < 1e-3);
        assert!((r.peak - 0.8).abs() < 1e-3);
        assert!((r.zcr - 2.0 * 1000.0 / 48_000.0).abs() < 1e-3);
        assert!(r.rms_dbfs < 0.0 && r.rms_dbfs.is_finite());
        assert!((r.duration.as_secs_f64() - 1.0).abs() < 1e-6); // 1s buffer
    }

    #[test]
    fn metrics_round_trip() {
        let r = AnalysisReport {
            rms: 0.5,
            rms_dbfs: -6.02,
            peak: 0.9,
            zcr: 0.1,
            duration: StdDuration::from_secs_f64(3.482),
        };
        // metrics() -> owned pairs -> from_metrics() must reconstruct exactly.
        let owned: Vec<(String, f64)> = r
            .metrics()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let back = AnalysisReport::from_metrics(&owned);
        assert_eq!(r, back);
        // Missing metrics default rather than panic.
        let partial = AnalysisReport::from_metrics(&[("peak".to_string(), 0.7)]);
        assert_eq!(partial.peak, 0.7);
        assert_eq!(partial.rms, 0.0);
        assert_eq!(partial.duration, StdDuration::ZERO);
    }

    #[test]
    fn pipeline_version_is_stable_and_sensitive() {
        // Deterministic across calls, and non-trivial (every analyzer folded in).
        assert_eq!(pipeline_version(), pipeline_version());
        assert_ne!(pipeline_version(), 0);
        // Sanity: hand-fold the current PIPELINE and match.
        let mut v: u32 = 0;
        for (id, ver) in PIPELINE {
            for b in id.bytes() {
                v = v.wrapping_mul(31).wrapping_add(b as u32);
            }
            v = v.wrapping_mul(31).wrapping_add(*ver);
        }
        assert_eq!(pipeline_version(), v);
    }
}
