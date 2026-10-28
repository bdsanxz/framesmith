//! The frame of the fixture `zero-border-and-square-corner` equals its checked-in picture to the byte on the processor, and
//! the picture holds the values worked by hand from the box's edges (`docs/spec/box-edges.md`, sections 1 and 5): a border of
//! width 0 adds no ring, so the box is white to its edge; a border of 6 on a radius of 0 is 6 red columns and rows, the
//! corner pixel included; a radius of 12 leaves the corner pixel outside the rounded shape, so the root's black shows.

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "zero-border-and-square-corner";

#[test]
fn the_frame_equals_its_checked_in_picture_on_the_processor() {
    let folder = project_copied_from_fixture("zero-border-checked-in", FIXTURE, FIXTURE);
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
    let red = [255, 0, 0, 255];
    let white = [255, 255, 255, 255];
    let black = [0, 0, 0, 255];
    // Width 0: the box's corner and its middle are white.
    assert_eq!(picture.pixel(8, 8).unwrap(), white);
    assert_eq!(picture.pixel(47, 37).unwrap(), white);
    // Radius 0 with a border of 6: the corner pixel and the 6 columns and rows inside the edge are red.
    assert_eq!(picture.pixel(60, 8).unwrap(), red);
    assert_eq!(picture.pixel(65, 20).unwrap(), red);
    assert_eq!(picture.pixel(66, 20).unwrap(), white);
    assert_eq!(picture.pixel(99, 37).unwrap(), red);
    // Radius 12 with a border of 6: the corner pixel is outside the rounded shape, the middle of the side is red.
    assert_eq!(picture.pixel(112, 8).unwrap(), black);
    assert_eq!(picture.pixel(112, 23).unwrap(), red);
    assert_eq!(picture.pixel(118, 23).unwrap(), white);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_checked_in_picture() {
    let folder = project_copied_from_fixture("zero-border-and-square-corner-card", FIXTURE, FIXTURE);
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
