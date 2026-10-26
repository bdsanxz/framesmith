//! A turn in space (`docs/spec/render-command.md`, section 4.5, rule 10): the projective map that takes a point of an
//! element's flat picture to the frame, its inverse, and when the element is edge-on.

use layout::Rect;

use crate::Map;

/// The turn of an element: its rectangle in the frame before any map, the point it turns about (its origin, in the frame before any map; [`Turn::about`] takes it by
/// the map the element is drawn with, to the point `Q`), the eye's distance `perspective` above it, and the angles about the
/// level line (`rotate_x`) and the upright line (`rotate_y`), in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    pub rect: Rect,
    pub origin: (f64, f64),
    pub perspective: f64,
    pub rotate_x: f64,
    pub rotate_y: f64,
}

/// The largest `|cos a × cos b|` at which an element is edge-on.
const EDGE_ON: f64 = 1e-12;

impl Turn {
    /// The turn about the origin taken by `map`: the point where the origin is drawn.
    pub fn about(self, map: Map) -> Turn {
        let (x, y) = map.apply(self.origin.0, self.origin.1);
        Turn { origin: (x, y), ..self }
    }

    /// Whether the turn leaves the picture as it is: both angles exactly 0.
    pub fn is_none(&self) -> bool {
        self.rotate_x == 0.0 && self.rotate_y == 0.0
    }

    /// The matrix of the turn on homogeneous points relative to the origin: `(u, v, 1)` goes to `(X P, Y P, P − Z)`.
    fn matrix(&self) -> [[f64; 3]; 3] {
        let (sin_a, cos_a) = self.rotate_x.to_radians().sin_cos();
        let (sin_b, cos_b) = self.rotate_y.to_radians().sin_cos();
        let p = self.perspective;
        [
            [p * cos_b, p * sin_a * sin_b, 0.0],
            [0.0, p * cos_a, 0.0],
            [sin_b, -sin_a * cos_b, p],
        ]
    }

    /// `P − Z` of the point `point` of the flat picture: above 0 when the turn leaves it in front of the eye.
    pub fn depth(&self, point: (f64, f64)) -> f64 {
        let [_, _, row] = self.matrix();
        let (u, v) = (point.0 - self.origin.0, point.1 - self.origin.1);
        row[0] * u + row[1] * v + row[2]
    }

    /// Where the turn draws the point `point` of the flat picture, when it stays in front of the eye.
    pub fn apply(&self, point: (f64, f64)) -> Option<(f64, f64)> {
        let m = self.matrix();
        let (u, v) = (point.0 - self.origin.0, point.1 - self.origin.1);
        let w = m[2][0] * u + m[2][1] * v + m[2][2];
        (w > 0.0).then(|| {
            (
                self.origin.0 + (m[0][0] * u + m[0][1] * v) / w,
                self.origin.1 + (m[1][0] * u + m[1][1] * v) / w,
            )
        })
    }

    /// Whether the turn draws the element under `map`: it is not edge-on, and no corner of its rectangle, taken by `map`,
    /// comes to the eye or behind it. `self` is the turn about the origin already taken by `map`.
    pub fn draws(&self, map: Map) -> bool {
        let (left, top) = (f64::from(self.rect.left), f64::from(self.rect.top));
        let (right, bottom) = (left + f64::from(self.rect.width), top + f64::from(self.rect.height));
        !self.is_edge_on()
            && [(left, top), (right, top), (left, bottom), (right, bottom)]
                .into_iter()
                .all(|(x, y)| self.depth(map.apply(x, y)) > 0.0)
    }

    /// Whether the element is edge-on: `|cos a × cos b|` at most `10⁻¹²`.
    pub fn is_edge_on(&self) -> bool {
        (self.rotate_x.to_radians().cos() * self.rotate_y.to_radians().cos()).abs() <= EDGE_ON
    }

    /// The inverse of the turn, on homogeneous points of the frame relative to the origin: `(sx, sy, 1)` goes to a
    /// multiple of `(u, v, 1)`. `None` when the turn has no inverse (edge-on).
    pub fn inverse(&self) -> Option<[[f64; 3]; 3]> {
        if self.is_edge_on() {
            return None;
        }
        let m = self.matrix();
        let cofactor = |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
        let adjugate = [
            [cofactor(1, 2, 1, 2), -cofactor(0, 2, 1, 2), cofactor(0, 1, 1, 2)],
            [-cofactor(1, 2, 0, 2), cofactor(0, 2, 0, 2), -cofactor(0, 1, 0, 2)],
            [cofactor(1, 2, 0, 1), -cofactor(0, 2, 0, 1), cofactor(0, 1, 0, 1)],
        ];
        let determinant = m[0][0] * adjugate[0][0] + m[0][1] * adjugate[1][0] + m[0][2] * adjugate[2][0];
        (determinant != 0.0 && determinant.is_finite()).then(|| adjugate.map(|row| row.map(|value| value / determinant)))
    }

    /// The point of the flat picture that the turn draws at `point` of the frame, when that point is in front of the eye.
    pub fn back(&self, inverse: &[[f64; 3]; 3], point: (f64, f64)) -> Option<(f64, f64)> {
        let (sx, sy) = (point.0 - self.origin.0, point.1 - self.origin.1);
        let h = inverse.map(|row| row[0] * sx + row[1] * sy + row[2]);
        if h[2] == 0.0 {
            return None;
        }
        let flat = (self.origin.0 + h[0] / h[2], self.origin.1 + h[1] / h[2]);
        (self.depth(flat) > 0.0).then_some(flat)
    }
}
