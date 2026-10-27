//! The frames of the blur fixture equal their checked-in pictures to the byte on the processor, and the pictures hold the
//! values worked by hand from a Gaussian of our own (`docs/spec/render-command.md`, section 4.5, rule 11; `PLAN.md`,
//! section 5.2).

mod common;

use common::gaussian::share;
use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "blur-and-brightness";

#[test]
fn the_frames_of_blurred_brightened_and_faded_elements_equal_their_checked_in_pictures() {
    let folder = project_copied_from_fixture("blur-fixture-checked-in", FIXTURE, FIXTURE);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(FIXTURE);
    assert_eq!(numbers, [0, 15, 29], "the chosen frames have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{FIXTURE}: {difference}");
        }
    }
}

/// The background, `#102040`.
const BACKGROUND: [f64; 3] = [16.0, 32.0, 64.0];

/// `colour` laid with the share `alpha` over the background.
fn over(colour: [f64; 3], alpha: f64) -> [f64; 3] {
    [0, 1, 2].map(|channel| colour[channel] * alpha + BACKGROUND[channel] * (1.0 - alpha))
}

fn holds(picture: &draw::Picture, x: u32, y: u32, wanted: [f64; 3], within: f64, what: &str) {
    let found = picture.pixel(x, y).unwrap();
    assert!(
        (0..3).all(|channel| (f64::from(found[channel]) - wanted[channel]).abs() <= within),
        "{what} at ({x}, {y}): {found:?} against {wanted:?}"
    );
}

#[test]
fn the_checked_in_pictures_hold_the_worked_values_of_blur_brightness_and_fading() {
    let yellow = [240.0, 192.0, 64.0];
    for (number, moved) in [(0, 0.0), (15, 15.25), (29, 30.5 * 29.0 / 30.0)] {
        let picture = expected(FIXTURE, number);
        // The yellow box, from (20 + moved, 20) to (80 + moved, 100), blurred by 5: along its middle row and down its middle
        // column. Each value is rounded once in the element's layer and once where it is laid, so within 2 levels.
        for x in 0..120 {
            let c = f64::from(x) + 0.5;
            let alpha = share(20.0 + moved, 80.0 + moved, c, 5.0) * share(20.0, 100.0, 60.5, 5.0);
            holds(
                &picture,
                x,
                60,
                over(yellow, alpha),
                2.0,
                &format!("frame {number}, the blurred yellow box"),
            );
        }
        for y in 0..120 {
            let alpha = share(20.0 + moved, 80.0 + moved, 50.5 + moved.floor(), 5.0) * share(20.0, 100.0, f64::from(y) + 0.5, 5.0);
            holds(
                &picture,
                50 + moved.floor() as u32,
                y,
                over(yellow, alpha),
                2.0,
                &format!("frame {number}, the yellow box's column"),
            );
        }
        // The element blurred by 3 and brightened by 2: 200 beside 40 at x = 170, mixed and then doubled, stopped at white.
        // From x = 124, where the moved yellow box lays less than half a level.
        for x in 124..225 {
            let c = f64::from(x) + 0.5;
            let vertical = share(20.0, 100.0, 60.5, 3.0);
            let (left, right) = (share(130.0, 170.0, c, 3.0), share(170.0, 210.0, c, 3.0));
            let alpha = (left + right) * vertical;
            let grey = (2.0 * (200.0 * left + 40.0 * right) * vertical).min(255.0 * alpha);
            let wanted = [0, 1, 2].map(|channel| grey + BACKGROUND[channel] * (1.0 - alpha));
            if x < 220 {
                holds(&picture, x, 60, wanted, 2.0, &format!("frame {number}, the brightened seam"));
            }
        }
        // The element at opacity 0.5: red alone, blue alone and the overlap, the overlap as strong as blue alone.
        let half = 128.0 / 255.0;
        holds(&picture, 230, 60, over([255.0, 0.0, 0.0], half), 1.0, "red alone");
        holds(&picture, 300, 60, over([0.0, 0.0, 255.0], half), 1.0, "blue alone");
        holds(&picture, 265, 60, over([0.0, 0.0, 255.0], half), 1.0, "the overlap");
        assert_eq!(
            picture.pixel(265, 60),
            picture.pixel(300, 60),
            "frame {number}: the overlap is blue alone"
        );
    }
}
