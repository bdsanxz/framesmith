//! The easing curves (`docs/spec/motion.md`, "Easing names"): the nineteen named curves and `out-back(n)`. Each is written
//! by us from its definition in that section; no easing library is used.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

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
        }
    }

    /// The named curve of this name, if any. `out-back(n)` is not a name: the notation reads it and gives
    /// [`Ease::OutBack`].
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
        }
    }
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
