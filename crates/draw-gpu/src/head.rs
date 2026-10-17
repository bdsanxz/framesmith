//! The box of the glyphs of a font, read from its `head` table (`docs/spec/card-renderer.md`, section 9.3).

/// The numbers of the `head` table that the card reads: the units of the em, and the box that holds every glyph
/// of the font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadBox {
    pub units_per_em: u16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
}

/// Reads the `head` table of a file of one face, whose table directory begins at its first byte. TrueType and
/// OpenType faces are read alike. `None` when the file is a collection or another kind of file, or when its table
/// is missing or cut short.
pub fn read(data: &[u8]) -> Option<HeadBox> {
    let version = data.get(0..4)?;
    if !(version == [0, 1, 0, 0] || version == *b"true" || version == *b"OTTO") {
        return None;
    }
    let tables = usize::from(be_u16(data, 4)?);
    let mut table = None;
    for index in 0..tables {
        let record = 12 + 16 * index;
        if data.get(record..record + 4)? == b"head" {
            table = Some(usize::try_from(be_u32(data, record + 8)?).ok()?);
            break;
        }
    }
    let table = table?;
    Some(HeadBox {
        units_per_em: be_u16(data, table + 18)?,
        x_min: be_i16(data, table + 36)?,
        y_min: be_i16(data, table + 38)?,
        x_max: be_i16(data, table + 40)?,
        y_max: be_i16(data, table + 42)?,
    })
}

fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn be_i16(data: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}
