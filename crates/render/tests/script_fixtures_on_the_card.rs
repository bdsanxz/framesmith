//! The card's pictures of the script fixtures agree with the processor's, frame by frame, at the tolerance of
//! `PLAN.md` section 5 (`docs/spec/card-renderer.md` sections 4 and 7). The comparison covers every frame of each
//! fixture, which includes the chosen frames 0 and 9. The fixtures name the machine's font, so the tests are compiled
//! for macOS only, and each needs a graphics card that frx7 can draw with: reference machine only.

#![cfg(target_os = "macos")]

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const GROUP: &str = "script-fixtures-on-the-card";

fn scene_of(fixture: &str) -> Scene {
    let folder = project_copied_from_fixture(GROUP, fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

/// The card's pictures of `fixture` agree with the processor's on every frame, within the tolerance.
fn the_card_agrees_with_the_processor(fixture: &str) {
    let scene = scene_of(fixture);
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    let report = comparison.report();
    assert!(comparison.passed(), "{fixture}: {}", report.join("\n"));
    assert_eq!(
        report.last().unwrap(),
        &format!("the comparison passed: all {} frames are within the tolerance", scene.frames()),
        "{fixture}"
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_cyrillic_text() {
    the_card_agrees_with_the_processor("text-cyrillic");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_arabic_text() {
    the_card_agrees_with_the_processor("text-arabic");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_hebrew_and_latin_text() {
    the_card_agrees_with_the_processor("text-hebrew-latin");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_japanese_text() {
    the_card_agrees_with_the_processor("text-japanese");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_thai_text() {
    the_card_agrees_with_the_processor("text-thai");
}
