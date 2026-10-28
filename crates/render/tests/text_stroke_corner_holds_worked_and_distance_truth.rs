//! The corner glyph of the test font (U+E000) with a 16-pixel stroke: the frame is held to the worked picture
//! (`evidence/textgaps/corner_ref.py`, from `docs/spec/text.md`, section 6.6, rules 6 and 10), and the sharp inner corner
//! of its third triangle lies in the stroke's fold, where one side is left out. The two pixels of that fold are also held to
//! the distance truth: the points of the frame within 8 pixels of the outline, counted on a 48 by 48 grid of each pixel. At
//! (193, 160) the truth is 0 and at (93, 117) it is 41, the values the stroke must give on both sides of the fold.

mod common;

use common::held::{drawn, within_levels_where_held, worked};

const FIXTURE: &str = "text-join-corner";

#[test]
fn the_frame_is_within_one_level_of_its_worked_picture() {
    let (worked, held) = worked(FIXTURE, 0);
    let checked = within_levels_where_held(&drawn("text-join-corner-worked", FIXTURE, 0), &worked, &held, 1)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(checked, 300 * 240);
}

#[test]
fn the_folded_corner_lies_at_the_distance_truth() {
    let picture = drawn("text-join-corner-fold", FIXTURE, 0);
    let red = |x: u32, y: u32| picture.pixel(x, y).unwrap()[0];
    assert_eq!(
        red(193, 160),
        0,
        "the sharp inner corner of the third triangle is inked at its distance truth"
    );
    assert!(
        red(93, 117).abs_diff(41) <= 1,
        "the stroke beside the fold is 41 at its distance truth, not {}",
        red(93, 117)
    );
}
