//! A PNG file from 8-bit red, green and blue, written here so that no program (and no outside crate)
//! is needed to save the sheet. Each row is filtered with "Sub" and packed with a single DEFLATE
//! block of fixed Huffman codes and a greedy search for repeats (RFC 1951, RFC 1950, and the PNG
//! specification, section 4.1 and 9).

const WINDOW: usize = 32768;
const CHAIN: usize = 24;
const MAX_MATCH: usize = 258;

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385,
    24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

/// Bits are packed starting from the low end of each byte.
struct Bits {
    bytes: Vec<u8>,
    held: u32,
    count: u32,
}

impl Bits {
    fn new() -> Bits {
        Bits {
            bytes: Vec::new(),
            held: 0,
            count: 0,
        }
    }

    /// Puts `count` bits of `value`, the lowest bit first (the way numbers are packed).
    fn number(&mut self, value: u32, count: u32) {
        self.held |= value << self.count;
        self.count += count;
        while self.count >= 8 {
            self.bytes.push(self.held as u8);
            self.held >>= 8;
            self.count -= 8;
        }
    }

    /// Puts a Huffman code, its highest bit first.
    fn code(&mut self, code: u32, length: u32) {
        let mut reversed = 0;
        for bit in 0..length {
            reversed |= ((code >> bit) & 1) << (length - 1 - bit);
        }
        self.number(reversed, length);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.bytes.push(self.held as u8);
        }
        self.bytes
    }
}

/// A literal or length symbol with the fixed codes of the format.
fn symbol(bits: &mut Bits, value: u32) {
    match value {
        0..=143 => bits.code(0x30 + value, 8),
        144..=255 => bits.code(0x190 + value - 144, 9),
        256..=279 => bits.code(value - 256, 7),
        _ => bits.code(0xC0 + value - 280, 8),
    }
}

fn hash(data: &[u8], at: usize) -> usize {
    ((usize::from(data[at]) << 10) ^ (usize::from(data[at + 1]) << 5) ^ usize::from(data[at + 2])) & 0x7fff
}

/// Packs `data` as one final block of the fixed codes.
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits::new();
    bits.number(1, 1); // the last block
    bits.number(1, 2); // fixed Huffman codes
    let mut head = vec![usize::MAX; 1 << 15];
    let mut previous = vec![usize::MAX; data.len()];
    let mut at = 0;
    while at < data.len() {
        let mut best = (0usize, 0usize);
        if at + 3 <= data.len() {
            let mut candidate = head[hash(data, at)];
            let mut tries = 0;
            while candidate != usize::MAX && at - candidate <= WINDOW && tries < CHAIN {
                let limit = MAX_MATCH.min(data.len() - at);
                let mut length = 0;
                while length < limit && data[candidate + length] == data[at + length] {
                    length += 1;
                }
                if length > best.0 {
                    best = (length, at - candidate);
                    if length == limit {
                        break;
                    }
                }
                candidate = previous[candidate];
                tries += 1;
            }
        }
        let step = if best.0 >= 3 {
            let (length, distance) = best;
            let code = LENGTH_BASE.iter().rposition(|&base| usize::from(base) <= length).unwrap_or(0);
            symbol(&mut bits, 257 + code as u32);
            bits.number((length - usize::from(LENGTH_BASE[code])) as u32, u32::from(LENGTH_EXTRA[code]));
            let slot = DISTANCE_BASE.iter().rposition(|&base| usize::from(base) <= distance).unwrap_or(0);
            bits.code(slot as u32, 5);
            bits.number(
                (distance - usize::from(DISTANCE_BASE[slot])) as u32,
                u32::from(DISTANCE_EXTRA[slot]),
            );
            length
        } else {
            symbol(&mut bits, u32::from(data[at]));
            1
        };
        for (covered, slot) in previous.iter_mut().enumerate().skip(at).take(step) {
            if covered + 3 <= data.len() {
                let h = hash(data, covered);
                *slot = head[h];
                head[h] = covered;
            }
        }
        at += step;
    }
    symbol(&mut bits, 256);
    bits.finish()
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut table = [0u32; 256];
    for (n, entry) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
        }
        *entry = c;
    }
    let mut crc = 0xffff_ffffu32;
    for part in parts {
        for &byte in *part {
            crc = table[((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
        }
    }
    crc ^ 0xffff_ffff
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// The bytes of a PNG file of `width` x `height` pixels from `rgb`, three bytes for each pixel, row by row.
pub fn encode(width: u32, height: u32, rgb: &[u8]) -> Result<Vec<u8>, String> {
    let row = width as usize * 3;
    if width == 0 || height == 0 || rgb.len() != row * height as usize {
        return Err(format!(
            "a picture of {width}x{height} needs {} bytes of colour and has {}",
            row * height as usize,
            rgb.len()
        ));
    }
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgb.chunks_exact(row) {
        raw.push(1); // the filter "Sub": each byte less the byte one pixel to the left
        for (at, &byte) in line.iter().enumerate() {
            raw.push(byte.wrapping_sub(if at >= 3 { line[at - 3] } else { 0 }));
        }
    }
    let mut zlib = vec![0x78, 0x9c];
    zlib.extend(deflate(&raw));
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, red green blue, deflate, no filter set, no interlace
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}
