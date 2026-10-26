//! How a number is written (`docs/spec/notation.md` section 4 and rule N9): an optional sign, digits
//! with an optional point, an optional exponent; finite. A number with an exponent and one with a
//! sign are read; `inf`, `nan`, a unit and a value too large to be finite are reported.

mod common;

use common::*;
use notation::{read_root, Code};

/// Valid files that each write `value` in an attribute whose form is a number: the name of the
/// place and the file.
fn with_number(value: &str) -> Vec<(&'static str, String)> {
    vec![
        ("x of a box", document(&format!("<box x=\"{value}\"/>"))),
        ("opacity of a rect", document(&format!("<rect opacity=\"{value}\"/>"))),
        (
            "scale-x of an image",
            document(&format!("<image src=\"i.png\" scale-x=\"{value}\"/>")),
        ),
        (
            "rotation of a shape",
            document(&format!("<shape d=\"M0 0\" rotation=\"{value}\"/>")),
        ),
        ("font-size of a text", document(&format!("<text font-size=\"{value}\"/>"))),
        ("typed of a span", document(&format!("<text>\n<span typed=\"{value}\"/>\n</text>"))),
        ("gap of a box", document(&format!("<box gap=\"{value}\"/>"))),
        (
            "border-radius of a video",
            document(&format!(
                "<video src=\"v.mp4\" width=\"2\" height=\"2\" border-radius=\"{value}\"/>"
            )),
        ),
        (
            "perspective of a nested composition",
            document(&format!("<composition src=\"a.xml\" perspective=\"{value}\"/>")),
        ),
        (
            "stroke-width of a shape",
            document(&format!("<shape d=\"M0 0\" stroke-width=\"{value}\"/>")),
        ),
        (
            "volume of an audio",
            document(&format!("<audio src=\"a.wav\" duration=\"1\" volume=\"{value}\"/>")),
        ),
        (
            "value of a volume",
            document(&format!(
                "<audio src=\"a.wav\" duration=\"1\">\n<volume value=\"{value}\"/>\n</audio>"
            )),
        ),
        (
            "from-x of a tween",
            document(&format!("<box>\n<tween duration=\"1\" from-x=\"{value}\"/>\n</box>")),
        ),
        (
            "to-opacity of a tween",
            document(&format!("<box>\n<tween duration=\"1\" to-opacity=\"{value}\"/>\n</box>")),
        ),
        (
            "to-typed of a tween",
            document(&format!("<text>\n<tween duration=\"1\" to-typed=\"{value}\"/>\n</text>")),
        ),
        ("y of a set", document(&format!("<box>\n<set y=\"{value}\"/>\n</box>"))),
        (
            "count of a set",
            document(&format!("<text count=\"0\">\n<set count=\"{value}\"/>\n</text>")),
        ),
    ]
}

#[test]
fn a_number_written_with_a_sign_or_an_exponent_is_read() {
    for value in [
        "12",
        "-4",
        "+5",
        "+0.5",
        "-0.5",
        ".5",
        "-.5",
        "+.5",
        "5.",
        "-5.",
        "0",
        "-0",
        "007",
        "1e3",
        "1E3",
        "1e+3",
        "1e-3",
        "2.5E-1",
        "-2.5e+2",
        ".5e1",
        "5.e2",
        "+1e0",
        "123456789012345678901234567890",
        "1e300",
        "-1e300",
        "1e-400",
        " 12 ",
        "\t-4\n",
    ] {
        for (place, text) in with_number(value) {
            let problems = problems_of(&text);
            assert!(problems.is_empty(), "{place}={value:?}: {problems:#?}");
        }
    }
}

#[test]
fn a_number_that_is_not_finite_or_not_written_as_a_number_is_reported() {
    for value in [
        "",
        " ",
        "+",
        "-",
        ".",
        "+.",
        "-.",
        "e3",
        "1e",
        "1e+",
        "1e-",
        "1e3.5",
        "1e3e3",
        "1.2.3",
        "--1",
        "+-1",
        "-+1",
        "1-",
        "1 2",
        "1,5",
        "1_000",
        "12px",
        "12 px",
        "50%",
        "0x10",
        "inf",
        "-inf",
        "+inf",
        "INF",
        "Infinity",
        "-infinity",
        "nan",
        "NaN",
        "-nan",
        "one",
        "abc",
        "٣",
        "１２",
        "=width/2",
        "(1)",
        "1/2",
    ] {
        for (place, text) in with_number(value) {
            let problems = problems_of(&text);
            assert_eq!(problems.len(), 1, "{place}={value:?}: {problems:#?}");
            assert_eq!(problems[0].code(), Code::NotANumber, "{place}={value:?}");
            let name = place.split(' ').next().unwrap();
            assert_eq!(
                problems[0].message(),
                format!(
                    "`{name}=\"{}\"` is not a number: an optional sign, digits with an optional point and an optional exponent, such as 12, -4, .5 or 2.5e-1",
                    value.trim_matches([' ', '\t', '\n'])
                ),
                "{place}"
            );
        }
    }
}

#[test]
fn a_value_too_large_to_be_finite_is_reported_as_too_large() {
    let huge = "9".repeat(400);
    for value in [
        "1e999",
        "-1e999",
        "1e309",
        "+1E400",
        huge.as_str(),
        "0.0e0001e",
        "1e99999999999999999999",
    ] {
        for (place, text) in with_number(value) {
            let problems = problems_of(&text);
            assert_eq!(problems.len(), 1, "{place}={value:?}: {problems:#?}");
            assert_eq!(problems[0].code(), Code::NotANumber, "{place}={value:?}");
        }
    }
    // The wording tells a number that is too large from one that is not written as a number.
    let (_, text) = with_number("1e999").remove(0);
    assert_eq!(
        problems_of(&text)[0].message(),
        "`x=\"1e999\"` is too large: a number must be finite"
    );
}

#[test]
fn the_width_and_height_of_a_video_are_whole_numbers() {
    for value in ["2", "640", "1.28e3", "+720", "6.4e2", "720.0", " 640 "] {
        let text = document(&format!("<video src=\"v.mp4\" width=\"{value}\" height=\"{value}\"/>"));
        assert!(problems_of(&text).is_empty(), "{value}: {:?}", problems_of(&text));
    }
    for value in ["10.5", "1e-1", "abc", "", "50%", "inf", "1.5e0"] {
        let text = document(&format!("<video src=\"v.mp4\" width=\"{value}\" height=\"640\"/>"));
        let problems = problems_of(&text);
        assert_eq!(problems.len(), 1, "{value}: {problems:#?}");
        assert_eq!(problems[0].code(), Code::NotANumber);
        assert_eq!(problems[0].message(), format!("`width=\"{value}\"` is not a whole number"));
    }
}

#[test]
fn a_number_that_is_reported_has_the_line_of_its_attribute() {
    let text = document("<box\n x=\"1\"\n y=\"12px\"\n opacity=\"inf\"/>");
    let places: Vec<(Code, u32)> = problems_of(&text).iter().map(|problem| (problem.code(), problem.line())).collect();
    assert_eq!(places, [(Code::NotANumber, 4), (Code::NotANumber, 5)]);
}

#[test]
fn attributes_that_are_lengths_or_lists_are_not_judged_as_numbers_here() {
    // Their forms belong to the part that draws the element: a percentage is a length, and a list
    // is a list.
    let text = document(
        "<box left=\"50%\" width=\"10px\" padding=\"1 2\" margin=\"a\" line-height=\"normal\" letter-spacing=\"0.1em\" color=\"red\"/>",
    );
    assert!(read_root(text.as_bytes()).is_ok(), "{:?}", problems_of(&text));
}

#[test]
fn the_value_of_an_attribute_in_a_style_is_judged_on_the_element_that_takes_it() {
    let text = document("<style id=\"s\" x=\"nonsense\" start=\"nonsense\" font-size=\"big\"/>");
    assert!(read_root(text.as_bytes()).is_ok(), "{:?}", problems_of(&text));
}
