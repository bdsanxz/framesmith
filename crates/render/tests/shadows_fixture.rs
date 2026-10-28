//! The frames of the fixture `shadows` equal their checked-in pictures to the byte on the processor, and the card's frames
//! agree with the processor's within the tolerance of `docs/spec/card-renderer.md` section 4 (`docs/spec/render-command.md`,
//! section 4.7, rule 6). The card needs a graphics card that frx7 can draw with, so that test is reference machine only.

mod common;

use common::agreement::compare_scene;
use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture, project_from_fixture};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "shadows";

#[test]
fn the_drawn_frames_of_the_shadows_fixture_equal_their_checked_in_pictures() {
    let folder = project_copied_from_fixture("shadows-fixture", "pictures", FIXTURE);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(FIXTURE);
    assert_eq!(numbers, [0, 5], "the chosen frames have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{FIXTURE}: {difference}");
        }
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_shadows_fixture_is_within_the_tolerance_of_the_processor() {
    let folder = project_from_fixture("shadows-fixture-card", "card", FIXTURE);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    assert!(comparison.passed(), "{}", report.join("\n"));
}
