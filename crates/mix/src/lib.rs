//! The sound mix of a composition (`docs/spec/render-command.md` section 5.2): 48,000 sample
//! frames a second in two channels, as long as the picture to the sample. This version places no
//! sound in it, so every sample is silence; sounds are placed by a later ticket.

/// Sample frames a second of the mix.
pub const SAMPLE_RATE: u32 = 48_000;
/// Channels of the mix: left, then right.
pub const CHANNELS: u16 = 2;

/// The mix of a film, read from its first sample frame to its last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mix {
    length: u64,
    read: u64,
}

impl Mix {
    /// The mix of a film of `frames` frames at `fps` frames a second: its length is the picture's
    /// length in seconds times 48,000, rounded to the nearest sample frame (a half rounds up).
    pub fn for_picture(frames: u64, fps: u32) -> Mix {
        Mix {
            length: length_for_picture(frames, fps),
            read: 0,
        }
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
    /// `samples` holds whole frames and as are left, and says how many frames were given.
    pub fn read(&mut self, samples: &mut [f32]) -> usize {
        let room = (samples.len() / usize::from(CHANNELS)) as u64;
        let given = room.min(self.left());
        let values = given as usize * usize::from(CHANNELS);
        samples[..values].fill(0.0);
        self.read += given;
        given as usize
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
