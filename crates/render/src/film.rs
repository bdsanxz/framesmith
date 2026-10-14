//! The film: the frames of a scene, drawn in order, brought to the planes the encoder takes, encoded
//! by the system's H.264 encoder and written as an MP4 file, all through frx7's public traits
//! (`docs/spec/render-command.md` section 5).

use std::fmt;
use std::fs;
use std::path::Path;
use std::time::Instant;

use base::{Codec, Color, Error, Frame, Hdr, Picture, PixelFormat, Rational};
use codec::{Hardware, RateControl, VideoEncoder, VideoEncoderSetup};
use draw::Renderer;
use format::{Media, Muxer, TrackSetup, Video};

use crate::scene::Scene;

/// The quality `render.quality: standard` asks the encoder for, from 0 to 1 (decision D22).
const STANDARD_QUALITY: f32 = 0.65;
/// The most seconds between two key pictures (decision D22).
const KEY_SECONDS: f64 = 2.0;

/// What was written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Written {
    /// The number of frames in the file.
    pub frames: u64,
    /// The size of the file in bytes.
    pub bytes: u64,
    /// The seconds from the start of the drawing to the end of the file.
    pub seconds: f64,
    /// Whether the machine's hardware encoded the pictures.
    pub hardware: bool,
}

/// Why the film could not be made: the reason, as one sentence without a full stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure(String);

impl Failure {
    fn new(message: impl Into<String>) -> Failure {
        Failure(message.into())
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failure {}

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

/// Draws every frame of `scene` with `renderer` and writes the film to `output`, making the
/// folder it is in when that is missing. A film that cannot be finished leaves no file at
/// `output`.
pub fn write_film(scene: &Scene, renderer: &mut dyn Renderer, output: &Path) -> Result<Written, Failure> {
    if let Some(folder) = output.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        fs::create_dir_all(folder).map_err(|error| Failure::new(format!("the folder for the file could not be made: {error}")))?;
    }
    let began = Instant::now();
    tracing::info!(
        frames = scene.frames(),
        width = scene.width(),
        height = scene.height(),
        fps = scene.fps(),
        renderer = renderer.name(),
        "render started"
    );
    let written = make(scene, renderer, output, began).inspect_err(|_| {
        // A film that was not finished leaves no file that looks finished.
        let _ = fs::remove_file(output);
    })?;
    tracing::info!(frames = written.frames, bytes = written.bytes, "render finished");
    Ok(written)
}

fn make(scene: &Scene, renderer: &mut dyn Renderer, output: &Path, began: Instant) -> Result<Written, Failure> {
    let named = |error: Error| Failure::new(error.to_string());
    let rate = Rational::new(scene.fps(), 1);
    let setup = VideoEncoderSetup {
        codec: Codec::H264,
        width: scene.width(),
        height: scene.height(),
        rate,
        bits: 8,
        rate_control: RateControl::Quality(STANDARD_QUALITY),
        key_distance: VideoEncoderSetup::key_distance_within(KEY_SECONDS, rate),
        reorder: true,
        color: Color::BT709,
        hdr: Hdr::default(),
        hardware: Hardware::Any,
    };
    let mut encoder = codecs().video_encoder(&setup).map_err(named)?;
    tracing::debug!(hardware = encoder.hardware(), "encoder started");
    let timescale = encoder.timescale();
    let ticks = u64::from(timescale) / u64::from(scene.fps());
    if ticks == 0 || ticks * u64::from(scene.fps()) != u64::from(timescale) {
        return Err(Failure::new(format!(
            "a frame at {} frames a second does not last a whole number of the encoder's {timescale} ticks a second",
            scene.fps()
        )));
    }
    let mut muxer = formats().create(output).map_err(named)?;
    let track = muxer
        .add_track(TrackSetup {
            codec: Codec::H264,
            config: Vec::new(),
            timescale,
            media: Media::Video(Video {
                width: scene.width(),
                height: scene.height(),
                color: Some(Color::BT709),
                hdr: Hdr::default(),
                rotation: 0,
                rate,
                steady: true,
            }),
            expected_packets: Some(scene.frames()),
        })
        .map_err(named)?;

    let mut configured = false;
    for number in 0..scene.frames() {
        let drawn = scene
            .picture(number, renderer)
            .map_err(|error| Failure::new(format!("frame {number}: {error}")))?;
        let mut rgba = Picture::new(PixelFormat::Rgba8, drawn.width(), drawn.height());
        rgba.planes[0].data.copy_from_slice(drawn.pixels());
        let planes = pixel::convert(&rgba, None, PixelFormat::Nv12, Some(Color::BT709)).map_err(named)?;
        let surface = encoder.surface().map_err(named)?;
        surface.write(&planes).map_err(named)?;
        encoder
            .send(Frame {
                pts: number as i64 * ticks as i64,
                duration: ticks as u32,
                surface,
            })
            .map_err(named)?;
        take_packets(encoder.as_mut(), muxer.as_mut(), track, &mut configured).map_err(named)?;
    }
    encoder.finish().map_err(named)?;
    take_packets(encoder.as_mut(), muxer.as_mut(), track, &mut configured).map_err(named)?;
    if !configured {
        return Err(Failure::new("the encoder gave no picture"));
    }
    let hardware = encoder.hardware();
    muxer.end_track(track).map_err(named)?;
    let finished = muxer.finish().map_err(named)?;
    Ok(Written {
        frames: scene.frames(),
        bytes: finished.bytes,
        seconds: began.elapsed().as_secs_f64(),
        hardware,
    })
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
