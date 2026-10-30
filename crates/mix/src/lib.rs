//! The sound mix of a composition (`docs/spec/render-command.md` section 5.2): 48,000 sample
//! frames a second in two channels, as long as the picture to the sample. Each sound is placed in
//! it to the sample and read from its source as the mix is read (`docs/spec/sound.md`).

use audio::Converter;
use codec::AudioSource;

pub mod peak;

/// Sample frames a second of the mix.
pub const SAMPLE_RATE: u32 = 48_000;
/// Channels of the mix: left, then right.
pub const CHANNELS: u16 = 2;

/// Source frames asked of a source at a time.
const CHUNK: usize = 4_096;

/// The level of a sound in the mix (`docs/spec/sound.md`, section 3a): its `volume` times the value of its curve, which is
/// the straight line between its points, the value of the nearest point before the first and after the last.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// The `volume` of the sound: 1 when none is written.
    pub volume: f64,
    /// The points of the curve as `(seconds from the sound's first sample frame, value)`, in order of time.
    pub points: Vec<(f64, f64)>,
}

impl Default for Level {
    /// A level of 1 at every time: a sound with no `volume` and no points.
    fn default() -> Level {
        Level {
            volume: 1.0,
            points: Vec::new(),
        }
    }
}

impl Level {
    /// The level at `seconds` from the sound's first sample frame, in 32-bit floating point.
    pub fn at(&self, seconds: f64) -> f32 {
        (self.volume * self.curve(seconds)) as f32
    }

    /// The value of the curve at `seconds`.
    fn curve(&self, seconds: f64) -> f64 {
        let Some(last) = self.points.last() else {
            return 1.0;
        };
        // The points are in order of time, so the ones at or before `seconds` come first.
        let after = self.points.partition_point(|point| point.0 <= seconds);
        if after == self.points.len() {
            return last.1;
        }
        if after == 0 {
            return self.points[0].1;
        }
        let (from, to) = (self.points[after - 1], self.points[after]);
        from.1 + (to.1 - from.1) * (seconds - from.0) / (to.0 - from.0)
    }
}

/// A sound to place: from sample frame `first` of the mix up to `end` (not on it), heard from
/// sample frame `skip` of its source, at its `level`.
pub struct Sound {
    pub first: u64,
    pub end: u64,
    pub skip: u64,
    pub source: Box<dyn AudioSource>,
    pub level: Level,
}

/// A placed sound as it is read.
struct Placed {
    first: u64,
    end: u64,
    /// The source's sample frame first heard: rewinding moves the source back here.
    skip: u64,
    source: Box<dyn AudioSource>,
    converter: Converter,
    /// Frames of the source asked for at a time, the channels in turn.
    chunk: Vec<f32>,
    /// Stereo frames at the mix's rate not yet added, from `taken` on.
    ready: Vec<f32>,
    /// Frames of `ready` already added.
    taken: usize,
    /// The source gave its last sample.
    ended: bool,
    /// The level the sound is heard at, counted from its first sample frame `first`.
    level: Level,
}

impl Placed {
    /// Makes at least `frames` stereo frames ready, or as many as the source still gives.
    fn fill(&mut self, frames: usize) -> base::Result<()> {
        let channels = usize::from(CHANNELS);
        while (self.ready.len() / channels) - self.taken < frames && !self.ended {
            if self.taken > 0 {
                self.ready.drain(..self.taken * channels);
                self.taken = 0;
            }
            let read = self.source.read(&mut self.chunk)?;
            let width = usize::from(self.source.channels());
            self.converter.push(&self.chunk[..read * width], &mut self.ready);
            if read == 0 {
                self.converter.finish(&mut self.ready);
                self.ended = true;
            }
        }
        Ok(())
    }
}

/// The mix of a film, read from its first sample frame to its last.
pub struct Mix {
    length: u64,
    read: u64,
    sounds: Vec<Placed>,
    /// The amount every sample is multiplied by as it is read: 1 when the mix needs no lowering
    /// (`docs/spec/sound.md`, section 4).
    gain: f32,
}

impl Mix {
    /// The mix of a film of `frames` frames at `fps` frames a second: its length is the picture's
    /// length in seconds times 48,000, rounded to the nearest sample frame (a half rounds up).
    pub fn for_picture(frames: u64, fps: u32) -> Mix {
        Mix {
            length: length_for_picture(frames, fps),
            read: 0,
            sounds: Vec::new(),
            gain: 1.0,
        }
    }

    /// Places a sound: moves its source to `skip` and makes the conversion to the mix's form.
    /// Nothing of it is heard past the end of the mix.
    pub fn place(&mut self, sound: Sound) -> base::Result<()> {
        let Sound {
            first,
            end,
            skip,
            mut source,
            level,
        } = sound;
        source.seek(skip)?;
        let converter = Converter::to_mix(source.sample_rate(), source.channels())?;
        let chunk = vec![0.0; CHUNK * usize::from(source.channels())];
        self.sounds.push(Placed {
            first,
            end: end.min(self.length),
            skip,
            source,
            converter,
            chunk,
            ready: Vec::new(),
            taken: 0,
            ended: false,
            level,
        });
        Ok(())
    }

    /// Lowers the mix as a whole: every sample read from now on is multiplied by `gain`
    /// (`docs/spec/sound.md`, section 4).
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }

    /// The true peak of the whole mix (`peak::Meter`): reads the mix to its end and throws the
    /// samples away. Rewind and read again to hear it.
    pub fn true_peak(&mut self) -> base::Result<f64> {
        let mut meter = peak::Meter::new();
        let mut frames = vec![[0.0; 2]; CHUNK];
        let mut flat = vec![0.0; CHUNK * usize::from(CHANNELS)];
        let mut last = [0.0; 2];
        while self.left() > 0 {
            let given = self.read(&mut flat)?;
            for (slot, pair) in frames.iter_mut().zip(flat.as_chunks::<2>().0).take(given) {
                *slot = *pair;
                last = *slot;
            }
            meter.push(&frames[..given]);
        }
        Ok(meter.peak(last))
    }

    /// Reads the mix again from its first sample frame, with the gain `set_gain` gave: moves every
    /// source back to where it was placed and makes the conversion again.
    pub fn rewind(&mut self) -> base::Result<()> {
        for sound in &mut self.sounds {
            sound.source.seek(sound.skip)?;
            sound.converter = Converter::to_mix(sound.source.sample_rate(), sound.source.channels())?;
            sound.chunk = vec![0.0; CHUNK * usize::from(sound.source.channels())];
            sound.ready.clear();
            sound.taken = 0;
            sound.ended = false;
        }
        self.read = 0;
        Ok(())
    }

    /// The length in sample frames.
    pub fn length(&self) -> u64 {
        self.length
    }

    /// Sample frames not yet read.
    pub fn left(&self) -> u64 {
        self.length - self.read
    }

    /// Fills `samples` (the channels in turn) with the next sample frames, at most as many as
    /// `samples` holds whole frames and as are left, and says how many frames were given. Where
    /// no sound plays the mix is silence; sounds that play together are added.
    pub fn read(&mut self, samples: &mut [f32]) -> base::Result<usize> {
        let channels = usize::from(CHANNELS);
        let room = (samples.len() / channels) as u64;
        let given = room.min(self.left());
        let from = self.read;
        let until = from + given;
        samples[..given as usize * channels].fill(0.0);
        for sound in &mut self.sounds {
            let start = sound.first.max(from);
            let stop = sound.end.min(until);
            if start >= stop {
                continue;
            }
            let wanted = (stop - start) as usize;
            sound.fill(wanted)?;
            let have = (sound.ready.len() / channels - sound.taken).min(wanted);
            let gain = self.gain;
            for k in 0..have {
                // The level is taken at each sample frame, so it moves from one sample to the next.
                let frame = start + k as u64;
                let level = sound.level.at((frame - sound.first) as f64 / f64::from(SAMPLE_RATE));
                let at = (frame - from) as usize * channels;
                for channel in 0..channels {
                    // The whole mix is lowered by the one amount (`sound.md`, section 4): with the
                    // gain 1 the samples are what they were, to the bit.
                    samples[at + channel] += sound.ready[(sound.taken + k) * channels + channel] * level * gain;
                }
            }
            // What the source no longer gives is silence: the frames are counted as heard.
            sound.taken += have;
        }
        self.read = until;
        Ok(given as usize)
    }
}

/// The picture's length in seconds (`frames / fps`) times 48,000, rounded to the nearest whole
/// sample frame, a half up. For a rate that divides 48,000 (24, 30, 60 and the like) it is exact.
pub fn length_for_picture(frames: u64, fps: u32) -> u64 {
    if fps == 0 {
        return 0;
    }
    let scaled = u128::from(frames) * u128::from(SAMPLE_RATE);
    let fps = u128::from(fps);
    ((scaled * 2 + fps) / (fps * 2)) as u64
}
