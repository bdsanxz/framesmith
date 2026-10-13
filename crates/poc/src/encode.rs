//! The encoder: frx7's film writer, given frames by a thread of its own. No other program is
//! started. See `docs/spec/native-render-pipeline.md`.

use crate::model::Composition;
use crate::project::{self, Coder, Quality};
use crate::yuv;
use frx7_base::{Codec, Color, PixelFormat, Rational, Rows};
use frx7_codec::{Hardware, RateControl, VideoEncoderSetup};
use frx7_export::{Film, FilmSetup};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How many frames may wait for the encoder's thread.
const WAITING: usize = 1;
/// The frame buffers that go round between the frame loop and the encoder's thread: one being
/// filled, the ones waiting, one being written.
const BUFFERS: usize = WAITING + 2;
/// The longest stretch from one key picture to the next, in seconds of film.
const KEY_SECONDS: u32 = 2;

/// The fewest bits a second the software encoder is given. A very small film needs more bits
/// for each pixel than a large one; given fewer, the system's encoder leaves pictures out.
const LEAST_BITS: u32 = 200_000;

/// What the encoder's thread spent.
#[derive(Clone, Copy, Default)]
pub struct Fed {
    pub converting: Duration,
    pub writing: Duration,
}

pub struct Encoder {
    frames: Option<SyncSender<Vec<u8>>>,
    /// Buffers the thread is done with.
    spare: Receiver<Vec<u8>>,
    made: usize,
    thread: Option<JoinHandle<std::io::Result<Fed>>>,
}

/// The container formats and codecs the proof of concept is built with.
pub fn registries() -> (frx7_format::Registry, frx7_codec::Registry) {
    let mut formats = frx7_format::Registry::new();
    formats.register(frx7_mp4::FORMAT);
    let mut codecs = frx7_codec::Registry::new();
    codecs.register(frx7_backend_os::BACKEND);
    (formats, codecs)
}

fn failed(error: frx7_base::Error) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

impl Encoder {
    /// Starts a film with the encoding the product uses (H.264 High or HEVC Main, 8-bit 4:2:0,
    /// BT.709, tables in front), and the thread that feeds it. `quality` sets how much of the
    /// picture is kept, `coder` where the encoder runs, `frames` how many frames will come.
    pub fn start(
        composition: &Composition,
        output: &Path,
        quality: Quality,
        coder: Coder,
        codec: project::Codec,
        frames: u64,
    ) -> std::io::Result<Encoder> {
        let (width, height) = (usize::from(composition.width), usize::from(composition.height));
        if !yuv::fits(width, height) {
            return Err(std::io::Error::other(format!(
                "a film of {width} by {height} pixels cannot be encoded: both sides must be even"
            )));
        }
        let setup = FilmSetup {
            video: VideoEncoderSetup {
                codec: match codec {
                    project::Codec::H264 => Codec::H264,
                    project::Codec::Hevc => Codec::Hevc,
                },
                width: width as u32,
                height: height as u32,
                rate: Rational::new(composition.fps, 1),
                bits: 8,
                rate_control: match coder {
                    Coder::Hardware => RateControl::Quality(f32::from(quality.hardware()) / 100.0),
                    // The system's software encoder takes no quality setting, only a bit rate.
                    Coder::Software => RateControl::Bitrate(
                        ((quality.bits_a_pixel() * (width * height) as f64 * f64::from(composition.fps)) as u32).max(LEAST_BITS),
                    ),
                },
                key_distance: KEY_SECONDS * composition.fps,
                reorder: true,
                color: Color::BT709,
                hdr: frx7_base::Hdr::default(),
                hardware: match coder {
                    Coder::Hardware => Hardware::Required,
                    Coder::Software => Hardware::Refused,
                },
            },
            audio: None,
            frames: Some(frames),
        };
        let (formats, codecs) = registries();
        let film = Film::create(output, &setup, &formats, &codecs).map_err(|error| match (coder, codec) {
            (Coder::Hardware, project::Codec::H264) => std::io::Error::other(format!(
                "this machine's hardware does not encode H.264 at {width} by {height}; render with `codec: hevc`, which hardware encodes at larger sizes, or with `encoder: software` ({error})"
            )),
            (Coder::Hardware, project::Codec::Hevc) => std::io::Error::other(format!(
                "this machine's hardware does not encode HEVC at {width} by {height}; render with `encoder: software` ({error})"
            )),
            (Coder::Software, _) => failed(error),
        })?;
        let (frames, waiting) = std::sync::mpsc::sync_channel::<Vec<u8>>(WAITING);
        let (done, spare) = std::sync::mpsc::channel::<Vec<u8>>();
        let thread = std::thread::Builder::new()
            .name("encoder".to_string())
            .spawn(move || feed(film, &waiting, &done, (width, height)))?;
        Ok(Encoder {
            frames: Some(frames),
            spare,
            made: 0,
            thread: Some(thread),
        })
    }

    /// Why the thread stopped, when it stopped before it was told to.
    fn failure(&mut self) -> std::io::Error {
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(Err(error))) => error,
            Some(Err(_)) => std::io::Error::other("the encoder's thread stopped"),
            _ => std::io::Error::other("the encoder takes no more frames"),
        }
    }

    /// Whether the encoder is given planes, so that a renderer may hand its frames over so.
    pub fn takes_planes(&self) -> bool {
        true
    }

    /// Takes a copy of a finished frame and hands it to the thread: RGBA, or the planes of
    /// `yuv` when the frame comes so. Waits while the thread has as many frames as it may hold.
    pub fn frame(&mut self, pixels: &[u8]) -> std::io::Result<()> {
        let mut buffer = match self.spare.try_recv() {
            Ok(buffer) => buffer,
            Err(_) if self.made < BUFFERS => {
                self.made += 1;
                Vec::with_capacity(pixels.len())
            }
            Err(_) => match self.spare.recv() {
                Ok(buffer) => buffer,
                Err(_) => return Err(self.failure()),
            },
        };
        buffer.clear();
        buffer.extend_from_slice(pixels);
        let sent = match self.frames.as_ref() {
            Some(frames) => frames.send(buffer).is_ok(),
            None => false,
        };
        if sent {
            Ok(())
        } else {
            Err(self.failure())
        }
    }

    /// Lets the thread encode what it still holds and waits for the file to be finished.
    pub fn finish(mut self) -> std::io::Result<Fed> {
        drop(self.frames.take());
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(fed)) => fed,
            _ => Err(std::io::Error::other("the encoder's thread stopped")),
        }
    }
}

/// The encoder's thread: every frame that arrives, in order, converted to planes when it comes
/// as RGBA, and copied into the encoder's own picture. The film is finished when no more
/// frames come; a film whose frames stopped coming because of a failure is left unfinished,
/// and so leaves no file.
fn feed(mut film: Film, frames: &Receiver<Vec<u8>>, done: &Sender<Vec<u8>>, (width, height): (usize, usize)) -> std::io::Result<Fed> {
    let mut fed = Fed::default();
    let mut converted = vec![0; yuv::size(width, height)];
    let (luma, across) = (width * height, width / 2);
    while let Ok(frame) = frames.recv() {
        let began = Instant::now();
        // A frame that is not as long as its RGBA came as planes already.
        let planes = if frame.len() == width * height * 4 {
            yuv::from_rgba(&frame, width, height, &mut converted);
            &converted
        } else if frame.len() == converted.len() {
            &frame
        } else {
            return Err(std::io::Error::other(format!(
                "a frame of {} bytes is neither the RGBA nor the planes of {width} by {height} pixels",
                frame.len()
            )));
        };
        let ready = Instant::now();
        let chroma = across * (height / 2);
        let rows = [
            Rows {
                data: &planes[..luma],
                stride: width,
            },
            Rows {
                data: &planes[luma..luma + chroma],
                stride: across,
            },
            Rows {
                data: &planes[luma + chroma..],
                stride: across,
            },
        ];
        film.rows(PixelFormat::I420, &rows).map_err(failed)?;
        fed.converting += ready - began;
        fed.writing += ready.elapsed();
        // The frame loop may have gone; then nobody wants the buffer back.
        let _ = done.send(frame);
    }
    let began = Instant::now();
    film.finish().map_err(failed)?;
    fed.writing += began.elapsed();
    Ok(fed)
}

/// Puts the parts of a picture, each a file of its own, one after the other into one file,
/// without encoding again. A file of sound, when one is given, becomes its sound track.
pub fn join(parts: &[std::path::PathBuf], sound: Option<&Path>, output: &Path) -> std::io::Result<()> {
    frx7_mp4::join(parts, sound, output).map(|_| ()).map_err(failed)
}
