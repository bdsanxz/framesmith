//! The four steps of a colour `filter` on a still picture (`docs/spec/notation.md`, section 7.9), worked by hand
//! from the W3C Filter Effects specification (section 13.1) on the stored sRGB values: each step alone gives the colour
//! its arithmetic states within one level, two steps in the two orders give the two different results, `grayscale(1)`
//! equalises the channels, and the three identity steps change nothing.
//!
//! Proposal for T060 (spec-first; the implementation follows when the build constraint lifts): the steps are held as
//! `draw::colour_filter::ColourStep` and applied in order by `draw::colour_filter::apply_steps`.

use draw::colour_filter::{ColourStep, apply_steps};

/// A worked colour: warm, cool, white, black, pure red, green, blue, and mid grey.
const WARM: [u8; 3] = [200, 100, 50];
const COOL: [u8; 3] = [16, 32, 64];
const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// Asserts that every channel of `got` is within one level of `want`.
fn assert_within_one(got: [u8; 3], want: [u8; 3]) {
    for (channel, (got, want)) in got.iter().zip(want.iter()).enumerate() {
        assert!(
            got.abs_diff(*want) <= 1,
            "channel {channel}: got {got}, want {want} (within one level)"
        );
    }
}

#[test]
fn each_step_alone_gives_the_colour_its_arithmetic_states() {
    use ColourStep::{Brightness, Contrast, Grayscale, Saturate};
    let cases: &[(&[ColourStep], [u8; 3], [u8; 3])] = &[
        (&[Brightness(0.5)], WARM, [100, 50, 25]),
        (&[Brightness(0.5)], COOL, [8, 16, 32]),
        (&[Brightness(0.5)], RED, [128, 0, 0]),
        (&[Contrast(2.0)], WARM, [255, 73, 0]),
        (&[Contrast(2.0)], COOL, [0, 0, 1]),
        (&[Contrast(2.0)], GREEN, [0, 255, 0]),
        (&[Saturate(0.0)], WARM, [118, 118, 118]),
        (&[Saturate(0.0)], RED, [54, 54, 54]),
        (&[Saturate(0.0)], GREEN, [182, 182, 182]),
        (&[Saturate(0.0)], BLUE, [18, 18, 18]),
        (&[Saturate(0.5)], WARM, [159, 109, 84]),
        (&[Grayscale(1.0)], WARM, [118, 118, 118]),
        (&[Grayscale(1.0)], COOL, [31, 31, 31]),
        (&[Grayscale(1.0)], BLUE, [18, 18, 18]),
        (&[Grayscale(0.5)], WARM, [159, 109, 84]),
        (&[Brightness(0.5)], WHITE, [128, 128, 128]),
        (&[Contrast(2.0)], BLACK, [0, 0, 0]),
    ];
    for (steps, input, want) in cases {
        assert_within_one(apply_steps(steps, *input), *want);
    }
}

#[test]
fn two_steps_in_the_two_orders_give_the_two_different_results() {
    use ColourStep::{Brightness, Contrast, Saturate};
    let first = apply_steps(&[Brightness(0.5), Contrast(2.0)], WARM);
    let second = apply_steps(&[Contrast(2.0), Brightness(0.5)], WARM);
    assert_within_one(first, [73, 0, 0]);
    assert_within_one(second, [128, 37, 0]);
    assert_ne!(first, second, "the two orders give the same colours");

    let first = apply_steps(&[Saturate(0.0), Contrast(2.0)], WARM);
    let second = apply_steps(&[Contrast(2.0), Saturate(0.0)], WARM);
    assert_within_one(first, [109, 109, 109]);
    assert_within_one(second, [106, 106, 106]);
    assert_ne!(first, second, "the two orders give the same colours");
}

#[test]
fn full_grayscale_equalises_the_channels_and_the_identity_steps_change_nothing() {
    use ColourStep::{Brightness, Contrast, Grayscale, Saturate};
    for colour in [WARM, COOL, RED, GREEN, BLUE] {
        let grey = apply_steps(&[Grayscale(1.0)], colour);
        assert!(
            grey[0] == grey[1] && grey[1] == grey[2],
            "grayscale(1) of {colour:?} is {grey:?}, not equal channels"
        );
    }
    for colour in [WARM, COOL, WHITE, BLACK, RED, GREEN, BLUE] {
        assert_within_one(apply_steps(&[Saturate(1.0)], colour), colour);
        assert_within_one(apply_steps(&[Contrast(1.0)], colour), colour);
        assert_within_one(apply_steps(&[Brightness(1.0)], colour), colour);
    }
}
