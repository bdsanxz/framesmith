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

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_emoji_text() {
    the_card_agrees_with_the_processor("text-emoji");
}

/// Whether a pixel is a yellow of the emoji face: red high, green high, blue low.
fn is_emoji_yellow(pixel: &[u8]) -> bool {
    pixel[0] >= 200 && pixel[1] >= 140 && pixel[2] <= 120
}

/// Whether a pixel is the blue of the text in the fixture, to within a wide margin.
fn is_text_blue(pixel: &[u8]) -> bool {
    let [red, green, blue] = [0x1e_i32, 0x6f, 0xd9];
    (i32::from(pixel[0]) - red).abs() < 40 && (i32::from(pixel[1]) - green).abs() < 40 && (i32::from(pixel[2]) - blue).abs() < 40
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_draws_the_emoji_in_its_own_colours_whatever_the_text_colour() {
    let scene = scene_of("text-emoji");
    let frame = scene.frame(0).unwrap();
    let emoji = frame
        .marks
        .iter()
        .find_map(|mark| match mark {
            draw::Mark::Text(text) => text.pictures.first(),
            _ => None,
        })
        .expect("the emoji is drawn as a picture");
    let rendered = scene.picture(0, &mut card()).unwrap();
    let (left, top) = (emoji.x.round() as u32, emoji.y.round() as u32);
    let (mut yellow, mut blue) = (0_usize, 0_usize);
    for row in 0..emoji.height {
        for column in 0..emoji.width {
            let pixel = rendered.pixel(left + column, top + row).expect("the emoji lies on the frame");
            yellow += usize::from(is_emoji_yellow(&pixel));
            blue += usize::from(is_text_blue(&pixel));
        }
    }
    assert!(yellow > 50, "the card draws the emoji's yellow: {yellow} pixels");
    assert_eq!(blue, 0, "no pixel of the emoji takes the colour of the text on the card");
}
