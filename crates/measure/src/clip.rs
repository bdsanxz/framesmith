//! A short clip made and read with frx7 alone: its writer puts the file together and its reader
//! gives the facts back, so no `ffmpeg` and no `ffprobe` is started. The interface uses types of
//! this crate; none of frx7's appears in it.
//!
//! The clip is an MP4 file whose picture track holds the facts of a film and placeholder packets.
//! frx7's writer takes packets that are already encoded and no encoder is used here, so the clip
//! cannot be decoded. It serves a test that needs a file whose container facts are known.

use base::{Codec, Color, Hdr, Packet, Rational};
use format::{Media, Registry, TrackSetup, Video};
use std::path::Path;

/// Bytes that stand for an H.264 configuration, as the picture track holds it. Nothing decodes them.
const CONFIGURATION: [u8; 11] = [1, 100, 0, 31, 0xff, 0xe1, 0, 1, 0x67, 1, 0];
/// The track's clock runs `1000 * num` ticks to the second for a rate of `num` over `den`, so that a
/// picture lasts `1000 * den` ticks, a whole number.
const TICKS: u32 = 1000;
/// Decoding may start at every twelfth picture.
const KEY_EVERY: u32 = 12;

/// What a clip is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipSpec {
    pub width: u32,
    pub height: u32,
    /// The number of pictures.
    pub frames: u32,
    /// Pictures a second as a fraction: 30 over 1, or 30000 over 1001.
    pub rate: (u32, u32),
}

/// What reading a clip back gives.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipFacts {
    /// The container's form: `mp4` or `mov`.
    pub container: String,
    /// The name of the picture track's codec, for example `h264`.
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    /// Pictures a second as the track says it, a fraction.
    pub rate: (u32, u32),
    /// The seconds the pictures play.
    pub seconds: f64,
    /// The bytes of the whole file.
    pub bytes: u64,
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(mp4::FORMAT);
    registry
}

/// Writes a clip to `path`, in the container its extension names (`.mp4`).
pub fn write_clip(path: &Path, spec: &ClipSpec) -> Result<(), String> {
    let (num, den) = spec.rate;
    if spec.width == 0 || spec.height == 0 || spec.frames == 0 || num == 0 || den == 0 {
        return Err(format!(
            "a clip needs a size, a rate and at least one picture; asked for {}x{}, {} pictures at {num}/{den}",
            spec.width, spec.height, spec.frames
        ));
    }
    let timescale = num
        .checked_mul(TICKS)
        .ok_or_else(|| format!("the rate {num}/{den} is too large for the track's clock"))?;
    let length = den
        .checked_mul(TICKS)
        .ok_or_else(|| format!("the rate {num}/{den} is too slow for the track's clock"))?;
    let named = |error: base::Error| format!("{}: {error}", path.display());
    let mut muxer = registry().create(path).map_err(named)?;
    let track = muxer
        .add_track(TrackSetup {
            codec: Codec::H264,
            config: CONFIGURATION.to_vec(),
            timescale,
            media: Media::Video(Video {
                width: spec.width,
                height: spec.height,
                color: Some(Color::BT709),
                hdr: Hdr::default(),
                rotation: 0,
                rate: Rational::new(num, den),
                steady: true,
            }),
            expected_packets: Some(u64::from(spec.frames)),
        })
        .map_err(named)?;
    for picture in 0..spec.frames {
        let at = i64::from(picture) * i64::from(length);
        let mut data = vec![0xa5; 24 + (picture as usize * 7) % 40];
        data[..4].copy_from_slice(&picture.to_be_bytes());
        muxer
            .write(
                track,
                Packet {
                    data,
                    pts: at,
                    dts: at,
                    duration: length,
                    key: picture % KEY_EVERY == 0,
                },
            )
            .map_err(named)?;
    }
    muxer.end_track(track).map_err(named)?;
    muxer.finish().map_err(named)?;
    Ok(())
}

/// Reads the facts of a clip, by what its first bytes say and not by its name.
pub fn clip_facts(path: &Path) -> Result<ClipFacts, String> {
    let info = probe::look(path, &registry()).map_err(|error| error.to_string())?;
    let found = info
        .tracks
        .iter()
        .find(|found| found.track.video().is_some())
        .ok_or_else(|| format!("{} has no picture track", path.display()))?;
    let video = found
        .track
        .video()
        .ok_or_else(|| format!("{} has no picture track", path.display()))?;
    Ok(ClipFacts {
        container: info.container.name.to_string(),
        codec: found.track.codec.name(),
        width: video.width,
        height: video.height,
        frames: found.track.samples,
        rate: (video.rate.num, video.rate.den),
        seconds: found.track.seconds(),
        bytes: info.bytes,
    })
}
