//! The level of a sound in the mix, sample by sample: its `volume`, its `<volume>` points, the level before the first point
//! and after the last, the straight line between two points, and a sound cut at the end of the film (`docs/spec/sound.md`,
//! section 3a and section 2). Every expected value is worked by hand from the rule in the spec, from the source's values.

use codec::{AudioSource, SoundKind};
use mix::{Level, Mix, Sound, CHANNELS};

/// A source of our own numbers: frame `i` of channel `c` is `value(i, c)`.
struct Numbers {
    frames: u64,
    at: u64,
    value: fn(u64, u16) -> f32,
}

impl AudioSource for Numbers {
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
            for c in 0..2 {
                samples[i * 2 + usize::from(c)] = (self.value)(self.at + i as u64, c);
            }
        }
        self.at += count as u64;
        Ok(count)
    }
}

fn source(frames: u64, value: fn(u64, u16) -> f32) -> Box<dyn AudioSource> {
    Box::new(Numbers { frames, at: 0, value })
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

/// The left channel of the mix at sample frame `frame`.
fn left(all: &[f32], frame: usize) -> f32 {
    all[frame * 2]
}

/// A sound of `seconds` seconds from sample frame 0 at `level`, of a source `value`.
fn sound_at(seconds: u64, level: Level, value: fn(u64, u16) -> f32) -> Sound {
    Sound {
        first: 0,
        end: seconds * 48_000,
        skip: 0,
        source: source(seconds * 48_000, value),
        level,
    }
}

fn steady(_: u64, _: u16) -> f32 {
    0.25
}

fn eighths(sample: u64, _: u16) -> f32 {
    (sample % 8) as f32 * 0.125
}

#[test]
fn a_volume_of_one_half_is_half_the_amplitude_of_every_sample() {
    let mut plain = Mix::for_picture(30, 30);
    plain.place(sound_at(1, Level::default(), eighths)).unwrap();
    let plain = read_all(plain);
    let mut halved = Mix::for_picture(30, 30);
    halved
        .place(sound_at(
            1,
            Level {
                volume: 0.5,
                points: Vec::new(),
            },
            eighths,
        ))
        .unwrap();
    let halved = read_all(halved);
    for frame in 0..48_000 {
        // Worked by hand: the source at frame n is (n mod 8) / 8; at a volume of 0.5 it is (n mod 8) / 16.
        assert_eq!(left(&plain, frame), (frame % 8) as f32 / 8.0, "no volume, frame {frame}");
        assert_eq!(left(&halved, frame), (frame % 8) as f32 / 16.0, "volume 0.5, frame {frame}");
        assert_eq!(halved[frame * 2 + 1], (frame % 8) as f32 / 16.0, "right channel, frame {frame}");
    }
}

#[test]
fn points_at_one_and_two_seconds_hold_then_ramp_then_silence() {
    let mut mix = Mix::for_picture(90, 30);
    let level = Level {
        volume: 1.0,
        points: vec![(1.0, 1.0), (2.0, 0.0)],
    };
    mix.place(sound_at(3, level, steady)).unwrap();
    let all = read_all(mix);
    // Full amplitude until 1 second: the source's 0.25, as it is.
    assert_eq!(left(&all, 0), 0.25);
    assert_eq!(left(&all, 47_999), 0.25);
    assert_eq!(left(&all, 48_000), 0.25, "at 1 second the level is the first point's 1");
    // The level 0.5 at 1.5 seconds: a quarter of the source is an eighth.
    assert_eq!(left(&all, 72_000), 0.125);
    // Silence from 2 seconds on, to the end of the sound.
    for frame in 96_000..144_000 {
        assert_eq!(left(&all, frame), 0.0, "silence at frame {frame}");
    }
}

#[test]
fn before_the_first_point_and_after_the_last_the_level_is_that_of_the_nearest_point() {
    let mut mix = Mix::for_picture(90, 30);
    let level = Level {
        volume: 1.0,
        points: vec![(1.0, 0.5), (2.0, 0.2)],
    };
    mix.place(sound_at(3, level, |_, _| 1.0)).unwrap();
    let all = read_all(mix);
    assert_eq!(left(&all, 0), 0.5, "before the first point holds its level");
    assert_eq!(left(&all, 47_999), 0.5, "just before the first point");
    assert_eq!(left(&all, 96_000), 0.2, "at the last point");
    assert_eq!(left(&all, 143_999), 0.2, "after the last point holds its level");
}

#[test]
fn the_level_moves_from_sample_to_sample_between_two_points() {
    let mut mix = Mix::for_picture(90, 30);
    let level = Level {
        volume: 1.0,
        points: vec![(1.0, 1.0), (2.0, 0.0)],
    };
    mix.place(sound_at(3, level, |_, _| 1.0)).unwrap();
    let all = read_all(mix);
    // Worked by hand: between 1 and 2 seconds the level falls by 1 over 48,000 sample frames, one sample at a time.
    let mut previous = f32::INFINITY;
    for k in 0..48_000 {
        let got = left(&all, 48_000 + k);
        let expected = (1.0 - k as f64 / 48_000.0) as f32;
        assert!(
            (got - expected).abs() < 1e-6,
            "sample {k} after 1 second: {got}, expected {expected}"
        );
        if k > 0 {
            assert!(got < previous, "the level does not fall at sample {k}: {previous} then {got}");
        }
        previous = got;
    }
}

#[test]
fn a_sound_longer_than_the_film_is_cut_at_the_last_sample_frame() {
    // A film of one second is 48,000 sample frames; a sound of two seconds from its start is heard for all of them.
    let mut mix = Mix::for_picture(30, 30);
    mix.place(Sound {
        first: 0,
        end: 96_000,
        skip: 0,
        source: source(96_000, steady),
        level: Level::default(),
    })
    .unwrap();
    assert_eq!(mix.length(), 48_000);
    let all = read_all(mix);
    assert_eq!(all.len(), 48_000 * 2, "the mix is as long as the film");
    assert!(
        all.iter().all(|&value| value == 0.25),
        "every sample of the film is the sound's own"
    );
}
