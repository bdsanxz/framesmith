//! A root with something wrong: the line, and the words of the message.

use notation::{read_root, Problem};

/// What these tests judge of a problem: its line, its code and its message. The kind and the remedy
/// that follow on the printed line belong to the code and are tested with the catalogue.
trait Worded {
    fn worded(&self) -> String;
}

impl Worded for Problem {
    fn worded(&self) -> String {
        format!("line {}: {}: {}", self.line(), self.code(), self.message())
    }
}

/// The one problem of a root with these attributes, written on one line.
fn problem(attributes: &str) -> Problem {
    only(format!("<composition {attributes}/>").as_bytes())
}

/// The one problem of a file: it is read, it is not valid, and there is exactly one report.
fn only(source: &[u8]) -> Problem {
    match read_root(source) {
        Ok(root) => panic!("{} was read: {root:?}", String::from_utf8_lossy(source)),
        Err(mut problems) => {
            assert_eq!(problems.len(), 1, "{problems:?}");
            problems.remove(0)
        }
    }
}

const GOOD: &str = r#"width="640" height="360" duration="2""#;

#[test]
fn a_root_with_no_version_is_reported_with_the_versions_read() {
    let problem = problem(GOOD);
    assert_eq!(problem.line(), 1);
    assert_eq!(
        problem.worded(),
        "line 1: FS-0004: <composition> needs the attribute `version`; the versions read are: 1"
    );
}

#[test]
fn a_version_other_than_1_is_reported_with_the_versions_read() {
    for version in ["2", "0", "1.5", "-1", "one", "", "11", "1e1"] {
        let problem = problem(&format!(r#"version="{version}" {GOOD}"#));
        assert_eq!(
            problem.worded(),
            format!("line 1: FS-0005: `version=\"{version}\"` is not a version this program reads; the versions read are: 1"),
            "{version}"
        );
    }
}

#[test]
fn a_size_that_is_odd_small_large_or_not_whole_is_reported_with_the_rule() {
    for (name, other) in [("width", "height"), ("height", "width")] {
        for value in [
            "641",
            "1",
            "0",
            "-2",
            "8193",
            "8194",
            "10.5",
            "abc",
            "",
            "1e9",
            "99999999999999999999999",
            "1e999999999999",
            "12px",
            "50%",
        ] {
            let text = format!("version=\"1\" {name}=\"{value}\" {other}=\"360\" duration=\"2\"");
            assert_eq!(
                problem(&text).worded(),
                format!("line 1: FS-0007: `{name}` must be an even whole number from 2 to 8192, not {value}"),
                "{name}={value}"
            );
        }
    }
}

#[test]
fn a_missing_width_or_height_is_reported_at_the_root() {
    assert_eq!(
        problem(r#"version="1" height="360" duration="2""#).worded(),
        "line 1: FS-0006: <composition> needs the attribute `width`, an even whole number from 2 to 8192"
    );
    assert_eq!(
        problem(r#"version="1" width="640" duration="2""#).worded(),
        "line 1: FS-0006: <composition> needs the attribute `height`, an even whole number from 2 to 8192"
    );
}

#[test]
fn an_fps_that_is_not_a_whole_number_from_1_to_240_is_reported_with_the_form() {
    for value in ["0", "241", "-30", "30.5", "abc", "", "30000/1001", "1e3", "29.97"] {
        let problem = problem(&format!(r#"version="1" width="640" height="360" fps="{value}" duration="2""#));
        assert_eq!(
            problem.worded(),
            format!("line 1: FS-0008: `fps` must be a whole number from 1 to 240, not {value}"),
            "{value}"
        );
    }
}

#[test]
fn a_duration_of_zero_is_reported_as_not_above_zero() {
    for value in ["0", "0.0", "0.000000000", "00"] {
        let problem = problem(&format!(r#"version="1" width="640" height="360" duration="{value}""#));
        assert_eq!(
            problem.worded(),
            format!("line 1: FS-0011: `duration=\"{value}\"` must be above 0"),
            "{value}"
        );
    }
}

#[test]
fn a_duration_that_is_negative_or_not_a_number_is_reported_with_the_form() {
    for value in [
        "-5",
        "-0.5",
        "-0",
        "abc",
        "",
        "5s",
        "1e3",
        "5.",
        "+5",
        "1,5",
        "1234567890",
        "0.1234567891",
        "inf",
        "nan",
    ] {
        let problem = problem(&format!(r#"version="1" width="640" height="360" duration="{value}""#));
        assert_eq!(
            problem.worded(),
            format!("line 1: FS-0010: `duration=\"{value}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25"),
            "{value}"
        );
    }
}

#[test]
fn a_missing_duration_is_reported_at_the_root() {
    assert_eq!(
        problem(r#"version="1" width="640" height="360""#).worded(),
        "line 1: FS-0009: <composition> needs a `duration` above 0"
    );
}

#[test]
fn a_background_that_is_not_one_colour_is_reported_with_the_form() {
    for value in [
        "red",
        "#fff",
        "#12345",
        "#1234567",
        "#gggggg",
        "101820",
        "rgb(0,0,0)",
        "",
        "#101820, #ffffff",
        "linear-gradient(#000000 0%, #ffffff 100%)",
        "# 101820",
    ] {
        let problem = problem(&format!(
            r#"version="1" width="640" height="360" duration="2" background="{value}""#
        ));
        assert_eq!(
            problem.worded(),
            format!("line 1: FS-0012: `background=\"{value}\"` is not a colour written as #rrggbb or #rrggbbaa"),
            "{value}"
        );
    }
}

#[test]
fn a_problem_is_on_the_line_of_its_attribute() {
    let text = "<?xml version=\"1.0\"?>\n<!-- a\n comment -->\n<composition\n  version=\"1\"\n  width=\"640\"\n  height=\"361\"\n  duration=\"2\"\n  background=\"red\">\n</composition>\n";
    let problems = read_root(text.as_bytes()).unwrap_err();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert_eq!(problems[0].line(), 7);
    assert_eq!(
        problems[0].worded(),
        "line 7: FS-0007: `height` must be an even whole number from 2 to 8192, not 361"
    );
    assert_eq!(problems[1].line(), 9);
    assert!(problems[1].message().starts_with("`background=\"red\"`"));

    let text = text.replace("height=\"361\"", "height=\"360\"");
    let problem = only(text.as_bytes());
    assert_eq!(problem.line(), 9);
}

#[test]
fn a_missing_attribute_is_reported_on_the_line_the_root_opens() {
    let text = "<!-- first -->\n\n<composition\n  version=\"1\"\n  height=\"360\"\n  duration=\"2\"/>";
    let problem = only(text.as_bytes());
    assert_eq!(problem.line(), 3);
    assert_eq!(
        problem.message(),
        "<composition> needs the attribute `width`, an even whole number from 2 to 8192"
    );
}

#[test]
fn a_root_that_is_not_a_composition_is_reported() {
    let problem = only(b"\n<html>\n</html>");
    assert_eq!(problem.worded(), "line 2: FS-0003: the root element is <html>, not <composition>");
}

#[test]
fn a_file_that_is_not_xml_is_reported_with_the_readers_reason_and_line() {
    let problem = only(b"<composition version=\"1\">\n<box>\n</composition>");
    assert_eq!(problem.line(), 3);
    assert!(problem.message().starts_with("the file is not well-formed XML: "), "{problem}");

    for text in ["", "not xml at all", "<composition", "<composition version=1/>", "<a/><b/>"] {
        let problem = only(text.as_bytes());
        assert!(
            problem.message().starts_with("the file is not well-formed XML: "),
            "{text:?}: {problem}"
        );
    }
}

#[test]
fn a_file_that_is_not_utf_8_is_reported_on_the_line_of_the_first_bad_byte() {
    let mut text = b"<composition version=\"1\"\n width=\"640\" id=\"".to_vec();
    text.push(0xff);
    text.extend_from_slice(b"\"/>");
    let problem = only(&text);
    assert_eq!(
        problem.worded(),
        "line 2: FS-0001: the file is not well-formed XML: it is not valid UTF-8 text"
    );
}

#[test]
fn a_message_is_one_line_of_plain_text_whatever_the_value_holds() {
    // A character reference makes a real line feed, an escape and a very long value.
    let long = "9".repeat(5000);
    let values = ["a&#10;b", "&#27;[31mred&#27;[0m", "tab&#9;tab", "&#x85;x", long.as_str()];
    for attribute in ["version", "width", "fps", "duration", "background"] {
        for value in values {
            let others = ["version=\"1\"", "width=\"640\"", "height=\"360\"", "duration=\"2\""];
            let mut attributes: Vec<String> = others
                .iter()
                .filter(|other| !other.starts_with(attribute))
                .map(|s| s.to_string())
                .collect();
            attributes.push(format!("{attribute}=\"{value}\""));
            let text = format!("<composition {}/>", attributes.join(" "));
            let message = only(text.as_bytes()).worded();
            assert!(
                !message.contains(['\n', '\r', '\u{1b}', '\t', '\u{85}']),
                "{attribute}={value:?}: {message:?}"
            );
            assert!(message.chars().count() < 200, "{attribute}: {} characters", message.chars().count());
        }
    }
    let message = only(b"<composition version=\"a&#10;b\" width=\"2\" height=\"2\" duration=\"1\"/>").worded();
    assert_eq!(
        message,
        "line 1: FS-0005: `version=\"a\\u{a}b\"` is not a version this program reads; the versions read are: 1"
    );
}

#[test]
fn a_long_value_is_cut_after_80_characters() {
    let value = "x".repeat(81);
    let message = only(format!(r#"<composition version="1" width="{value}" height="2" duration="1"/>"#).as_bytes()).worded();
    assert_eq!(
        message,
        format!(
            "line 1: FS-0007: `width` must be an even whole number from 2 to 8192, not {}...",
            "x".repeat(80)
        )
    );
    let value = "x".repeat(80);
    let message = only(format!(r#"<composition version="1" width="{value}" height="2" duration="1"/>"#).as_bytes()).worded();
    assert!(message.ends_with(&format!("not {value}")), "{message}");
}

#[test]
fn no_message_carries_colour_or_a_control_character() {
    let attributes = [
        "",
        r#"version="2""#,
        r#"version="1" width="3" height="4" duration="1""#,
        r#"version="1" width="4" height="4" fps="0" duration="1""#,
        r#"version="1" width="4" height="4" duration="0""#,
        r#"version="1" width="4" height="4" duration="x""#,
        r#"version="1" width="4" height="4" duration="1" background="red""#,
    ];
    for attributes in attributes {
        let text = format!("<composition {attributes}/>");
        for problem in read_root(text.as_bytes()).unwrap_err() {
            let message = problem.worded();
            assert!(message.chars().all(|c| !c.is_control()), "{message:?}");
        }
    }
}
