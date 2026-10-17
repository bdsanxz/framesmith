//! `framesmith check` on the shared styles that are wrong: each report names the file, the line, the
//! code, the kind, the message and the remedy, and the command exits with 1 (`docs/spec/check-command.md`
//! section 3c and section 4).

mod common;

use common::{framesmith, project};
use project::Code;

/// One report as the command prints it: the place, the code, its kind, the message and the remedy
/// (`docs/spec/error-catalogue.md` section 3).
fn report(place: &str, code: Code, message: &str) -> String {
    format!("{place}: {code}: {}: {message}. {}", code.kind().as_str(), code.remedy())
}

const INVALID: &str = "Invalid: the project is not valid (composition.xml)";

/// Runs `framesmith check` on a composition file `text` and returns the exit code and the lines printed.
fn check_text(group: &str, name: &str, text: &str) -> (Option<i32>, Vec<String>) {
    let (project_folder, cwd) = project(group, name, text);
    let output = framesmith(&cwd, ["check".as_ref(), project_folder.as_os_str()], &[]);
    let printed: Vec<String> = String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert!(output.stderr.is_empty(), "{:?}", String::from_utf8_lossy(&output.stderr));
    (output.status.code(), printed)
}

#[test]
fn a_style_attribute_the_element_does_not_take_is_reported_on_the_element_line_and_exits_1() {
    let text = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                <composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n\
                <style id=\"shade\" object-fit=\"cover\"/>\n\
                <text class=\"shade\">HELLO</text>\n\
                </composition>\n";
    let (code, printed) = check_text("shared-styles-check", "object-fit-on-text", text);
    assert_eq!(code, Some(1));
    assert_eq!(
        printed,
        [
            report(
                "composition.xml, line 4",
                Code::AttributeNotTaken,
                "<text> has no attribute `object-fit`, which the style `shade` gives it"
            ),
            INVALID.to_string(),
        ]
    );
}

#[test]
fn each_style_report_is_printed_with_its_line_in_the_order_of_the_file_and_exits_1() {
    let text = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                <composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n\
                <style background=\"#ffffff\"/>\n\
                <style id=\"label\" color=\"#000000\"/>\n\
                <style id=\"label\" color=\"#ffffff\"/>\n\
                <box class=\"label missing\"/>\n\
                <style id=\"frame\">\n\
                <box/>\n\
                </style>\n\
                </composition>\n";
    let (code, printed) = check_text("shared-styles-check", "every-style-report", text);
    assert_eq!(code, Some(1));
    assert_eq!(
        printed,
        [
            report(
                "composition.xml, line 3",
                Code::StyleWithoutId,
                "<style> needs the attribute `id`: the name elements take it by"
            ),
            report("composition.xml, line 5", Code::StyleIdTwice, "two <style> elements have the id `label`"),
            report(
                "composition.xml, line 6",
                Code::ClassNamesNoStyle,
                "`class` names the style `missing`, and this file has no <style> with that id"
            ),
            report(
                "composition.xml, line 8",
                Code::ElementNotHere,
                "the style `frame` holds <box>, and a style holds attributes only; a <box> stands directly in <composition>, <box> or <rect>"
            ),
            INVALID.to_string(),
        ]
    );
}

#[test]
fn a_style_that_names_another_style_is_reported_on_its_class_line_and_exits_1() {
    let text = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                <composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n\
                <style id=\"other\" color=\"#ffffff\"/>\n\
                <style id=\"chain\" class=\"other\" color=\"#000000\"/>\n\
                </composition>\n";
    let (code, printed) = check_text("shared-styles-check", "style-with-class", text);
    assert_eq!(code, Some(1));
    assert_eq!(
        printed,
        [
            report(
                "composition.xml, line 4",
                Code::AttributeNotTaken,
                "the style `chain` has `class`; a style cannot take another style"
            ),
            INVALID.to_string(),
        ]
    );
}
