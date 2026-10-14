//! A short clip made and read with frx7 alone: the system's H.264 encoder encodes pictures of our
//! own making, frx7's writer puts the file together and its reader gives the facts back, so no
//! `ffmpeg` and no `ffprobe` is started. The interface uses types of this crate; none of frx7's
//! appears in it.
//!
//! The clip is an MP4 file whose picture track holds real H.264 pictures, so it decodes. It serves
//! a test that needs a file whose container facts are known.

use crate::{write_video, Frame};
use format::Registry;
use std::path::Path;

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

/// Picture `number` of a clip: a gradient that moves with the number, so that no two pictures are alike.
fn picture(spec: &ClipSpec, number: u32) -> Frame {
    let mut rgb = Vec::with_capacity((spec.width * spec.height * 3) as usize);
    for y in 0..spec.height {
        for x in 0..spec.width {
            rgb.extend_from_slice(&[
                ((x * 255) / spec.width.max(1)) as u8,
                ((y * 255) / spec.height.max(1)) as u8,
                (number.wrapping_mul(23) % 256) as u8,
            ]);
        }
    }
    Frame {
        width: spec.width,
        height: spec.height,
        rgb,
    }
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
    let pictures: Vec<Frame> = (0..spec.frames).map(|number| picture(spec, number)).collect();
    write_video(path, spec.rate, &pictures)
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
