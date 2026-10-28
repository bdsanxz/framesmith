//! The card's pictures of the texts with figure variants agree with the processor's, frame by frame, within the tolerance
//! (`docs/spec/card-renderer.md`, sections 4, 5 and 9.9). Each test needs a graphics card that frx7 can draw with, so each is
//! reference machine only (`docs/dev/gates.md`, section 4).

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_copied_from_fixture("card-text-figure-variants", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

fn every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor(fixture: &str) {
    let scene = scene_of(fixture);
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    let report = comparison.report();
    assert!(comparison.passed(), "{}", report.join("\n"));
    assert_eq!(
        report.last().unwrap(),
        &format!("the comparison passed: all {} frames are within the tolerance", scene.frames())
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_tabular_and_proportional_figures_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-figure-widths");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_lining_and_old_style_figures_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-figure-oldstyle");
}
