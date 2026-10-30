//! The film: the frames of a scene, drawn in order, brought to the planes the encoder takes, encoded
//! by the system's H.264 or HEVC encoder, and written as an MP4 file with the scene's mix as its one sound
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
use project::{Codec as PictureCodec, Encoder, Quality, Settings};

use timeline::Scene;

use crate::footage::Sources;

/// The quality each `render.quality` asks the machine's own encoder for, from 0 to 1 (`docs/spec/render-command.md`
/// section 5.4; `standard` is decision D22's).
const HARDWARE_QUALITY: [(Quality, f32); 3] = [(Quality::Draft, 0.45), (Quality::Standard, 0.65), (Quality::High, 0.85)];
/// The bits for each pixel of each picture that each `render.quality` asks the system's encoder on the processor for, which
/// takes a bit rate and no quality (section 5.4).
const SOFTWARE_BITS_PER_PIXEL: [(Quality, f64); 3] = [(Quality::Draft, 0.025), (Quality::Standard, 0.05), (Quality::High, 0.1)];
/// The fewest bits a second each `render.quality` asks the system's encoder on the processor for, so that a small film
/// keeps its pictures and the three qualities stay in order (section 5.4).
const SOFTWARE_LEAST_BITRATE: [(Quality, f64); 3] = [
    (Quality::Draft, 250_000.0),
    (Quality::Standard, 500_000.0),
    (Quality::High, 1_000_000.0),
];
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
    /// By how many decibels the mix was lowered as a whole, or none when it needed none
    /// (`docs/spec/sound.md`, section 4).
    pub lowered_db: Option<f64>,
}

/// How the picture is encoded: `render.codec`, `render.encoder` and `render.quality` (`docs/spec/render-command.md`
/// sections 5.3 and 5.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Encoding {
    pub codec: PictureCodec,
    pub encoder: Encoder,
    pub quality: Quality,
}

impl Encoding {
    /// The encoding the settings in use ask for.
    pub fn of(settings: &Settings) -> Encoding {
        Encoding {
            codec: settings.codec(),
            encoder: settings.encoder(),
            quality: settings.quality(),
        }
    }

    /// The setup of the picture encoder for a film of `width` by `height` at `fps`: the machine's own encoder only, or
    /// the system's encoder on the processor only, never left to the system (section 5.4).
    fn setup(&self, width: u32, height: u32, fps: u32) -> VideoEncoderSetup {
        let rate = Rational::new(fps, 1);
        let (hardware, rate_control) = match self.encoder {
            Encoder::Hardware => (Hardware::Required, RateControl::Quality(pick(&HARDWARE_QUALITY, self.quality))),
            Encoder::Software => {
                let bits = pick(&SOFTWARE_BITS_PER_PIXEL, self.quality) * f64::from(width) * f64::from(height) * f64::from(fps);
                let bits = bits.round().clamp(pick(&SOFTWARE_LEAST_BITRATE, self.quality), f64::from(u32::MAX)) as u32;
                (Hardware::Refused, RateControl::Bitrate(bits))
            }
        };
        VideoEncoderSetup {
            codec: picture_codec(self.codec),
            width,
            height,
            rate,
            bits: 8,
            rate_control,
            key_distance: VideoEncoderSetup::key_distance_within(KEY_SECONDS, rate),
            reorder: true,
            color: Color::BT709,
            hdr: Hdr::default(),
            hardware,
        }
    }
}

fn pick<T: Copy>(table: &[(Quality, T); 3], quality: Quality) -> T {
    table.iter().find(|(each, _)| *each == quality).map(|(_, value)| *value).unwrap()
}

/// The encoder's refusal of a film's size, before the first frame (`docs/spec/render-command.md` section 5.4): which
/// encoder, the size, the settings in effect, and the setting that makes the film render, as one sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SizeRefused(pub String);

/// Makes the encoder `encoding` names for a film of `width` by `height` at `fps`, and gives its refusal when it does not
/// take that size. Nothing is drawn or written. An encoder that cannot be made for another reason is not a refusal of the
/// size: the render reports it when it makes the encoder.
pub fn size_refused(encoding: Encoding, width: u32, height: u32, fps: u32) -> Option<SizeRefused> {
    let takes = |encoding: Encoding| {
        !matches!(
            codecs().video_encoder(&encoding.setup(width, height, fps)),
            Err(Error::Unsupported(_))
        )
    };
    if takes(encoding) {
        return None;
    }
    let hevc_taken = match (encoding.encoder, encoding.codec) {
        (Encoder::Hardware, PictureCodec::H264) => takes(Encoding {
            codec: PictureCodec::Hevc,
            ..encoding
        }),
        _ => false,
    };
    Some(SizeRefused(refused_sentence(encoding, width, height, hevc_taken)))
}

/// The sentence a refusal of `encoding` at `width` by `height` reports: the encoder, the size, the settings in effect,
/// and the setting that makes the film render. `hevc_taken`: whether the machine's own HEVC encoder takes the size;
/// it decides the remedy named when the machine's own H.264 encoder does not. Pure: no encoder is made.
pub fn refused_sentence(encoding: Encoding, width: u32, height: u32, hevc_taken: bool) -> String {
    let (who, codec) = match (encoding.encoder, encoding.codec) {
        (Encoder::Hardware, PictureCodec::H264) => ("the machine's own H.264 encoder", "h264"),
        (Encoder::Hardware, PictureCodec::Hevc) => ("the machine's own HEVC encoder", "hevc"),
        (Encoder::Software, PictureCodec::H264) => ("the system's H.264 encoder on the processor", "h264"),
        (Encoder::Software, PictureCodec::Hevc) => ("the system's HEVC encoder on the processor", "hevc"),
    };
    let remedy = match encoding.encoder {
        Encoder::Hardware if encoding.codec == PictureCodec::H264 && hevc_taken => {
            "set codec: hevc, which the machine's own encoder takes at this size, or encoder: software"
        }
        Encoder::Hardware => "set encoder: software",
        Encoder::Software => "make the film smaller",
    };
    format!(
        "{who} does not take a film of {width}x{height} (render.encoder: {}, render.codec: {codec}); {remedy}",
        encoding.encoder
    )
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

/// The hidden folder of a run: beside the output path and named with the process id of the run, so that a person can
/// tell whose it is. The sweep decides by the lock file of the run and never by this name (`docs/spec/render-command.md`,
/// section 6).
pub fn staging_folder_of(output: &Path) -> PathBuf {
    output.with_file_name(format!(".framesmith-{}", std::process::id()))
}

/// The lock file of a run: `.framesmith-<pid>.lock` beside the output path, named with the process id of the run. The
/// sweep reads its name and decides by its lock (`docs/spec/render-command.md`, section 6).
fn lock_of(output: &Path) -> PathBuf {
    output.with_file_name(format!(".framesmith-{}.lock", std::process::id()))
}

/// Creates the lock file of a run already held, in one step: on macOS, `O_EXLOCK` of open(2) takes the lock of
/// `flock(2)` as the file is opened, so no other render can see the file unlocked.
#[cfg(target_os = "macos")]
fn create_locked(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    /// `O_EXLOCK` from the system's `fcntl.h`.
    const O_EXLOCK: i32 = 0x0000_0020;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(O_EXLOCK)
        .open(path)
}

/// Creates the lock file of a run and then takes its lock. Only macOS is offered for now, so this is the form for the
/// systems that follow it, where the file is unlocked for an instant after it is made.
#[cfg(not(target_os = "macos"))]
fn create_locked(path: &Path) -> std::io::Result<fs::File> {
    let file = fs::OpenOptions::new().write(true).create(true).truncate(false).open(path)?;
    file.try_lock().map_err(std::io::Error::other)?;
    Ok(file)
}

/// A run's hold on its lock. When it ends, whether it completes or fails, its hidden folder and its lock file are removed
/// while the lock is still held, and the lock is released after them.
struct Run {
    folder: PathBuf,
    lock_path: PathBuf,
    _lock: fs::File,
}

impl Run {
    /// Takes the lock of a run whose output is `output`. The folder of an earlier run with the same process id, which only
    /// a killed run leaves, is removed: the lock is the run's own, so the folder is none of another run's.
    fn lock(output: &Path) -> std::io::Result<Run> {
        let lock_path = lock_of(output);
        let lock = create_locked(&lock_path)?;
        let run = Run {
            folder: staging_folder_of(output),
            lock_path,
            _lock: lock,
        };
        let _ = fs::remove_dir_all(&run.folder);
        Ok(run)
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if remove_folder(&self.folder) {
            let _ = fs::remove_file(&self.lock_path);
        }
    }
}

/// Removes a hidden folder of a run, with what it holds. A folder that is already gone counts as removed. A folder that
/// cannot be removed is reported and stays, and its lock file stays with it, so that the sweep takes both later.
fn remove_folder(folder: &Path) -> bool {
    match fs::remove_dir_all(folder) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => {
            tracing::warn!(error = %error, "a staging folder could not be removed; its lock file stays so that the next render sweeps both");
            false
        }
    }
}

/// The staging file of a film: the output's name with `.part` added before its extension, in the hidden folder of the
/// run, so that the move to the output path is one rename and the file keeps the extension that names its container
/// (`film.mp4` gives `.framesmith-<pid>/film.part.mp4`; `docs/spec/render-command.md`, section 6).
pub fn staging_of(output: &Path) -> PathBuf {
    staging_folder_of(output).join(staging_name(output))
}

/// The name of the staging file of `output`: see `staging_of`.
fn staging_name(output: &Path) -> std::ffi::OsString {
    let mut name = output.file_stem().unwrap_or_default().to_os_string();
    name.push(".part");
    if let Some(extension) = output.extension() {
        name.push(".");
        name.push(extension);
    }
    name
}

/// Removes the hidden folders of renders into `folder` whose lock no render holds, and logs each one: what a render
/// killed outright left behind. The lock file of a run is read, and its lock alone decides. A lock that is held is left as
/// it is, the caller's own included, and the process id in a name is never asked about.
pub fn remove_stale_staging(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name
            .to_str()
            .and_then(|name| name.strip_prefix(".framesmith-"))
            .and_then(|name| name.strip_suffix(".lock"))
            .and_then(|pid| pid.parse::<u32>().ok());
        let Some(pid) = pid else {
            continue;
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(lock) = fs::File::open(entry.path()) else {
            continue;
        };
        if lock.try_lock().is_err() {
            continue;
        }
        if !remove_folder(&folder.join(format!(".framesmith-{pid}"))) {
            continue;
        }
        if fs::remove_file(entry.path()).is_ok() {
            tracing::info!(
                pid,
                "removed a staging folder that no render holds: its render was killed or has ended"
            );
        }
    }
}

/// Checks, before the first frame, that the output path can be made and written: its folder is made when it is missing,
/// and its staging file can be opened for writing in the hidden folder of a run, under the run's lock; the probe and the
/// lock are removed again. A place that cannot be written gives a [`Refusal`] with the system's reason, and nothing is drawn.
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
    let run = Run::lock(output).map_err(refuse)?;
    let probed = fs::create_dir(&run.folder)
        .and_then(|()| {
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(staging_of(output))
        })
        .map(drop);
    drop(run);
    probed.map_err(refuse)
}

/// The container formats a film is written in: the formats `render` was built with, so that a test
/// can read what the registry holds (`docs/spec/render-command.md`, section 6).
pub fn formats() -> format::Registry {
    let mut formats = format::Registry::new();
    formats.register(mp4::FORMAT);
    formats
}

pub(crate) fn codecs() -> codec::Registry {
    let mut codecs = codec::Registry::new();
    codecs.register(backend_os::BACKEND);
    codecs
}

/// Draws every frame of `scene` with `renderer` and writes the film to `output`, making the
/// folder it is in when that is missing, with the sound encoded at `audio_bitrate` kbit/s
/// (`render.audio-bitrate`, section 5.2). A film that cannot be finished leaves no file at
/// `output`. The scene's sounds are opened first (`docs/spec/sound.md`); one that cannot be is the failure.
/// The picture is encoded as the defaults of the settings ask: H.264 by the machine's own encoder at `standard`.
pub fn write_film(scene: &Scene, renderer: &mut dyn Renderer, output: &Path, audio_bitrate: u32) -> Result<Written, Failure> {
    let sounds = crate::open_sounds("", scene.sounds()).map_err(|reports| {
        let messages: Vec<String> = reports.iter().map(|report| report.problem().message().to_string()).collect();
        Failure::new(messages.join("; "))
    })?;
    write_film_marked(scene, sounds, renderer, output, audio_bitrate, Encoding::default(), None)
}

/// Writes the film as `write_film` does, with `sounds` (the scene's, as [`crate::open_sounds`] opens them) placed in its
/// mix, with its picture encoded as `encoding` asks (sections 5.3 and 5.4) and, when `marks` is given, puts in it the mark
/// of each frame drawn, in the order of the frames (`docs/spec/render-command.md`, section 7.1). A film that is not finished
/// leaves the marks it gave so far.
pub fn write_film_marked(
    scene: &Scene,
    sounds: Vec<mix::Sound>,
    renderer: &mut dyn Renderer,
    output: &Path,
    audio_bitrate: u32,
    encoding: Encoding,
    marks: Option<&mut Vec<u64>>,
) -> Result<Written, Failure> {
    write_film_stepped(scene, sounds, renderer, output, audio_bitrate, encoding, marks, &mut |_| {})
}

/// The points of a render's start at which `write_film_stepped` stops to let its caller look at the folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The run holds its lock, and its hidden folder is not made yet.
    Locked,
    /// The hidden folder is made, and the run still holds its lock. No frame is drawn yet.
    Made,
}

/// Writes the film as `write_film_marked` does and calls `on_step` at each `Step` of the start, in order. A test uses the
/// steps to run the sweep of another render at each point of a start (`docs/spec/render-command.md`, section 6).
#[allow(clippy::too_many_arguments)]
pub fn write_film_stepped(
    scene: &Scene,
    sounds: Vec<mix::Sound>,
    renderer: &mut dyn Renderer,
    output: &Path,
    audio_bitrate: u32,
    encoding: Encoding,
    marks: Option<&mut Vec<u64>>,
    on_step: &mut dyn FnMut(Step),
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
        codec = %encoding.codec,
        encoder = %encoding.encoder,
        quality = %encoding.quality,
        "render started"
    );
    let staging = staging_of(output);
    // The lock file is created held before any folder is made, so a render killed at any point leaves a lock file that the
    // sweep can take. The run's guard removes its folder and its lock file however the render ends.
    let run = Run::lock(output).map_err(|error| Failure::new(format!("the staging folder could not be locked: {error}")))?;
    on_step(Step::Locked);
    fs::create_dir(&run.folder).map_err(|error| Failure::new(format!("the staging folder could not be made: {error}")))?;
    on_step(Step::Made);
    let written = make(scene, sounds, renderer, &staging, audio_bitrate, encoding, began, marks).and_then(|written| {
        fs::rename(&staging, output)
            .map_err(|error| Failure::new(format!("the finished video could not be moved to its place: {error}")))?;
        Ok(written)
    })?;
    drop(run);
    tracing::info!(frames = written.frames, bytes = written.bytes, "render finished");
    Ok(written)
}

#[allow(clippy::too_many_arguments)]
fn make(
    scene: &Scene,
    sounds: Vec<mix::Sound>,
    renderer: &mut dyn Renderer,
    output: &Path,
    audio_bitrate: u32,
    encoding: Encoding,
    began: Instant,
    mut marks: Option<&mut Vec<u64>>,
) -> Result<Written, Failure> {
    let named = |error: Error| Failure::new(error.to_string());
    let sound_bitrate = audio_bitrate
        .checked_mul(1000)
        .ok_or_else(|| Failure::new(format!("a sound bit rate of {audio_bitrate} kbit/s is too large")))?;
    // The footage is opened, and the first picture of each source decoded, before the encoder starts (section 4.8, rule 5).
    let mut footage = Sources::open(scene).map_err(Failure::new)?;
    let rate = Rational::new(scene.fps(), 1);
    let setup = encoding.setup(scene.width(), scene.height(), scene.fps());
    let picture = setup.codec;
    let codecs = codecs();
    let mut encoder = codecs.video_encoder(&setup).map_err(named)?;
    tracing::debug!(hardware = encoder.hardware(), "encoder started");
    // The film is encoded by the encoder the author chose, or not at all (section 5.4).
    if encoder.hardware() != (encoding.encoder == Encoder::Hardware) {
        return Err(Failure::new(format!(
            "the system gave another encoder than the {} one that render.encoder names",
            encoding.encoder
        )));
    }
    let mut mix = Mix::for_picture(scene.frames(), scene.fps());
    for sound in sounds {
        mix.place(sound).map_err(named)?;
    }
    // The true peak of the finished mix is found before anything is encoded, and a mix over the
    // limit is lowered as a whole (`docs/spec/sound.md`, section 4): the mix is read once to find
    // its peak and rewound to write it, so the sound the film holds is the lowered mix throughout.
    let peak = mix.true_peak().map_err(named)?;
    let lowered_db = mix::peak::gain_for(peak).map(|gain| {
        mix.set_gain(gain as f32);
        mix::peak::lowered_db(gain)
    });
    mix.rewind().map_err(named)?;
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
            codec: picture,
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
            display: None,
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
            display: None,
            // The lead-in and the last, part-filled packet are a few packets more.
            expected_packets: Some(mix.length() / AAC_PACKET + 8),
        })
        .map_err(named)?;
    let mut samples = Vec::new();
    let mut sound_packets = Vec::new();

    let mut configured = false;
    for number in 0..scene.frames() {
        let drawn = scene
            .picture_with(number, renderer, &mut footage)
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
        lowered_db,
    })
}

/// frx7's codec for the picture of `render.codec`: HEVC is written in 8 bits, as H.264 is (section 5.3).
fn picture_codec(codec: project::Codec) -> Codec {
    match codec {
        project::Codec::H264 => Codec::H264,
        project::Codec::Hevc => Codec::Hevc,
    }
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
