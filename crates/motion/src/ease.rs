//! The easing curves (`docs/spec/motion.md`, "Easing names", plus the Sprint 7 motion set): the nineteen named curves,
//! `out-back(n)`, and the full curve set (`cubic-bezier`, `steps`, `spring`, `bounce`, `elastic`, `wiggle`, `hold`).
//! Each is written by us from its definition; no easing library is used.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Default stiffness of a bare `spring`: the angular frequency is its square root.
pub const SPRING_DEFAULT_STIFFNESS: f64 = 100.0;
/// Default damping of a bare `spring`.
pub const SPRING_DEFAULT_DAMPING: f64 = 5.0;
/// Default wiggle count of a bare `wiggle`.
pub const WIGGLE_DEFAULT_COUNT: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ease {
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InExpo,
    OutExpo,
    InOutExpo,
    /// `out-back(n)`: runs past its end by an amount `n` sets, and settles back to it.
    OutBack(f64),
    /// `cubic-bezier(x1,y1,x2,y2)`: the CSS timing function; `x1` and `x2` run 0 to 1.
    CubicBezier(f64, f64, f64, f64),
    /// `steps(n)`: `n` equal held intervals, jumping at each interval end.
    Steps(u32),
    /// `spring(stiffness,damping)`: a damped spring settling to its end; bare `spring` uses the defaults.
    Spring(f64, f64),
    /// `bounce`: the standard four-segment ease-out bounce.
    Bounce,
    /// `elastic`: the standard ease-out elastic snap.
    Elastic,
    /// `wiggle(n)`: `n` decaying sine wiggles about the diagonal; bare `wiggle` uses the default count.
    Wiggle(u32),
    /// `hold`: keeps the prior value until the next stop, then jumps.
    Hold,
}

impl Ease {
    /// The nineteen named curves, in the order of the spec.
    pub const ALL: [Ease; 19] = [
        Ease::Linear,
        Ease::InQuad,
        Ease::OutQuad,
        Ease::InOutQuad,
        Ease::InCubic,
        Ease::OutCubic,
        Ease::InOutCubic,
        Ease::InQuart,
        Ease::OutQuart,
        Ease::InOutQuart,
        Ease::InQuint,
        Ease::OutQuint,
        Ease::InOutQuint,
        Ease::InSine,
        Ease::OutSine,
        Ease::InOutSine,
        Ease::InExpo,
        Ease::OutExpo,
        Ease::InOutExpo,
    ];

    /// The curve's name; `out-back` for every `out-back(n)`.
    pub fn name(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::InQuad => "in-quad",
            Ease::OutQuad => "out-quad",
            Ease::InOutQuad => "in-out-quad",
            Ease::InCubic => "in-cubic",
            Ease::OutCubic => "out-cubic",
            Ease::InOutCubic => "in-out-cubic",
            Ease::InQuart => "in-quart",
            Ease::OutQuart => "out-quart",
            Ease::InOutQuart => "in-out-quart",
            Ease::InQuint => "in-quint",
            Ease::OutQuint => "out-quint",
            Ease::InOutQuint => "in-out-quint",
            Ease::InSine => "in-sine",
            Ease::OutSine => "out-sine",
            Ease::InOutSine => "in-out-sine",
            Ease::InExpo => "in-expo",
            Ease::OutExpo => "out-expo",
            Ease::InOutExpo => "in-out-expo",
            Ease::OutBack(_) => "out-back",
            Ease::CubicBezier(..) => "cubic-bezier",
            Ease::Steps(_) => "steps",
            Ease::Spring(..) => "spring",
            Ease::Bounce => "bounce",
            Ease::Elastic => "elastic",
            Ease::Wiggle(_) => "wiggle",
            Ease::Hold => "hold",
        }
    }

    /// The named curve of this name, if any: exactly the nineteen historic names. Parameterized curves
    /// (`out-back(n)` and the Sprint 7 set) are not names: the notation reads them and gives the matching
    /// variant directly.
    pub fn from_name(name: &str) -> Option<Ease> {
        Ease::ALL.into_iter().find(|ease| ease.name() == name)
    }

    /// The eased progress for `p`, a value outside 0 to 1 taken as the nearer end. Exactly 0 at 0 and exactly 1 at 1.
    pub fn apply(self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        if p == 0.0 {
            return 0.0;
        }
        if p == 1.0 {
            return 1.0;
        }
        match self {
            Ease::Linear => p,
            Ease::InQuad => power_in(p, 2),
            Ease::OutQuad => power_out(p, 2),
            Ease::InOutQuad => power_in_out(p, 2),
            Ease::InCubic => power_in(p, 3),
            Ease::OutCubic => power_out(p, 3),
            Ease::InOutCubic => power_in_out(p, 3),
            Ease::InQuart => power_in(p, 4),
            Ease::OutQuart => power_out(p, 4),
            Ease::InOutQuart => power_in_out(p, 4),
            Ease::InQuint => power_in(p, 5),
            Ease::OutQuint => power_out(p, 5),
            Ease::InOutQuint => power_in_out(p, 5),
            Ease::InSine => 1.0 - (p * PI / 2.0).cos(),
            Ease::OutSine => (p * PI / 2.0).sin(),
            Ease::InOutSine => (1.0 - (p * PI).cos()) / 2.0,
            Ease::InExpo => 2f64.powf(10.0 * p - 10.0),
            Ease::OutExpo => 1.0 - 2f64.powf(-10.0 * p),
            Ease::InOutExpo => {
                if p < 0.5 {
                    2f64.powf(20.0 * p - 10.0) / 2.0
                } else {
                    (2.0 - 2f64.powf(10.0 - 20.0 * p)) / 2.0
                }
            }
            Ease::OutBack(n) => {
                let q = p - 1.0;
                q * q * ((n + 1.0) * q + n) + 1.0
            }
            Ease::CubicBezier(x1, y1, x2, y2) => cubic_bezier(p, x1, y1, x2, y2),
            // Validation only builds counts from 1 up; a directly built 0 falls back to linear, never NaN.
            Ease::Steps(n) if n == 0 => p,
            Ease::Steps(n) => (p * n as f64).floor() / n as f64,
            Ease::Spring(stiffness, damping) => spring(p, stiffness, damping),
            Ease::Bounce => bounce_out(p),
            Ease::Elastic => elastic_out(p),
            Ease::Wiggle(n) => wiggle(p, n),
            Ease::Hold => 0.0,
        }
    }
}

/// The CSS `cubic-bezier` timing function: `x1` and `x2` run 0 to 1, `y1` and `y2` are unbounded. The x
/// coordinate is inverted for the progress by Newton iteration with a bisection fallback (a fixed pass
/// count, so every platform reaches the same value), then the y coordinate is read at the solved time.
fn cubic_bezier(p: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    fn coord(t: f64, a: f64, b: f64) -> f64 {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    }
    fn slope(t: f64, a: f64, b: f64) -> f64 {
        let u = 1.0 - t;
        3.0 * u * u * a + 6.0 * u * t * (b - a) + 3.0 * t * t * (1.0 - b)
    }
    let mut t = p;
    for _ in 0..8 {
        let x = coord(t, x1, x2) - p;
        if x.abs() < 1e-9 {
            return coord(t, y1, y2);
        }
        let d = slope(t, x1, x2);
        if d.abs() < 1e-9 {
            break;
        }
        t = (t - x / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    t = p;
    for _ in 0..32 {
        let x = coord(t, x1, x2);
        if (x - p).abs() < 1e-9 {
            break;
        }
        if x < p {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.0;
    }
    coord(t, y1, y2)
}

/// A damped spring settling to its end: angular frequency `sqrt(stiffness)`, decay `damping`. Exactly 0
/// at 0 by construction (and exactly 1 at 1 through the caller's end rule).
fn spring(p: f64, stiffness: f64, damping: f64) -> f64 {
    let w = stiffness.sqrt();
    1.0 - (-damping * p).exp() * ((w * p).cos() + damping / w * (w * p).sin())
}

/// The standard four-segment ease-out bounce: three decaying parabolic rebounds after the first fall.
fn bounce_out(p: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if p < 1.0 / D1 {
        N1 * p * p
    } else if p < 2.0 / D1 {
        let q = p - 1.5 / D1;
        N1 * q * q + 0.75
    } else if p < 2.5 / D1 {
        let q = p - 2.25 / D1;
        N1 * q * q + 0.9375
    } else {
        let q = p - 2.625 / D1;
        N1 * q * q + 0.984375
    }
}

/// The standard ease-out elastic snap: an exponentially decaying sine about the end value.
fn elastic_out(p: f64) -> f64 {
    const C: f64 = 2.0 * PI / 3.0;
    2f64.powf(-10.0 * p) * ((p * 10.0 - 0.75) * C).sin() + 1.0
}

/// `n` decaying sine wiggles about the diagonal, fading to the exact end.
fn wiggle(p: f64, n: u32) -> f64 {
    p + (2.0 * PI * n as f64 * p).sin() * (1.0 - p) / 4.0
}

fn power_in(p: f64, n: i32) -> f64 {
    p.powi(n)
}

fn power_out(p: f64, n: i32) -> f64 {
    1.0 - (1.0 - p).powi(n)
}

fn power_in_out(p: f64, n: i32) -> f64 {
    if p < 0.5 {
        2f64.powi(n - 1) * p.powi(n)
    } else {
        1.0 - (2.0 - 2.0 * p).powi(n) / 2.0
    }
}
