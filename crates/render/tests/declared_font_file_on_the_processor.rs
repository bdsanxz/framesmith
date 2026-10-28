//! A text that names the family of a declared font is drawn with that font's glyphs on frame 0 and on every later frame, and
//! the frames of the fixture equal their checked-in pictures on the processor (`docs/spec/text.md`, section 2.1;
//! `docs/spec/render-command.md`, section 9).

mod common;

use std::fs;

use common::{compare, expected, fixtures, numbers_with_a_picture, project_copied_from_fixture};
use draw::Mark;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "declared-font-file";
const DECLARED_FILE: &str = "framesmith-test-stroke-400.ttf";

fn scene() -> Scene {
    let folder = project_copied_from_fixture("declared-font-file-processor", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_frames_of_a_declared_font_file_equal_their_checked_in_pictures() {
    let scene = scene();
    let numbers = numbers_with_a_picture(FIXTURE);
    assert_eq!(numbers, vec![0, 4, 9], "frame 0, a middle frame and the last frame have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{difference}");
        }
    }
}

#[test]
fn a_text_naming_the_declared_family_is_drawn_with_the_declared_file_on_frame_0_and_on_every_later_frame() {
    let declared = fs::read(fixtures().join(FIXTURE).join("fonts").join(DECLARED_FILE)).unwrap();
    let scene = scene();
    assert_eq!(scene.frames(), 10, "the fixture is 10 frames long");
    for number in 0..scene.frames() {
        let frame = scene.frame(number).unwrap();
        let runs: Vec<_> = frame
            .marks
            .iter()
            .flat_map(|mark| match mark {
                Mark::Text(text) => text.runs.clone(),
                Mark::Step(_) | Mark::Group(_) | Mark::Background(_) | Mark::Ring(_) | Mark::Cut(_) | Mark::BoxShadow(_) => Vec::new(),
            })
            .collect();
        assert_eq!(runs.len(), 1, "frame {number} draws one run of glyphs");
        assert_eq!(
            runs[0].font.data.as_slice(),
            declared.as_slice(),
            "frame {number} draws the glyphs of the declared file"
        );
        assert_eq!(runs[0].glyphs.len(), 4, "frame {number} draws the four capitals");
    }
}
