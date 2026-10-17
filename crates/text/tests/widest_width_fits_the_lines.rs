//! A paragraph with a widest width is broken so that no line is wider than it, and its block is as wide as its
//! widest line, so that a paragraph shorter than the widest width keeps its own width (`docs/spec/text.md`,
//! section 5.3).

mod common;

use common::{fonts, style};
use text::{layout, layout_fitted};

#[test]
fn no_line_of_a_paragraph_with_a_widest_width_is_wider_than_it() {
    let mut fonts = fonts();
    let words = "ABCDE FGHIJ KLMNO PQRST UVWXY";
    let whole = layout(&mut fonts, words, &style(40.0), None).unwrap().width;
    let max = whole * 0.6;
    let block = layout_fitted(&mut fonts, words, &style(40.0), max).unwrap();
    assert!(block.lines.len() > 1, "the paragraph breaks into lines: {:?}", block.lines);
    for (number, line) in block.lines.iter().enumerate() {
        assert!(
            line.width <= max + 0.01,
            "line {number} is {} wide, the widest width is {max}",
            line.width
        );
    }
}

#[test]
fn the_block_is_as_wide_as_its_widest_line_and_not_as_the_widest_width() {
    let mut fonts = fonts();
    let words = "ABCDE FGHIJ KLMNO PQRST UVWXY";
    let whole = layout(&mut fonts, words, &style(40.0), None).unwrap().width;
    let max = whole * 0.6;
    let block = layout_fitted(&mut fonts, words, &style(40.0), max).unwrap();
    let widest = block.lines.iter().map(|line| line.width).fold(0.0_f32, f32::max);
    assert_eq!(block.width, widest);
    assert!(block.width <= max + 0.01);
}

#[test]
fn a_paragraph_shorter_than_the_widest_width_keeps_its_own_width() {
    let mut fonts = fonts();
    let own = layout(&mut fonts, "AB CD", &style(40.0), None).unwrap();
    let fitted = layout_fitted(&mut fonts, "AB CD", &style(40.0), 10_000.0).unwrap();
    assert_eq!(fitted.lines.len(), 1, "{:?}", fitted.lines);
    assert_eq!(fitted.width, own.width);
}

#[test]
fn a_word_wider_than_the_widest_width_is_kept_whole_on_its_own_line() {
    let mut fonts = fonts();
    let whole = layout(&mut fonts, "ABCDEFGHIJ", &style(40.0), None).unwrap().width;
    let block = layout_fitted(&mut fonts, "ABCDEFGHIJ", &style(40.0), whole / 4.0).unwrap();
    assert_eq!(block.lines.len(), 1, "{:?}", block.lines);
    assert!(block.lines[0].width > whole / 4.0);
}
