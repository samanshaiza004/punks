//! Portable metadata: the small, format-neutral subset of embedded metadata
//! Punks round-trips, plus the backends that map it to/from real files.
//!
//! The model type ([`Metadata`]) names *the information* — description, keywords,
//! category, creator — independent of where it came from, so it can later carry
//! values from embedded tags, user overrides, analyzers, or project files alike.
//! A [`MetadataBackend`] names the *storage*: WAV/BWF uses our own atomic writer
//! (precise control over irreplaceable field recordings); everything else uses
//! `lofty`.
//!
//! **Invariant — Punks never discards metadata it doesn't understand.** Every
//! backend `write` is read-modify-write: it touches only the fields it maps and
//! that the caller set, and preserves every other chunk / tag / picture in the
//! file byte-for-byte. Losing a producer's iXML or a cover picture on a
//! description edit is the kind of regression professionals never forgive.

use std::fmt;
use std::path::Path;

use lofty::config::WriteOptions;
use lofty::prelude::*;
use lofty::tag::Tag;

use crate::{decode, PlaybackError};

/// The portable metadata model. Origin-agnostic on purpose (see the module doc):
/// today it's populated from embedded tags, tomorrow it can front overrides /
/// analysis facts / project metadata without a rename. Keywords/category/creator
/// exist in the model now but aren't written by any backend yet (next pass).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Metadata {
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub category: Option<String>,
    pub creator: Option<String>,
}

/// Where a resolved metadata value came from. Aligns with punks-library's
/// facts + fact_overrides layering:
///   - `fact_overrides` rows            -> [`Override`](Self::Override)
///   - `audio_analysis` facts           -> [`Analysis`](Self::Analysis), except
///     a fact that merely mirrors a container tag (e.g. the cached embedded
///     Description) which is really [`Embedded`](Self::Embedded)
///   - a tag read straight off the file -> [`Embedded`](Self::Embedded)
///   - the future `asset_metadata` layer (notes/pack/author/license) ->
///     [`Project`](Self::Project)
///
/// Carried per field so the UI can label every value with its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataSource {
    /// The file's own container tags (BWF bext, ID3, Vorbis comments, …).
    Embedded,
    /// A user correction that patches a detected value (`fact_overrides`).
    Override,
    /// Produced by an analyzer (`audio_analysis`).
    Analysis,
    /// Independently-authored project metadata (the future `asset_metadata`).
    Project,
}

impl MetadataSource {
    /// Resolution rank when several sources supply the same field: highest
    /// wins. Strongest first — Override > Project > Embedded > Analysis: a user
    /// correction always wins; deliberately-authored project metadata beats the
    /// file's own tags; both beat a mere analyzer guess. Generalises
    /// punks-library's `override ?? analysis` to four layers. Change here if
    /// product intent differs.
    fn rank(self) -> u8 {
        match self {
            MetadataSource::Override => 3,
            MetadataSource::Project => 2,
            MetadataSource::Embedded => 1,
            MetadataSource::Analysis => 0,
        }
    }
}

/// A value tagged with its provenance — the unit the UI renders per field.
#[derive(Debug, Clone, PartialEq)]
pub struct Sourced<T> {
    pub value: T,
    pub source: MetadataSource,
}

/// [`Metadata`] resolved across sources: each present field carries the
/// [`MetadataSource`] that supplied it. Built by [`resolve`]. With only the
/// Embedded backend wired today a single read resolves to all-Embedded; the
/// override / analysis / project layers slot in through [`resolve`] as their
/// backends land.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ResolvedMetadata {
    pub description: Option<Sourced<String>>,
    pub keywords: Option<Sourced<Vec<String>>>,
    pub category: Option<Sourced<String>>,
    pub creator: Option<Sourced<String>>,
}

/// The winning scalar for one field across `layers`: the value from the
/// highest-[`rank`](MetadataSource::rank)ed source that actually supplies it.
fn best(
    layers: &[(MetadataSource, Metadata)],
    get: impl Fn(&Metadata) -> Option<&str>,
) -> Option<Sourced<String>> {
    layers
        .iter()
        .filter_map(|(src, m)| get(m).map(|v| (*src, v)))
        .max_by_key(|(src, _)| src.rank())
        .map(|(source, value)| Sourced {
            value: value.to_string(),
            source,
        })
}

/// Merge per-source metadata into one resolved view: for each field, the value
/// from the highest-ranked source that supplies it wins and carries that
/// source. `layers` may be in any order; a `None` scalar or empty keyword list
/// doesn't count as supplying the field.
///
/// ponytail: keywords resolve whole-set from the winning source rather than
/// unioning across sources — provenance stays unambiguous (one source per shown
/// value) at the cost of not blending, say, embedded + project keyword sets.
/// Upgrade to per-keyword provenance if blended sets are ever needed.
pub fn resolve(layers: &[(MetadataSource, Metadata)]) -> ResolvedMetadata {
    ResolvedMetadata {
        description: best(layers, |m| m.description.as_deref()),
        category: best(layers, |m| m.category.as_deref()),
        creator: best(layers, |m| m.creator.as_deref()),
        keywords: layers
            .iter()
            .filter(|(_, m)| !m.keywords.is_empty())
            .max_by_key(|(src, _)| src.rank())
            .map(|(source, m)| Sourced {
                value: m.keywords.clone(),
                source: *source,
            }),
    }
}

/// One logical field, for per-field capability queries and partial writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Description,
    Keywords,
    Category,
    Creator,
}

/// What a backend can do with one [`Field`]. Per-field (rather than a global
/// read/write pair) so formats can diverge cleanly — e.g. `Description` RW while
/// `Creator` is RO or a future `Timecode` is RO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Unsupported,
    ReadOnly,
    ReadWrite,
}

impl Capability {
    pub fn can_read(self) -> bool {
        matches!(self, Capability::ReadOnly | Capability::ReadWrite)
    }
    pub fn can_write(self) -> bool {
        matches!(self, Capability::ReadWrite)
    }
}

#[derive(Debug)]
pub enum MetadataError {
    Io(String),
    Unsupported(String),
}

impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataError::Io(e) => write!(f, "metadata io error: {e}"),
            MetadataError::Unsupported(e) => write!(f, "unsupported metadata operation: {e}"),
        }
    }
}

impl std::error::Error for MetadataError {}

impl From<PlaybackError> for MetadataError {
    fn from(e: PlaybackError) -> Self {
        MetadataError::Io(e.to_string())
    }
}

impl From<lofty::error::LoftyError> for MetadataError {
    fn from(e: lofty::error::LoftyError) -> Self {
        MetadataError::Io(e.to_string())
    }
}

/// A metadata storage backend for one family of file formats.
pub trait MetadataBackend {
    /// What this backend can do with `field` on files it handles.
    fn capability(&self, field: Field) -> Capability;
    /// Read the portable metadata this backend understands (unmapped fields
    /// come back empty).
    fn read(&self, path: &Path) -> Result<Metadata, MetadataError>;
    /// Read-modify-write: writes only fields this backend `can_write` AND that
    /// are set (`Some`/non-empty) in `m`; leaves unmapped fields — and every
    /// tag/chunk/picture Punks doesn't model — untouched.
    fn write(&self, path: &Path, m: &Metadata) -> Result<(), MetadataError>;

    /// The provenance every value this backend reads carries. File-format
    /// backends read the file's own container tags, so the default is
    /// [`MetadataSource::Embedded`]; library-backed backends (overrides /
    /// analysis facts / project metadata) override this.
    fn source(&self) -> MetadataSource {
        MetadataSource::Embedded
    }

    /// [`read`](Self::read) tagged with this backend's [`source`](Self::source)
    /// as a resolved provenance view — the single-source case. Multi-source
    /// merges (embedded + override + analysis + project) go through [`resolve`].
    fn read_resolved(&self, path: &Path) -> Result<ResolvedMetadata, MetadataError> {
        Ok(resolve(&[(self.source(), self.read(path)?)]))
    }
}

/// WAV/BWF via our own atomic bext writer. RF64 (>4 GB) is read-only — its
/// structural rules (`ds64`, 64-bit sizes) differ enough that refusing to write
/// is far safer than a partial implementation.
pub struct WaveBackend {
    /// Plain writable RIFF/WAVE (not RF64, not the Ogg-in-WAV oddball).
    writable: bool,
}

impl WaveBackend {
    fn new(path: &Path) -> Self {
        WaveBackend {
            writable: decode::can_write_bext(path),
        }
    }
}

impl MetadataBackend for WaveBackend {
    fn capability(&self, field: Field) -> Capability {
        match field {
            Field::Description => {
                if self.writable {
                    Capability::ReadWrite
                } else {
                    Capability::ReadOnly // e.g. RF64: bext is readable, not writable
                }
            }
            // Keywords/Category/Creator (RIFF INFO / iXML) land next pass.
            _ => Capability::Unsupported,
        }
    }

    fn read(&self, path: &Path) -> Result<Metadata, MetadataError> {
        let prefix = decode::read_header_prefix(path, decode::HEADER_PREFIX_MAX)?;
        let am = decode::parse_riff_metadata(&prefix);
        Ok(Metadata {
            description: am.description,
            ..Default::default()
        })
    }

    fn write(&self, path: &Path, m: &Metadata) -> Result<(), MetadataError> {
        if let Some(desc) = &m.description {
            // The bext splice preserves every other chunk (fmt, data, cue, …)
            // and every other bext field byte-for-byte.
            decode::write_bext_description(path, desc)?;
        }
        Ok(())
    }
}

/// Everything WAV isn't (FLAC, MP3, OGG, AIFF, …) via `lofty`, which reads and
/// writes existing tag structures rather than rewriting wholesale — so unrelated
/// items and pictures survive an edit.
pub struct LoftyBackend;

impl MetadataBackend for LoftyBackend {
    fn capability(&self, field: Field) -> Capability {
        match field {
            Field::Description => Capability::ReadWrite,
            _ => Capability::Unsupported,
        }
    }

    fn read(&self, path: &Path) -> Result<Metadata, MetadataError> {
        let tagged = lofty::read_from_path(path)?;
        let description = tagged.primary_tag().and_then(|t| {
            t.get_string(ItemKey::Description)
                .or_else(|| t.get_string(ItemKey::Comment))
                .map(str::to_string)
        });
        Ok(Metadata {
            description,
            ..Default::default()
        })
    }

    fn write(&self, path: &Path, m: &Metadata) -> Result<(), MetadataError> {
        let Some(desc) = m.description.clone() else {
            return Ok(());
        };
        // Edit a temp copy of the original, then atomically rename it into place
        // — lofty edits a file in-place, so `write_atomically` gives it the
        // crash-safety it doesn't provide on its own. The temp keeps the
        // original extension so lofty probes the same format.
        decode::write_atomically(path, |tmp| {
            std::fs::copy(path, tmp)
                .map_err(|e| PlaybackError::DecodeError(format!("{tmp:?}: {e}")))?;
            let mut tagged = lofty::read_from_path(tmp)
                .map_err(|e| PlaybackError::DecodeError(e.to_string()))?;
            if tagged.primary_tag_mut().is_none() {
                let tag_type = tagged.primary_tag_type();
                tagged.insert_tag(Tag::new(tag_type));
            }
            if let Some(tag) = tagged.primary_tag_mut() {
                tag.insert_text(ItemKey::Description, desc.clone());
            }
            tagged
                .save_to_path(tmp, WriteOptions::default())
                .map_err(|e| PlaybackError::DecodeError(e.to_string()))?;
            Ok(())
        })?;
        Ok(())
    }
}

/// The storage backend chosen for a path. An enum (not `dyn`) so the two — soon
/// more — backends stay first-class and cheap to dispatch.
pub enum Backend {
    Wave(WaveBackend),
    Lofty(LoftyBackend),
}

impl Backend {
    /// Pick the backend by extension (cheap — no file read for the routing
    /// decision; the WAV backend then reads the header once to tell RIFF from
    /// RF64, matching prior behaviour).
    pub fn for_path(path: &Path) -> Backend {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "wav" | "wave" | "bwf" => Backend::Wave(WaveBackend::new(path)),
            _ => Backend::Lofty(LoftyBackend),
        }
    }
}

impl MetadataBackend for Backend {
    fn capability(&self, field: Field) -> Capability {
        match self {
            Backend::Wave(b) => b.capability(field),
            Backend::Lofty(b) => b.capability(field),
        }
    }
    fn read(&self, path: &Path) -> Result<Metadata, MetadataError> {
        match self {
            Backend::Wave(b) => b.read(path),
            Backend::Lofty(b) => b.read(path),
        }
    }
    fn write(&self, path: &Path, m: &Metadata) -> Result<(), MetadataError> {
        match self {
            Backend::Wave(b) => b.write(path, m),
            Backend::Lofty(b) => b.write(path, m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(
        description: Option<&str>,
        keywords: &[&str],
        category: Option<&str>,
        creator: Option<&str>,
    ) -> Metadata {
        Metadata {
            description: description.map(str::to_string),
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            category: category.map(str::to_string),
            creator: creator.map(str::to_string),
        }
    }

    #[test]
    fn override_outranks_embedded_outranks_analysis() {
        let r = resolve(&[
            (MetadataSource::Analysis, meta(Some("guess"), &[], None, None)),
            (MetadataSource::Embedded, meta(Some("tag"), &[], None, None)),
            (MetadataSource::Override, meta(Some("fixed"), &[], None, None)),
        ]);
        let d = r.description.expect("description resolved");
        assert_eq!(d.value, "fixed");
        assert_eq!(d.source, MetadataSource::Override);
    }

    #[test]
    fn absent_high_rank_value_does_not_shadow_lower_source() {
        // Override supplies no description, so Embedded's stands; the override's
        // category is still picked up for its own field.
        let r = resolve(&[
            (MetadataSource::Embedded, meta(Some("tag"), &[], None, None)),
            (MetadataSource::Override, meta(None, &[], Some("sfx"), None)),
        ]);
        let d = r.description.expect("description resolved");
        assert_eq!((d.value.as_str(), d.source), ("tag", MetadataSource::Embedded));
        assert_eq!(r.category.expect("category resolved").source, MetadataSource::Override);
    }

    #[test]
    fn project_outranks_embedded() {
        let r = resolve(&[
            (MetadataSource::Embedded, meta(None, &[], None, Some("file tag"))),
            (MetadataSource::Project, meta(None, &[], None, Some("pack author"))),
        ]);
        let c = r.creator.expect("creator resolved");
        assert_eq!((c.value.as_str(), c.source), ("pack author", MetadataSource::Project));
    }

    #[test]
    fn keywords_resolve_whole_set_from_highest_source() {
        let r = resolve(&[
            (MetadataSource::Embedded, meta(None, &["a", "b"], None, None)),
            (MetadataSource::Override, meta(None, &["c"], None, None)),
        ]);
        let k = r.keywords.expect("keywords resolved");
        assert_eq!(k.value, vec!["c".to_string()]);
        assert_eq!(k.source, MetadataSource::Override);
    }

    #[test]
    fn empty_layers_resolve_to_default() {
        assert_eq!(resolve(&[]), ResolvedMetadata::default());
    }

    struct FixedBackend(Metadata, MetadataSource);
    impl MetadataBackend for FixedBackend {
        fn capability(&self, _f: Field) -> Capability {
            Capability::ReadOnly
        }
        fn read(&self, _p: &Path) -> Result<Metadata, MetadataError> {
            Ok(self.0.clone())
        }
        fn write(&self, _p: &Path, _m: &Metadata) -> Result<(), MetadataError> {
            Ok(())
        }
        fn source(&self) -> MetadataSource {
            self.1
        }
    }

    #[test]
    fn read_resolved_tags_every_field_with_backend_source() {
        let be = FixedBackend(meta(Some("hi"), &[], None, None), MetadataSource::Override);
        let r = be.read_resolved(Path::new("unused")).expect("read");
        assert_eq!(r.description.expect("description").source, MetadataSource::Override);
    }

    #[test]
    fn file_backends_default_to_embedded_source() {
        // WaveBackend/LoftyBackend rely on the trait default.
        assert_eq!(LoftyBackend.source(), MetadataSource::Embedded);
    }
}
