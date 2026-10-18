//! Reads of the bytes of a font file, each checked against the file's length (`docs/spec/text.md`, sections 2.6 and
//! 12.4): the header of a collection, the table directory of a face, the family name of its `name` table, and the entries
//! of the tables that place a colour picture (`head`, `maxp`, `loca` and `glyf`). A read that falls outside the bytes gives
//! nothing, so a damaged file is refused and never read past its end.

/// A table of a face: its offset from the start of the file and its length in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Table {
    pub(crate) offset: usize,
    pub(crate) length: usize,
}

/// The `len` bytes at `at`, when they lie inside `bytes`.
pub(crate) fn bytes_at(bytes: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    bytes.get(at..at.checked_add(len)?)
}

/// The unsigned 16-bit number at `at`, big-endian.
pub(crate) fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes_at(bytes, at, 2)?;
    Some(u16::from_be_bytes([pair[0], pair[1]]))
}

/// The signed 16-bit number at `at`, big-endian.
pub(crate) fn i16_at(bytes: &[u8], at: usize) -> Option<i16> {
    u16_at(bytes, at).map(|value| value as i16)
}

/// The unsigned 32-bit number at `at`, big-endian.
pub(crate) fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let quad = bytes_at(bytes, at, 4)?;
    Some(u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

/// The offsets of the faces of a font collection, from its header (the OpenType specification, the font collection
/// header; section 2.6, point 1). `None` when the header or an offset lies outside the file, or when there is no face.
pub(crate) fn face_offsets(bytes: &[u8]) -> Option<Vec<usize>> {
    if bytes_at(bytes, 0, 4)? != b"ttcf" {
        return None;
    }
    let count = usize::try_from(u32_at(bytes, 8)?).ok()?;
    if count == 0 {
        return None;
    }
    let mut offsets = Vec::with_capacity(count.min(bytes.len() / 4));
    for number in 0..count {
        let offset = u32_at(bytes, 12 + 4 * number)?;
        offsets.push(usize::try_from(offset).ok()?);
    }
    Some(offsets)
}

/// The table with `tag` in the table directory of the face at `face` (its offset in the file). `Ok(None)` when the face
/// has no such table; `Err(())` when the directory, or the table record or the table, lies outside the file.
pub(crate) fn find_table(bytes: &[u8], face: usize, tag: &[u8; 4]) -> Result<Option<Table>, ()> {
    let count = usize::from(u16_at(bytes, face.checked_add(4).ok_or(())?).ok_or(())?);
    for number in 0..count {
        let record = face.checked_add(12 + 16 * number).ok_or(())?;
        if bytes_at(bytes, record, 4).ok_or(())? != tag {
            continue;
        }
        let offset = usize::try_from(u32_at(bytes, record + 8).ok_or(())?).map_err(|_| ())?;
        let length = usize::try_from(u32_at(bytes, record + 12).ok_or(())?).map_err(|_| ())?;
        return match offset.checked_add(length) {
            Some(end) if end <= bytes.len() => Ok(Some(Table { offset, length })),
            _ => Err(()),
        };
    }
    Ok(None)
}

/// The bytes of a table, when it lies inside the file.
pub(crate) fn table_bytes(bytes: &[u8], table: Table) -> Option<&[u8]> {
    bytes_at(bytes, table.offset, table.length)
}

/// The family name of a face, from its `name` table: the first name with name ID 1 of the Macintosh platform (platform ID
/// 1), or, when there is none, of the Windows platform (platform ID 3). A Macintosh name is read one byte to a character,
/// which is exact for the names of ASCII; a Windows name is read as UTF-16 (`docs/spec/text.md`, section 2.6). `None` when
/// the table cannot be read or gives no family name.
pub(crate) fn family_name(bytes: &[u8], name: Table) -> Option<String> {
    let table = table_bytes(bytes, name)?;
    let count = usize::from(u16_at(table, 2)?);
    let storage = usize::from(u16_at(table, 4)?);
    for platform in [1_u16, 3] {
        for number in 0..count {
            let record = 6 + 12 * number;
            if u16_at(table, record)? != platform || u16_at(table, record + 6)? != 1 {
                continue;
            }
            let length = usize::from(u16_at(table, record + 8)?);
            let start = storage.checked_add(usize::from(u16_at(table, record + 10)?))?;
            let text = bytes_at(table, start, length)?;
            if text.is_empty() {
                continue;
            }
            return match platform {
                1 => Some(text.iter().map(|&byte| char::from(byte)).collect()),
                _ => {
                    let units: Vec<u16> = text.as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
                    String::from_utf16(&units).ok()
                }
            };
        }
    }
    None
}

/// `unitsPerEm` of the `head` table, which is its entry at byte 18, and whether its `loca` offsets are long (its entry at
/// byte 50 is not 0).
pub(crate) fn units_per_em_and_long_offsets(bytes: &[u8], head: Table) -> Option<(u16, bool)> {
    if head.length < 54 {
        return None;
    }
    let table = table_bytes(bytes, head)?;
    let units = u16_at(table, 18)?;
    let long = i16_at(table, 50)? != 0;
    if units == 0 {
        return None;
    }
    Some((units, long))
}

/// The number of glyphs of the font, the `numGlyphs` entry of the `maxp` table at byte 4.
pub(crate) fn glyph_count(bytes: &[u8], maxp: Table) -> Option<usize> {
    if maxp.length < 6 {
        return None;
    }
    Some(usize::from(u16_at(table_bytes(bytes, maxp)?, 4)?))
}

/// The start of each glyph's record in `glyf`, relative to the start of `glyf`, for each of the `count` glyphs and the
/// end of the last: `count + 1` offsets, read from the `loca` table (short offsets are doubled, long offsets are as they
/// are).
pub(crate) fn glyph_offsets(bytes: &[u8], loca: Table, long: bool, count: usize) -> Option<Vec<usize>> {
    let table = table_bytes(bytes, loca)?;
    let width = if long { 4 } else { 2 };
    let mut offsets = Vec::with_capacity(count + 1);
    for number in 0..=count {
        let at = number * width;
        let offset = if long {
            usize::try_from(u32_at(table, at)?).ok()?
        } else {
            usize::from(u16_at(table, at)?) * 2
        };
        offsets.push(offset);
    }
    Some(offsets)
}

/// The lower-left corner of a glyph's bounding box, as its record in `glyf` gives it (the first two entries after the
/// contour count are `xMin` and `yMin`). `Some(None)` for a glyph with an empty record, which has no contours; `None` when
/// the record lies outside `glyf`.
pub(crate) fn box_corner(bytes: &[u8], glyf: Table, offsets: &[usize], glyph: usize) -> Option<Option<(i16, i16)>> {
    let first = *offsets.get(glyph)?;
    let next = *offsets.get(glyph + 1)?;
    if next <= first {
        return Some(None);
    }
    // A record with contours holds its header of ten bytes: the contour count, then the four values of the box.
    if next > glyf.length || next - first < 10 {
        return None;
    }
    let record = glyf.offset.checked_add(first)?;
    let x_min = i16_at(bytes, record + 2)?;
    let y_min = i16_at(bytes, record + 4)?;
    Some(Some((x_min, y_min)))
}
