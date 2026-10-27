//! The text HIIH at 96 pixels with a blue outline of 2 pixels over a black fill: no shadow, no transform, so the whole frame
//! is held to the worked picture (`evidence/textgaps/dressing_ref.py`), made from the strokes of the test font by the rules of
//! `docs/spec/test-font.md` (section 3, rules 1, 2 and 5) and `docs/spec/text.md` (section 6.6). The I has a bar at its top and
//! its bottom, with its advance of 540 units, as the generator draws it. The fill is laid first, then the outline over it.

mod common;

use common::held::{drawn, within_levels_where_held, worked};
use common::{compare, expected};

const FIXTURE: &str = "text-dressing-outline";

#[test]
fn the_frames_equal_their_checked_in_pictures_on_the_processor() {
    for number in [0, 9] {
        if let Err(difference) = compare(
            number,
            &drawn("text-dressing-outline-worked", FIXTURE, number),
            &expected(FIXTURE, number),
        ) {
            panic!("{difference}");
        }
    }
}

#[test]
fn the_frame_is_within_one_level_of_its_worked_picture() {
    let (worked, held) = worked(FIXTURE, 0);
    let checked = within_levels_where_held(&drawn("text-dressing-outline-worked", FIXTURE, 0), &worked, &held, 1)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(checked, 240 * 120);
}
