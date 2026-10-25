//! How a time is written (`docs/spec/notation.md` section 4 and rule N8): digits, or digits, a point
//! and digits, or a point and digits. A time written without a leading digit is read; a point with no
//! digits after it, a sign, an exponent, a unit and an empty value are reported.

mod common;

use common::*;
use notation::{read_root, Code};

/// A valid file with one element that takes the time attribute `name`, written `value`.
fn with_time(name: &str, value: &str) -> String {
    match name {
        "start" => document(&format!("<box start=\"{value}\" duration=\"1\"/>")),
        "duration" => document(&format!("<box start=\"0\" duration=\"{value}\"/>")),
        "media-start" => document(&format!(
            "<video src=\"c.mp4\" width=\"640\" height=\"360\" media-start=\"{value}\"/>"
        )),
        "audio-start" => document(&format!("<audio src=\"a.wav\" start=\"{value}\" duration=\"1\"/>")),
        "audio-duration" => document(&format!("<audio src=\"a.wav\" duration=\"{value}\"/>")),
        "audio-media-start" => document(&format!("<audio src=\"a.wav\" duration=\"1\" media-start=\"{value}\"/>")),
        "volume-at" => document(&format!(
            "<audio src=\"a.wav\" duration=\"1\">\n<volume at=\"{value}\" value=\"1\"/>\n</audio>"
        )),
        "tween-at" => document(&format!("<box>\n<tween at=\"{value}\" duration=\"1\" to-x=\"1\"/>\n</box>")),
        "tween-duration" => document(&format!("<box>\n<tween duration=\"{value}\" to-x=\"1\"/>\n</box>")),
        "set-at" => document(&format!("<box>\n<set at=\"{value}\" x=\"1\"/>\n</box>")),
        "nested-start" => document(&format!("<composition src=\"a.xml\" start=\"{value}\"/>")),
        "nested-duration" => document(&format!("<composition src=\"a.xml\" duration=\"{value}\"/>")),
        other => panic!("{other}"),
    }
}

const PLACES: [&str; 12] = [
    "start",
    "duration",
    "media-start",
    "audio-start",
    "audio-duration",
    "audio-media-start",
    "volume-at",
    "tween-at",
    "tween-duration",
    "set-at",
    "nested-start",
    "nested-duration",
];

/// The name of the attribute in `place`.
fn attribute_of(place: &str) -> &str {
    place.rsplit_once('-').map_or(place, |(first, last)| match (first, last) {
        ("media", "start") => "media-start",
        ("audio-media", "start") => "media-start",
        ("volume", "at") | ("tween", "at") | ("set", "at") => "at",
        (_, last) => last,
    })
}

#[test]
fn a_time_written_without_a_leading_digit_is_read() {
    for place in PLACES {
        for value in [
            ".5",
            ".25",
            ".000000001",
            " .5 ",
            "0.5",
            "5",
            "25",
            "0",
            "000",
            "123456789.123456789",
            "000000001.5",
        ] {
            // A tween's duration must be above 0 as well, which is a rule of its own (`FS-0058`), not of the form.
            if place == "tween-duration" && value.trim().chars().all(|c| c == '0') {
                continue;
            }
            let text = with_time(place, value);
            assert!(read_root(text.as_bytes()).is_ok(), "{place}={value:?}: {:?}", problems_of(&text));
        }
    }
}

#[test]
fn a_point_with_no_digit_after_it_is_reported() {
    for place in PLACES {
        for value in ["5.", ".", "0.", "12."] {
            let text = with_time(place, value);
            let problems = problems_of(&text);
            assert_eq!(problems.len(), 1, "{place}={value:?}: {problems:#?}");
            assert_eq!(problems[0].code(), Code::NotATime, "{place}={value:?}");
            let name = attribute_of(place);
            assert_eq!(
                problems[0].message(),
                format!("`{name}=\"{value}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25"),
                "{place}"
            );
        }
    }
}

#[test]
fn an_empty_value_a_sign_an_exponent_a_unit_and_words_are_not_times() {
    for place in PLACES {
        for value in [
            "",
            "   ",
            "-1",
            "-0",
            "+1",
            "+.5",
            "-.5",
            "1e3",
            "1E3",
            "2.5e-1",
            "5s",
            "5 s",
            "500ms",
            "50%",
            "abc",
            "inf",
            "nan",
            "0x10",
            "1,5",
            "1_0",
            "1 2",
            "..5",
            "1.2.3",
            "٣",
            "1234567890",
            "0.1234567890",
            "intro",
            "intro+1",
            "auto",
            "@intro",
        ] {
            let text = with_time(place, value);
            let problems = problems_of(&text);
            assert_eq!(problems.len(), 1, "{place}={value:?}: {problems:#?}");
            assert_eq!(problems[0].code(), Code::NotATime, "{place}={value:?}");
            assert!(problems[0].message().contains("is not a time in seconds"), "{}", problems[0]);
        }
    }
}

#[test]
fn a_time_at_the_limits_of_nine_digits_on_each_side_is_read_and_one_digit_more_is_not() {
    let text = with_time("start", "999999999.999999999");
    assert!(read_root(text.as_bytes()).is_ok(), "{:?}", problems_of(&text));
    for value in ["9999999999", "0.9999999999", "1234567890.5", "0000000001.5"] {
        let text = with_time("start", value);
        assert_eq!(problems_of(&text)[0].code(), Code::NotATime, "{value}");
    }
}

#[test]
fn a_time_that_is_reported_has_the_line_of_its_attribute() {
    let text = document("<box\n start=\"1\"\n duration=\"5.\"/>");
    let problems = problems_of(&text);
    assert_eq!(problems.len(), 1);
    assert_eq!((problems[0].code(), problems[0].line()), (Code::NotATime, 4));
}

#[test]
fn a_time_without_a_leading_digit_is_the_same_time_as_one_with_it_for_the_root() {
    // The root's `duration` gives the frame count: .5 s and 0.5 s are the same 15 frames at 30.
    for (value, frames) in [(".5", 15), ("0.5", 15), ("2", 60), (".0333333", 1), ("1.0333", 31)] {
        let text = format!("<composition version=\"1\" width=\"640\" height=\"360\" duration=\"{value}\"/>");
        assert_eq!(read_root(text.as_bytes()).unwrap().frames, frames, "{value}");
    }
}
