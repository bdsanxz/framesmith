//! `framesmith check` on a composition that writes a form the notation does not list: an element,
//! an attribute, a place, a number or a time. Each is reported with the file and the line, the
//! command ends with 1, and the same composition without the form is ready
//! (`docs/spec/check-command.md` section 3a).

mod common;

use std::ffi::OsStr;
use std::path::Path;
use std::process::Output;

use common::*;
use project::Code;

const GROUP: &str = "closed-notation-check";

/// A composition whose root is valid and whose body is `body`, starting on line 2.
fn composition(body: &str) -> String {
    format!("<composition version=\"1\" id=\"demo\" width=\"640\" height=\"360\" duration=\"2\">\n{body}\n</composition>\n")
}

fn check(group_name: &str, text: &str) -> (Output, Vec<String>) {
    let (project, cwd) = project(GROUP, group_name, text);
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[]);
    let printed = out(&output).lines().map(str::to_string).collect();
    (output, printed)
}

fn report(place: &str, code: Code, message: &str) -> String {
    format!("{place}: {code}: {}: {message}. {}", code.kind().as_str(), code.remedy())
}

const INVALID: &str = "Invalid: the project is not valid (composition.xml)";

#[test]
fn an_element_the_spec_does_not_list_is_reported_with_its_file_and_line_and_the_exit_code_is_1() {
    let (output, printed) = check("unknown-element", &composition("<box/>\n<circle radius=\"5\"/>"));
    assert_eq!(output.status.code(), Some(1));
    assert!(err(&output).is_empty());
    assert_eq!(
        printed,
        [
            report(
                "composition.xml, line 3",
                Code::ElementNotKnown,
                "<circle> is not a known element; inside <composition> the elements taken are <style>, <font>, <audio>, <box>, <rect>, <text>, <image>, <video>, <shape> and <composition src>"
            ),
            INVALID.to_string()
        ]
    );
}

#[test]
fn an_attribute_its_element_does_not_take_is_reported_with_its_line_the_attribute_and_the_element() {
    let (output, printed) = check(
        "attribute",
        &composition("<box>\n<tween duration=\"1\"\n  to-width=\"10\"/>\n</box>\n<rect colour=\"red\"/>"),
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        printed,
        [
            report(
                "composition.xml, line 4",
                Code::AttributeNotTaken,
                "<tween> has no attribute `to-width`"
            ),
            report(
                "composition.xml, line 6",
                Code::AttributeNotTaken,
                "<rect> has no attribute `colour`"
            ),
            INVALID.to_string()
        ]
    );
}

#[test]
fn an_element_in_a_place_it_may_not_stand_is_reported_with_its_line_and_the_place_where_it_may_stand() {
    let (output, printed) = check(
        "place",
        &composition("<box>\n<style id=\"label\"/>\n</box>\n<text>\n<box/>\n</text>"),
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        printed,
        [
            report(
                "composition.xml, line 3",
                Code::ElementNotHere,
                "<style> is not known inside <box>; a <style> stands only directly in the <composition>, never inside an element"
            ),
            report(
                "composition.xml, line 6",
                Code::ElementNotHere,
                "<box> is not known inside <text>; only <span>, <tween> and <set> are; a <box> stands directly in <composition>, <box> or <rect>"
            ),
            INVALID.to_string()
        ]
    );
}

#[test]
fn a_time_without_a_leading_digit_is_read_and_one_with_a_bare_point_is_reported() {
    let (output, printed) = check("time-read", &composition("<box start=\".5\" duration=\".25\"/>"));
    assert_eq!(output.status.code(), Some(0), "{printed:?}");
    assert_eq!(printed, ["Ready: the project is valid, 60 frames, 640x360, 30 fps"]);

    let (output, printed) = check("time-reported", &composition("<box start=\"5.\" duration=\"1\"/>"));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        printed,
        [
            report(
                "composition.xml, line 2",
                Code::NotATime,
                "`start=\"5.\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25"
            ),
            INVALID.to_string()
        ]
    );
}

#[test]
fn a_number_with_an_exponent_or_a_sign_is_read_and_a_non_finite_one_is_reported() {
    let (output, printed) = check(
        "number-read",
        &composition("<box x=\"-4\" y=\"+0.5\" scale=\"1e3\" opacity=\"2.5E-1\" blur=\".5\"/>"),
    );
    assert_eq!(output.status.code(), Some(0), "{printed:?}");
    assert_eq!(printed.len(), 1);

    let (output, printed) = check("number-reported", &composition("<box x=\"inf\" y=\"12px\" scale=\"1e999\"/>"));
    assert_eq!(output.status.code(), Some(1));
    let codes: Vec<&str> = printed
        .iter()
        .filter_map(|line| line.split(": ").nth(1))
        .filter(|word| word.starts_with("FS-"))
        .collect();
    assert_eq!(codes, ["FS-0032", "FS-0032", "FS-0032"]);
    assert!(printed[0].contains("`x=\"inf\"` is not a number"), "{}", printed[0]);
    assert!(printed[2].contains("`scale=\"1e999\"` is too large"), "{}", printed[2]);
}

#[test]
fn two_elements_with_one_id_are_accepted_and_nothing_is_reported() {
    let text = composition("<box id=\"panel\"/>\n<box id=\"panel\"/>\n<audio id=\"panel\" src=\"a.wav\" duration=\"1\"/>");
    let (project, cwd) = project(GROUP, "same-id", &text);
    // The sound the `<audio>` names: a tenth of a second of silence.
    testkit::sound_files::write_wav(&project.join("a.wav"), 48_000, 2, testkit::sound_files::Pcm::Int(16), &[0.0; 9_600]).unwrap();
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[]);
    let printed: Vec<String> = out(&output).lines().map(str::to_string).collect();
    assert_eq!(output.status.code(), Some(0), "{printed:?}");
    assert_eq!(printed, ["Ready: the project is valid, 60 frames, 640x360, 30 fps"]);
}

#[test]
fn a_composition_that_differs_from_a_valid_one_only_by_an_unknown_attribute_is_not_ready() {
    let (output, printed) = check(
        "valid-twin",
        &composition("<box left=\"10\" top=\"10\" width=\"100\" height=\"50\"/>"),
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(printed[0].starts_with("Ready: "), "{printed:?}");

    let (output, printed) = check(
        "unknown-attribute-twin",
        &composition("<box left=\"10\" top=\"10\" width=\"100\" height=\"50\" glow=\"1\"/>"),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(printed.iter().all(|line| !line.starts_with("Ready")), "{printed:?}");
    assert_eq!(
        printed,
        [
            report("composition.xml, line 2", Code::AttributeNotTaken, "<box> has no attribute `glow`"),
            INVALID.to_string()
        ]
    );
}

#[test]
fn the_reports_of_the_tree_and_of_the_root_come_in_the_order_of_the_file_and_the_file_is_not_changed() {
    let text = "<composition version=\"2\" width=\"641\" height=\"360\" duration=\"2\">\n<circle/>\n<box colour=\"red\" x=\"a\"/>\n</composition>\n";
    let (project, cwd) = project(GROUP, "order", text);
    let before = std::fs::read(Path::new(&project).join("composition.xml")).unwrap();
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[]);
    assert_eq!(output.status.code(), Some(1));
    let codes: Vec<String> = out(&output)
        .lines()
        .filter_map(|line| line.split(": ").nth(1).map(str::to_string))
        .filter(|word| word.starts_with("FS-"))
        .collect();
    assert_eq!(codes, ["FS-0005", "FS-0007", "FS-0029", "FS-0030", "FS-0032"]);
    assert_eq!(std::fs::read(Path::new(&project).join("composition.xml")).unwrap(), before);
}

#[test]
fn the_log_of_a_run_holds_the_codes_and_lines_of_these_reports_and_none_of_their_words() {
    let text = composition("<needle-element/>\n<box needle-attribute=\"1\" x=\"needle-number\" start=\"needle-time\"/>");
    let (project, cwd) = project(GROUP, "log", &text);
    let mut printed = String::new();
    for level in ["trace", "debug", "info"] {
        let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("FRAMESMITH_LOG", level)]);
        printed.push_str(&out(&output));
    }
    // The control: the command shows the words, so they are in play.
    for needle in ["needle-element", "needle-attribute", "needle-number", "needle-time"] {
        assert!(printed.contains(needle), "{needle}: {printed}");
    }
    let log = after_run_id(&log_text(&cwd));
    for needle in ["needle-element", "needle-attribute", "needle-number", "needle-time"] {
        assert!(!log.contains(needle), "{needle} is in the log:\n{log}");
    }
    for expected in [
        r#"code="FS-0029" line=2"#,
        r#"code="FS-0030" line=3"#,
        r#"code="FS-0032" line=3"#,
        r#"code="FS-0033" line=3"#,
    ] {
        assert!(log.contains(expected), "{expected} is not in the log:\n{log}");
    }
}
