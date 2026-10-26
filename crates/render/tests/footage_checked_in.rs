//! The frames of the footage fixture, drawn on the processor, equal their checked-in pictures to the byte, and each
//! checked-in picture holds the values worked by hand from the composition and the colours its clips were made from
//! (`docs/spec/render-command.md` sections 4.7 and 9). The clips are checked in beside the composition; the example
//! `write_footage_clips` made them.

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::{Scene, Sources};

const FIXTURE: &str = "footage-on-screen";
/// 0 and 29 are the first and last frames; 9 and 10, 24 and 25 the edges of the window of 1 s to 2.5 s; 15 a middle; 19 and
/// 20 the last picture of the clip and the first frame that holds it.
const CHECKED: [u64; 9] = [0, 9, 10, 15, 19, 20, 24, 25, 29];

const CANVAS: [u8; 4] = [0x10, 0x20, 0x40, 255];
const OCHRE: [u8; 4] = [0xe0, 0xa0, 0x30, 255];
const RED: [u8; 3] = [220, 20, 20];
const BLUE: [u8; 3] = [20, 20, 220];

/// The colour picture `k` of the numbered clip was made from (`examples/write_footage_clips.rs`).
fn numbered(k: u64) -> [u8; 3] {
    [(k % 5 * 50 + 20) as u8, 128, (k / 5 * 100 + 20) as u8]
}

fn near(picture: &Picture, (x, y): (u32, u32), rgb: [u8; 3], by: u8) {
    let pixel = picture.pixel(x, y).unwrap();
    assert!(
        (0..3).all(|i| pixel[i].abs_diff(rgb[i]) <= by) && pixel[3] == 255,
        "({x}, {y}) is {pixel:?}, not within {by} of {rgb:?}"
    );
}

/// The values worked by hand for frame `number`. The numbered clip fills (4, 4) to (44, 28) in frames 10 to 24, showing
/// picture `number - 10`, held at its last, 9, from frame 19 on. The box fills (50, 6) to (90, 46) in ochre; the split clip,
/// 8 by 8 stretched to 32 by 16, fills (54, 10) to (86, 26), red on its left half and blue on its right. The encoder keeps
/// flat colours within 2 levels; the halves are read in their outer two source columns (54 to 62 and 78 to 86), since
/// next to where they meet the encoder rings by a few levels more.
fn holds_the_worked_values(number: u64, picture: &Picture) {
    for point in [(1, 1), (95, 53), (24, 40), (2, 16), (46, 16)] {
        assert_eq!(picture.pixel(point.0, point.1), Some(CANVAS), "frame {number} at {point:?}");
    }
    for point in [(50, 6), (89, 45), (52, 18), (88, 18), (70, 8), (70, 27)] {
        assert_eq!(picture.pixel(point.0, point.1), Some(OCHRE), "frame {number} at {point:?}");
    }
    for point in [(55, 13), (58, 18), (56, 22)] {
        near(picture, point, RED, 6);
    }
    for point in [(79, 13), (82, 18), (85, 22)] {
        near(picture, point, BLUE, 6);
    }
    let footage = [(4, 4), (24, 16), (43, 27)];
    if (10..=24).contains(&number) {
        let shown = (number - 10).min(9);
        for point in footage {
            near(picture, point, numbered(shown), 2);
        }
        assert_eq!(picture.pixel(3, 16), Some(CANVAS), "frame {number}: left of the footage");
        assert_eq!(picture.pixel(44, 16), Some(CANVAS), "frame {number}: right of the footage");
        assert_eq!(picture.pixel(24, 3), Some(CANVAS), "frame {number}: above the footage");
        assert_eq!(picture.pixel(24, 28), Some(CANVAS), "frame {number}: below the footage");
    } else {
        for point in footage {
            assert_eq!(picture.pixel(point.0, point.1), Some(CANVAS), "frame {number} at {point:?}: outside the window");
        }
    }
}

#[test]
fn the_checked_in_pictures_of_the_footage_fixture_hold_the_worked_values() {
    assert_eq!(numbers_with_a_picture(FIXTURE), CHECKED.to_vec(), "{FIXTURE}: the frames with a picture");
    for number in CHECKED {
        holds_the_worked_values(number, &expected(FIXTURE, number));
    }
}

#[test]
fn the_frames_of_the_footage_fixture_equal_their_checked_in_pictures() {
    let folder = project_copied_from_fixture("footage-checked-in", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut sources = Sources::open(&scene).unwrap();
    for number in CHECKED {
        let drawn = scene.picture_with(number, &mut CpuRenderer, &mut sources).unwrap();
        holds_the_worked_values(number, &drawn);
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{FIXTURE}: {difference}");
        }
    }
}
