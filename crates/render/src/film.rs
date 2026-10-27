//! The film: the frames of a scene, drawn in order, brought to the planes the encoder takes, encoded
//! by the system's H.264 encoder, and written as an MP4 file with the scene's mix as its one sound
//! track, encoded by the system's AAC encoder, all through frx7's public traits
//! (`docs/spec/render-command.md` sections 5 and 5.2).

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
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

/// An output path that `render` cannot make or write: the path and the system's reason, as one sentence without a
/// full stop (`docs/spec/render-command.md`, section 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    path: PathBuf,
    reason: String,
}

impl Refusal {
    /// The output path as the command was given it, resolved.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The system's reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// The hidden folder of a run: beside the output path and named with the process id of the run, so that the staging
/// file of a render killed outright is found and removed by the next render (`docs/spec/render-command.md`, section 6).
pub fn staging_folder_of(output: &Path) -> PathBuf {
    output.with_file_name(format!(".framesmith-{}", std::process::id()))
}

/// The staging file of a film: the output's name with `.part` added before its extension, in the hidden folder of the
/// run, so that the move to the output path is one rename and the file keeps the extension that names its container
/// (`film.mp4` gives `.framesmith-<pid>/film.part.mp4`; `docs/spec/render-command.md`, section 6).
pub fn staging_of(output: &Path) -> PathBuf {
    let stem = output.file_stem().unwrap_or_default().to_os_string();
    let mut name = stem;
    name.push(".part");
    if let Some(extension) = output.extension() {
        name.push(".");
        name.push(extension);
    }
    staging_folder_of(output).join(name)
}

/// Removes the hidden folders of renders into `folder` whose process no longer runs, and logs each one: what a render
/// killed outright left behind. A hidden folder whose process still runs is left as it is.
pub fn remove_stale_staging(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name
            .to_str()
            .and_then(|name| name.strip_prefix(".framesmith-"))
            .and_then(|pid| pid.parse::<u32>().ok());
        let Some(pid) = pid else {
            continue;
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) || pid == std::process::id() || process_runs(pid) {
            continue;
        }
        if fs::remove_dir_all(entry.path()).is_ok() {
            tracing::info!(pid, "removed the staging folder of a render that is no longer running");
        }
    }
}

/// Whether a process with this id runs. When the process table cannot be asked, the process is taken to run, so that
/// its folder is then left alone.
pub fn process_runs(pid: u32) -> bool {
    std::process::Command::new("/bin/ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(true)
}

/// Checks, before the first frame, that the output path can be made and written: its folder is made when it is missing,
/// and its staging file can be opened for writing; that staging file is removed again. A place that cannot be written gives
/// a [`Refusal`] with the system's reason, and nothing is drawn.
pub fn check_place(output: &Path) -> Result<(), Refusal> {
    let refuse = |error: std::io::Error| Refusal {
        path: output.to_path_buf(),
        reason: error.to_string(),
    };
    if output.is_dir() {
        return Err(refuse(std::io::ErrorKind::IsADirectory.into()));
    }
    if let Some(folder) = output.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        fs::create_dir_all(folder).map_err(refuse)?;
    }
    let staging = staging_of(output);
    fs::create_dir_all(staging.parent().unwrap_or(Path::new("."))).map_err(refuse)?;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&staging)
        .map_err(refuse)?;
    let _ = fs::remove_file(&staging);
    let _ = fs::remove_dir(staging_folder_of(output));
    Ok(())
}

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
/// `output`. The scene's sounds are opened first (`docs/spec/sound.md`); one that cannot be is the failure.
pub fn write_film(scene: &Scene, renderer: &mut dyn Renderer, output: &Path, audio_bitrate: u32) -> Result<Written, Failure> {
    let sounds = crate::open_sounds("", scene.sounds()).map_err(|reports| {
        let messages: Vec<String> = reports.iter().map(|report| report.problem().message().to_string()).collect();
        Failure::new(messages.join("; "))
    })?;
    write_film_marked(scene, sounds, renderer, output, audio_bitrate, None)
}

/// Writes the film as `write_film` does, with `sounds` (the scene's, as [`crate::open_sounds`] opens them) placed in its
/// mix and, when `marks` is given, puts in it the mark of each frame drawn, in the order of the frames
/// (`docs/spec/render-command.md`, section 7.1). A film that is not finished leaves the marks it gave so far.
pub fn write_film_marked(
    scene: &Scene,
    sounds: Vec<mix::Sound>,
    renderer: &mut dyn Renderer,
    output: &Path,
    audio_bitrate: u32,
    marks: Option<&mut Vec<u64>>,
) -> Result<Written, Failure> {
    let folder = output
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(folder).map_err(|error| Failure::new(format!("the folder for the file could not be made: {error}")))?;
    remove_stale_staging(folder);
    let began = Instant::now();
    tracing::info!(
        frames = scene.frames(),
        width = scene.width(),
        height = scene.height(),
        fps = scene.fps(),
        renderer = renderer.name(),
        "render started"
    );
    let staging = staging_of(output);
    let run_folder = staging_folder_of(output);
    fs::create_dir_all(&run_folder).map_err(|error| Failure::new(format!("the staging folder could not be made: {error}")))?;
    let written = make(scene, sounds, renderer, &staging, audio_bitrate, began, marks)
        .and_then(|written| {
            fs::rename(&staging, output)
                .map_err(|error| Failure::new(format!("the finished video could not be moved to its place: {error}")))?;
            Ok(written)
        })
        .inspect_err(|_| {
            let _ = fs::remove_dir_all(&run_folder);
        })?;
    let _ = fs::remove_dir(&run_folder);
    tracing::info!(frames = written.frames, bytes = written.bytes, "render finished");
    Ok(written)
}

fn make(
    scene: &Scene,
    sounds: Vec<mix::Sound>,
    renderer: &mut dyn Renderer,
    output: &Path,
    audio_bitrate: u32,
    began: Instant,
    mut marks: Option<&mut Vec<u64>>,
) -> Result<Written, Failure> {
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
    for sound in sounds {
        mix.place(sound).map_err(named)?;
    }
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
        if let Some(marks) = marks.as_deref_mut() {
            marks.push(drawn.mark());
        }
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
        let given = mix.read(&mut samples).map_err(named)?;
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
