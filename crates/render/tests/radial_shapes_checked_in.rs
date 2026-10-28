//! The frame of the fixture `radial-shapes-and-tile-percent` equals its checked-in picture to the byte on the processor, and
//! the picture holds the values worked by hand from the spec's formulas (`docs/spec/render-command.md`, section 4.6, rules 6
//! and 8): for a black and white radial gradient the share of the way from the centre at a pixel centre `(du, dv)` is
//! `√((du / rx)² + (dv / ry)²)`, the colour `⌊255 (1 − f) + 0.5⌋`, and 0 past the edge. The radii: `circle` with none, the
//! distance to the farthest corner (hypot(40, 20)); `ellipse` with none, √2 times 40 and 20; `ellipse 50% 25%`, 40 and 10.
//! Tiles of 20 by 20 in a 96 by 50 box: at 0 %, 50 % and 100 %, the room is 76 across and 30 down.

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "radial-shapes-and-tile-percent";

/// `⌊255 v + 0.5⌋`, the level of a value from 0 to 1.
fn level(value: f64) -> u8 {
    (value * 255.0 + 0.5).floor() as u8
}

/// The expected grey of the box at `(left, top)` of 80 by 40 pixels, at the pixel `(x, y)` of the frame, for radii `radii`
/// and the centre of the box, from the formula above.
fn grey(left: u32, top: u32, radii: (f64, f64), x: u32, y: u32) -> u8 {
    let (cx, cy) = (f64::from(left) + 40.0, f64::from(top) + 20.0);
    let (du, dv) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
    let share = ((du / radii.0).powi(2) + (dv / radii.1).powi(2)).sqrt();
    level((1.0 - share).clamp(0.0, 1.0))
}

#[test]
fn the_frame_equals_its_checked_in_picture_on_the_processor() {
    let folder = project_copied_from_fixture("radial-shapes-checked-in", FIXTURE, FIXTURE);
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
    let picture: Picture = expected(FIXTURE, 0);
    let diagonal = 40.0_f64.hypot(20.0);
    let ellipse = (40.0 * std::f64::consts::SQRT_2, 20.0 * std::f64::consts::SQRT_2);
    let percent = (40.0, 10.0);
    for (left, radii) in [(0, (diagonal, diagonal)), (100, ellipse), (200, percent)] {
        for (x, y) in [(left + 39, 19), (left, 0), (left + 79, 39), (left + 40, 30), (left + 10, 35)] {
            let found = picture.pixel(x, y).unwrap();
            let wanted = grey(left, 0, radii, x, y);
            assert!(
                found[..3].iter().all(|&value| value == wanted),
                "box at {left}: ({x}, {y}) {found:?} against {wanted}"
            );
        }
    }
    // The tiles: red 20 by 20 at (38, 80) for 50 % and 100 %, at the top left for 0 %, and at (276, 80) for 100 %.
    let red = [255, 0, 0, 255];
    let white = [255, 255, 255, 255];
    assert_eq!(picture.pixel(37, 80).unwrap(), white);
    assert_eq!(picture.pixel(38, 80).unwrap(), red);
    assert_eq!(picture.pixel(57, 99).unwrap(), red);
    assert_eq!(picture.pixel(58, 99).unwrap(), white);
    assert_eq!(picture.pixel(100, 50).unwrap(), red);
    assert_eq!(picture.pixel(120, 50).unwrap(), white);
    assert_eq!(picture.pixel(275, 80).unwrap(), white);
    assert_eq!(picture.pixel(276, 80).unwrap(), red);
    assert_eq!(picture.pixel(295, 99).unwrap(), red);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_checked_in_picture() {
    let folder = project_copied_from_fixture("radial-shapes-and-tile-percent-card", FIXTURE, FIXTURE);
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
