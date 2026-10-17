//! The frames of the text fixtures equal their checked-in pictures to the byte on the processor (`docs/spec/text.md`,
//! section 9; `PLAN.md`, section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// Checks every checked-in picture of a text fixture against the frame drawn for it by the processor.
fn drawn_frames_equal_the_checked_in_pictures(fixture: &str) {
    let folder = project_copied_from_fixture("text-checked-in-pictures", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(fixture);
    assert_eq!(numbers, vec![0, 9], "the first and the last frame have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
            panic!("{difference}");
        }
    }
}

#[test]
fn the_frames_of_a_text_in_two_weights_and_colours_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-colour-and-weight");
}

#[test]
fn the_frames_of_a_text_with_no_colour_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-default-colour");
}

#[test]
fn the_frames_of_wrapped_and_aligned_texts_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-wrapping-and-alignment");
}

#[test]
fn the_frames_of_line_heights_and_letter_spacings_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-line-height-and-spacing");
}

#[test]
fn the_frames_of_texts_inside_a_box_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-attributes-inherited");
}

#[test]
fn the_frames_of_a_span_in_another_colour_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-spans-colour-and-baseline");
}

#[test]
fn the_frames_of_a_span_in_a_larger_size_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-spans-larger-size");
}

#[test]
fn the_frames_of_a_span_that_takes_the_attributes_of_its_text_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-spans-inherit-attributes");
}

#[test]
fn the_frames_of_a_span_broken_across_two_lines_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-spans-line-break");
}

#[test]
fn the_frames_of_a_span_with_margin_and_padding_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-spans-margin-and-padding");
}
