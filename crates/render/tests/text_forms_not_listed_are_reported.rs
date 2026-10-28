//! A text attribute written in a value the spec does not list for it is reported with its line, the attribute and its
//! value, and the values it takes (`docs/spec/text.md`, section 6.6; `docs/spec/render-command.md`, section 3).

mod common;

use common::project_from_fixture;
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

/// The reports of the composition of the refusal fixture `name`, which holds its text on line 5.
fn reports_of(name: &str) -> Vec<String> {
    let folder = project_from_fixture("text-forms-not-listed", name, &format!("forms-not-listed/{name}"));
    let project = Project::open(&folder).unwrap();
    match Scene::from_project(&project, &render::system_codecs()) {
        Ok(_) => Vec::new(),
        Err(reports) => reports.iter().map(shown).collect(),
    }
}

#[test]
fn a_text_transform_other_than_none_or_uppercase_is_reported_with_the_words_it_takes() {
    assert_eq!(
        reports_of("text-transform-lowercase"),
        vec!["composition.xml 5 FS-0039 `text-transform=\"lowercase\"` is not one of: none, uppercase"]
    );
}

#[test]
fn a_white_space_other_than_normal_or_nowrap_is_reported_with_the_words_it_takes() {
    assert_eq!(
        reports_of("white-space-pre"),
        vec!["composition.xml 5 FS-0039 `white-space=\"pre\"` is not one of: normal, nowrap"]
    );
}

#[test]
fn a_figure_word_the_spec_does_not_list_is_reported_with_the_words_it_takes() {
    let not_taken = |value: &str| {
        format!(
            "composition.xml 5 FS-0039 `font-variant-numeric=\"{value}\"` is not normal or a list of: lining-nums, oldstyle-nums, tabular-nums, proportional-nums"
        )
    };
    assert_eq!(reports_of("font-variant-numeric-slashed-zero"), vec![not_taken("slashed-zero")]);
    assert_eq!(
        reports_of("font-variant-numeric-normal-in-a-list"),
        vec![not_taken("normal tabular-nums")]
    );
}

#[test]
fn a_text_shadow_that_is_not_two_lengths_with_an_optional_blur_and_colour_is_reported() {
    assert_eq!(
        reports_of("text-shadow-two-words"),
        vec!["composition.xml 5 FS-0052 `text-shadow=\"2px 2px red\"` is not a list of shadows: two lengths, an optional blur and an optional colour, such as 2px 2px 4px #000000"]
    );
}

#[test]
fn a_text_shadow_blur_below_zero_is_reported_as_a_length_that_is_not_accepted() {
    assert_eq!(
        reports_of("text-shadow-negative-blur"),
        vec!["composition.xml 5 FS-0034 `text-shadow=\"2px 2px -4px #000000\"` must be 0 or more"]
    );
}

#[test]
fn a_text_stroke_with_an_alpha_below_ff_is_reported_as_a_form_not_drawn() {
    assert_eq!(
        reports_of("text-stroke-alpha"),
        vec!["composition.xml 5 FS-0035 `text-stroke=\"2px #00000080\"` has an alpha below ff, which this version does not draw"]
    );
}
