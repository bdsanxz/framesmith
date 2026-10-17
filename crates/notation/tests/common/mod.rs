//! What the tests of the form of a composition file share: a valid file around a snippet, and the
//! problems of a file in a form a test can compare.

#![allow(dead_code)]

use notation::{read_root, Element, Problem};

/// A valid root with `body` inside it, `body` starting on line 2.
pub fn document(body: &str) -> String {
    format!("<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n{body}\n</composition>\n")
}

/// Every problem of a file, or none.
pub fn problems_of(text: &str) -> Vec<Problem> {
    match read_root(text.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(problems) => problems,
    }
}

/// Every problem of a valid root with `body` in it.
pub fn problems_in(body: &str) -> Vec<Problem> {
    problems_of(&document(body))
}

/// The one problem of a valid root with `body` in it.
pub fn only_problem_in(body: &str) -> Problem {
    let mut problems = problems_in(body);
    assert_eq!(problems.len(), 1, "{body}: {problems:#?}");
    problems.remove(0)
}

/// `FS-0030 line 2: <message>`, the form these tests compare.
pub fn worded(problem: &Problem) -> String {
    format!("line {}: {}: {}", problem.line(), problem.code(), problem.message())
}

/// A document in which `inner` stands inside one element of the kind `parent`, the parent itself
/// standing where it may, with the inner text on line 3 or later and starting a line of its own.
pub fn inside(parent: Element, inner: &str) -> String {
    let body = match parent {
        Element::Composition => return document(inner),
        Element::Nested => format!("<composition src=\"scenes/other.xml\">\n{inner}\n</composition>"),
        Element::Style => format!("<style id=\"label\">\n{inner}\n</style>"),
        Element::Font => format!("<font src=\"fonts/Display.ttf\">\n{inner}\n</font>"),
        Element::Audio => format!("<audio src=\"audio/bed.wav\" duration=\"2\">\n{inner}\n</audio>"),
        Element::Volume => format!("<audio src=\"audio/bed.wav\" duration=\"2\">\n<volume value=\"1\">\n{inner}\n</volume>\n</audio>"),
        Element::Box => format!("<box>\n{inner}\n</box>"),
        Element::Rect => format!("<rect>\n{inner}\n</rect>"),
        Element::Text => format!("<text>\n{inner}\n</text>"),
        Element::Span => format!("<text>\n<span>\n{inner}\n</span>\n</text>"),
        Element::Image => format!("<image src=\"images/logo.png\">\n{inner}\n</image>"),
        Element::Video => format!("<video src=\"clips/intro.mp4\" width=\"640\" height=\"360\">\n{inner}\n</video>"),
        Element::Shape => format!("<shape d=\"M0 0\">\n{inner}\n</shape>"),
        Element::Tween => format!("<box>\n<tween duration=\"1\" to-x=\"1\">\n{inner}\n</tween>\n</box>"),
        Element::Set => format!("<box>\n<set x=\"1\">\n{inner}\n</set>\n</box>"),
    };
    document(&body)
}

/// One element of the kind `element` written on a line of its own, with no attribute but those a
/// valid element of the kind needs here.
pub fn written(element: Element) -> String {
    match element {
        Element::Composition => "<composition/>".to_string(),
        Element::Nested => "<composition src=\"scenes/more.xml\"/>".to_string(),
        Element::Style => "<style id=\"label\"/>".to_string(),
        other => format!("<{}/>", other.name()),
    }
}

/// The line (counted from 1) of the first line of `text` that is exactly `line`.
pub fn line_of(text: &str, line: &str) -> u32 {
    let at = text
        .lines()
        .position(|candidate| candidate == line)
        .unwrap_or_else(|| panic!("no line {line:?} in {text}"));
    at as u32 + 1
}
