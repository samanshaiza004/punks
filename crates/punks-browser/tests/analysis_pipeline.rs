//! End-to-end check of the analysis worker's inner chain — the one path the
//! per-crate unit tests can't cover because it spans decode + analyze + store:
//! a real WAV is decoded, run through the analyzer registry, and stored/read
//! back through the library, exactly as the background worker does it (minus the
//! GUI/audio-device that the worker thread lives behind).

use std::io::Write;
use std::path::{Path, PathBuf};

use punks_analysis::{run_all, AnalysisContext, AnalysisReport, AudioBuffer};
use punks_library::Library;
use punks_playback::decode_file;

/// Write a mono 16-bit PCM WAV of `freq` Hz at `amp` for `frames` samples.
fn write_sine_wav(path: &Path, sample_rate: u32, freq: f32, amp: f32, frames: usize) {
    let mut pcm: Vec<u8> = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let s = amp * (std::f32::consts::TAU * freq * n as f32 / sample_rate as f32).sin();
        let v = (s * i16::MAX as f32) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    let byte_rate = sample_rate * 2;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap(); // PCM fmt chunk size
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
    f.write_all(&sample_rate.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap(); // block align
    f.write_all(&16u16.to_le_bytes()).unwrap(); // bits/sample
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    f.write_all(&pcm).unwrap();
}

#[test]
fn worker_chain_decodes_analyzes_and_stores() {
    let dir = std::env::temp_dir().join(format!("punks_pipe_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sr = 48_000;
    let wav = dir.join("tone.wav");
    write_sine_wav(&wav, sr, 1000.0, 0.8, sr as usize); // 1s of 1 kHz

    let mut lib = Library::create(&dir).unwrap();
    lib.reconcile(&punks_library::scan_files(&dir).unwrap())
        .unwrap();
    lib.enqueue_all(punks_analysis::pipeline_version()).unwrap();

    // The worker's inner loop, verbatim: claim → decode → run_all → store.
    let claimed = lib.claim_next_pending().unwrap().expect("a pending job");
    assert_eq!(claimed, wav);
    let decoded = decode_file(&claimed).unwrap();
    let ctx = AnalysisContext {
        audio: AudioBuffer::new(&decoded.interleaved, decoded.sample_rate, decoded.channels),
        source_duration: decoded.source_duration,
    };
    let report = run_all(&ctx);
    lib.store_analysis(&claimed, &report.metrics(), 7).unwrap();
    assert!(lib.claim_next_pending().unwrap().is_none()); // queue drained

    // Read back through the same seam the UI cache uses.
    let stored = AnalysisReport::from_metrics(&lib.analysis_metrics(&wav).unwrap());
    assert_eq!(stored, report);
    // Sanity on the numbers: 1 kHz sine at amp 0.8.
    assert!((stored.peak - 0.8).abs() < 0.02, "peak {}", stored.peak);
    assert!(
        (stored.rms - 0.8 / std::f32::consts::SQRT_2).abs() < 0.02,
        "rms {}",
        stored.rms
    );
    assert!(
        (stored.zcr - 2.0 * 1000.0 / sr as f32).abs() < 1e-2,
        "zcr {}",
        stored.zcr
    );
    // 1 s WAV → duration ≈ 1.0 s, round-tripped through storage.
    assert!(
        (stored.duration.as_secs_f64() - 1.0).abs() < 1e-3,
        "duration {:?}",
        stored.duration
    );
    assert_eq!(lib.job_status(&wav).unwrap().as_deref(), Some("done"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that won't decode is marked `error`, not left spinning in the queue.
#[test]
fn undecodable_file_is_failed_not_reclaimed() {
    let dir: PathBuf = std::env::temp_dir().join(format!("punks_pipe_bad_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let junk = dir.join("broken.wav");
    std::fs::write(&junk, b"not really a wav").unwrap();

    let mut lib = Library::create(&dir).unwrap();
    lib.reconcile(&punks_library::scan_files(&dir).unwrap())
        .unwrap();
    lib.enqueue_all(punks_analysis::pipeline_version()).unwrap();

    let claimed = lib.claim_next_pending().unwrap().unwrap();
    match decode_file(&claimed) {
        Ok(_) => panic!("junk should not decode"),
        Err(e) => lib.fail_analysis(&claimed, &e.to_string()).unwrap(),
    }
    assert_eq!(lib.job_status(&junk).unwrap().as_deref(), Some("error"));
    assert!(lib.claim_next_pending().unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}
