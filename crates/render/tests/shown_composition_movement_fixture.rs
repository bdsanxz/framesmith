//! The frames of the fixture of a shown composition that moves equal their checked-in pictures to the byte on the
//! processor, each picture holds the worked places (`docs/spec/render-command.md`, section 4.5, rule 11; `PLAN.md`,
//! section 5.2), and the card agrees with the processor at the tolerance of `PLAN.md` section 5 (reference machine only).

mod common;

use common::agreement::compare_scene;
use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "shown-composition-movement";

fn scene(group: &str) -> Scene {
    let folder = project_copied_from_fixture(group, FIXTURE, FIXTURE);
    Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_frames_of_a_moving_shown_composition_equal_their_checked_in_pictures() {
    let scene = scene("shown-composition-movement-fixture");
    let numbers = numbers_with_a_picture(FIXTURE);
    assert_eq!(numbers, [30, 45, 60], "the chosen frames have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{FIXTURE}: {difference}");
        }
    }
}

fn colour(picture: &Picture, x: u32, y: u32) -> [u8; 3] {
    let pixel = picture.pixel(x, y).unwrap();
    [pixel[0], pixel[1], pixel[2]]
}

/// Asserts the colour of the pixels `from` up to `to` along a row (`across`) or a column.
fn run(picture: &Picture, across: bool, at: u32, from: u32, to: u32, wanted: [u8; 3], what: &str) {
    for along in from..to {
        let (x, y) = if across { (along, at) } else { (at, along) };
        assert_eq!(colour(picture, x, y), wanted, "{what}: ({x}, {y})");
    }
}

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREEN: [u8; 3] = [0, 255, 0];
const BLACK: [u8; 3] = [0, 0, 0];

#[test]
fn the_checked_in_pictures_hold_the_worked_places() {
    for (number, dx) in [(30, 0), (45, 30), (60, 60)] {
        let picture = expected(FIXTURE, number);
        let what = format!("frame {number}");
        // The top file at (10 + dx, 10): row 15 red from 10 + dx to 30 + dx and blue to 50 + dx; row 25 crosses the green
        // inside the file, x 40 + dx to 50 + dx, which ends at the file's edge, cut before the move.
        run(&picture, true, 15, 0, 10 + dx, BLACK, &what);
        run(&picture, true, 15, 10 + dx, 30 + dx, RED, &what);
        run(&picture, true, 15, 30 + dx, 50 + dx, BLUE, &what);
        run(&picture, true, 15, 50 + dx, 160, BLACK, &what);
        run(&picture, true, 25, 30 + dx, 40 + dx, BLUE, &what);
        run(&picture, true, 25, 40 + dx, 50 + dx, GREEN, &what);
        run(&picture, true, 25, 50 + dx, 160, BLACK, &what);
        // The bottom file, turned 90 degrees about (30, 75): x 15 to 45, y 55 to 95; column 20 red from 55 to 75 and blue
        // to 95. Its green, frame x 40 to 50 and y 70 to 80 before the turn, lies at x 25 to 35 and y 85 to 95.
        run(&picture, false, 20, 55, 75, RED, &what);
        run(&picture, false, 20, 75, 95, BLUE, &what);
        run(&picture, false, 20, 95, 100, BLACK, &what);
        run(&picture, false, 30, 85, 95, GREEN, &what);
        run(&picture, true, 60, 15, 45, RED, &what);
        run(&picture, true, 60, 45, 160, BLACK, &what);
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_a_moving_shown_composition() {
    let scene = scene("shown-composition-movement-fixture-on-the-card");
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("{}", report.join("\n"));
    assert!(comparison.passed(), "{}", report.join("\n"));
}
