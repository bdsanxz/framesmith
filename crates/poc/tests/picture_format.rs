//! The picture format the encoder is given and footage arrives in: YUV 4:2:0, BT.709, limited
//! range.

use poc::yuv;

/// A picture of one colour.
fn flat(width: usize, height: usize, colour: [u8; 3]) -> Vec<u8> {
    [colour[0], colour[1], colour[2], 255].repeat(width * height)
}

fn planes(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0; yuv::size(width, height)];
    yuv::from_rgba(rgba, width, height, &mut out);
    out
}

fn pixels(planes: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0; width * height * 4];
    yuv::to_rgba(planes, width, height, &mut out, |_| {});
    out
}

#[test]
fn the_plain_colours_have_their_bt709_limited_values() {
    for (colour, expected) in [
        ([255, 255, 255], [235, 128, 128]),
        ([0, 0, 0], [16, 128, 128]),
        ([128, 128, 128], [126, 128, 128]),
        ([255, 0, 0], [63, 102, 240]),
        ([0, 255, 0], [173, 42, 26]),
        ([0, 0, 255], [32, 240, 118]),
    ] {
        let out = planes(&flat(4, 2, colour), 4, 2);
        assert_eq!(out.len(), 12);
        assert!(out[..8].iter().all(|y| *y == expected[0]), "{colour:?}: Y {:?}", &out[..8]);
        assert_eq!(
            out[8..],
            [expected[1], expected[1], expected[2], expected[2]],
            "{colour:?}: Cb and Cr"
        );
    }
}

#[test]
fn a_chroma_sample_stands_at_the_left_column_of_its_block() {
    // Six columns, red from the fourth on: red, white, white | white, red, red ... per pair.
    let (width, height) = (6, 2);
    let mut rgba = flat(width, height, [255, 255, 255]);
    for pixel in rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
        .filter(|(index, _)| index % width >= 3)
    {
        pixel.1.copy_from_slice(&[255, 0, 0, 255]);
    }
    let out = planes(&rgba, width, height);
    let cr = &out[width * height + 3..];
    // The first block is white on both sides of its left column. The second stands on a white
    // column with red to its right only: a quarter red. The third is red on every side.
    assert_eq!(cr, [128, 156, 240]);
}

#[test]
fn a_colour_comes_back_from_the_planes_within_two_levels() {
    let (width, height) = (64, 128);
    for colour in [
        [255, 255, 255],
        [0, 0, 0],
        [200, 30, 90],
        [12, 180, 240],
        [90, 90, 90],
        [255, 255, 0],
    ] {
        let back = pixels(&planes(&flat(width, height, colour), width, height), width, height);
        assert_eq!(back.len(), width * height * 4);
        for pixel in back.as_chunks::<4>().0 {
            let apart = (0..3).map(|c| pixel[c].abs_diff(colour[c])).max();
            assert!(apart <= Some(2) && pixel[3] == 255, "{colour:?} came back as {pixel:?}");
        }
    }
}

#[test]
fn chroma_is_spread_evenly_between_its_samples() {
    // Two blocks side by side and two rows of blocks: Y is mid grey everywhere, Cb rises from
    // the left block to the right one.
    let (width, height) = (4, 4);
    let mut source = vec![126_u8; yuv::size(width, height)];
    source[16..20].copy_from_slice(&[100, 160, 100, 160]);
    let out = pixels(&source, width, height);
    let blue = |x: usize, y: usize| i32::from(out[(y * width + x) * 4 + 2]);
    // On the left sample, half way to the right one, on the right one, and past it at the edge.
    let (on_left, between, on_right, edge) = (blue(0, 1), blue(1, 1), blue(2, 1), blue(3, 1));
    assert!(on_left < between && between < on_right, "{on_left} {between} {on_right}");
    assert!(
        (2 * between - on_left - on_right).abs() <= 1,
        "the middle is half way: {on_left} {between} {on_right}"
    );
    assert_eq!(on_right, edge);
}

#[test]
fn each_row_is_handed_over_once_it_is_made() {
    let (width, height) = (8, 256);
    let source = planes(&flat(width, height, [10, 200, 30]), width, height);
    let mut out = vec![0; width * height * 4];
    // The hand-over may change the row: here it marks the alpha.
    yuv::to_rgba(&source, width, height, &mut out, |row| {
        assert_eq!(row.len(), width * 4);
        row.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 7);
    });
    assert!(out.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 7));
}

#[test]
fn only_a_picture_with_even_sides_fits() {
    assert!(yuv::fits(1920, 1080));
    assert!(!yuv::fits(811, 456));
    assert!(!yuv::fits(810, 457));
    assert!(!yuv::fits(0, 2));
    assert_eq!(yuv::size(1920, 1080), 1920 * 1080 * 3 / 2);
}
