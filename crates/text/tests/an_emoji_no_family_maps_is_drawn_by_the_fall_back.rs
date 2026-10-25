//! An emoji is in none of the families the composition declares, so a paragraph with one is refused with the character
//! named, and nothing is drawn for it in black or in the colour of its text (`docs/spec/text.md`, section 12.4). The
//! test font maps no emoji, so this test needs no font of the machine.

mod common;

use common::{fonts, style};
use text::{layout, TextError};

#[test]
fn a_paragraph_with_an_emoji_is_refused_with_the_emoji_named() {
    let mut fonts = fonts();
    let result = layout(&mut fonts, "Smile \u{1f600} now", &style(20.0), None);
    assert_eq!(result, Err(TextError::MissingCharacter('\u{1f600}')));
}
