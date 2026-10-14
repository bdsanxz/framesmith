//! A PNG file of 8-bit red, green, blue and alpha, written and read here so that the checked-in
//! pictures can be made and read with no program and no outside crate (the PNG specification,
//! sections 4, 9 and 10; RFC 1950 for the wrapper and RFC 1951 for the compressed data).
//!
//! The writer filters each row with "Sub" and compresses with one block of fixed codes and a greedy
//! search for repeats, which is plenty for flat pictures. The reader takes any file of this form:
//! every filter, and stored, fixed and dynamic blocks.

// The example that makes the pictures uses the writer alone, and the tests use both.
#![allow(dead_code)]

use std::collections::HashMap;

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
const CODE_LENGTH_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for part in parts {
        for &byte in *part {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { 0xedb8_8320 ^ (crc >> 1) } else { crc >> 1 };
            }
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

struct Writer {
    bytes: Vec<u8>,
    held: u32,
    count: u32,
}

impl Writer {
    /// `count` bits of `value`, the lowest first.
    fn number(&mut self, value: u32, count: u32) {
        self.held |= value << self.count;
        self.count += count;
        while self.count >= 8 {
            self.bytes.push(self.held as u8);
            self.held >>= 8;
            self.count -= 8;
        }
    }

    /// A Huffman code of `length` bits, the highest first.
    fn code(&mut self, code: u32, length: u32) {
        let mut reversed = 0;
        for bit in 0..length {
            reversed |= ((code >> bit) & 1) << (length - 1 - bit);
        }
        self.number(reversed, length);
    }

    /// A symbol of the fixed literal and length alphabet.
    fn symbol(&mut self, symbol: u32) {
        match symbol {
            0..=143 => self.code(0x30 + symbol, 8),
            144..=255 => self.code(0x190 + symbol - 144, 9),
            256..=279 => self.code(symbol - 256, 7),
            _ => self.code(0xc0 + symbol - 280, 8),
        }
    }
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut out = Writer {
        bytes: Vec::new(),
        held: 0,
        count: 0,
    };
    out.number(1, 1); // the last block
    out.number(1, 2); // fixed codes
    let mut seen: HashMap<[u8; 3], usize> = HashMap::new();
    let mut at = 0;
    while at < data.len() {
        let mut best = (0usize, 0usize);
        if at + 3 <= data.len() {
            if let Some(&earlier) = seen.get(&[data[at], data[at + 1], data[at + 2]]) {
                if at - earlier <= 32768 {
                    let mut length = 0;
                    while length < 258 && at + length < data.len() && data[earlier + length] == data[at + length] {
                        length += 1;
                    }
                    best = (length, at - earlier);
                }
            }
        }
        let step = if best.0 >= 3 {
            let (length, distance) = best;
            let l = (0..29).rev().find(|&i| usize::from(LENGTH_BASE[i]) <= length).unwrap();
            out.symbol(257 + l as u32);
            out.number((length - usize::from(LENGTH_BASE[l])) as u32, u32::from(LENGTH_EXTRA[l]));
            let d = (0..30).rev().find(|&i| usize::from(DISTANCE_BASE[i]) <= distance).unwrap();
            out.code(d as u32, 5);
            out.number((distance - usize::from(DISTANCE_BASE[d])) as u32, u32::from(DISTANCE_EXTRA[d]));
            length
        } else {
            out.symbol(u32::from(data[at]));
            1
        };
        for place in at..at + step {
            if place + 3 <= data.len() {
                seen.insert([data[place], data[place + 1], data[place + 2]], place);
            }
        }
        at += step;
    }
    out.symbol(256);
    out.number(0, 7); // the last byte is filled out
    out.bytes
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// The bytes of a PNG file of `width` by `height` pixels from `rgba`, four bytes for each pixel,
/// row by row from the top.
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    assert_eq!(rgba.len(), row * height as usize, "the bytes of a {width}x{height} picture");
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks_exact(row) {
        raw.push(1); // the filter "Sub": each byte less the byte one pixel to the left
        for (at, &byte) in line.iter().enumerate() {
            raw.push(byte.wrapping_sub(if at >= 4 { line[at - 4] } else { 0 }));
        }
    }
    let mut zlib = vec![0x78, 0x9c];
    zlib.extend(deflate(&raw));
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, red green blue alpha, deflate, filtered, not interlaced
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    held: u32,
    count: u32,
}

/// A canonical Huffman code: how many codes there are of each length, and the symbols in order.
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Huffman {
        let mut count = [0u16; 16];
        for &length in lengths {
            count[usize::from(length)] += 1;
        }
        let mut offsets = [0u16; 16];
        for length in 1..15 {
            offsets[length + 1] = offsets[length] + count[length];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (value, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbol[usize::from(offsets[usize::from(length)])] = value as u16;
                offsets[usize::from(length)] += 1;
            }
        }
        Huffman { count, symbol }
    }
}

impl Reader<'_> {
    fn bits(&mut self, need: u32) -> Result<u32, String> {
        while self.count < need {
            let byte = *self.data.get(self.at).ok_or("the compressed data ends too soon")?;
            self.at += 1;
            self.held |= u32::from(byte) << self.count;
            self.count += 8;
        }
        let value = self.held & ((1u32 << need) - 1);
        self.held >>= need;
        self.count -= need;
        Ok(value)
    }

    fn symbol(&mut self, code: &Huffman) -> Result<u16, String> {
        let (mut value, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..16 {
            value |= self.bits(1)? as i32;
            let count = i32::from(code.count[length]);
            if value - count < first {
                return Ok(code.symbol[(index + (value - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            value <<= 1;
        }
        Err("a code of the compressed data is not in its table".to_string())
    }
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = Reader {
        data,
        at: 0,
        held: 0,
        count: 0,
    };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = reader.bits(1)?;
        match reader.bits(2)? {
            0 => {
                reader.held = 0;
                reader.count = 0;
                let header = data.get(reader.at..reader.at + 4).ok_or("a stored block ends too soon")?;
                let length = usize::from(u16::from_le_bytes([header[0], header[1]]));
                reader.at += 4;
                out.extend_from_slice(data.get(reader.at..reader.at + length).ok_or("a stored block ends too soon")?);
                reader.at += length;
            }
            kind @ (1 | 2) => {
                let (literals, distances) = if kind == 1 {
                    let mut lengths = [8u8; 288];
                    lengths[144..256].fill(9);
                    lengths[256..280].fill(7);
                    (Huffman::new(&lengths), Huffman::new(&[5u8; 30]))
                } else {
                    let literal_count = reader.bits(5)? as usize + 257;
                    let distance_count = reader.bits(5)? as usize + 1;
                    let code_count = reader.bits(4)? as usize + 4;
                    let mut code_lengths = [0u8; 19];
                    for &place in CODE_LENGTH_ORDER.iter().take(code_count) {
                        code_lengths[place] = reader.bits(3)? as u8;
                    }
                    let code = Huffman::new(&code_lengths);
                    let mut lengths = vec![0u8; literal_count + distance_count];
                    let mut place = 0;
                    while place < lengths.len() {
                        let symbol = reader.symbol(&code)?;
                        let (value, repeat) = match symbol {
                            0..=15 => (symbol as u8, 1),
                            16 => (
                                *lengths.get(place.wrapping_sub(1)).ok_or("a repeat has nothing to repeat")?,
                                3 + reader.bits(2)? as usize,
                            ),
                            17 => (0, 3 + reader.bits(3)? as usize),
                            _ => (0, 11 + reader.bits(7)? as usize),
                        };
                        if place + repeat > lengths.len() {
                            return Err("the table of codes is too long".to_string());
                        }
                        lengths[place..place + repeat].fill(value);
                        place += repeat;
                    }
                    (Huffman::new(&lengths[..literal_count]), Huffman::new(&lengths[literal_count..]))
                };
                loop {
                    let symbol = reader.symbol(&literals)? as usize;
                    match symbol {
                        0..=255 => out.push(symbol as u8),
                        256 => break,
                        _ => {
                            let l = symbol - 257;
                            if l >= 29 {
                                return Err("a length of the compressed data is not one".to_string());
                            }
                            let length = usize::from(LENGTH_BASE[l]) + reader.bits(u32::from(LENGTH_EXTRA[l]))? as usize;
                            let d = reader.symbol(&distances)? as usize;
                            if d >= 30 {
                                return Err("a distance of the compressed data is not one".to_string());
                            }
                            let distance = usize::from(DISTANCE_BASE[d]) + reader.bits(u32::from(DISTANCE_EXTRA[d]))? as usize;
                            if distance > out.len() {
                                return Err("a distance reaches before the start of the data".to_string());
                            }
                            for _ in 0..length {
                                out.push(out[out.len() - distance]);
                            }
                        }
                    }
                }
            }
            _ => return Err("a block of the compressed data has a kind that does not exist".to_string()),
        }
        if last == 1 {
            return Ok(out);
        }
    }
}

/// The width, the height and the bytes (red, green, blue, alpha; row by row from the top) of a PNG
/// file of 8 bits and four channels, not interlaced.
pub fn decode(png: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if png.len() < 8 || png[..8] != [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        return Err("not a PNG file".to_string());
    }
    let mut at = 8;
    let (mut width, mut height) = (0usize, 0usize);
    let mut compressed = Vec::new();
    while at + 12 <= png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let kind = &png[at + 4..at + 8];
        let data = png.get(at + 8..at + 8 + length).ok_or("a chunk ends too soon")?;
        let expected = u32::from_be_bytes(png[at + 8 + length..at + 12 + length].try_into().unwrap());
        if crc32(&[kind, data]) != expected {
            return Err("a chunk has a check value that is not its own".to_string());
        }
        match kind {
            b"IHDR" => {
                width = u32::from_be_bytes(data[0..4].try_into().unwrap()) as usize;
                height = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
                if data[8..13] != [8, 6, 0, 0, 0] {
                    return Err("only 8 bits of red, green, blue and alpha, not interlaced, are read".to_string());
                }
            }
            b"IDAT" => compressed.extend_from_slice(data),
            _ => {}
        }
        at += 12 + length;
    }
    let raw = inflate(compressed.get(2..).ok_or("the compressed data is missing")?)?;
    let row = width * 4;
    if raw.len() != (row + 1) * height {
        return Err(format!(
            "the picture holds {} bytes, not the {} of {width}x{height}",
            raw.len(),
            (row + 1) * height
        ));
    }
    let mut pixels = vec![0u8; row * height];
    for y in 0..height {
        let filter = raw[y * (row + 1)];
        for x in 0..row {
            let byte = raw[y * (row + 1) + 1 + x];
            let left = if x >= 4 { pixels[y * row + x - 4] } else { 0 };
            let up = if y > 0 { pixels[(y - 1) * row + x] } else { 0 };
            let up_left = if x >= 4 && y > 0 { pixels[(y - 1) * row + x - 4] } else { 0 };
            pixels[y * row + x] = byte.wrapping_add(match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => {
                    let (a, b, c) = (i32::from(left), i32::from(up), i32::from(up_left));
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    (if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }) as u8
                }
                _ => return Err(format!("a row has the filter {filter}, which does not exist")),
            });
        }
    }
    Ok((width as u32, height as u32, pixels))
}
