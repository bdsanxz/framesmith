//! The frames of the shape fixtures equal their checked-in pictures to the byte on the processor, and those pictures hold the
//! values worked out from `docs/spec/shape.md` (`fixtures/shape-worked-values.md`): a pixel the outline wholly covers or
//! leaves is exact, and a pixel it partly covers is within one level of its worked coverage.

mod common;

use common::{compare, count_of, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const CHOSEN: [u64; 1] = [0];
const INK: [u8; 4] = [224, 160, 48, 255];
const BACKGROUND: [u8; 4] = [16, 32, 64, 255];
const ONE_LEVEL: i32 = 1;

fn drawn_frames_equal_the_checked_in_pictures(fixture: &str) {
    let folder = project_copied_from_fixture("shape-fixtures-checked-in", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let numbers = numbers_with_a_picture(fixture);
    assert_eq!(numbers, CHOSEN, "{fixture}: the chosen frames have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
            panic!("{fixture}: {difference}");
        }
    }
}

/// Each probe `(x, y, worked, partly)` of the checked-in picture: exact, or within one level when partly covered.
fn hold(fixture: &str, picture: &Picture, probes: &[(u32, u32, [u8; 4], bool)]) {
    for &(x, y, worked, partly) in probes {
        let got = picture.pixel(x, y).unwrap();
        if partly {
            let worst = (0..3).map(|i| (i32::from(got[i]) - i32::from(worked[i])).abs()).max().unwrap();
            assert!(worst <= ONE_LEVEL, "{fixture}: ({x}, {y}) is {got:?}, worked {worked:?}");
        } else {
            assert_eq!(got, worked, "{fixture}: ({x}, {y})");
        }
    }
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_frames_of_the_shape_corners_fixture_equal_their_checked_in_picture() {
    drawn_frames_equal_the_checked_in_pictures("shape-corners");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_checked_in_picture_of_the_shape_corners_fixture_holds_the_worked_corners() {
    let picture = expected("shape-corners", 0);
    hold(
        "shape-corners",
        &picture,
        &[
            (8, 8, INK, false),
            (55, 8, INK, false),
            (8, 55, INK, false),
            (55, 55, INK, false),
            (32, 32, INK, false),
            (20, 40, INK, false),
            (7, 7, BACKGROUND, false),
            (56, 7, BACKGROUND, false),
            (7, 56, BACKGROUND, false),
            (56, 56, BACKGROUND, false),
            (7, 20, BACKGROUND, false),
            (20, 7, BACKGROUND, false),
            (56, 40, BACKGROUND, false),
            (40, 56, BACKGROUND, false),
        ],
    );
    assert_eq!(count_of(&picture, INK), 2304, "the square is 48 by 48 pixels");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_frames_of_the_shape_fill_and_stroke_fixture_equal_their_checked_in_picture() {
    drawn_frames_equal_the_checked_in_pictures("shape-fill-and-stroke");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_checked_in_picture_of_the_shape_fill_and_stroke_fixture_holds_the_worked_fill_and_stroke() {
    let picture = expected("shape-fill-and-stroke", 0);
    hold(
        "shape-fill-and-stroke",
        &picture,
        &[
            (4, 4, INK, false),
            (27, 27, INK, false),
            (15, 15, INK, false),
            (3, 3, BACKGROUND, false),
            (28, 28, BACKGROUND, false),
            (3, 15, BACKGROUND, false),
            (15, 28, BACKGROUND, false),
            (34, 40, INK, false),
            (37, 40, INK, false),
            (58, 40, INK, false),
            (61, 40, INK, false),
            (45, 34, INK, false),
            (45, 37, INK, false),
            (45, 58, INK, false),
            (45, 61, INK, false),
            (34, 34, INK, false),
            (61, 61, INK, false),
            (33, 40, BACKGROUND, false),
            (38, 40, BACKGROUND, false),
            (57, 40, BACKGROUND, false),
            (62, 40, BACKGROUND, false),
            (45, 33, BACKGROUND, false),
            (45, 38, BACKGROUND, false),
            (45, 57, BACKGROUND, false),
            (45, 62, BACKGROUND, false),
            (38, 38, BACKGROUND, false),
            (57, 57, BACKGROUND, false),
        ],
    );
    assert_eq!(count_of(&picture, INK), 960, "the fill is 576 pixels and the stroke 384");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_frames_of_the_shape_view_box_scaled_fixture_equal_their_checked_in_picture() {
    drawn_frames_equal_the_checked_in_pictures("shape-view-box-scaled");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_checked_in_picture_of_the_shape_view_box_scaled_fixture_holds_the_worked_square() {
    let picture = expected("shape-view-box-scaled", 0);
    hold(
        "shape-view-box-scaled",
        &picture,
        &[
            (32, 32, INK, false),
            (55, 55, INK, false),
            (40, 45, INK, false),
            (31, 31, BACKGROUND, false),
            (56, 56, BACKGROUND, false),
            (31, 40, BACKGROUND, false),
            (40, 56, BACKGROUND, false),
            (8, 8, BACKGROUND, false),
            (20, 20, BACKGROUND, false),
        ],
    );
    assert_eq!(count_of(&picture, INK), 576, "the square is 24 by 24 pixels");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_frames_of_the_shape_dashes_fixture_equal_their_checked_in_picture() {
    drawn_frames_equal_the_checked_in_pictures("shape-dashes");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_checked_in_picture_of_the_shape_dashes_fixture_holds_the_worked_runs_and_caps() {
    let cap = [206, 149, 49, 255];
    hold(
        "shape-dashes",
        &expected("shape-dashes", 0),
        &[
            (6, 15, cap, true),
            (57, 15, cap, true),
            (7, 15, INK, false),
            (56, 15, INK, false),
            (5, 16, BACKGROUND, false),
            (58, 16, BACKGROUND, false),
            (8, 14, INK, false),
            (8, 17, INK, false),
            (12, 14, INK, false),
            (12, 17, INK, false),
            (17, 15, INK, false),
            (18, 15, BACKGROUND, false),
            (22, 15, BACKGROUND, false),
            (23, 15, INK, false),
            (32, 15, INK, false),
            (33, 15, BACKGROUND, false),
            (37, 15, BACKGROUND, false),
            (38, 15, INK, false),
            (47, 15, INK, false),
            (48, 15, BACKGROUND, false),
            (52, 15, BACKGROUND, false),
            (53, 15, INK, false),
            (55, 15, INK, false),
            (12, 13, BACKGROUND, false),
            (12, 18, BACKGROUND, false),
            (30, 30, BACKGROUND, false),
        ],
    );
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_frames_of_the_shape_curve_fixture_equal_their_checked_in_picture() {
    drawn_frames_equal_the_checked_in_pictures("shape-curve");
}

#[test]
#[ignore = "waits for the shape notation (Bima's T092 hand-over) and the fixtures it draws"]
fn the_checked_in_picture_of_the_shape_curve_fixture_holds_the_worked_circle() {
    let edge = [223, 159, 48, 255];
    hold(
        "shape-curve",
        &expected("shape-curve", 0),
        &[
            (11, 50, INK, false),
            (50, 11, INK, false),
            (50, 50, INK, false),
            (9, 50, BACKGROUND, false),
            (90, 50, BACKGROUND, false),
            (50, 9, BACKGROUND, false),
            (50, 90, BACKGROUND, false),
            (15, 15, BACKGROUND, false),
            (10, 40, BACKGROUND, false),
            (10, 60, BACKGROUND, false),
            (40, 10, BACKGROUND, false),
            (60, 10, BACKGROUND, false),
            (89, 50, edge, true),
            (50, 89, edge, true),
            (10, 50, edge, true),
            (50, 10, edge, true),
        ],
    );
}
