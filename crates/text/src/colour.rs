//! The colour pictures of a font collection (`docs/spec/text.md`, sections 2.6 and 12.4). A collection is read for its one
//! face, which must hold `sbix` strikes; its outlines are never read. A strike gives the data of each glyph at one size in
//! pixels per em, and the data of a glyph is a picture when its type is `png `.

use crate::tables::{
    box_corner, bytes_at, face_offsets, family_name, find_table, glyph_count, glyph_offsets, i16_at, table_bytes, u16_at, u32_at,
    units_per_em_and_long_offsets, Table,
};

/// Why a collection is not read (`docs/spec/text.md`, section 2.6): the file is not a font (FS-0051), or it is a collection
/// of a kind this version does not draw, with the reason (FS-0035).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    Invalid,
    Unsupported(String),
}

/// One strike of the `sbix` table: its size in pixels per em, the start of the strike in the file, and the offsets of each
/// glyph's data from that start, one more offset than there are glyphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Strike {
    pub(crate) ppem: u16,
    start: usize,
    offsets: Vec<usize>,
}

/// The one face of a collection that is read: its family name, its index in the collection, the units of its design, the
/// boxes of its glyph records (none when it has no `loca` and `glyf` tables), and its strikes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColourFace {
    pub(crate) name: String,
    pub(crate) index: u32,
    pub(crate) units_per_em: u16,
    glyf: Option<(Table, Vec<usize>)>,
    strikes: Vec<Strike>,
}

/// The data of a glyph in one strike: its two origin offsets in design units, its type tag and its graphic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Graphic<'a> {
    pub(crate) origin_x: i16,
    pub(crate) origin_y: i16,
    pub(crate) kind: [u8; 4],
    pub(crate) data: &'a [u8],
}

/// Reads the one face of a collection that is drawn, from the bytes of the whole file (`docs/spec/text.md`, section 2.6).
pub(crate) fn read_collection(bytes: &[u8]) -> Result<ColourFace, Refusal> {
    let offsets = face_offsets(bytes).ok_or(Refusal::Invalid)?;
    let mut faces: Vec<(usize, usize, String)> = Vec::with_capacity(offsets.len());
    for (number, &face) in offsets.iter().enumerate() {
        let name = find_table(bytes, face, b"name")
            .map_err(|()| Refusal::Invalid)?
            .ok_or(Refusal::Invalid)?;
        let family = family_name(bytes, name).ok_or(Refusal::Invalid)?;
        faces.push((number, face, family));
    }
    let shown: Vec<&(usize, usize, String)> = faces.iter().filter(|(_, _, family)| !family.starts_with('.')).collect();
    if shown.len() != 1 {
        return Err(Refusal::Unsupported(format!(
            "a font collection of {} faces that are not hidden, where exactly one is drawn",
            shown.len()
        )));
    }
    let (number, face, name) = shown[0];
    let sbix = find_table(bytes, *face, b"sbix")
        .map_err(|()| Refusal::Invalid)?
        .ok_or_else(|| Refusal::Unsupported("a font collection whose one face has outlines and no colour pictures".into()))?;
    let head = find_table(bytes, *face, b"head")
        .map_err(|()| Refusal::Invalid)?
        .ok_or(Refusal::Invalid)?;
    let (units_per_em, long) = units_per_em_and_long_offsets(bytes, head).ok_or(Refusal::Invalid)?;
    let maxp = find_table(bytes, *face, b"maxp")
        .map_err(|()| Refusal::Invalid)?
        .ok_or(Refusal::Invalid)?;
    let count = glyph_count(bytes, maxp).ok_or(Refusal::Invalid)?;
    let strikes = read_strikes(bytes, sbix, count).ok_or(Refusal::Invalid)?;
    let loca = find_table(bytes, *face, b"loca").map_err(|()| Refusal::Invalid)?;
    let glyf = find_table(bytes, *face, b"glyf").map_err(|()| Refusal::Invalid)?;
    let glyf = match (loca, glyf) {
        (Some(loca), Some(glyf)) => {
            let offsets = glyph_offsets(bytes, loca, long, count).ok_or(Refusal::Invalid)?;
            if offsets.last().is_some_and(|&last| last > glyf.length) {
                return Err(Refusal::Invalid);
            }
            Some((glyf, offsets))
        }
        _ => None,
    };
    Ok(ColourFace {
        name: name.clone(),
        index: u32::try_from(*number).map_err(|_| Refusal::Invalid)?,
        units_per_em,
        glyf,
        strikes,
    })
}

/// The strikes of the `sbix` table for `count` glyphs: the header's strike offsets, and in each strike its size and its
/// glyph offsets. `None` when a strike lies outside the table.
fn read_strikes(bytes: &[u8], sbix: Table, count: usize) -> Option<Vec<Strike>> {
    let table = table_bytes(bytes, sbix)?;
    let number = usize::try_from(u32_at(table, 4)?).ok()?;
    let mut strikes = Vec::with_capacity(number.min(table.len() / 4));
    for strike in 0..number {
        let start = usize::try_from(u32_at(table, 8 + 4 * strike)?).ok()?;
        let ppem = u16_at(table, start)?;
        let mut offsets = Vec::with_capacity(count + 1);
        for glyph in 0..=count {
            let at = start.checked_add(4 + 4 * glyph)?;
            offsets.push(usize::try_from(u32_at(table, at)?).ok()?);
        }
        strikes.push(Strike {
            ppem,
            start: sbix.offset.checked_add(start)?,
            offsets,
        });
    }
    Some(strikes)
}

/// The data of one glyph in one strike, when it has any: `None` for a glyph with no data there, or with less than a header.
fn graphic<'a>(bytes: &'a [u8], strike: &Strike, glyph: usize) -> Option<Graphic<'a>> {
    let first = *strike.offsets.get(glyph)?;
    let next = *strike.offsets.get(glyph + 1)?;
    if next.saturating_sub(first) < 8 {
        return None;
    }
    let start = strike.start.checked_add(first)?;
    let end = strike.start.checked_add(next)?;
    let header = bytes_at(bytes, start, 8)?;
    Some(Graphic {
        origin_x: i16_at(bytes, start)?,
        origin_y: i16_at(bytes, start + 2)?,
        kind: [header[4], header[5], header[6], header[7]],
        data: bytes.get(start + 8..end)?,
    })
}

impl ColourFace {
    /// The strike and the graphic a glyph is drawn from at `size` pixels (`docs/spec/text.md`, section 12.4, point 2): among
    /// the strikes where the glyph has a graphic of the type `png `, the one with the smallest size at least `size`, else the
    /// one with the largest size. `None` when no strike has such a graphic.
    pub(crate) fn chosen_graphic<'a>(&self, bytes: &'a [u8], glyph: usize, size: f32) -> Option<(u16, Graphic<'a>)> {
        let mut at_least: Option<(u16, Graphic<'a>)> = None;
        let mut largest: Option<(u16, Graphic<'a>)> = None;
        for strike in &self.strikes {
            let Some(found) = graphic(bytes, strike, glyph) else {
                continue;
            };
            if found.kind != *b"png " {
                continue;
            }
            let ppem = strike.ppem;
            if f32::from(ppem) >= size && at_least.as_ref().is_none_or(|(best, _)| ppem < *best) {
                at_least = Some((ppem, found));
            }
            if largest.as_ref().is_none_or(|(best, _)| ppem > *best) {
                largest = Some((ppem, found));
            }
        }
        at_least.or(largest)
    }

    /// The lower-left corner of a glyph's bounding box in design units, when its record has contours; `Some(None)` for a
    /// glyph with no contours; `None` when the record cannot be read.
    pub(crate) fn box_corner(&self, bytes: &[u8], glyph: usize) -> Option<Option<(i16, i16)>> {
        match &self.glyf {
            None => Some(None),
            Some((glyf, offsets)) => box_corner(bytes, *glyf, offsets, glyph),
        }
    }
}
