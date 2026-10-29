//! The pictures of a blur or a brightness that a tween or a set moves are drawn on the processor to the checked-in pictures,
//! and the checked-in pictures hold the values worked out in `fixtures/blur-brightness-worked-values.md`, independently of
//! the renderer: a picture the product made wrong, and that was checked in as it was made, fails here. A pixel that is the
//! background or the ink to the level is exact; any other is within one level per channel.

mod common;

use common::{compare, expected, fixtures, numbers_with_a_picture, picture_name, png, project_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;
use std::fs;

const BACKGROUND: [u8; 3] = [16, 32, 64];
const INK: [u8; 3] = [224, 160, 48];

/// The chosen frames of each fixture, the frames of `fixtures/blur-brightness-worked-values.md`.
const CHOSEN: [(&str, &[u64]); 5] = [
    ("blur-tween-linear", &[0, 15, 29]),
    ("blur-set-at-an-instant", &[0, 14, 15, 29]),
    ("blur-tween-held-at-zero", &[0, 11, 17, 29, 30]),
    ("brightness-tween-linear", &[0, 15, 29]),
    ("brightness-tween-held-at-zero", &[0, 11, 17, 29, 30]),
];

fn scene_of(fixture: &str) -> Scene {
    let folder = project_from_fixture("blur-and-brightness-tweens", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// Asserts that each pixel `(x, y, colour)` of frame `number` of `fixture`, read from its checked-in picture, lies within
/// `within` levels per channel of `colour`.
fn holds(fixture: &str, number: u64, want: &[(u32, u32, [u8; 3])], within: u8) {
    let picture = expected(fixture, number);
    for &(x, y, colour) in want {
        let [r, g, b, _] = picture.pixel(x, y).unwrap();
        let drawn = [r, g, b];
        let off = (0..3).any(|channel| drawn[channel].abs_diff(colour[channel]) > within);
        assert!(!off, "{fixture}, frame {number}, ({x}, {y}): drawn {drawn:?}, worked {colour:?}");
    }
}

/// Row 32, the middle row of the 64 by 64 frame, at the given columns.
fn row(columns: &[(u32, [u8; 3])]) -> Vec<(u32, u32, [u8; 3])> {
    columns.iter().map(|&(x, colour)| (x, 32, colour)).collect()
}

/// The edges of the ink at the start of a blur of 20: row 32, columns 0 to 40 in steps of 4 (`fixtures/blur-brightness-worked-values.md`).
fn blur_of_20() -> Vec<(u32, u32, [u8; 3])> {
    row(&[
        (0, [56, 57, 61]),
        (4, [67, 64, 60]),
        (8, [80, 71, 59]),
        (12, [93, 79, 58]),
        (16, [106, 87, 57]),
        (20, [118, 95, 56]),
        (24, [128, 101, 55]),
        (28, [136, 106, 55]),
        (32, [141, 109, 54]),
        (36, [142, 110, 54]),
        (40, [140, 108, 54]),
    ])
}

/// The edge of a blur of 10: row 32, columns 8 to 24.
fn blur_of_10() -> Vec<(u32, u32, [u8; 3])> {
    row(&[
        (8, [63, 61, 60]),
        (9, [70, 65, 60]),
        (10, [76, 69, 59]),
        (11, [84, 74, 59]),
        (12, [91, 78, 58]),
        (13, [99, 83, 58]),
        (14, [107, 88, 57]),
        (15, [116, 93, 56]),
        (16, [124, 98, 56]),
        (17, [132, 104, 55]),
        (18, [140, 109, 54]),
        (19, [148, 113, 54]),
        (20, [156, 118, 53]),
        (21, [163, 123, 53]),
        (22, [170, 127, 52]),
        (23, [177, 131, 52]),
        (24, [183, 134, 51]),
    ])
}

/// The edge of a blur of 4: row 32, columns 8 to 24.
fn blur_of_4() -> Vec<(u32, u32, [u8; 3])> {
    row(&[
        (8, [22, 36, 64]),
        (9, [27, 39, 63]),
        (10, [34, 43, 63]),
        (11, [43, 49, 62]),
        (12, [56, 56, 61]),
        (13, [71, 66, 60]),
        (14, [90, 77, 58]),
        (15, [110, 90, 57]),
        (16, [130, 102, 55]),
        (17, [150, 115, 54]),
        (18, [169, 126, 52]),
        (19, [184, 136, 51]),
        (20, [197, 143, 50]),
        (21, [206, 149, 49]),
        (22, [213, 153, 49]),
        (23, [218, 156, 48]),
        (24, [221, 158, 48]),
    ])
}

/// The edge of a blur of 2/3, at the end of a blur of 20 to 0: row 32, columns 13 to 24.
fn blur_of_two_thirds() -> Vec<(u32, u32, [u8; 3])> {
    row(&[
        (13, BACKGROUND),
        (14, [19, 34, 64]),
        (15, [63, 61, 60]),
        (16, [177, 131, 52]),
        (17, [221, 158, 48]),
        (18, INK),
        (19, INK),
        (24, INK),
    ])
}

/// A sharp edge, row 32: the background left of the ink and the ink from its first column, and the ink to its last.
fn sharp() -> Vec<(u32, u32, [u8; 3])> {
    row(&[(15, BACKGROUND), (16, INK), (55, INK), (56, BACKGROUND)])
}

/// Two probes inside the ink and the background far from it, every channel of which a brightness multiplies alike.
fn probes(ink: [u8; 3], background: [u8; 3]) -> Vec<(u32, u32, [u8; 3])> {
    vec![(30, 10, ink), (40, 50, ink), (2, 32, background), (62, 2, background)]
}

#[test]
fn the_chosen_frames_of_each_fixture_have_a_checked_in_picture() {
    for (fixture, numbers) in CHOSEN {
        assert_eq!(numbers_with_a_picture(fixture), numbers, "{fixture}");
    }
}

#[test]
fn the_frames_drawn_for_each_fixture_equal_their_checked_in_pictures() {
    for (fixture, numbers) in CHOSEN {
        let scene = scene_of(fixture);
        for &number in numbers {
            let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
            if let Err(difference) = compare(number, &drawn, &expected(fixture, number)) {
                panic!("{fixture}: {difference}");
            }
        }
    }
}

#[test]
fn a_blur_tween_from_20_has_the_worked_edges_at_its_start_and_its_middle_and_end() {
    let fixture = "blur-tween-linear";
    holds(fixture, 0, &blur_of_20(), 1);
    holds(fixture, 15, &blur_of_10(), 1);
    holds(fixture, 29, &blur_of_two_thirds(), 1);
}

#[test]
fn a_blur_set_at_an_instant_is_sharp_before_its_time_and_blurred_from_it() {
    let fixture = "blur-set-at-an-instant";
    holds(fixture, 0, &sharp(), 0);
    holds(fixture, 14, &sharp(), 0);
    holds(fixture, 15, &blur_of_10(), 1);
    holds(fixture, 29, &blur_of_10(), 1);
}

#[test]
fn a_blur_tween_with_an_overshoot_is_held_at_zero_where_its_value_is_below_zero() {
    let fixture = "blur-tween-held-at-zero";
    holds(fixture, 0, &blur_of_4(), 1);
    // Frame 11: the eased value is 0.99599, so the blur is 0.016, which is sharp to the level.
    holds(fixture, 11, &sharp(), 0);
    // Frame 17: the value is -0.398 and is held at 0. Taken as its absolute value it would be blurred here, with x 15 at
    // (38, 45, 62) and x 16 at (202, 147, 50), so this frame tells the two apart.
    holds(fixture, 17, &sharp(), 0);
    holds(fixture, 29, &sharp(), 0);
    holds(fixture, 30, &sharp(), 0);
}

#[test]
fn a_brightness_tween_halves_each_colour_at_its_middle() {
    let fixture = "brightness-tween-linear";
    holds(fixture, 0, &probes(INK, BACKGROUND), 0);
    holds(fixture, 15, &probes([112, 80, 24], BACKGROUND), 1);
    holds(fixture, 29, &probes([7, 5, 2], BACKGROUND), 1);
}

#[test]
fn a_brightness_tween_with_an_overshoot_is_held_at_zero_where_its_value_is_below_zero() {
    let fixture = "brightness-tween-held-at-zero";
    holds(fixture, 0, &probes(INK, BACKGROUND), 0);
    holds(fixture, 11, &probes([1, 1, 0], BACKGROUND), 0);
    // Frame 17: the brightness is -0.0995, held at 0, so the ink is black; taken as its absolute value it would be (22, 16, 5).
    holds(fixture, 17, &probes([0, 0, 0], BACKGROUND), 0);
    holds(fixture, 29, &probes([0, 0, 0], BACKGROUND), 0);
    holds(fixture, 30, &probes([0, 0, 0], BACKGROUND), 0);
}

/// Writes the checked-in pictures of the blur and brightness fixtures from the processor. Ignored: it changes the checked-in
/// files, so it runs by hand with `--include-ignored`, and the pictures it writes are then held to the worked values above.
#[test]
#[ignore = "writes the checked-in pictures of the blur and brightness fixtures: run by hand with --include-ignored"]
fn write_the_checked_in_pictures_of_the_blur_and_brightness_fixtures() {
    for (fixture, numbers) in CHOSEN {
        let scene = scene_of(fixture);
        let folder = fixtures().join(fixture).join("expected");
        fs::create_dir_all(&folder).unwrap();
        for &number in numbers {
            let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
            let bytes = png::encode(drawn.width(), drawn.height(), drawn.pixels());
            fs::write(folder.join(picture_name(number)), bytes).unwrap();
        }
    }
}
