//! The H of the test font at 96 pixels with a stroke of 2 pixels: its checked-in picture is held to the worked picture of
//! the three rectangles of its strokes, which `docs/spec/test-font.md` (section 3) and `docs/spec/text.md` (section 6.6, rules 6
//! and 10) give, drawn by `evidence/textgaps/edges_ref.py` with no product code. Its junctions and flat ends are worked by hand
//! in that script; the old picture had their overlaps counted twice, 45 levels too dark at (20, 45).

mod common;

use common::held::{drawn, within_levels_where_held, worked};
use common::{compare, expected};

const FIXTURE: &str = "text-stroke-edges";

#[test]
fn the_frame_equals_its_checked_in_picture_on_the_processor() {
    if let Err(difference) = compare(0, &drawn("text-stroke-edges-worked", FIXTURE, 0), &expected(FIXTURE, 0)) {
        panic!("{difference}");
    }
}

#[test]
fn the_frame_is_within_one_level_of_its_worked_picture() {
    let (worked, held) = worked(FIXTURE, 0);
    let checked = within_levels_where_held(&drawn("text-stroke-edges-worked", FIXTURE, 0), &worked, &held, 1)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(checked, 120 * 120);
}
