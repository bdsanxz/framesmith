//! The expansion of a stroke along a closed contour (`docs/spec/text.md`, section 6.6, rule 6): the two sides of the line, the
//! inner corners where the offsets cross, the outer corners mitred or bevelled past the limit. Each value is worked by hand
//! from the rule, not read from the crate.

use draw::stroke::{stroke_contour, Point};

/// Twice the signed area of a closed polygon, by the shoelace formula.
fn area2(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| points[i].0 * points[(i + 1) % n].1 - points[(i + 1) % n].0 * points[i].1)
        .sum()
}

/// The points of the two sub-paths of a stroke, one after the other.
fn all_points(paths: &[Vec<Point>; 2]) -> Vec<Point> {
    paths.iter().flatten().copied().collect()
}

fn near(a: Point, b: Point) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn a_square_of_side_40_with_a_stroke_of_16_has_a_band_of_2560_square_pixels() {
    // the outer square is 56 by 56 and the inner one 24 by 24: 3136 - 576 = 2560, which is 4 * 40 * 16
    let square = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
    let paths = stroke_contour(&square, 8.0, 4.0);
    let total = area2(&paths[0]) + area2(&paths[1]);
    assert!(
        (total.abs() / 2.0 - 2560.0).abs() < 1e-9,
        "the band is {} square pixels, worked 2560",
        total.abs() / 2.0
    );
}

#[test]
fn the_inner_corners_of_a_right_angle_are_where_the_offsets_cross() {
    // the offsets of the sides at 8 pixels cross at (8, 8), (32, 8), (32, 32) and (8, 32) inside the square
    let square = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
    let points = all_points(&stroke_contour(&square, 8.0, 4.0));
    for corner in [(8.0, 8.0), (32.0, 8.0), (32.0, 32.0), (8.0, 32.0)] {
        assert!(
            points.iter().any(|&p| near(p, corner)),
            "no point of the stroke at the inner corner {corner:?}"
        );
    }
}

#[test]
fn the_outer_corners_of_a_right_angle_are_mitred_square() {
    // a right angle has a ratio of sqrt(2), within the limit of 4, so the outer corner is the meeting point 8 beyond each side
    let square = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
    let points = all_points(&stroke_contour(&square, 8.0, 4.0));
    for corner in [(-8.0, -8.0), (48.0, -8.0), (48.0, 48.0), (-8.0, 48.0)] {
        assert!(
            points.iter().any(|&p| near(p, corner)),
            "no point of the stroke at the mitred corner {corner:?}"
        );
    }
}

#[test]
fn a_corner_past_the_limit_is_bevelled_and_not_mitred() {
    // a sharp apex at the origin, its sides 5 units off the axis 100 units away: the ratio is about 20, past the limit of 4,
    // so its outer side is the two offset points 8 from the apex, and its inner side is the apex itself
    let triangle = [(0.0, 0.0), (100.0, -5.0), (100.0, 5.0)];
    let near_apex: Vec<Point> = all_points(&stroke_contour(&triangle, 8.0, 4.0))
        .into_iter()
        .filter(|&(x, y)| (x * x + y * y).sqrt() < 20.0)
        .collect();
    assert!(
        near_apex.iter().all(|&(x, y)| {
            let d = (x * x + y * y).sqrt();
            d.abs() < 1e-9 || (d - 8.0).abs() < 1e-9
        }),
        "a point of the stroke near the apex lies neither on the apex nor 8 from it: {near_apex:?}"
    );
    assert!(
        near_apex
            .iter()
            .filter(|&&(x, y)| ((x * x + y * y).sqrt() - 8.0).abs() < 1e-9)
            .count()
            >= 2,
        "the bevel's two points are missing"
    );
}

#[test]
fn an_inner_side_with_no_crossing_is_left_out_whole() {
    // at the corner (0, 0) the offsets of the side on the right do not cross on the edges: the crossing is (-8, -11.1), past the
    // end of the edge to (0, -8). The corner's offset point (-8, 0) lies 64 / sqrt(640), about 2.53, from the edge from
    // (-24, -8) to (0, 0), which is nearer than the half-width of 8, so the whole side is left out and nothing of it is drawn
    let contour = [(-24.0, -8.0), (0.0, 0.0), (0.0, -8.0), (16.0, 8.0)];
    let [_, right] = stroke_contour(&contour, 8.0, 4.0);
    assert!(
        right.is_empty(),
        "the inner side has no crossing and must be left out, got {} points",
        right.len()
    );
}

/// The triangle with its apex at the origin and its two sides from the apex at the angle `theta`, of length 100: the apex is a
/// corner of interior angle `theta`, whose mitre ratio is 1 / sin(theta / 2).
fn apex_triangle(theta: f64) -> [Point; 3] {
    [(0.0, 0.0), (100.0, 0.0), (100.0 * theta.cos(), 100.0 * theta.sin())]
}

#[test]
fn a_corner_just_below_the_limit_keeps_its_mitre_tip() {
    // the ratio is 3.99, so sin(theta / 2) = 1 / 3.99. The outer sides of the apex meet at (-8 cot(theta / 2), -8), which lies
    // 8 * 3.99 = 31.92 from the apex: the mitre is kept
    let half_turn = (1.0 / 3.99f64).asin();
    let theta = 2.0 * half_turn;
    let tip = (-8.0 * half_turn.cos() / half_turn.sin(), -8.0);
    let points = all_points(&stroke_contour(&apex_triangle(theta), 8.0, 4.0));
    assert!(points.iter().any(|&p| near(p, tip)), "no mitre tip at {tip:?} for the ratio 3.99");
}

#[test]
fn a_corner_just_past_the_limit_is_bevelled_at_its_two_offset_points() {
    // the ratio is 4.01: the outer side of the apex is cut straight between its two offset points, (-8 sin(theta), 8 cos(theta))
    // on the incoming side and (0, -8) on the outgoing one, and has no tip
    let half_turn = (1.0 / 4.01f64).asin();
    let theta = 2.0 * half_turn;
    let points = all_points(&stroke_contour(&apex_triangle(theta), 8.0, 4.0));
    assert!(
        points.iter().any(|&p| near(p, (-8.0 * theta.sin(), 8.0 * theta.cos()))),
        "no bevel point on the incoming side"
    );
    assert!(points.iter().any(|&p| near(p, (0.0, -8.0))), "no bevel point on the outgoing side");
    let tip = (-8.0 * half_turn.cos() / half_turn.sin(), -8.0);
    assert!(points.iter().all(|&p| !near(p, tip)), "a mitre tip is drawn past the limit");
}
