//! How a paragraph is flowed into lines: where a line may end, the order of right-to-left
//! words, and the space a line ends at. See `docs/spec/native-notation.md`, section 7.
//! The fonts are this machine's, so the tests compare a text with itself laid out another
//! way and never with fixed numbers.

use poc::model::{GlyphRun, TextStyle};
use poc::text::{Flowed, Piece, Shaper};
use poc::{layout, xml};
use std::collections::HashSet;
use std::path::Path;

fn style() -> TextStyle {
    TextStyle {
        size: 32.0,
        ..TextStyle::default()
    }
}

/// The paragraph on one line.
fn unbroken(shaper: &mut Shaper, text: &str) -> Flowed {
    shaper.flow(text, &[], &style(), None)
}

fn width(shaper: &mut Shaper, text: &str) -> f64 {
    let flowed = unbroken(shaper, text);
    assert_eq!(flowed.lines.len(), 1, "`{text}` with no limit is one line");
    flowed.lines[0].width
}

fn runs(flowed: &Flowed, line: usize) -> Vec<&GlyphRun> {
    let pieces = flowed.lines[line].pieces.iter();
    pieces
        .filter_map(|piece| match piece {
            Piece::Run(run) => Some(run),
            Piece::Box(..) => None,
        })
        .collect()
}

/// The glyphs a word is drawn with.
fn glyphs_of(shaper: &mut Shaper, word: &str) -> HashSet<u32> {
    let flowed = unbroken(shaper, word);
    let ids: HashSet<u32> = runs(&flowed, 0)
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id))
        .collect();
    assert!(!ids.contains(&0), "this machine has a font for `{word}`");
    ids
}

/// The left and right end of the glyphs among `ids` on a line: where a word stands.
fn span(flowed: &Flowed, line: usize, ids: &HashSet<u32>) -> (f32, f32) {
    let placed = runs(flowed, line)
        .into_iter()
        .flat_map(|run| run.glyphs.iter())
        .filter(|glyph| ids.contains(&glyph.id))
        .map(|glyph| glyph.x);
    let xs: Vec<f32> = placed.collect();
    assert!(!xs.is_empty(), "the word is on line {line}");
    (
        xs.iter().copied().fold(f32::MAX, f32::min),
        xs.iter().copied().fold(f32::MIN, f32::max),
    )
}

#[test]
fn a_line_ends_at_a_space_and_its_width_leaves_that_space_out() {
    let mut shaper = Shaper::new();
    let both = width(&mut shaper, "render finished");
    let first = width(&mut shaper, "render");
    let flowed = shaper.flow("render finished", &[], &style(), Some(both - 1.0));
    assert_eq!(flowed.lines.len(), 2, "the two words do not fit on one line");
    assert!(
        (flowed.lines[0].width - first).abs() < 0.01,
        "the first line is as wide as its word alone"
    );
    // Given exactly its own width, the text stays on one line.
    assert_eq!(shaper.flow("render finished", &[], &style(), Some(both)).lines.len(), 1);
}

#[test]
fn a_word_wider_than_its_line_is_not_broken() {
    let mut shaper = Shaper::new();
    let whole = width(&mut shaper, "Hamburgefonstiv");
    let flowed = shaper.flow("Hamburgefonstiv", &[], &style(), Some(whole / 3.0));
    assert_eq!(flowed.lines.len(), 1);
}

#[test]
fn a_no_break_space_holds_its_words_together() {
    let mut shaper = Shaper::new();
    let held = width(&mut shaper, "10\u{a0}km");
    let all = width(&mut shaper, "about 10\u{a0}km away");
    let flowed = shaper.flow("about 10\u{a0}km away", &[], &style(), Some(all - 1.0));
    assert_eq!(flowed.lines.len(), 2);
    // The line ends after `km`, not inside `10 km`.
    let about = width(&mut shaper, "about 10\u{a0}km");
    assert!((flowed.lines[0].width - about).abs() < 0.01 && about > held);
}

#[test]
fn a_script_without_spaces_breaks_between_its_characters() {
    let mut shaper = Shaper::new();
    let text = "これは日本語の文章です";
    let whole = width(&mut shaper, text);
    let limit = whole * 0.4;
    let flowed = shaper.flow(text, &[], &style(), Some(limit));
    assert!(
        flowed.lines.len() >= 3,
        "eleven characters at four tenths of their width: {} lines",
        flowed.lines.len()
    );
    assert!(
        flowed.lines.iter().all(|line| line.width <= limit + 0.1),
        "no line is wider than the limit"
    );
}

#[test]
fn thai_breaks_between_words_found_by_a_dictionary() {
    let mut shaper = Shaper::new();
    // "Thai language" and "test": two words written without a space.
    let (first, second) = ("ภาษาไทย", "ทดสอบ");
    let text = format!("{first}{second}");
    let whole = width(&mut shaper, &text);
    let flowed = shaper.flow(&text, &[], &style(), Some(whole - 1.0));
    assert_eq!(flowed.lines.len(), 2, "the text breaks once");
    let alone = width(&mut shaper, first);
    assert!(
        (flowed.lines[0].width - alone).abs() < 0.5,
        "the first line is the first word: {} against {alone}",
        flowed.lines[0].width
    );
}

#[test]
fn right_to_left_words_run_from_the_right_between_left_to_right_words() {
    let mut shaper = Shaper::new();
    // Latin, two Hebrew words, Latin: the first Hebrew word stands right of the second.
    let (first, second) = ("שלום", "עברית");
    let latin_before = glyphs_of(&mut shaper, "abc");
    let latin_after = glyphs_of(&mut shaper, "xyz");
    let (one, two) = (glyphs_of(&mut shaper, first), glyphs_of(&mut shaper, second));
    assert!(one.is_disjoint(&two), "the two words share no letter");
    let flowed = unbroken(&mut shaper, &format!("abc {first} {second} xyz"));
    let (before, after) = (span(&flowed, 0, &latin_before), span(&flowed, 0, &latin_after));
    let (one, two) = (span(&flowed, 0, &one), span(&flowed, 0, &two));
    assert!(before.1 < two.0, "the Latin word before stands left of both");
    assert!(two.1 < one.0, "the second Hebrew word stands left of the first");
    assert!(one.1 < after.0, "the Latin word after stands right of both");
}

#[test]
fn the_space_a_right_to_left_line_ends_at_hangs_too() {
    let mut shaper = Shaper::new();
    let (first, second) = ("שלום", "עברית");
    let text = format!("{first} {second}");
    let whole = width(&mut shaper, &text);
    let alone = width(&mut shaper, first);
    let flowed = shaper.flow(&text, &[], &style(), Some(whole - 1.0));
    assert_eq!(flowed.lines.len(), 2);
    assert!(
        (flowed.lines[0].width - alone).abs() < 0.01,
        "the first line is as wide as its word alone"
    );
    // The word starts at the line's left end, as it does alone: no space stands before it.
    let ids = glyphs_of(&mut shaper, first);
    let lone = unbroken(&mut shaper, first);
    let (here, there) = (span(&flowed, 0, &ids), span(&lone, 0, &ids));
    assert!(
        (here.0 - there.0).abs() < 0.01,
        "the word starts at {} and alone at {}",
        here.0,
        there.0
    );
}

#[test]
fn white_space_between_words_is_one_space() {
    let scene = r##"<composition width="640" height="64" fps="10" duration="1">
  <text id="plain" left="0" top="0" white-space="nowrap" font-size="32">one two three</text>
  <text id="spread" left="0" top="32" white-space="nowrap" font-size="32">  one   two
      three  </text>
</composition>"##;
    let mut composition = xml::parse(scene, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    let widths: Vec<f64> = composition.root.children.iter().map(|text| text.rect.width()).collect();
    assert!(widths[0] > 0.0);
    assert!((widths[0] - widths[1]).abs() < 0.01, "both texts are as wide: {widths:?}");
}
