//! The four sound checks: the length of our sound against our picture, the loudness, the peak and
//! the alignment of the two sounds (`docs/dev/comparison.md`, section 12). The measures are our own
//! arithmetic, stated in that section; both sounds are read with frx7.

use crate::report::{Report, Verdict};
use measure::{Sound, VideoFacts};
use serde_json::json;
use std::path::Path;

/// The limits of `PLAN.md`, section 5.1.
pub const LOUDNESS_LIMIT_LU: f64 = 0.5;
pub const PEAK_LIMIT_DB: f64 = 0.5;
pub const ALIGNMENT_LIMIT_MS: f64 = 1.0;
/// The widest shift the alignment measure looks for, each way.
pub const ALIGNMENT_SEARCH_MS: f64 = 20.0;
/// The part of each sound the alignment measure compares, from its start.
pub const ALIGNMENT_SPAN_S: f64 = 10.0;

/// The absolute gate of the loudness measure, in LUFS.
const ABSOLUTE_GATE: f64 = -70.0;
/// The relative gate, in LU under the loudness of the blocks above the absolute gate.
const RELATIVE_GATE: f64 = -10.0;

/// One filter of second order: `y = b0 x + b1 x1 + b2 x2 - a1 y1 - a2 y2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b: [f64; 3],
    pub a: [f64; 2],
}

impl Biquad {
    fn run(&self, input: &[f64]) -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        input
            .iter()
            .map(|&x| {
                let y = self.b[0] * x + self.b[1] * x1 + self.b[2] * x2 - self.a[0] * y1 - self.a[1] * y2;
                (x2, x1, y2, y1) = (x1, x, y1, y);
                y
            })
            .collect()
    }
}

/// The two filters of the frequency weighting of the loudness measure at a sample rate: the shelf
/// that models the head, then the high-pass. Section 12.3 of the spec gives their analogue
/// parameters and the transform.
pub fn weighting(rate: u32) -> [Biquad; 2] {
    let fs = f64::from(rate);
    // The shelf.
    let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    // The high-pass.
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    [shelf, high_pass]
}

/// The weight of each channel in the sum of the loudness measure: 1 for each, but for six channels
/// (left, right, centre, low-frequency effects, left surround, right surround) the low-frequency
/// channel is left out and each surround channel weighs 1.41.
pub fn channel_weights(channels: u16) -> Vec<f64> {
    if channels == 6 {
        vec![1.0, 1.0, 1.0, 0.0, 1.41, 1.41]
    } else {
        vec![1.0; usize::from(channels)]
    }
}

fn channel(sound: &Sound, number: usize) -> impl Iterator<Item = f64> + '_ {
    sound
        .samples
        .iter()
        .skip(number)
        .step_by(usize::from(sound.channels).max(1))
        .map(|&v| f64::from(v))
}

/// The integrated loudness of a sound in LUFS (section 12.3): weighted, in blocks of 400 ms stepped
/// by 100 ms, gated at -70 LUFS and then 10 LU under the loudness of the blocks above that gate.
/// `None` when no block is above the gates: the sound is silent, or shorter than one block.
pub fn loudness(sound: &Sound) -> Option<f64> {
    let rate = sound.rate;
    if rate == 0 || sound.channels == 0 {
        return None;
    }
    let block = (f64::from(rate) * 0.4).round() as usize;
    let step = (f64::from(rate) * 0.1).round() as usize;
    let frames = sound.frames() as usize;
    if block == 0 || step == 0 || frames < block {
        return None;
    }
    let [shelf, high_pass] = weighting(rate);
    let weights = channel_weights(sound.channels);
    // For each channel, the running sum of its weighted squares, so that a block is one subtraction.
    let sums: Vec<(f64, Vec<f64>)> = (0..usize::from(sound.channels))
        .filter(|&c| weights[c] != 0.0)
        .map(|c| {
            let x: Vec<f64> = channel(sound, c).collect();
            let y = high_pass.run(&shelf.run(&x));
            let mut running = Vec::with_capacity(y.len() + 1);
            running.push(0.0);
            let mut total = 0.0;
            for v in y {
                total += v * v;
                running.push(total);
            }
            (weights[c], running)
        })
        .collect();
    let blocks = (frames - block) / step + 1;
    let powers: Vec<f64> = (0..blocks)
        .map(|j| {
            let (start, end) = (j * step, j * step + block);
            sums.iter()
                .map(|(weight, running)| weight * (running[end] - running[start]) / block as f64)
                .sum()
        })
        .collect();
    let lufs = |power: f64| -0.691 + 10.0 * power.log10();
    let mean = |kept: &[f64]| kept.iter().sum::<f64>() / kept.len() as f64;
    let above: Vec<f64> = powers.iter().copied().filter(|&p| p > 0.0 && lufs(p) > ABSOLUTE_GATE).collect();
    if above.is_empty() {
        return None;
    }
    let gate = lufs(mean(&above)) + RELATIVE_GATE;
    let kept: Vec<f64> = above.into_iter().filter(|&p| lufs(p) > gate).collect();
    if kept.is_empty() {
        return None;
    }
    Some(lufs(mean(&kept)))
}

/// The sample peak of a sound in dB under full scale: the largest magnitude of any sample of any
/// channel. `None` when every sample is 0.
pub fn peak(sound: &Sound) -> Option<f64> {
    let largest = sound.samples.iter().fold(0f64, |m, &v| m.max(f64::from(v).abs()));
    (largest > 0.0).then(|| 20.0 * largest.log10())
}

/// The shift of `ours` against `theirs`, in sample frames: positive when ours is later (section
/// 12.5). The channels of each are averaged; the shift is the one, within the search, at which the
/// sum of products of the two over the first span is largest. `None` when either is silent in the
/// span or the rates differ.
pub fn shift(ours: &Sound, theirs: &Sound) -> Option<i64> {
    if ours.rate != theirs.rate || ours.rate == 0 {
        return None;
    }
    let mono = |sound: &Sound| -> Vec<f64> {
        let width = usize::from(sound.channels).max(1);
        let span = (f64::from(sound.rate) * ALIGNMENT_SPAN_S) as usize;
        sound
            .samples
            .chunks_exact(width)
            .take(span)
            .map(|frame| frame.iter().map(|&v| f64::from(v)).sum::<f64>() / width as f64)
            .collect()
    };
    let (a, b) = (mono(ours), mono(theirs));
    if a.iter().all(|&v| v == 0.0) || b.iter().all(|&v| v == 0.0) {
        return None;
    }
    let reach = (f64::from(ours.rate) * ALIGNMENT_SEARCH_MS / 1000.0).round() as i64;
    let mut best: Option<(i64, f64)> = None;
    for lag in -reach..=reach {
        // ours[i + lag] against theirs[i].
        let first = 0.max(-lag) as usize;
        let last = (b.len() as i64).min(a.len() as i64 - lag);
        if last <= first as i64 {
            continue;
        }
        let sum: f64 = (first..last as usize).map(|i| a[(i as i64 + lag) as usize] * b[i]).sum();
        // A tie keeps the smaller shift.
        let better = match best {
            None => true,
            Some((at, value)) => sum > value || (sum == value && lag.abs() < at.abs()),
        };
        if better {
            best = Some((lag, sum));
        }
    }
    best.map(|(lag, _)| lag)
}

fn lufs_text(value: Option<f64>) -> String {
    value.map_or("silent (no block above the gates)".to_string(), |v| format!("{v:.2} LUFS"))
}

fn db_text(value: Option<f64>) -> String {
    value.map_or("silent (every sample 0)".to_string(), |v| format!("{v:.2} dBFS"))
}

fn verdict(passes: bool) -> Verdict {
    if passes {
        Verdict::Pass
    } else {
        Verdict::Fail
    }
}

/// Judges two levels in the same unit against a limit: both measured and no further apart than the
/// limit, or both silent.
fn levels(ours: Option<f64>, theirs: Option<f64>, limit: f64, unit: &str) -> (Verdict, String) {
    match (ours, theirs) {
        (Some(a), Some(b)) => {
            let apart = (a - b).abs();
            (verdict(apart <= limit), format!("{apart:.2} {unit} apart (limit {limit} {unit})"))
        }
        (None, None) => (Verdict::Pass, "both silent".to_string()),
        _ => (Verdict::Fail, "one is silent and the other is not".to_string()),
    }
}

/// The sound length check alone: our sound against our picture, to the sample.
pub fn length_line(sound: &Sound, picture: &VideoFacts) -> (Verdict, String) {
    let rate = u128::from(sound.rate);
    let ticks = u128::from(picture.ticks);
    let timescale = u128::from(picture.timescale.max(1));
    let frames = u128::from(sound.frames());
    let same = frames * timescale == ticks * rate;
    let picture_frames = ticks * rate / timescale;
    let whole = ticks * rate % timescale == 0;
    let picture_text = if whole {
        format!("{picture_frames}")
    } else {
        format!("{:.3}", ticks as f64 * rate as f64 / timescale as f64)
    };
    (
        verdict(same),
        format!(
            "ours: sound {frames} sample frames, picture {picture_text} sample frames ({:.6} s at {} Hz)",
            picture.seconds(),
            sound.rate
        ),
    )
}

/// The four sound checks of a pair whose sounds are already read, put into the report with what
/// was measured. `picture` is the facts of our picture track.
pub fn judge(report: &mut Report, ours: Option<&Sound>, theirs: Option<&Sound>, picture: &VideoFacts) {
    let not_applicable = "neither render has sound";
    let (Some(a), Some(b)) = (ours, theirs) else {
        match (ours, theirs) {
            (None, None) => {
                report.check("sound length", Verdict::NotApplicable, "ours has no sound track");
                for name in ["loudness", "peak", "alignment"] {
                    report.check(name, Verdict::NotApplicable, not_applicable);
                }
                report.data.push(("sound".to_string(), json!({"ours": null, "reference": null})));
            }
            (Some(a), None) => {
                let (verdict, detail) = length_line(a, picture);
                report.check("sound length", verdict, detail);
                for name in ["loudness", "peak", "alignment"] {
                    report.check(name, Verdict::Fail, "ours has sound and the reference's render has none");
                }
            }
            _ => {
                report.check(
                    "sound length",
                    Verdict::Fail,
                    "ours has no sound track and the reference's render has one",
                );
                for name in ["loudness", "peak", "alignment"] {
                    report.check(name, Verdict::Fail, "the reference's render has sound and ours has none");
                }
            }
        }
        return;
    };
    let (length_verdict, length_detail) = length_line(a, picture);
    report.check("sound length", length_verdict, length_detail);

    let (la, lb) = (loudness(a), loudness(b));
    let (verdict_l, apart_l) = levels(la, lb, LOUDNESS_LIMIT_LU, "LU");
    report.check(
        "loudness",
        verdict_l,
        format!("ours {}, reference {}; {apart_l}", lufs_text(la), lufs_text(lb)),
    );

    let (pa, pb) = (peak(a), peak(b));
    let (verdict_p, apart_p) = levels(pa, pb, PEAK_LIMIT_DB, "dB");
    report.check(
        "peak",
        verdict_p,
        format!("ours {}, reference {}; {apart_p}", db_text(pa), db_text(pb)),
    );

    let found = shift(a, b);
    match found {
        Some(lag) => {
            let ms = lag as f64 * 1000.0 / f64::from(a.rate);
            let reach = (f64::from(a.rate) * ALIGNMENT_SEARCH_MS / 1000.0).round() as i64;
            let at_edge = lag.abs() == reach;
            let way = if lag > 0 {
                "later than"
            } else if lag < 0 {
                "earlier than"
            } else {
                "in step with"
            };
            let detail = if at_edge {
                format!(
                    "measured shift {ms:+.3} ms ({lag:+} sample frames at {} Hz), the edge of the search of {ALIGNMENT_SEARCH_MS} ms each way: the shift may be larger; limit {ALIGNMENT_LIMIT_MS} ms",
                    a.rate
                )
            } else {
                format!(
                    "measured shift {ms:+.3} ms ({lag:+} sample frames at {} Hz): ours is {way} the reference; limit {ALIGNMENT_LIMIT_MS} ms",
                    a.rate
                )
            };
            report.check("alignment", verdict(!at_edge && ms.abs() <= ALIGNMENT_LIMIT_MS), detail);
        }
        None if a.rate != b.rate => report.check(
            "alignment",
            Verdict::Fail,
            format!("the sounds are at different rates: ours {} Hz, reference {} Hz", a.rate, b.rate),
        ),
        None => report.check(
            "alignment",
            Verdict::NotRun,
            "nothing to align: a sound is silent in its first span",
        ),
    }
    report.data.push((
        "sound".to_string(),
        json!({
            "ours": {"rate": a.rate, "channels": a.channels, "frames": a.frames(), "loudness_lufs": la, "peak_dbfs": pa},
            "reference": {"rate": b.rate, "channels": b.channels, "frames": b.frames(), "loudness_lufs": lb, "peak_dbfs": pb},
            "shift_frames": found,
            "shift_ms": found.map(|lag| lag as f64 * 1000.0 / f64::from(a.rate)),
        }),
    ));
}

/// Reads the sounds of both files with frx7 and puts the four checks into the report.
pub fn checks(report: &mut Report, ours: &Path, theirs: &Path, picture: &VideoFacts) -> Result<(), String> {
    let a = measure::read_sound(ours)?;
    let b = measure::read_sound(theirs)?;
    judge(report, a.as_ref(), b.as_ref(), picture);
    Ok(())
}
