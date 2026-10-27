//! A written length keeps every digit it is written with, its point and its exponent, so the layout
//! can work its percentages from the decimal and not from a float (`docs/spec/notation.md`, section 4).
//! The values here are read by hand from the written text.

mod common;

use common::document;
use notation::{read, Length};

fn length(value: &str) -> Option<Length> {
    let text = document(&format!("<box width=\"{value}\"/>"));
    let composition = read(text.as_bytes()).unwrap();
    composition.tree.nodes()[1].attribute("width").unwrap().length()
}

/// The parts of a written percentage: its sign, its digits and its exponent.
fn percent_parts(value: &str) -> (bool, String, i32) {
    match length(value) {
        Some(Length::Percent(number)) => (number.negative(), number.digits().to_string(), number.exponent()),
        other => panic!("{value} is not a percentage: {other:?}"),
    }
}

#[test]
fn a_percentage_keeps_every_digit_it_is_written_with() {
    // 38.4999999999999999 is 18 digits, the point taken out, at the exponent -16.
    assert_eq!(
        percent_parts("38.4999999999999999%"),
        (false, "384999999999999999".to_string(), -16)
    );
}

#[test]
fn a_written_exponent_is_taken_into_the_exponent_with_the_fraction_places() {
    // 2.5e1 is 25: the digits 25 at the exponent 0.
    assert_eq!(percent_parts("2.5e1%"), (false, "25".to_string(), 0));
    // 3.85e1 is 38.5: the digits 385 at the exponent -1.
    assert_eq!(percent_parts("3.85e1%"), (false, "385".to_string(), -1));
}

#[test]
fn a_tiny_percentage_keeps_its_exponent() {
    assert_eq!(percent_parts("1e-40%"), (false, "1".to_string(), -40));
    assert_eq!(percent_parts("-1e-300%"), (true, "1".to_string(), -300));
}

#[test]
fn a_length_says_whether_it_is_a_whole_number_of_pixels() {
    // 1.0 is kept as written, the digits 10 at the exponent -1, and it is whole: its fraction is a zero.
    match length("1.0") {
        Some(Length::Pixels(number)) => assert!(number.is_whole() && number.to_f64() == 1.0),
        other => panic!("1.0 is not a length in pixels: {other:?}"),
    }
    let fraction = match length("0.35") {
        Some(Length::Pixels(number)) => number,
        other => panic!("0.35 is not a length in pixels: {other:?}"),
    };
    assert!(!fraction.is_whole());
    let written = match length("3.85e1") {
        Some(Length::Pixels(number)) => number,
        other => panic!("3.85e1 is not a length in pixels: {other:?}"),
    };
    assert!(!written.is_whole(), "3.85e1 is 38.5, which has a fraction");
}

#[test]
fn a_number_too_large_for_a_float_is_still_not_a_length() {
    assert_eq!(length("1e400"), None);
    assert_eq!(length("inf"), None);
    assert_eq!(length("nan%"), None);
}

#[test]
fn a_fraction_of_zeros_is_whole_and_one_nonzero_digit_after_them_is_not() {
    // 1.0000000000000000000000 has 22 places after the point, all zeros: the digits are 1 followed by 22 zeros, at the
    // exponent -22, and the value is whole. 1.0000000000000000000001 has a 1 in the 22nd place: the digits are 1,
    // 21 zeros and a 1 (23 digits) at the exponent -22, and the value has a fraction, so it is not a whole number.
    let whole = match length("1.0000000000000000000000") {
        Some(Length::Pixels(number)) => number,
        other => panic!("1.0000000000000000000000 is not a length in pixels: {other:?}"),
    };
    assert_eq!((whole.digits(), whole.exponent()), ("10000000000000000000000", -22));
    assert!(whole.is_whole());
    let fraction = match length("1.0000000000000000000001") {
        Some(Length::Pixels(number)) => number,
        other => panic!("1.0000000000000000000001 is not a length in pixels: {other:?}"),
    };
    assert_eq!((fraction.digits(), fraction.exponent()), ("10000000000000000000001", -22));
    assert!(!fraction.is_whole());
}
