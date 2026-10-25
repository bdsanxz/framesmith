//! A `<tween>` that cannot move anything: no duration, no property, a property that is never animated, and a property
//! the element it moves does not take. Each is reported with its own code on the line that shows it
//! (`docs/spec/check-command.md`, "A tween"; `docs/spec/error-catalogue.md`).

use notation::{read_root, Problem};

fn problems(body: &str) -> Vec<Problem> {
    let text = format!("<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<box>\n{body}\n</box>\n</composition>");
    match read_root(text.as_bytes()) {
        Ok(root) => panic!("{body} was read: {root:?}"),
        Err(problems) => problems,
    }
}

fn places(problems: &[Problem]) -> Vec<(&'static str, u32)> {
    problems.iter().map(|problem| (problem.code().as_str(), problem.line())).collect()
}

#[test]
fn a_tween_with_no_duration_or_a_duration_of_zero_is_reported_on_its_line() {
    assert_eq!(places(&problems("<tween to-x=\"5\"/>")), [("FS-0058", 3)]);
    assert_eq!(places(&problems("<tween\n duration=\"0\"\n to-x=\"5\"/>")), [("FS-0058", 3)]);
}

#[test]
fn a_tween_with_no_property_is_reported_on_its_line() {
    assert_eq!(places(&problems("<tween at=\"0\" duration=\"1\"/>")), [("FS-0059", 3)]);
}

#[test]
fn a_property_that_is_not_animated_is_reported_on_its_attribute_line() {
    assert_eq!(places(&problems("<tween duration=\"1\"\n to-width=\"5\"/>")), [("FS-0030", 4)]);
}

#[test]
fn a_property_the_element_does_not_take_is_reported_on_its_attribute_line() {
    assert_eq!(places(&problems("<tween duration=\"1\"\n to-count=\"5\"/>")), [("FS-0060", 4)]);
}
