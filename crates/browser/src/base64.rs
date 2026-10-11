//! Base64, as the DevTools protocol carries file bodies and pictures.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[(n >> 18) as usize & 63]);
        out.push(ALPHABET[(n >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] } else { b'=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] } else { b'=' });
    }
    // Only ASCII was pushed.
    String::from_utf8(out).unwrap_or_default()
}

const fn reverse() -> [u8; 256] {
    let mut table = [255u8; 256];
    let mut i = 0;
    while i < 64 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
}

const REVERSE: [u8; 256] = reverse();

/// Decodes standard base64. Returns `None` on any character outside the
/// alphabet or on a wrong length.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.trim_end_matches('=').as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3 + 3);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &b) in chunk.iter().enumerate() {
            let value = REVERSE[b as usize];
            if value == 255 {
                return None;
            }
            n |= u32::from(value) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}
