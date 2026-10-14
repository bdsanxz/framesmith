//! The PNG file the sheet is saved as: its chunks, its checks and, through a small reader of the
//! format written here from RFC 1950 and 1951, every pixel it holds.

use parity::png::encode;

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    bit: u32,
}

impl Reader<'_> {
    fn bit(&mut self) -> u32 {
        let value = (u32::from(self.data[self.at]) >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.at += 1;
        }
        value
    }

    fn number(&mut self, count: u32) -> u32 {
        (0..count).map(|k| self.bit() << k).sum()
    }

    /// The symbol of the fixed code that comes next.
    fn symbol(&mut self) -> u32 {
        let mut code = 0;
        for _ in 0..7 {
            code = code << 1 | self.bit();
        }
        if code <= 0b0010111 {
            return 256 + code;
        }
        code = code << 1 | self.bit();
        if (0b00110000..=0b10111111).contains(&code) {
            return code - 0b00110000;
        }
        if (0b11000000..=0b11000111).contains(&code) {
            return 280 + code - 0b11000000;
        }
        code = code << 1 | self.bit();
        assert!(
            (0b110010000..=0b111111111).contains(&code),
            "not a code of the fixed table: {code:b}"
        );
        144 + code - 0b110010000
    }
}

const LENGTH_BASE: [u32; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const LENGTH_EXTRA: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [u32; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385,
    24577,
];
const DISTANCE_EXTRA: [u32; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

/// Unpacks one final block of fixed codes.
fn inflate(deflated: &[u8]) -> Vec<u8> {
    let mut reader = Reader {
        data: deflated,
        at: 0,
        bit: 0,
    };
    assert_eq!(reader.number(1), 1, "the block is the last");
    assert_eq!(reader.number(2), 1, "the block uses the fixed codes");
    let mut out: Vec<u8> = Vec::new();
    loop {
        let symbol = reader.symbol();
        match symbol {
            0..=255 => out.push(symbol as u8),
            256 => return out,
            _ => {
                let slot = (symbol - 257) as usize;
                let length = LENGTH_BASE[slot] + reader.number(LENGTH_EXTRA[slot]);
                let mut distance_code = 0;
                for _ in 0..5 {
                    distance_code = distance_code << 1 | reader.bit();
                }
                let distance = DISTANCE_BASE[distance_code as usize] + reader.number(DISTANCE_EXTRA[distance_code as usize]);
                for _ in 0..length {
                    out.push(out[out.len() - distance as usize]);
                }
            }
        }
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { 0xedb88320 ^ (crc >> 1) } else { crc >> 1 };
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
    b << 16 | a
}

/// The width, height and pixels a PNG file of this writer holds, after checking each chunk's CRC.
fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    let (mut at, mut header, mut data) = (8, Vec::new(), Vec::new());
    let mut kinds = Vec::new();
    while at < png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let kind = &png[at + 4..at + 8];
        let body = &png[at + 8..at + 8 + length];
        let crc = u32::from_be_bytes(png[at + 8 + length..at + 12 + length].try_into().unwrap());
        assert_eq!(
            crc,
            crc32(&png[at + 4..at + 8 + length]),
            "the CRC of {:?}",
            String::from_utf8_lossy(kind)
        );
        kinds.push(String::from_utf8_lossy(kind).to_string());
        match kind {
            b"IHDR" => header = body.to_vec(),
            b"IDAT" => data.extend_from_slice(body),
            _ => {}
        }
        at += 12 + length;
    }
    assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
    let (width, height) = (
        u32::from_be_bytes(header[0..4].try_into().unwrap()),
        u32::from_be_bytes(header[4..8].try_into().unwrap()),
    );
    assert_eq!(&header[8..], &[8, 2, 0, 0, 0]);
    assert_eq!(&data[..2], &[0x78, 0x9c]);
    let raw = inflate(&data[2..data.len() - 4]);
    assert_eq!(
        u32::from_be_bytes(data[data.len() - 4..].try_into().unwrap()),
        adler32(&raw),
        "the Adler-32 of the data"
    );
    let row = width as usize * 3;
    assert_eq!(raw.len(), (row + 1) * height as usize);
    let mut rgb = Vec::new();
    for line in raw.chunks_exact(row + 1) {
        assert_eq!(line[0], 1, "each row is filtered with Sub");
        let start = rgb.len();
        for (k, &byte) in line[1..].iter().enumerate() {
            let left = if k >= 3 { rgb[start + k - 3] } else { 0 };
            rgb.push(byte.wrapping_add(left));
        }
    }
    (width, height, rgb)
}

/// A repeatable stream of bytes that looks like noise.
fn noise(count: usize, mut seed: u32) -> Vec<u8> {
    (0..count)
        .map(|_| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 16) as u8
        })
        .collect()
}

#[test]
fn a_png_of_noise_holds_every_pixel_it_was_given() {
    let rgb = noise(37 * 23 * 3, 7);
    let (width, height, back) = decode(&encode(37, 23, &rgb).unwrap());
    assert_eq!((width, height), (37, 23));
    assert_eq!(back, rgb);
}

#[test]
fn a_png_of_long_runs_and_repeated_rows_holds_every_pixel_and_is_far_smaller_than_the_picture() {
    let (width, height) = (300u32, 200u32);
    let mut rgb = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let colour = if y < 100 {
                [20, 30, 50]
            } else if x % 40 < 20 {
                [250, 250, 250]
            } else {
                [(y % 7) as u8, 90, 200]
            };
            rgb.extend_from_slice(&colour);
        }
    }
    let png = encode(width, height, &rgb).unwrap();
    assert!(png.len() < rgb.len() / 20, "{} bytes for {}", png.len(), rgb.len());
    let (w, h, back) = decode(&png);
    assert_eq!((w, h), (width, height));
    assert_eq!(back, rgb);
}

#[test]
fn a_one_pixel_picture_and_one_pixel_wide_pictures_are_written() {
    for (width, height) in [(1u32, 1u32), (1, 9), (9, 1)] {
        let rgb = noise((width * height * 3) as usize, width * 31 + height);
        let (w, h, back) = decode(&encode(width, height, &rgb).unwrap());
        assert_eq!((w, h, back), (width, height, rgb));
    }
}

#[test]
fn colour_that_does_not_fit_the_size_is_refused() {
    assert!(encode(4, 4, &[0; 47]).unwrap_err().contains("needs 48 bytes"));
    assert!(encode(0, 4, &[]).is_err());
}
