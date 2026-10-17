//! The weight of a text chooses its face of the family, by the matching rule of `docs/spec/text.md`, section 3.2,
//! and the two faces of the test font take the same advances.

mod common;

use common::{font_file, fonts, style};
use text::layout;

fn face_drawn_for(weight: f32) -> Vec<u8> {
    let mut fonts = fonts();
    let mut weighted = style(40.0);
    weighted.weight = weight;
    let block = layout(&mut fonts, "AB", &weighted, None).unwrap();
    block.runs[0].font.as_ref().clone()
}

#[test]
fn a_weight_of_400_draws_the_regular_face_and_700_the_bold_face() {
    let regular = font_file("framesmith-test-stroke-400.ttf");
    let bold = font_file("framesmith-test-stroke-700.ttf");
    assert_eq!(face_drawn_for(400.0), regular);
    assert_eq!(face_drawn_for(700.0), bold);
}

#[test]
fn a_weight_between_the_two_faces_draws_the_face_that_the_matching_rule_gives() {
    let regular = font_file("framesmith-test-stroke-400.ttf");
    let bold = font_file("framesmith-test-stroke-700.ttf");
    assert_eq!(face_drawn_for(500.0), regular, "500 draws the Regular face");
    assert_eq!(face_drawn_for(600.0), bold, "600 draws the Bold face");
}

#[test]
fn the_regular_and_the_bold_face_give_a_line_the_same_width() {
    let mut fonts = fonts();
    let regular = layout(&mut fonts, "ABCDE", &style(40.0), None).unwrap().width;
    let mut bold = style(40.0);
    bold.weight = 700.0;
    let heavy = layout(&mut fonts, "ABCDE", &bold, None).unwrap().width;
    assert!((regular - heavy).abs() < 0.01, "{regular} and {heavy}");
}
