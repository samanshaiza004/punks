//! Library-wide metadata health checks: surfaces problems in a library's
//! embedded metadata by comparing the same [`Metadata`] model + capability
//! gating `punks-app` already bridges to `punks_audio::metadata`
//! against the facts/overrides `punks-library` already stores. No new
//! storage, no new schema — this is a read-only diagnostic over data both
//! crates already own.
//!
//! [`check_asset`] compares plain values without I/O or database access.
//! [`scan_assets`] is the impure boundary: it does the actual per-file
//! metadata reads and unpacks `Fact`s into the plain values `check_asset`
//! compares. `SampleBrowser::check_library_health` runs `scan_assets` on a
//! background thread — see its doc comment for why.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use punks_audio::{Backend, Capability, Field, Metadata, MetadataBackend};
use punks_library::Fact;

/// What's wrong with one (asset, field) pair. Only *disagreement* is a
/// problem — an absent optional field isn't (see [`check_asset`]).
#[derive(Debug, Clone, PartialEq)]
pub enum HealthIssueKind {
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

/// The fields the health check drift-covers. `Keywords` isn't included — it's
/// a `Vec` with no scalar `Fact` to cache/override against. Extending this
/// list needs no other change: [`check_asset`] skips any field a backend
/// reports [`Capability::Unsupported`] for, so a field only checks where the
/// format can actually read it.
const CHECKED_FIELDS: [Field; 3] = [Field::Description, Field::Category, Field::Creator];

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
/// two entries, so a linear scan beats a `HashMap` here). This check uses
/// only the supplied values and performs no I/O or database access.
///
/// A field is skipped entirely when `backend.capability(field)` is
/// [`Capability::Unsupported`]. The backend cannot read that field, so
/// reporting it as missing would be misleading.
///
/// Drift (`CacheDrift`/`OverrideDrift`) only fires when both sides have a
/// value and they disagree; an absent embedded field is skipped, not
/// reported — an optional field being empty is a choice, not a defect.
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
        // An absent optional field is not a defect — only *disagreement*
        // between the file and a stored copy is. So no "missing" report; just
        // drift, which needs a value on both sides to exist.
        let Some(e) = embedded_value(embedded, field) else {
            continue;
        };
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
    issues
}

/// Walk `assets`, reading each file's embedded metadata fresh and comparing
/// it against the `cached` embedded values and `overrides` snapshots taken at
/// the moment the check was triggered (so this never touches the live
/// `LibraryContext` caches the UI thread owns). Does the actual per-file I/O —
/// meant to run on a background thread; see
/// `SampleBrowser::check_library_health`. A file that fails to read is logged
/// and skipped, not reported as an issue — this checks metadata *content*,
/// not file health.
pub fn scan_assets(
    assets: &[PathBuf],
    cached: &HashMap<PathBuf, Vec<(Field, String)>>,
    overrides: &HashMap<PathBuf, HashMap<String, Option<Fact>>>,
) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    let empty = Vec::new();
    for path in assets {
        let backend = Backend::for_path(path);
        let embedded = match backend.read(path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("health check: read {path:?}: {e}");
                continue;
            }
        };

        let cached = cached.get(path).unwrap_or(&empty);

        let mut overridden = Vec::new();
        if let Some(m) = overrides.get(path) {
            for field in CHECKED_FIELDS {
                if let Some(Some(Fact::Text(s))) = m.get(metric_name(field)) {
                    overridden.push((field, s.clone()));
                }
            }
        }

        issues.extend(check_asset(path, &backend, &embedded, cached, &overridden));
    }
    issues
}
