//! Still-picture decoding through the FFmpeg subprocess.
//!
//! This is the replacement for a linked media stack: any picture the
//! renderer needs (future picture inputs, emoji sheets) is decoded by
//! the same pinned-or-host FFmpeg binary that encodes the MP4, never
//! by an in-process decoder. A file FFmpeg cannot read is a refusal
//! naming the file, never a blank picture.

use std::path::Path;

/// A decoded still picture: dimensions plus raw RGB24 bytes.
#[derive(Debug, Clone)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

/// Probe one stream's `width` and `height` with `ffprobe_path`.
fn probe_size(ffprobe: &Path, path: &Path) -> Result<(u32, u32), String> {
    let out = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            &path.display().to_string(),
        ])
        .output()
        .map_err(|e| {
            format!(
                "picture {:?} could not be probed (ffprobe stage): {e}",
                path.display()
            )
        })?;
    if !out.status.success() {
        return Err(format!(
            "picture {:?} is unreadable (ffprobe reports no video stream); no picture was decoded",
            path.display()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (w, h) = text
        .trim()
        .split_once(',')
        .and_then(|(a, b)| Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?)))
        .ok_or_else(|| {
            format!(
                "picture {:?} has no readable dimensions; no picture was decoded",
                path.display()
            )
        })?;
    if w == 0 || h == 0 {
        return Err(format!(
            "picture {:?} reports empty dimensions; no picture was decoded",
            path.display()
        ));
    }
    Ok((w, h))
}

/// A decoded still picture with straight (non-premultiplied) alpha:
/// four bytes per pixel, rows with no padding.
#[derive(Debug, Clone)]
pub struct RgbaPicture {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

/// PNG dimensions from the `IHDR` chunk: the 8-byte signature, then the
/// length (13), the `IHDR` type, and the big-endian width and height.
/// `None` for anything that is not a PNG with a readable header.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[0..8] != SIG {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (w > 0 && h > 0).then_some((w, h))
}

/// Decode in-memory encoded picture bytes (a PNG graphic extracted from
/// an `sbix` strike, an emoji sheet) to straight RGBA with `ffmpeg_bin`.
///
/// The bytes travel over the encoder's stdin (`-i -`): no temporary
/// file, no linked decoder. This is the frx7 replacement — the old
/// engine decoded font-embedded pictures with a linked media stack;
/// this system decodes them with the same pinned-or-host FFmpeg binary
/// that encodes the MP4. A refusal names `label` (the character or the
/// face the bytes came from), never a blank picture.
pub fn decode_memory(ffmpeg_bin: &Path, bytes: &[u8], label: &str) -> Result<RgbaPicture, String> {
    let (width, height) = png_size(bytes).ok_or_else(|| {
        format!("picture {label:?} is not a PNG with a readable header; no picture was decoded")
    })?;
    let mut child = std::process::Command::new(ffmpeg_bin)
        .args(["-v", "error", "-i", "-", "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("picture {label:?} could not be decoded (ffmpeg stage): {e}"))?;
    let write_out = child
        .stdin
        .take()
        .map(|mut stdin| {
            use std::io::Write;
            stdin.write_all(bytes).and_then(|_| stdin.flush())
        })
        .unwrap_or(Err(std::io::Error::other("no stdin")));
    if let Err(e) = write_out {
        let _ = child.wait();
        return Err(format!("picture {label:?} could not be fed to ffmpeg (ffmpeg stage): {e}"));
    }
    let out = child.wait_with_output().map_err(|e| {
        format!("picture {label:?} could not be decoded (ffmpeg stage): {e}")
    })?;
    if !out.status.success() {
        let tail = String::from_utf8_lossy(&out.stderr);
        let tail: String = tail.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
        return Err(format!(
            "picture {label:?} is undecodable (ffmpeg stage{})",
            if tail.is_empty() { String::new() } else { format!(": {tail}") }
        ));
    }
    let expect = width as usize * height as usize * 4;
    if out.stdout.len() != expect {
        return Err(format!(
            "picture {label:?} decoded to {} bytes, expected {expect} ({}x{} rgba); no picture was decoded",
            out.stdout.len(),
            width,
            height
        ));
    }
    Ok(RgbaPicture {
        width,
        height,
        bytes: out.stdout,
    })
}

/// Decode `path` to raw RGB24 with `ffmpeg_bin`.
///
/// `ffprobe_bin` must sit next to the resolved FFmpeg (host pair or
/// the pinned tool-cache entry). Dimensions come from ffprobe; pixels
/// come from one decoded frame. The byte length is always exactly
/// `width * height * 3`, so callers never guess a layout.
pub fn decode_image(ffmpeg_bin: &Path, ffprobe_bin: &Path, path: &Path) -> Result<Picture, String> {
    if !path.is_file() {
        return Err(format!(
            "picture {:?} does not exist or is not readable; no picture was decoded",
            path.display()
        ));
    }
    let (width, height) = probe_size(ffprobe_bin, path)?;
    let out = std::process::Command::new(ffmpeg_bin)
        .args([
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .map_err(|e| {
            format!(
                "picture {:?} could not be decoded (ffmpeg stage): {e}",
                path.display()
            )
        })?;
    if !out.status.success() {
        let tail = String::from_utf8_lossy(&out.stderr);
        let tail: String = tail.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
        return Err(format!(
            "picture {:?} is undecodable (ffmpeg stage{})",
            path.display(),
            if tail.is_empty() { String::new() } else { format!(": {tail}") }
        ));
    }
    let expect = width as usize * height as usize * 3;
    if out.stdout.len() != expect {
        return Err(format!(
            "picture {:?} decoded to {} bytes, expected {expect} ({}x{} rgb24); no picture was decoded",
            path.display(),
            out.stdout.len(),
            width,
            height
        ));
    }
    Ok(Picture {
        width,
        height,
        bytes: out.stdout,
    })
}
