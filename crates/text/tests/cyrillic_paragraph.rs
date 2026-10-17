//! A Cyrillic paragraph is drawn with its letters in reading order, and it breaks at the spaces inside its width
//! (`docs/spec/text.md`, sections 12.1 to 12.3). The paragraph is drawn with the machine's font, so the test is
//! compiled for macOS only: that font is a file of the machine, which the other systems do not have.

#![cfg(target_os = "macos")]

mod common;

use common::scripts::{glyph_names, line_text, mac_fonts, mac_style};
use text::layout;

const TEXT: &str = "Привет, мир! Это текст на русском языке.";

#[test]
fn cyrillic_letters_are_drawn_in_reading_order() {
    let mut fonts = mac_fonts();
    let names = glyph_names(&mut fonts, TEXT);
    let block = layout(&mut fonts, TEXT, &mac_style(20.0), None).unwrap();
    assert_eq!(block.lines.len(), 1, "{:?}", block.lines);
    assert_eq!(line_text(&block, &names), vec![TEXT.to_string()]);
}

#[test]
fn cyrillic_lines_break_at_the_spaces_inside_the_width() {
    let mut fonts = mac_fonts();
    let names = glyph_names(&mut fonts, TEXT);
    let words: Vec<&str> = TEXT.split(' ').collect();
    // The width of the widest word plus a half pixel: each word takes a line of its own, and no line is wider.
    let width = words
        .iter()
        .map(|word| layout(&mut fonts, word, &mac_style(20.0), None).unwrap().width)
        .fold(0.0_f32, f32::max)
        + 0.5;
    let block = layout(&mut fonts, TEXT, &mac_style(20.0), Some(width)).unwrap();
    let lines: Vec<String> = line_text(&block, &names).iter().map(|line| line.trim().to_string()).collect();
    assert_eq!(lines, words, "every word is a line of its own at this width");
    for (number, line) in block.lines.iter().enumerate() {
        assert!(line.width <= width + 0.01, "line {number} is {} wide, the width is {width}", line.width);
    }
}

#[test]
fn cyrillic_words_are_not_split_across_lines() {
    let mut fonts = mac_fonts();
    let names = glyph_names(&mut fonts, TEXT);
    let width = layout(&mut fonts, "мир! Это", &mac_style(20.0), None).unwrap().width + 0.5;
    let block = layout(&mut fonts, TEXT, &mac_style(20.0), Some(width)).unwrap();
    let lines: Vec<String> = line_text(&block, &names).iter().map(|line| line.trim().to_string()).collect();
    assert!(lines.len() > 1, "the paragraph breaks into lines: {lines:?}");
    let rejoined = lines.join(" ");
    assert_eq!(rejoined, TEXT, "no letter is lost or repeated, and every line ends at a space: {lines:?}");
}
