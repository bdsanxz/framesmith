//! The card's pictures agree with the processor's, frame by frame, on the fixtures; and a card
//! that draws a different picture fails the comparison and says which frame, with both figures
//! (`docs/spec/card-renderer.md` sections 4, 5 and 7). Each test needs a graphics card that frx7
//! can draw with, so each is reference machine only (`docs/dev/gates.md` section 4).

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_copied_from_fixture("card-agreement", fixture, fixture);
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
fn reference_machine_only_every_frame_of_the_box_placed_in_pixels_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("placement-in-pixels");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_boxes_placed_in_percentages_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("placement-in-percentages");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_boxes_placed_between_two_edges_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("placement-between-edges");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_box_placed_inside_a_box_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("placement-inside-a-box");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_boxes_placed_from_the_bottom_and_right_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("placement-from-the-bottom-and-right");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_boxes_with_a_margin_and_a_position_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("placement-margin-and-position");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_boxes_of_zero_size_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("placement-zero-sizes");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_of_colour_and_weight_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-colour-and-weight");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_in_the_default_colour_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-default-colour");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_wrapped_and_aligned_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-wrapping-and-alignment");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_with_line_height_and_spacing_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-line-height-and-spacing");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_text_that_takes_its_attributes_is_within_the_tolerance_of_the_processor() {
    every_frame_of_the_fixture_is_within_the_tolerance_of_the_processor("text-attributes-inherited");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_span_colour_fixture_is_within_the_tolerance_of_the_processor() {
    let scene = scene_of("text-spans-colour-and-baseline");
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    assert!(comparison.passed(), "{}", comparison.report().join("\n"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_larger_span_fixture_is_within_the_tolerance_of_the_processor() {
    let scene = scene_of("text-spans-larger-size");
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    assert!(comparison.passed(), "{}", comparison.report().join("\n"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_span_attributes_fixture_is_within_the_tolerance_of_the_processor() {
    let scene = scene_of("text-spans-inherit-attributes");
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    assert!(comparison.passed(), "{}", comparison.report().join("\n"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_span_line_break_fixture_is_within_the_tolerance_of_the_processor() {
    let scene = scene_of("text-spans-line-break");
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    assert!(comparison.passed(), "{}", comparison.report().join("\n"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_span_margin_fixture_is_within_the_tolerance_of_the_processor() {
    let scene = scene_of("text-spans-margin-and-padding");
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card(), None);
    assert!(comparison.passed(), "{}", comparison.report().join("\n"));
}
