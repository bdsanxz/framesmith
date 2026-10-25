//! The frames of the background fixtures equal their checked-in pictures to the byte on the processor
//! (`docs/spec/render-command.md`, section 4.6; `PLAN.md`, section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

fn drawn_frames_at_equal_the_checked_in_pictures(fixture: &str, chosen: &[u64]) {
    let folder = project_copied_from_fixture("background-fixtures-checked-in", fixture, fixture);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(fixture);
    assert_eq!(numbers, chosen, "{fixture}: the chosen frames have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
            panic!("{fixture}: {difference}");
        }
    }
}

#[test]
fn the_frame_of_layers_of_colours_and_a_gradient_equals_its_checked_in_picture() {
    drawn_frames_at_equal_the_checked_in_pictures("background-layers", &[0]);
}

#[test]
fn the_frames_of_linear_and_radial_gradients_and_one_that_moves_equal_their_checked_in_pictures() {
    drawn_frames_at_equal_the_checked_in_pictures("background-gradients", &[0, 15, 29]);
}

#[test]
fn the_frame_of_tiles_repeated_drawn_once_and_moved_equals_its_checked_in_picture() {
    drawn_frames_at_equal_the_checked_in_pictures("background-tiles", &[0]);
}
