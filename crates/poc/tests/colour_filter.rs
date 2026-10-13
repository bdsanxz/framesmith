//! The colour filter of a picture or a video: each step as CSS defines it, clamped after it.

use poc::style::{apply_color_ops, color_ops, ColorOp};

/// The filter worked out the long way, in fractions.
fn by_definition(ops: &[ColorOp], colour: [u8; 3]) -> [f32; 3] {
    let mut c = colour.map(|value| f32::from(value) / 255.0);
    for op in ops {
        c = match *op {
            ColorOp::Brightness(amount) => c.map(|v| v * amount),
            ColorOp::Contrast(amount) => c.map(|v| (v - 0.5) * amount + 0.5),
            ColorOp::Saturate(amount) => mixed(c, amount),
            ColorOp::Grayscale(amount) => mixed(c, 1.0 - amount.clamp(0.0, 1.0)),
            ColorOp::Table(_) => unreachable!("these filters have no table"),
        }
        .map(|v| v.clamp(0.0, 1.0));
    }
    c.map(|v| v * 255.0)
}

/// Keeps `s` of the colour and replaces the rest by its grey.
fn mixed(c: [f32; 3], s: f32) -> [f32; 3] {
    let grey = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    c.map(|v| grey + (v - grey) * s)
}

/// A spread of colours: greys, the corners of the cube and mixed ones.
fn colours() -> Vec<[u8; 3]> {
    let mut all = Vec::new();
    for r in (0..=255).step_by(15) {
        for g in (0..=255).step_by(17) {
            for b in (0..=255).step_by(51) {
                all.push([r as u8, g as u8, b as u8]);
            }
        }
    }
    all.extend((0..=255).map(|grey| [grey as u8; 3]));
    all
}

#[test]
fn every_order_of_steps_gives_the_colour_of_the_definition() {
    for text in [
        "brightness(0.6) contrast(1.14)",
        "brightness(0.6) contrast(1.14) saturate(0.8)",
        "saturate(0.9) contrast(1.05)",
        "grayscale(1) brightness(0.55) contrast(1.2)",
        "brightness(1.3) saturate(1.6) contrast(0.8) brightness(0.9)",
        "saturate(1.4) grayscale(0.5)",
        "contrast(1.2) saturate(0.5) brightness(1.1) grayscale(0.3) contrast(0.9)",
    ] {
        let ops = color_ops(text).expect("the filter is valid");
        let colours = colours();
        let mut pixels: Vec<u8> = colours.iter().flat_map(|c| [c[0], c[1], c[2], 200]).collect();
        apply_color_ops(&ops, &mut pixels);
        for (colour, pixel) in colours.iter().zip(pixels.as_chunks::<4>().0) {
            let expected = by_definition(&ops, *colour);
            for channel in 0..3 {
                let apart = (f32::from(pixel[channel]) - expected[channel]).abs();
                assert!(apart <= 1.0, "`{text}` on {colour:?}: {pixel:?}, by the definition {expected:?}");
            }
            assert_eq!(pixel[3], 200, "alpha is left as it is");
        }
    }
}

#[test]
fn a_large_picture_is_coloured_like_a_small_one() {
    let ops = color_ops("saturate(0.9) contrast(1.05)").expect("the filter is valid");
    let mut small = [40, 90, 200, 255];
    apply_color_ops(&ops, &mut small);
    // Large enough to be shared out over several threads.
    let mut large = [40, 90, 200, 255].repeat(1 << 19);
    apply_color_ops(&ops, &mut large);
    assert!(large.as_chunks::<4>().0.iter().all(|pixel| *pixel == small));
}

#[test]
fn a_filter_without_steps_changes_nothing() {
    let mut pixels = vec![1, 2, 3, 4, 250, 251, 252, 253];
    apply_color_ops(&[], &mut pixels);
    assert_eq!(pixels, [1, 2, 3, 4, 250, 251, 252, 253]);
}
