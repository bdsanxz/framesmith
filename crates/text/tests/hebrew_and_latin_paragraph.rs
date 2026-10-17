//! A paragraph that mixes Hebrew and Latin is drawn with each run in its own direction, and the runs in the order the
//! Unicode bidirectional algorithm gives for a left-to-right paragraph (`docs/spec/text.md`, section 12.2). The
//! paragraph is drawn with the machine's font, so the test is compiled for macOS only: that font is a file of the
//! machine, which the other systems do not have.

#![cfg(target_os = "macos")]

mod common;

use common::scripts::{glyph_names, line_text, mac_fonts, mac_style};
use text::layout;

const TEXT: &str = "Hello שלום World";

#[test]
fn hebrew_runs_are_drawn_right_to_left_between_the_latin_runs() {
    let mut fonts = mac_fonts();
    let names = glyph_names(&mut fonts, TEXT);
    let block = layout(&mut fonts, TEXT, &mac_style(40.0), None).unwrap();
    assert_eq!(block.lines.len(), 1, "{:?}", block.lines);
    // The Latin runs keep their order, and each Latin run sits before or after the Hebrew run as the text does. The
    // Hebrew run is drawn from its last letter at the left to its first at the right, so the drawn line reads "Hello",
    // then the Hebrew letters reversed, then "World". The spaces are drawn between the runs and are left out here.
    let drawn = line_text(&block, &names).concat().replace(' ', "");
    assert_eq!(drawn, "Hello\u{5dd}\u{5d5}\u{5dc}\u{5e9}World", "the drawn order of the letters");
}
