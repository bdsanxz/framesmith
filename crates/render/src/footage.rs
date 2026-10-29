//! The pictures of the footage a scene shows, through frx7's public traits (`docs/spec/render-command.md` section 4.8): each
//! source is opened once, its pictures are decoded by the system's own decoder in the process, asked for by their number,
//! brought to red, green and blue by the colour standard the stream names and to the size of the box by frx7's arithmetic.

use std::collections::VecDeque;
use std::sync::Arc;

use base::{Color, Error, Picture, PixelFormat};
use codec::{Hardware, Pictures};
use timeline::{Footage, FootageSource, Scene};

/// The picture given last: its number, its size and its pixels.
type Given = (u64, (u32, u32), Arc<Vec<u8>>);

/// One source, open: its pictures, what its stream says its colours mean, the presentation times of its pictures in the
/// order they are shown with the ticks to the second (the at-or-before rule picks the picture from these), and the
/// pictures given lately with their numbers and sizes, newest last: the drawing stage and the footage stage ahead of
/// it ask for the same pictures, and a frame of the scene and the next often show the same picture.
struct Open {
    pictures: Pictures,
    color: Option<Color>,
    scale: u32,
    times: Vec<i64>,
    recent: VecDeque<Given>,
}

/// How many decoded pictures a source keeps: enough for the frames the stages hold ahead and the pictures one frame
/// shows, few enough that memory does not grow with the film.
const REMEMBERED: usize = 8;

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
        recent: VecDeque::new(),
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
        if let Some((_, _, pixels)) = open
            .recent
            .iter()
            .find(|(held, held_size, _)| *held == number && *held_size == size)
        {
            return Ok(pixels.clone());
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
        open.recent.push_back((number, size, pixels.clone()));
        while open.recent.len() > REMEMBERED {
            open.recent.pop_front();
        }
        Ok(pixels)
    }
}

/// The pictures one frame shows: asked through the scene's walk of the frame, exactly as drawing asks for them, but
/// without decoding any (`docs/spec/render-command.md`, section 5.5). The footage stage decodes these ahead of the
/// drawing stage; the drawing stage takes them from the shared sources, decoding itself whatever the footage stage
/// has not reached, so a picture is never missed, only ever decoded twice across a lost race.
pub struct Needs<'a> {
    sources: &'a mut Sources,
    needs: Vec<(usize, u64, (u32, u32))>,
}

impl<'a> Needs<'a> {
    pub fn of(sources: &'a mut Sources) -> Needs<'a> {
        Needs {
            sources,
            needs: Vec::new(),
        }
    }

    pub fn take(self) -> Vec<(usize, u64, (u32, u32))> {
        self.needs
    }
}

impl Footage for Needs<'_> {
    fn count(&mut self, source: usize) -> Result<u64, String> {
        self.sources.count(source)
    }

    fn picture_times(&mut self, source: usize) -> Result<(u32, Vec<i64>), String> {
        self.sources.picture_times(source)
    }

    fn picture(&mut self, source: usize, number: u64, size: (u32, u32)) -> Result<Arc<Vec<u8>>, String> {
        self.needs.push((source, number, size));
        Ok(Arc::new(Vec::new()))
    }
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
