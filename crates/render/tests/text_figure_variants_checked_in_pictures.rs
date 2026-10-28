//! The frames of the texts with figure variants equal their checked-in pictures to the byte on the processor
//! (`docs/spec/text.md`, section 6.6, rule 4; `docs/spec/test-font.md`, section 7; `PLAN.md`, section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// Checks every checked-in picture of a fixture against the frame drawn for it by the processor.
fn drawn_frames_equal_the_checked_in_pictures(fixture: &str) {
    let folder = project_copied_from_fixture("text-figure-variants-checked-in-pictures", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
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
fn the_frames_of_the_tabular_and_proportional_figure_widths_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-figure-widths");
}

#[test]
fn the_frames_of_the_lining_and_old_style_figures_equal_their_checked_in_pictures() {
    drawn_frames_equal_the_checked_in_pictures("text-figure-oldstyle");
}
