//! 3D turn projection (Sprint 7 motion, CAP-005).
//!
//! A visual element turns as a rigid card about the horizontal and
//! vertical axes through its `origin` point. `rotation-x` tips the card
//! around the horizontal axis (positive brings the bottom edge toward
//! the viewer, like nodding forward); `rotation-y` turns it around the
//! vertical axis (positive sends the right edge away from the viewer).
//! `perspective` foreshortens by true perspective division with the
//! viewer `p` pixels in front of the card plane; `None` renders flat
//! (orthographic foreshortening by the axis cosines only).
//!
//! The forward map projects element-space points to the screen; the
//! inverse map feeds the renderer's sampling loops (every output pixel
//! pulls its source texel through the inverse). Both are closed-form
//! and deterministic — no iteration, no worker dependence. Edge-on
//! cards (zero projected area) draw nothing, and rays parallel to the
//! card (the horizon line) sample nothing.

/// One rigid-card turn: precomputed axis cosines/sines, an optional
/// viewer distance, and the origin in element-space pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    crx: f64,
    srx: f64,
    cry: f64,
    sry: f64,
    perspective: Option<f64>,
    ox: f64,
    oy: f64,
}

impl Turn {
    /// Build the turn of an element: `rx_deg`/`ry_deg` in degrees (the
    /// evaluated angles, static plus animated), `perspective` in pixels
    /// (`None` for flat), `ox`/`oy` the origin in element-space pixels.
    /// Returns `None` for the identity (both angles zero or
    /// non-finite): the caller then draws through the old fast path
    /// byte-identically, whatever the perspective or origin.
    pub fn new(
        rx_deg: f64,
        ry_deg: f64,
        perspective: Option<f64>,
        ox: f64,
        oy: f64,
    ) -> Option<Self> {
        if (rx_deg == 0.0 || !rx_deg.is_finite()) && (ry_deg == 0.0 || !ry_deg.is_finite()) {
            return None;
        }
        let (srx, crx) = rx_deg.to_radians().sin_cos();
        let (sry, cry) = ry_deg.to_radians().sin_cos();
        Some(Turn {
            crx,
            srx,
            cry,
            sry,
            perspective: perspective.filter(|p| p.is_finite() && *p > 0.0),
            ox,
            oy,
        })
    }

    /// Project one element-space point to the screen.
    pub fn forward(&self, ux: f64, uy: f64) -> (f64, f64) {
        let dx = ux - self.ox;
        let dy = uy - self.oy;
        let x2 = dx * self.cry + dy * self.srx * self.sry;
        let y2 = dy * self.crx;
        let z2 = -dx * self.sry + dy * self.srx * self.cry;
        let s = match self.perspective {
            Some(p) => {
                let d = p - z2;
                if !d.is_finite() || d == 0.0 {
                    return (f64::NAN, f64::NAN);
                }
                (p / d).clamp(-1e4, 1e4)
            }
            None => 1.0,
        };
        (self.ox + x2 * s, self.oy + y2 * s)
    }

    /// Pull one screen point back into element space. Returns `None`
    /// for rays parallel to the card (the horizon line), which sample
    /// nothing.
    pub fn inverse(&self, qx: f64, qy: f64) -> Option<(f64, f64)> {
        const EPS: f64 = 1e-12;
        let bx = qx - self.ox;
        let by = qy - self.oy;
        let Some(p) = self.perspective else {
            // Flat: [cry, sry*srx; 0, crx] [dx,dy] = [bx,by].
            let det = self.cry * self.crx;
            if det.abs() < EPS || !det.is_finite() {
                return None;
            }
            if self.crx.abs() < EPS || self.cry.abs() < EPS {
                return None;
            }
            let dy = by / self.crx;
            let dx = (bx - self.sry * self.srx * dy) / self.cry;
            if !dx.is_finite() || !dy.is_finite() {
                return None;
            }
            return Some((self.ox + dx, self.oy + dy));
        };
        let ax = bx / p;
        let ay = by / p;
        // M [dx,dy,d] = [0,0,p] with
        // M = [[cry, sry*srx, -ax],[0, crx, -ay],[-sry, cry*srx, 1]].
        let (a11, a12, a13) = (self.cry, self.sry * self.srx, -ax);
        let (a22, a23) = (self.crx, -ay);
        let (a31, a32) = (-self.sry, self.cry * self.srx);
        let det = a11 * (a22 - a23 * a32) - a12 * (0.0 - a23 * a31) + a13 * (0.0 - a22 * a31);
        if det.abs() < EPS || !det.is_finite() {
            return None;
        }
        // Cramer's rule for dx (column 0 replaced by [0,0,p]) and dy
        // (column 1 replaced by [0,0,p]). The column-1 minor's outer
        // terms vanish, leaving dy = -a11*a23*p / det.
        let dx = (-a12 * (0.0 - a23 * p) + a13 * (0.0 - a22 * p)) / det;
        let dy = (a11 * (0.0 - a23 * p)) / det;
        if !dx.is_finite() || !dy.is_finite() {
            return None;
        }
        Some((self.ox + dx, self.oy + dy))
    }

    /// Projected corners of an element rect plus its screen bounds
    /// `(lo_x, lo_y, hi_x, hi_y)`. Returns `None` when the projected
    /// area is degenerate (an edge-on card draws nothing) or any
    /// corner escapes to infinity (the card crosses the viewer).
    pub fn quad(
        &self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    ) -> Option<([(f64, f64); 4], (f64, f64, f64, f64))> {
        let c = [
            self.forward(x0, y0),
            self.forward(x1, y0),
            self.forward(x1, y1),
            self.forward(x0, y1),
        ];
        if c.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return None;
        }
        if c.iter().any(|(x, y)| x.abs() > 1e6 || y.abs() > 1e6) {
            return None;
        }
        // Shoelace area of the projected quad.
        let mut area = 0.0;
        for i in 0..4 {
            let (x0, y0) = c[i];
            let (x1, y1) = c[(i + 1) % 4];
            area += x0 * y1 - x1 * y0;
        }
        if area.abs() < 1e-9 {
            return None;
        }
        let lo_x = c.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let lo_y = c.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
        let hi_x = c.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
        let hi_y = c.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
        Some((c, (lo_x, lo_y, hi_x, hi_y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(rx: f64, ry: f64, p: Option<f64>, ox: f64, oy: f64) -> Turn {
        Turn::new(rx, ry, p, ox, oy).expect("non-identity turn")
    }

    #[test]
    fn identity_angles_yield_no_turn() {
        assert!(Turn::new(0.0, 0.0, Some(500.0), 10.0, 10.0).is_none());
        assert!(Turn::new(f64::NAN, 0.0, None, 0.0, 0.0).is_none());
        assert!(Turn::new(0.0, f64::INFINITY, None, 0.0, 0.0).is_none());
        assert!(Turn::new(1.0, 0.0, None, 0.0, 0.0).is_some());
        assert!(Turn::new(0.0, -45.0, None, 0.0, 0.0).is_some());
    }

    #[test]
    fn flat_turn_foreshortens_by_axis_cosines_about_origin() {
        // Card (0,0)-(100,40), center origin, rx=60 flat: the vertical
        // extent scales by cos(60) = 0.5 about y=20; x is untouched.
        let t = turn(60.0, 0.0, None, 50.0, 20.0);
        let (qx, qy) = t.forward(0.0, 0.0);
        assert!((qx - 0.0).abs() < 1e-9, "x untouched, got {qx}");
        assert!((qy - 10.0).abs() < 1e-9, "top foreshortens to 10, got {qy}");
        let (qx, qy) = t.forward(100.0, 40.0);
        assert!((qx - 100.0).abs() < 1e-9, "got {qx}");
        assert!(
            (qy - 30.0).abs() < 1e-9,
            "bottom foreshortens to 30, got {qy}"
        );
        // ry=60 flat about the same origin: horizontal extent halves.
        let t = turn(0.0, 60.0, None, 50.0, 20.0);
        let (qx, qy) = t.forward(0.0, 0.0);
        assert!(
            (qx - 25.0).abs() < 1e-9,
            "left foreshortens to 25, got {qx}"
        );
        assert!((qy - 0.0).abs() < 1e-9, "got {qy}");
    }

    #[test]
    fn top_origin_collapses_differently_than_center() {
        // rx=90 flat: the card collapses to the origin line. Through
        // the top edge (origin y=0) the line sits at y=0; through the
        // center (y=20) it sits at y=20.
        let top = turn(90.0, 0.0, None, 50.0, 0.0);
        let quad = top.quad(0.0, 0.0, 100.0, 40.0);
        assert!(quad.is_none(), "edge-on through top draws nothing");
        let flat = turn(89.0, 0.0, None, 50.0, 0.0);
        let (_, b) = flat.quad(0.0, 0.0, 100.0, 40.0).expect("sliver draws");
        assert!((b.1 - 0.0).abs() < 0.05, "sliver hugs the top, got {:?}", b);
        assert!((b.3 - 40.0 * 89.0f64.to_radians().cos()).abs() < 0.05);
        let center = turn(89.0, 0.0, None, 50.0, 20.0);
        let (_, b) = center.quad(0.0, 0.0, 100.0, 40.0).expect("sliver draws");
        let half = 20.0 * 89.0f64.to_radians().cos();
        assert!((b.1 - (20.0 - half)).abs() < 0.05, "got {:?}", b);
        assert!((b.3 - (20.0 + half)).abs() < 0.05, "got {:?}", b);
    }

    #[test]
    fn perspective_makes_a_trapezoid() {
        // ry=45 with p=200 about the center: the right (far) edge
        // shrinks, the left (near) edge grows — a trapezoid, not the
        // flat rectangle.
        let t = turn(0.0, 45.0, Some(200.0), 50.0, 20.0);
        t.quad(0.0, 0.0, 100.0, 40.0).expect("turned card projects");
        let left_top = t.forward(0.0, 0.0);
        let right_top = t.forward(100.0, 0.0);
        let left_bot = t.forward(0.0, 40.0);
        let right_bot = t.forward(100.0, 40.0);
        let left_h = left_bot.1 - left_top.1;
        let right_h = right_bot.1 - right_top.1;
        assert!(
            left_h > right_h + 1.0,
            "near edge {left_h} exceeds far edge {right_h}"
        );
        // Hand-computed: dx=±50, ry=45: X2=±35.355, Z2=∓35.355.
        // The left edge sits toward the viewer (Z2=+35.355), so
        // s_left = 200/(200-35.355) > 1; the right edge recedes.
        let s_left = 200.0 / (200.0 - 35.35533905932738);
        let s_right = 200.0 / (200.0 + 35.35533905932738);
        assert!((left_top.0 - (50.0 - 35.35533905932738 * s_left)).abs() < 1e-6);
        assert!((right_top.0 - (50.0 + 35.35533905932738 * s_right)).abs() < 1e-6);
        assert!((left_h - 40.0 * s_left).abs() < 1e-6, "got {left_h}");
        assert!((right_h - 40.0 * s_right).abs() < 1e-6, "got {right_h}");
    }

    #[test]
    fn inverse_round_trips_forward() {
        let t = turn(30.0, 40.0, Some(300.0), 50.0, 20.0);
        for (ux, uy) in [(0.0, 0.0), (100.0, 40.0), (25.0, 33.0), (50.0, 20.0)] {
            let (qx, qy) = t.forward(ux, uy);
            let (ix, iy) = t.inverse(qx, qy).expect("round trip inverts");
            assert!((ix - ux).abs() < 1e-6, "x round trip {ux} -> {ix}");
            assert!((iy - uy).abs() < 1e-6, "y round trip {uy} -> {iy}");
        }
        let flat = turn(-25.0, 35.0, None, 10.0, 10.0);
        for (ux, uy) in [(0.0, 0.0), (70.0, 22.0)] {
            let (qx, qy) = flat.forward(ux, uy);
            let (ix, iy) = flat.inverse(qx, qy).expect("flat round trip");
            assert!((ix - ux).abs() < 1e-9);
            assert!((iy - uy).abs() < 1e-9);
        }
    }
}
