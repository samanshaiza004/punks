//! Backend-level metadata tests, exercised through the public API exactly as the
//! browser does. The headline is the trust-building invariant: **editing the
//! Description must leave every byte Punks doesn't model untouched.**
//!
//! WAV is the backend Punks owns, so it's proven end-to-end here. The `lofty`
//! backend's file round-trip (and its own foreign-metadata-survives case) needs a
//! real checked-in audio fixture and lands with that fixture task; its capability
//! and routing are covered below since those need no file.

use std::path::{Path, PathBuf};

use punks_playback::{Backend, Capability, Field, MetadataBackend};

/// Append one RIFF chunk (id + LE size + body + pad byte if odd), as spec.
fn chunk(id: &[u8; 4], body: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

fn pcm_fmt() -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fmt.extend_from_slice(&1u16.to_le_bytes()); // mono
    fmt.extend_from_slice(&8000u32.to_le_bytes()); // sample rate
    fmt.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
    fmt.extend_from_slice(&2u16.to_le_bytes()); // block align
    fmt.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
    fmt
}

/// A minimal writable RIFF/WAVE with optional extra chunks between `fmt ` and `data`.
fn wav(extra: &[(&[u8; 4], &[u8])], data: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"WAVE");
    chunk(b"fmt ", &pcm_fmt(), &mut body);
    for (id, b) in extra {
        chunk(id, b, &mut body);
    }
    chunk(b"data", data, &mut body);
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// RF64-shaped WAV: the `RF64` form magic (with the `data` chunk carrying the
/// 0xFFFFFFFF sentinel size) marks it as the >4 GB / 64-bit variant — bext writes
/// are refused (Description ReadOnly).
fn rf64() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"WAVE");
    chunk(b"fmt ", &pcm_fmt(), &mut body);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    body.extend_from_slice(&[0u8, 0u8]);
    let mut out = Vec::new();
    out.extend_from_slice(b"RF64");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn tmp(tag: &str, bytes: &[u8]) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "punks2_meta_{tag}_{}_{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn wav_description_round_trip_via_backend() {
    let path = tmp("rt", &wav(&[], b"\x01\x02\x03\x04"));
    let backend = Backend::for_path(&path);

    assert!(backend.capability(Field::Description).can_write());
    assert_eq!(backend.read(&path).unwrap().description, None);

    let mut m = backend.read(&path).unwrap();
    m.description = Some("Rain on a tin roof".into());
    backend.write(&path, &m).unwrap();

    // Fresh backend + read: the value came back from the file, not a cache.
    let got = Backend::for_path(&path).read(&path).unwrap();
    assert_eq!(got.description.as_deref(), Some("Rain on a tin roof"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn foreign_wav_chunk_survives_description_edit() {
    // An unknown 'PNKS' chunk (16 bytes, even) plus the audio `data` must both
    // survive a Description write byte-for-byte.
    let foreign: &[u8] = b"do-not-touch-me!";
    let path = tmp("foreign", &wav(&[(b"PNKS", foreign)], b"AUDIODATA"));
    let backend = Backend::for_path(&path);

    let mut m = backend.read(&path).unwrap();
    m.description = Some("edited".into());
    backend.write(&path, &m).unwrap();

    let out = std::fs::read(&path).unwrap();
    assert!(
        out.windows(foreign.len()).any(|w| w == foreign),
        "foreign PNKS chunk was lost on a Description edit"
    );
    assert!(
        out.windows(9).any(|w| w == b"AUDIODATA"),
        "audio data was lost on a Description edit"
    );
    assert_eq!(
        backend.read(&path).unwrap().description.as_deref(),
        Some("edited")
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn capability_by_container() {
    // Plain RIFF/WAVE: Description is writable.
    let w = tmp("cap_wav", &wav(&[], b"\x00\x00"));
    assert_eq!(
        Backend::for_path(&w).capability(Field::Description),
        Capability::ReadWrite
    );
    // Unmapped fields are Unsupported everywhere this pass.
    assert_eq!(
        Backend::for_path(&w).capability(Field::Keywords),
        Capability::Unsupported
    );
    let _ = std::fs::remove_file(&w);

    // RF64: Description is readable but write-refused.
    let r = tmp("cap_rf64", &rf64());
    assert_eq!(
        Backend::for_path(&r).capability(Field::Description),
        Capability::ReadOnly
    );
    let _ = std::fs::remove_file(&r);

    // lofty formats route by extension and report Description ReadWrite without
    // needing the file to exist (routing is the container decision).
    for ext in ["mp3", "flac", "ogg", "aiff"] {
        let fake = PathBuf::from(format!("does-not-exist.{ext}"));
        assert_eq!(
            Backend::for_path(&fake).capability(Field::Description),
            Capability::ReadWrite,
            "{ext} should be a writable lofty container"
        );
    }
}

#[test]
fn unknown_extension_routes_to_lofty_not_wav() {
    // Anything that isn't a WAV variant is a lofty container (Description RW by
    // capability; an actual read would fail on a non-audio file, which is fine).
    let p = Path::new("mystery.xyz");
    assert_eq!(
        Backend::for_path(p).capability(Field::Description),
        Capability::ReadWrite
    );
}
