//! A mix that is too loud is lowered as a whole, by the least amount, and a quiet one is left alone
//! (`docs/spec/sound.md`, section 4). Every expected value is worked by hand: the peak of tones whose crests meet,
//! the gain that brings that peak exactly to the limit, and the crest a sine hides between its samples.

use codec::{AudioSource, SoundKind};
use mix::{peak, Level, Mix, Sound, CHANNELS};

/// A cosine tone of our own numbers: sample `i` is `amp` times cos(2π·freq·i/48000 + phase), on both channels.
struct Tone {
    frames: u64,
    at: u64,
    freq: f64,
    amp: f32,
    phase: f64,
}

impl AudioSource for Tone {
    fn kind(&self) -> SoundKind {
        SoundKind::Wav
    }
    fn sample_rate(&self) -> u32 {
        48_000
    }
    fn channels(&self) -> u16 {
        2
    }
    fn frames(&self) -> Option<u64> {
        Some(self.frames)
    }
    fn seek(&mut self, frame: u64) -> base::Result<()> {
        self.at = frame.min(self.frames);
        Ok(())
    }
    fn read(&mut self, samples: &mut [f32]) -> base::Result<usize> {
        let count = ((samples.len() / 2) as u64).min(self.frames - self.at) as usize;
        for i in 0..count {
            let sample =
                self.amp * (2.0 * std::f64::consts::PI * self.freq * (self.at + i as u64) as f64 / 48_000.0 + self.phase).cos() as f32;
            samples[i * 2] = sample;
            samples[i * 2 + 1] = sample;
        }
        self.at += count as u64;
        Ok(count)
    }
}

/// A one-second sound of the tone, from the mix's first sample frame at the default level.
fn sound(freq: f64, amp: f32, phase: f64) -> Sound {
    Sound {
        first: 0,
        end: 48_000,
        skip: 0,
        source: Box::new(Tone {
            frames: 48_000,
            at: 0,
            freq,
            amp,
            phase,
        }),
        level: Level::default(),
    }
}

/// A one-second mix holding the sounds.
fn mix_of(sounds: Vec<Sound>) -> Mix {
    let mut mix = Mix::for_picture(30, 30);
    for sound in sounds {
        mix.place(sound).unwrap();
    }
    mix
}

/// Reads the whole mix in pieces of an awkward size, as the film reads it frame by frame.
fn read_all(mix: &mut Mix) -> Vec<f32> {
    let mut all = Vec::new();
    let mut piece = vec![0.0; 1_601 * usize::from(CHANNELS)];
    while mix.left() > 0 {
        let given = mix.read(&mut piece).unwrap();
        all.extend_from_slice(&piece[..given * usize::from(CHANNELS)]);
    }
    all
}

/// The true peak of stereo samples, measured as the film measures it.
fn meter_of(samples: &[f32]) -> f64 {
    let mut meter = peak::Meter::new();
    let frames: Vec<[f32; 2]> = samples.as_chunks::<2>().0.iter().map(|pair| [pair[0], pair[1]]).collect();
    let last = *frames.last().unwrap();
    meter.push(&frames);
    meter.peak(last)
}

/// Decibels under full scale of a linear peak.
fn db_under(peak: f64) -> f64 {
    -20.0 * peak.log10()
}

#[test]
fn two_full_scale_tones_lower_to_one_db_under_full_scale() {
    // Two full-level cosines (440 and 880 Hz) meet crest to crest at the first sample frame: the
    // true peak is 2.0, and the least gain that meets the limit is the limit over 2. Lowered, the
    // mix peaks at the limit, within the 0.1 dB of the decisions table.
    let mut mix = mix_of(vec![sound(440.0, 1.0, 0.0), sound(880.0, 1.0, 0.0)]);
    let peak = mix.true_peak().unwrap();
    assert!((peak - 2.0).abs() < 1e-9, "{peak}");
    let gain = peak::gain_for(peak).expect("over the limit");
    assert!(((gain - peak::limit() / 2.0) / (peak::limit() / 2.0)).abs() < 1e-12, "{gain}");
    mix.set_gain(gain as f32);
    mix.rewind().unwrap();
    let lowered = read_all(&mut mix);
    let under = db_under(meter_of(&lowered));
    assert!((under - 1.0).abs() <= 0.05, "{under}");
}

/// The amplitude of `freq` in the left channel: both tones run whole cycles in the second, so each
/// is orthogonal to the other and the correlation reads its amplitude alone.
fn amplitude(samples: &[f32], freq: f64) -> f64 {
    let mut sum = 0.0;
    for (n, pair) in samples.as_chunks::<2>().0.iter().enumerate() {
        sum += pair[0] as f64 * (2.0 * std::f64::consts::PI * freq * n as f64 / 48_000.0).cos();
    }
    2.0 * sum / (samples.len() / 2) as f64
}

#[test]
fn lowered_tones_keep_their_written_ratio() {
    // Tones at the written levels 1.0 and 0.5: at the first sample frame both cosines are 1, so the
    // lowered first frame is the gain times their sum of 1.5; over the whole second each tone still
    // carries twice and half the lowered level, a ratio of 2 however much was lowered.
    let mut mix = mix_of(vec![sound(440.0, 1.0, 0.0), sound(660.0, 0.5, 0.0)]);
    let peak = mix.true_peak().unwrap();
    assert!((peak - 1.5).abs() < 1e-9, "{peak}");
    let gain = peak::gain_for(peak).expect("over the limit");
    mix.set_gain(gain as f32);
    mix.rewind().unwrap();
    let lowered = read_all(&mut mix);
    assert!(((lowered[0] as f64 - gain * 1.5) / (gain * 1.5)).abs() < 1e-7);
    let ratio = amplitude(&lowered, 440.0) / amplitude(&lowered, 660.0);
    assert!((ratio - 2.0).abs() < 1e-4, "{ratio}");
}

#[test]
fn a_peak_between_two_samples_is_found_and_lowered() {
    // A 12,000 Hz sine at a 45-degree phase, amplitude 1: its samples are ±0.70710678, while its
    // crest of 1.0 stands halfway between two samples. The meter must see past the samples.
    let mut mix = mix_of(vec![sound(12_000.0, 1.0, std::f64::consts::FRAC_PI_4)]);
    let peak = mix.true_peak().unwrap();
    assert!(peak > 0.9, "past the sample peak 0.70710678: {peak}");
    assert!((db_under(peak) - 0.0).abs() <= 0.1, "{peak}");
    let gain = peak::gain_for(peak).expect("over the limit");
    assert!(
        (peak::lowered_db(gain) - 1.0).abs() <= 0.1,
        "lowered by a decibel: {}",
        peak::lowered_db(gain)
    );
    mix.set_gain(gain as f32);
    mix.rewind().unwrap();
    let under = db_under(meter_of(&read_all(&mut mix)));
    assert!((under - 1.0).abs() <= 0.05, "{under}");
}

#[test]
fn a_quiet_mix_has_no_sample_changed() {
    // One 440 Hz cosine at 0.5: its peak of 0.5 is under the limit, so there is no gain, and what
    // is read is the tone itself, to the bit.
    let mut mix = mix_of(vec![sound(440.0, 0.5, 0.0)]);
    let peak = mix.true_peak().unwrap();
    assert!((peak - 0.5).abs() < 1e-9, "{peak}");
    assert_eq!(peak::gain_for(peak), None);
    mix.rewind().unwrap();
    let heard = read_all(&mut mix);
    assert_eq!(heard.len(), 48_000 * 2);
    for (i, pair) in heard.as_chunks::<2>().0.iter().enumerate() {
        let wanted = 0.5 * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48_000.0).cos() as f32;
        assert_eq!([pair[0], pair[1]], [wanted, wanted], "frame {i}");
    }
}

#[test]
fn the_lowering_in_decibels_is_twenty_times_the_log_of_the_gain() {
    // Halving is 6.0206 dB, worked by hand: 20 · log10(2).
    assert!((peak::lowered_db(0.5) - 6.020599913279624).abs() < 1e-9);
    assert_eq!(peak::gain_for(peak::limit()), None);
    assert_eq!(peak::gain_for(peak::limit() - 1e-12), None);
}
