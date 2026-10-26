//! The sixteen blends (`docs/spec/blends.md`, sections 3 and 4), each in a case of its own, with an opaque element over
//! an opaque colour, so that the result is `B(Cb, Cs)` in whole levels. Every expected colour is worked by hand from the
//! spec's formulas, for two pairs of colours whose channels are multiples of 51 (0.2 of the range):
//!
//! - pair A: the element (204, 51, 153) = (0.8, 0.2, 0.6) over (102, 153, 51) = (0.4, 0.6, 0.2);
//! - pair B: the element (51, 255, 102) = (0.2, 1, 0.4) over (204, 0, 255) = (0.8, 0, 1).
//!
//! The lightness (0.3 R + 0.59 G + 0.11 B) is 0.424 for A's element, 0.496 for what is under it, 0.694 for B's element and
//! 0.35 for what is under it; the saturations are 0.6, 0.4, 0.8 and 1.

use draw::Blend;

const A: ([u8; 3], [u8; 3]) = ([204, 51, 153], [102, 153, 51]);
const B: ([u8; 3], [u8; 3]) = ([51, 255, 102], [204, 0, 255]);

fn opaque(colour: [u8; 3]) -> [u8; 4] {
    [colour[0], colour[1], colour[2], 255]
}

/// Asserts that `blend` gives `a` for pair A and `b` for pair B, each channel within one level.
fn gives(blend: Blend, a: [u8; 3], b: [u8; 3]) {
    for (pair, (source, under), wanted) in [("A", A, a), ("B", B, b)] {
        let got = blend.pixel(opaque(source), opaque(under));
        assert_eq!(got[3], 255, "{}: pair {pair}: the alpha", blend.name());
        for channel in 0..3 {
            assert!(
                got[channel].abs_diff(wanted[channel]) <= 1,
                "{}: pair {pair}: {got:?} against the worked {wanted:?}",
                blend.name()
            );
        }
    }
}

#[test]
fn normal_gives_the_elements_colour() {
    gives(Blend::Normal, [204, 51, 153], [51, 255, 102]);
}

#[test]
fn multiply_gives_the_product() {
    // A: 0.32, 0.12, 0.12 → 81.6, 30.6, 30.6. B: 0.16, 0, 0.4 → 40.8, 0, 102.
    gives(Blend::Multiply, [82, 31, 31], [41, 0, 102]);
}

#[test]
fn screen_gives_the_inverse_of_the_product_of_the_inverses() {
    // A: 0.4 + 0.8 - 0.32 = 0.88, 0.68, 0.68 → 224.4, 173.4, 173.4. B: 0.84, 1, 1 → 214.2, 255, 255.
    gives(Blend::Screen, [224, 173, 173], [214, 255, 255]);
}

#[test]
fn overlay_is_the_hard_light_with_the_colours_exchanged() {
    // A: under 0.4 → 2 · 0.4 · 0.8 = 0.64; under 0.6 → screen(0.2, 0.2) = 0.36; under 0.2 → 2 · 0.2 · 0.6 = 0.24.
    // B: under 0.8 → screen(0.2, 0.6) = 0.68; under 0 → 0; under 1 → screen(0.4, 1) = 1.
    gives(Blend::Overlay, [163, 92, 61], [173, 0, 255]);
}

#[test]
fn darken_gives_the_lesser() {
    gives(Blend::Darken, [102, 51, 51], [51, 0, 102]);
}

#[test]
fn lighten_gives_the_greater() {
    gives(Blend::Lighten, [204, 153, 153], [204, 255, 255]);
}

#[test]
fn color_dodge_divides_by_the_inverse_of_the_element() {
    // A: 0.4 / 0.2 → 1; 0.6 / 0.8 = 0.75 → 191.25; 0.2 / 0.4 = 0.5 → 127.5. B: 0.8 / 0.8 = 1; under 0 → 0; under 1 → 1.
    gives(Blend::ColorDodge, [255, 191, 128], [255, 0, 255]);
}

#[test]
fn color_burn_divides_the_inverse_of_what_is_under_by_the_element() {
    // A: 1 - 0.6 / 0.8 = 0.25 → 63.75; 1 - min(1, 0.4 / 0.2) = 0; 1 - min(1, 0.8 / 0.6) = 0.
    // B: 1 - 0.2 / 0.2 = 0; 1 - min(1, 1 / 1) = 0; under 1 → 1.
    gives(Blend::ColorBurn, [64, 0, 0], [0, 0, 255]);
}

#[test]
fn hard_light_multiplies_or_screens_by_the_element() {
    // A: element 0.8 → screen(0.4, 0.6) = 0.76 → 193.8; 0.2 → 0.6 · 0.4 = 0.24 → 61.2; 0.6 → screen(0.2, 0.2) = 0.36 → 91.8.
    // B: 0.2 → 0.8 · 0.4 = 0.32 → 81.6; 1 → screen(0, 1) = 1; 0.4 → 1 · 0.8 = 0.8 → 204.
    gives(Blend::HardLight, [194, 61, 92], [82, 255, 204]);
}

#[test]
fn soft_light_follows_the_recommendations_formula() {
    // A red: element 0.8 over 0.4: 0.4 + 0.6 · (√0.4 - 0.4) = 0.4 + 0.6 · 0.232456 = 0.539473 → 137.57.
    // A green: element 0.2 over 0.6: 0.6 - 0.6 · 0.6 · 0.4 = 0.456 → 116.28.
    // A blue: element 0.6 over 0.2: D(0.2) = ((3.2 - 12) · 0.2 + 4) · 0.2 = 0.448; 0.2 + 0.2 · 0.248 = 0.2496 → 63.65.
    // B red: element 0.2 over 0.8: 0.8 - 0.6 · 0.8 · 0.2 = 0.704 → 179.52; green: over 0, D(0) = 0 → 0; blue: over 1 → 1.
    gives(Blend::SoftLight, [138, 116, 64], [180, 0, 255]);
    // Where what is under is dark, D is the cubic, not the root: white over 13 / 255 = 0.050980 gives D(b) =
    // ((16 · 0.050980 - 12) · 0.050980 + 4) · 0.050980 = 3.429820 · 0.050980 = 0.174854 → 44.59, where √b would give 57.58.
    assert_eq!(Blend::SoftLight.pixel([255, 255, 255, 255], [13, 13, 13, 255]), [45, 45, 45, 255]);
}

#[test]
fn difference_gives_the_distance() {
    gives(Blend::Difference, [102, 102, 102], [153, 255, 153]);
}

#[test]
fn exclusion_gives_the_sum_less_twice_the_product() {
    // A: 0.4 + 0.8 - 0.64 = 0.56 → 142.8, and 0.56 twice more. B: 0.68 → 173.4; 1; 1 + 0.4 - 0.8 = 0.6 → 153.
    gives(Blend::Exclusion, [143, 143, 143], [173, 255, 153]);
}

#[test]
fn hue_takes_the_elements_hue_with_the_saturation_and_lightness_under_it() {
    // A: the element at saturation 0.4: (0.4, 0, 0.266667), lightness 0.149333; raised by 0.346667 to 0.496:
    // (0.746667, 0.346667, 0.613333) → 190.4, 88.4, 156.4.
    // B: the element at saturation 1: (0, 1, 0.25), lightness 0.6175; lowered by 0.2675: (-0.2675, 0.7325, -0.0175),
    // brought into range about 0.35 by 0.35 / 0.6175: (0, 0.566802, 0.141700) → 0, 144.53, 36.13.
    gives(Blend::Hue, [190, 88, 156], [0, 145, 36]);
}

#[test]
fn saturation_takes_the_elements_saturation_with_the_hue_and_lightness_under_it() {
    // A: what is under at saturation 0.6: (0.3, 0.6, 0), lightness 0.444; raised by 0.052: (0.352, 0.652, 0.052) →
    // 89.76, 166.26, 13.26.
    // B: what is under at saturation 0.8: (0.64, 0, 0.8), lightness 0.28; raised by 0.07: (0.71, 0.07, 0.87) → 181.05,
    // 17.85, 221.85.
    gives(Blend::Saturation, [90, 166, 13], [181, 18, 222]);
}

#[test]
fn color_takes_the_elements_hue_and_saturation_with_the_lightness_under_it() {
    // A: the element raised by 0.072: (0.872, 0.272, 0.672) → 222.36, 69.36, 171.36.
    // B: the element lowered by 0.344: (-0.144, 0.656, 0.056), brought into range about 0.35 by 0.35 / 0.494:
    // (0, 0.566802, 0.141700) → 0, 144.53, 36.13.
    gives(Blend::Color, [222, 69, 171], [0, 145, 36]);
}

#[test]
fn luminosity_takes_the_elements_lightness_with_the_hue_and_saturation_under_it() {
    // A: what is under lowered by 0.072: (0.328, 0.528, 0.128) → 83.64, 134.64, 32.64.
    // B: what is under raised by 0.344: (1.144, 0.344, 1.344), brought into range about 0.694 by 0.306 / 0.65:
    // (0.905846, 0.529231, 1) → 230.99, 134.95, 255.
    gives(Blend::Luminosity, [84, 135, 33], [231, 135, 255]);
}

#[test]
fn the_sixteen_names_are_read_in_the_notations_order_and_no_other() {
    let names: Vec<&str> = Blend::ALL.iter().map(|blend| blend.name()).collect();
    assert_eq!(
        names,
        [
            "normal",
            "multiply",
            "screen",
            "overlay",
            "darken",
            "lighten",
            "color-dodge",
            "color-burn",
            "hard-light",
            "soft-light",
            "difference",
            "exclusion",
            "hue",
            "saturation",
            "color",
            "luminosity"
        ]
    );
    for blend in Blend::ALL {
        assert_eq!(Blend::named(blend.name()), Some(blend));
    }
    for name in ["Multiply", "burn", "", "plus-lighter"] {
        assert_eq!(Blend::named(name), None, "{name:?}");
    }
}

#[test]
fn a_clear_element_leaves_what_is_under_it_and_a_half_clear_one_mixes_by_its_alpha() {
    // The element premultiplied (102, 0, 0, 102) is red at alpha 0.4 over opaque (0, 0, 255), by multiply: B = (0, 0, 0),
    // so the colour is 0.6 · (0, 0, 1) + 0.4 · (0, 0, 0) = (0, 0, 0.6) → (0, 0, 153); the alpha stays 1.
    assert_eq!(Blend::Multiply.pixel([102, 0, 0, 102], [0, 0, 255, 255]), [0, 0, 153, 255]);
    assert_eq!(Blend::Screen.pixel([0, 0, 0, 0], [10, 20, 30, 255]), [10, 20, 30, 255]);
    // Over a clear pixel the element is laid as it is: cs · (1 - 0) = cs.
    assert_eq!(Blend::Difference.pixel([40, 80, 120, 200], [0, 0, 0, 0]), [40, 80, 120, 200]);
}
