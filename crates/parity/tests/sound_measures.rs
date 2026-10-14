//! The loudness and peak measures of the sound checks on made signals whose loudness and peak are
//! known (`docs/dev/comparison.md`, sections 12.3 and 12.4).

use measure::Sound;
use parity::sound::{loudness, peak, weighting};

fn tone(rate: u32, channels: u16, hz: f64, amplitude: f64, seconds: f64) -> Sound {
    let frames = (f64::from(rate) * seconds) as usize;
    let mut samples = Vec::with_capacity(frames * usize::from(channels));
    for n in 0..frames {
        let v = (amplitude * (2.0 * std::f64::consts::PI * hz * n as f64 / f64::from(rate)).sin()) as f32;
        for _ in 0..channels {
            samples.push(v);
        }
    }
    Sound { rate, channels, samples }
}

fn near(found: f64, expected: f64, within: f64) {
    assert!(
        (found - expected).abs() <= within,
        "measured {found}, expected {expected} within {within}"
    );
}

#[test]
fn the_weighting_at_48_khz_has_the_coefficients_of_the_standard() {
    let [shelf, high_pass] = weighting(48_000);
    let tables = [
        (
            [1.53512485958697, -2.69169618940638, 1.19839281085285],
            [-1.69065929318241, 0.73248077421585],
        ),
        ([1.0, -2.0, 1.0], [-1.99004745483398, 0.99007225036621]),
    ];
    for (filter, (b, a)) in [shelf, high_pass].iter().zip(tables) {
        for (found, expected) in filter.b.iter().zip(b).chain(filter.a.iter().zip(a)) {
            near(*found, expected, 1e-12);
        }
    }
}

#[test]
fn a_full_scale_tone_of_997_hz_in_one_channel_measures_minus_3_01_lufs() {
    near(loudness(&tone(48_000, 1, 997.0, 1.0, 5.0)).unwrap(), -3.01, 0.01);
}

#[test]
fn a_tone_of_1000_hz_at_minus_23_db_in_both_channels_measures_minus_23_lufs() {
    let amplitude = 10f64.powf(-23.0 / 20.0);
    near(loudness(&tone(48_000, 2, 1000.0, amplitude, 20.0)).unwrap(), -23.0, 0.01);
}

#[test]
fn silence_after_a_tone_is_gated_out_of_the_loudness() {
    // 2 s of a tone at -23 LUFS, then 10 s of silence. The 17 blocks of tone alone and the 3 blocks
    // holding 3/4, 1/2 and 1/4 of it are kept; the blocks of silence are under -70 LUFS and left out:
    // -23 + 10 log10(18.5 / 20) = -23.339. Ungated, the mean would be near -30.8.
    let amplitude = 10f64.powf(-23.0 / 20.0);
    let mut sound = tone(48_000, 2, 1000.0, amplitude, 12.0);
    for v in sound.samples.iter_mut().skip(2 * 96_000) {
        *v = 0.0;
    }
    near(loudness(&sound).unwrap(), -23.0 + 10.0 * (18.5f64 / 20.0).log10(), 0.01);
}

#[test]
fn silence_and_a_sound_shorter_than_a_block_have_no_loudness() {
    let silent = Sound {
        rate: 48_000,
        channels: 2,
        samples: vec![0.0; 96_000],
    };
    assert_eq!(loudness(&silent), None);
    assert_eq!(loudness(&tone(48_000, 2, 1000.0, 0.5, 0.39)), None);
}

#[test]
fn a_gain_changes_the_loudness_by_as_many_lu() {
    let sound = measure::made_sound(48_000, 2, 96_000, 0.5, 0);
    let gained = Sound {
        samples: sound.samples.iter().map(|v| v * 10f32.powf(-0.6 / 20.0)).collect(),
        ..sound.clone()
    };
    near(loudness(&sound).unwrap() - loudness(&gained).unwrap(), 0.6, 1e-4);
}

#[test]
fn a_single_sample_of_half_scale_peaks_at_minus_6_02_dbfs() {
    let mut samples = vec![0.0f32; 2_000];
    samples[777] = -0.5;
    let found = peak(&Sound {
        rate: 48_000,
        channels: 2,
        samples,
    })
    .unwrap();
    near(found, 20.0 * 0.5f64.log10(), 1e-9);
    near(found, -6.02, 0.001);
}

#[test]
fn a_tone_of_amplitude_half_peaks_at_minus_6_02_dbfs() {
    near(peak(&tone(48_000, 2, 997.0, 0.5, 1.0)).unwrap(), -6.0206, 0.001);
}

#[test]
fn a_gain_changes_the_peak_by_as_many_db_and_silence_has_none() {
    let sound = measure::made_sound(48_000, 2, 48_000, 0.5, 0);
    let gained = Sound {
        samples: sound.samples.iter().map(|v| v * 10f32.powf(0.4 / 20.0)).collect(),
        ..sound.clone()
    };
    near(peak(&gained).unwrap() - peak(&sound).unwrap(), 0.4, 1e-5);
    assert_eq!(
        peak(&Sound {
            rate: 48_000,
            channels: 1,
            samples: vec![0.0; 10]
        }),
        None
    );
}
