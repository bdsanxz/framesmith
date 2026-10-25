//! The frames of the motion fixtures equal their checked-in pictures to the byte on the processor, at the chosen frames
//! from before a tween or a set starts to after it ends (`docs/spec/render-command.md`, section 4.5; `docs/spec/timeline.md`,
//! section 5; `PLAN.md`, section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

fn drawn_frames_equal_the_checked_in_pictures(fixture: &str) {
    drawn_frames_at_equal_the_checked_in_pictures(fixture, &[0, 5, 10, 15, 19]);
}

fn drawn_frames_at_equal_the_checked_in_pictures(fixture: &str, chosen: &[u64]) {
    let folder = project_copied_from_fixture("motion-fixtures-checked-in", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
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
fn the_frames_of_a_box_moved_across_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("motion-box-across");
}

#[test]
fn the_frames_of_a_box_that_turns_scales_slants_and_fades_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("motion-box-turn-and-fade");
}

#[test]
fn the_frames_of_a_dressed_text_that_moves_scales_and_turns_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("motion-text");
}

#[test]
fn the_frames_of_a_box_of_boxes_and_text_that_fades_as_one_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("motion-group-fade");
}

#[test]
fn the_frames_of_boxes_moved_by_sets_on_and_beside_frame_times_equal_their_checked_in_pictures() {
    drawn_frames_at_equal_the_checked_in_pictures("motion-set-at-an-instant", &[0, 29, 30, 31, 59, 60, 89]);
}

#[test]
fn the_frames_of_sets_in_order_with_tweens_and_a_set_text_equal_their_checked_in_pictures() {
    drawn_frames_at_equal_the_checked_in_pictures("motion-set-and-tween", &[0, 29, 30, 45, 60, 75, 89]);
}
