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
