//! One-pass loudness normalization (Sprint 10 sound, CAP-009).
//!
//! A composition declaring `normalize="loud"` lands its integrated
//! loudness at −16 LUFS with true-peak guarding, applied as one
//! overall gain inside the Framesmith mixer (never through an FFmpeg
//! filter, so re-renders stay byte-identical). Digital silence fails
//! closed instead of scaling silence to target.
//!
//! The measurement follows ITU-R BS.1770: K-weighted (pre-filter plus
//! RLB high-pass, 48 kHz coefficients) mean-square blocks of 400 ms
//! overlapped by 75%, an absolute gate at −70 LUFS, and a relative
//! gate at −10 LU. True peak is a 4x Catmull-Rom oversampled peak,
//! guarded to a −1 dBTP ceiling so the normalized mix cannot clip by
//! construction. All math is fixed-order `f64`, so the same mix bytes
//! always plan the same gain on any worker count.

/// Integrated loudness target of `normalize="loud"` (Sprint 10
/// sound, CAP-009), in LUFS.
pub const TARGET_LUFS: f64 = -16.0;

/// True-peak ceiling the normalization guard holds (Sprint 10 sound,
/// CAP-009/REC-005), in dBTP: the normalized mix never peaks above
/// this, matching the hot-master warning line.
pub const TRUE_PEAK_CEILING_DB: f64 = -1.0;

/// Linear amplitude of the true-peak ceiling.
pub fn true_peak_ceiling_linear() -> f64 {
    10.0f64.powf(TRUE_PEAK_CEILING_DB / 20.0)
}

/// One planned loudness gain (Sprint 10 sound, CAP-009): the measured
/// integrated loudness, the applied gain, and the true-peak facts the
/// guard used. The mixer multiplies the whole mix by `gain`.
#[derive(Debug, Clone, Copy)]
pub struct NormalizeReport {
    /// Measured integrated loudness of the pre-gain mix, in LUFS.
    pub measured_lufs: f64,
    /// Applied gain, in dB.
    pub gain_db: f64,
    /// Applied gain, linear (the mixer multiplier).
    pub gain: f32,
    /// True peak of the pre-gain mix, linear.
    pub true_peak_before: f64,
    /// True peak of the post-gain mix, linear.
    pub true_peak_after: f64,
    /// True when the ceiling capped the target gain.
    pub guarded: bool,
}

/// Marker for a mix with no measurable loudness (Sprint 10 sound,
/// CAP-009): empty, digital silence, or fully below the absolute
/// gate. Normalization fails closed on it instead of scaling silence
/// to target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SilentMix;

/// K-weighting pre-filter (high shelf) at 48 kHz, BS.1770 Table 1:
/// `y = b0*x + b1*x1 + b2*x2 - a1*y1 - a2*y2`.
const PRE_B: [f64; 3] = [1.53512485958697, -2.69169618940638, 1.19839281085285];
const PRE_A: [f64; 3] = [1.0, -1.69065929318241, 0.73248077421585];

/// K-weighting RLB high-pass at 48 kHz, BS.1770 Table 1.
const RLB_B: [f64; 3] = [1.0, -2.0, 1.0];
const RLB_A: [f64; 3] = [1.0, -1.99004745483398, 0.99007225036621];

/// K-weighted block length: 400 ms at 48 kHz (BS.1770 §3.2).
const BLOCK_FRAMES: usize = 19_200;
/// K-weighted block hop: 100 ms at 48 kHz (75% overlap).
const HOP_FRAMES: usize = 4_800;
/// Absolute gate: blocks below −70 LUFS never join the integral.
const ABS_GATE_LUFS: f64 = -70.0;
/// Relative gate: blocks 10 LU below the ungated integral drop out.
const REL_GATE_LU: f64 = 10.0;
/// Level offset of the BS.1770 energy sum (the −0.691 LU constant).
const LK_OFFSET: f64 = -0.691;

/// Run one biquad section over `samples` in place (Direct Form I).
fn biquad(samples: &mut [f64], b: &[f64; 3], a: &[f64; 3]) {
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for s in samples.iter_mut() {
        let x0 = *s;
        let y0 = b[0] * x0 + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2;
        x2 = x1;
        x1 = x0;
        y2 = y1;
        y1 = y0;
        *s = y0;
    }
}

/// K-weight one channel: pre-filter into the RLB high-pass, in
/// place. Both stages start from rest, so the same samples always
/// filter to the same weights on any worker count.
fn k_weight(channel: &mut [f64]) {
    biquad(channel, &PRE_B, &PRE_A);
    biquad(channel, &RLB_B, &RLB_A);
}

/// Loudness of one gated block energy (BS.1770 §4.3.2): the two
/// stereo channels carry weight 1.0 each.
fn block_loudness(energy: f64) -> f64 {
    LK_OFFSET + 10.0 * energy.log10()
}

/// Integrated loudness of an interleaved stereo `f32` mix at
/// 48 kHz, in LUFS. Returns `None` when no block clears the absolute
/// gate (digital silence or fully gated input), so the caller fails
/// closed instead of scaling silence.
pub fn integrated_loudness(mix: &[f32]) -> Option<f64> {
    const CHANNELS: usize = 2;
    if mix.len() < CHANNELS {
        return None;
    }
    let frames = mix.len() / CHANNELS;
    // Deinterleave into fixed-order f64 channels for the filters.
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    let (pairs, _) = mix.as_chunks::<CHANNELS>();
    for pair in pairs {
        left.push(pair[0] as f64);
        right.push(pair[1] as f64);
    }
    k_weight(&mut left);
    k_weight(&mut right);
    // Block energies over 400 ms windows every 100 ms; a mix shorter
    // than one block measures as one whole-mix block.
    let mut energies = Vec::new();
    if frames < BLOCK_FRAMES {
        let sum: f64 =
            left.iter().map(|x| x * x).sum::<f64>() + right.iter().map(|x| x * x).sum::<f64>();
        energies.push(sum / frames.max(1) as f64);
    } else {
        let mut start = 0usize;
        while start + BLOCK_FRAMES <= frames {
            let end = start + BLOCK_FRAMES;
            let sum: f64 = left[start..end].iter().map(|x| x * x).sum::<f64>()
                + right[start..end].iter().map(|x| x * x).sum::<f64>();
            energies.push(sum / BLOCK_FRAMES as f64);
            start += HOP_FRAMES;
        }
    }
    let abs_gate_energy = 10.0f64.powf((ABS_GATE_LUFS - LK_OFFSET) / 10.0);
    let gated: Vec<f64> = energies
        .into_iter()
        .filter(|e| *e > abs_gate_energy)
        .collect();
    if gated.is_empty() {
        return None;
    }
    let ungated = gated.iter().sum::<f64>() / gated.len() as f64;
    let rel_gate_energy = 10.0f64.powf((block_loudness(ungated) - REL_GATE_LU - LK_OFFSET) / 10.0);
    let regal: Vec<f64> = gated.into_iter().filter(|e| *e > rel_gate_energy).collect();
    if regal.is_empty() {
        return None;
    }
    Some(block_loudness(
        regal.iter().sum::<f64>() / regal.len() as f64,
    ))
}

/// 4x oversampled true peak of an interleaved stereo `f32` mix
/// (linear amplitude): Catmull-Rom interpolation between samples
/// reveals inter-sample overshoots that the sample peak misses. Edge
/// samples clamp, so short inputs still measure deterministically.
pub fn true_peak(mix: &[f32]) -> f64 {
    const CHANNELS: usize = 2;
    let frames = mix.len() / CHANNELS;
    if frames == 0 {
        return 0.0;
    }
    let at = |i: usize, c: usize| -> f64 { mix[i.clamp(0, frames - 1) * CHANNELS + c] as f64 };
    let mut peak = 0.0f64;
    for c in 0..CHANNELS {
        for i in 0..frames {
            let p0 = at(i.saturating_sub(1), c);
            let p1 = at(i, c);
            let p2 = at(i + 1, c);
            let p3 = at(i + 2, c);
            for k in 0..4 {
                let t = k as f64 / 4.0;
                let t2 = t * t;
                let t3 = t2 * t;
                // Catmull-Rom through p1..p2 at fraction t.
                let y = 0.5
                    * (2.0 * p1
                        + (-p0 + p2) * t
                        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
                peak = peak.max(y.abs());
            }
        }
    }
    peak
}

/// Plan one overall loudness gain for a 48 kHz stereo `f32` mix
/// (Sprint 10 sound, CAP-009): the gain landing integrated loudness
/// at [`TARGET_LUFS`], capped so the oversampled true peak never
/// exceeds the −1 dBTP ceiling. Silent mixes (empty, digital zero, or
/// fully gated) return [`SilentMix`] so the caller fails closed.
pub fn normalize_gain_for_mix(mix: &[f32]) -> Result<(f32, NormalizeReport), SilentMix> {
    let measured = integrated_loudness(mix).ok_or(SilentMix)?;
    let tp_before = true_peak(mix);
    if tp_before.is_nan() || tp_before <= 0.0 || !measured.is_finite() {
        return Err(SilentMix);
    }
    let mut gain_lin = 10.0f64.powf((TARGET_LUFS - measured) / 20.0);
    if !gain_lin.is_finite() || gain_lin <= 0.0 {
        return Err(SilentMix);
    }
    let ceiling = true_peak_ceiling_linear();
    let mut guarded = false;
    if tp_before * gain_lin > ceiling {
        gain_lin = ceiling / tp_before;
        guarded = true;
    }
    let gain = gain_lin as f32;
    Ok((
        gain,
        NormalizeReport {
            measured_lufs: measured,
            gain_db: 20.0 * gain_lin.log10(),
            gain,
            true_peak_before: tp_before,
            true_peak_after: tp_before * gain_lin,
            guarded,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 48 kHz stereo sine frames at `peak` amplitude (both channels).
    fn sine_frames(peak: f32, secs: f64) -> Vec<f32> {
        let frames = (secs * 48_000.0).round() as usize;
        let mut out = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = peak * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48_000.0).sin() as f32;
            out.push(s);
            out.push(s);
        }
        out
    }

    #[test]
    fn silence_fails_closed() {
        assert_eq!(
            normalize_gain_for_mix(&vec![0.0f32; 96_000]).err(),
            Some(SilentMix)
        );
        assert_eq!(normalize_gain_for_mix(&[]).err(), Some(SilentMix));
        // A whisper 80 dB down never clears the absolute gate.
        assert_eq!(
            normalize_gain_for_mix(&sine_frames(0.000_01, 1.0)).err(),
            Some(SilentMix)
        );
        assert_eq!(integrated_loudness(&vec![0.0f32; 96_000]), None);
    }

    #[test]
    fn steady_tone_lands_on_target() {
        let mix = sine_frames(10_000.0 / 32_768.0, 2.0);
        let measured = integrated_loudness(&mix).expect("tone measures");
        // A −13 dBFS-RMS-class tone K-weights near its RMS level.
        assert!(
            (-19.0..-9.0).contains(&measured),
            "tone measures {measured:.2} LUFS"
        );
        let (gain, report) = normalize_gain_for_mix(&mix).expect("tone normalizes");
        assert!(!report.guarded, "a modest tone needs no guard");
        assert!(
            (report.gain_db - (TARGET_LUFS - measured)).abs() < 1e-9,
            "gain is exactly the target gap"
        );
        let mut normalized = mix.clone();
        for s in normalized.iter_mut() {
            *s *= gain;
        }
        let relanded = integrated_loudness(&normalized).expect("re-measures");
        assert!(
            (relanded - TARGET_LUFS).abs() < 0.15,
            "re-measures {relanded:.3} LUFS"
        );
        assert!(
            report.true_peak_after <= true_peak_ceiling_linear(),
            "true peak stays guarded"
        );
    }

    #[test]
    fn gain_planning_is_deterministic() {
        let mix = sine_frames(0.25, 1.5);
        let (g1, r1) = normalize_gain_for_mix(&mix).expect("plans");
        let (g2, r2) = normalize_gain_for_mix(&mix).expect("plans again");
        assert_eq!(g1.to_bits(), g2.to_bits());
        assert_eq!(r1.measured_lufs.to_bits(), r2.measured_lufs.to_bits());
        assert_eq!(r1.gain_db.to_bits(), r2.gain_db.to_bits());
    }

    #[test]
    fn ceiling_guards_peaky_quiet_mixes() {
        // A whisper-quiet bed with one full-scale tick: the integrated
        // level reads far below target, so the raw target gain would
        // burst the peak — the guard caps the mix at the −1 dBTP
        // ceiling instead.
        let mut mix = sine_frames(0.001, 2.0);
        mix[48_000] = 1.0;
        mix[48_001] = 1.0;
        let (gain, report) = normalize_gain_for_mix(&mix).expect("peaky mix plans");
        assert!(report.guarded, "the ceiling caps the peaky gain");
        // The enforced multiplier is the `f32` gain the mixer applies,
        // so the peak lands at the ceiling within `f32` rounding.
        assert!(
            (report.true_peak_before * gain as f64 - true_peak_ceiling_linear()).abs() < 1e-6,
            "guarded mix peaks exactly at the ceiling"
        );
        assert!(
            report.true_peak_after <= true_peak_ceiling_linear() + 1e-9,
            "reported peak stays guarded"
        );
    }

    #[test]
    fn oversampled_peak_catches_intersample_overshoot() {
        // A sine peaking between samples: the sample peak misses it,
        // the 4x interpolation must not.
        let mut peak_sample = 0.0f32;
        let frames = 48_000usize;
        let mut mix = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = 0.99
                * (2.0 * std::f64::consts::PI * 1_000.0 * (i as f64 + 0.37) / 48_000.0).sin()
                    as f32;
            peak_sample = peak_sample.max(s.abs());
            mix.push(s);
            mix.push(s);
        }
        let tp = true_peak(&mix);
        assert!(
            tp >= peak_sample as f64 - 1e-6,
            "tp {tp} vs sample {peak_sample}"
        );
        assert!(tp <= 1.0 + 1e-6, "sine overshoot stays near unity");
        assert_eq!(true_peak(&[]), 0.0);
    }
}
