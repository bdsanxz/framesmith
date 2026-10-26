//! The fixture `blends-sixteen` (`docs/spec/blends.md`, section 7): its frames on the processor equal their checked-in
//! pictures to the byte; those pictures hold, cell by cell, the colours worked by hand in
//! `crates/draw/tests/blend_functions.rs` and `crates/render/tests/blends_worked_by_hand.rs`; and the card's frames agree
//! with the processor's within `card-renderer.md` section 4 (reference machine only).

mod common;

use common::agreement::compare_scene;
use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "blends-sixteen";

/// Each blend in the order of its cell, with its worked colour for pair A and for pair B.
const WORKED: [(&str, [u8; 3], [u8; 3]); 16] = [
    ("normal", [204, 51, 153], [51, 255, 102]),
    ("multiply", [82, 31, 31], [41, 0, 102]),
    ("screen", [224, 173, 173], [214, 255, 255]),
    ("overlay", [163, 92, 61], [173, 0, 255]),
    ("darken", [102, 51, 51], [51, 0, 102]),
    ("lighten", [204, 153, 153], [204, 255, 255]),
    ("color-dodge", [255, 191, 128], [255, 0, 255]),
    ("color-burn", [64, 0, 0], [0, 0, 255]),
    ("hard-light", [194, 61, 92], [82, 255, 204]),
    ("soft-light", [138, 116, 64], [180, 0, 255]),
    ("difference", [102, 102, 102], [153, 255, 153]),
    ("exclusion", [143, 143, 143], [173, 255, 153]),
    ("hue", [190, 88, 156], [0, 145, 36]),
    ("saturation", [90, 166, 13], [181, 18, 222]),
    ("color", [222, 69, 171], [0, 145, 36]),
    ("luminosity", [84, 135, 33], [231, 135, 255]),
];

const A: ([u8; 3], [u8; 3]) = ([204, 51, 153], [102, 153, 51]);
const B: ([u8; 3], [u8; 3]) = ([51, 255, 102], [204, 0, 255]);

fn scene() -> Scene {
    let folder = project_copied_from_fixture("blends-fixture", FIXTURE, FIXTURE);
    Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_frames_drawn_on_the_processor_equal_their_checked_in_pictures() {
    let scene = scene();
    let numbers = numbers_with_a_picture(FIXTURE);
    assert_eq!(numbers, vec![0, 9]);
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{difference}");
        }
    }
}

#[test]
fn the_checked_in_pictures_hold_the_worked_colours_in_every_cell() {
    let near = |got: [u8; 4], wanted: [u8; 3]| (0..3).all(|channel| got[channel].abs_diff(wanted[channel]) <= 1);
    for number in [0, 9] {
        let picture = expected(FIXTURE, number);
        let at = |x: u32, y: u32| picture.pixel(x, y).unwrap();
        for (pair, (source, under)) in [A, B].into_iter().enumerate() {
            for (index, (name, worked_a, worked_b)) in WORKED.iter().enumerate() {
                let (left, middle) = ((index as u32 % 4) * 30, (pair as u32 * 4 + index as u32 / 4) * 10 + 5);
                let worked = if pair == 0 { worked_a } else { worked_b };
                assert_eq!(
                    at(left + 5, middle)[..3],
                    under,
                    "frame {number}, {name}, pair {pair}: the box below alone"
                );
                assert!(
                    near(at(left + 15, middle), *worked),
                    "frame {number}, {name}, pair {pair}: the overlap {:?} against {worked:?}",
                    at(left + 15, middle)
                );
                assert_eq!(
                    at(left + 25, middle)[..3],
                    source,
                    "frame {number}, {name}, pair {pair}: the blended box alone"
                );
            }
        }
        // Multiply at an opacity of 0.6 (worked in blends_worked_by_hand.rs), and multiply with no parent over black.
        assert!(near(at(15, 85), [90, 80, 39]), "{:?}", at(15, 85));
        assert!(near(at(25, 85), [122, 31, 92]), "{:?}", at(25, 85));
        assert!(near(at(75, 85), [82, 31, 31]), "{:?}", at(75, 85));
        assert_eq!(at(85, 85)[..3], [0, 0, 0]);
        assert_eq!(at(65, 85)[..3], A.1);
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_the_sixteen_blends() {
    let scene = scene();
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("{FIXTURE}:\n{}", report.join("\n"));
    assert!(comparison.passed(), "{FIXTURE}: {}", report.join("\n"));
}
