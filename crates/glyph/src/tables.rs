//! Byte-level font-file reads shared by the collection, the machine
//! scan, and the color-emoji path: family names, single-face slicing,
//! character coverage, and `sbix` PNG extraction.
//!
//! All reads are bounds-checked: a damaged file yields `None`, never a
//! panic or an out-of-bounds read. This is the std-only replacement for
//! the `read-fonts` queries the old engine used; shaping itself is
//! parley's business, in the `text` crate.

use super::font::{be16, be32, find_table, parse_cmap, table_dir, tag, Face};

/// Offset of face `index` in a `ttcf` file, or `0` for a plain sfnt.
/// `None` when the header or the offset lies outside the bytes.
fn face_base(bytes: &[u8], index: u32) -> Option<usize> {
    if bytes.len() < 4 {
        return None;
    }
    if &bytes[0..4] == b"ttcf" {
        let n = be32(bytes, 8)? as usize;
        if n == 0 {
            return None;
        }
        let i = (index as usize).min(n - 1);
        // Note: an out-of-range index reads the last face rather than
        // failing; callers pass indices they discovered, so this only
        // guards damaged headers.
        let _ = n;
        be32(bytes, 12 + i * 4)
            .map(|o| o as usize)
            .filter(|&o| o < bytes.len())
    } else {
        (index == 0).then_some(0)
    }
}

/// Number of faces in the file: the `ttcf` count, else 1 for an sfnt.
pub fn face_count(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 4 {
        return None;
    }
    if &bytes[0..4] == b"ttcf" {
        let n = be32(bytes, 8)? as usize;
        (n > 0).then_some(n)
    } else {
        Some(1)
    }
}

/// Family name of face `index`, from its `name` table: name ID 1 on the
/// Macintosh platform first, else on the Windows platform (UTF-16).
/// `None` when the table cannot be read or names no family.
pub fn family_name(bytes: &[u8], index: u32) -> Option<String> {
    let base = face_base(bytes, index)?;
    let dir = table_dir(bytes, base)?;
    let (off, len) = find_table(&dir, &tag(b'n', b'a', b'm', b'e'))?;
    let table = bytes.get(off..off.checked_add(len)?)?;
    family_name_of_table(table)
}

/// Whether face `index` maps `ch` to a glyph other than glyph 0.
/// A face without a usable `cmap` maps nothing.
pub fn maps_char(bytes: &[u8], index: u32, ch: char) -> bool {
    let Some(base) = face_base(bytes, index) else {
        return false;
    };
    let Some(dir) = table_dir(bytes, base) else {
        return false;
    };
    let Some((off, len)) = find_table(&dir, &tag(b'c', b'm', b'a', b'p')) else {
        return false;
    };
    let Ok(pairs) = parse_cmap(bytes, off, len) else {
        return false;
    };
    match pairs.binary_search_by_key(&(ch as u32), |(c, _)| *c) {
        Ok(i) => pairs[i].1 != 0,
        Err(_) => false,
    }
}

/// Glyph id face `index` maps `ch` to, or `None` for glyph 0 / unmapped.
pub fn glyph_id(bytes: &[u8], index: u32, ch: char) -> Option<u16> {
    let base = face_base(bytes, index)?;
    let dir = table_dir(bytes, base)?;
    let (off, len) = find_table(&dir, &tag(b'c', b'm', b'a', b'p'))?;
    let pairs = parse_cmap(bytes, off, len).ok()?;
    match pairs.binary_search_by_key(&(ch as u32), |(c, _)| *c) {
        Ok(i) => (pairs[i].1 != 0).then_some(pairs[i].1),
        Err(_) => None,
    }
}

/// Units per em of face `index`, from its `head` table.
pub fn units_per_em(bytes: &[u8], index: u32) -> Option<u16> {
    let base = face_base(bytes, index)?;
    let dir = table_dir(bytes, base)?;
    let (off, _) = find_table(&dir, &tag(b'h', b'e', b'a', b'd'))?;
    let upm = be16(bytes, off + 18)?;
    (upm > 0).then_some(upm)
}

/// Family name straight from `name`-table bytes (the table slice, not the
/// whole file): lets the machine scan read only the name table.
pub fn family_name_of_table(table: &[u8]) -> Option<String> {
    let count = be16(table, 2)? as usize;
    let storage = be16(table, 4)? as usize;
    for platform in [1u16, 3] {
        for number in 0..count {
            let record = 6 + 12 * number;
            if be16(table, record)? != platform || be16(table, record + 6)? != 1 {
                continue;
            }
            let length = be16(table, record + 8)? as usize;
            let start = storage.checked_add(be16(table, record + 10)? as usize)?;
            let text = table.get(start..start.checked_add(length)?)?;
            if text.is_empty() {
                continue;
            }
            if platform == 1 {
                return Some(text.iter().map(|&b| char::from(b)).collect());
            }
            let units: Vec<u16> = text
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect();
            if let Ok(name) = String::from_utf16(&units) {
                return Some(name);
            }
        }
    }
    None
}

/// Character coverage from raw `cmap`-table bytes (the table slice).
/// Lets the machine scan test coverage without reading whole files.
pub fn maps_char_in_table(cmap: &[u8], ch: char) -> bool {
    let Ok(pairs) = parse_cmap(cmap, 0, cmap.len()) else {
        return false;
    };
    match pairs.binary_search_by_key(&(ch as u32), |(c, _)| *c) {
        Ok(i) => pairs[i].1 != 0,
        Err(_) => false,
    }
}

/// Glyph id from raw `cmap`-table bytes, or `None` for glyph 0/unmapped.
pub fn glyph_id_in_table(cmap: &[u8], ch: char) -> Option<u16> {
    let pairs = parse_cmap(cmap, 0, cmap.len()).ok()?;
    match pairs.binary_search_by_key(&(ch as u32), |(c, _)| *c) {
        Ok(i) => (pairs[i].1 != 0).then_some(pairs[i].1),
        Err(_) => None,
    }
}

/// Whether face `index` carries an `sbix` color table.
pub fn has_sbix(bytes: &[u8], index: u32) -> bool {
    face_base(bytes, index)
        .and_then(|base| table_dir(bytes, base))
        .and_then(|dir| find_table(&dir, &tag(b's', b'b', b'i', b'x')))
        .is_some()
}

/// A glyph's color picture in one `sbix` strike: pixels-per-em of the
/// strike, the raw graphic bytes, and the origin offsets in design units.
pub struct SbixGraphic {
    pub ppem: u16,
    pub kind: [u8; 4],
    pub origin_x: i16,
    pub origin_y: i16,
    pub data: Vec<u8>,
}

/// The PNG picture of `glyph` at `size_px`: among strikes holding a
/// `png ` graphic for the glyph, the smallest strike at least `size_px`,
/// else the largest strike. The FFmpeg decode of `data` is the caller's
/// business (see `ffmpeg::picture::decode_memory`): this crate never
/// decodes pictures, it only locates them.
pub fn sbix_graphic(bytes: &[u8], index: u32, glyph: usize, size_px: f32) -> Option<SbixGraphic> {
    let base = face_base(bytes, index)?;
    let dir = table_dir(bytes, base)?;
    let (sbix_off, sbix_len) = find_table(&dir, &tag(b's', b'b', b'i', b'x'))?;
    let end = sbix_off.checked_add(sbix_len)?;
    if end > bytes.len() || sbix_len < 8 {
        return None;
    }
    let number = be32(bytes, sbix_off)? as usize;
    let mut best: Option<(u16, usize, usize, i16, i16, [u8; 4])> = None;
    let mut largest: Option<(u16, usize, usize, i16, i16, [u8; 4])> = None;
    for strike in 0..number {
        let strike_rel = be32(bytes, sbix_off + 8 + 4 * strike)? as usize;
        let sstart = sbix_off.checked_add(strike_rel)?;
        if sstart + 4 > end {
            continue;
        }
        let ppem = be16(bytes, sstart)?;
        // Glyph offsets follow the 4-byte strike header: numGlyphs+1
        // uint32 entries. The count is unknown here, so probe the two
        // entries for `glyph` directly, bounds-checked.
        let o0 = sstart.checked_add(4 + 4 * glyph)?;
        let o1 = o0.checked_add(4)?;
        if o1 + 4 > end {
            continue;
        }
        let first = be32(bytes, o0)? as usize;
        let next = be32(bytes, o1)? as usize;
        if next <= first {
            continue;
        }
        let gstart = sstart.checked_add(first)?;
        let gend = sstart.checked_add(next)?;
        if gend > end || gend - gstart < 8 {
            continue;
        }
        let ox = be16(bytes, gstart)? as i16;
        let oy = be16(bytes, gstart + 2)? as i16;
        let kind = [
            bytes[gstart + 4],
            bytes[gstart + 5],
            bytes[gstart + 6],
            bytes[gstart + 7],
        ];
        if kind != *b"png " {
            continue;
        }
        let entry = (ppem, gstart + 8, gend - gstart - 8, ox, oy, kind);
        if largest
            .as_ref()
            .is_none_or(|(best_ppem, ..)| ppem > *best_ppem)
        {
            largest = Some(entry.clone());
        }
        if f32::from(ppem) >= size_px
            && best
                .as_ref()
                .is_none_or(|(best_ppem, ..)| ppem < *best_ppem)
        {
            best = Some(entry);
        }
    }
    let (ppem, dstart, dlen, ox, oy, kind) = best.or(largest)?;
    Some(SbixGraphic {
        ppem,
        kind,
        origin_x: ox,
        origin_y: oy,
        data: bytes.get(dstart..dstart.checked_add(dlen)?)?.to_vec(),
    })
}

/// The bytes of face `index` alone: a TrueType file as it is, and for a
/// collection a rebuilt file of that face's table directory and tables,
/// each table at a multiple-of-four offset. Both renderers read a
/// TrueType file; no card reads a collection.
pub fn one_face(bytes: &[u8], index: u32) -> Option<Vec<u8>> {
    if !bytes.starts_with(b"ttcf") {
        return (index == 0).then(|| bytes.to_vec());
    }
    let face = be32(bytes, 12 + 4 * index as usize)? as usize;
    if face + 12 > bytes.len() {
        return None;
    }
    let count = be16(bytes, face + 4)? as usize;
    let mut alone = bytes.get(face..face + 12)?.to_vec();
    let mut tables = Vec::new();
    let mut at = 12 + 16 * count;
    for number in 0..count {
        let record = face + 12 + 16 * number;
        if record + 16 > bytes.len() {
            return None;
        }
        let (offset, length) = (
            be32(bytes, record + 8)? as usize,
            be32(bytes, record + 12)? as usize,
        );
        let data = bytes.get(offset..offset.checked_add(length)?)?;
        alone.extend_from_slice(bytes.get(record..record + 8)?);
        alone.extend_from_slice(&u32::try_from(at).ok()?.to_be_bytes());
        alone.extend_from_slice(&u32::try_from(length).ok()?.to_be_bytes());
        tables.extend_from_slice(data);
        tables.resize(tables.len().next_multiple_of(4), 0);
        at = 12 + 16 * count + tables.len();
    }
    alone.extend_from_slice(&tables);
    Some(alone)
}

impl Face {
    /// Glyph id the face draws `ch` with, or `0` for no outline: the
    /// `cmap` mapping for a TrueType face, the cell index for a bitmap
    /// face holding the cell. Glyph 0 is never drawn as outlines.
    pub fn gid_of(&self, ch: char) -> u16 {
        match self {
            Face::Tt(t) => t.gid(ch as u32),
            Face::Bitmap => matches!(ch, '\u{20}'..='\u{7e}')
                .then_some(ch as u16)
                .unwrap_or(0),
        }
    }

    /// Whether the face draws `ch` as outlines: anything else falls
    /// back per character (system face, then the bitmap last resort).
    pub fn has_glyph(&self, ch: char) -> bool {
        self.gid_of(ch) != 0
    }
}
