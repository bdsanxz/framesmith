//! The motion-text fixture (HIIH at 24 pixels with a blue shadow of 3 pixels, blurred by 2, and a red outline of 1 pixel,
//! moved, scaled and turned over two seconds) is held to the worked pictures of `evidence/textgaps/motion_ref.py`: the spec's
//! tween, its transform (scale, rotate, move, about the box's top-left), its glyph rectangles and its outline. Frame 0 has no
//! transform, so its whole frame is held. The later frames hold the pixels where no shadow reaches under either reading of the
//! blur, the blur's placement in the transform being left to the follow-up order (#982), as the worked files' alpha marks them.

mod common;

use common::held::{drawn, within_levels_where_held, worked};

const FIXTURE: &str = "motion-text";

#[test]
fn the_held_pixels_of_each_frame_are_within_one_level_of_its_worked_picture() {
    for number in [0, 5, 10, 15, 19] {
        let (worked, held) = worked(FIXTURE, number);
        let drawn = drawn("motion-text-worked", FIXTURE, number);
        within_levels_where_held(&drawn, &worked, &held, 1).unwrap_or_else(|error| panic!("frame {number}: {error}"));
    }
}

#[test]
fn frame_zero_is_held_whole() {
    let (_, held) = worked(FIXTURE, 0);
    assert!(held.iter().all(|&h| h), "frame 0 has no transform, so every pixel is held");
}
