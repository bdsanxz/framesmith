//! A paragraph is broken into lines within its width, at the places the line-breaking rules allow, and no line is
//! wider than the width when each of its words fits it (`docs/spec/text.md`, sections 5.2 and 5.3).

mod common;

use common::{fonts, style};
use text::layout;

#[test]
fn no_line_of_a_wrapped_paragraph_is_wider_than_its_width() {
    let mut fonts = fonts();
    let words = "ABCDE FGHIJ KLMNO PQRST UVWXY";
    let whole = layout(&mut fonts, words, &style(40.0), None).unwrap().width;
    let width = whole * 0.6;
    let block = layout(&mut fonts, words, &style(40.0), Some(width)).unwrap();
    assert!(block.lines.len() > 1, "the paragraph breaks into lines: {:?}", block.lines);
    assert_eq!(block.width, width);
    for (number, line) in block.lines.iter().enumerate() {
        assert!(
            line.width <= width + 0.01,
            "line {number} is {} wide, the width is {width}",
            line.width
        );
    }
}

#[test]
fn a_line_ends_at_the_space_between_two_words_when_the_second_does_not_fit() {
    let mut fonts = fonts();
    let first = layout(&mut fonts, "ABCDE", &style(40.0), None).unwrap().width;
    let block = layout(&mut fonts, "ABCDE FGHIJ", &style(40.0), Some(first + 1.0)).unwrap();
    assert_eq!(block.lines.len(), 2, "{:?}", block.lines);
    assert!(
        (block.lines[0].width - first).abs() < 0.01,
        "the first line is the first word: {:?}",
        block.lines
    );
}

#[test]
fn a_line_ends_after_a_hyphen_when_the_next_word_does_not_fit() {
    let mut fonts = fonts();
    let before = layout(&mut fonts, "ABCD-", &style(40.0), None).unwrap().width;
    let width = before + 1.0;
    let block = layout(&mut fonts, "ABCD-EFGH", &style(40.0), Some(width)).unwrap();
    assert_eq!(block.lines.len(), 2, "{:?}", block.lines);
    for (number, line) in block.lines.iter().enumerate() {
        assert!(
            line.width <= width + 0.01,
            "line {number} is {} wide, the width is {width}",
            line.width
        );
    }
}

#[test]
fn a_width_of_zero_gives_each_word_a_line_of_its_own() {
    let mut fonts = fonts();
    let block = layout(&mut fonts, "AB CD EF", &style(40.0), Some(0.0)).unwrap();
    assert_eq!(block.lines.len(), 3, "{:?}", block.lines);
}

#[test]
fn a_text_without_a_width_is_one_line_as_wide_as_its_text() {
    let mut fonts = fonts();
    let block = layout(&mut fonts, "AB CD EF", &style(40.0), None).unwrap();
    assert_eq!(block.lines.len(), 1, "{:?}", block.lines);
    assert_eq!(block.width, block.lines[0].width);
}
