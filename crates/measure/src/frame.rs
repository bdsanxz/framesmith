//! Frames as plain pixels.

use std::path::Path;
use std::process::{Command, Stdio};

pub type Rgb = [u8; 3];

/// One picture: 8-bit RGB, row by row from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// The pixels a test matched: the edges are the first and last matching
/// column and row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Bounds {
    pub fn width(&self) -> u32 {
        self.right - self.left + 1
    }

    pub fn height(&self) -> u32 {
        self.bottom - self.top + 1
    }
}

fn near(a: Rgb, b: Rgb, tolerance: u8) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= tolerance)
}

impl Frame {
    pub fn pixel(&self, x: u32, y: u32) -> Rgb {
        let at = ((y * self.width + x) * 3) as usize;
        [self.rgb[at], self.rgb[at + 1], self.rgb[at + 2]]
    }

    /// The part of the picture inside a rectangle, as its own frame.
    pub fn crop(&self, left: u32, top: u32, width: u32, height: u32) -> Frame {
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for y in top..top + height {
            let from = ((y * self.width + left) * 3) as usize;
            rgb.extend_from_slice(&self.rgb[from..from + (width * 3) as usize]);
        }
        Frame { width, height, rgb }
    }

    /// A hash of the size and every pixel value.
    pub fn hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        self.width
            .to_le_bytes()
            .into_iter()
            .chain(self.height.to_le_bytes())
            .for_each(&mut mix);
        self.rgb.iter().copied().for_each(&mut mix);
        hash
    }

    /// The box around every pixel within `tolerance` of `colour` on each channel.
    pub fn bounds(&self, colour: Rgb, tolerance: u8) -> Option<Bounds> {
        let mut found: Option<Bounds> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if near(self.pixel(x, y), colour, tolerance) {
                    let b = found.get_or_insert(Bounds {
                        left: x,
                        top: y,
                        right: x,
                        bottom: y,
                    });
                    b.left = b.left.min(x);
                    b.right = b.right.max(x);
                    b.top = b.top.min(y);
                    b.bottom = b.bottom.max(y);
                }
            }
        }
        found
    }

    /// How many pixels are within `tolerance` of `colour`.
    pub fn count(&self, colour: Rgb, tolerance: u8) -> usize {
        self.rgb
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|&&pixel| near(pixel, colour, tolerance))
            .count()
    }

    /// Whether every pixel is within `tolerance` of `colour`.
    pub fn is_all(&self, colour: Rgb, tolerance: u8) -> bool {
        self.count(colour, tolerance) == (self.width * self.height) as usize
    }

    /// The first pixel that differs from the other frame, as `(x, y)`.
    pub fn first_pixel_difference(&self, other: &Frame) -> Option<(u32, u32)> {
        if (self.width, self.height) != (other.width, other.height) {
            return Some((0, 0));
        }
        let at = self
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(other.rgb.as_chunks::<3>().0)
            .position(|(a, b)| a != b)? as u32;
        Some((at % self.width, at / self.width))
    }
}

/// The number of the first frame whose hash differs between two runs, or
/// the shorter length when one run has fewer frames.
pub fn first_difference(first: &[u64], second: &[u64]) -> Option<usize> {
    first
        .iter()
        .zip(second)
        .position(|(a, b)| a != b)
        .or_else(|| (first.len() != second.len()).then(|| first.len().min(second.len())))
}

/// Compares two runs frame for frame, by a hash of each frame's pixels.
/// The error names the first frame that differs.
pub fn same_frames(first: &[Frame], second: &[Frame]) -> Result<(), String> {
    let hashes = |frames: &[Frame]| frames.iter().map(Frame::hash).collect::<Vec<u64>>();
    let Some(frame) = first_difference(&hashes(first), &hashes(second)) else {
        return Ok(());
    };
    match (first.get(frame), second.get(frame)) {
        (Some(a), Some(b)) => match a.first_pixel_difference(b) {
            Some((x, y)) if (a.width, a.height) == (b.width, b.height) => {
                Err(format!("frame {frame} differs: its first differing pixel is at ({x}, {y})"))
            }
            _ => Err(format!(
                "frame {frame} differs: it is {}x{} in one run and {}x{} in the other",
                a.width, a.height, b.width, b.height
            )),
        },
        _ => Err(format!(
            "frame {frame} is missing from one run: the runs have {} and {} frames",
            first.len(),
            second.len()
        )),
    }
}

/// The width and height written in a PNG file's header.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let number = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    Some((number(16)?, number(20)?))
}

fn decode(ffmpeg: &Path, input: &[&str], width: u32, height: u32) -> Result<Vec<Frame>, String> {
    let output = Command::new(ffmpeg)
        .args(["-v", "error"])
        .args(input)
        .args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-sws_flags",
            "accurate_rnd+full_chroma_int",
            "-",
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{} could not be run: {e}", ffmpeg.display()))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg could not read the frames: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let size = (width * height * 3) as usize;
    if size == 0 || output.stdout.len() % size != 0 {
        return Err(format!(
            "ffmpeg gave {} bytes, which is not a whole number of {width}x{height} frames",
            output.stdout.len()
        ));
    }
    Ok(output
        .stdout
        .chunks_exact(size)
        .map(|rgb| Frame {
            width,
            height,
            rgb: rgb.to_vec(),
        })
        .collect())
}

/// Reads the pictures `frame-00000.png`, `frame-00001.png`, … of a folder,
/// in order, with the pixels exactly as the files hold them.
pub fn frames_from_pngs(ffmpeg: &Path, dir: &Path) -> Result<Vec<Frame>, String> {
    let first = std::fs::read(dir.join("frame-00000.png")).map_err(|e| format!("{} has no frame-00000.png ({e})", dir.display()))?;
    let (width, height) = png_size(&first).ok_or("frame-00000.png is not a PNG file")?;
    let pattern = dir.join("frame-%05d.png");
    decode(
        ffmpeg,
        &["-f", "image2", "-start_number", "0", "-i", &pattern.to_string_lossy()],
        width,
        height,
    )
}

/// Decodes every frame of a video's first video stream.
pub fn frames_from_video(ffmpeg: &Path, video: &Path, width: u32, height: u32) -> Result<Vec<Frame>, String> {
    decode(
        ffmpeg,
        &["-i", &video.to_string_lossy(), "-map", "0:v:0", "-fps_mode", "passthrough"],
        width,
        height,
    )
}
