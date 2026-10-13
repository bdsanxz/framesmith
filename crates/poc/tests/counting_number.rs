//! A text that counts: the forms of number, the number each frame shows, and the box that
//! keeps its width. See `docs/spec/native-notation.md`, section 11.
//! The fonts are this machine's, so a counting text is compared with a plain text of the same
//! words and never with fixed numbers.

use poc::count::Format;
use poc::model::Composition;
use poc::scene::{Evaluator, Op};
use poc::text::Shaper;
use poc::{layout, xml};
use std::path::Path;

fn load(source: &str) -> Composition {
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

fn written(form: &str, value: f64) -> String {
    Format::from_name(form).unwrap_or_else(|| panic!("`{form}` is a form")).write(value)
}

/// The glyphs a frame draws, with where each stands.
fn glyphs(composition: &Composition, frame: u64) -> Vec<(u32, f32)> {
    let ops = Evaluator::new(composition).frame(frame).ops;
    let runs = ops.iter().filter_map(|op| match op {
        Op::Glyphs { run, .. } => Some(*run),
        _ => None,
    });
    runs.flat_map(|run| run.glyphs.iter().map(|glyph| (glyph.id, glyph.x))).collect()
}

fn ids(composition: &Composition, frame: u64) -> Vec<u32> {
    glyphs(composition, frame).into_iter().map(|(id, _)| id).collect()
}

/// A plain text of `words`, set as the counting texts of these tests are.
fn plain(words: &str) -> Composition {
    load(&format!(
        r##"<composition width="320" height="64" fps="10" duration="1">
  <text left="0" top="0" white-space="nowrap" font-size="32" color="#ffffff">{words}</text>
</composition>"##
    ))
}

#[test]
fn each_form_writes_its_number() {
    assert_eq!(written("0", 46.2), "46");
    assert_eq!(written("0.00", 99.98), "99.98");
    assert_eq!(written("0.00", 7.0), "7.00");
    assert_eq!(written("0,0", 1_234_567.4), "1,234,567");
    assert_eq!(written("0,0", 999.0), "999");
    assert_eq!(written("0,0.00", 1234.5), "1,234.50");
    assert_eq!(written("m:ss", 202.1), "3:22");
    assert_eq!(written("m:ss.0", 239.4), "3:59.4");
    assert_eq!(written("m:ss.0", 5.0), "0:05.0");
    assert_eq!(written("0.000000", 0.5), "0.500000");
}

#[test]
fn a_number_is_rounded_to_its_last_place_before_it_is_written() {
    // A half goes away from zero.
    assert_eq!(written("0", 2.5), "3");
    assert_eq!(written("0", -2.5), "-3");
    assert_eq!(written("0.0", 0.25), "0.3");
    // Rounding up carries into the seconds and the minutes.
    assert_eq!(written("m:ss.0", 59.96), "1:00.0");
    assert_eq!(written("m:ss", 59.5), "1:00");
    assert_eq!(written("0,0", 999.5), "1,000");
    // A number that rounds to nothing has no sign.
    assert_eq!(written("0", -0.2), "0");
    assert_eq!(written("0.0", -0.2), "-0.2");
}

#[test]
fn only_the_named_forms_are_forms() {
    for form in ["0", "0.0", "0.000000", "0,0", "0,0.00", "m:ss", "m:ss.000"] {
        assert!(Format::from_name(form).is_some(), "`{form}` is a form");
    }
    for form in ["", "00", "0.", "0.0000000", "0.05", "0,000", "m:s", "mm:ss", "h:mm:ss", "#"] {
        assert!(Format::from_name(form).is_none(), "`{form}` is not a form");
    }
}

const COUNTING: &str = r##"<composition width="320" height="64" fps="10" duration="2">
  <text left="0" top="0" font-size="32" color="#ffffff" count="0" count-format="0.0">
    <tween at="0" duration="1" ease="linear" to-count="10"/>
  </text>
</composition>"##;

#[test]
fn each_frame_shows_the_number_of_its_time() {
    let composition = load(COUNTING);
    assert_eq!(ids(&composition, 0), ids(&plain("0.0"), 0));
    assert_eq!(ids(&composition, 3), ids(&plain("3.0"), 0));
    assert_eq!(ids(&composition, 7), ids(&plain("7.0"), 0));
    // After its tween the number stays.
    assert_eq!(ids(&composition, 10), ids(&plain("10.0"), 0));
    assert_eq!(ids(&composition, 19), ids(&plain("10.0"), 0));
    assert_ne!(ids(&composition, 3), ids(&composition, 7));
}

#[test]
fn the_attribute_alone_is_the_number_shown() {
    let composition = load(
        r##"<composition width="320" height="64" fps="10" duration="1">
  <text left="0" top="0" font-size="32" color="#ffffff" count="1234.5" count-format="0,0.00"/>
</composition>"##,
    );
    assert_eq!(ids(&composition, 0), ids(&plain("1,234.50"), 0));
    assert_eq!(ids(&composition, 9), ids(&plain("1,234.50"), 0));
}

/// A number that goes from 8 to 12: one digit, then two.
fn growing(attributes: &str) -> String {
    format!(
        r##"<composition width="320" height="64" fps="10" duration="1">
  <text left="0" top="0" font-size="32" color="#ffffff" count="8" {attributes}>
    <tween at="0" duration="1" ease="linear" to-count="12"/>
  </text>
</composition>"##
    )
}

#[test]
fn the_box_is_as_wide_as_the_widest_number_shown() {
    let composition = load(&growing(""));
    let text = &composition.root.children[0];
    let shown: Vec<&str> = text.counted.iter().map(|(number, _)| number.as_str()).collect();
    assert_eq!(shown, ["10", "11", "12", "8", "9"], "every number a frame shows, and no other");
    let width = |words: &str| plain(words).root.children[0].rect.width();
    let widest = ["8", "9", "10", "11", "12"].map(width).into_iter().fold(0.0, f64::max);
    assert!((text.rect.width() - widest).abs() < 0.01, "{} against {widest}", text.rect.width());
    assert!(text.rect.width() > width("8") + 1.0, "wider than its first number");
    assert!((text.rect.height() - plain("8").root.children[0].rect.height()).abs() < 0.01);
}

#[test]
fn the_number_is_placed_in_its_box_by_the_alignment_of_the_text() {
    let first = |composition: &Composition, frame: u64| glyphs(composition, frame)[0].1;
    let left = load(&growing(""));
    assert!(
        (first(&left, 0) - first(&left, 9)).abs() < 0.01,
        "from the left edge both start at one place"
    );
    let right = load(&growing(r#"text-align="right""#));
    assert!(
        first(&right, 0) > first(&right, 9) + 1.0,
        "from the right edge the short number starts later"
    );
}

#[test]
fn a_number_no_frame_shows_does_not_widen_the_box() {
    // The clip is there from frame 5 to frame 9, while the count runs from 5 to 9.
    let composition = load(
        r##"<composition width="320" height="64" fps="10" duration="2">
  <text left="0" top="0" font-size="32" color="#ffffff" count="0" start="0.5" duration="0.5">
    <tween at="0" duration="2" ease="linear" to-count="20"/>
  </text>
</composition>"##,
    );
    let text = &composition.root.children[0];
    let shown: Vec<&str> = text.counted.iter().map(|(number, _)| number.as_str()).collect();
    assert_eq!(shown, ["5", "6", "7", "8", "9"]);
    assert_eq!(ids(&composition, 6), ids(&plain("6"), 0));
    assert!(ids(&composition, 4).is_empty() && ids(&composition, 10).is_empty());
}

#[test]
fn what_a_counting_text_may_not_be_is_refused_with_its_line() {
    let source = r##"<composition width="320" height="64" fps="10" duration="1">
  <rect width="10" height="10" count="3"/>
  <text count="3">three</text>
  <text count-format="0.00"/>
  <text>
    <tween at="0" duration="1" to-count="5"/>
  </text>
  <text count="3" count-format="0.0%"/>
</composition>"##;
    let problems = xml::parse(source, Path::new(".")).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "`count` is for a <text>, not a <rect>"),
            (3, "a <text> that counts holds no words of its own: its text is the number"),
            (4, "`count-format` needs `count`"),
            (5, "a tween of `count` needs the attribute `count` on its <text>"),
            (
                8,
                "`count-format=\"0.0%\"` is not a form of number; the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals"
            ),
        ]
    );
}
