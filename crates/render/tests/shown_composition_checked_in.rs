//! The frames of the shown-composition fixture, drawn on the processor, equal their checked-in pictures to the
//! byte. The frames checked are the first and the last, and each frame at an edge of a window of a shown file
//! (`docs/spec/render-command.md` sections 4 and 9; `PLAN.md` section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "shown-composition";
/// The frames with a checked-in picture: 0 and 119 are the first and the last; 14 to 15 hold the corner's start,
/// 29 to 30 the strip's and the inset's, 44 to 45 the inner box's, 59 to 60 the badge's, 74 to 75 the inset's end.
const CHECKED: [u64; 11] = [0, 15, 29, 30, 44, 45, 59, 60, 74, 75, 119];

#[test]
fn the_frames_of_the_shown_composition_fixture_equal_their_checked_in_pictures() {
    let folder = project_copied_from_fixture("shown-composition-checked-in", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    assert_eq!(
        numbers_with_a_picture(FIXTURE),
        CHECKED.to_vec(),
        "{FIXTURE}: the frames with a picture"
    );
    for number in CHECKED {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{FIXTURE}: {difference}");
        }
    }
}
