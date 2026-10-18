//! The frames of the texts that are dressed (upper case, shadows, outline, no wrapping) equal their checked-in pictures to
//! the byte on the processor (`docs/spec/text.md`, sections 6.6 and 9; `PLAN.md`, section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// Checks every checked-in picture of a fixture against the frame drawn for it by the processor.
fn drawn_frames_equal_the_checked_in_pictures(fixture: &str) {
    let folder = project_copied_from_fixture("text-dressing-checked-in-pictures", fixture, fixture);
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
fn the_frames_of_a_text_with_the_uppercase_transform_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-dressing-upper-case");
}

#[test]
fn the_frames_of_a_text_with_one_shadow_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-dressing-shadow");
}

#[test]
fn the_frames_of_a_text_with_two_shadows_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-dressing-two-shadows");
}

#[test]
fn the_frames_of_a_text_with_a_blurred_shadow_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-dressing-blurred-shadow");
}

#[test]
fn the_frames_of_a_text_with_an_outline_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-dressing-outline");
}

#[test]
fn the_frames_of_a_text_with_wrapping_turned_off_beside_a_wrapped_one_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-dressing-no-wrap");
}
