//! A colour `filter` on a picture (`docs/spec/notation.md`, sections 7.9 and 13): a step that is not one of the
//! four is reported with its line and the four names, and a `filter` on a box, a text or a shape is reported with its
//! line and the elements that take it.
//!
//! Proposal for T060 (spec-first; the implementation follows when the build constraint lifts): the reports are new
//! invalid problems, provisionally `FS-0091` (`Code::FilterNotInForm`) and `FS-0092` (`Code::FilterNotTaken`), to be
//! confirmed against the catalogue on `main` at the rebase.

mod common;

use common::{only_problem_in, problems_in, worded};

fn image(filter: &str) -> String {
    format!(
        "<image src=\"pictures/worked-strip.png\" left=\"0\" top=\"0\" width=\"8\" height=\"1\" filter=\"{filter}\"/>"
    )
}

#[test]
fn a_step_that_is_not_one_of_the_four_is_reported_with_the_four_names() {
    let problem = only_problem_in(&image("blur(2)"));
    assert_eq!(
        worded(&problem),
        "line 2: FS-0091: `filter=\"...\"` is not a list of: brightness(), contrast(), saturate(), grayscale()"
    );
}

#[test]
fn a_step_with_no_number_is_reported_with_the_four_names() {
    let problem = only_problem_in(&image("brightness"));
    assert_eq!(
        worded(&problem),
        "line 2: FS-0091: `filter=\"...\"` is not a list of: brightness(), contrast(), saturate(), grayscale()"
    );
}

#[test]
fn one_bad_step_in_a_list_is_reported_with_the_four_names() {
    let problem = only_problem_in(&image("brightness(0.5) blur(2)"));
    assert_eq!(
        worded(&problem),
        "line 2: FS-0091: `filter=\"...\"` is not a list of: brightness(), contrast(), saturate(), grayscale()"
    );
}

#[test]
fn the_four_steps_in_order_are_read_with_no_problem() {
    let problems = problems_in(&image("brightness(0.5) contrast(2) saturate(0) grayscale(1)"));
    assert!(problems.is_empty(), "the four steps read with problems: {problems:#?}");
}

#[test]
fn a_filter_on_a_box_is_reported_with_the_elements_that_take_it() {
    let problem =
        only_problem_in("<box left=\"0\" top=\"0\" width=\"8\" height=\"8\" filter=\"brightness(0.5)\"/>");
    assert_eq!(
        worded(&problem),
        "line 2: FS-0092: `filter` is for an `<image>` or a `<video>`, not a `<box>`"
    );
}

#[test]
fn a_filter_on_a_text_is_reported_with_the_elements_that_take_it() {
    let problem = only_problem_in(
        "<text width=\"8\" height=\"8\" filter=\"saturate(0)\">\nhello\n</text>",
    );
    assert_eq!(
        worded(&problem),
        "line 2: FS-0092: `filter` is for an `<image>` or a `<video>`, not a `<text>`"
    );
}

#[test]
fn a_filter_on_a_shape_is_reported_with_the_elements_that_take_it() {
    let problem = only_problem_in(
        "<shape d=\"M0 0\" width=\"8\" height=\"8\" filter=\"grayscale(1)\"/>",
    );
    assert_eq!(
        worded(&problem),
        "line 2: FS-0092: `filter` is for an `<image>` or a `<video>`, not a `<shape>`"
    );
}
