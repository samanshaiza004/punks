//! Time-domain audio feature analysis: RMS, peak, and zero-crossing rate.
//!
//! Pure DSP with no dependencies — no file I/O, no SQLite, no playback. Callers
//! decode audio elsewhere and hand a borrowed [`AudioBuffer`] to a static
//! `Analyzer::analyze`. Each analyzer type *is* its own output.
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

/// A feature analyzer. `analyze` is static — there's no analyzer state to
/// configure — and returns the analyzer's own result type. `VERSION` bumps
/// whenever the algorithm changes, so persisted results can be invalidated.
pub trait Analyzer {
    const VERSION: u32;
    type Output;
    fn analyze(buf: &AudioBuffer) -> Self::Output;
}

/// Linear amplitude below which dBFS is floored to stay finite (≈ -180 dBFS),
/// so silence stores a real number rather than `-inf`.
const DBFS_FLOOR_AMP: f32 = 1e-9;

/// Root-mean-square level over the whole buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rms {
    /// Linear RMS amplitude.
    pub value: f32,
    /// The same level in dBFS (`20·log10(value)`), floored finite.
    pub dbfs: f32,
}

impl Analyzer for Rms {
    const VERSION: u32 = 1;
    type Output = Rms;

    fn analyze(buf: &AudioBuffer) -> Rms {
        let n = buf.samples.len();
        if n == 0 {
            return Rms {
                value: 0.0,
                dbfs: 20.0 * DBFS_FLOOR_AMP.log10(),
            };
        }
        // Accumulate in f64 so long buffers don't lose precision.
        let sum_sq: f64 = buf.samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
        let value = (sum_sq / n as f64).sqrt() as f32;
        Rms {
            value,
            dbfs: 20.0 * value.max(DBFS_FLOOR_AMP).log10(),
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
    const VERSION: u32 = 1;
    type Output = Peak;

    fn analyze(buf: &AudioBuffer) -> Peak {
        let value = buf.samples.iter().fold(0.0_f32, |m, &x| m.max(x.abs()));
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
    const VERSION: u32 = 1;
    type Output = Zcr;

    fn analyze(buf: &AudioBuffer) -> Zcr {
        let ch = buf.channels.max(1) as usize;
        let frames = buf.samples.len() / ch;
        if frames < 2 {
            return Zcr { value: 0.0 };
        }
        let mut crossings: u64 = 0;
        for c in 0..ch {
            let mut prev_neg = buf.samples[c] < 0.0;
            for f in 1..frames {
                let neg = buf.samples[f * ch + c] < 0.0;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, sample_rate: u32, amp: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|n| amp * (std::f32::consts::TAU * freq * n as f32 / sample_rate as f32).sin())
            .collect()
    }

    #[test]
    fn silence_is_zero_everywhere() {
        let s = vec![0.0f32; 1000];
        let buf = AudioBuffer::new(&s, 48_000, 1);
        assert_eq!(Rms::analyze(&buf).value, 0.0);
        assert_eq!(Peak::analyze(&buf).value, 0.0);
        assert_eq!(Zcr::analyze(&buf).value, 0.0);
        // Silence floors to a finite, very low dBFS (not -inf).
        assert!(Rms::analyze(&buf).dbfs < -150.0 && Rms::analyze(&buf).dbfs.is_finite());
    }

    #[test]
    fn constant_half_is_the_known_rms_fixture() {
        // A DC signal of 0.5 has RMS exactly 0.5 and peak 0.5; no sign changes.
        let s = vec![0.5f32; 512];
        let buf = AudioBuffer::new(&s, 48_000, 1);
        assert!((Rms::analyze(&buf).value - 0.5).abs() < 1e-6);
        assert!((Rms::analyze(&buf).dbfs - (20.0 * 0.5_f32.log10())).abs() < 1e-4);
        assert_eq!(Peak::analyze(&buf).value, 0.5);
        assert_eq!(Zcr::analyze(&buf).value, 0.0);
    }

    #[test]
    fn sine_has_expected_rms_peak_and_zcr() {
        let sr = 48_000;
        let freq = 1000.0;
        let amp = 0.8;
        let s = sine(freq, sr, amp, sr as usize); // 1 second
        let buf = AudioBuffer::new(&s, sr, 1);

        // RMS of a sine is amp / sqrt(2).
        let expected_rms = amp / std::f32::consts::SQRT_2;
        assert!((Rms::analyze(&buf).value - expected_rms).abs() < 1e-3);
        // Peak ≈ amp.
        assert!((Peak::analyze(&buf).value - amp).abs() < 1e-3);
        // ZCR ≈ 2·freq/sample_rate (two crossings per cycle).
        let expected_zcr = 2.0 * freq / sr as f32;
        assert!((Zcr::analyze(&buf).value - expected_zcr).abs() < 1e-3);
    }

    #[test]
    fn alternating_sign_is_full_zcr() {
        let s: Vec<f32> = (0..256)
            .map(|n| if n % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let buf = AudioBuffer::new(&s, 48_000, 1);
        assert!((Zcr::analyze(&buf).value - 1.0).abs() < 1e-6);
        assert_eq!(Peak::analyze(&buf).value, 1.0);
        assert!((Rms::analyze(&buf).value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zcr_is_per_channel_not_across_interleave() {
        // Two channels, each a constant with no sign change within the channel:
        // L = +1, R = -1 interleaved. A naive flat walk would see a crossing on
        // every step; the per-channel computation must see none.
        let s = vec![1.0f32, -1.0, 1.0, -1.0, 1.0, -1.0];
        let buf = AudioBuffer::new(&s, 48_000, 2);
        assert_eq!(Zcr::analyze(&buf).value, 0.0);
    }

    #[test]
    fn version_is_readable() {
        assert_eq!(Rms::VERSION, 1);
        assert_eq!(Peak::VERSION, 1);
        assert_eq!(Zcr::VERSION, 1);
    }
}
