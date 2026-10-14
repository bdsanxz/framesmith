//! The sound of a video file, read and written with frx7 alone, so that no `ffmpeg` is started.
//!
//! * [`read_sound`] gives every sample of the first sound track of a file, at the file's own rate
//!   and number of channels, as 32-bit floating point numbers ([`Sound`]); `None` when the file has
//!   no sound track.
//! * [`write_video_with_sound`] writes pictures (H.264) and a sound (AAC) to one MP4 file through
//!   the operating system's encoders. It is a test-only writer: nothing that ships uses it.
//!
//! The interface uses types of this crate; none of frx7's appears in it.

use crate::Frame;
use base::{Codec, Error};
use codec::{AudioEncoderSetup, AudioSource};
use format::{Audio, Media, TrackSetup};
use std::path::Path;

fn codecs() -> codec::Registry {
    let mut codecs = codec::Registry::new();
    codecs.register(backend_os::BACKEND);
    codecs
}

fn formats() -> format::Registry {
    let mut formats = format::Registry::new();
    formats.register(mp4::FORMAT);
    formats
}

/// Every sample of a sound, the channels in turn.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// Sample frames a second.
    pub rate: u32,
    pub channels: u16,
    /// `frames() * channels` numbers: the first sample of every channel, then the second, and so on.
    pub samples: Vec<f32>,
}

impl Sound {
    /// How many sample frames the sound has.
    pub fn frames(&self) -> u64 {
        if self.channels == 0 {
            0
        } else {
            (self.samples.len() / usize::from(self.channels)) as u64
        }
    }
}

/// The sample frames the first sound track of a file plays, by what its container says (its length
/// after the edit list, at the track's own rate); `None` when the file has no sound track.
fn played_frames(path: &Path) -> Result<Option<(u64, u32)>, String> {
    let info = probe::look(path, &formats()).map_err(|error| error.to_string())?;
    let Some(found) = info.tracks.iter().find(|found| found.track.audio().is_some()) else {
        return Ok(None);
    };
    let track = &found.track;
    let rate = track.audio().map_or(0, |audio| audio.sample_rate);
    if track.timescale == 0 || rate == 0 {
        return Err(format!("`{}`: its sound track has no clock or no rate", path.display()));
    }
    let frames = u128::from(track.duration) * u128::from(rate) / u128::from(track.timescale);
    Ok(Some((frames as u64, rate)))
}

/// Whether the file has a sound track, by what its container says.
pub fn has_sound(path: &Path) -> Result<bool, String> {
    Ok(played_frames(path)?.is_some())
}

/// Reads the sound of a file through the system's decoder, counted from the first sample of real
/// sound (the encoder's lead-in is not given), and ends it where the container says the sound
/// ends: a decoder gives the last coded block whole, and what lies past the played length is not
/// sound. `None` when the file has no sound track. The error names the file, and says when the
/// decoder gives less than the container says is played.
pub fn read_sound(path: &Path) -> Result<Option<Sound>, String> {
    let Some((played, rate)) = played_frames(path)? else {
        return Ok(None);
    };
    let named = |error: Error| format!("`{}`: {error}", path.display());
    let mut source: Box<dyn AudioSource> = codecs().audio_source(path).map_err(named)?;
    let channels = source.channels();
    if source.sample_rate() != rate {
        return Err(format!(
            "`{}`: the container says its sound is at {rate} Hz and the decoder gives {} Hz",
            path.display(),
            source.sample_rate()
        ));
    }
    let width = usize::from(channels.max(1));
    let wanted = played as usize * width;
    let mut samples = Vec::with_capacity(wanted);
    let mut piece = vec![0f32; 4096 * width];
    while samples.len() < wanted {
        let read = source.read(&mut piece).map_err(named)?;
        if read == 0 {
            break;
        }
        samples.extend_from_slice(&piece[..read * width]);
    }
    if samples.len() < wanted {
        return Err(format!(
            "`{}`: the container says its sound plays {played} sample frames and the decoder gives {}",
            path.display(),
            samples.len() / width
        ));
    }
    samples.truncate(wanted);
    Ok(Some(Sound { rate, channels, samples }))
}

/// Writes `frames` as H.264 at `rate` pictures a second (as [`crate::write_video`] does) and, when
/// `sound` is given, the sound as AAC at 256,000 bits a second in a second track of the same MP4
/// file, its encoder's lead-in recorded in the file so that a reader gives the sound from its first
/// real sample. Test-only: nothing that ships writes a video this way.
pub fn write_video_with_sound(path: &Path, rate: (u32, u32), frames: &[Frame], sound: Option<&Sound>) -> Result<(), String> {
    let mut extra = Vec::new();
    if let Some(sound) = sound {
        extra.push(sound_track(path, sound)?);
    }
    crate::video::write_video_and(path, rate, frames, extra)
}

/// A sound encoded and ready to be written: its track and its packets.
pub(crate) struct EncodedTrack {
    pub setup: TrackSetup,
    pub config: Vec<u8>,
    pub packets: Vec<base::Packet>,
}

fn sound_track(path: &Path, sound: &Sound) -> Result<EncodedTrack, String> {
    if sound.rate == 0 || sound.channels == 0 || sound.samples.len() % usize::from(sound.channels) != 0 {
        return Err(format!(
            "a sound needs a rate, channels and whole sample frames; given {} Hz, {} channels, {} numbers",
            sound.rate,
            sound.channels,
            sound.samples.len()
        ));
    }
    let named = |error: Error| format!("{}: {error}", path.display());
    let setup = AudioEncoderSetup {
        codec: Codec::Aac,
        sample_rate: sound.rate,
        channels: sound.channels,
        bitrate: 256_000,
    };
    let mut encoder = codecs().audio_encoder(&setup).map_err(named)?;
    let mut packets = Vec::new();
    encoder.send(&sound.samples, &mut packets).map_err(named)?;
    encoder.finish(&mut packets).map_err(named)?;
    Ok(EncodedTrack {
        setup: TrackSetup {
            codec: Codec::Aac,
            config: encoder.config(),
            timescale: sound.rate,
            media: Media::Audio(Audio {
                sample_rate: sound.rate,
                channels: sound.channels,
                lead_in: encoder.lead_in(),
            }),
            expected_packets: Some(packets.len() as u64),
        },
        config: encoder.config(),
        packets,
    })
}

/// A made sound for tests (no recording enters the repository): `frames` sample frames at `rate`,
/// the same in each of `channels` channels, of four steady tones at 220, 347.3, 561.7 and 913.1 Hz
/// whose sum swells and fades in bursts of uneven length, so that no shift of it but none looks
/// like itself. Its largest magnitude is at most `level` (a gain of full scale); `delay` sample
/// frames of silence come first, and the sound is cut at `frames` all the same.
pub fn made_sound(rate: u32, channels: u16, frames: usize, level: f32, delay: usize) -> Sound {
    let fs = f64::from(rate.max(1));
    let tones = [(220.0, 0.35), (347.3, 0.3), (561.7, 0.2), (913.1, 0.15)];
    let mut samples = Vec::with_capacity(frames * usize::from(channels));
    for n in 0..frames {
        let value = if n < delay {
            0.0
        } else {
            let t = (n - delay) as f64 / fs;
            let swell = 0.55 + 0.45 * (2.0 * std::f64::consts::PI * 1.7 * t).sin() * (2.0 * std::f64::consts::PI * 0.43 * t + 0.6).cos();
            let sum: f64 = tones.iter().map(|(f, a)| a * (2.0 * std::f64::consts::PI * f * t).sin()).sum();
            (f64::from(level) * swell * sum) as f32
        };
        for _ in 0..channels {
            samples.push(value);
        }
    }
    Sound { rate, channels, samples }
}
