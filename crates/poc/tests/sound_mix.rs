//! The level of the finished mix.

use poc::audio::{limit, true_peak, Peak, CEILING};

/// A tone of a quarter of the sample rate, on both channels, whose samples all miss its crests:
/// each sample is at 0.7071 of the tone's height.
fn between_the_samples(height: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|n| {
            let value = height * (std::f32::consts::FRAC_PI_2 * n as f32 + std::f32::consts::FRAC_PI_4).sin();
            [value, value]
        })
        .collect()
}

#[test]
fn a_peak_between_the_samples_is_found() {
    let samples = between_the_samples(0.9, 4800);
    let on_samples = samples.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!((on_samples - 0.636).abs() < 0.01, "the samples reach {on_samples}");
    let peak = true_peak(&samples);
    assert!((peak - 0.9).abs() < 0.02, "the tone reaches 0.9 between its samples: {peak}");
}

#[test]
fn a_mix_over_the_ceiling_is_lowered_as_a_whole_to_the_ceiling() {
    // Two tones of different heights: the balance between them must stay.
    let mut samples = between_the_samples(1.2, 4800);
    for (index, sample) in samples.iter_mut().enumerate() {
        if index % 2 == 1 {
            *sample *= 0.5;
        }
    }
    let before = samples.clone();
    let (peak, lowered) = limit(&mut samples);
    assert!((peak - 1.2).abs() < 0.03, "the peak as written: {peak}");
    let expected = 20.0 * (peak / CEILING).log10();
    assert!((lowered - expected).abs() < 1e-3 && lowered > 2.0, "lowered by {lowered} dB");
    assert!((true_peak(&samples) - CEILING).abs() < 1e-3, "the peak is at the ceiling");
    // Every sample was multiplied by the same number.
    let gain = samples[0] / before[0];
    assert!(samples.iter().zip(&before).all(|(now, was)| (now - was * gain).abs() < 1e-6));
}

#[test]
fn a_mix_under_the_ceiling_is_left_alone() {
    let mut samples = between_the_samples(0.8, 4800);
    let before = samples.clone();
    let (peak, lowered) = limit(&mut samples);
    assert!(peak < CEILING && lowered == 0.0, "peak {peak}, lowered {lowered}");
    assert_eq!(samples, before);
}

#[test]
fn a_peak_followed_piece_by_piece_is_the_peak_of_the_whole() {
    // A loud tone inside quieter sound, so that the peak lies between two samples somewhere
    // in the middle; the pieces are cut at every length, also right at the peak.
    let mut samples = between_the_samples(0.3, 2000);
    let loud = between_the_samples(0.9, 40);
    samples[2 * 977..2 * 977 + loud.len()].copy_from_slice(&loud);
    let whole = true_peak(&samples);
    assert!((whole - 0.9).abs() < 0.05, "the peak of the whole: {whole}");
    for frames in [1, 2, 5, 7, 13, 64, 977, 1999] {
        let mut peak = Peak::new();
        for piece in samples.chunks(frames * 2) {
            peak.add(piece);
        }
        assert!(
            (peak.highest() - whole).abs() < 1e-6,
            "in pieces of {frames} frames the peak is {}, of the whole {whole}",
            peak.highest()
        );
    }
}
