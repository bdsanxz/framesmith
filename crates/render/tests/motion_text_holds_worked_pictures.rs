//! The motion-text fixture (HIIH at 24 pixels with a blue shadow of 3 pixels, blurred by 2, and a red outline of 1 pixel,
//! moved, scaled and turned over two seconds) is held to the worked pictures of `evidence/textgaps/motion_ref.py`, every
//! pixel of frames 0, 5, 10, 15 and 19: the spec's tween, its transform (scale, rotate, move, about the box's top-left), its
//! glyph rectangles, its outline, and its shadow, offset in the text's own pixels, mapped, and blurred with the blur scaled by
//! the square root of the absolute determinant of the map (`docs/spec/render-command.md` section 4.5, rule 7).

mod common;

use common::held::{drawn, within_levels_where_held, worked};

const FIXTURE: &str = "motion-text";

#[test]
fn every_pixel_of_each_frame_is_within_one_level_of_its_worked_picture() {
    for number in [0, 5, 10, 15, 19] {
        let (worked, held) = worked(FIXTURE, number);
        assert!(held.iter().all(|&h| h), "frame {number}: every pixel is held");
        let drawn = drawn("motion-text-worked", FIXTURE, number);
        within_levels_where_held(&drawn, &worked, &held, 1).unwrap_or_else(|error| panic!("frame {number}: {error}"));
    }
}
