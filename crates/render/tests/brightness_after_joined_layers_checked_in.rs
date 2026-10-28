//! The frame of the fixture `brightness-after-joined-layers` equals its checked-in picture to the byte on the processor, and
//! the picture holds the values worked by hand (`docs/spec/notation.md`, section 7.6): a white of alpha 64 over `#202020` is
//! 64 + (1 − 64/255) × 32 = 87.97, shown as 88 (`#585858`); brightened by 2 it is 175.9, shown as 176 (`#b0b0b0`). The left
//! element has no brightness and the right one has 2; the two are a box each, 96 pixels wide.

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "brightness-after-joined-layers";

#[test]
fn the_frame_equals_its_checked_in_picture_on_the_processor() {
    let folder = project_copied_from_fixture("brightness-after-joined-layers-checked-in", FIXTURE, FIXTURE);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(FIXTURE);
    assert_eq!(numbers, [0], "the chosen frame has a picture");
    let drawn = scene.picture(0, &mut CpuRenderer).unwrap();
    if let Err(difference) = compare(0, &drawn, &expected(FIXTURE, 0)) {
        panic!("{FIXTURE}: {difference}");
    }
}

#[test]
fn the_checked_in_picture_holds_the_worked_values() {
    let picture = expected(FIXTURE, 0);
    for x in [0, 48, 95] {
        assert_eq!(picture.pixel(x, 50).unwrap(), [88, 88, 88, 255], "joined, no brightness, at {x}");
    }
    for x in [104, 152, 199] {
        assert_eq!(picture.pixel(x, 50).unwrap(), [176, 176, 176, 255], "joined, brightness 2, at {x}");
    }
    // Between the two elements the root's black shows.
    assert_eq!(picture.pixel(99, 50).unwrap(), [0, 0, 0, 255]);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_checked_in_picture() {
    let folder = project_copied_from_fixture("brightness-after-joined-layers-card", FIXTURE, FIXTURE);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let drawn = scene.picture(0, &mut card).unwrap();
    let wanted = expected(FIXTURE, 0);
    // The card agrees with the processor within 4 levels of 255 (`PLAN.md`, section 5.2).
    let worst = drawn
        .pixels()
        .iter()
        .zip(wanted.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(worst <= 4, "{FIXTURE}: the card differs by {worst} levels");
}
