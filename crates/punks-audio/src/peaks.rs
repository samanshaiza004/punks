pub const DEFAULT_NUM_BUCKETS: usize = 512;

#[derive(Debug, Clone)]
pub struct WaveformPeaks {
    pub peaks: Vec<(f32, f32)>,
    pub num_buckets: usize,
}

pub fn compute_peaks(samples: &[f32], channels: usize, num_buckets: usize) -> WaveformPeaks {
    let channels = channels.max(1);
    let num_frames = samples.len() / channels;

    if num_frames == 0 || num_buckets == 0 {
        return WaveformPeaks {
            peaks: vec![(0.0, 0.0); num_buckets],
            num_buckets,
        };
    }

    let frames_per_bucket = (num_frames as f64 / num_buckets as f64).max(1.0);
    let inv_channels = 1.0 / channels as f32;
    let mut peaks = Vec::with_capacity(num_buckets);

    for bucket in 0..num_buckets {
        let start = (bucket as f64 * frames_per_bucket) as usize;
        let end = (((bucket + 1) as f64 * frames_per_bucket) as usize).min(num_frames);

        let mut min = f32::MAX;
        let mut max = f32::MIN;

        for frame in start..end {
            let base = frame * channels;
            let mut mono = 0.0f32;
            for ch in 0..channels {
                mono += samples[base + ch];
            }
            mono *= inv_channels;
            min = min.min(mono);
            max = max.max(mono);
        }

        if min == f32::MAX {
            min = 0.0;
            max = 0.0;
        }

        peaks.push((min.clamp(-1.0, 1.0), max.clamp(-1.0, 1.0)));
    }

    WaveformPeaks { peaks, num_buckets }
}
