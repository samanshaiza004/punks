use punks_audio::decode_file;
use std::path::Path;

#[test]
fn decodes_supported_sample_formats() {
    for extension in ["wav", "flac", "mp3", "ogg"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(format!("tone.{extension}"));
        let decoded = decode_file(&path).unwrap_or_else(|error| {
            panic!(
                "failed to decode {extension} fixture {}: {error}",
                path.display()
            )
        });

        assert_eq!(decoded.sample_rate, 48_000, "{extension}");
        assert!(decoded.channels > 0, "{extension}");
        assert!(decoded.interleaved.len() >= 12_000, "{extension}");
        assert!(
            (0.25..=1.0).contains(&decoded.source_duration.as_secs_f64()),
            "{extension}"
        );
        assert!(!decoded.truncated, "{extension}");
    }
}

#[test]
fn corrupt_audio_reports_a_decode_error() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("corrupt.wav");
    assert!(decode_file(&path).is_err());
}
