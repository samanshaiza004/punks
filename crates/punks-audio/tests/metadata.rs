//! Backend-level metadata tests, exercised through the public API exactly as the
//! browser does. The headline is the trust-building invariant: **editing the
//! Description must leave every byte Punks doesn't model untouched.**
//!
//! WAV is the backend Punks owns, so it's proven end-to-end here. The `lofty`
//! backend's file round-trip (and its own foreign-metadata-survives case) needs a
//! real checked-in audio fixture and lands with that fixture task; its capability
//! and routing are covered below since those need no file.

use std::path::{Path, PathBuf};

use punks_audio::{Backend, Capability, Field, Metadata, MetadataBackend};

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
        "punks_meta_{tag}_{}_{}.wav",
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
    // Plain RIFF/WAVE: every mapped field is writable (Description→bext,
    // Creator→IART, Category→IGNR, Keywords→IKEY).
    let w = tmp("cap_wav", &wav(&[], b"\x00\x00"));
    for field in [
        Field::Description,
        Field::Keywords,
        Field::Category,
        Field::Creator,
    ] {
        assert_eq!(
            Backend::for_path(&w).capability(field),
            Capability::ReadWrite,
            "{field:?} should be writable on a plain WAV"
        );
    }
    let _ = std::fs::remove_file(&w);

    // RF64: readable but write-refused, for every field.
    let r = tmp("cap_rf64", &rf64());
    assert_eq!(
        Backend::for_path(&r).capability(Field::Description),
        Capability::ReadOnly
    );
    assert_eq!(
        Backend::for_path(&r).capability(Field::Creator),
        Capability::ReadOnly
    );
    let _ = std::fs::remove_file(&r);

    // lofty formats route by extension: Description/Creator/Category are
    // writable; Keywords has no standard lofty tag, so it's Unsupported there
    // (WAV-only). Routing needs no file on disk.
    for ext in ["mp3", "flac", "ogg", "aiff"] {
        let fake = PathBuf::from(format!("does-not-exist.{ext}"));
        let backend = Backend::for_path(&fake);
        for field in [Field::Description, Field::Creator, Field::Category] {
            assert_eq!(
                backend.capability(field),
                Capability::ReadWrite,
                "{ext} {field:?} should be writable"
            );
        }
        assert_eq!(
            backend.capability(Field::Keywords),
            Capability::Unsupported,
            "{ext} Keywords is WAV-only this pass"
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

#[test]
fn wav_riff_info_round_trips_and_preserves_unmapped_subfields() {
    // A LIST/INFO carrying an unmapped INAM subfield, plus a foreign chunk —
    // both must survive a Creator/Category/Keywords write byte-for-byte.
    let mut info = Vec::new();
    info.extend_from_slice(b"INFO");
    let name = b"Original Name\0"; // 14 bytes, even
    info.extend_from_slice(b"INAM");
    info.extend_from_slice(&(name.len() as u32).to_le_bytes());
    info.extend_from_slice(name);
    let path = tmp(
        "info",
        &wav(&[(b"LIST", &info), (b"PNKS", b"keepme!!")], b"AUDIODATA"),
    );
    let backend = Backend::for_path(&path);

    let mut m = backend.read(&path).unwrap();
    m.creator = Some("Recordist".into());
    m.category = Some("Foley".into());
    m.keywords = vec!["gravel".into(), "footsteps".into()];
    backend.write(&path, &m).unwrap();

    let got = Backend::for_path(&path).read(&path).unwrap();
    assert_eq!(got.creator.as_deref(), Some("Recordist"));
    assert_eq!(got.category.as_deref(), Some("Foley"));
    assert_eq!(
        got.keywords,
        vec!["gravel".to_string(), "footsteps".to_string()]
    );

    let raw = std::fs::read(&path).unwrap();
    assert!(
        raw.windows(4).any(|w| w == b"INAM"),
        "unmapped INAM subfield lost"
    );
    assert!(
        raw.windows(13).any(|w| w == b"Original Name"),
        "INAM value lost"
    );
    assert!(
        raw.windows(8).any(|w| w == b"keepme!!"),
        "foreign chunk lost on an INFO edit"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn wav_ixml_mirror_synced_and_unrelated_elements_survive() {
    // bext absent, so read falls back to the iXML mirror; a Description edit
    // updates bext AND the existing <BWF_DESCRIPTION> leaf, leaving <SCENE>
    // (a field punks doesn't model) untouched. Different-length text
    // exercises the chunk resize + RIFF-size patch.
    let ixml =
        b"<BWFXML><BEXT><BWF_DESCRIPTION>old</BWF_DESCRIPTION></BEXT><SCENE>MyScene</SCENE></BWFXML>";
    let path = tmp("ixml", &wav(&[(b"iXML", ixml.as_slice())], b"AUDIODATA"));
    let backend = Backend::for_path(&path);

    let mut m = backend.read(&path).unwrap();
    assert_eq!(m.description.as_deref(), Some("old"), "iXML fallback read");

    m.description = Some("a much longer new description".into());
    backend.write(&path, &m).unwrap();

    let raw = std::fs::read(&path).unwrap();
    let s = String::from_utf8_lossy(&raw);
    assert!(
        s.contains("<BWF_DESCRIPTION>a much longer new description</BWF_DESCRIPTION>"),
        "iXML mirror not updated"
    );
    assert!(
        s.contains("<SCENE>MyScene</SCENE>"),
        "unrelated iXML element lost"
    );
    assert_eq!(
        Backend::for_path(&path)
            .read(&path)
            .unwrap()
            .description
            .as_deref(),
        Some("a much longer new description"),
        "bext now carries it (precedence over the mirror)"
    );
    assert!(raw.windows(9).any(|w| w == b"AUDIODATA"), "audio data lost");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn wav_ixml_note_read_fallback() {
    let ixml = b"<BWFXML><NOTE>just a note</NOTE></BWFXML>";
    let path = tmp("note", &wav(&[(b"iXML", ixml.as_slice())], b"AUDIODATA"));
    assert_eq!(
        Backend::for_path(&path)
            .read(&path)
            .unwrap()
            .description
            .as_deref(),
        Some("just a note")
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn wav_ixml_never_created_or_populated() {
    // No iXML → an edit must not create one.
    let path = tmp("noixml", &wav(&[], b"AUDIODATA"));
    let mut m = Metadata {
        description: Some("hello".into()),
        ..Default::default()
    };
    Backend::for_path(&path).write(&path, &m).unwrap();
    assert!(
        !std::fs::read(&path)
            .unwrap()
            .windows(4)
            .any(|w| w == b"iXML"),
        "iXML must never be created"
    );
    let _ = std::fs::remove_file(&path);

    // iXML present but with no <BWF_DESCRIPTION> mirror → an edit must not add
    // one (never create the leaf), and must preserve the existing content.
    let ixml = b"<BWFXML><SCENE>S1</SCENE></BWFXML>";
    let path = tmp(
        "nomirror",
        &wav(&[(b"iXML", ixml.as_slice())], b"AUDIODATA"),
    );
    m.description = Some("desc".into());
    Backend::for_path(&path).write(&path, &m).unwrap();
    let s = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
    assert!(
        !s.contains("BWF_DESCRIPTION"),
        "must not create a mirror leaf"
    );
    assert!(s.contains("<SCENE>S1</SCENE>"), "iXML content preserved");
    let _ = std::fs::remove_file(&path);
}
