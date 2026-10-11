use model::image::is_animated;

/// A GIF with the given number of 1x1 pictures.
fn gif(frames: usize) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    // Screen 1x1, a two-colour table, then the table.
    bytes.extend([1, 0, 1, 0, 0x80, 0, 0]);
    bytes.extend([0, 0, 0, 255, 255, 255]);
    for _ in 0..frames {
        // A graphic control extension, then a picture with its data.
        bytes.extend([0x21, 0xF9, 4, 0, 10, 0, 0, 0]);
        bytes.extend([0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
        bytes.extend([2, 2, 0x44, 0x01, 0]);
    }
    bytes.push(0x3B);
    bytes
}

fn png(chunks: &[&[u8; 4]]) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    for kind in chunks {
        bytes.extend(4u32.to_be_bytes());
        bytes.extend(*kind);
        bytes.extend([0u8; 8]);
    }
    bytes
}

fn webp(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    for (kind, data) in chunks {
        body.extend(*kind);
        body.extend((data.len() as u32).to_le_bytes());
        body.extend(*data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(body);
    bytes
}

fn avif(brands: &[&[u8; 4]]) -> Vec<u8> {
    let mut bytes = ((8 + 4 * brands.len() + 4) as u32).to_be_bytes().to_vec();
    bytes.extend(b"ftyp");
    bytes.extend(brands[0]);
    bytes.extend([0u8; 4]);
    for brand in &brands[1..] {
        bytes.extend(*brand);
    }
    bytes
}

#[test]
fn t030_c1_a_gif_with_more_than_one_picture_is_animated() {
    assert!(!is_animated(&gif(1)));
    assert!(is_animated(&gif(2)));
    assert!(is_animated(&gif(5)));
}

#[test]
fn t030_c1_png_webp_and_avif_animation_is_recognised() {
    assert!(!is_animated(&png(&[b"IHDR", b"IDAT", b"IEND"])));
    assert!(is_animated(&png(&[b"IHDR", b"acTL", b"IDAT", b"IEND"])));
    // The marker only counts before the picture data.
    assert!(!is_animated(&png(&[b"IHDR", b"IDAT", b"acTL", b"IEND"])));

    assert!(!is_animated(&webp(&[(b"VP8 ", &[0; 5])])));
    assert!(!is_animated(&webp(&[
        (b"VP8X", &[0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (b"VP8L", &[0; 4])
    ])));
    assert!(is_animated(&webp(&[(b"VP8X", &[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0])])));
    assert!(is_animated(&webp(&[(b"VP8X", &[0; 10]), (b"ANIM", &[0; 6])])));

    assert!(!is_animated(&avif(&[b"avif", b"mif1", b"miaf"])));
    assert!(is_animated(&avif(&[b"avis", b"msf1"])));
    assert!(is_animated(&avif(&[b"avif", b"mif1", b"avis"])));
}

#[test]
fn t030_c1_an_svg_that_animates_itself_is_recognised() {
    let still = "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>rect { fill: red }</style><rect width=\"4\" height=\"4\"/></svg>";
    assert!(!is_animated(still.as_bytes()));
    for moving in [
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"4\"><animate attributeName=\"x\" dur=\"1s\"/></rect></svg>",
        "<?xml version=\"1.0\"?>\n<svg><g><animateTransform type=\"rotate\"/></g></svg>",
        "<svg><style>@keyframes spin { to { rotate: 1turn } }</style></svg>",
        "<svg><rect style=\"animation: spin 1s\"/></svg>",
    ] {
        assert!(is_animated(moving.as_bytes()), "{moving}");
    }
}

#[test]
fn other_files_and_cut_off_files_are_not_animated() {
    for bytes in [
        &b""[..],
        b"GIF89a",
        b"\x89PNG\r\n\x1a\n",
        b"RIFF\x00\x00\x00\x00WEBP",
        b"plain text",
        b"\x00\x00\x00\x08ftyp",
    ] {
        assert!(!is_animated(bytes));
    }
    let mut cut = gif(3);
    cut.truncate(30);
    let _ = is_animated(&cut);
}
