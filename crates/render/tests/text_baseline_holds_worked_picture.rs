//! The whole picture of `text-baseline-whole-pixel` (two capitals at 30 and 25 pixels, whose baselines are rounded to rows 26 and
//! 21) equals its checked-in picture on the processor, and every pixel is within one level of its worked picture. The worked
//! picture is the text model of `evidence/textgaps/text_model.py` drawn from a display list written by hand from the spec: the
//! glyph `H` at the left edge of each text, its baseline at 0.85 times the size below the top, rounded half up (25.5 to 26,
//! 21.25 to 21), as `docs/spec/text.md` (section 4) states. No product code makes that picture.

mod common;

use common::held::{drawn, within_levels_where_held, worked};
use common::{compare, expected};

const FIXTURE: &str = "text-baseline-whole-pixel";

#[test]
fn the_frame_equals_its_checked_in_picture_on_the_processor() {
    if let Err(difference) = compare(0, &drawn("text-baseline-whole-pixel-worked", FIXTURE, 0), &expected(FIXTURE, 0)) {
        panic!("{difference}");
    }
}

#[test]
fn the_frame_is_within_one_level_of_its_worked_picture() {
    let (worked, held) = worked(FIXTURE, 0);
    let checked = within_levels_where_held(&drawn("text-baseline-whole-pixel-worked", FIXTURE, 0), &worked, &held, 1)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(checked, 120 * 60);
}
