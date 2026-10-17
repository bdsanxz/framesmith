//! An Arabic paragraph is drawn right to left with its letters joined, and its lines break at word boundaries
//! (`docs/spec/text.md`, sections 12.2 and 12.3). The paragraph is drawn with the machine's font, so the test is
//! compiled for macOS only: that font is a file of the machine, which the other systems do not have.

#![cfg(target_os = "macos")]

mod common;

use common::scripts::{glyph_ids, mac_fonts, mac_style, space_glyph};
use text::layout;

/// A word whose letters all join on both sides, so each has a joined form of its own.
const WORD: &str = "مرحبا";
const TEXT: &str = "مرحبا بالعالم هذا نص عربي";

/// The glyph of a character laid out alone: its isolated form.
fn isolated(fonts: &mut text::Fonts, character: char) -> u32 {
    let block = layout(fonts, &character.to_string(), &mac_style(40.0), None).unwrap();
    block.runs[0].glyphs[0].id
}

#[test]
fn arabic_letters_are_joined_inside_a_word() {
    let mut fonts = mac_fonts();
    let block = layout(&mut fonts, WORD, &mac_style(40.0), None).unwrap();
    let mut logical: Vec<u32> = glyph_ids(&block).concat();
    // The glyphs are drawn from the left, so the first letter of the word is the last of them.
    logical.reverse();
    let letters: Vec<char> = WORD.chars().collect();
    assert_eq!(logical.len(), letters.len(), "one glyph per letter at this word: {logical:?}");
    for (letter, glyph) in letters.iter().zip(&logical) {
        let alone = isolated(&mut fonts, *letter);
        assert_ne!(*glyph, alone, "the letter {letter:?} is drawn in its joined form, not its isolated one");
    }
}

/// The glyph ids of a text laid out on its own, in the order they are drawn from the left.
fn drawn(fonts: &mut text::Fonts, words: &str) -> Vec<u32> {
    let block = layout(fonts, words, &mac_style(40.0), None).unwrap();
    glyph_ids(&block).concat()
}

#[test]
fn arabic_letters_are_drawn_right_to_left() {
    // "با" ends the word "مرحبا" with the same letters in the same context, so its leftmost glyph is the last letter
    // of the word, and "مر" begins it, so its rightmost glyph is the first letter of the word. Both are checked in the
    // drawing of the word: the first letter at the right edge and the last one at the left.
    let mut fonts = mac_fonts();
    let word = drawn(&mut fonts, WORD);
    assert_eq!(word.len(), WORD.chars().count(), "one glyph per letter at this word");
    let last_letter_alone_at_the_end = drawn(&mut fonts, "با");
    let first_letter_alone_at_the_start = drawn(&mut fonts, "مر");
    assert_eq!(
        word.first(),
        last_letter_alone_at_the_end.first(),
        "the last letter of the word is drawn at the left edge"
    );
    assert_eq!(
        word.last(),
        first_letter_alone_at_the_start.last(),
        "the first letter of the word is drawn at the right edge"
    );
}

#[test]
fn arabic_lines_break_at_word_boundaries() {
    let mut fonts = mac_fonts();
    let words: Vec<&str> = TEXT.split(' ').collect();
    // The width of the widest word plus a half pixel: each word takes a line of its own.
    let width = words
        .iter()
        .map(|word| layout(&mut fonts, word, &mac_style(40.0), None).unwrap().width)
        .fold(0.0_f32, f32::max)
        + 0.5;
    let block = layout(&mut fonts, TEXT, &mac_style(40.0), Some(width)).unwrap();
    assert_eq!(block.lines.len(), words.len(), "{:?}", block.lines);
    // A line that ends at a space draws the space at its end: it is left out of the comparison.
    let space = space_glyph(&mut fonts);
    let lines: Vec<Vec<u32>> = glyph_ids(&block)
        .into_iter()
        .map(|line| line.into_iter().filter(|glyph| *glyph != space).collect())
        .collect();
    for (number, word) in words.iter().enumerate() {
        let alone = layout(&mut fonts, word, &mac_style(40.0), None).unwrap();
        assert_eq!(
            lines[number],
            glyph_ids(&alone).concat(),
            "line {number} holds the word {word:?} and its joined letters, and no part of another word"
        );
    }
}
