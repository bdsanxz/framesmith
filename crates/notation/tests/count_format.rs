//! The forms of a number that counts (`docs/spec/notation.md` section 7.12; `docs/spec/text.md` section 7.1, point 1):
//! the words each form writes for a number, worked out by hand from the table and its rounding rule, and the reports of a
//! `count` or a `count-format` written where or as the notation does not take it (`docs/spec/check-command.md`).

mod common;

use common::{problems_in, worded};
use notation::CountFormat;

fn written(format: &str, value: f64) -> String {
    CountFormat::read(format)
        .unwrap_or_else(|| panic!("{format} is a form"))
        .write(value)
}

#[test]
fn the_table_s_number_is_written_in_each_form_as_the_table_reads_it() {
    // 1234.567 as the table of section 7.12 reads it.
    for (format, words) in [
        ("0", "1235"),
        ("0.0", "1234.6"),
        ("0.00", "1234.57"),
        ("0,0", "1,235"),
        ("0,0.00", "1,234.57"),
        ("m:ss", "20:35"),
        ("m:ss.0", "20:34.6"),
    ] {
        assert_eq!(written(format, 1234.567), words, "{format}");
    }
}

#[test]
fn a_number_is_rounded_to_its_last_place_a_half_away_from_zero_before_it_is_written() {
    // 59.96 seconds round to 60.0 first, so they read 1:00.0, not 0:60.0.
    assert_eq!(written("m:ss.0", 59.96), "1:00.0");
    assert_eq!(written("0", 2.5), "3");
    assert_eq!(written("0", -2.5), "-3");
    // 1234.565 is written 1234.565 and rounds up at its third place, whatever its nearest binary value.
    assert_eq!(written("0.00", 1234.565), "1234.57");
    assert_eq!(written("0,0", 999_999.5), "1,000,000");
    assert_eq!(written("m:ss", 3599.5), "60:00");
}

#[test]
fn a_number_below_zero_has_a_minus_sign_unless_it_rounds_to_zero() {
    assert_eq!(written("0", -0.4), "0");
    assert_eq!(written("0.0", -0.04), "0.0");
    assert_eq!(written("0,0.00", -1234.5), "-1,234.50");
    assert_eq!(written("m:ss", -75.0), "-1:15");
}

#[test]
fn decimals_are_always_shown_and_the_thousands_are_counted_from_the_right() {
    assert_eq!(written("0.000000", 1.0), "1.000000");
    assert_eq!(written("0,0", 100.0), "100");
    assert_eq!(written("0,0", 1000.0), "1,000");
    assert_eq!(written("0,0.00", 12_345_678.0), "12,345,678.00");
    assert_eq!(written("m:ss.00", 5.0), "0:05.00");
}

#[test]
fn only_the_forms_of_the_table_are_read() {
    for form in ["0", "0.0", "0.000000", "0,0", "0,0.00", "m:ss", "m:ss.000000"] {
        assert!(CountFormat::read(form).is_some(), "{form}");
    }
    for form in [
        "",
        "0.0.0",
        "0.0000000",
        "0.",
        "00",
        "0,00",
        "#,##0",
        "mm:ss",
        "m:ss.",
        "m:ss.5",
        "h:mm:ss",
        " 0",
    ] {
        assert_eq!(CountFormat::read(form), None, "{form}");
    }
}

#[test]
fn count_on_a_box_is_reported_on_the_line_of_its_attribute() {
    let problems: Vec<String> = problems_in("<box\n count=\"5\"/>").iter().map(worded).collect();
    assert_eq!(problems, ["line 3: FS-0030: `count` is for a <text>, not <box>"]);
}

#[test]
fn a_format_the_table_does_not_list_is_reported_on_the_line_of_its_attribute() {
    let problems: Vec<String> = problems_in("<text count=\"0\"\n count-format=\"0.0.0\"/>")
        .iter()
        .map(worded)
        .collect();
    assert_eq!(
        problems,
        ["line 3: FS-0082: `count-format=\"0.0.0\"` is not a form of number; the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals"]
    );
}

#[test]
fn a_counting_text_with_words_and_a_count_without_its_attribute_are_reported() {
    let problems: Vec<String> =
        problems_in("<text count=\"0\">12</text>\n<text\n count-format=\"0\">\n<tween duration=\"1\"\n to-count=\"5\"/>\n</text>")
            .iter()
            .map(worded)
            .collect();
    assert_eq!(
        problems,
        [
            "line 2: FS-0082: a <text> that counts holds no words of its own: its text is the number",
            "line 4: FS-0082: `count-format` needs `count`",
            "line 6: FS-0082: a tween of `count` needs the attribute `count` on its <text>",
        ]
    );
}
