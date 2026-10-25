//! The sound mix of a composition (`docs/spec/render-command.md` section 5.2): 48,000 sample
//! frames a second in two channels, as long as the picture to the sample. Each sound is placed in
//! it to the sample and read from its source as the mix is read (`docs/spec/sound.md`).

use audio::Converter;
use codec::AudioSource;

/// Sample frames a second of the mix.
pub const SAMPLE_RATE: u32 = 48_000;
/// Channels of the mix: left, then right.
pub const CHANNELS: u16 = 2;

/// Source frames asked of a source at a time.
const CHUNK: usize = 4_096;

/// A sound to place: from sample frame `first` of the mix up to `end` (not on it), heard from
/// sample frame `skip` of its source.
pub struct Sound {
    pub first: u64,
    pub end: u64,
    pub skip: u64,
    pub source: Box<dyn AudioSource>,
}

/// A placed sound as it is read.
struct Placed {
    first: u64,
    end: u64,
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
}

impl Mix {
    /// The mix of a film of `frames` frames at `fps` frames a second: its length is the picture's
    /// length in seconds times 48,000, rounded to the nearest sample frame (a half rounds up).
    pub fn for_picture(frames: u64, fps: u32) -> Mix {
        Mix {
            length: length_for_picture(frames, fps),
            read: 0,
            sounds: Vec::new(),
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
        } = sound;
        source.seek(skip)?;
        let converter = Converter::to_mix(source.sample_rate(), source.channels())?;
        let chunk = vec![0.0; CHUNK * usize::from(source.channels())];
        self.sounds.push(Placed {
            first,
            end: end.min(self.length),
            source,
            converter,
            chunk,
            ready: Vec::new(),
            taken: 0,
            ended: false,
        });
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
            let source = &sound.ready[sound.taken * channels..(sound.taken + have) * channels];
            let at = (start - from) as usize * channels;
            for (out, value) in samples[at..at + have * channels].iter_mut().zip(source) {
                *out += value;
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
