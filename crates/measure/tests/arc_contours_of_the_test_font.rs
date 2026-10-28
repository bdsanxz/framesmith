//! The arcs of the test font are one contour each, and a full circle is an outer contour and a hole. Read back from the checked-in
//! faces with skrifa (`docs/spec/test-font.md`, section 4, rules 3 and 5): the glyph 8 is two full circles, so it has four contours,
//! two outer ones that run clockwise and two holes that run counter-clockwise; each contour is twelve pieces of thirty degrees, so
//! it has twelve control points and thirteen points on the outline (the start, and the end of each piece); and every point on the outline lies on its circle, whose centre
//! and radius are the spec's: the centreline of the glyph 8 is a circle of radius 160 units about (150, 500) for the upper loop and
//! of radius 150 about (150, 190) for the lower one, moved 110 units right, with the stroke half the width of the face on each side.

use measure::font::Face;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};
use std::path::PathBuf;

/// Each contour as its points, the flag of each: true on the outline, false for a control point.
#[derive(Default)]
struct Contours(Vec<Vec<(f32, f32, bool)>>);

impl OutlinePen for Contours {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(vec![(x, y, true)]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.last_mut().unwrap().push((x, y, true));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let contour = self.0.last_mut().unwrap();
        contour.push((cx0, cy0, false));
        contour.push((x, y, true));
    }

    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        panic!("the test font has no cubic curves");
    }

    fn close(&mut self) {}
}

fn glyph_contours(face: Face) -> Vec<Vec<(f32, f32, bool)>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts")
        .join(face.file_name());
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let font = FontRef::new(&bytes).unwrap();
    let glyph = font.charmap().map('8').expect("the test font maps the character 8");
    let outline = font.outline_glyphs().get(glyph).expect("the test font has an outline for 8");
    let mut pen = Contours::default();
    outline
        .draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen)
        .unwrap();
    pen.0
}

/// Twice the signed area of the polygon through the points on the outline: negative for a clockwise contour in a y-up font.
fn signed_area(contour: &[(f32, f32, bool)]) -> f32 {
    let on: Vec<(f32, f32)> = contour.iter().filter(|p| p.2).map(|p| (p.0, p.1)).collect();
    (0..on.len())
        .map(|i| {
            let (x0, y0) = on[i];
            let (x1, y1) = on[(i + 1) % on.len()];
            x0 * y1 - x1 * y0
        })
        .sum()
}

/// The centre of an upper or a lower loop, and the radius of its outline circle, in units: the upper loop of the glyph 8 has its
/// centreline at radius 160 about y = 500 and the lower one at 150 about y = 190; the outline is the centreline moved out and in by
/// half the stroke width of the face, and moved 110 units right.
fn expected_radius(face: Face, upper: bool, hole: bool) -> f32 {
    let half = face.stroke() as f32 / 2.0;
    let centreline = if upper { 160.0 } else { 150.0 };
    centreline + if hole { -half } else { half }
}

#[test]
fn the_eight_has_an_outer_contour_and_a_hole_for_each_of_its_two_loops() {
    for face in Face::ALL {
        let contours = glyph_contours(face);
        assert_eq!(
            contours.len(),
            4,
            "the glyph 8 of the {:?} face has two loops, each with an outer contour and a hole",
            face
        );
    }
}

#[test]
fn an_outer_contour_runs_clockwise_and_a_hole_counter_clockwise() {
    for face in Face::ALL {
        let contours = glyph_contours(face);
        let clockwise = contours.iter().filter(|c| signed_area(c) < 0.0).count();
        let counter = contours.iter().filter(|c| signed_area(c) > 0.0).count();
        assert_eq!(
            (clockwise, counter),
            (2, 2),
            "{:?}: two outer contours clockwise and two holes counter-clockwise",
            face
        );
        for contour in &contours {
            let on = contour.iter().filter(|p| p.2).count();
            // the move to the first point, then the end of each of the twelve pieces: the last piece ends on the first point,
            // which skrifa reports again, so a contour of twelve pieces has thirteen points on the outline
            assert_eq!(
                on, 13,
                "a contour of an arc is twelve pieces of thirty degrees: the start and the twelve piece ends"
            );
            assert_eq!(contour.len() - on, 12, "and twelve control points, one for each piece");
        }
    }
}

#[test]
fn each_point_on_an_outline_lies_on_its_circle_of_the_spec() {
    for face in Face::ALL {
        for contour in glyph_contours(face) {
            // the centre of a loop: the upper loop has its centre at y = 500, the lower at y = 190
            let on: Vec<(f32, f32)> = contour.iter().filter(|p| p.2).map(|p| (p.0, p.1)).collect();
            let mean_y = on.iter().map(|p| p.1).sum::<f32>() / on.len() as f32;
            let upper = mean_y > 345.0;
            let (cx, cy) = (260.0, if upper { 500.0 } else { 190.0 });
            let hole = signed_area(&contour) > 0.0;
            let radius = expected_radius(face, upper, hole);
            for (x, y) in on {
                let distance = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                assert!(
                    (distance - radius).abs() <= 1.0,
                    "{:?}: a point at {distance:.2} units from the centre of its loop, the circle of the spec is {radius}",
                    face
                );
            }
        }
    }
}
