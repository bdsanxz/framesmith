//! The outlines of a shape drawn in part or in dashes. See `docs/spec/native-notation.md`,
//! section 12.

use vello_cpu::kurbo::{BezPath, CubicBez, Line, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, QuadBez};

/// How closely a length along a curve is worked out, in the outline's own units.
const ACCURACY: f64 = 1e-4;

/// One outline of a path: where it starts, its pieces in order, and whether it is closed.
struct Outline {
    start: Point,
    pieces: Vec<PathSeg>,
    closed: bool,
}

/// The outlines a path holds: each `M` starts one.
fn outlines(path: &BezPath) -> Vec<Outline> {
    let mut found: Vec<Outline> = Vec::new();
    let mut last = Point::ZERO;
    for element in path.elements() {
        let piece = match *element {
            PathEl::MoveTo(to) => {
                found.push(Outline {
                    start: to,
                    pieces: Vec::new(),
                    closed: false,
                });
                last = to;
                continue;
            }
            PathEl::LineTo(to) => PathSeg::Line(Line::new(last, to)),
            PathEl::QuadTo(by, to) => PathSeg::Quad(QuadBez::new(last, by, to)),
            PathEl::CurveTo(by, then, to) => PathSeg::Cubic(CubicBez::new(last, by, then, to)),
            PathEl::ClosePath => {
                let Some(outline) = found.last_mut() else { continue };
                outline.closed = true;
                let back = PathSeg::Line(Line::new(last, outline.start));
                last = outline.start;
                if back.start() == back.end() {
                    continue;
                }
                back
            }
        };
        last = piece.end();
        if let Some(outline) = found.last_mut() {
            outline.pieces.push(piece);
        }
    }
    found
}

/// The length of each outline of a path, in the order they are written.
pub fn lengths(path: &BezPath) -> Vec<f64> {
    outlines(path)
        .iter()
        .map(|outline| outline.pieces.iter().map(|piece| piece.arclen(ACCURACY)).sum())
        .collect()
}

/// Each outline of `path` from its start to `share` of its own length. A share of 1 or more
/// gives the path as it is; a share of 0 or less gives nothing.
pub fn drawn(path: &BezPath, share: f64) -> BezPath {
    if share >= 1.0 {
        return path.clone();
    }
    let mut part = BezPath::new();
    if share <= 0.0 {
        return part;
    }
    for outline in outlines(path) {
        let whole: f64 = outline.pieces.iter().map(|piece| piece.arclen(ACCURACY)).sum();
        let mut left = whole * share;
        if left <= 0.0 {
            continue;
        }
        part.move_to(outline.start);
        for piece in &outline.pieces {
            let length = piece.arclen(ACCURACY);
            if length <= left {
                part.push(piece.as_path_el());
                left -= length;
            } else {
                let until = piece.inv_arclen(left, ACCURACY);
                part.push(piece.subsegment(0.0..until).as_path_el());
                break;
            }
        }
    }
    part
}

/// `path` as a row of dashes: `dash` holds pairs of lengths, a dash and then a gap, repeated
/// along each outline from its start.
pub fn dashed(path: &BezPath, dash: &[f64]) -> BezPath {
    let mut row = BezPath::new();
    // Each outline is dashed on its own, so that every one begins with a whole dash.
    for outline in outlines(path) {
        let mut one = BezPath::new();
        one.move_to(outline.start);
        for piece in &outline.pieces {
            one.push(piece.as_path_el());
        }
        row.extend(vello_cpu::kurbo::dash(one.iter(), 0.0, dash));
    }
    row
}
