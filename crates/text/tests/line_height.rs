//! The baselines of a paragraph are one line height apart: a multiple of the font size, a number of pixels, or the
//! font's own height (`docs/spec/text.md`, section 4).

mod common;

use common::{fonts, style};
use text::{layout, Block, LineHeight};

/// Two lines, "AB" and "CD", at 40 pixels: the width holds "AB" but not "AB CD".
fn two_lines(line_height: LineHeight) -> Block {
    let mut fonts = fonts();
    let mut style = style(40.0);
    style.line_height = line_height;
    let first = layout(&mut fonts, "AB", &style, None).unwrap().width;
    let block = layout(&mut fonts, "AB CD", &style, Some(first + 1.0)).unwrap();
    assert_eq!(block.lines.len(), 2, "{:?}", block.lines);
    block
}

fn distance_between_the_baselines(block: &Block) -> f32 {
    block.lines[1].baseline - block.lines[0].baseline
}

#[test]
fn a_multiple_of_the_font_size_gives_that_distance_between_the_baselines() {
    // 1.5 times 40 pixels.
    let block = two_lines(LineHeight::Multiple(1.5));
    assert!((distance_between_the_baselines(&block) - 60.0).abs() < 0.01, "{:?}", block.lines);
}

#[test]
fn a_number_of_pixels_gives_that_distance_between_the_baselines() {
    let block = two_lines(LineHeight::Pixels(50.0));
    assert!((distance_between_the_baselines(&block) - 50.0).abs() < 0.01, "{:?}", block.lines);
}

#[test]
fn the_normal_line_height_is_the_font_size_times_one_point_one() {
    // The test font's ascender is 850 and its descender 250, with no line gap, so `normal` is 1.1 times the size.
    let block = two_lines(LineHeight::Normal);
    assert!((distance_between_the_baselines(&block) - 44.0).abs() < 0.01, "{:?}", block.lines);
}

#[test]
fn the_height_of_a_paragraph_is_the_sum_of_its_line_heights() {
    let block = two_lines(LineHeight::Pixels(50.0));
    assert!((block.height - 100.0).abs() < 0.01, "height {}", block.height);
}
