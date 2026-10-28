//! The TrueType file of a face, as `docs/spec/test-font.md`, sections 1 and 5 set out: the tables
//! `GSUB`, `OS/2`, `cmap`, `glyf`, `head`, `hhea`, `hmtx`, `loca`, `maxp`, `name` and `post`, in tag order.
//! Every number is written from the design and the OpenType specification; nothing is read from a font.

use super::design::{self, Piece};
use super::layout;
use super::outline::{self, Point};
use super::{Face, FAMILY};

const UNITS_PER_EM: u16 = 1000;
/// The advance of U+0020, which has no outline.
const SPACE_ADVANCE: u16 = 300;
/// The leftmost point of a glyph's centreline is moved to this x.
const LEFT_EDGE: f64 = 100.0;
/// The advance of a glyph is the width of its centreline box plus this.
const SIDE_GAP: f64 = 240.0;
/// The advance of every tabular figure: the advance of the 8 of the lining figures, which is 320 units of centreline
/// box and the side gaps (`docs/spec/test-font.md`, section 7).
const TABULAR_ADVANCE: u16 = 560;
const ASCENDER: i16 = 850;
const DESCENDER: i16 = -250;

/// One glyph: its advance and its contours. A glyph with no contours has no outline.
struct Glyph {
    advance: u16,
    contours: Vec<Vec<Point>>,
}

impl Glyph {
    fn points(&self) -> impl Iterator<Item = &Point> {
        self.contours.iter().flatten()
    }

    /// xMin, yMin, xMax and yMax over every point of the glyph, or none when it has no outline.
    fn bounds(&self) -> Option<[i32; 4]> {
        self.points().fold(None, |found, p| match found {
            None => Some([p.x, p.y, p.x, p.y]),
            Some([x0, y0, x1, y1]) => Some([x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)]),
        })
    }
}

/// The glyphs of a face in glyph order: the missing-character glyph, the space, then U+0021 to U+007E, then the ten
/// tabular figures, then the ten old-style figures (`docs/spec/test-font.md`, section 7).
fn glyphs(face: Face) -> Vec<Glyph> {
    let stroke = f64::from(face.stroke());
    let mut glyphs = vec![
        outlined(design::NOTDEF, stroke),
        Glyph {
            advance: SPACE_ADVANCE,
            contours: Vec::new(),
        },
    ];
    glyphs.extend(design::PRINTABLE.iter().map(|(_, pieces)| outlined(pieces, stroke)));
    // The lining figures, which are the printable digits, in the order of the digits.
    let lining: Vec<&[Piece]> = design::PRINTABLE
        .iter()
        .filter(|(character, _)| character.is_ascii_digit())
        .map(|(_, pieces)| *pieces)
        .collect();
    glyphs.extend(lining.iter().map(|pieces| placed(pieces, stroke, TABULAR_ADVANCE)));
    glyphs.extend(
        design::OLDSTYLE
            .iter()
            .zip(&lining)
            .map(|(pieces, figure)| placed(pieces, stroke, outlined(figure, stroke).advance)),
    );
    glyphs.push(corners());
    glyphs
}

/// The corner glyph: each triangle of `design::CORNERS` as one clockwise contour, moved to the left edge like a lining figure's
/// centreline box. Its advance is that of a glyph of the same width.
fn corners() -> Glyph {
    let contours = design::CORNERS
        .iter()
        .map(|triangle| {
            triangle
                .iter()
                .map(|&(x, y)| Point {
                    x: x + LEFT_EDGE as i32,
                    y,
                    on_curve: true,
                })
                .collect()
        })
        .collect();
    Glyph {
        advance: (957.0 + SIDE_GAP) as u16,
        contours,
    }
}

fn outlined(pieces: &[Piece], stroke: f64) -> Glyph {
    let (lo, hi) = outline::centreline_span(pieces);
    let advance = ((hi - lo).round() + SIDE_GAP) as u16;
    Glyph {
        advance,
        contours: outline::contours(pieces, stroke, LEFT_EDGE - lo),
    }
}

/// A glyph of the given advance whose centreline box is centred in the room the advance leaves, that is the advance less
/// the side gaps. A figure of a variant is drawn this way; the room of a lining figure is its own centreline box.
fn placed(pieces: &[Piece], stroke: f64, advance: u16) -> Glyph {
    let (lo, hi) = outline::centreline_span(pieces);
    let room = f64::from(advance) - SIDE_GAP;
    let left = LEFT_EDGE + (room - (hi - lo)) / 2.0;
    Glyph {
        advance,
        contours: outline::contours(pieces, stroke, left - lo),
    }
}

/// The whole TrueType file of a face. The same bytes on every run.
pub fn build(face: Face) -> Vec<u8> {
    let glyphs = glyphs(face);
    let (glyf, loca) = glyf_and_loca(&glyphs);
    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"GSUB", layout::gsub(&layout::figure_features())),
        (*b"OS/2", os2(face, &glyphs)),
        (*b"cmap", cmap()),
        (*b"glyf", glyf),
        (*b"head", head(face, &glyphs)),
        (*b"hhea", hhea(&glyphs)),
        (*b"hmtx", hmtx(&glyphs)),
        (*b"loca", loca),
        (*b"maxp", maxp(&glyphs)),
        (*b"name", name(face)),
        (*b"post", post()),
    ];
    tables.sort_by_key(|(tag, _)| *tag);

    let count = tables.len() as u16;
    let entry_selector = count.ilog2() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let mut file = Out::default();
    file.u32(0x0001_0000);
    file.u16(count);
    file.u16(search_range);
    file.u16(entry_selector);
    file.u16(count * 16 - search_range);

    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    let mut head_at = 0;
    for (tag, data) in &tables {
        file.bytes(tag);
        file.u32(checksum(data));
        file.u32(offset as u32);
        file.u32(data.len() as u32);
        if tag == b"head" {
            head_at = offset;
        }
        let block = padded(data.clone());
        offset += block.len();
        body.extend(block);
    }
    let mut bytes = file.finish();
    bytes.extend(body);
    // The head table holds the adjustment that makes the sum of the whole file come to 0xB1B0AFBA.
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&bytes));
    bytes[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
    bytes
}

/// Big-endian values, appended in order.
#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u16(&mut self, value: u16) {
        self.0.extend(value.to_be_bytes());
    }

    fn i16(&mut self, value: i16) {
        self.0.extend(value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.extend(value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.0.extend(value.to_be_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// The checksum of a table or of a whole file: the sum of its big-endian 32-bit words.
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn padded(mut data: Vec<u8>) -> Vec<u8> {
    data.resize(data.len().next_multiple_of(4), 0);
    data
}

fn all_bounds(glyphs: &[Glyph]) -> [i32; 4] {
    glyphs
        .iter()
        .filter_map(Glyph::bounds)
        .fold([i32::MAX, i32::MAX, i32::MIN, i32::MIN], |a, b| {
            [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
        })
}

fn head(face: Face, glyphs: &[Glyph]) -> Vec<u8> {
    let [x0, y0, x1, y1] = all_bounds(glyphs);
    let mut out = Out::default();
    out.u32(0x0001_0000);
    out.u32(0x0001_0000);
    out.u32(0);
    out.u32(0x5F0F_3CF5);
    out.u16(0x0001);
    out.u16(UNITS_PER_EM);
    out.i64(0);
    out.i64(0);
    for value in [x0, y0, x1, y1] {
        out.i16(value as i16);
    }
    out.u16(if face == Face::Bold { 0x0001 } else { 0 });
    out.u16(8);
    out.i16(2);
    out.i16(1);
    out.i16(0);
    out.finish()
}

fn hhea(glyphs: &[Glyph]) -> Vec<u8> {
    // (xMin, xMax, advance) of every glyph with an outline.
    let spans: Vec<(i32, i32, i32)> = glyphs
        .iter()
        .filter_map(|g| g.bounds().map(|b| (b[0], b[2], i32::from(g.advance))))
        .collect();
    let advance_max = glyphs.iter().map(|g| g.advance).max().unwrap_or(0);
    let lsb_min = spans.iter().map(|s| s.0).min().unwrap_or(0);
    let rsb_min = spans.iter().map(|s| s.2 - s.1).min().unwrap_or(0);
    let extent = spans.iter().map(|s| s.1).max().unwrap_or(0);
    let mut out = Out::default();
    out.u32(0x0001_0000);
    out.i16(ASCENDER);
    out.i16(DESCENDER);
    out.i16(0);
    out.u16(advance_max);
    out.i16(lsb_min as i16);
    out.i16(rsb_min as i16);
    out.i16(extent as i16);
    out.i16(1);
    out.i16(0);
    out.i16(0);
    for _ in 0..4 {
        out.i16(0);
    }
    out.i16(0);
    out.u16(glyphs.len() as u16);
    out.finish()
}

fn hmtx(glyphs: &[Glyph]) -> Vec<u8> {
    let mut out = Out::default();
    for glyph in glyphs {
        out.u16(glyph.advance);
        out.i16(glyph.bounds().map_or(0, |b| b[0] as i16));
    }
    out.finish()
}

fn maxp(glyphs: &[Glyph]) -> Vec<u8> {
    let points = glyphs.iter().map(|g| g.points().count()).max().unwrap_or(0);
    let contours = glyphs.iter().map(|g| g.contours.len()).max().unwrap_or(0);
    let mut out = Out::default();
    out.u32(0x0001_0000);
    out.u16(glyphs.len() as u16);
    out.u16(points as u16);
    out.u16(contours as u16);
    out.u16(0);
    out.u16(0);
    out.u16(1);
    for _ in 0..8 {
        out.u16(0);
    }
    out.finish()
}

/// The glyph offsets of the long format, one per glyph and one at the end, and the glyph data.
fn glyf_and_loca(glyphs: &[Glyph]) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Vec::new();
    let mut loca = Out::default();
    for glyph in glyphs {
        loca.u32(glyf.len() as u32);
        if let Some(bounds) = glyph.bounds() {
            glyf.extend(padded(simple_glyph(glyph, bounds)));
        }
    }
    loca.u32(glyf.len() as u32);
    (glyf, loca.finish())
}

/// A simple glyph: its contours' last points, no instructions, then the flags and the coordinates.
fn simple_glyph(glyph: &Glyph, bounds: [i32; 4]) -> Vec<u8> {
    let mut out = Out::default();
    out.i16(glyph.contours.len() as i16);
    for value in bounds {
        out.i16(value as i16);
    }
    let mut end = 0;
    for contour in &glyph.contours {
        end += contour.len();
        out.u16((end - 1) as u16);
    }
    out.u16(0);
    let mut flags = Vec::new();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let (mut px, mut py) = (0, 0);
    for point in glyph.points() {
        let mut flag = if point.on_curve { 0x01 } else { 0x00 };
        push_delta(&mut flag, &mut xs, point.x - px, 0x02, 0x10);
        push_delta(&mut flag, &mut ys, point.y - py, 0x04, 0x20);
        px = point.x;
        py = point.y;
        flags.push(flag);
    }
    if glyph.contours.len() > 1 {
        flags[0] |= 0x40;
    }
    out.bytes(&flags);
    out.bytes(&xs);
    out.bytes(&ys);
    out.finish()
}

/// One coordinate change: no byte when it is zero (the flag says "same"), one byte when it is at
/// most 255 in size (the flag says "short", and its sign), two bytes otherwise.
fn push_delta(flag: &mut u8, bytes: &mut Vec<u8>, delta: i32, short: u8, same: u8) {
    if delta == 0 {
        *flag |= same;
    } else if delta.abs() <= 255 {
        *flag |= short;
        if delta > 0 {
            *flag |= same;
        }
        bytes.push(delta.unsigned_abs() as u8);
    } else {
        bytes.extend((delta as i16).to_be_bytes());
    }
}

/// One format 4 subtable mapping U+0020 to U+007E onto glyphs 1 to 95 by a single range.
fn cmap() -> Vec<u8> {
    let mut out = Out::default();
    out.u16(0);
    out.u16(1);
    out.u16(3);
    out.u16(1);
    out.u32(12);
    out.u16(4);
    out.u16(40);
    out.u16(0);
    out.u16(6);
    out.u16(4);
    out.u16(1);
    out.u16(2);
    out.u16(0x7E);
    out.u16(0xE000);
    out.u16(0xFFFF);
    out.u16(0);
    out.u16(0x20);
    out.u16(0xE000);
    out.u16(0xFFFF);
    // The glyph of a code is the code plus this, modulo 65536: U+0020 gives glyph 1, U+E000 gives glyph 116.
    out.u16(0xFFE1);
    out.u16(0x2074);
    out.u16(1);
    out.u16(0);
    out.u16(0);
    out.u16(0);
    out.finish()
}

fn os2(face: Face, glyphs: &[Glyph]) -> Vec<u8> {
    let (x_height, cap_height, selection) = match face {
        Face::Regular => (500, 700, 0x00C0),
        Face::Bold => (530, 730, 0x00A0),
    };
    let advances: Vec<u32> = glyphs.iter().filter(|g| g.advance > 0).map(|g| u32::from(g.advance)).collect();
    let average = advances.iter().sum::<u32>().checked_div(advances.len() as u32).unwrap_or(0);
    let mut out = Out::default();
    out.u16(4);
    out.i16(average as i16);
    out.u16(face.weight_class());
    out.u16(5);
    out.u16(0);
    for _ in 0..10 {
        out.i16(0);
    }
    out.i16(0);
    out.bytes(&[0u8; 10]);
    out.u32(0x0000_0001);
    out.u32(0);
    out.u32(0);
    out.u32(0);
    out.bytes(b"FSTS");
    out.u16(selection);
    out.u16(0x20);
    out.u16(0xE000);
    out.i16(ASCENDER);
    out.i16(DESCENDER);
    out.i16(0);
    out.u16(850);
    out.u16(250);
    out.u32(0);
    out.u32(0);
    out.i16(x_height);
    out.i16(cap_height);
    out.u16(0x20);
    out.u16(0x20);
    out.u16(0);
    out.finish()
}

/// The names: copyright note, family, style, unique identifier, full name, version and PostScript name.
fn name(face: Face) -> Vec<u8> {
    let style = face.style();
    let records: [(u16, String); 7] = [
        (0, "Framesmith test font: a test fixture, not part of any product".to_string()),
        (1, FAMILY.to_string()),
        (2, style.to_string()),
        (3, format!("1.000;FSTS;FramesmithTestStroke-{style}")),
        (4, format!("{FAMILY} {style}")),
        (5, "Version 1.000".to_string()),
        (6, format!("FramesmithTestStroke-{style}")),
    ];
    let mut storage = Vec::new();
    let mut entries = Vec::new();
    for (id, text) in &records {
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        entries.push((*id, encoded.len() as u16, storage.len() as u16));
        storage.extend(encoded);
    }
    let mut out = Out::default();
    out.u16(0);
    out.u16(records.len() as u16);
    out.u16((6 + 12 * records.len()) as u16);
    for (id, length, offset) in entries {
        out.u16(3);
        out.u16(1);
        out.u16(0x0409);
        out.u16(id);
        out.u16(length);
        out.u16(offset);
    }
    out.bytes(&storage);
    out.finish()
}

fn post() -> Vec<u8> {
    let mut out = Out::default();
    out.u32(0x0003_0000);
    out.u32(0);
    out.i16(-75);
    out.i16(50);
    for _ in 0..5 {
        out.u32(0);
    }
    out.finish()
}
