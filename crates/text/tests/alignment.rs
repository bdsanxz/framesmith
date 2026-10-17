//! The lines of a paragraph lie at the left, the centre or the right of its width (`docs/spec/text.md`, section 6.2).

mod common;

use common::{fonts, style};
use text::{layout, Align, Block};

/// A paragraph of two lines of different widths: "ABCDEFGHIJ", which fills the width, and "KL", which is narrower.
fn two_lines_aligned(align: Align) -> Block {
    let mut fonts = fonts();
    let first = layout(&mut fonts, "ABCDEFGHIJ", &style(40.0), None).unwrap().width;
    let mut style = style(40.0);
    style.align = align;
    let block = layout(&mut fonts, "ABCDEFGHIJ KL", &style, Some(first + 1.0)).unwrap();
    assert_eq!(block.lines.len(), 2, "{:?}", block.lines);
    assert!(block.lines[0].width > block.lines[1].width, "{:?}", block.lines);
    block
}

#[test]
fn every_line_lies_at_the_left_of_the_width_when_aligned_left() {
    let block = two_lines_aligned(Align::Left);
    for line in &block.lines {
        assert!(line.left.abs() < 0.01, "{line:?}");
    }
}

#[test]
fn every_line_is_centred_in_the_width_when_aligned_to_the_centre() {
    let block = two_lines_aligned(Align::Center);
    for line in &block.lines {
        let want = (block.width - line.width) / 2.0;
        assert!((line.left - want).abs() < 0.01, "left {} is not {want}: {line:?}", line.left);
    }
}

#[test]
fn every_line_ends_at_the_right_of_the_width_when_aligned_right() {
    let block = two_lines_aligned(Align::Right);
    for line in &block.lines {
        assert!((line.left + line.width - block.width).abs() < 0.01, "{line:?} in {}", block.width);
    }
}
