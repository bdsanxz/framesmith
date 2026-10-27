//! The fixture `finishes-torn-and-grain` (`docs/spec/finishes.md`, section 5): its frames on the processor equal their
//! checked-in pictures to the byte; those pictures hold, pixel by pixel, the torn edge and the grain worked from the spec with
//! a second working of the noise (`crates/draw/tests/reference_noise`); and the card's frames agree with the processor's
//! within `card-renderer.md` section 4 (reference machine only).

mod common;
#[path = "../../draw/tests/reference_noise/mod.rs"]
mod reference_noise;

use common::agreement::compare_scene;
use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use reference_noise::Reference;
use render::Scene;

const FIXTURE: &str = "finishes-torn-and-grain";
const BACKGROUND: [u8; 3] = [0x1a, 0x2b, 0x3c];
const ORANGE: [u8; 3] = [0xe0, 0x40, 0x20];
const CREAM: [u8; 3] = [0xf0, 0xe8, 0xd0];

fn scene() -> Scene {
    let folder = project_copied_from_fixture("finishes-fixture", FIXTURE, FIXTURE);
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
fn the_checked_in_pictures_hold_the_worked_torn_edge_and_grain() {
    let torn = Reference::new(0.035, 3, 9);
    let grain = Reference::new(0.9, 2, 5);
    for (number, moved) in [(0_u64, 0_i64), (9, 18)] {
        let picture = expected(FIXTURE, number);
        let left = 20 + moved;
        for row in 0..100_i64 {
            for column in 0..160_i64 {
                let got = picture.pixel(column as u32, row as u32).unwrap();
                if column < 100 {
                    // The torn square: the noise at the centre less the moved square's corner; the place it names, whole.
                    let n = torn.value([column as f64 + 0.5 - left as f64, row as f64 + 0.5 - 20.0]);
                    let step = (14.0 * (n - 0.5)).round() as i64;
                    let inside = (left..left + 50).contains(&(column + step)) && (20..70).contains(&(row + step));
                    let wanted = if inside { ORANGE } else { BACKGROUND };
                    assert_eq!(got[..3], wanted, "frame {number}, ({column}, {row})");
                } else if (100..140).contains(&column) && (20..80).contains(&row) {
                    // The grained box: its alpha is 255 g, g = clamp(-1.7 n + 1.75, 0, 1); each premultiplied colour is
                    // floor(c g + 0.5), laid over the background by src-over.
                    let n = grain.value([column as f64 + 0.5 - 100.0, row as f64 + 0.5 - 20.0]);
                    let share = (-1.7 * n + 1.75).clamp(0.0, 1.0);
                    let alpha = (255.0 * share + 0.5).floor();
                    for channel in 0..3 {
                        let ink = (f64::from(CREAM[channel]) * share + 0.5).floor();
                        let wanted = ink + f64::from(BACKGROUND[channel]) * (255.0 - alpha) / 255.0;
                        assert!(
                            (f64::from(got[channel]) - wanted).abs() <= 1.0,
                            "frame {number}, ({column}, {row}), channel {channel}: {} against {wanted}",
                            got[channel]
                        );
                    }
                } else {
                    assert_eq!(got[..3], BACKGROUND, "frame {number}, ({column}, {row})");
                }
            }
        }
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_the_torn_edge_and_grain() {
    let scene = scene();
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("{FIXTURE}:\n{}", report.join("\n"));
    assert!(comparison.passed(), "{FIXTURE}: {}", report.join("\n"));
}
