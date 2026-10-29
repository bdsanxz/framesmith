//! Sounds placed in the mix to the sample, read from `media-start`, brought to two channels, and
//! silent before, after and past the end of a short source (`docs/spec/sound.md`).

use codec::{AudioSource, SoundKind};
use mix::{Mix, Sound, CHANNELS};

/// A source of our own numbers: frame `i` of channel `c` is `values(i, c)`.
struct Numbers {
    rate: u32,
    channels: u16,
    frames: u64,
    at: u64,
    value: fn(u64, u16) -> f32,
}

impl AudioSource for Numbers {
    fn kind(&self) -> SoundKind {
        SoundKind::Wav
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn frames(&self) -> Option<u64> {
        Some(self.frames)
    }
    fn seek(&mut self, frame: u64) -> base::Result<()> {
        self.at = frame.min(self.frames);
        Ok(())
    }
    fn read(&mut self, samples: &mut [f32]) -> base::Result<usize> {
        let width = usize::from(self.channels);
        let count = ((samples.len() / width) as u64).min(self.frames - self.at) as usize;
        for i in 0..count {
            for c in 0..self.channels {
                samples[i * width + usize::from(c)] = (self.value)(self.at + i as u64, c);
            }
        }
        self.at += count as u64;
        Ok(count)
    }
}

fn source(channels: u16, frames: u64, value: fn(u64, u16) -> f32) -> Box<dyn AudioSource> {
    Box::new(Numbers {
        rate: 48_000,
        channels,
        frames,
        at: 0,
        value,
    })
}

/// Reads the whole mix in pieces of an awkward size, as the film reads it frame by frame.
fn read_all(mut mix: Mix) -> Vec<f32> {
    let mut all = Vec::new();
    let mut piece = vec![0.0; 1_601 * usize::from(CHANNELS)];
    while mix.left() > 0 {
        let given = mix.read(&mut piece).unwrap();
        all.extend_from_slice(&piece[..given * usize::from(CHANNELS)]);
    }
    all
}

fn left(all: &[f32], frame: usize) -> f32 {
    all[frame * 2]
}

fn heard(all: &[f32]) -> Vec<usize> {
    (0..all.len() / 2).filter(|&f| all[f * 2] != 0.0 || all[f * 2 + 1] != 0.0).collect()
}

#[test]
fn a_click_is_at_its_own_sample_not_a_frame_edge() {
    // 3 s at 30 frames a second; a click of one sample at start 1.0001 s, which is frame 48,005.
    let mut mix = Mix::for_picture(90, 30);
    mix.place(Sound {
        first: 48_005,
        end: 48_005 + 4_800,
        skip: 0,
        source: source(2, 1, |_, _| 0.5),
        level: Default::default(),
    })
    .unwrap();
    let all = read_all(mix);
    assert_eq!(heard(&all), vec![48_005]);
    assert_eq!(left(&all, 48_005), 0.5);
}

#[test]
fn a_sound_of_two_seconds_is_heard_for_96000_frames_and_silent_around() {
    let mut mix = Mix::for_picture(120, 30);
    mix.place(Sound {
        first: 48_000,
        end: 144_000,
        skip: 0,
        source: source(2, 480_000, |_, _| 0.25),
        level: Default::default(),
    })
    .unwrap();
    let all = read_all(mix);
    let heard = heard(&all);
    assert_eq!(heard.len(), 96_000);
    assert_eq!((heard[0], *heard.last().unwrap()), (48_000, 143_999));
}

#[test]
fn media_start_makes_the_first_sample_the_source_frame_there() {
    let mut mix = Mix::for_picture(30, 30);
    // Each frame's value is its own number, so the frame heard is read back from the value.
    mix.place(Sound {
        first: 0,
        end: 48_000,
        skip: 24_000,
        source: source(2, 96_000, |i, _| i as f32 / 1e6),
        level: Default::default(),
    })
    .unwrap();
    let all = read_all(mix);
    assert_eq!(left(&all, 0), 24_000.0 / 1e6);
    assert_eq!(left(&all, 1), 24_001.0 / 1e6);
}

#[test]
fn one_channel_is_heard_on_both_sides_3_db_lower() {
    let mut mix = Mix::for_picture(30, 30);
    mix.place(Sound {
        first: 0,
        end: 48_000,
        skip: 0,
        source: source(1, 48_000, |_, _| 0.5),
        level: Default::default(),
    })
    .unwrap();
    let all = read_all(mix);
    let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    for frame in [0, 1, 24_000, 47_999] {
        assert!((all[frame * 2] - expected).abs() < 1e-6, "left at {frame}: {}", all[frame * 2]);
        assert!((all[frame * 2 + 1] - expected).abs() < 1e-6, "right at {frame}");
    }
}

#[test]
fn a_short_source_is_heard_to_its_end_then_silence() {
    let mut mix = Mix::for_picture(90, 30);
    mix.place(Sound {
        first: 0,
        end: 144_000,
        skip: 0,
        source: source(2, 48_000, |_, _| 0.25),
        level: Default::default(),
    })
    .unwrap();
    let all = read_all(mix);
    let heard = heard(&all);
    assert_eq!(heard.len(), 48_000);
    assert_eq!(*heard.last().unwrap(), 47_999);
    assert_eq!(all.len(), 144_000 * 2);
}

#[test]
fn a_sound_past_the_film_is_cut_and_the_length_kept() {
    let mut mix = Mix::for_picture(30, 30);
    mix.place(Sound {
        first: 24_000,
        end: 120_000,
        skip: 0,
        source: source(2, 480_000, |_, _| 0.25),
        level: Default::default(),
    })
    .unwrap();
    let all = read_all(mix);
    assert_eq!(all.len(), 48_000 * 2);
    assert_eq!(heard(&all).len(), 24_000);
}

#[test]
fn sounds_at_the_same_time_are_added() {
    let mut mix = Mix::for_picture(30, 30);
    mix.place(Sound {
        first: 0,
        end: 100,
        skip: 0,
        source: source(2, 480, |_, _| 0.25),
        level: Default::default(),
    })
    .unwrap();
    mix.place(Sound {
        first: 50,
        end: 150,
        skip: 0,
        source: source(2, 480, |_, _| 0.5),
        level: Default::default(),
    })
    .unwrap();
    let all = read_all(mix);
    assert_eq!(
        (left(&all, 49), left(&all, 50), left(&all, 99), left(&all, 100), left(&all, 150)),
        (0.25, 0.75, 0.75, 0.5, 0.0)
    );
}
