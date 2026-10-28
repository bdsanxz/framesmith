//! The arcs of a generated face are pieces of thirty degrees, each with its control point where the tangents at its ends meet
//! (`docs/spec/test-font.md`, section 4, rule 3). Read back from the bytes the generator writes for each face, with skrifa, so a
//! change to the generator is seen here and not only in the checked-in files: the glyph 8 is two full circles, four contours,
//! each of twelve pieces with twelve control points; a control point lies on the bisecting radius at the radius of its circle
//! divided by the cosine of half a piece, that is 15 degrees, which is 0.9659, so a control point of a circle of radius 200 lies
//! at 207.1 from the centre, and one at the radius itself, at 200, is not the curve the spec gives.

use measure::font::Face;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

/// Each contour as its points, the flag of each: true on the outline, false for a control point.
#[derive(Default)]
struct Contours(Vec<Vec<(f64, f64, bool)>>);

impl OutlinePen for Contours {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(vec![(f64::from(x), f64::from(y), true)]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.last_mut().unwrap().push((f64::from(x), f64::from(y), true));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let contour = self.0.last_mut().unwrap();
        contour.push((f64::from(cx0), f64::from(cy0), false));
        contour.push((f64::from(x), f64::from(y), true));
    }

    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        panic!("the generated face has no cubic curves");
    }

    fn close(&mut self) {}
}

/// The contours of the glyph 8 in the given bytes of a face.
fn glyph_eight_contours(bytes: &[u8]) -> Vec<Vec<(f64, f64, bool)>> {
    let font = FontRef::new(bytes).unwrap();
    let glyph = font.charmap().map('8').expect("the generated face maps the character 8");
    let outline = font.outline_glyphs().get(glyph).expect("the generated face has an outline for 8");
    let mut pen = Contours::default();
    outline
        .draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen)
        .unwrap();
    pen.0
}

#[test]
fn each_circle_of_the_generated_eight_is_twelve_pieces_with_control_points_at_the_radius_over_the_cosine() {
    // the tangents at the ends of a piece of 30 degrees meet on the bisecting radius at radius / cos(15 degrees)
    let cosine = 15f64.to_radians().cos();
    for face in Face::ALL {
        let bytes = face.bytes();
        let contours = glyph_eight_contours(&bytes);
        assert_eq!(
            contours.len(),
            4,
            "{face:?}: the 8 is two circles, each an outer contour and a hole"
        );
        for contour in &contours {
            let mut on: Vec<(f64, f64)> = contour.iter().filter(|p| p.2).map(|p| (p.0, p.1)).collect();
            // the outline closes on its first point, which is repeated at the end: it is one point, counted once
            if on.first() == on.last() {
                on.pop();
            }
            let off: Vec<(f64, f64)> = contour.iter().filter(|p| !p.2).map(|p| (p.0, p.1)).collect();
            assert_eq!(
                on.len(),
                12,
                "{face:?}: a circle of thirty-degree pieces has twelve points on its outline, it has {}",
                on.len()
            );
            assert_eq!(
                off.len(),
                12,
                "{face:?}: a circle of thirty-degree pieces has twelve control points, it has {}",
                off.len()
            );
            let n = on.len() as f64;
            let centre = (on.iter().map(|p| p.0).sum::<f64>() / n, on.iter().map(|p| p.1).sum::<f64>() / n);
            let radius = on
                .iter()
                .map(|p| ((p.0 - centre.0).powi(2) + (p.1 - centre.1).powi(2)).sqrt())
                .sum::<f64>()
                / n;
            // the outline circle of the loops: the centreline at 160 (upper) or 150 (lower), moved out or in by half the stroke
            let half = f64::from(face.stroke()) / 2.0;
            let expected = [160.0 - half, 160.0 + half, 150.0 - half, 150.0 + half];
            assert!(
                expected.iter().any(|r| (radius - r).abs() <= 2.0),
                "{face:?}: a circle of radius {radius:.1} is none of the spec's outline circles {expected:?}"
            );
            let control = radius / cosine;
            for (x, y) in &off {
                let distance = ((x - centre.0).powi(2) + (y - centre.1).powi(2)).sqrt();
                assert!(
                    (distance - control).abs() <= 2.0,
                    "{face:?}: a control point at {distance:.1} from the centre, worked {control:.1} (radius {radius:.1} over the cosine of 15 degrees)"
                );
            }
        }
    }
}
