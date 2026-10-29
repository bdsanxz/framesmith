//! The true peak of a finished mix (`docs/spec/sound.md`, section 4): the largest absolute value, on the
//! samples and between them, over both channels. Each channel is oversampled four times through an interpolating
//! low-pass filter, as ITU-R BS.1770-4 Annex 2 describes; the filter is our own, every coefficient from the formula
//! the spec gives (decision B88), so no table of numbers stands here.

/// Oversampling of the measure: each sample frame is read at four positions.
pub const PHASES: usize = 4;
/// Taps of the filter in each phase.
pub const TAPS: usize = 12;

/// The limit a mix is kept under: 1 dB under full scale, in linear amplitude.
pub fn limit() -> f64 {
    10f64.powf(-1.0 / 20.0)
}

/// The amount a mix lowered by `gain` (every sample multiplied by `gain`) was lowered, in decibels.
pub fn lowered_db(gain: f64) -> f64 {
    -20.0 * gain.log10()
}

/// The gain that brings `peak` exactly to the limit, or none when the peak is at or under it.
pub fn gain_for(peak: f64) -> Option<f64> {
    if peak > limit() {
        Some(limit() / peak)
    } else {
        None
    }
}

/// The coefficients of the filter: `coefficients()[phase][tap]`, each phase holding its level.
pub fn coefficients() -> [[f64; TAPS]; PHASES] {
    let mut phases = [[0.0; TAPS]; PHASES];
    for (phase, row) in phases.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (tap, slot) in row.iter_mut().enumerate() {
            let distance = (tap as f64 - 5.0) - phase as f64 / PHASES as f64;
            let sinc = if distance == 0.0 {
                1.0
            } else {
                (std::f64::consts::PI * distance).sin() / (std::f64::consts::PI * distance)
            };
            let angle = 2.0 * std::f64::consts::PI * tap as f64 / (TAPS as f64 - 1.0);
            let window = 0.35875 - 0.48829 * angle.cos() + 0.14128 * (2.0 * angle).cos() - 0.01168 * (3.0 * angle).cos();
            *slot = sinc * window;
            sum += *slot;
        }
        for slot in row.iter_mut() {
            *slot /= sum;
        }
    }
    phases
}

/// The true peak of stereo frames pushed through it, two samples (left, then right) a frame. Past the first and
/// last frames the edge frame is repeated, so no invented overshoot enters the measure (`sound.md` section 4).
pub struct Meter {
    coefficients: [[f64; TAPS]; PHASES],
    /// Frames seen, at most twelve: the five repeats of the first frame, then the mix, then repeats of the last.
    window: Vec<[f32; 2]>,
    peak: f64,
}

impl Meter {
    pub fn new() -> Meter {
        Meter {
            coefficients: coefficients(),
            window: Vec::with_capacity(TAPS),
            peak: 0.0,
        }
    }

    /// Pushes stereo frames, two samples (left, then right) a frame.
    pub fn push(&mut self, frames: &[[f32; 2]]) {
        for frame in frames {
            if self.window.is_empty() {
                self.window.resize(5, *frame);
            }
            self.window.push(*frame);
            if self.window.len() == TAPS {
                self.give();
                self.window.remove(0);
            }
        }
    }

    /// The true peak of everything pushed: call after pushing the whole mix, which repeats the last frame past
    /// its end.
    pub fn peak(mut self, last: [f32; 2]) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        for _ in 0..6 {
            self.window.push(last);
            if self.window.len() == TAPS {
                self.give();
                self.window.remove(0);
            }
        }
        self.peak
    }

    fn give(&mut self) {
        for phase in 0..PHASES {
            for channel in 0..2 {
                let mut value = 0.0;
                for tap in 0..TAPS {
                    value += self.window[tap][channel] as f64 * self.coefficients[phase][tap];
                }
                if value.abs() > self.peak {
                    self.peak = value.abs();
                }
            }
        }
    }
}

impl Default for Meter {
    fn default() -> Meter {
        Meter::new()
    }
}
