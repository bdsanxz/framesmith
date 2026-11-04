//! Temporal shutter blur (Sprint 11 motion blur, CAP-006/T007).
//!
//! A composition `motion-blur` element opens a shutter window of
//! `shutter`/360 of a frame interval ending at frame time and averages
//! evenly spaced sub-frame pictures across it, so content moving along
//! its motion vectors smears in its direction of travel. Sampling is
//! evenly spaced with no jitter, so the same inputs re-render
//! byte-identically; sub-frame composition times are exact
//! nanosecond-quantized rationals, so every run plans the same sample
//! instants. Blur applies after transitions: each sub-sample draws
//! through the same frame path (plain or bridged) at its own instant.
//!
//! The tier turns the written `samples` budget into the effective
//! per-frame count: `high` draws the full budget, `medium` at most 8,
//! `low` at most 4, and `auto` scales by the window's own motion (the
//! fraction of pixels differing between the window endpoints), drawing
//! exactly once when the window is still — pixel-identical to blur
//! off.

use notation::model::{Fps, MotionBlur};
use timebase::Time;

/// One frame interval in seconds, exactly: `den / num`.
fn frame_interval_secs(fps: &Fps) -> f64 {
    fps.den.max(1) as f64 / fps.num.max(1) as f64
}

/// Shutter-open length in seconds: `shutter`/360 of a frame interval.
/// A 180-degree shutter covers half the interval; 0 opens no window.
pub fn exposure_secs(blur: &MotionBlur, fps: &Fps) -> f64 {
    (blur.shutter / 360.0) * frame_interval_secs(fps)
}

/// Exact composition time from float seconds: nanosecond-quantized, so
/// every run plans the same sample instants deterministically.
fn time_from_secs(secs: f64) -> Time {
    Time::new((secs * 1e9).round() as i128, 1_000_000_000)
}

/// Sub-frame sample instants for one output frame (Sprint 11 motion
/// blur, CAP-006/T007): `n` evenly spaced times across the
/// shutter-open window `[end - exposure, end]`, endpoints included, so
/// a wider shutter spreads its samples over more motion. `n == 1`
/// samples exactly at frame time (sharp); sub-frame times may predate
/// composition zero on early frames (nothing visible there yet, so the
/// window naturally weights the background — exactly like a real
/// shutter opening before content arrives).
pub fn sub_times(end: Time, fps: &Fps, shutter: f64, n: usize) -> Vec<Time> {
    let n = n.max(1);
    if n == 1 {
        return vec![end];
    }
    let end_secs = end.to_f64();
    let exposure = (shutter / 360.0) * frame_interval_secs(fps);
    let mut times: Vec<Time> = (0..n)
        .map(|i| {
            let frac = i as f64 / (n - 1) as f64;
            time_from_secs(end_secs - exposure + exposure * frac)
        })
        .collect();
    // The window ends exactly at frame time (never a quantized
    // neighbor of it), so one window always samples the sharp instant.
    if let Some(last) = times.last_mut() {
        *last = end;
    }
    times
}

/// Fraction of bytes differing between two same-length RGB24 frames
/// (0–1, saturated): the `auto` tier's motion reading. Identical
/// pictures read exactly 0, so still windows rest sharp.
pub fn motion_fraction(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let diff = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    (diff as f64 / a.len() as f64).min(1.0)
}

/// Box-average of same-size RGB24 frames (Sprint 11 motion blur,
/// CAP-006/T007): per-byte accumulation with round-half-up, so
/// averaging identical pictures returns them exactly and re-renders
/// stay byte-identical. Empty input yields an empty frame; ragged
/// input averages the shortest common prefix length.
pub fn average_frames(frames: &[Vec<u8>]) -> Vec<u8> {
    if frames.is_empty() {
        return Vec::new();
    }
    let len = frames.iter().map(|f| f.len()).min().unwrap_or(0);
    let n = frames.len() as u32;
    let mut out = vec![0u8; len];
    for i in 0..len {
        let mut sum: u32 = 0;
        for f in frames {
            sum += f[i] as u32;
        }
        out[i] = ((sum + n / 2) / n).min(255) as u8;
    }
    out
}

/// Draw one output frame with shutter blur (Sprint 11 motion blur,
/// CAP-006/T007): `draw_at` draws the frame (plain or transition
/// bridged) at one composition instant. Named tiers draw their capped
/// count and average; `auto` probes the window endpoints first and
/// scales by their motion, reusing the frame-time probe when the
/// window is still. A one-sample outcome calls `draw_at` exactly once
/// at frame time — the same instant as blur off — so it renders
/// pixel-identical to blur off; a zero shutter short-circuits the same
/// way. Every path is deterministic: fixed instants, fixed counts.
pub fn draw_blurred(
    draw_at: &mut dyn FnMut(Time) -> Result<Vec<u8>, String>,
    end: Time,
    fps: &Fps,
    blur: &MotionBlur,
) -> Result<Vec<u8>, String> {
    if blur.tier != notation::model::MotionBlurTier::Auto {
        let n = MotionBlur::tier_cap(blur.tier, blur.samples);
        if n <= 1 || blur.shutter <= 0.0 {
            return draw_at(end);
        }
        let mut frames = Vec::with_capacity(n);
        for t in sub_times(end, fps, blur.shutter, n) {
            frames.push(draw_at(t)?);
        }
        return Ok(average_frames(&frames));
    }
    // `auto`: probe the window endpoints, then scale by their motion.
    let exposure = exposure_secs(blur, fps);
    if blur.samples <= 1 || blur.shutter <= 0.0 || exposure <= 0.0 {
        return draw_at(end);
    }
    let start = time_from_secs(end.to_f64() - exposure);
    let first = draw_at(start)?;
    let last = draw_at(end)?;
    let n = blur.effective_samples(motion_fraction(&first, &last));
    if n <= 1 {
        return Ok(last);
    }
    if n == 2 {
        return Ok(average_frames(&[first, last]));
    }
    let mut frames = Vec::with_capacity(n);
    frames.push(first);
    for t in sub_times(end, fps, blur.shutter, n)
        .into_iter()
        .skip(1)
        .take(n - 2)
    {
        frames.push(draw_at(t)?);
    }
    frames.push(last);
    Ok(average_frames(&frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::MotionBlurTier;

    fn blur(shutter: f64, samples: u32, tier: MotionBlurTier) -> MotionBlur {
        MotionBlur {
            shutter,
            samples,
            tier,
            line: 1,
            column: 1,
        }
    }

    fn fps30() -> Fps {
        Fps::from_int(30)
    }

    #[test]
    fn exposure_scales_with_the_shutter_angle() {
        let fps = fps30();
        // A 180-degree shutter covers half the 1/30s frame interval.
        let half = exposure_secs(&blur(180.0, 8, MotionBlurTier::High), &fps);
        assert!((half - 1.0 / 60.0).abs() < 1e-12, "half interval: {half}");
        let full = exposure_secs(&blur(360.0, 8, MotionBlurTier::High), &fps);
        assert!((full - 1.0 / 30.0).abs() < 1e-12, "full interval: {full}");
        assert_eq!(
            exposure_secs(&blur(0.0, 8, MotionBlurTier::High), &fps),
            0.0
        );
    }

    #[test]
    fn sub_times_span_the_window_with_both_endpoints() {
        let fps = fps30();
        let end = Time::of_frame(30, 30);
        let times = sub_times(end, &fps, 180.0, 4);
        assert_eq!(times.len(), 4);
        // First sample opens the window half an interval back; the
        // last samples exactly at frame time.
        assert!((times[0].to_f64() - (1.0 - 1.0 / 60.0)).abs() < 1e-9);
        assert_eq!(times[3], end);
        // Even spacing between the endpoints (nanosecond
        // quantization dust aside).
        let step = times[1].to_f64() - times[0].to_f64();
        assert!((step - 1.0 / 180.0).abs() < 1e-6, "step is {step}");
        assert!((times[2].to_f64() - times[1].to_f64() - step).abs() < 1e-6);
        assert!((times[3].to_f64() - times[2].to_f64() - step).abs() < 1e-6);
        // One sample means exactly frame time.
        assert_eq!(sub_times(end, &fps, 180.0, 1), vec![end]);
        assert_eq!(sub_times(end, &fps, 180.0, 0), vec![end]);
    }

    #[test]
    fn motion_fraction_reads_zero_for_identical_frames() {
        let a = vec![10u8; 300];
        assert_eq!(motion_fraction(&a, &a.clone()), 0.0);
        let mut b = a.clone();
        b[0] = 11;
        assert!((motion_fraction(&a, &b) - 1.0 / 300.0).abs() < 1e-12);
        let c = vec![200u8; 300];
        assert_eq!(motion_fraction(&a, &c), 1.0);
        // Ragged or empty input never rests sharp by accident.
        assert_eq!(motion_fraction(&[], &[]), 1.0);
        assert_eq!(motion_fraction(&[1u8], &[1u8, 2u8]), 1.0);
    }

    #[test]
    fn averaging_identical_frames_is_exact() {
        let a = vec![37u8; 96];
        let frames = vec![a.clone(), a.clone(), a.clone(), a.clone()];
        assert_eq!(average_frames(&frames), a);
    }

    #[test]
    fn averaging_blends_halfway_and_rerenders_identically() {
        let black = vec![0u8; 12];
        let white = vec![255u8; 12];
        // Round-half-up: (0 + 255 + 1) / 2 = 128.
        assert_eq!(
            average_frames(&[black.clone(), white.clone()]),
            vec![128u8; 12]
        );
        let once = average_frames(&[black.clone(), white.clone()]);
        let twice = average_frames(&[black, white]);
        assert_eq!(once, twice, "re-renders are byte-identical");
        assert!(average_frames(&[]).is_empty());
    }

    #[test]
    fn named_tiers_draw_their_capped_counts() {
        use std::cell::Cell;
        let fps = fps30();
        let end = Time::of_frame(30, 30);
        let calls = Cell::new(0usize);
        let mut draw_at = |_t: Time| {
            calls.set(calls.get() + 1);
            Ok(vec![7u8; 6])
        };
        let out = draw_blurred(
            &mut draw_at,
            end,
            &fps,
            &blur(180.0, 16, MotionBlurTier::High),
        )
        .expect("high draws");
        assert_eq!(calls.get(), 16, "high draws the full budget");
        assert_eq!(out, vec![7u8; 6]);

        calls.set(0);
        draw_blurred(
            &mut draw_at,
            end,
            &fps,
            &blur(180.0, 16, MotionBlurTier::Medium),
        )
        .expect("medium draws");
        assert_eq!(calls.get(), 8, "medium caps at 8");

        calls.set(0);
        draw_blurred(
            &mut draw_at,
            end,
            &fps,
            &blur(180.0, 16, MotionBlurTier::Low),
        )
        .expect("low draws");
        assert_eq!(calls.get(), 4, "low caps at 4");
    }

    #[test]
    fn zero_shutter_draws_once_at_frame_time() {
        use std::cell::RefCell;
        let fps = fps30();
        let end = Time::of_frame(30, 30);
        let seen = RefCell::new(Vec::new());
        let mut draw_at = |t: Time| {
            seen.borrow_mut().push(t);
            Ok(vec![9u8; 6])
        };
        let out = draw_blurred(
            &mut draw_at,
            end,
            &fps,
            &blur(0.0, 16, MotionBlurTier::High),
        )
        .expect("zero shutter draws");
        assert_eq!(out, vec![9u8; 6]);
        assert_eq!(seen.borrow().as_slice(), &[end]);
    }

    #[test]
    fn auto_rests_sharp_on_still_content_and_blurs_motion() {
        use std::cell::Cell;
        let fps = fps30();
        let end = Time::of_frame(30, 30);
        // Still content: the two endpoint probes match, so only they
        // draw and the frame-time probe is returned untouched.
        let calls = Cell::new(0usize);
        let mut still = |_t: Time| {
            calls.set(calls.get() + 1);
            Ok(vec![5u8; 6])
        };
        let out = draw_blurred(
            &mut still,
            end,
            &fps,
            &blur(180.0, 16, MotionBlurTier::Auto),
        )
        .expect("auto still draws");
        assert_eq!(out, vec![5u8; 6], "still auto returns the sharp frame");
        assert_eq!(calls.get(), 2, "still auto probes endpoints only");
        // Full-window motion: endpoints differ everywhere, so the full
        // budget draws and the average carries both endpoints.
        let calls = Cell::new(0usize);
        let mut moving = |t: Time| {
            calls.set(calls.get() + 1);
            Ok(vec![if t == end { 255u8 } else { 0u8 }; 6])
        };
        let out = draw_blurred(
            &mut moving,
            end,
            &fps,
            &blur(180.0, 4, MotionBlurTier::Auto),
        )
        .expect("auto motion draws");
        assert_eq!(calls.get(), 4, "full motion earns the full budget");
        // Three black samples and one white: 255/4 rounds to 64.
        assert_eq!(out, vec![64u8; 6], "average of black..white window");
    }
}
