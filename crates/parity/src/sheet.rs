//! The side-by-side sheet: every sampled frame from both videos, with its frame number.
//!
//! The installed `ffmpeg` may lack `drawtext`, so the numbers are drawn here with a small built-in
//! bitmap font, and `ffmpeg` encodes the finished picture as a PNG file.

use measure::Frame;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// One pair of frames and what to print above it.
pub struct SheetEntry<'a> {
    pub frame: usize,
    pub ssim: f64,
    pub ours: &'a Frame,
    pub theirs: &'a Frame,
}

/// Width of each thumbnail on the sheet.
pub const THUMB_WIDTH: u32 = 480;
const GAP: u32 = 6;
const MARGIN: u32 = 10;
const LABEL_SCALE: u32 = 3;
const LABEL_HEIGHT: u32 = 7 * LABEL_SCALE + 8;
const COLUMNS: u32 = 2;
const BACKGROUND: [u8; 3] = [0x30, 0x30, 0x34];
const TEXT: [u8; 3] = [0xf0, 0xf0, 0xf0];

/// A drawing surface of 8-bit RGB.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32, colour: [u8; 3]) -> Canvas {
        Canvas {
            width,
            height,
            rgb: colour.iter().copied().cycle().take((width * height * 3) as usize).collect(),
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let at = ((y * self.width + x) * 3) as usize;
        [self.rgb[at], self.rgb[at + 1], self.rgb[at + 2]]
    }

    fn put(&mut self, x: u32, y: u32, colour: [u8; 3]) {
        if x < self.width && y < self.height {
            let at = ((y * self.width + x) * 3) as usize;
            self.rgb[at..at + 3].copy_from_slice(&colour);
        }
    }
}

/// The 5x7 shape of a character, one string per row; `None` for a character the font lacks.
fn glyph(c: char) -> Option<[&'static str; 7]> {
    Some(match c.to_ascii_uppercase() {
        ' ' => ["00000"; 7],
        '.' => ["00000", "00000", "00000", "00000", "00000", "00000", "00100"],
        '0' => ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
        '1' => ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
        '2' => ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
        '3' => ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
        '4' => ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
        '5' => ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
        '6' => ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
        '7' => ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
        '8' => ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
        '9' => ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
        'A' => ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
        'C' => ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
        'E' => ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
        'F' => ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
        'G' => ["01110", "10001", "10000", "10111", "10001", "10001", "01111"],
        'H' => ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
        'I' => ["01110", "00100", "00100", "00100", "00100", "00100", "01110"],
        'L' => ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
        'M' => ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
        'N' => ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
        'O' => ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
        'R' => ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
        'S' => ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
        'T' => ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
        'U' => ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
        _ => return None,
    })
}

/// Draws `text` with its top left corner at `(x, y)`; each font pixel is `scale` pixels square.
/// Returns the width drawn.
pub fn draw_text(canvas: &mut Canvas, x: u32, y: u32, text: &str, scale: u32, colour: [u8; 3]) -> u32 {
    let mut at = x;
    for c in text.chars() {
        if let Some(rows) = glyph(c) {
            for (row, bits) in rows.iter().enumerate() {
                for (col, bit) in bits.bytes().enumerate() {
                    if bit == b'1' {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                canvas.put(at + col as u32 * scale + dx, y + row as u32 * scale + dy, colour);
                            }
                        }
                    }
                }
            }
        }
        at += 6 * scale;
    }
    at - x
}

/// Shrinks a frame to `width` pixels across by averaging the source pixels under each new pixel.
fn shrink(frame: &Frame, width: u32) -> Canvas {
    let height = (frame.height * width).div_ceil(frame.width).max(1);
    let mut out = Canvas::new(width, height, [0, 0, 0]);
    for y in 0..height {
        let (y0, y1) = (
            y * frame.height / height,
            ((y + 1) * frame.height).div_ceil(height).min(frame.height),
        );
        for x in 0..width {
            let (x0, x1) = (x * frame.width / width, ((x + 1) * frame.width).div_ceil(width).min(frame.width));
            let (mut sum, mut n) = ([0u32; 3], 0u32);
            for sy in y0..y1.max(y0 + 1) {
                for sx in x0..x1.max(x0 + 1) {
                    let p = frame.pixel(sx.min(frame.width - 1), sy.min(frame.height - 1));
                    for k in 0..3 {
                        sum[k] += u32::from(p[k]);
                    }
                    n += 1;
                }
            }
            out.put(x, y, [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]);
        }
    }
    out
}

fn blit(target: &mut Canvas, source: &Canvas, left: u32, top: u32) {
    for y in 0..source.height {
        for x in 0..source.width {
            target.put(left + x, top + y, source.pixel(x, y));
        }
    }
}

/// Where everything goes on the sheet.
pub struct Layout {
    /// The `(left, top)` of each pair's cell.
    pub spots: Vec<(u32, u32)>,
    /// The size of one cell.
    pub cell: (u32, u32),
    /// The size of the whole sheet.
    pub size: (u32, u32),
}

/// The layout for `count` pairs whose thumbnails are `thumb_height` tall. The first row is a legend.
pub fn layout(count: usize, thumb_height: u32) -> Layout {
    let cell = (THUMB_WIDTH * 2 + GAP, LABEL_HEIGHT + thumb_height);
    let rows = (count as u32).div_ceil(COLUMNS);
    let legend = LABEL_HEIGHT + 4;
    let mut spots = Vec::new();
    for i in 0..count as u32 {
        let (col, row) = (i % COLUMNS, i / COLUMNS);
        spots.push((MARGIN + col * (cell.0 + MARGIN), legend + MARGIN + row * (cell.1 + MARGIN)));
    }
    let size = (MARGIN + COLUMNS * (cell.0 + MARGIN), legend + MARGIN + rows * (cell.1 + MARGIN));
    Layout { spots, cell, size }
}

/// Draws the sheet without saving it.
pub fn draw(entries: &[SheetEntry]) -> Canvas {
    let thumb_height = entries.first().map_or(1, |e| (e.ours.height * THUMB_WIDTH).div_ceil(e.ours.width));
    let Layout {
        spots,
        size: (width, height),
        ..
    } = layout(entries.len(), thumb_height);
    let mut canvas = Canvas::new(width, height, BACKGROUND);
    draw_text(&mut canvas, MARGIN, MARGIN, "LEFT OURS   RIGHT REFERENCE", LABEL_SCALE, TEXT);
    for (entry, &(left, top)) in entries.iter().zip(&spots) {
        let label = format!("FRAME {}   SSIM {:.4}", entry.frame, entry.ssim);
        draw_text(&mut canvas, left, top + 4, &label, LABEL_SCALE, TEXT);
        let y = top + LABEL_HEIGHT;
        blit(&mut canvas, &shrink(entry.ours, THUMB_WIDTH), left, y);
        blit(&mut canvas, &shrink(entry.theirs, THUMB_WIDTH), left + THUMB_WIDTH + GAP, y);
    }
    canvas
}

/// Writes one PNG sheet; `ffmpeg` does the encoding.
pub fn write(ffmpeg: &Path, entries: &[SheetEntry], path: &Path) -> Result<(), String> {
    if entries.is_empty() {
        return Err("there is nothing to put on the sheet".into());
    }
    let canvas = draw(entries);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{} could not be made ({e})", parent.display()))?;
    }
    let mut child = Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s"])
        .arg(format!("{}x{}", canvas.width, canvas.height))
        .args(["-i", "-", "-frames:v", "1"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{} could not be run: {e}", ffmpeg.display()))?;
    child
        .stdin
        .take()
        .ok_or("ffmpeg gave no input channel")?
        .write_all(&canvas.rgb)
        .map_err(|e| format!("the sheet could not be sent to ffmpeg ({e})"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("ffmpeg could not be waited for ({e})"))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg could not write the sheet: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}
