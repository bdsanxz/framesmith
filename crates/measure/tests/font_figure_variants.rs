//! The figure variants of each checked-in face of the test font, read back with skrifa (`docs/spec/test-font.md`,
//! section 7): the features that reach them, the advance of each tabular figure, the advance of each old-style figure, and
//! where the old-style figures stand and descend, against the lining figures of section 2.

use measure::font::Face;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{LocationRef, Size};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The glyph of the lining figure of the digit 0; the lining figures of 0 to 9 follow in order.
const LINING: u32 = 17;
/// The glyph of the tabular figure of the digit 0; the tabular figures follow in order.
const TABULAR: u32 = 96;
/// The glyph of the old-style figure of the digit 0; the old-style figures follow in order.
const OLDSTYLE: u32 = 106;
/// The advance of every tabular figure: the advance of the lining 8.
const TABULAR_ADVANCE: f32 = 560.0;
/// The advances of the lining figures 0 to 9, which the default figures keep, in both faces.
const LINING_ADVANCES: [f32; 10] = [500.0, 440.0, 540.0, 539.0, 540.0, 529.0, 540.0, 540.0, 560.0, 540.0];
/// The distance a figure may differ from its exact place, in units of the em, for the curves of its outline.
const TOLERANCE: f32 = 2.0;

fn checked_in(face: Face) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts")
        .join(face.file_name());
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The points of an outline that lie on it: the end of each line and each curve, not the control point of a curve.
#[derive(Default)]
struct OnCurve(Vec<(f32, f32)>);

impl OutlinePen for OnCurve {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push((x, y));
    }

    fn quad_to(&mut self, _cx0: f32, _cy0: f32, x: f32, y: f32) {
        self.0.push((x, y));
    }

    fn curve_to(&mut self, _cx0: f32, _cy0: f32, _cx1: f32, _cy1: f32, x: f32, y: f32) {
        self.0.push((x, y));
    }

    fn close(&mut self) {}
}

/// The least x, least y, greatest x and greatest y of the points on the outline of a glyph.
fn on_curve_bounds(font: &FontRef<'_>, glyph: u32) -> (f32, f32, f32, f32) {
    let outline = font
        .outline_glyphs()
        .get(GlyphId::new(glyph))
        .unwrap_or_else(|| panic!("glyph {glyph} has no outline"));
    let mut pen = OnCurve::default();
    outline
        .draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen)
        .expect("the outline draws");
    pen.0
        .iter()
        .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(x0, y0, x1, y1), &(x, y)| {
            (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
        })
}

fn advance(font: &FontRef<'_>, glyph: u32) -> f32 {
    font.glyph_metrics(Size::unscaled(), LocationRef::default())
        .advance_width(GlyphId::new(glyph))
        .unwrap_or_else(|| panic!("glyph {glyph} has no advance"))
}

/// Half the stroke of a face, in units of the em: the distance from a centreline to the outline.
fn half_stroke(face: Face) -> f32 {
    face.stroke() as f32 / 2.0
}

#[test]
fn the_layout_table_reaches_the_figure_variants_through_the_oldstyle_and_tabular_features_only() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        let gsub = font.gsub().expect("the face has a GSUB table");
        let features = gsub.feature_list().expect("the GSUB table has a feature list");
        let tags: BTreeSet<String> = features
            .feature_records()
            .iter()
            .map(|record| String::from_utf8_lossy(&record.feature_tag().to_be_bytes()).into_owned())
            .collect();
        println!("{}: features {tags:?}", face.file_name());
        let expected: BTreeSet<String> = ["onum", "tnum"].iter().map(|tag| tag.to_string()).collect();
        assert_eq!(tags, expected, "the features of {}", face.file_name());
    }
}

#[test]
fn every_tabular_figure_has_the_advance_of_the_lining_8_in_both_faces() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        for digit in 0..10 {
            assert_eq!(
                advance(&font, TABULAR + digit),
                TABULAR_ADVANCE,
                "the tabular figure {digit} of {}",
                face.file_name()
            );
        }
    }
}

#[test]
fn every_tabular_figure_stands_inside_its_advance() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        for digit in 0..10 {
            let (x0, _, x1, _) = on_curve_bounds(&font, TABULAR + digit);
            assert!(
                x0 >= 0.0 && x1 <= TABULAR_ADVANCE,
                "the tabular figure {digit} of {} runs from {x0} to {x1}, outside its advance",
                face.file_name()
            );
        }
    }
}

#[test]
fn every_oldstyle_figure_has_the_advance_of_its_lining_figure() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        for digit in 0..10 {
            assert_eq!(
                advance(&font, OLDSTYLE + digit),
                advance(&font, LINING + digit),
                "the old-style figure {digit} of {}",
                face.file_name()
            );
        }
    }
}

#[test]
fn the_lining_figures_keep_their_advances_in_both_faces() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        for (digit, expected) in LINING_ADVANCES.iter().enumerate() {
            assert_eq!(
                advance(&font, LINING + digit as u32),
                *expected,
                "the lining figure {digit} of {}",
                face.file_name()
            );
        }
    }
}

#[test]
fn the_lining_figures_and_the_old_style_0_1_2_6_and_8_do_not_reach_below_the_baseline() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        let limit = -half_stroke(face) - TOLERANCE;
        for digit in 0..10 {
            let (_, y0, _, _) = on_curve_bounds(&font, LINING + digit);
            assert!(y0 >= limit, "the lining figure {digit} of {} reaches {y0}", face.file_name());
            if [0, 1, 2, 6, 8].contains(&digit) {
                let (_, y0, _, _) = on_curve_bounds(&font, OLDSTYLE + digit);
                assert!(y0 >= limit, "the old-style figure {digit} of {} reaches {y0}", face.file_name());
            }
        }
    }
}

#[test]
fn the_old_style_3_4_5_7_and_9_descend_well_below_the_baseline() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        for digit in [3, 4, 5, 7, 9] {
            let (_, y0, _, _) = on_curve_bounds(&font, OLDSTYLE + digit);
            println!("{}: old-style {digit} reaches {y0}", face.file_name());
            assert!(
                y0 <= -120.0,
                "the old-style figure {digit} of {} reaches only {y0}, not below -120",
                face.file_name()
            );
        }
    }
}

#[test]
fn the_old_style_0_and_2_reach_the_x_height_and_the_old_style_6_and_8_the_cap_height() {
    for face in Face::ALL {
        let bytes = checked_in(face);
        let font = FontRef::new(&bytes).expect("the file is a font");
        let half = half_stroke(face);
        for digit in [0, 2] {
            let (_, _, _, y1) = on_curve_bounds(&font, OLDSTYLE + digit);
            assert!(
                (y1 - (420.0 + half)).abs() <= TOLERANCE,
                "the old-style figure {digit} of {} reaches {y1}, not the x-height",
                face.file_name()
            );
        }
        for digit in [6, 8] {
            let (_, _, _, y1) = on_curve_bounds(&font, OLDSTYLE + digit);
            assert!(
                (y1 - (660.0 + half)).abs() <= TOLERANCE,
                "the old-style figure {digit} of {} reaches {y1}, not the cap height",
                face.file_name()
            );
        }
    }
}
