//! End-to-end check of the analysis worker's inner chain — the one path the
//! per-crate unit tests can't cover because it spans decode + analyze + store:
//! a real WAV is decoded, run through the analyzer registry, and stored/read
//! back through the library, exactly as the background worker does it (minus the
//! GUI/audio-device that the worker thread lives behind).

use std::io::Write;
use std::path::{Path, PathBuf};

use punks_audio::{decode_file, Backend, Field, Metadata, MetadataBackend};
use punks_audio::{run_all, AnalysisContext, AnalysisReport, AudioBuffer};
use punks_library::{Fact, Library};

/// Store a report's typed facts, then read them back into a report (the same
/// numeric/text split the browser's bridge does, via public API only).
fn store_report(lib: &mut Library, path: &Path, report: &AnalysisReport, dur: u32) {
    let mut facts: Vec<(&str, Fact)> = report
        .numeric_facts()
        .into_iter()
        .map(|(k, v)| (k, Fact::Real(v)))
        .collect();
    for (k, v) in report.text_facts() {
        facts.push((k, Fact::Text(v)));
    }
    lib.store_analysis(path, &facts, dur).unwrap();
}

fn read_report(lib: &Library, path: &Path) -> AnalysisReport {
    let mut numeric = Vec::new();
    let mut text = Vec::new();
    for (m, f) in lib.facts(path).unwrap() {
        match f {
            Fact::Real(v) => numeric.push((m, v)),
            Fact::Text(s) => text.push((m, s)),
            Fact::Blob(_) => {}
        }
    }
    AnalysisReport::from_facts(&numeric, &text)
}

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
    // Name carries facts (instrument/BPM) the Filename analyzer must recover.
    let wav = dir.join("kick_120bpm.wav");
    write_sine_wav(&wav, sr, 1000.0, 0.8, sr as usize); // 1s of 1 kHz

    let mut lib = Library::create(&dir).unwrap();
    lib.reconcile(&punks_library::scan_files(&dir).unwrap())
        .unwrap();
    lib.enqueue_all(punks_audio::pipeline_version()).unwrap();

    // The worker's inner loop, verbatim: claim → decode → run_all → store.
    let claimed = lib.claim_next_pending().unwrap().expect("a pending job");
    assert_eq!(claimed, wav);
    let decoded = decode_file(&claimed).unwrap();
    let ctx = AnalysisContext {
        audio: AudioBuffer::new(&decoded.interleaved, decoded.sample_rate, decoded.channels),
        source_duration: decoded.source_duration,
        file_stem: claimed.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    };
    let report = run_all(&ctx);
    store_report(&mut lib, &claimed, &report, 7);
    assert!(lib.claim_next_pending().unwrap().is_none()); // queue drained

    // Read back through the same seam the UI cache uses.
    let stored = read_report(&lib, &wav);
    assert_eq!(stored, report);
    // DSP facts: 1 kHz sine at amp 0.8.
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
    // Filename facts recovered end-to-end, stored as text/numeric, read back.
    assert_eq!(stored.instrument.as_deref(), Some("kick"));
    assert_eq!(stored.bpm, Some(120.0));
    assert_eq!(lib.job_status(&wav).unwrap().as_deref(), Some("done"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// The priority path (`claim_path`) lets the worker fully decode/analyze/store
/// a specific asset out of FIFO order — the mechanism behind "jump the backlog
/// to whatever the user just selected."
#[test]
fn priority_claim_completes_out_of_fifo_order() {
    let dir = std::env::temp_dir().join(format!("punks_pipe_prio_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sr = 8_000;
    let a = dir.join("a.wav");
    let b = dir.join("b.wav");
    write_sine_wav(&a, sr, 440.0, 0.5, sr as usize / 10);
    write_sine_wav(&b, sr, 440.0, 0.5, sr as usize / 10);

    let mut lib = Library::create(&dir).unwrap();
    lib.reconcile(&punks_library::scan_files(&dir).unwrap())
        .unwrap();
    lib.enqueue_all(punks_audio::pipeline_version()).unwrap();

    // Jump straight to "b" via claim_path — the same call the worker makes for
    // a priority request — bypassing claim_next_pending's FIFO order entirely,
    // and run it through the full decode/analyze/store chain.
    let claimed = lib.claim_path(&b).unwrap().expect("b claimable");
    assert_eq!(claimed, b);
    let decoded = decode_file(&claimed).unwrap();
    let ctx = AnalysisContext {
        audio: AudioBuffer::new(&decoded.interleaved, decoded.sample_rate, decoded.channels),
        source_duration: decoded.source_duration,
        file_stem: claimed.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    };
    let report = run_all(&ctx);
    store_report(&mut lib, &claimed, &report, 1);

    // "b" is done without ever touching claim_next_pending; "a" is untouched.
    assert_eq!(lib.job_status(&b).unwrap().as_deref(), Some("done"));
    assert_eq!(lib.job_status(&a).unwrap().as_deref(), Some("pending"));

    // FIFO backlog still picks up "a" normally afterwards.
    assert_eq!(lib.claim_next_pending().unwrap(), Some(a));

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
    lib.enqueue_all(punks_audio::pipeline_version()).unwrap();

    let claimed = lib.claim_next_pending().unwrap().unwrap();
    match decode_file(&claimed) {
        Ok(_) => panic!("junk should not decode"),
        Err(e) => lib.fail_analysis(&claimed, &e.to_string()).unwrap(),
    }
    assert_eq!(lib.job_status(&junk).unwrap().as_deref(), Some("error"));
    assert!(lib.claim_next_pending().unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

/// The embedded-metadata round trip end to end: write a bext Description into
/// a real WAV, run it through the exact worker chain (claim → decode →
/// run_all → store, plus the description-caching line `analyze_claimed` adds),
/// and confirm the cache reflects what's on disk — proving the DB is a
/// rebuildable cache of the file's own embedded metadata, not a second source
/// of truth for it.
#[test]
fn bext_description_is_written_then_cached_by_analysis() {
    let dir = std::env::temp_dir().join(format!("punks_pipe_bext_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("field_recording.wav");
    write_sine_wav(&wav, 8_000, 440.0, 0.5, 8_000);

    let backend = Backend::for_path(&wav);
    assert!(backend.capability(Field::Description).can_write());
    backend
        .write(
            &wav,
            &Metadata {
                description: Some("Footsteps, gravel, take 3".into()),
                ..Default::default()
            },
        )
        .unwrap();

    let mut lib = Library::create(&dir).unwrap();
    lib.reconcile(&punks_library::scan_files(&dir).unwrap())
        .unwrap();
    lib.enqueue_all(punks_audio::pipeline_version()).unwrap();

    // The worker's inner loop, plus the description-caching line it adds.
    let claimed = lib.claim_next_pending().unwrap().unwrap();
    let decoded = decode_file(&claimed).unwrap();
    assert_eq!(
        decoded.metadata.description.as_deref(),
        Some("Footsteps, gravel, take 3")
    );
    let ctx = AnalysisContext {
        audio: AudioBuffer::new(&decoded.interleaved, decoded.sample_rate, decoded.channels),
        source_duration: decoded.source_duration,
        file_stem: claimed.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    };
    let report = run_all(&ctx);
    let mut facts: Vec<(&str, Fact)> = report
        .numeric_facts()
        .into_iter()
        .map(|(k, v)| (k, Fact::Real(v)))
        .collect();
    for (k, v) in report.text_facts() {
        facts.push((k, Fact::Text(v)));
    }
    if let Some(desc) = decoded.metadata.description {
        facts.push(("description", Fact::Text(desc)));
    }
    lib.store_analysis(&claimed, &facts, 3).unwrap();

    let stored = lib.facts(&wav).unwrap();
    assert!(stored.contains(&(
        "description".to_string(),
        Fact::Text("Footsteps, gravel, take 3".to_string())
    )));

    // A later edit through Library::set_description (the narrow write-through
    // path SampleBrowser uses) updates just that fact, without disturbing the
    // job the analysis pass already completed.
    lib.set_description(&wav, "Footsteps, gravel, take 4 (better)")
        .unwrap();
    let stored = lib.facts(&wav).unwrap();
    assert!(stored.contains(&(
        "description".to_string(),
        Fact::Text("Footsteps, gravel, take 4 (better)".to_string())
    )));
    assert_eq!(lib.job_status(&wav).unwrap().as_deref(), Some("done"));

    let _ = std::fs::remove_dir_all(&dir);
}
