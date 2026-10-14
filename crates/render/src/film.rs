//! The film: the frames of a scene, drawn in order, brought to the planes the encoder takes, encoded
//! by the system's H.264 encoder, and written as an MP4 file with the scene's mix as its one sound
//! track, encoded by the system's AAC encoder, all through frx7's public traits
//! (`docs/spec/render-command.md` sections 5 and 5.2).

use std::fmt;
use std::fs;
use std::path::Path;
use std::time::Instant;

use base::{Codec, Color, Error, Frame, Hdr, Packet, Picture, PixelFormat, Rational};
use codec::{AudioEncoder, AudioEncoderSetup, Hardware, RateControl, VideoEncoder, VideoEncoderSetup};
use draw::Renderer;
use format::{Audio, Media, Muxer, TrackSetup, Video};
use mix::Mix;

use timeline::Scene;

/// The quality `render.quality: standard` asks the encoder for, from 0 to 1 (decision D22).
const STANDARD_QUALITY: f32 = 0.65;
/// The most seconds between two key pictures (decision D22).
const KEY_SECONDS: f64 = 2.0;
/// Sample frames of AAC in one packet (ISO/IEC 14496-3).
const AAC_PACKET: u64 = 1024;

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
/// folder it is in when that is missing, with the sound encoded at `audio_bitrate` kbit/s
/// (`render.audio-bitrate`, section 5.2). A film that cannot be finished leaves no file at
/// `output`.
pub fn write_film(scene: &Scene, renderer: &mut dyn Renderer, output: &Path, audio_bitrate: u32) -> Result<Written, Failure> {
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
    let written = make(scene, renderer, output, audio_bitrate, began).inspect_err(|_| {
        // A film that was not finished leaves no file that looks finished.
        let _ = fs::remove_file(output);
    })?;
    tracing::info!(frames = written.frames, bytes = written.bytes, "render finished");
    Ok(written)
}

fn make(scene: &Scene, renderer: &mut dyn Renderer, output: &Path, audio_bitrate: u32, began: Instant) -> Result<Written, Failure> {
    let named = |error: Error| Failure::new(error.to_string());
    let sound_bitrate = audio_bitrate
        .checked_mul(1000)
        .ok_or_else(|| Failure::new(format!("a sound bit rate of {audio_bitrate} kbit/s is too large")))?;
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
    let codecs = codecs();
    let mut encoder = codecs.video_encoder(&setup).map_err(named)?;
    tracing::debug!(hardware = encoder.hardware(), "encoder started");
    let mut mix = Mix::for_picture(scene.frames(), scene.fps());
    let mut sound = codecs
        .audio_encoder(&AudioEncoderSetup {
            codec: Codec::Aac,
            sample_rate: mix::SAMPLE_RATE,
            channels: mix::CHANNELS,
            bitrate: sound_bitrate,
        })
        .map_err(named)?;
    tracing::debug!(sample_frames = mix.length(), bitrate = sound_bitrate, "sound encoder started");
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
    let sound_track = muxer
        .add_track(TrackSetup {
            codec: Codec::Aac,
            config: sound.config(),
            timescale: mix::SAMPLE_RATE,
            media: Media::Audio(Audio {
                sample_rate: mix::SAMPLE_RATE,
                channels: mix::CHANNELS,
                lead_in: sound.lead_in(),
            }),
            // The lead-in and the last, part-filled packet are a few packets more.
            expected_packets: Some(mix.length() / AAC_PACKET + 8),
        })
        .map_err(named)?;
    let mut samples = Vec::new();
    let mut sound_packets = Vec::new();

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
        // The mix up to the end of this frame goes with it, so that picture and sound lie side by
        // side in the file.
        let until = mix::length_for_picture(number + 1, scene.fps());
        let due = until - (mix.length() - mix.left());
        samples.resize(due as usize * usize::from(mix::CHANNELS), 0.0);
        let given = mix.read(&mut samples);
        sound
            .send(&samples[..given * usize::from(mix::CHANNELS)], &mut sound_packets)
            .map_err(named)?;
        write_sound(&mut sound_packets, muxer.as_mut(), sound_track).map_err(named)?;
    }
    encoder.finish().map_err(named)?;
    take_packets(encoder.as_mut(), muxer.as_mut(), track, &mut configured).map_err(named)?;
    finish_sound(sound.as_mut(), muxer.as_mut(), sound_track, &mut sound_packets).map_err(named)?;
    if !configured {
        return Err(Failure::new("the encoder gave no picture"));
    }
    let hardware = encoder.hardware();
    muxer.end_track(track).map_err(named)?;
    muxer.end_track(sound_track).map_err(named)?;
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

/// Moves the sound packets that are ready into the file.
fn write_sound(packets: &mut Vec<Packet>, muxer: &mut dyn Muxer, track: usize) -> base::Result<()> {
    for packet in packets.drain(..) {
        muxer.write(track, packet)?;
    }
    Ok(())
}

/// Ends the sound: the encoder's last packets go into the file.
fn finish_sound(sound: &mut dyn AudioEncoder, muxer: &mut dyn Muxer, track: usize, packets: &mut Vec<Packet>) -> base::Result<()> {
    sound.finish(packets)?;
    tracing::debug!("sound encoder finished");
    write_sound(packets, muxer, track)
}
