// Moved from the text crate: the emoji is read by frx7's backend, which only the render crate registers (decision B87).
//! An emoji is in none of the families of the test font's list, so the fall-back of `docs/spec/text.md`, section 2.4,
//! draws it: on macOS from `Apple Color Emoji`, as its picture, and nothing is drawn for it in the colour of its text
//! (section 12.4); on a system whose fonts are not read, as the missing-character mark, which the block names.

use text::{layout, Align, Fonts, LineHeight, Style};

const FAMILY: &str = "Framesmith Test Stroke";

/// The test font of the repository's `fixtures/fonts` folder, registered as a composition's `<font>` elements register it.
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

/// A paragraph style in the test font at `size` pixels: weight 400, upright, normal line height, no letter spacing, left.
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

#[cfg(target_os = "macos")]
#[test]
fn a_paragraph_with_an_emoji_draws_it_as_the_picture_of_the_machine_emoji_face() {
    let mut fonts = fonts();
    let block = layout(&mut fonts, "Smile \u{1f600} now", &style(20.0), None).unwrap();
    assert_eq!(block.missing, None);
    assert_eq!(block.pictures.len(), 1, "the emoji is one picture");
    let families: Vec<String> = fonts.machine_faces().into_iter().map(|face| face.family).collect();
    assert_eq!(families, ["Apple Color Emoji"]);
    let runs_of_the_machine = block.runs.iter().filter(|run| fonts.machine_face_of(&run.font).is_some()).count();
    assert_eq!(
        runs_of_the_machine, 0,
        "no glyph of the emoji face is drawn in the colour of the text"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn a_paragraph_with_an_emoji_names_it_as_missing_where_the_machine_fonts_are_not_read() {
    let mut fonts = fonts();
    let block = layout(&mut fonts, "Smile \u{1f600} now", &style(20.0), None).unwrap();
    assert_eq!(block.missing, Some('\u{1f600}'));
}
