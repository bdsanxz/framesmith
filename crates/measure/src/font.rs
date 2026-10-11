//! A test font made by us: a TrueType file in which the letter `X` is a
//! solid square one em wide and one em tall, and nothing else has a shape.
//! Text set in it paints a solid block, which no installed font does.

struct Bytes(Vec<u8>);

impl Bytes {
    fn u16(mut self, values: &[u16]) -> Bytes {
        values.iter().for_each(|v| self.0.extend_from_slice(&v.to_be_bytes()));
        self
    }

    fn i16(mut self, values: &[i16]) -> Bytes {
        values.iter().for_each(|v| self.0.extend_from_slice(&v.to_be_bytes()));
        self
    }

    fn u32(mut self, values: &[u32]) -> Bytes {
        values.iter().for_each(|v| self.0.extend_from_slice(&v.to_be_bytes()));
        self
    }

    fn raw(mut self, bytes: &[u8]) -> Bytes {
        self.0.extend_from_slice(bytes);
        self
    }
}

fn start() -> Bytes {
    Bytes(Vec::new())
}

/// The units in one em; the square glyph is this wide and this tall.
const EM: u16 = 1000;
const LETTER: u16 = b'X' as u16;

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn name_table(family: &str) -> Vec<u8> {
    // Family, style, full name and PostScript name, as Windows Unicode strings.
    let strings: [(u16, String); 4] = [
        (1, family.to_string()),
        (2, "Regular".to_string()),
        (4, family.to_string()),
        (6, family.replace(' ', "")),
    ];
    let mut records = start().u16(&[0, strings.len() as u16, 6 + 12 * strings.len() as u16]);
    let mut text: Vec<u8> = Vec::new();
    for (id, string) in &strings {
        let encoded: Vec<u8> = string.encode_utf16().flat_map(u16::to_be_bytes).collect();
        records = records.u16(&[3, 1, 0x0409, *id, encoded.len() as u16, text.len() as u16]);
        text.extend(encoded);
    }
    records.raw(&text).0
}

/// The bytes of the font file, with the given family name inside it.
pub fn block_font(family: &str) -> Vec<u8> {
    let em = EM as i16;
    let head = start()
        .u32(&[0x0001_0000, 0x0001_0000, 0, 0x5F0F_3CF5])
        .u16(&[0x0003, EM])
        .u32(&[0, 0, 0, 0])
        .i16(&[0, 0, em, em])
        .u16(&[0, 8])
        .i16(&[2, 0, 0])
        .0;
    let hhea = start()
        .u32(&[0x0001_0000])
        .i16(&[em, 0, 0])
        .u16(&[EM])
        .i16(&[0, 0, em, 1, 0, 0, 0, 0, 0, 0, 0])
        .u16(&[2])
        .0;
    let maxp = start().u32(&[0x0001_0000]).u16(&[2, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]).0;
    let hmtx = start().u16(&[EM, 0, EM, 0]).0;
    // One segment for the letter and the closing segment the format asks for.
    let cmap = start()
        .u16(&[0, 1, 3, 1])
        .u32(&[12])
        .u16(&[4, 32, 0, 4, 4, 1, 0, LETTER, 0xFFFF, 0, LETTER, 0xFFFF])
        .i16(&[1 - LETTER as i16, 1])
        .u16(&[0, 0])
        .0;
    // The square: four corners on the curve, drawn clockwise.
    let glyf = start()
        .i16(&[1, 0, 0, em, em])
        .u16(&[3, 0])
        .raw(&[1, 1, 1, 1])
        .i16(&[0, 0, em, 0])
        .i16(&[0, em, 0, -em])
        .raw(&[0, 0])
        .0;
    // The empty glyph has no bytes; offsets are written halved.
    let loca = start().u16(&[0, 0, (glyf.len() / 2) as u16]).0;
    let post = start().u32(&[0x0003_0000, 0]).i16(&[0, 0]).u32(&[0, 0, 0, 0, 0]).0;
    let os2 = start()
        .u16(&[4])
        .i16(&[em])
        .u16(&[400, 5, 0])
        .i16(&[650, 600, 0, 75, 650, 600, 0, 350, 50, 250, 0])
        .raw(&[0; 10])
        .u32(&[0, 0, 0, 0])
        .raw(b"FSMT")
        .u16(&[0x0040, LETTER, LETTER])
        .i16(&[em, 0, 0])
        .u16(&[EM, 0])
        .u32(&[0, 0])
        .i16(&[em / 2, em])
        .u16(&[0, 0x20, 1])
        .0;
    let name = name_table(family);

    // Tables in the order of their tags.
    let tables: [(&[u8; 4], Vec<u8>); 10] = [
        (b"OS/2", os2),
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
        (b"name", name),
        (b"post", post),
    ];
    let count = tables.len() as u16;
    let mut directory = start().u32(&[0x0001_0000]).u16(&[count, 128, 3, count * 16 - 128]);
    let mut body: Vec<u8> = Vec::new();
    let first = 12 + 16 * tables.len();
    let mut head_at = 0;
    for (tag, table) in &tables {
        let at = first + body.len();
        if *tag == b"head" {
            head_at = at;
        }
        directory = directory.raw(*tag).u32(&[checksum(table), at as u32, table.len() as u32]);
        body.extend_from_slice(table);
        body.resize(body.len().next_multiple_of(4), 0);
    }
    let mut file = directory.raw(&body).0;
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&file));
    file[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
    file
}
