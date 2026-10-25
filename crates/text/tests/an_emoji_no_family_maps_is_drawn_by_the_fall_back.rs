//! An emoji is in none of the families of the test font's list, so the fall-back of `docs/spec/text.md`, section 2.4,
//! draws it: on macOS from `Apple Color Emoji`, as its picture, and nothing is drawn for it in the colour of its text
//! (section 12.4); on a system whose fonts are not read, as the missing-character mark, which the block names.

mod common;

use common::{fonts, style};
use text::layout;

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
