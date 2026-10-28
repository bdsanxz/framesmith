//! The expansion of a stroke along closed contours, and the flattening of their quadratic pieces (`docs/spec/text.md`, section
//! 6.6, rules 6 and 10). A contour's line is two offsets, one on each side of it: the offset on the left is traversed with the
//! contour and the offset on the right is traversed back, so the two enclose the line. Their exact signed areas are summed by
//! the coverage rule of the renderers, so an offset must never loop back over itself.

/// A point in pixels.
pub type Point = (f64, f64);

/// The number of equal steps of the parameter a quadratic piece is cut into: the least whole number for which every chord
/// lies within `tolerance` of its curve (`docs/spec/text.md`, section 6.6, rule 10).
pub fn steps(start: Point, control: Point, end: Point, tolerance: f64) -> usize {
    let dx = start.0 - 2.0 * control.0 + end.0;
    let dy = start.1 - 2.0 * control.1 + end.1;
    // the error of a chord over a step h is |second derivative| h^2 / 8, and the second derivative is 2 (P0 - 2 P1 + P2)
    let needed = ((dx * dx + dy * dy).sqrt() / (4.0 * tolerance)).sqrt().ceil();
    (needed as usize).max(1)
}

/// The points that follow `start` along a quadratic piece, cut into `steps` equal steps of its parameter, the last of them
/// `end` itself.
pub fn flatten_quadratic(start: Point, control: Point, end: Point, tolerance: f64, out: &mut Vec<Point>) {
    let count = steps(start, control, end, tolerance);
    for k in 1..=count {
        let t = k as f64 / count as f64;
        let s = 1.0 - t;
        out.push((
            s * s * start.0 + 2.0 * s * t * control.0 + t * t * end.0,
            s * s * start.1 + 2.0 * s * t * control.1 + t * t * end.1,
        ));
    }
}

/// The two sub-paths that enclose the line of a closed contour: the offset on the left, then the offset on the right traversed
/// back. `points` are the contour's flattened points, without a repeated last point, and `half` is half the width of the line;
/// `limit` is the mitre limit as a ratio of the width. A contour with fewer than three distinct points has no line.
pub fn stroke_contour(points: &[Point], half: f64, limit: f64) -> [Vec<Point>; 2] {
    let points = distinct(points);
    if points.len() < 3 {
        return [Vec::new(), Vec::new()];
    }
    // a side that folds back on itself is not drawn: one of its points lies nearer than the half-width to the contour
    // (`docs/spec/text.md`, section 6.6, rule 6)
    let keep = |side: Vec<Point>| {
        if side.iter().all(|&p| distance_to_contour(p, &points) >= half - 1e-9) {
            side
        } else {
            Vec::new()
        }
    };
    let left = keep(offset(&points, half, 1.0, limit));
    let mut right = keep(offset(&points, half, -1.0, limit));
    right.reverse();
    [left, right]
}

/// The distance from a point to the nearest edge of a closed contour.
fn distance_to_contour(p: Point, points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            ((p.0 - (a.0 + t * dx)).powi(2) + (p.1 - (a.1 + t * dy)).powi(2)).sqrt()
        })
        .fold(f64::INFINITY, f64::min)
}

/// The sub-paths of the lines of several closed contours, two for each contour.
pub fn stroke_contours(contours: &[Vec<Point>], half: f64, limit: f64) -> Vec<Vec<Point>> {
    contours
        .iter()
        .flat_map(|points| stroke_contour(points, half, limit))
        .filter(|path| !path.is_empty())
        .collect()
}

/// The points of a closed contour with no repeated point, a point repeated being the same point as the one before it.
fn distinct(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &point in points {
        if out
            .last()
            .is_none_or(|last| (last.0 - point.0).abs() > 1e-12 || (last.1 - point.1).abs() > 1e-12)
        {
            out.push(point);
        }
    }
    while out.len() > 1 && {
        let (first, last) = (out[0], out[out.len() - 1]);
        (first.0 - last.0).abs() <= 1e-12 && (first.1 - last.1).abs() <= 1e-12
    } {
        out.pop();
    }
    out
}

/// One side of a closed contour's line, traversed with the contour. `side` is 1 for the left and -1 for the right.
///
/// At each corner, with `d0` the direction of the edge before it and `d1` the direction of the edge after it, `n0` and `n1`
/// the left normals of those edges and `cross` the cross product of `d0` and `d1`, the offset side turns outward when `cross * side`
/// is below zero and inward otherwise. The two offset points of the corner are `P + half * side * n0` and `P + half * side * n1`.
/// Outward, the corner is a mitre at the meeting point of the two offset edges when its ratio, one over the cosine of half the
/// turn, is at most `limit`, and a bevel of the two points otherwise. Inward, the offset edges cross at a point which is the
/// corner of that side when the crossing lies on both edges; otherwise the side runs from the end of the first offset point to
/// the contour's corner and out to the second. A straight run, with no turn, has its two offset points coincide.
fn offset(points: &[Point], half: f64, side: f64, limit: f64) -> Vec<Point> {
    let n = points.len();
    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let d = ((b.0 - a.0) / length, (b.1 - a.1) / length);
        edges.push((d, length));
    }
    let mut out = Vec::with_capacity(2 * n);
    for i in 0..n {
        let p = points[i];
        let ((d0, len0), (d1, _)) = (edges[(i + n - 1) % n], edges[i]);
        let n0 = (-d0.1, d0.0);
        let n1 = (-d1.1, d1.0);
        let a = (p.0 + half * side * n0.0, p.1 + half * side * n0.1);
        let b = (p.0 + half * side * n1.0, p.1 + half * side * n1.1);
        let cross = d0.0 * d1.1 - d0.1 * d1.0;
        let dot = d0.0 * d1.0 + d0.1 * d1.1;
        let denom = 1.0 + dot;
        if cross * side < 0.0 {
            // outward: a mitre of ratio 1 / cos(half the turn), or a bevel past the limit
            // the ratio is one over cos(half the turn), and cos(half the turn) squared is (1 + dot) / 2
            if denom > 1e-12 && (2.0 / denom).sqrt() <= limit {
                out.push(meet(p, n0, n1, half * side, denom));
            } else {
                out.push(a);
                out.push(b);
            }
        } else if denom > 1e-12 {
            // inward: the offset edges cross; the crossing is the corner when it lies on both edges
            let x = meet(p, n0, n1, half * side, denom);
            let along_first = (x.0 - p.0) * d0.0 + (x.1 - p.1) * d0.1;
            let along_second = (x.0 - p.0) * d1.0 + (x.1 - p.1) * d1.1;
            let eps = 1e-9;
            if along_first <= eps && along_first >= -len0 - eps && along_second >= -eps && along_second <= edges[i].1 + eps {
                out.push(x);
            } else {
                out.push(a);
                out.push(p);
                out.push(b);
            }
        } else {
            // a reversal, where the edges fold back on each other: the offsets are joined through the corner
            out.push(a);
            out.push(p);
            out.push(b);
        }
    }
    out
}

/// The meeting point of the offset lines of two edges at a corner `p`: `p + scale * (n0 + n1) / (1 + n0 . n1)`.
fn meet(p: Point, n0: Point, n1: Point, scale: f64, denom: f64) -> Point {
    (p.0 + scale * (n0.0 + n1.0) / denom, p.1 + scale * (n0.1 + n1.1) / denom)
}
