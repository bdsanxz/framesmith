//! The frames of the clip-window fixture, drawn on the processor, equal their checked-in pictures to
//! the byte. The frames checked are the first and the last, and each frame on both sides of a window
//! edge (`docs/spec/render-command.md` sections 4 and 9; `PLAN.md` section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "clip-window";
/// The frames with a checked-in picture: 0 and 119 are the first and the last; 9, 10 and 11 hold the
/// starts at a third of a second; 29 to 30, 39 to 41, 44 to 45, 59 to 60 and 89 to 90 are the edges.
const CHECKED: [u64; 16] = [0, 9, 10, 11, 29, 30, 39, 40, 41, 44, 45, 59, 60, 89, 90, 119];

#[test]
fn the_frames_of_the_clip_window_fixture_equal_their_checked_in_pictures() {
    let folder = project_from_fixture("clip-window-checked-in", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
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
