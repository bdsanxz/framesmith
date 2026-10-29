//! The card's pictures of the shape fixtures agree with the processor's, frame by frame,
//! at the tolerance of `PLAN.md` section 5 (`docs/spec/card-renderer.md`, section 10.7).
//! Each test needs a graphics card that frx7 can draw with, so each is reference machine
//! only (`docs/dev/gates.md` section 4).

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_copied_from_fixture("shape-fixtures-on-the-card", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"))
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
fn reference_machine_only_every_frame_of_the_shape_view_box_scaled_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("shape-view-box-scaled");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_shape_with_corners_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("shape-corners");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_shape_with_fill_and_stroke_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("shape-fill-and-stroke");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_shape_with_dashes_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("shape-dashes");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_shape_with_a_curve_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("shape-curve");
}
