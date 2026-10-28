//! The frames of the fixtures of borders, rounded corners and cut children equal their checked-in pictures to the byte on the
//! processor, at the chosen frames before, while and after something in them moves (`docs/spec/box-edges.md`, section 8).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const CHOSEN: [u64; 4] = [0, 5, 10, 19];

fn drawn_frames_equal_the_checked_in_pictures(fixture: &str) {
    let folder = project_copied_from_fixture("box-edges-fixtures-checked-in", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(fixture);
    assert_eq!(numbers, CHOSEN, "{fixture}: the chosen frames have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
            panic!("{fixture}: {difference}");
        }
    }
}

#[test]
fn the_frames_of_borders_of_one_and_two_lines_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("box-borders");
}

#[test]
fn the_frames_of_rounded_corners_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("box-corners");
}

#[test]
fn the_frames_of_children_cut_at_their_box_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("box-cut");
}
