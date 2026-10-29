//! The pictures of the footage a scene shows, through frx7's public traits (`docs/spec/render-command.md` section 4.8): each
//! source is opened once, its pictures are decoded by the system's own decoder in the process, asked for by their number,
//! brought to red, green and blue by the colour standard the stream names and to the size of the box by frx7's arithmetic.

use std::sync::Arc;

use base::{Color, Error, Picture, PixelFormat};
use codec::{Hardware, Pictures};
use project::{Code, Problem, Report};
use timebase::Time;
use timeline::{Footage, FootageSource, PlacedVideo, Scene};

/// The picture given last: its number, its size and its pixels.
type Given = (u64, (u32, u32), Arc<Vec<u8>>);

/// One source, open: its pictures, what its stream says its colours mean, the presentation times of its pictures in the
/// order they are shown with the ticks to the second (the at-or-before rule picks the picture from these), and the
/// picture given last with its number and size, since a frame of the scene and the next often show the same picture.
struct Open {
    pictures: Pictures,
    color: Option<Color>,
    scale: u32,
    times: Vec<i64>,
    last: Option<Given>,
}

/// The sources of a scene, opened.
pub struct Sources {
    open: Vec<Open>,
}

impl Sources {
    /// Opens every source of the scene and decodes its first picture (`notation.md` N3), in the order of
    /// [`Scene::sources`]. The error names the source as written.
    pub fn open(scene: &Scene) -> Result<Sources, String> {
        let formats = super::film::formats();
        let codecs = super::film::codecs();
        let mut open = Vec::with_capacity(scene.sources().len());
        for source in scene.sources() {
            let opened =
                open_one(source, &formats, &codecs).map_err(|error| format!("`{}` could not be opened: {error}", source.written))?;
            tracing::debug!(source = open.len(), pictures = opened.pictures.count(), "decoder started");
            open.push(opened);
        }
        Ok(Sources { open })
    }
}

fn open_one(source: &FootageSource, formats: &format::Registry, codecs: &codec::Registry) -> Result<Open, Error> {
    let demuxer = formats.open(&source.path)?;
    let tracks = demuxer.tracks();
    let color = tracks.iter().find_map(|track| track.video().map(|video| video.color)).flatten();
    // The same first picture track `Pictures` opens: its timescale and the presentation times of its pictures in the
    // order they are shown.
    let (scale, times) = match tracks.iter().position(|track| track.media.is_video()) {
        Some(track) => (tracks[track].timescale, demuxer.times(track)),
        None => (0, Vec::new()),
    };
    drop(demuxer);
    let pictures = Pictures::open(&source.path, formats, codecs, Hardware::Any)?;
    Ok(Open {
        pictures,
        color,
        scale,
        times,
        last: None,
    })
}

impl Footage for Sources {
    fn count(&mut self, source: usize) -> Result<u64, String> {
        let open = self.open.get(source).ok_or_else(|| format!("there is no source {source}"))?;
        Ok(open.pictures.count())
    }

    fn picture_times(&mut self, source: usize) -> Result<(u32, Vec<i64>), String> {
        let open = self.open.get(source).ok_or_else(|| format!("there is no source {source}"))?;
        Ok((open.scale, open.times.clone()))
    }

    fn picture(&mut self, source: usize, number: u64, size: (u32, u32)) -> Result<Arc<Vec<u8>>, String> {
        let open = self.open.get_mut(source).ok_or_else(|| format!("there is no source {source}"))?;
        if let Some((last, last_size, pixels)) = &open.last {
            if *last == number && *last_size == size {
                return Ok(pixels.clone());
            }
        }
        let named = |error: Error| format!("picture {number}: {error}");
        let numbered = open.pictures.picture(number).map_err(|error| error.to_string())?;
        let planes = numbered.frame.surface.read().map_err(named)?;
        let rgba = pixel::convert(&planes, open.color, PixelFormat::Rgba8, None).map_err(named)?;
        let sized = if (rgba.width, rgba.height) == size {
            rgba
        } else {
            pixel::resize(&rgba, size).map_err(named)?
        };
        let pixels = Arc::new(opaque_rows(&sized));
        open.last = Some((number, size, pixels.clone()));
        Ok(pixels)
    }
}

/// The reports and notes of the footage's starts in their sources (`docs/spec/notation.md` N4): every source is opened
/// through frx7, as `render` opens it, and every placed `<video>` is judged. A `media-start` below 0 is reported with
/// the element's line, the path as written and the source's length (`FS-0089`); one at or past the end of its source
/// the same way (`FS-0090`). A source that ends inside its clip is noted with the element's line, the source's length
/// and the clip's length. Sources that cannot be opened are left out: their reports come with T089. The placed
/// videos, sources and frames a second are what [`timeline::placed_videos`] gives, or a made scene's own.
pub fn footage_reports_and_notes(placed: &[PlacedVideo], sources: &[FootageSource], fps: u32) -> (Vec<Report>, Vec<String>) {
    let formats = super::film::formats();
    let mut lengths = Vec::with_capacity(sources.len());
    for source in sources {
        lengths.push(open_length(&formats, source));
    }
    let mut reports = Vec::new();
    let mut notes = Vec::new();
    for placed in placed {
        let Some(length) = lengths[placed.source] else {
            continue;
        };
        if let Some((start, shown)) = placed.media_start.as_ref() {
            if start.is_negative() {
                let message = format!(
                    "`media-start=\"{}\"` is before the start of `{}`, which is {} s long",
                    shown, placed.written, length
                );
                reports.push(Report::new(
                    placed.file.clone(),
                    Problem::new(Code::VideoMediaStartNegative, placed.line, message),
                ));
                continue;
            }
            if *start >= length {
                let message = format!(
                    "`media-start=\"{}\"` is past the end of `{}`, which is {} s long",
                    shown, placed.written, length
                );
                reports.push(Report::new(
                    placed.file.clone(),
                    Problem::new(Code::VideoMediaStartPastEnd, placed.line, message),
                ));
                continue;
            }
        }
        let clip = Time::new((placed.end - placed.first) as i128, fps as i128);
        if length < clip {
            let note = format!(
                "note: {}, line {}: `{}` is {} s long and its <video> lasts {} s; its last picture holds",
                placed.file, placed.line, placed.written, length, clip
            );
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
    }
    (reports, notes)
}

/// The length of a source in seconds, from the track `Pictures` opens: none when the file cannot be opened or holds
/// no picture track.
fn open_length(formats: &format::Registry, source: &FootageSource) -> Option<Time> {
    let demuxer = formats.open(&source.path).ok()?;
    let tracks = demuxer.tracks();
    let track = tracks.iter().position(|track| track.media.is_video())?;
    let timescale = tracks[track].timescale;
    if timescale == 0 {
        return None;
    }
    Some(Time::new(tracks[track].duration as i128, timescale as i128))
}

/// The pixels of a picture of red, green, blue and alpha as rows with no padding, every alpha 255: footage is opaque, so its
/// values are the same premultiplied.
fn opaque_rows(rgba: &Picture) -> Vec<u8> {
    let plane = &rgba.planes[0];
    let line = rgba.width as usize * 4;
    let mut pixels = Vec::with_capacity(line * rgba.height as usize);
    for row in plane.data.chunks(plane.stride).take(rgba.height as usize) {
        pixels.extend_from_slice(&row[..line]);
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    pixels
}

impl Drop for Sources {
    fn drop(&mut self) {
        for source in 0..self.open.len() {
            tracing::debug!(source, "decoder finished");
        }
    }
}
