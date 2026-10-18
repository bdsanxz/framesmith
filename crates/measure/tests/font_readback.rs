//! Each checked-in face of the test font is read back with skrifa, a font reader, and holds the facts
//! of `docs/spec/test-font.md`, sections 1, 2 and 6: the family, the style and the weight class, the
//! glyph count, the glyph of each character, the outlines, the shared advances, the missing glyph of
//! U+00E9, and the width of the stem of `l` in each weight.

use measure::font::{mapped_characters, Face, FAMILY};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{LocationRef, Size};
use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::path::PathBuf;

fn checked_in(face: Face) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts")
        .join(face.file_name());
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The points an outline is drawn with, control points included.
#[derive(Default)]
struct Points(Vec<(f32, f32)>);

impl OutlinePen for Points {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push((x, y));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push((cx0, cy0));
        self.0.push((x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.extend([(cx0, cy0), (cx1, cy1), (x, y)]);
    }

    fn close(&mut self) {}
}

/// The points of a glyph's outline, or none when the glyph has no outline entry.
fn draw(font: &FontRef<'_>, glyph: GlyphId) -> Option<Points> {
    let outline = font.outline_glyphs().get(glyph)?;
    let mut points = Points::default();
    outline
        .draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut points)
        .expect("the outline draws");
    Some(points)
}

/// The least x, least y, greatest x and greatest y of a set of points.
fn bounds(points: &Points) -> (f32, f32, f32, f32) {
    points
        .0
        .iter()
        .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(x0, y0, x1, y1), &(x, y)| {
            (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
        })
}

#[test]
fn each_checked_in_face_reads_back_as_its_family_style_and_weight() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        let family = font
            .localized_strings(StringId::FAMILY_NAME)
            .english_or_first()
            .expect("a family name")
            .to_string();
        let style = font
            .localized_strings(StringId::SUBFAMILY_NAME)
            .english_or_first()
            .expect("a style name")
            .to_string();
        let weight = font.attributes().weight.value();
        println!("{}: family {family:?}, style {style:?}, weight {weight}", face.file_name());
        assert_eq!(family, FAMILY);
        assert_eq!(style, face.style());
        assert_eq!(weight, f32::from(face.weight_class()));
    }
}

/// The glyphs of a face: the 96 of section 2, and the ten tabular and ten old-style figures of section 7.
const GLYPHS: u16 = 116;

#[test]
fn each_face_has_the_glyphs_of_the_spec() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        let glyphs = font.maxp().expect("a maxp table").num_glyphs();
        println!("{}: {glyphs} glyphs", face.file_name());
        assert_eq!(glyphs, GLYPHS);
    }
}

#[test]
fn every_listed_character_maps_to_its_glyph_and_only_the_space_has_no_ink() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        let charmap = font.charmap();
        for (index, character) in mapped_characters().enumerate() {
            let glyph = GlyphId::new(index as u32 + 1);
            assert_eq!(
                charmap.map(character),
                Some(glyph),
                "U+{:04X} in {}",
                character as u32,
                face.file_name()
            );
            let points = draw(&font, glyph);
            if character == ' ' {
                assert!(points.is_none_or(|p| p.0.is_empty()), "the space has ink in {}", face.file_name());
            } else {
                let points = points.unwrap_or_else(|| panic!("U+{:04X} has no outline in {}", character as u32, face.file_name()));
                assert!(!points.0.is_empty(), "U+{:04X} is empty in {}", character as u32, face.file_name());
            }
        }
    }
}

#[test]
fn the_missing_character_glyph_is_the_ink_box_of_a_rectangle() {
    // The rectangle is 400 by 620 units in its centreline box, from the baseline (40) to the cap height
    // (660), moved 100 units right; its ink adds half the stroke on each side.
    let expected = [
        (Face::Regular, (60.0, 0.0, 540.0, 700.0)),
        (Face::Bold, (30.0, -30.0, 570.0, 730.0)),
    ];
    for (face, box_of_ink) in expected {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        let points = draw(&font, GlyphId::new(0)).expect("the missing-character glyph has an outline");
        assert_eq!(bounds(&points), box_of_ink, "the missing-character glyph of {}", face.file_name());
    }
}

#[test]
fn u_plus_00e9_maps_to_no_glyph_in_either_face() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        assert_eq!(font.charmap().map('\u{e9}'), None, "U+00E9 is mapped in {}", face.file_name());
    }
}

#[test]
fn both_faces_share_their_advance_widths() {
    let regular = checked_in(Face::Regular);
    let bold = checked_in(Face::Bold);
    let regular = FontRef::new(&regular).expect("the regular file is a font");
    let bold = FontRef::new(&bold).expect("the bold file is a font");
    let regular_metrics = regular.glyph_metrics(Size::unscaled(), LocationRef::default());
    let bold_metrics = bold.glyph_metrics(Size::unscaled(), LocationRef::default());
    for index in 0..u32::from(GLYPHS) {
        let glyph = GlyphId::new(index);
        assert_eq!(
            regular_metrics.advance_width(glyph),
            bold_metrics.advance_width(glyph),
            "advance of glyph {index} differs between the faces"
        );
    }
}

/// The width of the stem of `l`: its outline is one rectangle, so the distance between its least
/// and greatest x is the stroke width, and no other x value lies between them.
fn stem_width(face: Face) -> f32 {
    let bytes = checked_in(face);
    let font = FontRef::new(&bytes).expect("the file is a font");
    let glyph = font.charmap().map('l').expect("l is mapped");
    let points = draw(&font, glyph).expect("l has an outline");
    let mut xs: Vec<f32> = points.0.iter().map(|p| p.0).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    assert_eq!(
        xs.len(),
        2,
        "the stem of l has {} distinct x values in {}",
        xs.len(),
        face.file_name()
    );
    xs[1] - xs[0]
}

#[test]
fn bold_stem_of_l_is_wider_than_regular_stem() {
    let regular = stem_width(Face::Regular);
    let bold = stem_width(Face::Bold);
    println!("stem of l: regular {regular} units, bold {bold} units");
    assert_eq!(regular, 80.0);
    assert_eq!(bold, 140.0);
    assert!(bold > regular);
}
