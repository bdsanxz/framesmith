//! The card's pictures of the fixtures of borders, rounded corners and cut children agree with the processor's on every
//! frame, at the tolerance of `PLAN.md` section 5 (`docs/spec/box-edges.md`, section 7). Each needs a graphics card that frx7
//! can draw with: reference machine only.

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

fn the_card_agrees_with_the_processor(fixture: &str) {
    let folder = project_copied_from_fixture("box-edges-fixtures-on-the-card", fixture, fixture);
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("{fixture}:\n{}", report.join("\n"));
    assert!(comparison.passed(), "{fixture}: {}", report.join("\n"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_borders_of_one_and_two_lines() {
    the_card_agrees_with_the_processor("box-borders");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_rounded_corners() {
    the_card_agrees_with_the_processor("box-corners");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_children_cut_at_their_box() {
    the_card_agrees_with_the_processor("box-cut");
}
