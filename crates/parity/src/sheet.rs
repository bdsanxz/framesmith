//! The sheet: for each frame listed as low, ours, the reference's and the pixels that differ, side
//! by side, with the frame number and the score.
//!
//! The numbers are drawn here with a small built-in bitmap font and the picture is saved as a PNG
//! file by [`crate::png`], so that no program is started to make the sheet.

use measure::Frame;
use std::path::Path;

/// One frame to put on the sheet.
pub struct SheetEntry<'a> {
    pub frame: u64,
    /// The score written as `87.500`.
    pub score: String,
    /// `fail` or `excused`.
    pub state: &'a str,
    pub ours: &'a Frame,
    pub theirs: &'a Frame,
    /// Where the two differ, row by row.
    pub mask: &'a [bool],
}

/// Width of each thumbnail on the sheet.
pub const THUMB_WIDTH: u32 = 256;
const GAP: u32 = 6;
const MARGIN: u32 = 10;
const LABEL_SCALE: u32 = 2;
const LABEL_HEIGHT: u32 = 7 * LABEL_SCALE + 8;
const BACKGROUND: [u8; 3] = [0x30, 0x30, 0x34];
const TEXT: [u8; 3] = [0xf0, 0xf0, 0xf0];
/// The colour of a pixel that differs on the third picture.
pub const MARK: [u8; 3] = [0xff, 0x20, 0x20];

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
        '%' => ["11001", "11010", "00010", "00100", "01000", "01011", "10011"],
        '-' => ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
        ':' => ["00000", "00100", "00100", "00000", "00100", "00100", "00000"],
        'A' => ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
        'C' => ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
        'D' => ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
        'E' => ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
        'F' => ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
        'G' => ["01110", "10001", "10000", "10111", "10001", "10001", "01111"],
        'H' => ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
        'I' => ["01110", "00100", "00100", "00100", "00100", "00100", "01110"],
        'K' => ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
        'L' => ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
        'M' => ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
        'N' => ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
        'O' => ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
        'P' => ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
        'R' => ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
        'S' => ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
        'T' => ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
        'U' => ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
        'V' => ["10001", "10001", "10001", "10001", "10001", "01010", "00100"],
        'X' => ["10001", "10001", "01010", "00100", "01010", "10001", "10001"],
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
    /// The `(left, top)` of each entry's row, label included.
    pub spots: Vec<(u32, u32)>,
    /// The size of the whole sheet.
    pub size: (u32, u32),
}

/// The width of one row: three thumbnails with a gap between.
const ROW_WIDTH: u32 = THUMB_WIDTH * 3 + GAP * 2;

/// The layout for `count` entries whose thumbnails are `thumb_height` tall, one row each under a
/// row that names the three pictures.
pub fn layout(count: usize, thumb_height: u32) -> Layout {
    let legend = LABEL_HEIGHT;
    let row = LABEL_HEIGHT + thumb_height + MARGIN;
    let spots = (0..count as u32).map(|i| (MARGIN, MARGIN + legend + MARGIN + i * row)).collect();
    Layout {
        spots,
        size: (ROW_WIDTH + 2 * MARGIN, MARGIN + legend + MARGIN + count as u32 * row),
    }
}

/// Shrinks the third picture: a thumbnail pixel is marked when any pixel under it differs, so that a
/// small difference does not fade away; the others show ours in pale grey.
fn shrink_marked(frame: &Frame, mask: &[bool], width: u32) -> Canvas {
    let height = (frame.height * width).div_ceil(frame.width).max(1);
    let mut out = Canvas::new(width, height, [0, 0, 0]);
    for y in 0..height {
        let (y0, y1) = (
            y * frame.height / height,
            (((y + 1) * frame.height).div_ceil(height)).min(frame.height),
        );
        for x in 0..width {
            let (x0, x1) = (x * frame.width / width, (((x + 1) * frame.width).div_ceil(width)).min(frame.width));
            let (mut marked, mut luma, mut n) = (false, 0u32, 0u32);
            for sy in y0..y1.max(y0 + 1) {
                for sx in x0..x1.max(x0 + 1) {
                    let (sx, sy) = (sx.min(frame.width - 1), sy.min(frame.height - 1));
                    marked |= mask.get((sy * frame.width + sx) as usize).copied().unwrap_or(false);
                    let p = frame.pixel(sx, sy);
                    luma += (u32::from(p[0]) * 3 + u32::from(p[1]) * 6 + u32::from(p[2])) / 10;
                    n += 1;
                }
            }
            let pale = 200 + (luma / n.max(1)) / 5;
            out.put(x, y, if marked { MARK } else { [pale as u8; 3] });
        }
    }
    out
}

/// Draws the sheet without saving it.
pub fn draw(entries: &[SheetEntry]) -> Canvas {
    let thumb_height = entries.first().map_or(1, |e| (e.ours.height * THUMB_WIDTH).div_ceil(e.ours.width));
    let Layout {
        spots,
        size: (width, height),
    } = layout(entries.len(), thumb_height);
    let mut canvas = Canvas::new(width, height, BACKGROUND);
    for (at, name) in ["OURS", "REFERENCE", "DIFFERENT PIXELS"].iter().enumerate() {
        draw_text(
            &mut canvas,
            MARGIN + at as u32 * (THUMB_WIDTH + GAP),
            MARGIN + 4,
            name,
            LABEL_SCALE,
            TEXT,
        );
    }
    for (entry, &(left, top)) in entries.iter().zip(&spots) {
        let label = format!("FRAME {}  SCORE {} %  {}", entry.frame, entry.score, entry.state);
        draw_text(&mut canvas, left, top + 4, &label, LABEL_SCALE, TEXT);
        let y = top + LABEL_HEIGHT;
        blit(&mut canvas, &shrink(entry.ours, THUMB_WIDTH), left, y);
        blit(&mut canvas, &shrink(entry.theirs, THUMB_WIDTH), left + THUMB_WIDTH + GAP, y);
        blit(
            &mut canvas,
            &shrink_marked(entry.ours, entry.mask, THUMB_WIDTH),
            left + 2 * (THUMB_WIDTH + GAP),
            y,
        );
    }
    canvas
}

/// Writes one PNG sheet, making the folder it goes in.
pub fn write(entries: &[SheetEntry], path: &Path) -> Result<(), String> {
    if entries.is_empty() {
        return Err("there is nothing to put on the sheet".into());
    }
    let canvas = draw(entries);
    let bytes = crate::png::encode(canvas.width, canvas.height, &canvas.rgb)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{} could not be made ({e})", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("the sheet {} could not be written ({e})", path.display()))
}
