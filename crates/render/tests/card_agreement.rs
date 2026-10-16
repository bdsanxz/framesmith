//! The card's pictures agree with the processor's, frame by frame, on the fixtures; and a card
//! that draws a different picture fails the comparison and says which frame, with both figures
//! (`docs/spec/card-renderer.md` sections 4, 5 and 7). Each test needs a graphics card that frx7
//! can draw with, so each is reference machine only (`docs/dev/gates.md` section 4).

mod common;

use common::agreement::compare_scene;
use common::project_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_from_fixture("card-agreement", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_first_fixture_is_within_the_tolerance_of_the_processor() {
    let scene = scene_of("first-render");
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    let report = comparison.report();
    assert!(comparison.passed(), "{}", report.join("\n"));
    assert_eq!(report.len() as u64, scene.frames() + 1, "{}", report.join("\n"));
    assert_eq!(
        report.last().unwrap(),
        &format!("the comparison passed: all {} frames are within the tolerance", scene.frames())
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_overlapping_boxes_fixture_is_within_the_tolerance_of_the_processor() {
    let scene = scene_of("overlapping-boxes");
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
fn reference_machine_only_a_doctored_card_fails_the_comparison_and_names_the_frame_with_both_figures() {
    let scene = scene_of("first-render");
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), Some(4));
    let report = comparison.report();
    assert!(!comparison.passed(), "{}", report.join("\n"));

    let line = report
        .iter()
        .find(|line| line.starts_with("frame 0004: similarity"))
        .unwrap_or_else(|| panic!("{}", report.join("\n")));
    assert!(line.ends_with("outside the tolerance"), "{line}");

    let miss = report
        .iter()
        .find(|line| line.starts_with("frame 0004 is outside the tolerance: "))
        .unwrap_or_else(|| panic!("{}", report.join("\n")));
    assert!(miss.contains("similarity ") && miss.contains(" is below 0.995"), "{miss}");
    assert!(miss.contains("% of values are further apart than 4 levels, above 0.05 %"), "{miss}");

    for number in (0..10).filter(|&number| number != 4) {
        let prefix = format!("frame {number:04}: similarity");
        let within = report.iter().find(|line| line.starts_with(&prefix)).unwrap();
        assert!(within.ends_with("within the tolerance"), "{within}");
    }
    assert_eq!(
        report.last().unwrap(),
        "the comparison failed: 1 of 10 frames are outside the tolerance"
    );
}
