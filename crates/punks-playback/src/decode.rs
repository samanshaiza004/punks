use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
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
    /// Duration actually decoded (== `source_duration` unless `truncated`).
    pub preview_duration: Duration,
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
const HEADER_PREFIX_MAX: usize = 1 << 20; // 1 MiB

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
fn read_header_prefix(path: &Path, max: usize) -> Result<Vec<u8>, PlaybackError> {
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
fn parse_riff_metadata(prefix: &[u8]) -> AudioMetadata {
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
        preview_duration: Duration::from_secs_f64(decoded_frames as f64 / rate),
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
        let (samples, decoded_frames) = decode_pcm(&mut probed, Some(window_frames))?;
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
            preview_duration: Duration::from_secs_f64(decoded_frames as f64 / rate),
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
        preview_duration: Duration::from_secs_f64(decoded_frames as f64 / rate),
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
}
