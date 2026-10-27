//! The glyph 8 of the test font at 250 pixels, filled with no outline: its fill is held to the worked picture of its
//! exact coverage, which `evidence/textgaps/fill_ref.py` computes from the contours of the test font flattened at the one
//! tolerance of the outlines (`docs/spec/text.md`, section 6.6, rule 10), with no product code. The old fill, left to the
//! rasteriser's own curve flattening, was up to 62 levels from it at the edge of the glyph.

mod common;

use common::held::{drawn, within_levels_where_held, worked};

const FIXTURE: &str = "text-fill-arcs";

#[test]
fn the_frame_is_within_one_level_of_its_worked_picture() {
    let (worked, held) = worked(FIXTURE, 0);
    let checked = within_levels_where_held(&drawn("text-fill-arcs-worked", FIXTURE, 0), &worked, &held, 1)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(checked, 300 * 300);
}
