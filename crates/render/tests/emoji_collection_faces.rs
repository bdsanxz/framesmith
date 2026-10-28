//! The Mac's emoji collection, read from the machine's own file (`docs/spec/text.md`, sections 2.6 and 12.4): its one
//! visible face is registered and the hidden face is not, a collection named as another family is refused, and an emoji
//! is read as a picture of its own colours at the size of its text. The file is a file of the machine, so the test is
//! compiled for macOS only.

#![cfg(target_os = "macos")]

// Moved from the text crate: the emoji pictures are read by frx7's backend, which only the render crate registers
// (`docs/spec/text.md`, section 12.4, point 4; decision B87).

use text::{layout, Align, FontError, Fonts, LineHeight, Style, TextError};

const EMOJI_FILE: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";
const EMOJI_FAMILY: &str = "Apple Color Emoji";

fn emoji_bytes() -> Vec<u8> {
    std::fs::read(EMOJI_FILE).unwrap_or_else(|error| panic!("{EMOJI_FILE}: {error}"))
}

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
fn the_emoji_collection_registers_its_one_visible_face_and_not_the_hidden_one() {
    let mut fonts = Fonts::new(&render::system_codecs());
    fonts.register(emoji_bytes(), Some(EMOJI_FAMILY)).expect("the collection registers");
    assert_eq!(fonts.len(), 1, "one face is drawn from the collection");
    assert!(fonts.has_family(EMOJI_FAMILY));
    assert!(!fonts.has_family(".Apple Color Emoji UI"), "the hidden face is never registered");
}

#[test]
fn a_collection_declared_under_another_family_is_refused_with_the_name_of_its_face() {
    let mut fonts = Fonts::new(&render::system_codecs());
    let error = fonts.register(emoji_bytes(), Some("Arial")).unwrap_err();
    assert_eq!(
        error,
        FontError::Collection("its one face is named Apple Color Emoji, not Arial".to_string())
    );
    assert!(fonts.is_empty(), "a refused collection registers nothing");
}

#[test]
fn a_collection_declared_with_no_family_takes_the_name_of_its_face() {
    let mut fonts = Fonts::new(&render::system_codecs());
    fonts.register(emoji_bytes(), None).expect("the collection registers");
    assert!(fonts.has_family(EMOJI_FAMILY));
}

#[test]
fn an_emoji_is_drawn_as_a_picture_in_its_own_colours() {
    let mut fonts = Fonts::new(&render::system_codecs());
    fonts.register(emoji_bytes(), Some(EMOJI_FAMILY)).expect("the collection registers");
    let block = layout(&mut fonts, "\u{1F600}", &emoji_style(20.0), None).expect("the emoji is drawn");
    assert_eq!(block.pictures.len(), 1, "the emoji is one picture");
    let picture = &block.pictures[0];
    assert!(picture.width > 0 && picture.height > 0, "the picture has pixels");
    assert_eq!(picture.pixels.len(), picture.width as usize * picture.height as usize * 4);
    // Premultiplied pixels: a yellow of the face is opaque and red and green high, with blue low.
    let yellow = picture
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] >= 200 && pixel[0] >= 200 && pixel[1] >= 140 && pixel[2] <= 120)
        .count();
    assert!(yellow > 50, "the emoji has yellow of its own: {yellow} pixels");
}

#[test]
fn an_emoji_is_drawn_at_the_size_of_its_text() {
    let mut fonts = Fonts::new(&render::system_codecs());
    fonts.register(emoji_bytes(), Some(EMOJI_FAMILY)).expect("the collection registers");
    let small = layout(&mut fonts, "\u{1F600}", &emoji_style(20.0), None).expect("drawn at 20");
    let large = layout(&mut fonts, "\u{1F600}", &emoji_style(40.0), None).expect("drawn at 40");
    let (small, large) = (&small.pictures[0], &large.pictures[0]);
    assert!(
        (large.width as i64 - 2 * small.width as i64).abs() <= 1,
        "twice the size gives twice the width: {} and {}",
        small.width,
        large.width
    );
    assert!(
        (large.height as i64 - 2 * small.height as i64).abs() <= 1,
        "twice the size gives twice the height: {} and {}",
        small.height,
        large.height
    );
}

#[test]
fn an_emoji_the_face_has_no_picture_for_is_refused_naming_it() {
    let mut fonts = Fonts::new(&render::system_codecs());
    fonts.register(emoji_bytes(), Some(EMOJI_FAMILY)).expect("the collection registers");
    let refused = layout(&mut fonts, "\u{2640}", &emoji_style(20.0), None);
    assert!(
        matches!(refused, Err(TextError::NoPicture('\u{2640}'))),
        "U+2640 has no picture in the face: {refused:?}"
    );
}
