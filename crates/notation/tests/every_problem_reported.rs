//! A root with several problems: all are returned, each with its own code and line, in the order
//! of the file; and the files that can be read no further give one problem.

use notation::{read_root, Code, Problem};
use std::collections::HashSet;

fn all_problems(text: &str) -> Vec<Problem> {
    match read_root(text.as_bytes()) {
        Ok(root) => panic!("{text} was read: {root:?}"),
        Err(problems) => problems,
    }
}

/// The codes and the lines of the problems, as `(FS-0007, 3)` pairs.
fn places(problems: &[Problem]) -> Vec<(&'static str, u32)> {
    problems.iter().map(|problem| (problem.code().as_str(), problem.line())).collect()
}

#[test]
fn three_wrong_values_are_three_problems_each_on_its_own_line() {
    let text = "<composition\n  version=\"1\"\n  width=\"641\"\n  height=\"360\"\n  fps=\"0\"\n  duration=\"2\"\n  background=\"red\"/>";
    let problems = all_problems(text);
    assert_eq!(places(&problems), [("FS-0007", 3), ("FS-0008", 5), ("FS-0012", 7)]);
    assert_eq!(
        problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
        [
            "line 3: FS-0007: `width` must be an even whole number from 2 to 8192, not 641",
            "line 5: FS-0008: `fps` must be a whole number from 1 to 240, not 0",
            "line 7: FS-0012: `background=\"red\"` is not a colour written as #rrggbb or #rrggbbaa",
        ]
    );
}

#[test]
fn every_attribute_of_the_root_can_be_wrong_at_once() {
    let text = "<composition version=\"2\" width=\"1\" height=\"1\" fps=\"0\" duration=\"0\" background=\"red\"/>";
    assert_eq!(
        places(&all_problems(text)),
        [
            ("FS-0005", 1),
            ("FS-0007", 1),
            ("FS-0007", 1),
            ("FS-0008", 1),
            ("FS-0011", 1),
            ("FS-0012", 1)
        ]
    );
}

#[test]
fn every_attribute_that_is_missing_is_reported_at_the_roots_opening() {
    let text = "<!-- first -->\n\n<composition\n  fps=\"30\"/>";
    assert_eq!(
        places(&all_problems(text)),
        [("FS-0004", 3), ("FS-0006", 3), ("FS-0006", 3), ("FS-0009", 3)]
    );
    let messages: Vec<String> = all_problems(text).iter().map(|problem| problem.message().to_string()).collect();
    assert!(messages[1].contains("`width`") && messages[2].contains("`height`"), "{messages:?}");
}

#[test]
fn the_problems_come_in_the_order_of_the_lines_of_the_file_not_of_the_checks() {
    // The reader judges version, width, height, fps, duration and background; the file lists them
    // the other way round.
    let text = "<composition\n background=\"red\"\n duration=\"x\"\n fps=\"500\"\n height=\"3\"\n width=\"5\"\n version=\"0\"/>";
    let problems = all_problems(text);
    assert_eq!(
        places(&problems),
        [
            ("FS-0012", 2),
            ("FS-0010", 3),
            ("FS-0008", 4),
            ("FS-0007", 5),
            ("FS-0007", 6),
            ("FS-0005", 7)
        ]
    );
    let lines: Vec<u32> = problems.iter().map(Problem::line).collect();
    assert!(lines.windows(2).all(|pair| pair[0] <= pair[1]), "{lines:?}");
    assert!(problems[3].message().starts_with("`height`") && problems[4].message().starts_with("`width`"));
}

#[test]
fn problems_on_one_line_come_left_to_right_and_a_missing_attribute_comes_before_them() {
    let text = "<composition width=\"641\" version=\"2\" height=\"1\"/>";
    let problems = all_problems(text);
    assert_eq!(
        places(&problems).iter().map(|(code, _)| *code).collect::<Vec<_>>(),
        ["FS-0009", "FS-0007", "FS-0005", "FS-0007"]
    );
    assert!(problems[1].message().starts_with("`width`"), "{}", problems[1]);
    assert!(problems[3].message().starts_with("`height`"), "{}", problems[3]);
}

#[test]
fn a_file_that_is_not_well_formed_xml_gives_one_problem_and_no_other() {
    // The root's values are wrong too, and so is the nesting; only the reader's report is made.
    let text = "<composition version=\"2\" width=\"1\" height=\"1\" duration=\"0\">\n<box>\n</composition>";
    let problems = all_problems(text);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].code(), Code::NotXml);
    assert_eq!(problems[0].line(), 3);
    assert!(problems[0].message().starts_with("the file is not well-formed XML: "));
    assert!(problems[0].message().len() > "the file is not well-formed XML: ".len());

    for text in ["", "no xml here", "<composition version=\"1\" width=\"1\"", "<a/><b/>"] {
        let problems = all_problems(text);
        assert_eq!(problems.len(), 1, "{text:?}: {problems:?}");
        assert_eq!(problems[0].code(), Code::NotXml, "{text:?}");
    }
}

#[test]
fn a_file_that_is_not_utf_8_gives_one_problem_and_no_other() {
    let mut text = b"<composition version=\"2\"\n width=\"1\" id=\"".to_vec();
    text.push(0xff);
    text.extend_from_slice(b"\"/>");
    let problems = read_root(&text).unwrap_err();
    assert_eq!(places(&problems), [("FS-0001", 2)]);
}

#[test]
fn a_root_that_is_not_a_composition_gives_one_problem_and_its_attributes_are_not_judged() {
    let problems = all_problems("\n<html width=\"1\" version=\"9\">\n</html>");
    assert_eq!(places(&problems), [("FS-0003", 2)]);
}

#[test]
fn a_root_with_nothing_wrong_is_read_and_a_root_corrected_line_by_line_is_read() {
    let text = "<composition\n version=\"1\"\n width=\"641\"\n height=\"361\"\n duration=\"0\"\n background=\"red\"/>";
    let before = all_problems(text);
    assert_eq!(places(&before), [("FS-0007", 3), ("FS-0007", 4), ("FS-0011", 5), ("FS-0012", 6)]);
    let corrected = text
        .replace("641", "640")
        .replace("361", "360")
        .replace("duration=\"0\"", "duration=\"2\"")
        .replace("red", "#ff0000");
    let root = read_root(corrected.as_bytes()).unwrap();
    assert_eq!((root.width, root.height, root.frames), (640, 360, 60));
}

#[test]
fn the_codes_begin_with_fs_are_distinct_and_have_four_digits() {
    let mut seen = HashSet::new();
    for code in Code::ALL {
        let text = code.as_str();
        assert!(text.starts_with("FS-"), "{text}");
        assert!(
            text["FS-".len()..].len() == 4 && text["FS-".len()..].bytes().all(|b| b.is_ascii_digit()),
            "{text}"
        );
        assert!(seen.insert(text), "{text} is given twice");
        assert_eq!(code.to_string(), text);
    }
    assert_eq!(seen.len(), Code::ALL.len());
}
