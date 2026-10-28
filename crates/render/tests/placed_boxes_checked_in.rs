//! The frames of the placement fixtures, drawn on the processor, equal their checked-in pictures to
//! the byte: the first frame and the last of each (`docs/spec/render-command.md` sections 4 and 9;
//! `PLAN.md` section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_from_fixture("placed-boxes-checked-in", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn drawn_frames_equal_their_checked_in_pictures(fixture: &str) {
    let scene = scene_of(fixture);
    let numbers = numbers_with_a_picture(fixture);
    assert_eq!(numbers, vec![0, 9], "{fixture}: the first and the last frame have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
            panic!("{fixture}: {difference}");
        }
    }
}

#[test]
fn the_frames_of_the_box_placed_in_pixels_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("placement-in-pixels");
}

#[test]
fn the_frames_of_the_boxes_placed_in_percentages_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("placement-in-percentages");
}

#[test]
fn the_frames_of_the_boxes_placed_between_two_edges_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("placement-between-edges");
}

#[test]
fn the_frames_of_the_box_placed_inside_a_box_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("placement-inside-a-box");
}

#[test]
fn the_frames_of_the_boxes_placed_from_the_bottom_and_right_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("placement-from-the-bottom-and-right");
}

#[test]
fn the_frames_of_the_boxes_with_a_margin_and_a_position_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("placement-margin-and-position");
}

#[test]
fn the_frames_of_the_boxes_of_zero_size_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("placement-zero-sizes");
}
