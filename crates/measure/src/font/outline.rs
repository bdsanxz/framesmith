//! Turns the centreline pieces of a glyph into the contours of its outline, as
//! `docs/spec/test-font.md`, section 4 sets out: a rectangle for each straight stroke (extended at a
//! corner shared with another straight stroke), a square for each dot, and an annular sector for each
//! arc, its edges cut into pieces of at most 30 degrees with quadratic edges. Every contour runs
//! clockwise, except the hole of a full circle, which runs counter-clockwise.

use super::design::Piece;

/// A point of a contour, in whole units of the em.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
    /// `true` for a point on the outline, `false` for the control point of a quadratic edge.
    pub on_curve: bool,
}

/// A point before rounding: x, y and whether it lies on the outline.
type Raw = (f64, f64, bool);

/// The least and greatest x of the centreline of a glyph, from its pieces, in units of the em.
/// An arc counts at its ends and at each quarter turn of its sweep that lies within it.
pub fn centreline_span(pieces: &[Piece]) -> (f64, f64) {
    let mut xs: Vec<f64> = Vec::new();
    for piece in pieces {
        match *piece {
            Piece::Line { x1, x2, .. } => xs.extend([f64::from(x1), f64::from(x2)]),
            Piece::Arc { cx, r, from, to, .. } => {
                let (lo, hi) = (from.min(to), from.max(to));
                let quarter_turns = (lo.div_euclid(90)..=hi.div_euclid(90) + 1).map(|q| q * 90);
                let angles = quarter_turns.filter(|a| (lo..=hi).contains(a)).chain([from, to]);
                xs.extend(angles.map(|a| f64::from(cx) + f64::from(r) * f64::from(a).to_radians().cos()));
            }
        }
    }
    let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (lo, hi)
}

/// The contours of the outline of a glyph drawn with strokes `stroke` wide, moved sideways by
/// `shift` units and rounded to whole units.
pub fn contours(pieces: &[Piece], stroke: f64, shift: f64) -> Vec<Vec<Point>> {
    let half = stroke / 2.0;
    let mut out: Vec<Vec<Point>> = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        match *piece {
            Piece::Line { x1, y1, x2, y2 } if (x1, y1) == (x2, y2) => {
                out.push(round(clockwise(square(f64::from(x1), f64::from(y1), half)), shift));
            }
            Piece::Line { x1, y1, x2, y2 } => {
                out.push(round(clockwise(rectangle(pieces, index, (x1, y1), (x2, y2), half)), shift));
            }
            Piece::Arc { cx, cy, r, from, to } => {
                let arc = sectors(f64::from(cx), f64::from(cy), f64::from(r), f64::from(from), f64::from(to), half);
                out.extend(arc.into_iter().map(|contour| round(contour, shift)));
            }
        }
    }
    out
}

/// The rectangle of a straight stroke from `a` to `b`. An end is pushed out by half the stroke width
/// when another straight stroke ends at the same point, so that a corner is square on the outside.
fn rectangle(pieces: &[Piece], index: usize, a: (i32, i32), b: (i32, i32), half: f64) -> Vec<Raw> {
    let (ax, ay) = (f64::from(a.0), f64::from(a.1));
    let (bx, by) = (f64::from(b.0), f64::from(b.1));
    let length = (bx - ax).hypot(by - ay);
    let (ux, uy) = ((bx - ax) / length, (by - ay) / length);
    let (nx, ny) = (-uy * half, ux * half);
    let grow_a = if shared_end(pieces, index, a) { half } else { 0.0 };
    let grow_b = if shared_end(pieces, index, b) { half } else { 0.0 };
    let (sx, sy) = (ax - ux * grow_a, ay - uy * grow_a);
    let (ex, ey) = (bx + ux * grow_b, by + uy * grow_b);
    vec![
        (sx + nx, sy + ny, true),
        (ex + nx, ey + ny, true),
        (ex - nx, ey - ny, true),
        (sx - nx, sy - ny, true),
    ]
}

/// True when another straight stroke, not the one at `skip`, has an end at `end`.
fn shared_end(pieces: &[Piece], skip: usize, end: (i32, i32)) -> bool {
    pieces.iter().enumerate().any(|(k, piece)| match *piece {
        Piece::Line { x1, y1, x2, y2 } => k != skip && (x1, y1) != (x2, y2) && ((x1, y1) == end || (x2, y2) == end),
        Piece::Arc { .. } => false,
    })
}

/// The square of a dot of the stroke width, centred on `(x, y)`.
fn square(x: f64, y: f64, half: f64) -> Vec<Raw> {
    vec![
        (x - half, y - half, true),
        (x + half, y - half, true),
        (x + half, y + half, true),
        (x - half, y + half, true),
    ]
}

/// The contours of an arc of radius `r` about `(cx, cy)` from `from` to `to` degrees (`docs/spec/test-font.md`, section 4,
/// rule 3). An arc is one annular sector: its outer edge and its inner edge, each cut into pieces of at most 30 degrees, closed
/// by its two radial ends. So the outline has no corner where the pieces of an arc meet. A full circle has no radial ends: its
/// outer edge is one contour, and its inner edge, the hole, is a second contour that runs the other way, so that a nonzero fill
/// leaves the hole empty.
fn sectors(cx: f64, cy: f64, r: f64, from: f64, to: f64, half: f64) -> Vec<Vec<Raw>> {
    let steps = ((to - from).abs() / 30.0).ceil().max(1.0) as usize;
    let step = (to - from) / steps as f64;
    let angles: Vec<f64> = (0..=steps).map(|k| (from + step * k as f64).to_radians()).collect();
    let mut inner_angles = angles.clone();
    inner_angles.reverse();
    let mut outer = edge(cx, cy, r + half, &angles);
    let mut inner = edge(cx, cy, r - half, &inner_angles);
    if (to - from).abs() >= 360.0 {
        // a closed edge ends on its first point, which the contour closes on
        outer.pop();
        inner.pop();
        let outer = clockwise(outer);
        if area2(&inner) < 0.0 {
            inner.reverse();
        }
        return vec![outer, inner];
    }
    outer.extend(inner);
    vec![clockwise(outer)]
}

/// One edge of an arc at `radius` through `angles` (radians), starting on the outline. Each piece is one quadratic curve whose
/// control point lies on the piece's bisecting radius, at the radius divided by the cosine of half the piece's angle.
fn edge(cx: f64, cy: f64, radius: f64, angles: &[f64]) -> Vec<Raw> {
    let at = |radius: f64, angle: f64| (cx + radius * angle.cos(), cy + radius * angle.sin());
    let start = at(radius, angles[0]);
    let mut points = vec![(start.0, start.1, true)];
    for pair in angles.windows(2) {
        let (a0, a1) = (pair[0], pair[1]);
        let spread = 1.0 / ((a1 - a0) / 2.0).cos();
        let control = at(radius * spread, (a0 + a1) / 2.0);
        let end = at(radius, a1);
        points.push((control.0, control.1, false));
        points.push((end.0, end.1, true));
    }
    points
}

/// The contour with its points in clockwise order: the shoelace sum is negative for a clockwise
/// contour in a y-up plane, and a contour that comes out counter-clockwise is turned round.
fn clockwise(mut contour: Vec<Raw>) -> Vec<Raw> {
    if area2(&contour) > 0.0 {
        contour.reverse();
    }
    contour
}

/// Twice the signed area of a contour by the shoelace sum: negative for a clockwise contour in a y-up plane.
fn area2(contour: &[Raw]) -> f64 {
    let count = contour.len();
    (0..count)
        .map(|i| {
            let (x0, y0, _) = contour[i];
            let (x1, y1, _) = contour[(i + 1) % count];
            x0 * y1 - x1 * y0
        })
        .sum()
}

/// Moves a contour sideways by `shift` and rounds it to whole units.
fn round(contour: Vec<Raw>, shift: f64) -> Vec<Point> {
    contour
        .into_iter()
        .map(|(x, y, on_curve)| Point {
            x: (x + shift).round() as i32,
            y: y.round() as i32,
            on_curve,
        })
        .collect()
}
