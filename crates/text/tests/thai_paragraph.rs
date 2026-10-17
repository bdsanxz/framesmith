//! A Thai paragraph, written with no spaces, breaks only at word boundaries (`docs/spec/text.md`, section 12.3). The
//! paragraph is drawn with the machine's font, so the test is compiled for macOS only: that font is a file of the
//! machine, which the other systems do not have.

#![cfg(target_os = "macos")]

mod common;

use common::scripts::{line_glyphs, mac_fonts, mac_style};
use text::layout;

/// The words of the paragraph, in order. The paragraph is their concatenation: Thai writes no spaces between words.
const WORDS: [&str; 11] = ["สวัสดี", "ชาว", "โลก", "ที่", "สวยงาม", "มาก", "และ", "ยินดี", "ต้อนรับ", "ทุก", "คน"];

/// The positions, in characters, where a word of the paragraph begins after the first: the places a line may end.
fn word_boundaries() -> Vec<usize> {
    let mut boundaries = Vec::new();
    let mut at = 0;
    for word in &WORDS[..WORDS.len() - 1] {
        at += word.chars().count();
        boundaries.push(at);
    }
    boundaries
}

#[test]
fn thai_lines_break_only_at_word_boundaries() {
    let mut fonts = mac_fonts();
    let text = WORDS.concat();
    let boundaries = word_boundaries();
    let mut breaks_seen = 0;
    let mut width = 40.0_f32;
    while width <= 320.0 {
        let block = layout(&mut fonts, &text, &mac_style(20.0), Some(width)).unwrap();
        // Each character is one glyph in this text, so the glyphs a line holds count its characters. The glyphs of a Thai
        // mark take forms that depend on the letter they sit on, so the glyphs are counted rather than named.
        let per_line: Vec<usize> = line_glyphs(&block).iter().map(Vec::len).collect();
        let total: usize = per_line.iter().sum();
        assert_eq!(
            total,
            text.chars().count(),
            "one glyph per character at width {width}: {per_line:?}"
        );
        let mut at = 0;
        for count in per_line.iter().take(per_line.len().saturating_sub(1)) {
            at += count;
            assert!(
                boundaries.contains(&at),
                "at width {width} a line ends after {at} characters, which is inside a word: {per_line:?}"
            );
            breaks_seen += 1;
        }
        width += 4.0;
    }
    assert!(breaks_seen > 0, "the paragraph breaks at some of these widths");
}
