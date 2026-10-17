//! The letter spacing widens a line by its amount for each letter, the last letter included (`docs/spec/text.md`,
//! section 6.1).

mod common;

use common::{fonts, style};
use text::layout;

#[test]
fn letter_spacing_of_ten_pixels_widens_two_letters_by_twenty() {
    let mut fonts = fonts();
    let plain = layout(&mut fonts, "AB", &style(40.0), None).unwrap().width;
    let mut spaced = style(40.0);
    spaced.letter_spacing = 10.0;
    let widened = layout(&mut fonts, "AB", &spaced, None).unwrap().width;
    assert!((widened - plain - 20.0).abs() < 0.01, "{plain} and {widened}");
}

#[test]
fn letter_spacing_widens_a_line_by_its_amount_for_each_of_its_letters() {
    let mut fonts = fonts();
    let plain = layout(&mut fonts, "ABCDE", &style(40.0), None).unwrap().width;
    let mut spaced = style(40.0);
    spaced.letter_spacing = 5.0;
    let widened = layout(&mut fonts, "ABCDE", &spaced, None).unwrap().width;
    assert!((widened - plain - 25.0).abs() < 0.01, "{plain} and {widened}");
}

#[test]
fn a_negative_letter_spacing_narrows_a_line_by_its_amount_for_each_letter() {
    let mut fonts = fonts();
    let plain = layout(&mut fonts, "ABC", &style(40.0), None).unwrap().width;
    let mut tight = style(40.0);
    tight.letter_spacing = -5.0;
    let narrowed = layout(&mut fonts, "ABC", &tight, None).unwrap().width;
    assert!((plain - narrowed - 15.0).abs() < 0.01, "{plain} and {narrowed}");
}
