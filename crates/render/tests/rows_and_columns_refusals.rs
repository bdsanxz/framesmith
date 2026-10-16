//! A gap, a padding, a direction or an alignment that the spec does not allow, or does not draw in this
//! version, is reported with its line, its code and the values it takes, both by `check` and by `render`
//! (`docs/spec/render-command.md` sections 3 and 4.3, rules 4, 8 and 10).

mod common;

use common::{composition, project_with};
use project::{OpenError, Project, Report};
use render::{place_reports, Scene};

fn shown(report: &Report) -> String {
    format!(
        "{} {} {} {}",
        report.file(),
        report.problem().line(),
        report.problem().code(),
        report.problem().message()
    )
}

/// The reports `render` gives for a composition with a box carrying `attribute` on line 3: those of
/// opening the project, when the notation refuses the value, and otherwise those of the scene.
fn reports_of(name: &str, attribute: &str) -> Vec<String> {
    let inside = format!("<box/>\n<box {attribute} background=\"#e0a030\"/>");
    let folder = project_with("rows-and-columns-refusals", name, &composition(&inside));
    match Project::open(&folder) {
        Err(OpenError::Invalid(reports)) => reports.iter().map(shown).collect(),
        Err(other) => panic!("{other}"),
        Ok(project) => match Scene::from_project(&project) {
            Ok(_) => Vec::new(),
            Err(reports) => reports.iter().map(shown).collect(),
        },
    }
}

#[test]
fn a_gap_below_zero_is_reported_with_the_rule_it_breaks() {
    assert_eq!(
        reports_of("gap-negative", "flex-direction=\"row\" gap=\"-2\""),
        vec!["composition.xml 3 FS-0034 `gap=\"-2\"` must be 0 or more"]
    );
}

#[test]
fn a_gap_that_is_not_a_plain_number_is_refused_as_a_number_when_the_project_is_opened() {
    // A gap is a plain number (`notation.md` N9), so a unit is refused at opening, as FS-0032.
    assert_eq!(
        reports_of("gap-with-unit", "flex-direction=\"row\" gap=\"8px\""),
        vec!["composition.xml 3 FS-0032 `gap=\"8px\"` is not a number: an optional sign, digits with an optional point and an optional exponent, such as 12, -4, .5 or 2.5e-1"]
    );
}

#[test]
fn a_padding_below_zero_is_reported_with_the_rule_it_breaks() {
    assert_eq!(
        reports_of("padding-negative", "flex-direction=\"row\" padding=\"-1\""),
        vec!["composition.xml 3 FS-0034 `padding=\"-1\"` must be 0 or more"]
    );
}

#[test]
fn a_padding_that_is_not_one_to_four_numbers_is_reported_with_the_form_it_takes() {
    assert_eq!(
        reports_of("padding-percent", "flex-direction=\"row\" padding=\"10%\""),
        vec!["composition.xml 3 FS-0032 `padding=\"10%\"` is not one to four numbers, such as 4 or 4 8"]
    );
}

#[test]
fn a_direction_other_than_row_or_column_is_reported_with_the_words_it_takes() {
    assert_eq!(
        reports_of("direction-diagonal", "flex-direction=\"diagonal\""),
        vec!["composition.xml 3 FS-0039 `flex-direction=\"diagonal\"` is not one of: row, column"]
    );
}

#[test]
fn a_main_axis_alignment_other_than_its_words_is_reported_with_the_words_it_takes() {
    assert_eq!(
        reports_of("justify-capital", "flex-direction=\"row\" justify-content=\"Center\""),
        vec!["composition.xml 3 FS-0039 `justify-content=\"Center\"` is not one of: flex-start, center, flex-end, space-between"]
    );
}

#[test]
fn a_cross_axis_alignment_other_than_its_words_is_reported_with_the_words_it_takes() {
    assert_eq!(
        reports_of("align-middle", "flex-direction=\"row\" align-items=\"middle\""),
        vec!["composition.xml 3 FS-0039 `align-items=\"middle\"` is not one of: flex-start, center, flex-end, stretch, baseline"]
    );
    assert_eq!(
        reports_of("self-middle", "flex-direction=\"row\" align-self=\"middle\""),
        vec!["composition.xml 3 FS-0039 `align-self=\"middle\"` is not one of: flex-start, center, flex-end, stretch, baseline"]
    );
}

#[test]
fn space_between_and_baseline_are_reported_as_not_drawn_in_this_version() {
    let between = reports_of("justify-space-between", "flex-direction=\"row\" justify-content=\"space-between\"");
    assert_eq!(between.len(), 1, "{between:?}");
    assert!(between[0].starts_with("composition.xml 3 FS-0035 "), "{between:?}");
    let baseline = reports_of("align-baseline", "flex-direction=\"row\" align-items=\"baseline\"");
    assert_eq!(baseline.len(), 1, "{baseline:?}");
    assert!(baseline[0].starts_with("composition.xml 3 FS-0035 "), "{baseline:?}");
}

#[test]
fn a_gap_below_zero_is_reported_by_check_as_well_as_by_render() {
    let inside = "<box/>\n<box flex-direction=\"row\" gap=\"-2\" background=\"#e0a030\"/>";
    let folder = project_with("rows-and-columns-refusals", "gap-check", &composition(inside));
    let project = Project::open(&folder).unwrap();
    let checked: Vec<String> = place_reports(&project).iter().map(shown).collect();
    assert_eq!(
        checked,
        vec!["composition.xml 3 FS-0034 `gap=\"-2\"` must be 0 or more".to_string()]
    );
}
