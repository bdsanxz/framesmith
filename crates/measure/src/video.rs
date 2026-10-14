//! Videos read and written with frx7 alone, so that no `ffmpeg` and no `ffprobe` is started.
//!
//! * [`video_facts`] looks at a file without decoding it: the picture track's codec, size, number
//!   of pictures, frame rate and length.
//! * [`PictureReader`] gives the pictures of a file one after the other, in the order they are
//!   shown, as plain 8-bit red, green and blue ([`Frame`]).
//! * [`write_video`] encodes pictures with the operating system's H.264 encoder and writes them to
//!   an MP4 file. It is the test-only writer of this repository: nothing that ships uses it.
//!
//! The interface uses types of this crate; none of frx7's appears in it.

use crate::Frame;
use base::{Codec, Color, Error, Frame as Decoded, Picture, PixelFormat, Rational};
use codec::{Hardware, Pictures, RateControl, VideoEncoder, VideoEncoderSetup};
use format::{Media, Muxer, TrackSetup, Video};
use std::path::Path;

fn formats() -> format::Registry {
    let mut formats = format::Registry::new();
    formats.register(mp4::FORMAT);
    formats
}

fn codecs() -> codec::Registry {
    let mut codecs = codec::Registry::new();
    codecs.register(backend_os::BACKEND);
    codecs
}

/// What a video file says of its picture track, read without decoding a picture.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoFacts {
    /// The container's form: `mp4` or `mov`.
    pub container: String,
    /// The name of the picture track's codec, for example `h264`.
    pub codec: String,
    /// The size the pictures are shown at: the size the stream's headers give, after cropping.
    pub width: u32,
    pub height: u32,
    /// The number of pictures the track holds.
    pub frames: u64,
    /// Pictures a second as the track says it, a fraction.
    pub rate: (u32, u32),
    /// The length the track plays, as a count of ticks of a clock of `timescale` ticks a second.
    pub ticks: u64,
    pub timescale: u32,
}

impl VideoFacts {
    /// The seconds the pictures play.
    pub fn seconds(&self) -> f64 {
        if self.timescale == 0 {
            0.0
        } else {
            self.ticks as f64 / f64::from(self.timescale)
        }
    }
}

/// Reads the facts of the first picture track of a file, by what its first bytes say and not by
/// its name. The error names the file.
pub fn video_facts(path: &Path) -> Result<VideoFacts, String> {
    let info = probe::look(path, &formats()).map_err(|error| error.to_string())?;
    let found = info
        .tracks
        .iter()
        .find(|found| found.track.video().is_some())
        .ok_or_else(|| format!("`{}` has no picture track", path.display()))?;
    let video = found
        .track
        .video()
        .ok_or_else(|| format!("`{}` has no picture track", path.display()))?;
    // The headers of the stream give the size after cropping; the container's size is the fall-back.
    let (width, height) = found.facts.map_or((video.width, video.height), |facts| (facts.width, facts.height));
    Ok(VideoFacts {
        container: info.container.name.to_string(),
        codec: found.track.codec.name(),
        width,
        height,
        frames: found.track.samples,
        rate: (video.rate.num, video.rate.den),
        ticks: found.track.duration,
        timescale: found.track.timescale,
    })
}

/// The pictures of a video file, one after the other in the order they are shown.
pub struct PictureReader {
    pictures: Pictures,
    /// What the track says its numbers mean, when it says.
    color: Option<Color>,
    path: String,
}

/// Opens the picture track of a file and tries its first picture, so that a stream the system does
/// not decode is refused here. The error names the file.
pub fn open_pictures(path: &Path) -> Result<PictureReader, String> {
    let formats = formats();
    let demuxer = formats.open(path).map_err(|error| error.to_string())?;
    let color = demuxer
        .tracks()
        .iter()
        .find_map(|track| track.video().map(|video| video.color))
        .flatten();
    drop(demuxer);
    let pictures = Pictures::open(path, &formats, &codecs(), Hardware::Any).map_err(|error| error.to_string())?;
    Ok(PictureReader {
        pictures,
        color,
        path: path.display().to_string(),
    })
}

impl PictureReader {
    /// How many pictures the track has.
    pub fn count(&self) -> u64 {
        self.pictures.count()
    }

    /// The next picture, or `None` after the last. The error names the file and, when the system
    /// could not decode a picture, its number.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, String> {
        let Some(numbered) = self.pictures.next_picture().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let named = |error: Error| format!("`{}`: picture {}: {error}", self.path, numbered.number);
        let picture = numbered.frame.surface.read().map_err(named)?;
        let rgba = pixel::convert(&picture, self.color, PixelFormat::Rgba8, None).map_err(named)?;
        Ok(Some(without_alpha(&rgba)))
    }
}

fn without_alpha(rgba: &Picture) -> Frame {
    let (width, height) = (rgba.width as usize, rgba.height as usize);
    let plane = &rgba.planes[0];
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in 0..height {
        let line = &plane.data[row * plane.stride..row * plane.stride + width * 4];
        for pixel in line.as_chunks::<4>().0 {
            rgb.extend_from_slice(&pixel[..3]);
        }
    }
    Frame {
        width: rgba.width,
        height: rgba.height,
        rgb,
    }
}

/// A picture of red, green, blue and full alpha from 8-bit red, green and blue.
fn with_alpha(frame: &Frame) -> Picture {
    let mut picture = Picture::new(PixelFormat::Rgba8, frame.width, frame.height);
    for (rgba, rgb) in picture.planes[0]
        .data
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(frame.rgb.as_chunks::<3>().0)
    {
        rgba[..3].copy_from_slice(rgb);
        rgba[3] = 255;
    }
    picture
}

/// Bits a second the encoder is asked for: enough that a picture of hard edges is kept to within a
/// level or two.
fn bit_rate(frame: &Frame, (num, den): (u32, u32)) -> u32 {
    let pixels = u64::from(frame.width) * u64::from(frame.height);
    let rate = u64::from(num) / u64::from(den.max(1)).max(1);
    (pixels * rate.max(1) * 3).clamp(500_000, 200_000_000) as u32
}

/// Encodes `frames` (all of one size) as H.264 with the operating system's encoder, at `rate`
/// pictures a second, and writes an MP4 file. Decoding may start at every twelfth picture;
/// pictures are not reordered. Test-only: nothing that ships writes a video this way.
pub fn write_video(path: &Path, rate: (u32, u32), frames: &[Frame]) -> Result<(), String> {
    let Some(first) = frames.first() else {
        return Err("a video needs at least one picture".to_string());
    };
    let (num, den) = rate;
    if num == 0 || den == 0 || first.width == 0 || first.height == 0 {
        return Err(format!(
            "a video needs a size and a rate; asked for {}x{} at {num}/{den}",
            first.width, first.height
        ));
    }
    if let Some(other) = frames
        .iter()
        .find(|frame| (frame.width, frame.height) != (first.width, first.height))
    {
        return Err(format!(
            "a video has one size, and its pictures are {}x{} and {}x{}",
            first.width, first.height, other.width, other.height
        ));
    }
    let setup = VideoEncoderSetup {
        codec: Codec::H264,
        width: first.width,
        height: first.height,
        rate: Rational::new(num, den),
        bits: 8,
        rate_control: RateControl::Bitrate(bit_rate(first, rate)),
        key_distance: 12,
        reorder: false,
        color: Color::BT709,
        hdr: base::Hdr::default(),
        hardware: Hardware::Any,
    };
    let named = |error: Error| format!("{}: {error}", path.display());
    let mut encoder = codecs().video_encoder(&setup).map_err(named)?;
    let timescale = encoder.timescale();
    let ticks = u64::from(timescale) * u64::from(den) / u64::from(num);
    if ticks == 0 || ticks * u64::from(num) != u64::from(timescale) * u64::from(den) {
        return Err(format!(
            "a picture at {num}/{den} a second does not last a whole number of the encoder's {timescale} ticks a second"
        ));
    }
    let mut muxer = formats().create(path).map_err(named)?;
    let track = muxer
        .add_track(TrackSetup {
            codec: Codec::H264,
            config: Vec::new(),
            timescale,
            media: Media::Video(Video {
                width: first.width,
                height: first.height,
                color: Some(Color::BT709),
                hdr: base::Hdr::default(),
                rotation: 0,
                rate: Rational::new(num, den),
                steady: true,
            }),
            expected_packets: Some(frames.len() as u64),
        })
        .map_err(named)?;
    let mut configured = false;
    for (number, frame) in frames.iter().enumerate() {
        let planes = pixel::convert(&with_alpha(frame), None, PixelFormat::Nv12, Some(Color::BT709)).map_err(named)?;
        let surface = encoder.surface().map_err(named)?;
        surface.write(&planes).map_err(named)?;
        encoder
            .send(Decoded {
                pts: number as i64 * ticks as i64,
                duration: ticks as u32,
                surface,
            })
            .map_err(named)?;
        take_packets(encoder.as_mut(), muxer.as_mut(), track, &mut configured).map_err(named)?;
    }
    encoder.finish().map_err(named)?;
    take_packets(encoder.as_mut(), muxer.as_mut(), track, &mut configured).map_err(named)?;
    muxer.end_track(track).map_err(named)?;
    muxer.finish().map_err(named)?;
    if !configured {
        return Err(format!("{}: the encoder gave no picture", path.display()));
    }
    Ok(())
}

/// Moves the packets the encoder has ready into the file, giving the track its configuration with
/// the first.
fn take_packets(encoder: &mut dyn VideoEncoder, muxer: &mut dyn Muxer, track: usize, configured: &mut bool) -> base::Result<()> {
    while let Some(packet) = encoder.receive()? {
        if !*configured {
            let config = encoder
                .config()
                .ok_or_else(|| Error::invalid("the encoder gave a packet but no configuration"))?;
            muxer.set_config(track, config)?;
            *configured = true;
        }
        muxer.write(track, packet)?;
    }
    Ok(())
}
