//! The frames of the row and column fixtures, drawn on the processor, equal their checked-in pictures to
//! the byte: the first frame and the last of each (`docs/spec/render-command.md` sections 4.3 and 9;
//! `PLAN.md` section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_from_fixture("rows-and-columns-checked-in", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"))
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
fn the_frames_of_the_row_with_gaps_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("rows-with-gaps");
}

#[test]
fn the_frames_of_the_column_with_gaps_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("columns-with-gaps");
}

#[test]
fn the_frames_of_the_row_centred_and_at_the_end_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("row-along-center-and-end");
}

#[test]
fn the_frames_of_the_row_aligned_across_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("row-align-across");
}

#[test]
fn the_frames_of_the_column_aligned_across_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("column-align-across");
}

#[test]
fn the_frames_of_the_padded_row_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("padding-inside-a-row");
}

#[test]
fn the_frames_of_the_row_with_no_size_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("row-with-no-size");
}

#[test]
fn the_frames_of_the_row_with_an_absolute_box_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("absolute-child-in-a-row");
}

#[test]
fn the_frames_of_the_row_with_a_gap_of_zero_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("gap-of-zero");
}

#[test]
fn the_frames_of_the_row_inside_a_row_with_no_size_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("row-inside-a-row-with-no-size");
}

#[test]
fn the_frames_of_the_stretched_children_of_rows_and_columns_with_no_size_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("stretch-across-with-no-size");
}
