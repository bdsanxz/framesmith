//! The card's pictures of the dressed texts agree with the processor's, frame by frame, within the tolerance
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
    let folder = project_copied_from_fixture("card-text-dressing", fixture, fixture);
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
fn reference_machine_only_every_frame_of_the_uppercase_text_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-dressing-upper-case");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_with_one_shadow_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-dressing-shadow");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_with_two_shadows_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-dressing-two-shadows");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_with_a_blurred_shadow_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-dressing-blurred-shadow");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_with_an_outline_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-dressing-outline");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_texts_with_wrapping_turned_off_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-dressing-no-wrap");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_moving_text_with_a_coloured_outline_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("motion-text");
}
