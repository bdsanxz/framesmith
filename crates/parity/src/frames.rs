//! Reading the frames of a video one at a time, so a long video never has to sit in memory whole.

use measure::Frame;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

/// Decodes the first video stream (chosen by type) with the same colour handling as `measure`,
/// and calls `visit` with the number (from 0) and picture of each frame. Returns how many frames there were.
pub fn each_frame(ffmpeg: &Path, video: &Path, width: u32, height: u32, mut visit: impl FnMut(usize, Frame)) -> Result<usize, String> {
    let mut child = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(video)
        .args([
            "-map",
            "0:v:0",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-sws_flags",
            "accurate_rnd+full_chroma_int",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{} could not be run: {e}", ffmpeg.display()))?;
    let mut stdout = child.stdout.take().ok_or("ffmpeg gave no output channel")?;
    let size = (width as usize) * (height as usize) * 3;
    if size == 0 {
        return Err("the picture size is zero".into());
    }
    let mut count = 0usize;
    loop {
        let mut rgb = vec![0u8; size];
        let mut filled = 0;
        while filled < size {
            let n = stdout
                .read(&mut rgb[filled..])
                .map_err(|e| format!("ffmpeg's output could not be read ({e})"))?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        if filled < size {
            let _ = child.kill();
            return Err(format!("ffmpeg ended in the middle of frame {count} of {}", video.display()));
        }
        visit(count, Frame { width, height, rgb });
        count += 1;
    }
    let status = child.wait().map_err(|e| format!("ffmpeg could not be waited for ({e})"))?;
    if !status.success() {
        return Err(format!("ffmpeg could not decode {}", video.display()));
    }
    Ok(count)
}

/// Parses a colour written as `RRGGBB` or `#RRGGBB`.
pub fn parse_colour(text: &str) -> Result<[u8; 3], String> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("'{text}' is not a colour written as RRGGBB"));
    }
    let part = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).map_err(|e| e.to_string());
    Ok([part(0)?, part(2)?, part(4)?])
}
