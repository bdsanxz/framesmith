//! The ten easing curves. Written by us: see the easing decision in `PROGRESS.md`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

impl Ease {
    pub const ALL: [Ease; 10] = [
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
    ];

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
        }
    }

    pub fn from_name(name: &str) -> Option<Ease> {
        Ease::ALL.into_iter().find(|ease| ease.name() == name)
    }

    /// The eased progress for `p` in 0 to 1. Exactly 0 at 0 and exactly 1 at 1.
    pub fn apply(self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
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
