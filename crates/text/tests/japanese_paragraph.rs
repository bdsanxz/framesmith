//! A Japanese paragraph breaks between characters where the line-breaking rules allow, and no line starts with a
//! character that may not start a line (`docs/spec/text.md`, section 12.3). The paragraph is drawn with the machine's
//! font, so the test is compiled for macOS only: that font is a file of the machine, which the other systems do not
//! have.

#![cfg(target_os = "macos")]

mod common;

use common::scripts::{glyph_names, line_text, mac_fonts, mac_style};
use text::layout;

/// The text has no space, and holds closing marks (`。`, `、` and `」`) and an opening bracket (`「`) where a line may
/// end or begin.
const TEXT: &str = "日本語の文章を、改行の位置を確かめるために「区切り」の記号を含めて、正しく折り返すかを調べます。";

/// The characters that may not start a line: closing punctuation (UAX 14, classes CL and CP).
const MAY_NOT_START: &[char] = &['。', '、', '」', '』', '）'];

/// The characters that may not end a line: opening brackets (UAX 14, class OP).
const MAY_NOT_END: &[char] = &['「', '『', '（'];

#[test]
fn japanese_lines_break_between_characters_and_keep_the_text_whole() {
    let mut fonts = mac_fonts();
    let names = glyph_names(&mut fonts, TEXT);
    let mut widths_that_break = 0;
    let mut width = 40.0_f32;
    while width <= 320.0 {
        let block = layout(&mut fonts, TEXT, &mac_style(20.0), Some(width)).unwrap();
        let lines = line_text(&block, &names);
        if lines.len() > 1 {
            widths_that_break += 1;
        }
        assert_eq!(lines.concat(), TEXT, "at width {width} no character is lost or repeated: {lines:?}");
        width += 3.0;
    }
    assert!(
        widths_that_break > 20,
        "the paragraph breaks at most of these widths: {widths_that_break}"
    );
}

#[test]
fn no_line_starts_with_a_closing_mark_nor_ends_with_an_opening_bracket() {
    let mut fonts = mac_fonts();
    let names = glyph_names(&mut fonts, TEXT);
    let mut width = 40.0_f32;
    while width <= 320.0 {
        let block = layout(&mut fonts, TEXT, &mac_style(20.0), Some(width)).unwrap();
        let lines = line_text(&block, &names);
        for (number, line) in lines.iter().enumerate() {
            let first = line.chars().next().unwrap();
            if number > 0 {
                assert!(
                    !MAY_NOT_START.contains(&first),
                    "at width {width} line {number} starts with {first:?}: {lines:?}"
                );
            }
            if number + 1 < lines.len() {
                let last = line.chars().last().unwrap();
                assert!(
                    !MAY_NOT_END.contains(&last),
                    "at width {width} line {number} ends with {last:?}: {lines:?}"
                );
            }
        }
        width += 3.0;
    }
}
