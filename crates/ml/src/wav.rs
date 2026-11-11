//! Minimal WAV audio IO (Sprint 23): std-only PCM reading plus
//! deterministic PCM writing. Runners never guess a format: an
//! unreadable or non-PCM file refuses naming the file and the cause.

/// Decoded mono samples plus the source rate.
#[derive(Debug, Clone)]
pub struct WavData {
    /// Mono samples in `[-1.0, 1.0]`, channels mixed down.
    pub samples: Vec<f32>,
    /// Source sample rate in Hz.
    pub rate: u32,
    /// Source duration in seconds.
    pub duration: f64,
}

fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn i16le(b: &[u8]) -> i16 {
    i16::from_le_bytes([b[0], b[1]])
}

/// Read a WAV file into mono `f32` samples. Supports 16-bit PCM and
/// 32-bit IEEE float, any channel count (mixed down). Anything else
/// refuses naming the file and the cause.
pub fn read_wav(path: &std::path::Path) -> Result<WavData, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("audio {} cannot be read: {e}", path.display()))?;
    decode_wav(&bytes).map_err(|cause| format!("audio {} {cause}", path.display()))
}

/// Decode WAV bytes (also used for test fixtures).
pub fn decode_wav(bytes: &[u8]) -> Result<WavData, String> {
    if bytes.len() < 44 {
        return Err("is too short to be a WAV file".to_string());
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("is not a WAV file (missing RIFF/WAVE)".to_string());
    }
    let mut pos = 12;
    let mut channels: u16 = 0;
    let mut rate: u32 = 0;
    let mut format: u16 = 0;
    let mut bits: u16 = 0;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32le(&bytes[pos + 4..pos + 8]) as usize;
        let body = pos + 8;
        let end = body.saturating_add(size).min(bytes.len());
        if id == b"fmt " {
            if end - body < 16 {
                return Err("has a corrupt WAV header".to_string());
            }
            format = u16le(&bytes[body..body + 2]);
            channels = u16le(&bytes[body + 2..body + 4]);
            rate = u32le(&bytes[body + 4..body + 8]);
            bits = u16le(&bytes[body + 14..body + 16]);
        } else if id == b"data" {
            data = Some(&bytes[body..end]);
        }
        pos = end + (end % 2);
    }
    if channels == 0 || rate == 0 {
        return Err("has a corrupt WAV header".to_string());
    }
    let data = data.ok_or_else(|| "has no audio data".to_string())?;
    let mut samples = Vec::new();
    match (format, bits) {
        (1, 16) => {
            let frame = channels as usize * 2;
            let frames = data.len() / frame;
            if frames == 0 {
                return Err("has no audio data".to_string());
            }
            samples.reserve(frames);
            for f in 0..frames {
                let mut acc = 0.0f32;
                for c in 0..channels as usize {
                    let o = f * frame + c * 2;
                    acc += i16le(&data[o..o + 2]) as f32 / 32768.0;
                }
                samples.push(acc / channels as f32);
            }
        }
        (3, 32) => {
            let frame = channels as usize * 4;
            let frames = data.len() / frame;
            if frames == 0 {
                return Err("has no audio data".to_string());
            }
            samples.reserve(frames);
            for f in 0..frames {
                let mut acc = 0.0f32;
                for c in 0..channels as usize {
                    let o = f * frame + c * 4;
                    acc += f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
                }
                samples.push(acc / channels as f32);
            }
        }
        _ => {
            return Err(format!(
                "uses an unsupported WAV encoding (format {format}, {bits}-bit); only 16-bit PCM and 32-bit float decode"
            ));
        }
    }
    let duration = samples.len() as f64 / rate as f64;
    Ok(WavData {
        samples,
        rate,
        duration,
    })
}

/// Write mono `f32` samples as 16-bit PCM WAV at `rate` Hz. The bytes
/// are a pure function of the samples and rate (byte-identical
/// reruns), with a correct header so audio tracks validate and
/// render it like authored media.
pub fn write_wav_mono16(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Write the WAV bytes to `path`, creating parent directories.
pub fn write_wav_file(path: &std::path::Path, samples: &[f32], rate: u32) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create output dir {}: {e}", parent.display()))?;
        }
    }
    let bytes = write_wav_mono16(samples, rate);
    std::fs::write(path, &bytes).map_err(|e| format!("cannot write audio {}: {e}", path.display()))
}
