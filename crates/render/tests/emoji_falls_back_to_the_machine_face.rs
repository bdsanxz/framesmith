//! The fall-back of `docs/spec/text.md`, section 2.4, for an emoji: the third family in the stated order that maps it is
//! the machine's `Apple Color Emoji`, drawn as its picture. Moved from the text crate's fall-back test, with its
//! assertions; the emoji is read by frx7's backend, which only the render crate registers (decision B87).

#![cfg(target_os = "macos")]

use text::{layout, Align, Fonts, LineHeight, Style};

const FAMILY: &str = "Framesmith Test Stroke";

fn fonts() -> Fonts {
    let font = |name: &str| {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/fonts")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    };
    let mut fonts = Fonts::new(&render::system_codecs());
    fonts
        .register(font("framesmith-test-stroke-400.ttf"), Some(FAMILY))
        .expect("the Regular face registers");
    fonts
        .register(font("framesmith-test-stroke-700.ttf"), Some(FAMILY))
        .expect("the Bold face registers");
    fonts
}

fn style(size: f32) -> Style {
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

#[test]
fn an_emoji_no_family_maps_is_drawn_from_the_machine_emoji_face_as_a_picture() {
    // An emoji is in neither of the first two families, and goes to the third, Apple Color Emoji.
    let mut fonts = fonts();
    let emoji = layout(&mut fonts, "A\u{1f600}", &style(40.0), None).unwrap();
    let families: Vec<String> = fonts.machine_faces().into_iter().map(|face| face.family).collect();
    println!("A\u{1f600} took the machine families {families:?}");
    assert_eq!(families, ["Apple Color Emoji"]);
    assert_eq!(emoji.pictures.len(), 1, "the emoji is drawn as its picture");
}
