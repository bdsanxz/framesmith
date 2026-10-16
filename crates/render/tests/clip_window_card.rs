//! The card's pictures of the clip-window fixture agree with the processor's, frame by frame, within the
//! tolerance (`docs/spec/card-renderer.md` sections 4, 5 and 7). The test needs a graphics card that frx7
//! can draw with, so it is reference machine only (`docs/dev/gates.md` section 4).

mod common;

use common::agreement::compare_scene;
use common::project_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_clip_window_fixture_is_within_the_tolerance_of_the_processor() {
    let folder = project_from_fixture("clip-window-card", "clip-window", "clip-window");
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    assert!(comparison.passed(), "{}", report.join("\n"));
    assert_eq!(
        report.last().unwrap(),
        &format!("the comparison passed: all {} frames are within the tolerance", scene.frames())
    );
}
