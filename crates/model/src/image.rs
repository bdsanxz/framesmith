//! Tells an animated image file from a still one by reading its structure.

use crate::html;

fn be32(bytes: &[u8], at: usize) -> Option<usize> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

fn le32(bytes: &[u8], at: usize) -> Option<usize> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

/// Counts the pictures in a GIF, stopping at two.
fn gif_frames(bytes: &[u8]) -> usize {
    let table = |flags: u8| if flags & 0x80 != 0 { 3usize << ((flags & 7) + 1) } else { 0 };
    let Some(&flags) = bytes.get(10) else {
        return 0;
    };
    let mut pos = 13 + table(flags);
    let mut frames = 0;
    let skip_blocks = |mut pos: usize| loop {
        match bytes.get(pos) {
            None => return pos,
            Some(0) => return pos + 1,
            Some(&size) => pos += 1 + size as usize,
        }
    };
    while frames < 2 {
        match bytes.get(pos) {
            Some(0x21) => pos = skip_blocks(pos + 2),
            Some(0x2C) => {
                frames += 1;
                let Some(&flags) = bytes.get(pos + 9) else {
                    break;
                };
                pos = skip_blocks(pos + 10 + table(flags) + 1);
            }
            _ => break,
        }
    }
    frames
}

/// A PNG is animated when an animation control chunk comes before its picture data.
fn png_animated(bytes: &[u8]) -> bool {
    let mut pos = 8;
    while let (Some(size), Some(kind)) = (be32(bytes, pos), bytes.get(pos + 4..pos + 8)) {
        match kind {
            b"acTL" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => pos += 12 + size,
        }
    }
    false
}

fn webp_animated(bytes: &[u8]) -> bool {
    let mut pos = 12;
    while let (Some(kind), Some(size)) = (bytes.get(pos..pos + 4), le32(bytes, pos + 4)) {
        match kind {
            b"ANIM" | b"ANMF" => return true,
            b"VP8X" if bytes.get(pos + 8).is_some_and(|flags| flags & 0x02 != 0) => return true,
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
    false
}

/// An AVIF is a sequence when its file type box lists the sequence brand.
fn avif_sequence(bytes: &[u8]) -> bool {
    let Some(size) = be32(bytes, 0) else {
        return false;
    };
    let Some(brands) = bytes.get(8..size.min(bytes.len())) else {
        return false;
    };
    // The major brand, a version, then the compatible brands, four bytes each.
    brands
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .any(|(i, brand)| i != 1 && brand == b"avis")
}

fn svg_animates(text: &str) -> bool {
    let dom = html::parse(text);
    dom.elements.iter().any(|element| match element.tag.as_str() {
        "animate" | "animatetransform" | "animatemotion" | "set" | "script" => true,
        "style" => {
            let lower = element.text.to_ascii_lowercase();
            lower.contains("@keyframes") || lower.contains("animation") || lower.contains("transition")
        }
        _ => element
            .value("style")
            .is_some_and(|style| style.contains("animation") || style.contains("transition")),
    })
}

/// Whether the file holds an animated image: a GIF with more than one
/// picture, an animated PNG or WebP, an AVIF sequence, or an SVG that
/// animates itself. The kind is read from the content, not the file name.
pub fn is_animated(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return gif_frames(bytes) > 1;
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png_animated(bytes);
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return webp_animated(bytes);
    }
    if bytes.get(4..8) == Some(b"ftyp") {
        return avif_sequence(bytes);
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    if head.contains("<svg") {
        return svg_animates(&String::from_utf8_lossy(bytes));
    }
    false
}
