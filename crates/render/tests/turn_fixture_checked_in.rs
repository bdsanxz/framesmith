//! The frames of the turn fixture equal their checked-in pictures to the byte on the processor
//! (`docs/spec/render-command.md`, section 4.5, rule 10; `PLAN.md`, section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

#[test]
fn the_frames_of_squares_turned_in_space_edge_on_and_mirrored_equal_their_checked_in_pictures() {
    let fixture = "turn-in-space";
    let folder = project_copied_from_fixture("turn-fixture-checked-in", fixture, fixture);
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(fixture);
    assert_eq!(numbers, [0, 15, 30, 45], "the chosen frames have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
            panic!("{fixture}: {difference}");
        }
    }
}

/// The share of the pixel covered by a mark, from its red channel on white: 255 for white, `mark` inside the mark.
fn covered(picture: &draw::Picture, x: u32, y: u32, mark: f64) -> f64 {
    ((255.0 - f64::from(picture.pixel(x, y).unwrap()[0])) / (255.0 - mark)).clamp(0.0, 1.0)
}

/// Where rule 10 draws `(u, v)` from the origin `origin`, turned `a` about the level line then `b` about the upright line.
fn worked(origin: (f64, f64), (u, v): (f64, f64), a: f64, b: f64, perspective: f64) -> (f64, f64) {
    let ((sin_a, cos_a), (sin_b, cos_b)) = (a.to_radians().sin_cos(), b.to_radians().sin_cos());
    let (x, y, z) = (u * cos_b + v * sin_a * sin_b, v * cos_a, -u * sin_b + v * sin_a * cos_b);
    let scale = perspective / (perspective - z);
    (origin.0 + x * scale, origin.1 + y * scale)
}

#[test]
fn the_checked_in_pictures_hold_the_worked_shapes_of_the_turns() {
    let first = expected("turn-in-space", 0);
    // The left square: origin (70, 100), turned 60 degrees about the upright line, its edges at the worked places.
    let (left, right) = (
        worked((70.0, 100.0), (-50.0, -50.0), 0.0, 60.0, 400.0),
        worked((70.0, 100.0), (50.0, -50.0), 0.0, 60.0, 400.0),
    );
    for x in (left.0.ceil() as u32 + 1)..(right.0.floor() as u32 - 1) {
        let at = f64::from(x) + 0.5;
        let top = left.1 + (right.1 - left.1) * (at - left.0) / (right.0 - left.0);
        let wanted = 2.0 * (100.0 - top);
        let found: f64 = (0..200).map(|y| covered(&first, x, y, 32.0)).sum();
        assert!((found - wanted).abs() <= 1.0, "left square, column {x}: {found} against {wanted}");
    }
    // The middle square: origin (200, 100), turned 45 degrees about the level line, its rows the worked widths.
    let (top, bottom) = (
        worked((200.0, 100.0), (-50.0, -50.0), 45.0, 0.0, 400.0),
        worked((200.0, 100.0), (-50.0, 50.0), 45.0, 0.0, 400.0),
    );
    for y in (top.1.ceil() as u32 + 1)..(bottom.1.floor() as u32 - 1) {
        let at = f64::from(y) + 0.5;
        let half = 200.0 - (top.0 + (bottom.0 - top.0) * (at - top.1) / (bottom.1 - top.1));
        let found: f64 = (135..265).map(|x| covered(&first, x, y, 32.0)).sum();
        assert!(
            (found - 2.0 * half).abs() <= 1.0,
            "middle square, row {y}: {found} against {}",
            2.0 * half
        );
    }
    // The right square is flat at frame 0, at its place, with the red mark at its left.
    assert_eq!(first.pixel(330, 100).unwrap()[..3], [0x20, 0xa0, 0x40]);
    assert_eq!(first.pixel(290, 100).unwrap()[..3], [0xd0, 0x30, 0x20]);
    // At frame 30 it is edge-on: nothing of it is drawn.
    let edge_on = expected("turn-in-space", 30);
    for y in 0..200 {
        for x in 270..400 {
            assert_eq!(edge_on.pixel(x, y).unwrap()[..3], [255, 255, 255], "({x}, {y}) at frame 30");
        }
    }
    // At frame 45 it is past 90 degrees: mirrored, the red mark on the right of the origin's upright line.
    let back = expected("turn-in-space", 45);
    let red: Vec<u32> = (270..400)
        .filter(|&x| back.pixel(x, 100).unwrap()[..3] == [0xd0, 0x30, 0x20])
        .collect();
    assert!(!red.is_empty() && red.iter().all(|&x| x > 330), "{red:?}");
}
