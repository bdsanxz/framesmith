//! An emoji whose colour picture no reader can read is refused with frx7's reason, and never drawn from its outlines
//! (`docs/spec/text.md`, section 12.4, points 4 and 9). The collection is made with a registry that has no reader of
//! stills from memory, which is what a caller gives when no system backend is registered. The face is the Mac's emoji
//! collection, read from the machine's own file, so the test is compiled for macOS only.

#![cfg(target_os = "macos")]

use text::{layout, Align, Fonts, LineHeight, Style, TextError};

const EMOJI_FILE: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";
const EMOJI_FAMILY: &str = "Apple Color Emoji";

fn emoji_style(size: f32) -> Style {
    Style {
        families: vec![EMOJI_FAMILY.to_string()],
        size,
        weight: 400.0,
        italic: false,
        line_height: LineHeight::Normal,
        letter_spacing: 0.0,
        align: Align::Left,
        uppercase: false,
        nowrap: false,
        figures: Vec::new(),
    }
}

#[test]
fn an_emoji_the_registry_cannot_read_is_refused_with_the_reason() {
    let bytes = std::fs::read(EMOJI_FILE).unwrap_or_else(|error| panic!("{EMOJI_FILE}: {error}"));
    let mut fonts = Fonts::new(&codec::Registry::new());
    fonts.register(bytes, Some(EMOJI_FAMILY)).expect("the collection registers");

    let refused = layout(&mut fonts, "\u{1F600}", &emoji_style(20.0), None);
    match &refused {
        Err(TextError::PictureRefused { character, reason }) => {
            assert_eq!(*character, '\u{1F600}');
            assert!(!reason.is_empty(), "frx7's reason is given");
            let message = refused.as_ref().unwrap_err().to_string();
            assert!(message.contains("U+1F600"), "the code point is named: {message}");
            assert!(message.contains(reason.as_str()), "frx7's reason is in the message: {message}");
        }
        other => panic!("the emoji must be refused, not drawn: {other:?}"),
    }
}

#[test]
fn every_emoji_the_registry_cannot_read_is_refused_and_never_outlined() {
    // Each emoji of the face is refused the same way: none is drawn as a glyph of the face's outlines.
    let bytes = std::fs::read(EMOJI_FILE).unwrap_or_else(|error| panic!("{EMOJI_FILE}: {error}"));
    let mut fonts = Fonts::new(&codec::Registry::new());
    fonts.register(bytes, Some(EMOJI_FAMILY)).expect("the collection registers");
    for emoji in ["\u{1F600}", "\u{1F44D}"] {
        let result = layout(&mut fonts, emoji, &emoji_style(20.0), None);
        assert!(matches!(result, Err(TextError::PictureRefused { .. })), "{emoji}: {result:?}");
    }
}
