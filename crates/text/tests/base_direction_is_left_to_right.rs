//! The base direction of a paragraph is left to right for every script (`docs/spec/text.md`, section 12.2, rule 2). So
//! a paragraph of Arabic or of Hebrew that ends with a full stop draws the full stop to the right of its last word, as
//! a left-to-right paragraph does. The paragraphs are drawn with the machine's font, so the test is compiled for macOS
//! only: that font is a file of the machine, which the other systems do not have.

#![cfg(target_os = "macos")]

mod common;

use common::scripts::{line_glyphs, mac_fonts, mac_style};
use text::{layout, Fonts};

/// The glyph of the full stop laid out alone.
fn full_stop(fonts: &mut Fonts) -> u32 {
    let block = layout(fonts, ".", &mac_style(20.0), None).unwrap();
    block.runs[0].glyphs[0].id
}

/// Lays `words` out on one line and checks that the full stop at its end is the glyph drawn furthest to the right.
fn assert_full_stop_drawn_right_of_the_last_word(words: &str) {
    let mut fonts = mac_fonts();
    let stop = full_stop(&mut fonts);
    let block = layout(&mut fonts, words, &mac_style(20.0), None).unwrap();
    assert_eq!(block.lines.len(), 1, "the paragraph is one line: {:?}", block.lines);
    // The glyphs of a line are sorted from the left, so the last one is the furthest to the right.
    let drawn: Vec<u32> = line_glyphs(&block).concat().iter().map(|glyph| glyph.id).collect();
    assert_eq!(
        drawn.last(),
        Some(&stop),
        "the full stop of {words:?} is drawn to the right of its last word, at the right edge: {drawn:?}"
    );
}

#[test]
fn arabic_full_stop_is_drawn_right_of_the_last_word() {
    assert_full_stop_drawn_right_of_the_last_word("مرحبا بالعالم، هذا نص عربي قصير للتجربة.");
}

#[test]
fn hebrew_full_stop_is_drawn_right_of_the_last_word() {
    assert_full_stop_drawn_right_of_the_last_word("שלום עולם.");
}
