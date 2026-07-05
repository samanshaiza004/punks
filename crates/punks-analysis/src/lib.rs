//! Time-domain audio feature analysis: RMS, peak, zero-crossing rate, duration.
//!
//! Pure DSP with no dependencies — no file I/O, no SQLite, no playback. Callers
//! decode audio elsewhere and hand a borrowed [`AnalysisContext`] (the audio plus
//! cheaply-known source facts) to a static `Analyzer::analyze`. Each analyzer type
//! *is* its own output.
//!
//! This crate owns the *algorithms*. The orchestrator runs [`run_all`] on a
//! context to get one [`AnalysisReport`], then hands its flattened facts
//! ([`AnalysisReport::numeric_facts`] / [`AnalysisReport::text_facts`]) to
//! storage — storage never names an analyzer. [`pipeline_version`] folds every
//! analyzer's `VERSION` so persisted results invalidate when any algorithm
//! changes.
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
    /// The file name without extension, for name-derived facts (instrument, BPM,
    /// key). The one non-audio input — analyzers that read it don't touch DSP.
    pub file_stem: &'a str,
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

/// Facts read from the file *name* (no DSP): instrument, tempo, musical key.
/// Deterministic observations — the parser is deliberately conservative, each
/// rule is a named function ([`instrument_rule`], [`bpm_rule`], [`key_rule`]), and
/// the negative guards (drum-machine numbers, indexes, sample rates) matter as
/// much as the matches. See the `filename` tests.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filename {
    /// Canonical instrument/type (e.g. `"kick"`), if a token matched the dict.
    pub instrument: Option<&'static str>,
    pub bpm: Option<f32>,
    /// Normalized musical key (e.g. `"F#min"`, `"Amin"`, `"9B"`).
    pub key: Option<String>,
}

impl Analyzer for Filename {
    const ID: &'static str = "filename";
    const VERSION: u32 = 1;
    type Output = Filename;

    fn analyze(ctx: &AnalysisContext) -> Filename {
        let tokens = tokenize(ctx.file_stem);
        Filename {
            instrument: instrument_rule(&tokens),
            bpm: bpm_rule(&tokens),
            key: key_rule(&tokens),
        }
    }
}

/// Split a stem into lowercase tokens on real separators only (`_ - . space`),
/// keeping `#` so sharps survive (`F#min`) and Camelot codes stay glued (`9B`).
/// Glued cases like `128bpm` / `Kick128` are handled inside the rules.
fn tokenize(stem: &str) -> Vec<String> {
    stem.split(|c: char| c == '_' || c == '-' || c == '.' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

/// InstrumentAliasRule: first token matching an alias → canonical name. Also
/// tries the token with trailing digits trimmed, so `Kick128` still matches.
fn instrument_rule(tokens: &[String]) -> Option<&'static str> {
    // (canonical, aliases). First matching token (in name order) wins.
    const DICT: &[(&str, &[&str])] = &[
        ("kick", &["kick", "kik", "bd", "bassdrum"]),
        ("snare", &["snare", "snr", "sd"]),
        ("clap", &["clap", "clp"]),
        ("hihat", &["hihat", "hats", "hat", "hh"]),
        ("openhat", &["openhat", "ohh", "oh"]),
        ("tom", &["tom", "toms"]),
        ("crash", &["crash", "cymbal", "cym"]),
        ("ride", &["ride"]),
        ("rim", &["rim", "rimshot"]),
        ("shaker", &["shaker", "shake"]),
        ("perc", &["perc", "percussion"]),
        ("bass", &["bass", "sub", "808"]),
        ("lead", &["lead"]),
        ("pad", &["pad", "pads"]),
        ("pluck", &["pluck"]),
        ("vocal", &["vocal", "vocals", "vox"]),
        ("fx", &["fx", "sfx"]),
        ("riser", &["riser", "rise", "uplifter"]),
        ("sweep", &["sweep"]),
    ];
    let lookup = |t: &str| {
        DICT.iter()
            .find(|(_, aliases)| aliases.contains(&t))
            .map(|(canonical, _)| *canonical)
    };
    tokens.iter().find_map(|tok| {
        lookup(tok).or_else(|| {
            let trimmed = tok.trim_end_matches(|c: char| c.is_ascii_digit());
            (trimmed != tok).then(|| lookup(trimmed)).flatten()
        })
    })
}

/// Numbers that look like tempos but aren't: classic drum machines.
const DRUM_MACHINES: &[u32] = &[808, 909, 707, 606, 505, 727, 626];

/// BpmKeywordRule (high confidence) then BpmStandaloneRule (medium, guarded):
/// a `bpm`-tagged number wins; else a lone plausible tempo if unambiguous.
fn bpm_rule(tokens: &[String]) -> Option<f32> {
    for (i, tok) in tokens.iter().enumerate() {
        // Glued: "128bpm".
        if let Some(digits) = tok.strip_suffix("bpm") {
            if let Some(n) = parse_tempo(digits) {
                return Some(n as f32);
            }
        }
        // Separated: a number adjacent to a "bpm" token.
        if tok == "bpm" {
            let neighbor = i
                .checked_sub(1)
                .and_then(|j| tokens.get(j))
                .or_else(|| tokens.get(i + 1));
            if let Some(n) = neighbor.and_then(|t| parse_tempo(t)) {
                return Some(n as f32);
            }
        }
    }
    // BpmStandaloneRule: unique plausible tempo. Ambiguous (≥2) → None.
    let mut candidates = tokens.iter().filter_map(|t| plausible_tempo(t));
    match (candidates.next(), candidates.next()) {
        (Some(n), None) => Some(n as f32),
        _ => None,
    }
}

/// A 2–3 digit tempo token in the broad BPM range (for keyword-tagged numbers).
fn parse_tempo(tok: &str) -> Option<u32> {
    if !(2..=3).contains(&tok.len()) || !tok.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    tok.parse::<u32>().ok().filter(|&n| (30..=300).contains(&n))
}

/// A standalone number allowed to be a tempo: index guard (leading zero) +
/// DrumMachineGuard + a tighter musical range than the keyword case.
fn plausible_tempo(tok: &str) -> Option<u32> {
    if tok.starts_with('0') {
        return None; // "04" is an index, not a tempo
    }
    let n = parse_tempo(tok)?;
    if DRUM_MACHINES.contains(&n) || !(60..=200).contains(&n) {
        return None;
    }
    Some(n)
}

/// MusicalKeyRule / CamelotKeyRule: a token is a key only with an explicit
/// accidental/quality (`Cmaj`, `F#min`, `Am`, `Bb`) or Camelot (`9B`). A bare
/// letter is rejected — precision over recall.
fn key_rule(tokens: &[String]) -> Option<String> {
    tokens
        .iter()
        .find_map(|t| musical_key(t).or_else(|| camelot_key(t)))
}

fn musical_key(tok: &str) -> Option<String> {
    let note = tok.bytes().next().filter(|b| (b'a'..=b'g').contains(b))?;
    let mut rest = &tok[1..];
    let mut acc = "";
    if let Some(r) = rest.strip_prefix('#') {
        (acc, rest) = ("#", r);
    } else if let Some(r) = rest.strip_prefix('b') {
        (acc, rest) = ("b", r);
    }
    let quality = match rest {
        "" => "",
        "m" | "min" | "minor" => "min",
        "maj" | "major" => "maj",
        _ => return None, // trailing junk → not a key
    };
    // Require a disambiguator (accidental or quality); bare "c" is rejected.
    if acc.is_empty() && quality.is_empty() {
        return None;
    }
    Some(format!(
        "{}{}{}",
        (note as char).to_ascii_uppercase(),
        acc,
        quality
    ))
}

fn camelot_key(tok: &str) -> Option<String> {
    let (num, letter) = tok.split_at(tok.len().checked_sub(1)?);
    let letter = match letter {
        "a" => "A",
        "b" => "B",
        _ => return None,
    };
    let n: u32 = num.parse().ok()?;
    (1..=12).contains(&n).then(|| format!("{n}{letter}"))
}

/// The registered analyzers as `(id, version)`, in run order. The single place
/// that knows the analyzer set; [`run_all`] and [`pipeline_version`] read it.
/// When an analyzer gains a non-empty `DEPENDS_ON`, sort this topologically.
const PIPELINE: &[(&str, u32)] = &[
    (Rms::ID, Rms::VERSION),
    (Peak::ID, Peak::VERSION),
    (Zcr::ID, Zcr::VERSION),
    (Duration::ID, Duration::VERSION),
    (Filename::ID, Filename::VERSION),
];

/// One asset's analysis facts. A typed carrier so analyzers stay type-safe, with
/// [`numeric_facts`](Self::numeric_facts)/[`text_facts`](Self::text_facts) /
/// [`from_facts`](Self::from_facts) as the only seam to storage — the DB schema
/// can evolve without the analyzers, and storage never names a field. Grows by
/// adding a field plus a line in the seam methods; nothing else moves. Optional
/// fields are absent from the flattened facts when `None` (not defaulted).
#[derive(Debug, Clone, PartialEq, Default)]
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
    /// Tempo parsed from the file name, if found.
    pub bpm: Option<f32>,
    /// Instrument/type parsed from the file name (canonical), if found.
    pub instrument: Option<String>,
    /// Musical key parsed from the file name (normalized), if found.
    pub key: Option<String>,
}

impl AnalysisReport {
    /// Flatten numeric facts to `(metric, value)` pairs. Optional numeric facts
    /// (`bpm`) appear only when present.
    pub fn numeric_facts(&self) -> Vec<(&'static str, f64)> {
        let mut out = vec![
            ("rms", self.rms as f64),
            ("rms_dbfs", self.rms_dbfs as f64),
            ("peak", self.peak as f64),
            ("zcr", self.zcr as f64),
            ("duration", self.duration.as_secs_f64()),
        ];
        if let Some(bpm) = self.bpm {
            out.push(("bpm", bpm as f64));
        }
        out
    }

    /// Flatten text facts to `(metric, value)` pairs — only the present ones.
    pub fn text_facts(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(i) = &self.instrument {
            out.push(("instrument", i.clone()));
        }
        if let Some(k) = &self.key {
            out.push(("key", k.clone()));
        }
        out
    }

    /// Rebuild from stored numeric + text facts. Always-present numeric facts
    /// default to 0 when absent; optional facts stay `None`.
    pub fn from_facts(numeric: &[(String, f64)], text: &[(String, String)]) -> Self {
        let raw = |k: &str| numeric.iter().find(|(m, _)| m == k).map(|(_, v)| *v);
        let get = |k: &str| raw(k).map(|v| v as f32).unwrap_or_default();
        let text_of = |k: &str| text.iter().find(|(m, _)| m == k).map(|(_, v)| v.clone());
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
            bpm: raw("bpm").map(|v| v as f32),
            instrument: text_of("instrument"),
            key: text_of("key"),
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
    let fname = Filename::analyze(ctx);
    AnalysisReport {
        rms: rms.value,
        rms_dbfs: rms.dbfs,
        peak: peak.value,
        zcr: zcr.value,
        duration: duration.value,
        bpm: fname.bpm,
        instrument: fname.instrument.map(String::from),
        key: fname.key,
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
            file_stem: "",
        }
    }

    /// Parse just a file stem (no audio needed).
    fn parse(stem: &str) -> Filename {
        Filename::analyze(&AnalysisContext {
            audio: AudioBuffer::new(&[], 48_000, 1),
            source_duration: StdDuration::ZERO,
            file_stem: stem,
        })
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
            file_stem: "",
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
        assert_eq!(Filename::VERSION, 1);
    }

    #[test]
    fn run_all_fills_the_whole_report() {
        let s = sine(1000.0, 48_000, 0.8, 48_000);
        let mut c = ctx(&s, 48_000, 1);
        c.file_stem = "Kick_128";
        let r = run_all(&c);
        assert!((r.rms - 0.8 / std::f32::consts::SQRT_2).abs() < 1e-3);
        assert!((r.peak - 0.8).abs() < 1e-3);
        assert!((r.zcr - 2.0 * 1000.0 / 48_000.0).abs() < 1e-3);
        assert!(r.rms_dbfs < 0.0 && r.rms_dbfs.is_finite());
        assert!((r.duration.as_secs_f64() - 1.0).abs() < 1e-6); // 1s buffer
        assert_eq!(r.instrument.as_deref(), Some("kick"));
        assert_eq!(r.bpm, Some(128.0));
    }

    // --- Filename parser (each rule named; negatives are the point) ---

    #[test]
    fn instrument_alias_rule() {
        assert_eq!(parse("Kick").instrument, Some("kick"));
        assert_eq!(parse("vocal_loop").instrument, Some("vocal"));
        assert_eq!(parse("hihat_closed").instrument, Some("hihat"));
        assert_eq!(parse("Kick128").instrument, Some("kick")); // glued trailing digits
        assert_eq!(parse("mysound").instrument, None);
    }

    #[test]
    fn bpm_keyword_rule() {
        assert_eq!(parse("loop_128bpm").bpm, Some(128.0)); // glued
        assert_eq!(parse("loop_90_bpm").bpm, Some(90.0)); // adjacent
                                                          // Keyword wins even when a decoy standalone is present.
        assert_eq!(parse("vocal_150_128bpm").bpm, Some(128.0));
    }

    #[test]
    fn bpm_standalone_rule_and_guards() {
        assert_eq!(parse("Kick_128").bpm, Some(128.0));
        assert_eq!(parse("Snare_04").bpm, None); // leading-zero index
        assert_eq!(parse("808_Bass").bpm, None); // DrumMachineGuard
        assert_eq!(parse("track_44100_24_128").bpm, Some(128.0)); // rate/bits out of range
        assert_eq!(parse("loop_120_140").bpm, None); // ambiguous → None
    }

    #[test]
    fn musical_and_camelot_key_rules() {
        assert_eq!(parse("Lead_Am").key.as_deref(), Some("Amin"));
        assert_eq!(parse("pad_Cmaj").key.as_deref(), Some("Cmaj"));
        assert_eq!(parse("vox_F#min").key.as_deref(), Some("F#min"));
        assert_eq!(parse("bass_Bb").key.as_deref(), Some("Bb"));
        assert_eq!(parse("loop_9B").key.as_deref(), Some("9B")); // Camelot
        assert_eq!(parse("take_C").key, None); // bare letter rejected
        assert_eq!(parse("mysound").key, None);
    }

    #[test]
    fn combined_facts() {
        let f = parse("808_Bass_Cmin");
        assert_eq!(f.instrument, Some("bass"));
        assert_eq!(f.bpm, None); // 808 guarded
        assert_eq!(f.key.as_deref(), Some("Cmin"));

        let f = parse("vocal_loop_90bpm_F#min");
        assert_eq!(f.instrument, Some("vocal"));
        assert_eq!(f.bpm, Some(90.0));
        assert_eq!(f.key.as_deref(), Some("F#min"));
    }

    #[test]
    fn facts_round_trip() {
        let r = AnalysisReport {
            rms: 0.5,
            rms_dbfs: -6.02,
            peak: 0.9,
            zcr: 0.1,
            duration: StdDuration::from_secs_f64(3.482),
            bpm: Some(128.0),
            instrument: Some("kick".into()),
            key: Some("F#min".into()),
        };
        let numeric: Vec<(String, f64)> = r
            .numeric_facts()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let text: Vec<(String, String)> = r
            .text_facts()
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect();
        assert_eq!(AnalysisReport::from_facts(&numeric, &text), r);

        // Optionals absent from storage → None; always-present numeric defaults.
        let partial = AnalysisReport::from_facts(&[("peak".to_string(), 0.7)], &[]);
        assert_eq!(partial.peak, 0.7);
        assert_eq!(partial.rms, 0.0);
        assert_eq!(partial.duration, StdDuration::ZERO);
        assert_eq!(partial.bpm, None);
        assert_eq!(partial.instrument, None);
        assert_eq!(partial.key, None);
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
