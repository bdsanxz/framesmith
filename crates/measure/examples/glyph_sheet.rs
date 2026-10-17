//! Draws every glyph of one face of the test font large, on one sheet, from the font file read back
//! with skrifa. A test-only rasterizer: each outline is flattened, and each pixel is covered by a
//! four-by-four grid of samples under the non-zero winding rule. The cell of a glyph is its glyph
//! index, read row by row, twelve cells to a row. Usage:
//!
//!     cargo run -p measure --example glyph_sheet -- <font file> <sheet.png>
//!
//! It prints the facts read from the font and the character of each cell, so that the sheet can be
//! checked against `docs/spec/test-font.md`, section 2.

use measure::font::mapped_characters;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{LocationRef, Size};
use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};

const COLUMNS: usize = 12;
const ROWS: usize = 8;
const CELL: usize = 180;
const WIDTH: usize = COLUMNS * CELL;
const HEIGHT: usize = ROWS * CELL;
/// The baseline of a cell, in pixels from its top.
const BASELINE: usize = 130;
/// The pen of a glyph starts this far in from the left of its cell.
const INSET: usize = 10;
/// Pixels per unit of the em: 1000 units are 150 pixels.
const SCALE: f64 = 0.15;
/// Samples across each pixel, in each direction.
const SAMPLES: usize = 4;
/// Straight pieces per curve of the flattened outline.
const STEPS: usize = 8;

/// An edge of the outline in sheet pixels, from one point to the next.
type Edge = ((f64, f64), (f64, f64));

/// The edges of one glyph, drawn with its origin at a point of the sheet.
struct Edges {
    edges: Vec<Edge>,
    start: (f64, f64),
    last: (f64, f64),
    origin: (f64, f64),
}

impl Edges {
    fn new(origin: (f64, f64)) -> Self {
        Edges {
            edges: Vec::new(),
            start: origin,
            last: origin,
            origin,
        }
    }

    /// A point of the font in sheet pixels: x to the right, y down from the top of the sheet.
    fn at(&self, x: f32, y: f32) -> (f64, f64) {
        (self.origin.0 + f64::from(x) * SCALE, self.origin.1 - f64::from(y) * SCALE)
    }

    fn segment(&mut self, to: (f64, f64)) {
        self.edges.push((self.last, to));
        self.last = to;
    }
}

impl OutlinePen for Edges {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = self.at(x, y);
        self.last = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.at(x, y);
        self.segment(to);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (from, control, to) = (self.last, self.at(cx0, cy0), self.at(x, y));
        for step in 1..=STEPS {
            let t = step as f64 / STEPS as f64;
            let u = 1.0 - t;
            let point = (
                u * u * from.0 + 2.0 * u * t * control.0 + t * t * to.0,
                u * u * from.1 + 2.0 * u * t * control.1 + t * t * to.1,
            );
            self.segment(point);
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (from, c0, c1, to) = (self.last, self.at(cx0, cy0), self.at(cx1, cy1), self.at(x, y));
        for step in 1..=STEPS {
            let t = step as f64 / STEPS as f64;
            let u = 1.0 - t;
            let point = (
                u * u * u * from.0 + 3.0 * u * u * t * c0.0 + 3.0 * u * t * t * c1.0 + t * t * t * to.0,
                u * u * u * from.1 + 3.0 * u * u * t * c0.1 + 3.0 * u * t * t * c1.1 + t * t * t * to.1,
            );
            self.segment(point);
        }
    }

    fn close(&mut self) {
        let start = self.start;
        self.segment(start);
    }
}

/// Adds one to the cover of each sample of a row of pixels whose x lies from `from` to `to`.
/// `left` is the sheet column of the cell's first pixel, and the row holds the cell's pixels only.
fn fill(row: &mut [u8], left: usize, from: f64, to: f64) {
    let samples = SAMPLES as f64;
    let first = (from * samples - 0.5).ceil() as i64;
    let end = (to * samples - 0.5).ceil() as i64;
    let low = (left * SAMPLES) as i64;
    let high = ((left + CELL) * SAMPLES) as i64;
    for sample in first.max(low)..end.min(high) {
        let pixel = sample.div_euclid(SAMPLES as i64) - left as i64;
        row[pixel as usize] += 1;
    }
}

/// The cover of each pixel of a cell, from 0 (outside) to 16 (every sample inside).
fn cover(edges: &[Edge], left: usize, top: usize) -> Vec<u8> {
    let mut cover = vec![0u8; CELL * CELL];
    for row in 0..CELL {
        for sub in 0..SAMPLES {
            let y = (top + row) as f64 + (sub as f64 + 0.5) / SAMPLES as f64;
            let mut crossings: Vec<(f64, i32)> = edges
                .iter()
                .filter_map(|&((x0, y0), (x1, y1))| {
                    let crosses = (y0 <= y && y < y1) || (y1 <= y && y < y0);
                    crosses.then(|| (x0 + (y - y0) * (x1 - x0) / (y1 - y0), if y1 > y0 { 1 } else { -1 }))
                })
                .collect();
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            let line = &mut cover[row * CELL..(row + 1) * CELL];
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    fill(line, left, pair[0].0, pair[1].0);
                }
            }
        }
    }
    cover
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend(kind);
    out.extend(data);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

/// A grayscale PNG with stored (uncompressed) deflate blocks, so that no compression code is needed.
fn png(width: usize, height: usize, gray: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width + 1) * height);
    for line in gray.chunks(width) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut header = Vec::new();
    header.extend((width as u32).to_be_bytes());
    header.extend((height as u32).to_be_bytes());
    header.extend([8, 0, 0, 0, 0]);
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend(len.to_le_bytes());
        zlib.extend((!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend(adler32(&raw).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn fail(message: &str) -> ! {
    eprintln!("glyph_sheet: {message}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [font_path, sheet_path] = args.as_slice() else {
        eprintln!("usage: glyph_sheet <font file> <sheet.png>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(font_path).unwrap_or_else(|error| fail(&format!("{font_path}: {error}")));
    let font = FontRef::new(&bytes).unwrap_or_else(|error| fail(&format!("{font_path}: {error:?}")));

    let name = |id: StringId| {
        font.localized_strings(id)
            .english_or_first()
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    println!("family: {}", name(StringId::FAMILY_NAME));
    println!("style: {}", name(StringId::SUBFAMILY_NAME));
    println!("weight: {}", font.attributes().weight.value());
    let glyphs = font.maxp().map(|maxp| maxp.num_glyphs()).unwrap_or(0);
    println!("glyphs: {glyphs}");

    let charmap = font.charmap();
    for (index, character) in mapped_characters().enumerate() {
        let expected = GlyphId::new(index as u32 + 1);
        if charmap.map(character) != Some(expected) {
            fail(&format!("U+{:04X} does not map to glyph {}", character as u32, index + 1));
        }
    }
    println!("cmap: U+0020 to U+007E map to glyphs 1 to 95, as the spec says");

    let mut gray = vec![255u8; WIDTH * HEIGHT];
    for glyph in 0..COLUMNS * ROWS {
        let (column, row) = (glyph % COLUMNS, glyph / COLUMNS);
        let (left, top) = (column * CELL, row * CELL);
        let label = match glyph {
            0 => ".notdef".to_string(),
            1 => "space".to_string(),
            _ => format!("U+{:04X} {}", glyph + 0x1F, char::from((glyph + 0x1F) as u8)),
        };
        println!("cell {glyph} (row {row}, column {column}): glyph {glyph}, {label}");

        for x in left..left + CELL {
            gray[top * WIDTH + x] = 200;
            gray[(top + CELL - 1) * WIDTH + x] = 200;
            gray[(top + BASELINE) * WIDTH + x] = 225;
        }
        for y in top..top + CELL {
            gray[y * WIDTH + left] = 200;
            gray[y * WIDTH + left + CELL - 1] = 200;
        }

        let origin = ((left + INSET) as f64, (top + BASELINE) as f64);
        let mut edges = Edges::new(origin);
        if let Some(outline) = font.outline_glyphs().get(GlyphId::new(glyph as u32)) {
            if let Err(error) = outline.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut edges) {
                fail(&format!("glyph {glyph} does not draw: {error:?}"));
            }
        }
        let cover = cover(&edges.edges, left, top);
        for (index, count) in cover.iter().enumerate() {
            let pixel = &mut gray[(top + index / CELL) * WIDTH + left + index % CELL];
            let shade = 255 - u32::from(*count) * 255 / (SAMPLES * SAMPLES) as u32;
            *pixel = (*pixel).min(shade as u8);
        }
    }

    let sheet = png(WIDTH, HEIGHT, &gray);
    if let Err(error) = std::fs::write(sheet_path, &sheet) {
        fail(&format!("{sheet_path}: {error}"));
    }
    println!(
        "sheet {sheet_path}: {WIDTH}x{HEIGHT} pixels, {COLUMNS} by {ROWS} cells of {CELL} pixels, {} bytes",
        sheet.len()
    );
}
