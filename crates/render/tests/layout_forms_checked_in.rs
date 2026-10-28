//! The frames of the layout-form fixtures (space between, a shared baseline, columns of stated widths and shares, and a
//! widest width), drawn on the processor, equal their checked-in pictures to the byte: the first frame and the last of
//! each (`docs/spec/render-command.md` sections 4.3, 4.4 and 9; `PLAN.md` section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_copied_from_fixture("layout-forms-checked-in", fixture, fixture);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn drawn_frames_equal_their_checked_in_pictures(fixture: &str) {
    let scene = scene_of(fixture);
    let numbers = numbers_with_a_picture(fixture);
    assert_eq!(numbers, vec![0, 9], "{fixture}: the first and the last frame have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap_or_else(|error| panic!("{error}"));
        if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
            panic!("{fixture}: {difference}");
        }
    }
}

#[test]
fn the_frames_of_the_row_with_space_between_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("space-between-row");
}

#[test]
fn the_frames_of_the_row_with_a_shared_baseline_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("shared-baseline-texts");
}

#[test]
fn the_frames_of_the_columns_of_stated_widths_and_shares_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("columns-stated-and-shares");
}

#[test]
fn the_frames_of_the_text_with_a_widest_width_equal_their_checked_in_pictures() {
    drawn_frames_equal_their_checked_in_pictures("widest-width-text");
}
