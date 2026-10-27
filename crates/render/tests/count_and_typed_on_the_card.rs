//! The card's pictures of a counting text and of typed text agree with the processor's on every frame, at the tolerance
//! of `PLAN.md` section 5 (`docs/spec/card-renderer.md`, section 10). Each needs a graphics card that frx7 can draw with:
//! reference machine only.

mod common;

use common::agreement::compare_scene;
use common::{fixtures, project_copied_from_fixture, project_with};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;
use std::fs;

fn the_card_agrees_with_the_processor(fixture: &str) {
    let folder = project_copied_from_fixture("count-and-typed-on-the-card", fixture, fixture);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("{fixture}:\n{}", report.join("\n"));
    assert!(comparison.passed(), "{fixture}: {}", report.join("\n"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_numbers_that_count() {
    the_card_agrees_with_the_processor("count-a-number");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_texts_typed_out() {
    the_card_agrees_with_the_processor("type-a-text");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_a_span_typed_out() {
    the_card_agrees_with_the_processor("type-a-span");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_a_typed_fraction() {
    let text = "<composition version=\"1\" width=\"180\" height=\"40\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n<font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>\n<text left=\"10\" top=\"8\" font-family=\"Framesmith Test Stroke\" font-size=\"20\" color=\"#202020\">HELLO<tween at=\"0\" duration=\"1\" from-typed=\"0\" to-typed=\"5\"/></text>\n</composition>\n";
    let folder = project_with("count-and-typed-on-the-card", "type-a-fraction", text);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let font = "framesmith-test-stroke-400.ttf";
    fs::copy(fixtures().join("fonts").join(font), folder.join("fonts").join(font)).unwrap();
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("type-a-fraction:\n{}", report.join("\n"));
    assert!(comparison.passed(), "type-a-fraction: {}", report.join("\n"));
}
