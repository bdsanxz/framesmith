//! The checked-in pictures of the frame-order fixture hold, pixel by pixel, to the worked pictures of its frames (the
//! `worked` folder): the boxes, their fades, their clips and the text that moves, grows and fades with them, each within one
//! level in each colour. The worked pictures are the scene model of `evidence/textgaps/model.py`, read from the composition,
//! with each glyph's contours covered by the accumulating scan of `docs/spec/text.md` (section 6.6, rule 6): the share of each
//! contour, added, capped at one.

mod common;

use common::held::{drawn, within_levels_where_held, worked};
use common::numbers_with_a_picture;

const FIXTURE: &str = "frame-order";

#[test]
fn every_frame_of_the_order_proof_is_within_one_level_of_its_worked_picture_at_every_pixel() {
    assert_eq!(
        numbers_with_a_picture(FIXTURE),
        [0, 20, 30, 36, 45, 59],
        "the frames with a picture"
    );
    for number in [0, 20, 30, 36, 45, 59] {
        let (worked, held) = worked(FIXTURE, number);
        assert!(held.iter().all(|&h| h), "frame {number}: every pixel is held");
        let checked = within_levels_where_held(&drawn("frame-order-worked", FIXTURE, number), &worked, &held, 1)
            .unwrap_or_else(|error| panic!("frame {number}: {error}"));
        assert_eq!(checked, 160 * 90, "frame {number}: every pixel of the frame is checked");
    }
}
