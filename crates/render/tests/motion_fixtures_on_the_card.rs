//! The card's pictures of the motion fixtures agree with the processor's on every frame, at the tolerance of `PLAN.md`
//! section 5 (`docs/spec/card-renderer.md`, section 10). Each needs a graphics card that frx7 can draw with: reference
//! machine only.

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

fn the_card_agrees_with_the_processor(fixture: &str) {
    let folder = project_copied_from_fixture("motion-fixtures-on-the-card", fixture, fixture);
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
fn reference_machine_only_the_card_agrees_with_the_processor_on_a_box_moved_across() {
    the_card_agrees_with_the_processor("motion-box-across");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_a_box_that_turns_scales_slants_and_fades() {
    the_card_agrees_with_the_processor("motion-box-turn-and-fade");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_a_dressed_text_that_moves_scales_and_turns() {
    the_card_agrees_with_the_processor("motion-text");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_a_box_of_boxes_and_text_that_fades_as_one() {
    the_card_agrees_with_the_processor("motion-group-fade");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_boxes_moved_by_sets() {
    the_card_agrees_with_the_processor("motion-set-at-an-instant");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_sets_in_order_with_tweens_and_a_set_text() {
    the_card_agrees_with_the_processor("motion-set-and-tween");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_boxes_moved_by_eased_tweens() {
    the_card_agrees_with_the_processor("motion-eased");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_the_order_proof_s_composition() {
    the_card_agrees_with_the_processor("frame-order");
}
