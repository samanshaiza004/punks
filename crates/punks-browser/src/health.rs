//! Library-wide metadata health checks: surfaces problems in a library's
//! embedded metadata by comparing the same [`Metadata`] model + capability
//! gating `punks-browser` already bridges to `punks_playback::metadata`
//! against the facts/overrides `punks-library` already stores. No new
//! storage, no new schema — this is a read-only diagnostic over data both
//! crates already own.
//!
//! [`check_asset`] is pure (plain strings in, no I/O, no SQLite, no
//! `punks_library::Fact`) so it's testable without a live library or file.
//! [`scan_assets`] is the impure boundary: it does the actual per-file
//! metadata reads and unpacks `Fact`s into the plain values `check_asset`
//! compares. `SampleBrowser::check_library_health` runs `scan_assets` on a
//! background thread — see its doc comment for why.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use punks_library::Fact;
use punks_playback::{Backend, Capability, Field, Metadata, MetadataBackend};

/// What's wrong with one (asset, field) pair.
#[derive(Debug, Clone, PartialEq)]
pub enum HealthIssueKind {
    /// The field has no embedded value.
    Missing,
    /// The file's current embedded value disagrees with the library's
    /// cached copy (`audio_analysis`) — most likely edited by something
    /// other than Punks since the cache last synced.
    CacheDrift { embedded: String, cached: String },
    /// The file's current embedded value disagrees with a user override
    /// (`fact_overrides`) recorded for the same field.
    OverrideDrift {
        embedded: String,
        overridden: String,
    },
}

/// One detected problem with a single asset's metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthIssue {
    pub path: PathBuf,
    pub field: Field,
    pub kind: HealthIssueKind,
}

/// The fields the health check covers today. Extending this list needs no
/// other change here: [`check_asset`] already skips any field a backend
/// reports [`Capability::Unsupported`] for, so a field "activates" the
/// moment a backend gains read support for it — Keywords isn't included
/// because it has no scalar representation to cache/override as a `Fact`
/// (see `metric_name`'s doc comment).
const CHECKED_FIELDS: [Field; 2] = [Field::Description, Field::Category];

/// The metric name a [`Field`] would be stored/overridden under in
/// punks-library's generic `audio_analysis` / `fact_overrides` tables —
/// the same string vocabulary `resolve_fact` uses for instrument/key/bpm,
/// extended here to embedded-metadata fields. `Keywords`/`Creator` aren't
/// wired to a caller yet (no fact or override is ever stored under these
/// names today, so those two fields' checks are structurally dormant
/// until one lands) but are named here so adding one is a one-line change,
/// not a new match arm elsewhere.
fn metric_name(field: Field) -> &'static str {
    match field {
        Field::Description => "description",
        Field::Category => "category",
        Field::Creator => "creator",
        Field::Keywords => "keywords",
    }
}

/// `field`'s current text value out of a freshly-read [`Metadata`].
fn embedded_value(meta: &Metadata, field: Field) -> Option<&str> {
    match field {
        Field::Description => meta.description.as_deref(),
        Field::Category => meta.category.as_deref(),
        Field::Creator => meta.creator.as_deref(),
        Field::Keywords => None,
    }
}

fn lookup(values: &[(Field, String)], field: Field) -> Option<&str> {
    values
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, v)| v.as_str())
}

/// Check one asset's [`CHECKED_FIELDS`] against its already-read embedded
/// [`Metadata`] plus the library's cached/override values for that path.
/// `cached` and `overrides` are `(field, text)` pairs already unpacked from
/// punks-library's `Fact`s by the caller (there's usually at most one or
/// two entries, so a linear scan beats a `HashMap` here). Pure: no I/O, no
/// SQLite, so it's directly unit-testable.
///
/// A field is skipped entirely when `backend.capability(field)` is
/// [`Capability::Unsupported`] — there's nothing embedded to compare
/// against, and flagging every file as "missing" a field no backend can
/// even read would be a false signal, not a real one (see CLAUDE.md's
/// "capability, not format checks").
///
/// Drift (`CacheDrift`/`OverrideDrift`) only fires when both sides have a
/// value and they disagree; an embedded value that went missing entirely
/// is reported as [`HealthIssueKind::Missing`], not drift.
pub fn check_asset(
    path: &Path,
    backend: &Backend,
    embedded: &Metadata,
    cached: &[(Field, String)],
    overrides: &[(Field, String)],
) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    for field in CHECKED_FIELDS {
        if backend.capability(field) == Capability::Unsupported {
            continue;
        }
        let issue = |kind: HealthIssueKind| HealthIssue {
            path: path.to_path_buf(),
            field,
            kind,
        };
        match embedded_value(embedded, field) {
            None => issues.push(issue(HealthIssueKind::Missing)),
            Some(e) => {
                if let Some(c) = lookup(cached, field) {
                    if c != e {
                        issues.push(issue(HealthIssueKind::CacheDrift {
                            embedded: e.to_string(),
                            cached: c.to_string(),
                        }));
                    }
                }
                if let Some(o) = lookup(overrides, field) {
                    if o != e {
                        issues.push(issue(HealthIssueKind::OverrideDrift {
                            embedded: e.to_string(),
                            overridden: o.to_string(),
                        }));
                    }
                }
            }
        }
    }
    issues
}

/// Walk `assets`, reading each file's embedded metadata fresh and comparing
/// it against `descriptions`/`overrides` snapshots taken at the moment the
/// check was triggered (so this never touches the live `LibraryContext`
/// caches the UI thread owns). Does the actual per-file I/O — meant to run
/// on a background thread; see `SampleBrowser::check_library_health`. A
/// file that fails to read is logged and skipped, not reported as an
/// issue — this checks metadata *content*, not file health.
pub fn scan_assets(
    assets: &[PathBuf],
    descriptions: &HashMap<PathBuf, String>,
    overrides: &HashMap<PathBuf, HashMap<String, Option<Fact>>>,
) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    for path in assets {
        let backend = Backend::for_path(path);
        let embedded = match backend.read(path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("health check: read {path:?}: {e}");
                continue;
            }
        };

        let mut cached = Vec::new();
        if let Some(d) = descriptions.get(path) {
            cached.push((Field::Description, d.clone()));
        }

        let mut overridden = Vec::new();
        if let Some(m) = overrides.get(path) {
            for field in CHECKED_FIELDS {
                if let Some(Some(Fact::Text(s))) = m.get(metric_name(field)) {
                    overridden.push((field, s.clone()));
                }
            }
        }

        issues.extend(check_asset(path, &backend, &embedded, &cached, &overridden));
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(description: Option<&str>, category: Option<&str>) -> Metadata {
        Metadata {
            description: description.map(str::to_string),
            category: category.map(str::to_string),
            ..Default::default()
        }
    }

    fn wav_backend() -> Backend {
        // A .wav path routes to WaveBackend, which supports Description
        // (ReadWrite) but not Category (Unsupported) — exactly the
        // capability split these tests exercise.
        Backend::for_path(Path::new("x.wav"))
    }

    #[test]
    fn missing_description_is_flagged() {
        let issues = check_asset(
            Path::new("kick.wav"),
            &wav_backend(),
            &meta(None, None),
            &[],
            &[],
        );
        assert_eq!(
            issues,
            vec![HealthIssue {
                path: PathBuf::from("kick.wav"),
                field: Field::Description,
                kind: HealthIssueKind::Missing,
            }]
        );
    }

    #[test]
    fn category_is_skipped_when_backend_does_not_support_it() {
        // Category is Capability::Unsupported on the WAV backend today, so
        // a file with no embedded category must NOT be flagged "missing" —
        // that would be a false signal (see check_asset's doc comment).
        let issues = check_asset(
            Path::new("kick.wav"),
            &wav_backend(),
            &meta(Some("desc"), None),
            &[],
            &[],
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn present_description_matching_cache_is_not_flagged() {
        let issues = check_asset(
            Path::new("kick.wav"),
            &wav_backend(),
            &meta(Some("hi"), None),
            &[(Field::Description, "hi".to_string())],
            &[],
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn embedded_description_differing_from_cache_is_flagged() {
        let issues = check_asset(
            Path::new("kick.wav"),
            &wav_backend(),
            &meta(Some("new text"), None),
            &[(Field::Description, "old text".to_string())],
            &[],
        );
        assert_eq!(
            issues,
            vec![HealthIssue {
                path: PathBuf::from("kick.wav"),
                field: Field::Description,
                kind: HealthIssueKind::CacheDrift {
                    embedded: "new text".to_string(),
                    cached: "old text".to_string(),
                },
            }]
        );
    }

    #[test]
    fn embedded_description_differing_from_override_is_flagged() {
        let issues = check_asset(
            Path::new("kick.wav"),
            &wav_backend(),
            &meta(Some("file says X"), None),
            &[],
            &[(Field::Description, "user says Y".to_string())],
        );
        assert_eq!(
            issues,
            vec![HealthIssue {
                path: PathBuf::from("kick.wav"),
                field: Field::Description,
                kind: HealthIssueKind::OverrideDrift {
                    embedded: "file says X".to_string(),
                    overridden: "user says Y".to_string(),
                },
            }]
        );
    }

    #[test]
    fn both_cache_and_override_drift_can_fire_together() {
        let issues = check_asset(
            Path::new("kick.wav"),
            &wav_backend(),
            &meta(Some("embedded"), None),
            &[(Field::Description, "cached".to_string())],
            &[(Field::Description, "overridden".to_string())],
        );
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .any(|i| matches!(i.kind, HealthIssueKind::CacheDrift { .. })));
        assert!(issues
            .iter()
            .any(|i| matches!(i.kind, HealthIssueKind::OverrideDrift { .. })));
    }

    #[test]
    fn scan_assets_skips_a_file_that_fails_to_read() {
        // No such file: Backend::read errors; scan_assets must log and
        // continue rather than panicking or aborting the whole scan.
        let issues = scan_assets(
            &[PathBuf::from("/nonexistent/definitely-missing.wav")],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(issues.is_empty());
    }

    /// Write a minimal valid RIFF/WAVE file — no real audio, just enough
    /// structure for `WaveBackend` to accept it as writable.
    fn write_minimal_wav(path: &Path) {
        let data: [u8; 4] = [0, 0, 0, 0];
        let mut f = std::fs::File::create(path).unwrap();
        use std::io::Write;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data.len() as u32).to_le_bytes())
            .unwrap();
        f.write_all(b"WAVE").unwrap();
        f.write_all(b"fmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        f.write_all(&8000u32.to_le_bytes()).unwrap(); // sample rate
        f.write_all(&16000u32.to_le_bytes()).unwrap(); // byte rate
        f.write_all(&2u16.to_le_bytes()).unwrap(); // block align
        f.write_all(&16u16.to_le_bytes()).unwrap(); // bits/sample
        f.write_all(b"data").unwrap();
        f.write_all(&(data.len() as u32).to_le_bytes()).unwrap();
        f.write_all(&data).unwrap();
    }

    /// End-to-end proof that `scan_assets` finds real drift, not just what
    /// `check_asset`'s pure unit tests can construct by hand: a real WAV's
    /// embedded bext Description is written through the real
    /// `MetadataBackend`, a real `punks_library::Library` caches a
    /// *different* description (simulating the cache falling behind an
    /// external edit), and the scan — reading the real file, querying the
    /// real DB via `Library::all_facts`/`all_overrides` — must report the
    /// exact drift.
    #[test]
    fn scan_assets_detects_real_cache_drift_end_to_end() {
        let dir = std::env::temp_dir().join(format!(
            "punks_health_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("field_recording.wav");
        write_minimal_wav(&wav);

        let backend = Backend::for_path(&wav);
        assert!(backend.capability(Field::Description).can_write());
        backend
            .write(
                &wav,
                &Metadata {
                    description: Some("current take".into()),
                    ..Default::default()
                },
            )
            .unwrap();

        let lib = punks_library::Library::create(&dir).unwrap();
        // Cache says "old take" — the file has since been edited to "current
        // take" by something other than the library's own write-through path.
        lib.set_description(&wav, "old take").unwrap();

        let mut descriptions = HashMap::new();
        descriptions.insert(wav.clone(), "old take".to_string());

        let issues = scan_assets(std::slice::from_ref(&wav), &descriptions, &HashMap::new());

        assert_eq!(
            issues,
            vec![HealthIssue {
                path: wav.clone(),
                field: Field::Description,
                kind: HealthIssueKind::CacheDrift {
                    embedded: "current take".to_string(),
                    cached: "old take".to_string(),
                },
            }]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
