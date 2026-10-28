//! A place or a size written in a form the spec does not list is reported with its line, with the
//! attribute and its value, and with the forms it takes (`docs/spec/render-command.md` section 3).

mod common;

use common::{composition, project_with};
use project::{Project, Report};
use render::Scene;

fn shown(report: &Report) -> String {
    format!(
        "{} {} {} {}",
        report.file(),
        report.problem().line(),
        report.problem().code(),
        report.problem().message()
    )
}

/// The reports of a composition with `inside` in its root, on the lines after the first box.
fn reports_of(name: &str, attribute: &str) -> Vec<String> {
    let inside = format!("<box/>\n<box {attribute} background=\"#e0a030\"/>");
    let folder = project_with("forms-not-listed", name, &composition(&inside));
    let project = Project::open(&folder).unwrap();
    match Scene::from_project(&project, &render::system_codecs()) {
        Ok(_) => Vec::new(),
        Err(reports) => reports.iter().map(shown).collect(),
    }
}

#[test]
fn a_length_with_a_unit_is_reported_with_the_forms_it_takes() {
    assert_eq!(
        reports_of("left-with-pixels", "left=\"10px\""),
        vec!["composition.xml 3 FS-0034 `left=\"10px\"` is not a length: a number of pixels or a percentage, such as 40 or 50%"]
    );
    assert_eq!(
        reports_of("width-with-em", "width=\"5em\""),
        vec!["composition.xml 3 FS-0034 `width=\"5em\"` is not a length: a number of pixels or a percentage, such as 40 or 50%"]
    );
}

#[test]
fn a_position_other_than_its_two_words_is_reported_with_the_words_it_takes() {
    assert_eq!(
        reports_of("position-fixed", "position=\"fixed\""),
        vec!["composition.xml 3 FS-0039 `position=\"fixed\"` is not one of: static, absolute"]
    );
    assert_eq!(
        reports_of("position-capital", "position=\"Absolute\""),
        vec!["composition.xml 3 FS-0039 `position=\"Absolute\"` is not one of: static, absolute"]
    );
}

#[test]
fn a_margin_that_is_not_one_to_four_numbers_is_reported_with_the_form_it_takes() {
    for (name, margin) in [
        ("margin-percent", "10%"),
        ("margin-five-numbers", "1 2 3 4 5"),
        ("margin-word", "a"),
    ] {
        assert_eq!(
            reports_of(name, &format!("margin=\"{margin}\"")),
            vec![format!(
                "composition.xml 3 FS-0032 `margin=\"{margin}\"` is not one to four numbers, such as 4 or 4 8"
            )],
            "{margin}"
        );
    }
}
