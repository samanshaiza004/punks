use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::peaks::WaveformPeaks;
use crate::PlaybackError;

/// Free-text metadata read from a file's container, kept domain-neutral so it
/// serves music and audio-for-picture equally. Currently populated from the
/// Broadcast Wave `bext` chunk; all fields are optional.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AudioMetadata {
    pub description: Option<String>,
    pub originator: Option<String>,
    pub origination_date: Option<String>,
    pub origination_time: Option<String>,
    /// bext TimeReference: sample count from midnight / media start — the file's
    /// start timecode, in the source sample rate.
    pub time_reference: Option<u64>,
}

pub struct DecodedAudio {
    pub interleaved: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
    pub metadata: AudioMetadata,
    /// True total duration of the source, even when only a preview was decoded.
    pub source_duration: Duration,
    /// Where in the source this buffer begins. 0 for a from-the-start preview;
    /// non-zero for an on-demand window decoded by [`decode_window`].
    pub window_start: Duration,
    /// True if only a bounded window of a longer source was decoded.
    pub truncated: bool,
}

/// WAVE format tag for Ogg Vorbis wrapped in a RIFF/WAVE container (the bytes
/// spell "Og"). Produced by the Vorbis ACM codec and some older sample-pack
/// tooling. symphonia's RIFF reader has no codec mapping for it and bails, but
/// the `data` chunk is a complete, standalone Ogg stream we can decode directly.
const WAVE_FORMAT_OGG_VORBIS: u16 = 0x674f;

/// Files longer than this are decoded as a bounded preview instead of loaded
/// whole. `PREVIEW_WINDOW` is how much of the start we decode. Tune for the
/// memory/usefulness trade-off (a 120 s stereo/48k window is ~92 MB).
const PREVIEW_THRESHOLD: Duration = Duration::from_secs(120);
const PREVIEW_WINDOW: Duration = Duration::from_secs(120);

/// How many header bytes to read for classification + metadata. All chunks
/// before `data` (fmt, ds64, bext, …) live comfortably within this.
pub(crate) const HEADER_PREFIX_MAX: usize = 1 << 20; // 1 MiB

pub fn decode_file(path: &Path) -> Result<DecodedAudio, PlaybackError> {
    decode_inner(path, PREVIEW_THRESHOLD, PREVIEW_WINDOW)
}

fn decode_inner(
    path: &Path,
    threshold: Duration,
    window: Duration,
) -> Result<DecodedAudio, PlaybackError> {
    // Read a bounded header prefix for classification + metadata rather than the
    // whole file — long production-sound files must not be slurped into memory.
    let prefix = read_header_prefix(path, HEADER_PREFIX_MAX)?;
    let metadata = parse_riff_metadata(&prefix);

    // RF64: the >4 GB WAV variant. symphonia only knows `RIFF`, so fix it up.
    if prefix.len() >= 12 && &prefix[0..4] == b"RF64" && &prefix[8..12] == b"WAVE" {
        return decode_rf64(path, &prefix, metadata, threshold, window);
    }

    // Ogg Vorbis in a WAV container: read fully and hand the inner Ogg stream to
    // symphonia. These are small sample files, so the full read is fine.
    if riff_fmt_tag(&prefix) == Some(WAVE_FORMAT_OGG_VORBIS) {
        let raw = std::fs::read(path)
            .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
        if let Some(ogg) = extract_ogg_in_wav(&raw) {
            let mss = MediaSourceStream::new(Box::new(Cursor::new(ogg)), Default::default());
            let mut hint = Hint::new();
            hint.with_extension("ogg");
            return decode_from_stream(mss, &hint, metadata, None, threshold, window);
        }
    }

    // Normal path: stream from the file so long files aren't fully buffered.
    let file =
        File::open(path).map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    decode_from_stream(mss, &hint, metadata, None, threshold, window)
}

/// Read up to `max` bytes from the start of `path`.
pub(crate) fn read_header_prefix(path: &Path, max: usize) -> Result<Vec<u8>, PlaybackError> {
    let mut file =
        File::open(path).map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
    let mut buf = vec![0u8; max];
    let mut filled = 0;
    while filled < max {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(PlaybackError::DecodeError(format!("{path:?}: {e}"))),
        }
    }
    buf.truncate(filled);
    Ok(buf)
}

/// True if `prefix` begins a RIFF/WAVE or RF64/WAVE container.
fn is_wave_prefix(prefix: &[u8]) -> bool {
    prefix.len() >= 12
        && &prefix[8..12] == b"WAVE"
        && (&prefix[0..4] == b"RIFF" || &prefix[0..4] == b"RF64")
}

/// The `fmt ` chunk's format tag, if this is a WAVE/RF64 file.
fn riff_fmt_tag(prefix: &[u8]) -> Option<u16> {
    if !is_wave_prefix(prefix) {
        return None;
    }
    let mut pos = 12;
    while pos + 8 <= prefix.len() {
        let id = &prefix[pos..pos + 4];
        let size = u32::from_le_bytes(prefix[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 2 <= prefix.len() {
            return Some(u16::from_le_bytes([prefix[body], prefix[body + 1]]));
        }
        if id == b"data" || size == 0xFFFF_FFFF {
            break;
        }
        let next = body + size + (size & 1);
        if next <= pos || next > prefix.len() {
            break;
        }
        pos = next;
    }
    None
}

/// Walk RIFF/RF64 chunks in `prefix` for a `bext` chunk and parse it.
pub(crate) fn parse_riff_metadata(prefix: &[u8]) -> AudioMetadata {
    if !is_wave_prefix(prefix) {
        return AudioMetadata::default();
    }
    let mut pos = 12;
    while pos + 8 <= prefix.len() {
        let id = &prefix[pos..pos + 4];
        let size = u32::from_le_bytes(prefix[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        if id == b"bext" {
            let end = (body + size).min(prefix.len());
            return parse_bext(&prefix[body..end]);
        }
        // `data` (and its 0xFFFFFFFF RF64 sentinel) is the audio payload; any
        // metadata worth reading comes before it.
        if id == b"data" || size == 0xFFFF_FFFF {
            break;
        }
        let next = body + size + (size & 1);
        if next <= pos || next > prefix.len() {
            break;
        }
        pos = next;
    }
    AudioMetadata::default()
}

/// Parse a Broadcast Wave `bext` chunk body (fixed little-endian layout).
fn parse_bext(body: &[u8]) -> AudioMetadata {
    let text = |range: std::ops::Range<usize>| -> Option<String> {
        body.get(range).map(read_c_string).filter(|s| !s.is_empty())
    };
    // TimeReference is two 32-bit words (low then high) == a u64 LE at 338..346.
    let time_reference = body
        .get(338..346)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()));
    AudioMetadata {
        description: text(0..256),
        originator: text(256..288),
        origination_date: text(320..330),
        origination_time: text(330..338),
        time_reference,
    }
}

/// Read a NUL-terminated (or space-padded) fixed field as a trimmed string.
fn read_c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end])
        .trim_end()
        .to_string()
}

/// Minimum size of a BWF `bext` chunk body (the fixed fields; CodingHistory, if
/// any, follows and is preserved as opaque trailing bytes).
const BEXT_MIN_BODY_LEN: usize = 602;

/// Whether [`write_bext_description`] can safely rewrite `path`: plain
/// `RIFF`/`WAVE` only. RF64 (>4 GB) and Ogg-Vorbis-in-WAV are refused — both
/// need format-specific handling this crate doesn't have yet — as is anything
/// that isn't a WAVE container at all. The writer re-checks this itself; this
/// is what the UI polls to grey out Save.
pub(crate) fn can_write_bext(path: &Path) -> bool {
    let Ok(prefix) = read_header_prefix(path, HEADER_PREFIX_MAX) else {
        return false;
    };
    prefix.len() >= 12
        && &prefix[0..4] == b"RIFF" // not RF64
        && &prefix[8..12] == b"WAVE"
        && riff_fmt_tag(&prefix) != Some(WAVE_FORMAT_OGG_VORBIS)
}

/// Where a `bext` chunk goes: replace an existing one in place, or insert a
/// fresh one at a byte offset (right after `fmt `, or wherever the pre-`data`
/// walk stopped if `fmt ` was never found).
enum BextSlot {
    Replace {
        /// Chunk start (the `"bext"` id itself).
        start: usize,
        /// One past the chunk's last byte (body end + pad byte, if any).
        end: usize,
        body_len: usize,
    },
    InsertAt(usize),
}

/// Walk chunks exactly as [`parse_riff_metadata`] does, but return *where*
/// `bext` is (or should go) instead of parsing it. Only look before `data` —
/// metadata never lives after the audio payload.
fn locate_bext(prefix: &[u8]) -> BextSlot {
    let mut pos = 12;
    let mut after_fmt = None;
    while pos + 8 <= prefix.len() {
        let id = &prefix[pos..pos + 4];
        let size = u32::from_le_bytes(prefix[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        let next = body + size + (size & 1);
        if id == b"bext" {
            return BextSlot::Replace {
                start: pos,
                end: next.min(prefix.len()).max(body),
                body_len: size,
            };
        }
        if id == b"data" || size == 0xFFFF_FFFF {
            break;
        }
        if id == b"fmt " {
            after_fmt = Some(next);
        }
        if next <= pos || next > prefix.len() {
            break;
        }
        pos = next;
    }
    BextSlot::InsertAt(after_fmt.unwrap_or(pos))
}

/// Byte length of a BWF `bext` Description field (fixed by the format's
/// layout — bytes 0..256, minus the terminator). Shared by the writer (which
/// truncates to it) and [`metadata::WaveBackend::max_len`](crate::metadata)
/// (which reports it to callers) so the two can never drift apart.
pub(crate) const BEXT_DESCRIPTION_MAX: usize = 255;

/// Truncate `s` to at most `max_bytes` bytes, on a UTF-8 char boundary.
pub(crate) fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Write `description` into `path`'s BWF `bext` chunk (creating one if absent),
/// preserving every other field of an existing bext (TimeReference, Originator,
/// UMID, coding history, …) and every other chunk in the file byte-for-byte —
/// most importantly the `data` chunk, which is never touched. Field recordings
/// are irreplaceable, so this never edits `path` in place: it streams a full
/// copy to a temp file in the same directory (so the final rename is atomic on
/// the same filesystem) and only replaces the original once that copy is
/// complete and flushed.
pub(crate) fn write_bext_description(path: &Path, description: &str) -> Result<(), PlaybackError> {
    if !can_write_bext(path) {
        return Err(PlaybackError::DecodeError(format!(
            "{path:?}: embedded metadata write is only supported for plain WAV/BWF files"
        )));
    }
    let prefix = read_header_prefix(path, HEADER_PREFIX_MAX)?;
    let slot = locate_bext(&prefix);

    // Start from the existing body (so every other bext field survives), or a
    // fresh minimum-size body when there's nothing to preserve.
    let mut body = match slot {
        BextSlot::Replace {
            start, body_len, ..
        } => {
            let mut src = File::open(path)
                .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
            read_exact_at(&mut src, (start + 8) as u64, body_len, path)?
        }
        BextSlot::InsertAt(_) => vec![0u8; BEXT_MIN_BODY_LEN],
    };
    if body.len() < BEXT_MIN_BODY_LEN {
        body.resize(BEXT_MIN_BODY_LEN, 0);
    }
    let desc_bytes = truncate_utf8(description, BEXT_DESCRIPTION_MAX).as_bytes();
    body[0..256].fill(0);
    body[0..desc_bytes.len()].copy_from_slice(desc_bytes);

    let mut chunk = Vec::with_capacity(8 + body.len() + 1);
    chunk.extend_from_slice(b"bext");
    chunk.extend_from_slice(&(body.len() as u32).to_le_bytes());
    chunk.extend_from_slice(&body);
    if body.len() % 2 == 1 {
        chunk.push(0);
    }

    let (head_end, tail_start) = match slot {
        BextSlot::Replace { start, end, .. } => (start, end),
        BextSlot::InsertAt(at) => (at, at),
    };
    splice_chunk(path, head_end, tail_start, &chunk)
}

/// Atomically rewrite `path` as: bytes `[0, head_end)` verbatim, then
/// `chunk`, then bytes `[tail_start, EOF)` verbatim — with the RIFF size
/// header (bytes 4..8) patched to the new total. `[head_end, tail_start)` is
/// the region being replaced (empty when `head_end == tail_start`, i.e. an
/// insertion). The audio `data` chunk and every unknown chunk are copied
/// byte-for-byte; the whole thing goes to a temp file that atomically renames
/// over the original (see [`write_atomically`]). Shared by every WAV metadata
/// writer (bext, RIFF INFO, iXML mirror) so the splice is written and tested
/// once.
fn splice_chunk(
    path: &Path,
    head_end: usize,
    tail_start: usize,
    chunk: &[u8],
) -> Result<(), PlaybackError> {
    write_atomically(path, |tmp| {
        let mut src =
            File::open(path).map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
        let mut dst =
            File::create(tmp).map_err(|e| PlaybackError::DecodeError(format!("{tmp:?}: {e}")))?;
        copy_n(&mut src, &mut dst, head_end as u64, path)?;
        dst.write_all(chunk)
            .map_err(|e| PlaybackError::DecodeError(format!("{tmp:?}: {e}")))?;
        src.seek(SeekFrom::Start(tail_start as u64))
            .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
        let tail_len = std::io::copy(&mut src, &mut dst)
            .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;

        // Patch the RIFF size header (bytes 4..8: total file size - 8).
        let total_len = head_end as u64 + chunk.len() as u64 + tail_len;
        dst.seek(SeekFrom::Start(4))
            .map_err(|e| PlaybackError::DecodeError(format!("{tmp:?}: {e}")))?;
        dst.write_all(&((total_len - 8) as u32).to_le_bytes())
            .map_err(|e| PlaybackError::DecodeError(format!("{tmp:?}: {e}")))?;
        Ok(())
    })
}

// ---- RIFF INFO (a LIST/INFO chunk): Creator / Category / Keywords ----
//
// The RIFF INFO list holds free-text tags; punks2 maps three of them —
// IART (artist → Creator), IGNR (genre → Category), IKEY (keywords). Every
// other INFO subfield (INAM, ICMT, ICOP, …) is read-modify-write-preserved,
// as is every non-INFO chunk. These fourcc tags live here, inside
// punks-playback; nothing above `metadata::Backend` ever names them.

/// The RIFF INFO subfields punks2 maps to canonical [`crate::metadata`]
/// fields. `None` = the tag was absent or empty.
#[derive(Default)]
pub(crate) struct InfoFields {
    pub creator: Option<String>,
    pub category: Option<String>,
    pub keywords: Option<String>,
}

/// Locate the `LIST`/`INFO` chunk (or where a new one should go — right after
/// `bext` if present, else after `fmt `). Only looks before `data`.
enum InfoSlot {
    Replace {
        start: usize,
        end: usize,
        body_len: usize,
    },
    InsertAt(usize),
}

fn locate_info(prefix: &[u8]) -> InfoSlot {
    let mut pos = 12;
    let mut insert_at = None;
    while pos + 8 <= prefix.len() {
        let id = &prefix[pos..pos + 4];
        let size = u32::from_le_bytes(prefix[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        let next = body + size + (size & 1);
        if id == b"LIST" && prefix.get(body..body + 4) == Some(b"INFO") {
            return InfoSlot::Replace {
                start: pos,
                end: next.min(prefix.len()).max(body),
                body_len: size,
            };
        }
        if id == b"data" || size == 0xFFFF_FFFF {
            break;
        }
        // Prefer inserting a fresh INFO list right after bext (metadata
        // grouped together), else right after fmt.
        if id == b"bext" || (id == b"fmt " && insert_at.is_none()) {
            insert_at = Some(next);
        }
        if next <= pos || next > prefix.len() {
            break;
        }
        pos = next;
    }
    InfoSlot::InsertAt(insert_at.unwrap_or(pos))
}

/// Parse the mapped subfields out of a `LIST`/`INFO` body (the bytes after the
/// chunk's size, i.e. starting with the `"INFO"` form type).
fn parse_info_body(body: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut out = Vec::new();
    let mut pos = 4; // skip "INFO"
    while pos + 8 <= body.len() {
        let mut id = [0u8; 4];
        id.copy_from_slice(&body[pos..pos + 4]);
        let size = u32::from_le_bytes(body[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let sbody = pos + 8;
        let end = (sbody + size).min(body.len());
        out.push((id, body[sbody..end].to_vec()));
        let next = sbody + size + (size & 1);
        if next <= pos {
            break;
        }
        pos = next;
    }
    out
}

/// Read the mapped RIFF INFO subfields, if a LIST/INFO chunk is present.
pub(crate) fn parse_riff_info(prefix: &[u8]) -> InfoFields {
    if !is_wave_prefix(prefix) {
        return InfoFields::default();
    }
    let InfoSlot::Replace {
        start, body_len, ..
    } = locate_info(prefix)
    else {
        return InfoFields::default();
    };
    let body = &prefix[(start + 8)..(start + 8 + body_len).min(prefix.len())];
    let mut out = InfoFields::default();
    for (id, raw) in parse_info_body(body) {
        let val = read_c_string(&raw);
        let val = (!val.is_empty()).then_some(val);
        match &id {
            b"IART" => out.creator = val,
            b"IGNR" => out.category = val,
            b"IKEY" => out.keywords = val,
            _ => {}
        }
    }
    out
}

/// Which RIFF INFO fields to set (read-modify-write: `None` leaves that tag
/// untouched, so unknown subfields and unset mapped tags survive).
#[derive(Default)]
pub(crate) struct InfoWrite<'a> {
    pub creator: Option<&'a str>,
    pub category: Option<&'a str>,
    pub keywords: Option<&'a str>,
}

/// Write Creator/Category/Keywords into `path`'s `LIST`/`INFO` chunk (creating
/// the list if absent), preserving every other INFO subfield and every other
/// chunk byte-for-byte. Plain RIFF/WAVE only (same gate as bext).
pub(crate) fn write_riff_info(path: &Path, w: &InfoWrite) -> Result<(), PlaybackError> {
    if w.creator.is_none() && w.category.is_none() && w.keywords.is_none() {
        return Ok(());
    }
    if !can_write_bext(path) {
        return Err(PlaybackError::DecodeError(format!(
            "{path:?}: embedded metadata write is only supported for plain WAV/BWF files"
        )));
    }
    let prefix = read_header_prefix(path, HEADER_PREFIX_MAX)?;
    let slot = locate_info(&prefix);

    // Existing subfields (so unmapped ones survive), or an empty list.
    let mut fields = match &slot {
        InfoSlot::Replace {
            start, body_len, ..
        } => {
            let body = &prefix[(start + 8)..(start + 8 + body_len).min(prefix.len())];
            parse_info_body(body)
        }
        InfoSlot::InsertAt(_) => Vec::new(),
    };

    let mut set = |tag: &[u8; 4], value: Option<&str>| {
        let Some(value) = value else { return };
        let bytes = value.as_bytes().to_vec();
        match fields.iter_mut().find(|(id, _)| id == tag) {
            Some((_, v)) => *v = bytes,
            None => fields.push((*tag, bytes)),
        }
    };
    set(b"IART", w.creator);
    set(b"IGNR", w.category);
    set(b"IKEY", w.keywords);

    // Rebuild the INFO body: "INFO" + each subchunk (NUL-terminated value,
    // padded to even), then wrap in a LIST chunk.
    let mut body = Vec::from(*b"INFO");
    for (id, value) in &fields {
        let size = value.len() + 1; // include the NUL terminator
        body.extend_from_slice(id);
        body.extend_from_slice(&(size as u32).to_le_bytes());
        body.extend_from_slice(value);
        body.push(0);
        if size % 2 == 1 {
            body.push(0);
        }
    }
    let mut chunk = Vec::with_capacity(8 + body.len() + 1);
    chunk.extend_from_slice(b"LIST");
    chunk.extend_from_slice(&(body.len() as u32).to_le_bytes());
    chunk.extend_from_slice(&body);
    if body.len() % 2 == 1 {
        chunk.push(0);
    }

    let (head_end, tail_start) = match slot {
        InfoSlot::Replace { start, end, .. } => (start, end),
        InfoSlot::InsertAt(at) => (at, at),
    };
    splice_chunk(path, head_end, tail_start, &chunk)
}

// ---- iXML: keep an existing <BWF_DESCRIPTION> mirror in sync ----
//
// iXML is an opaque XML chunk written by field recorders. punks2 never
// creates it, never parses it as a model, and touches exactly one leaf:
// <BWF_DESCRIPTION>, the spec-defined mirror of bext Description. On write we
// update that leaf iff it already exists (so the file stays self-consistent);
// everything else in the chunk is preserved byte-for-byte. On read we use it
// (then <NOTE>, opt-in compat) only as a Description fallback when bext is
// absent. ponytail: this is deliberately not a real XML parser — no
// namespaces, CDATA, or entity handling beyond the few basic escapes below;
// upgrade path is a real XML library if pro-audio fields (L4) ever land.

/// Locate the `iXML` chunk body `[start, end)` and its size, if present
/// (before `data`).
fn locate_ixml(prefix: &[u8]) -> Option<(usize, usize, usize)> {
    let mut pos = 12;
    while pos + 8 <= prefix.len() {
        let id = &prefix[pos..pos + 4];
        let size = u32::from_le_bytes(prefix[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        let next = body + size + (size & 1);
        if id == b"iXML" {
            return Some((pos, next.min(prefix.len()).max(body), size));
        }
        if id == b"data" || size == 0xFFFF_FFFF {
            break;
        }
        if next <= pos || next > prefix.len() {
            break;
        }
        pos = next;
    }
    None
}

/// The text content of the first `<tag>…</tag>` in `xml`, basic-unescaped.
fn xml_inner(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml_unescape(&xml[start..end]))
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Read the iXML Description fallback: `<BWF_DESCRIPTION>` then, as opt-in
/// compat, `<NOTE>` (which is not semantically a description, but some
/// recorders put the useful text only there).
pub(crate) fn read_ixml_description(prefix: &[u8]) -> Option<String> {
    let (start, end, body_len) = locate_ixml(prefix)?;
    let body = &prefix[(start + 8)..(start + 8 + body_len).min(end)];
    let xml = String::from_utf8_lossy(body);
    xml_inner(&xml, "BWF_DESCRIPTION")
        .or_else(|| xml_inner(&xml, "NOTE"))
        .filter(|s| !s.is_empty())
}

/// If `path` has an iXML chunk with a `<BWF_DESCRIPTION>` leaf, update it to
/// `description` so it can't disagree with the bext Description we just wrote.
/// No-op when there's no iXML chunk or no such leaf — punks2 never creates
/// either. Preserves the rest of the iXML (and every other chunk) verbatim.
pub(crate) fn write_ixml_description_mirror(
    path: &Path,
    description: &str,
) -> Result<(), PlaybackError> {
    if !can_write_bext(path) {
        return Ok(()); // not a writable WAV; nothing to mirror
    }
    let prefix = read_header_prefix(path, HEADER_PREFIX_MAX)?;
    let Some((start, end, body_len)) = locate_ixml(&prefix) else {
        return Ok(());
    };
    let body = &prefix[(start + 8)..(start + 8 + body_len).min(end)];
    let xml = String::from_utf8_lossy(body).into_owned();
    let open = "<BWF_DESCRIPTION>";
    let close = "</BWF_DESCRIPTION>";
    let Some(open_pos) = xml.find(open) else {
        return Ok(()); // no mirror to maintain
    };
    let inner_start = open_pos + open.len();
    let Some(rel_close) = xml[inner_start..].find(close) else {
        return Ok(()); // malformed; leave it entirely alone
    };
    let inner_end = inner_start + rel_close;
    let mut new_xml = String::with_capacity(xml.len() + description.len());
    new_xml.push_str(&xml[..inner_start]);
    new_xml.push_str(&xml_escape(description));
    new_xml.push_str(&xml[inner_end..]);
    if new_xml == xml {
        return Ok(()); // already in sync — don't rewrite the file
    }

    let mut new_body = new_xml.into_bytes();
    let mut chunk = Vec::with_capacity(8 + new_body.len() + 1);
    chunk.extend_from_slice(b"iXML");
    chunk.extend_from_slice(&(new_body.len() as u32).to_le_bytes());
    chunk.append(&mut new_body);
    if chunk.len() % 2 == 1 {
        chunk.push(0);
    }
    splice_chunk(path, start, end, &chunk)
}

/// Atomically replace `path` with a new file that `build` writes to a sibling
/// temp path. Field recordings and finished mixes are irreplaceable, so this is
/// the one write primitive for them: the temp lives in the same directory (same
/// filesystem ⇒ the final rename is atomic) and **keeps `path`'s extension**
/// (some writers — notably `lofty` — probe format from the path). On any error
/// the temp is removed and `path` is left byte-for-byte untouched; on success
/// the finished temp is `fsync`ed before the rename, so a crash can't leave a
/// renamed-but-unflushed file. Reused by the bext splice today and by RIFF
/// INFO / iXML / project-file writers later.
pub(crate) fn write_atomically(
    path: &Path,
    build: impl FnOnce(&Path) -> Result<(), PlaybackError>,
) -> Result<(), PlaybackError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("punks");
    let tmp_name = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!(".{stem}.punks-tmp.{ext}"),
        None => format!(".{stem}.punks-tmp"),
    };
    let tmp_path = dir.join(tmp_name);

    let result = build(&tmp_path).and_then(|()| {
        let f = File::open(&tmp_path)
            .map_err(|e| PlaybackError::DecodeError(format!("{tmp_path:?}: {e}")))?;
        f.sync_all()
            .map_err(|e| PlaybackError::DecodeError(format!("{tmp_path:?}: {e}")))?;
        std::fs::rename(&tmp_path, path)
            .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: rename failed: {e}")))
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

/// Read exactly `len` bytes at `offset` from `file`.
fn read_exact_at(
    file: &mut File,
    offset: u64,
    len: usize,
    path: &Path,
) -> Result<Vec<u8>, PlaybackError> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)
        .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
    Ok(buf)
}

/// Stream-copy exactly `len` bytes from `src`'s current position to `dst`, in
/// bounded chunks so a multi-GB `data` chunk never loads into memory at once.
fn copy_n(src: &mut File, dst: &mut File, len: u64, path: &Path) -> Result<(), PlaybackError> {
    let mut remaining = len;
    let mut buf = [0u8; 1 << 20]; // 1 MiB
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        src.read_exact(&mut buf[..want])
            .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
        dst.write_all(&buf[..want])
            .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
        remaining -= want as u64;
    }
    Ok(())
}

/// If `raw` is an Ogg-Vorbis-in-WAV file (format tag 0x674f), return the inner
/// Ogg stream from the `data` chunk. Returns `None` for any normal file.
fn extract_ogg_in_wav(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < 44 || &raw[0..4] != b"RIFF" || &raw[8..12] != b"WAVE" {
        return None;
    }

    let mut pos = 12;
    let mut fmt_tag: Option<u16> = None;

    while pos + 8 <= raw.len() {
        let chunk_id = &raw[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([raw[pos + 4], raw[pos + 5], raw[pos + 6], raw[pos + 7]]) as usize;
        let body = pos + 8;

        match chunk_id {
            b"fmt " if body + 2 <= raw.len() => {
                fmt_tag = Some(u16::from_le_bytes([raw[body], raw[body + 1]]));
            }
            b"data" => {
                if fmt_tag == Some(WAVE_FORMAT_OGG_VORBIS) {
                    let end = (body + chunk_size).min(raw.len());
                    let inner = &raw[body..end];
                    if inner.starts_with(b"OggS") {
                        return Some(inner.to_vec());
                    }
                }
                return None;
            }
            _ => {}
        }
        // Chunks are word-aligned: a pad byte follows an odd-sized chunk.
        pos = body + chunk_size + (chunk_size & 1);
    }
    None
}

/// Parsed pieces of an RF64 container needed to synthesize a plain RIFF stream.
struct Rf64Info {
    sample_rate: u32,
    block_align: u64,
    data_offset: u64, // file offset of the data payload
    data_size: u64,   // true data-chunk size in bytes (from ds64)
    source_frames: u64,
    fmt_chunk: Vec<u8>, // "fmt " + len + payload (+ pad), copied verbatim
}

fn parse_rf64(prefix: &[u8]) -> Result<Rf64Info, PlaybackError> {
    let mut data_size: Option<u64> = None;
    let mut fmt_range: Option<(usize, usize)> = None; // (start, total len incl header + pad)
    let mut data_offset: Option<usize> = None;

    let mut pos = 12;
    while pos + 8 <= prefix.len() {
        let id = &prefix[pos..pos + 4];
        let size = u32::from_le_bytes(prefix[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        match id {
            b"ds64" if body + 16 <= prefix.len() => {
                // riffSize u64 @ body, dataSize u64 @ body+8.
                data_size = Some(u64::from_le_bytes(
                    prefix[body + 8..body + 16].try_into().unwrap(),
                ));
            }
            b"fmt " => fmt_range = Some((pos, 8 + size + (size & 1))),
            b"data" => {
                data_offset = Some(body);
                break;
            }
            _ => {}
        }
        if size == 0xFFFF_FFFF {
            break;
        }
        let next = body + size + (size & 1);
        if next <= pos || next > prefix.len() {
            break;
        }
        pos = next;
    }

    let data_size =
        data_size.ok_or_else(|| PlaybackError::DecodeError("rf64: missing ds64 chunk".into()))?;
    let (fmt_start, fmt_total) =
        fmt_range.ok_or_else(|| PlaybackError::DecodeError("rf64: missing fmt chunk".into()))?;
    let data_offset =
        data_offset.ok_or_else(|| PlaybackError::DecodeError("rf64: missing data chunk".into()))?;

    let fb = fmt_start + 8;
    if fb + 14 > prefix.len() {
        return Err(PlaybackError::DecodeError(
            "rf64: truncated fmt chunk".into(),
        ));
    }
    let sample_rate = u32::from_le_bytes(prefix[fb + 4..fb + 8].try_into().unwrap());
    let block_align =
        u16::from_le_bytes(prefix[fb + 12..fb + 14].try_into().unwrap()).max(1) as u64;

    Ok(Rf64Info {
        sample_rate,
        block_align,
        data_offset: data_offset as u64,
        data_size,
        source_frames: data_size / block_align,
        fmt_chunk: prefix[fmt_start..(fmt_start + fmt_total).min(prefix.len())].to_vec(),
    })
}

/// Build a plain-`RIFF` `MediaSourceStream` over the RF64 file's data region,
/// starting at frame `start_frame` and covering at most `max_frames` (all
/// remaining if `None`). symphonia can't read RF64, so we synthesize a valid
/// header and chain it in front of the raw PCM. The stream is forward-only
/// (`ReadOnlySource`); to decode a window at an offset we position the file
/// itself here rather than relying on symphonia to seek.
fn build_rf64_stream(
    path: &Path,
    rf64: &Rf64Info,
    start_frame: u64,
    max_frames: Option<u64>,
) -> Result<MediaSourceStream, PlaybackError> {
    let byte_off = start_frame * rf64.block_align;
    let remaining = rf64.data_size.saturating_sub(byte_off);
    let data_bytes = match max_frames {
        Some(m) => (m * rf64.block_align).min(remaining),
        None => remaining,
    };
    // Fits u32: an untruncated file is < 4 GB; a window is far smaller.
    let data_size_u32 = data_bytes.min(u32::MAX as u64) as u32;

    let riff_size =
        (4 + rf64.fmt_chunk.len() + 8 + data_size_u32 as usize).min(u32::MAX as usize) as u32;
    let mut header = Vec::with_capacity(12 + rf64.fmt_chunk.len() + 8);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&riff_size.to_le_bytes());
    header.extend_from_slice(b"WAVE");
    header.extend_from_slice(&rf64.fmt_chunk);
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_size_u32.to_le_bytes());

    let mut file =
        File::open(path).map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
    file.seek(SeekFrom::Start(rf64.data_offset + byte_off))
        .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
    let reader = Cursor::new(header).chain(file.take(data_bytes));
    Ok(MediaSourceStream::new(
        Box::new(ReadOnlySource::new(reader)),
        Default::default(),
    ))
}

/// Decode the from-the-start preview of an RF64 file.
fn decode_rf64(
    path: &Path,
    prefix: &[u8],
    metadata: AudioMetadata,
    threshold: Duration,
    window: Duration,
) -> Result<DecodedAudio, PlaybackError> {
    let rf64 = parse_rf64(prefix)?;
    let budget = preview_budget_frames(rf64.source_frames, rf64.sample_rate, threshold, window);
    let mss = build_rf64_stream(path, &rf64, 0, budget)?;
    let mut hint = Hint::new();
    hint.with_extension("wav");
    decode_from_stream(
        mss,
        &hint,
        metadata,
        Some(rf64.source_frames),
        threshold,
        window,
    )
}

/// Open a `MediaSourceStream` over the WHOLE source (no preview budget), plus
/// its format hint and a known source-frame count when the container tells us
/// (RF64). Used by full-file peaks and by the seek path for the window decode.
fn full_source_stream(
    path: &Path,
    prefix: &[u8],
) -> Result<(MediaSourceStream, Hint, Option<u64>), PlaybackError> {
    if prefix.len() >= 12 && &prefix[0..4] == b"RF64" && &prefix[8..12] == b"WAVE" {
        let rf64 = parse_rf64(prefix)?;
        let mss = build_rf64_stream(path, &rf64, 0, None)?;
        let mut hint = Hint::new();
        hint.with_extension("wav");
        return Ok((mss, hint, Some(rf64.source_frames)));
    }
    if riff_fmt_tag(prefix) == Some(WAVE_FORMAT_OGG_VORBIS) {
        let raw = std::fs::read(path)
            .map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
        if let Some(ogg) = extract_ogg_in_wav(&raw) {
            let mss = MediaSourceStream::new(Box::new(Cursor::new(ogg)), Default::default());
            let mut hint = Hint::new();
            hint.with_extension("ogg");
            return Ok((mss, hint, None));
        }
    }
    let file =
        File::open(path).map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    Ok((mss, hint, None))
}

/// The preview frame budget for a source of `source_frames` at `sample_rate`,
/// or `None` if the source is short enough to decode whole.
fn preview_budget_frames(
    source_frames: u64,
    sample_rate: u32,
    threshold: Duration,
    window: Duration,
) -> Option<u64> {
    if sample_rate == 0 || source_frames == 0 {
        return None;
    }
    let dur_secs = source_frames as f64 / sample_rate as f64;
    if dur_secs > threshold.as_secs_f64() {
        Some((window.as_secs_f64() * sample_rate as f64) as u64)
    } else {
        None
    }
}

/// A probed stream ready to decode: the format reader plus the default track's
/// parameters, pulled out so callers can seek before decoding.
struct Probed {
    format: Box<dyn FormatReader>,
    track_id: u32,
    codec_params: CodecParameters,
    sample_rate: u32,
    channels: u16,
    n_frames: Option<u64>,
}

fn probe_format(mss: MediaSourceStream, hint: &Hint) -> Result<Probed, PlaybackError> {
    let probed = symphonia::default::get_probe()
        .format(
            hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| PlaybackError::DecodeError(format!("unsupported audio format: {e}")))?;
    let format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| PlaybackError::DecodeError("no audio track found".into()))?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| PlaybackError::DecodeError("unknown sample rate".into()))?;
    let channels = codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
    let n_frames = codec_params.n_frames;
    Ok(Probed {
        format,
        track_id,
        codec_params,
        sample_rate,
        channels,
        n_frames,
    })
}

/// Decode interleaved f32 samples from `probed`, stopping after `max_frames`
/// (all remaining if `None`). Decode starts wherever `probed.format` is
/// currently positioned, so callers may seek first.
fn decode_pcm(
    probed: &mut Probed,
    max_frames: Option<u64>,
) -> Result<(Vec<f32>, u64), PlaybackError> {
    let mut decoder = symphonia::default::get_codecs()
        .make(&probed.codec_params, &DecoderOptions::default())
        .map_err(|e| PlaybackError::DecodeError(format!("codec init failed: {e}")))?;

    let mut all_samples: Vec<f32> = Vec::new();
    let mut decoded_frames: u64 = 0;

    loop {
        let packet = match probed.format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(PlaybackError::DecodeError(format!("packet read: {e}"))),
        };
        if packet.track_id() != probed.track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(e)) => {
                log::warn!("decode error (skipping packet): {e}");
                continue;
            }
            Err(e) => return Err(PlaybackError::DecodeError(format!("decode: {e}"))),
        };
        let spec = *decoded.spec();
        let num_frames = decoded.frames();
        if num_frames == 0 {
            continue;
        }
        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        all_samples.extend_from_slice(sample_buf.samples());
        decoded_frames += num_frames as u64;
        if let Some(b) = max_frames {
            if decoded_frames >= b {
                break;
            }
        }
    }
    Ok((all_samples, decoded_frames))
}

/// Probe and decode an already-built stream into a from-the-start preview. For
/// sources longer than `threshold`, only the first `window` is decoded and
/// `truncated` is set. `source_frames_override` supplies the true length when
/// the stream's own header was rewritten (RF64 preview).
fn decode_from_stream(
    mss: MediaSourceStream,
    hint: &Hint,
    metadata: AudioMetadata,
    source_frames_override: Option<u64>,
    threshold: Duration,
    window: Duration,
) -> Result<DecodedAudio, PlaybackError> {
    let mut probed = probe_format(mss, hint)?;
    let source_frames_hint = source_frames_override.or(probed.n_frames).unwrap_or(0);
    let budget = preview_budget_frames(source_frames_hint, probed.sample_rate, threshold, window);

    let (all_samples, decoded_frames) = decode_pcm(&mut probed, budget)?;
    if all_samples.is_empty() {
        return Err(PlaybackError::DecodeError("no audio data decoded".into()));
    }

    let source_frames = if source_frames_hint > 0 {
        source_frames_hint
    } else {
        decoded_frames
    };
    let rate = probed.sample_rate as f64;
    Ok(DecodedAudio {
        interleaved: all_samples,
        channels: probed.channels,
        sample_rate: probed.sample_rate,
        metadata,
        source_duration: Duration::from_secs_f64(source_frames as f64 / rate),
        window_start: Duration::ZERO,
        truncated: budget.is_some(),
    })
}

/// Decode a bounded `window` of the source starting near `start`. Used for
/// on-demand scrubbing into a long file past its loaded preview. The returned
/// `window_start` is where the decode actually landed (seeks snap to a
/// packet/block boundary).
pub fn decode_window(
    path: &Path,
    start: Duration,
    window: Duration,
) -> Result<DecodedAudio, PlaybackError> {
    let prefix = read_header_prefix(path, HEADER_PREFIX_MAX)?;
    let metadata = parse_riff_metadata(&prefix);

    // RF64: its stream is forward-only, so position the file at the window
    // rather than asking symphonia to seek.
    if prefix.len() >= 12 && &prefix[0..4] == b"RF64" && &prefix[8..12] == b"WAVE" {
        let rf64 = parse_rf64(&prefix)?;
        let start_frame = ((start.as_secs_f64() * rf64.sample_rate as f64) as u64)
            .min(rf64.source_frames.saturating_sub(1));
        let window_frames = ((window.as_secs_f64() * rf64.sample_rate as f64) as u64).max(1);
        let mss = build_rf64_stream(path, &rf64, start_frame, Some(window_frames))?;
        let mut hint = Hint::new();
        hint.with_extension("wav");
        let mut probed = probe_format(mss, &hint)?;
        let (samples, _decoded_frames) = decode_pcm(&mut probed, Some(window_frames))?;
        if samples.is_empty() {
            return Err(PlaybackError::DecodeError("no audio data decoded".into()));
        }
        let rate = probed.sample_rate as f64;
        return Ok(DecodedAudio {
            interleaved: samples,
            channels: probed.channels,
            sample_rate: probed.sample_rate,
            metadata,
            source_duration: Duration::from_secs_f64(rf64.source_frames as f64 / rate),
            window_start: Duration::from_secs_f64(start_frame as f64 / rate),
            truncated: true,
        });
    }

    // Normal / Ogg-in-WAV: probe the full source, then seek.
    let (mss, hint, sfo) = full_source_stream(path, &prefix)?;
    let mut probed = probe_format(mss, &hint)?;
    let source_frames = sfo.or(probed.n_frames).unwrap_or(0);
    let window_frames = ((window.as_secs_f64() * probed.sample_rate as f64) as u64).max(1);

    let track_id = probed.track_id;
    let actual_ts = match probed.format.seek(
        SeekMode::Accurate,
        SeekTo::Time {
            time: start.into(),
            track_id: Some(track_id),
        },
    ) {
        Ok(seeked) => seeked.actual_ts,
        Err(e) => {
            log::warn!("seek failed, decoding from start: {e}");
            0
        }
    };

    let (samples, decoded_frames) = decode_pcm(&mut probed, Some(window_frames))?;
    if samples.is_empty() {
        return Err(PlaybackError::DecodeError("no audio data decoded".into()));
    }
    let rate = probed.sample_rate as f64;
    let src = if source_frames > 0 {
        source_frames
    } else {
        actual_ts + decoded_frames
    };
    Ok(DecodedAudio {
        interleaved: samples,
        channels: probed.channels,
        sample_rate: probed.sample_rate,
        metadata,
        source_duration: Duration::from_secs_f64(src as f64 / rate),
        window_start: Duration::from_secs_f64(actual_ts as f64 / rate),
        truncated: true,
    })
}

/// Compute waveform peaks over the ENTIRE source with O(num_buckets) memory:
/// stream-decode chunk by chunk, fold each frame into its bucket's min/max,
/// discard the samples. Requires a known source length; returns an error for
/// the rare unknown-length source (some header-less MP3) so the caller can
/// keep the preview peaks.
pub fn compute_source_peaks(
    path: &Path,
    num_buckets: usize,
) -> Result<WaveformPeaks, PlaybackError> {
    let num_buckets = num_buckets.max(1);
    let prefix = read_header_prefix(path, HEADER_PREFIX_MAX)?;
    let (mss, hint, sfo) = full_source_stream(path, &prefix)?;
    let mut probed = probe_format(mss, &hint)?;
    let source_frames = sfo.or(probed.n_frames).unwrap_or(0);
    if source_frames == 0 {
        return Err(PlaybackError::DecodeError(
            "unknown source length; full-file peaks unavailable".into(),
        ));
    }

    let mut decoder = symphonia::default::get_codecs()
        .make(&probed.codec_params, &DecoderOptions::default())
        .map_err(|e| PlaybackError::DecodeError(format!("codec init failed: {e}")))?;

    let mut mins = vec![f32::MAX; num_buckets];
    let mut maxs = vec![f32::MIN; num_buckets];
    let mut frame_idx: u64 = 0;

    loop {
        let packet = match probed.format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(PlaybackError::DecodeError(format!("packet read: {e}"))),
        };
        if packet.track_id() != probed.track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(e)) => {
                log::warn!("decode error (skipping packet): {e}");
                continue;
            }
            Err(e) => return Err(PlaybackError::DecodeError(format!("decode: {e}"))),
        };
        let spec = *decoded.spec();
        let num_frames = decoded.frames();
        if num_frames == 0 {
            continue;
        }
        let ch = spec.channels.count().max(1);
        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        let samples = sample_buf.samples();
        let inv_ch = 1.0 / ch as f32;
        for f in 0..num_frames {
            let base = f * ch;
            let mut mono = 0.0f32;
            for c in 0..ch {
                mono += samples[base + c];
            }
            mono *= inv_ch;
            let bucket = ((frame_idx.saturating_mul(num_buckets as u64) / source_frames) as usize)
                .min(num_buckets - 1);
            if mono < mins[bucket] {
                mins[bucket] = mono;
            }
            if mono > maxs[bucket] {
                maxs[bucket] = mono;
            }
            frame_idx += 1;
        }
    }

    let peaks = (0..num_buckets)
        .map(|b| {
            if mins[b] == f32::MAX {
                (0.0, 0.0)
            } else {
                (mins[b].clamp(-1.0, 1.0), maxs[b].clamp(-1.0, 1.0))
            }
        })
        .collect();
    Ok(WaveformPeaks { peaks, num_buckets })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal RIFF/WAVE with the given fmt tag and data-chunk body.
    fn wav(fmt_tag: u16, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&fmt_tag.to_le_bytes());
        v.extend_from_slice(&[0u8; 14]);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    /// A 16-bit PCM fmt chunk body (16 bytes).
    fn pcm_fmt(channels: u16, sample_rate: u32) -> Vec<u8> {
        let bits = 16u16;
        let block_align = channels * bits / 8;
        let byte_rate = sample_rate * block_align as u32;
        let mut f = Vec::new();
        f.extend_from_slice(&1u16.to_le_bytes()); // PCM
        f.extend_from_slice(&channels.to_le_bytes());
        f.extend_from_slice(&sample_rate.to_le_bytes());
        f.extend_from_slice(&byte_rate.to_le_bytes());
        f.extend_from_slice(&block_align.to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        f
    }

    /// A full 16-bit PCM WAV with `frames` mono samples of value 0.
    fn pcm_wav(sample_rate: u32, frames: usize) -> Vec<u8> {
        let fmt = pcm_fmt(1, sample_rate);
        let data = vec![0u8; frames * 2];
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&((4 + 8 + fmt.len() + 8 + data.len()) as u32).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        v.extend_from_slice(&fmt);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(&data);
        v
    }

    /// A bext chunk body with the standard fixed layout.
    fn bext_body(desc: &str, originator: &str, time_ref: u64) -> Vec<u8> {
        let mut b = vec![0u8; 602];
        b[..desc.len()].copy_from_slice(desc.as_bytes());
        b[256..256 + originator.len()].copy_from_slice(originator.as_bytes());
        b[320..330].copy_from_slice(b"2026-06-22");
        b[330..338].copy_from_slice(b"14:30:00");
        b[338..346].copy_from_slice(&time_ref.to_le_bytes());
        b
    }

    #[test]
    fn extracts_inner_ogg_stream() {
        let ogg = b"OggS\x00\x02 fake vorbis bitstream";
        let file = wav(WAVE_FORMAT_OGG_VORBIS, ogg);
        assert_eq!(extract_ogg_in_wav(&file).as_deref(), Some(&ogg[..]));
    }

    #[test]
    fn ignores_normal_pcm_wav() {
        let file = wav(0x0001, b"\x00\x01\x02\x03");
        assert_eq!(extract_ogg_in_wav(&file), None);
    }

    #[test]
    fn ignores_ogg_tag_without_ogg_magic() {
        let file = wav(WAVE_FORMAT_OGG_VORBIS, b"not ogg data");
        assert_eq!(extract_ogg_in_wav(&file), None);
    }

    #[test]
    fn ignores_non_riff() {
        assert_eq!(extract_ogg_in_wav(b"OggS at the very start, no RIFF"), None);
    }

    #[test]
    fn parse_bext_reads_fields_and_trims_nuls() {
        let m = parse_bext(&bext_body("Scene 5 Take 2", "ZOOM F8", 48_000 * 3600));
        assert_eq!(m.description.as_deref(), Some("Scene 5 Take 2"));
        assert_eq!(m.originator.as_deref(), Some("ZOOM F8"));
        assert_eq!(m.origination_date.as_deref(), Some("2026-06-22"));
        assert_eq!(m.origination_time.as_deref(), Some("14:30:00"));
        assert_eq!(m.time_reference, Some(48_000 * 3600));
    }

    #[test]
    fn parse_riff_metadata_finds_bext() {
        // RIFF/WAVE with fmt + bext + data.
        let fmt = pcm_fmt(2, 48_000);
        let bext = bext_body("Boom mic", "Sound Devices", 0);
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        v.extend_from_slice(&fmt);
        v.extend_from_slice(b"bext");
        v.extend_from_slice(&(bext.len() as u32).to_le_bytes());
        v.extend_from_slice(&bext);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(&[0u8; 4]);

        let m = parse_riff_metadata(&v);
        assert_eq!(m.description.as_deref(), Some("Boom mic"));
        assert_eq!(m.originator.as_deref(), Some("Sound Devices"));
    }

    #[test]
    fn parse_riff_metadata_empty_without_bext() {
        assert_eq!(
            parse_riff_metadata(&pcm_wav(48_000, 4)),
            AudioMetadata::default()
        );
    }

    #[test]
    fn preview_budget_thresholds() {
        let thr = Duration::from_secs(120);
        let win = Duration::from_secs(120);
        // 130 s at 48k -> preview of 120 s.
        assert_eq!(
            preview_budget_frames(48_000 * 130, 48_000, thr, win),
            Some(48_000 * 120)
        );
        // 60 s -> decode whole.
        assert_eq!(preview_budget_frames(48_000 * 60, 48_000, thr, win), None);
        // Unknown length / rate -> no budget.
        assert_eq!(preview_budget_frames(0, 48_000, thr, win), None);
        assert_eq!(preview_budget_frames(48_000, 0, thr, win), None);
    }

    #[test]
    fn decode_truncates_long_stream() {
        // 1 s mono @ 8 kHz, but decode with a 100 ms threshold / 200 ms window.
        let bytes = pcm_wav(8_000, 8_000);
        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("wav");
        let out = decode_from_stream(
            mss,
            &hint,
            AudioMetadata::default(),
            None,
            Duration::from_millis(100),
            Duration::from_millis(200),
        )
        .expect("decode");

        assert!(out.truncated);
        let frames = out.interleaved.len() / out.channels as usize;
        // ~200 ms @ 8k = 1600 frames, within a packet of slop.
        assert!((1600..8_000).contains(&frames), "frames = {frames}");
        // True duration reflects the whole 1 s source.
        assert!((out.source_duration.as_secs_f64() - 1.0).abs() < 0.05);
    }

    #[test]
    fn decode_rf64_end_to_end() {
        // Build a tiny RF64: RF64 + ds64 + fmt (PCM mono 8k) + data (4 frames).
        let frames: [i16; 4] = [0, 1000, -1000, 500];
        let mut data = Vec::new();
        for s in frames {
            data.extend_from_slice(&s.to_le_bytes());
        }
        let fmt = pcm_fmt(1, 8_000);

        let mut ds64 = Vec::new();
        ds64.extend_from_slice(&0u64.to_le_bytes()); // riffSize (unused here)
        ds64.extend_from_slice(&(data.len() as u64).to_le_bytes()); // dataSize
        ds64.extend_from_slice(&(frames.len() as u64).to_le_bytes()); // sampleCount
        ds64.extend_from_slice(&0u32.to_le_bytes()); // table length

        let mut v = Vec::new();
        v.extend_from_slice(b"RF64");
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"ds64");
        v.extend_from_slice(&(ds64.len() as u32).to_le_bytes());
        v.extend_from_slice(&ds64);
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        v.extend_from_slice(&fmt);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // RF64 data sentinel
        v.extend_from_slice(&data);

        let dir = std::env::temp_dir();
        let path = dir.join(format!("punks2_rf64_{}.wav", std::process::id()));
        std::fs::write(&path, &v).expect("write temp rf64");
        let out = decode_file(&path);
        let _ = std::fs::remove_file(&path);
        let out = out.expect("decode rf64");

        assert_eq!(out.channels, 1);
        assert_eq!(out.sample_rate, 8_000);
        assert_eq!(out.interleaved.len(), frames.len());
        assert!(!out.truncated);
    }

    /// Write bytes to a unique temp file; caller removes it.
    fn temp_wav(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "punks2_{tag}_{}_{}.wav",
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
    fn compute_source_peaks_bucket_count() {
        // 1 s mono @ 8 kHz of silence.
        let path = temp_wav("peaks", &pcm_wav(8_000, 8_000));
        let peaks = compute_source_peaks(&path, 64);
        let _ = std::fs::remove_file(&path);
        let peaks = peaks.expect("peaks");
        assert_eq!(peaks.num_buckets, 64);
        assert_eq!(peaks.peaks.len(), 64);
        // Silence -> every bucket flat at zero.
        assert!(peaks.peaks.iter().all(|&(lo, hi)| lo == 0.0 && hi == 0.0));
    }

    #[test]
    fn decode_window_lands_near_offset() {
        // 1 s mono @ 8 kHz; grab a 200 ms window starting at 500 ms.
        let path = temp_wav("window", &pcm_wav(8_000, 8_000));
        let out = decode_window(
            &path,
            Duration::from_millis(500),
            Duration::from_millis(200),
        );
        let _ = std::fs::remove_file(&path);
        let out = out.expect("window");

        // Seeks snap to a PCM packet boundary at or just before the request
        // (symphonia's 1152-frame packets = 144 ms at this 8 kHz test rate;
        // ~24 ms at 48 kHz), so 0.5 s lands at 0.432 s.
        let ws = out.window_start.as_secs_f64();
        assert!((0.35..=0.5).contains(&ws), "window_start = {ws}");
        // True source length is still the whole 1 s.
        assert!((out.source_duration.as_secs_f64() - 1.0).abs() < 0.02);
        // At least the requested ~1600 frames, rounded up to a packet boundary.
        let frames = out.interleaved.len() / out.channels as usize;
        assert!((1600..3000).contains(&frames), "frames = {frames}");
        assert!(out.truncated);
    }

    /// A full RIFF/WAVE: `fmt ` + any extra chunks (e.g. `bext`, `cue `) + `data`,
    /// with the RIFF size header patched to match.
    fn build_wav(fmt: &[u8], extra_chunks: &[(&[u8; 4], &[u8])], data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&0u32.to_le_bytes()); // patched below
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        v.extend_from_slice(fmt);
        if fmt.len() % 2 == 1 {
            v.push(0);
        }
        for (id, body) in extra_chunks {
            v.extend_from_slice(*id);
            v.extend_from_slice(&(body.len() as u32).to_le_bytes());
            v.extend_from_slice(body);
            if body.len() % 2 == 1 {
                v.push(0);
            }
        }
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        if data.len() % 2 == 1 {
            v.push(0);
        }
        let total = (v.len() - 8) as u32;
        v[4..8].copy_from_slice(&total.to_le_bytes());
        v
    }

    /// The `data` chunk's payload bytes, by walking chunks like the reader does.
    fn data_bytes(raw: &[u8]) -> &[u8] {
        let mut pos = 12;
        loop {
            let id = &raw[pos..pos + 4];
            let size = u32::from_le_bytes(raw[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body = pos + 8;
            if id == b"data" {
                return &raw[body..body + size];
            }
            pos = body + size + (size & 1);
        }
    }

    #[test]
    fn write_description_inserts_bext_when_absent() {
        let data = vec![7u8; 200];
        let raw = build_wav(&pcm_fmt(1, 8_000), &[], &data);
        let path = temp_wav("bext_insert", &raw);

        write_bext_description(&path, "Footsteps, gravel, take 3").unwrap();

        let out = decode_file(&path).expect("decode after write");
        assert_eq!(
            out.metadata.description.as_deref(),
            Some("Footsteps, gravel, take 3")
        );
        let rewritten = std::fs::read(&path).unwrap();
        assert_eq!(data_bytes(&rewritten), &data[..]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_description_replaces_and_preserves_other_fields_and_chunks() {
        let data = vec![9u8; 500];
        let bext = bext_body("Old description", "Studio A", 123_456_789);
        let cue: &[u8] = b"CUE-CHUNK-PAYLOAD-not-touched";
        let raw = build_wav(
            &pcm_fmt(2, 44_100),
            &[(b"bext", &bext), (b"cue ", cue)],
            &data,
        );
        let path = temp_wav("bext_replace", &raw);

        write_bext_description(&path, "New description").unwrap();

        let out = decode_file(&path).expect("decode after write");
        assert_eq!(out.metadata.description.as_deref(), Some("New description"));
        // Fields the write must not disturb.
        assert_eq!(out.metadata.originator.as_deref(), Some("Studio A"));
        assert_eq!(out.metadata.time_reference, Some(123_456_789));

        let rewritten = std::fs::read(&path).unwrap();
        // The unrelated `cue ` chunk and the audio data are untouched.
        let cue_pos = rewritten
            .windows(cue.len())
            .position(|w| w == cue)
            .expect("cue payload preserved verbatim");
        assert!(cue_pos > 0);
        assert_eq!(data_bytes(&rewritten), &data[..]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_description_truncates_long_unicode_safely() {
        // A multi-byte character (3 bytes each) repeated past the 255-byte
        // budget; truncation must land on a char boundary and stay valid UTF-8.
        let long = "\u{2603}".repeat(120); // 360 bytes
        let raw = build_wav(&pcm_fmt(1, 8_000), &[], &[0u8; 10]);
        let path = temp_wav("bext_truncate", &raw);

        write_bext_description(&path, &long).unwrap();

        let out = decode_file(&path).expect("decode after write");
        let desc = out.metadata.description.expect("description present");
        assert!(desc.len() <= 255);
        assert!(long.starts_with(&desc));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cannot_write_bext_for_rf64() {
        let fmt = pcm_fmt(1, 8_000);
        let data = vec![0u8; 16];
        let mut ds64 = Vec::new();
        ds64.extend_from_slice(&0u64.to_le_bytes()); // riff size (unused by us)
        ds64.extend_from_slice(&(data.len() as u64).to_le_bytes());
        ds64.extend_from_slice(&0u64.to_le_bytes()); // sample count
        ds64.extend_from_slice(&0u32.to_le_bytes()); // table length

        let mut v = Vec::new();
        v.extend_from_slice(b"RF64");
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"ds64");
        v.extend_from_slice(&(ds64.len() as u32).to_le_bytes());
        v.extend_from_slice(&ds64);
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        v.extend_from_slice(&fmt);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        v.extend_from_slice(&data);
        let path = temp_wav("bext_rf64", &v);

        assert!(!can_write_bext(&path));
        assert!(write_bext_description(&path, "nope").is_err());
        // Refusing to write must never touch the original file.
        assert_eq!(std::fs::read(&path).unwrap(), v);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_atomically_leaves_original_untouched_on_error() {
        let original = b"the original bytes".to_vec();
        let path = temp_wav("atomic_err", &original);

        // A closure that writes a partial temp then fails must roll back cleanly:
        // original intact, no temp left behind.
        let result = write_atomically(&path, |tmp| {
            std::fs::write(tmp, b"half-written garbage").unwrap();
            Err(PlaybackError::DecodeError("boom".into()))
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);

        // The sibling temp for *this* file must be gone (exact name, so parallel
        // tests writing their own temps can't make this flaky).
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let tmp = path
            .parent()
            .unwrap()
            .join(format!(".{stem}.punks-tmp.wav"));
        assert!(!tmp.exists(), "temp file leaked: {tmp:?}");
        let _ = std::fs::remove_file(&path);
    }
}
