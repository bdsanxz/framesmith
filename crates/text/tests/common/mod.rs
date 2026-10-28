//! What the tests of the text crate share: the test font files of the repository, the collection that holds
//! them, and the style of a paragraph in them.

#![allow(dead_code)]

use std::path::PathBuf;

pub mod scripts;

use text::{Align, Fonts, LineHeight, Style};

/// The family name of the test font (`docs/spec/test-font.md`, section 1).
pub const FAMILY: &str = "Framesmith Test Stroke";

/// The bytes of a test font file of the repository's `fixtures/fonts` folder.
pub fn font_file(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// A collection with the Regular and the Bold face of the test font registered under its family name, the way a
/// composition's two `<font>` elements register them.
pub fn fonts() -> Fonts {
    let mut fonts = Fonts::new(&codec::Registry::new());
    fonts
        .register(font_file("framesmith-test-stroke-400.ttf"), Some(FAMILY))
        .expect("the Regular face registers");
    fonts
        .register(font_file("framesmith-test-stroke-700.ttf"), Some(FAMILY))
        .expect("the Bold face registers");
    fonts
}

/// A paragraph style in the test font at `size` pixels: weight 400, upright, normal line height, no letter
/// spacing, aligned to the left.
pub fn style(size: f32) -> Style {
    Style {
        families: vec![FAMILY.to_string()],
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
