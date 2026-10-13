//! Source pictures: frx7 reads and decodes them in this program, and hands them over at the
//! size and pace the composition draws them at. See `docs/spec/native-render-pipeline.md`.

use crate::encode::registries;
use crate::model::Still;
use crate::style::{self, apply_color_ops, ColorOp, Grade, Paint};
use crate::yuv;
use frx7_base::{Rational, Rows, Transfer};
use frx7_export::{Footage, Form, Pace};
use frx7_pixel::{Scaler, Siting};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// What the decoders spent, in nanoseconds, for the report.
#[derive(Default)]
pub struct Spent {
    /// Waiting for the decoder's thread and taking its pictures.
    pub reading: AtomicU64,
    /// Turning a picture into RGBA and applying its colour filter.
    pub colouring: AtomicU64,
}

/// What is known of a video source before it is decoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    /// Whether its frames hold two fields.
    pub interlaced: bool,
    /// Its pictures a second, on average.
    pub rate: f64,
    /// Whether its pictures come at one steady rate.
    pub steady: bool,
}

/// The longest side, in pixels, of the picture decoded to see that a source can be decoded.
const TRIAL: u32 = 64;

/// Looks at a video source and decodes its first picture. A source of a kind that would be
/// shown wrongly, or that this build does not read or the system does not decode, is an error
/// that says why.
pub fn look(source: &Path) -> Result<Source, String> {
    let (formats, _) = registries();
    let found = frx7_export::source(source, &formats).map_err(|error| format!("`{}` is not read: {error}", source.display()))?;
    let transfer = match found.color.map(|color| color.transfer) {
        Some(Transfer::Pq) => Some("PQ"),
        Some(Transfer::Hlg) => Some("HLG"),
        _ => None,
    };
    if let Some(transfer) = transfer {
        return Err(format!(
            "`{}` is HDR video (its transfer is {transfer}); HDR video is not supported yet",
            source.display()
        ));
    }
    if found.layers > 1 {
        return Err(format!(
            "`{}` is video of {} layers, as video with transparency is; that is not supported yet",
            source.display(),
            found.layers
        ));
    }
    // The system says for certain that it decodes a stream only when it is given a picture, so
    // the first one is asked for here, small, and not at the first frame of a render.
    let (shown_width, shown_height) = found.shown();
    let pace = Pace {
        size: (shown_width.min(TRIAL), shown_height.min(TRIAL)),
        rate: found.rate,
        first: 0,
        form: Form::Rgba,
    };
    let (formats, codecs) = registries();
    let mut picture = vec![0; pace.form.bytes(pace.size)];
    Footage::start(source, &pace, &formats, &codecs)
        .and_then(|mut footage| footage.read(0, &mut picture))
        .map_err(|error| format!("`{}` could not be decoded: {error}", source.display()))?;
    Ok(Source {
        interlaced: found.interlaced,
        rate: found.rate.value(),
        steady: found.steady,
    })
}

/// The pictures of one video, already at the size and rate the composition draws them at. A
/// thread of frx7's reads and decodes them ahead.
pub struct Decoder {
    footage: Footage,
    size: (usize, usize),
    /// The planes of a picture that the frame loop takes as RGBA.
    planes: Option<Vec<u8>>,
    /// The video's colour filter, when the pictures are handed over as RGBA.
    grade: Option<Grade>,
    spent: Arc<Spent>,
}

/// What a decoder is asked for.
#[derive(Clone, Copy)]
pub struct Asked<'a> {
    /// The width and height the video is drawn at.
    pub size: (u32, u32),
    /// The composition's frame rate, which the pictures come at.
    pub fps: u32,
    /// The frame of the source to begin with, counted at the composition's rate.
    pub first: u64,
    /// The video's colour filter.
    pub filter: &'a [ColorOp],
    /// Whether a picture with even sides is handed over as its YUV planes, as it comes, for a
    /// renderer that makes the RGBA itself. The colour filter is then the renderer's to apply.
    pub raw: bool,
}

impl Decoder {
    /// Starts reading `file` as `asked`.
    pub fn start(file: &Path, asked: &Asked, spent: &Arc<Spent>) -> Result<Decoder, String> {
        let (width, height) = (asked.size.0 as usize, asked.size.1 as usize);
        // A picture with even sides comes as YUV planes, a third of the bytes. One with an odd
        // side cannot be 4:2:0 and comes as RGBA.
        let planes = yuv::fits(width, height);
        let pace = Pace {
            size: asked.size,
            rate: Rational::new(asked.fps, 1),
            first: asked.first,
            form: if planes { Form::Planes } else { Form::Rgba },
        };
        let (formats, codecs) = registries();
        let footage = Footage::start(file, &pace, &formats, &codecs).map_err(|error| error.to_string())?;
        let raw = asked.raw && planes;
        Ok(Decoder {
            footage,
            size: (width, height),
            planes: (planes && !raw).then(|| vec![0; yuv::size(width, height)]),
            grade: (!raw).then(|| Grade::new(asked.filter)),
            spent: Arc::clone(spent),
        })
    }

    /// The frame of the source the next read gives.
    pub fn next(&self) -> u64 {
        self.footage.next()
    }

    /// Puts the picture frame `wanted` shows into `buffer`, passing over the frames before it,
    /// and waits for it when the decoder's thread does not have it yet. Returns whether a
    /// frame was read. Past the end of the source the buffer keeps the last picture.
    pub fn read(&mut self, wanted: u64, buffer: &mut [u8]) -> Result<bool, String> {
        let began = Instant::now();
        let read = match &mut self.planes {
            Some(planes) => self.footage.read(wanted, planes),
            None => self.footage.read(wanted, buffer),
        }
        .map_err(|error| error.to_string())?;
        let taken = Instant::now();
        self.spent.reading.fetch_add((taken - began).as_nanos() as u64, Ordering::Relaxed);
        if let (true, Some(grade)) = (read, &self.grade) {
            match &self.planes {
                // Each row is coloured as soon as it is made, while it is still at hand.
                Some(planes) => yuv::to_rgba(planes, self.size.0, self.size.1, buffer, |row| grade.apply(row)),
                None => grade.apply(buffer),
            }
            self.spent.colouring.fetch_add(taken.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
        Ok(read)
    }
}

/// A still picture in memory: RGBA with the colour multiplied by the alpha, top row first.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub opaque: bool,
}

/// The alpha of a mask gradient at a point of a box, both given as shares of the box.
fn mask_alpha(mask: &Paint, x: f64, y: f64, width: f64, height: f64) -> f64 {
    // Where the point lies along the gradient, the gradient's stops, and its length in pixels.
    let (at, stops, length) = match mask {
        Paint::Solid(color) => return f64::from(color[3]) / 255.0,
        Paint::Linear { angle, stops } => {
            let (sin, cos) = angle.to_radians().sin_cos();
            let length = style::line_length(*angle, width, height);
            let along = ((x - 0.5) * width * sin - (y - 0.5) * height * cos) / length + 0.5;
            (along, stops, length)
        }
        Paint::Radial { reach, centre, stops } => {
            let (rx, ry) = style::radii(*reach, *centre, width, height);
            let along = ((x - centre.0) * width / rx).hypot((y - centre.1) * height / ry);
            (along, stops, rx)
        }
    };
    let alpha = |index: usize| f64::from(stops[index].1[3]) / 255.0;
    let after = stops.partition_point(|stop| stop.0.of(length) <= at);
    match after {
        0 => alpha(0),
        n if n == stops.len() => alpha(n - 1),
        n => {
            let (from, to) = (stops[n - 1].0.of(length), stops[n].0.of(length));
            let share = if to > from { (at - from) / (to - from) } else { 1.0 };
            alpha(n - 1) + (alpha(n) - alpha(n - 1)) * share
        }
    }
}

/// The size in pixels of a still picture.
fn size(folder: &Path, still: &Still) -> Result<(u32, u32), String> {
    let path = folder.join(&still.source);
    if !path.is_file() {
        return Err(format!("the picture `{}` was not found", path.display()));
    }
    let (_, codecs) = registries();
    let (width, height) = codecs
        .still_size(&path)
        .map_err(|error| format!("`{}` is not a picture this build reads: {error}", path.display()))?;
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err(format!("`{}` is {width} by {height} pixels, which is not drawn", path.display()));
    }
    Ok((width, height))
}

/// Does `work` for every item, several at a time, and gives the results in order.
fn each<T: Sync, R: Send>(items: &[T], work: impl Fn(usize, &T) -> Result<R, String> + Sync) -> Result<Vec<R>, String> {
    let mut results: Vec<Option<Result<R, String>>> = items.iter().map(|_| None).collect();
    let share = items.len().div_ceil(8).max(1);
    std::thread::scope(|scope| {
        for (group, (slots, items)) in results.chunks_mut(share).zip(items.chunks(share)).enumerate() {
            let work = &work;
            scope.spawn(move || {
                for (index, (slot, item)) in slots.iter_mut().zip(items).enumerate() {
                    *slot = Some(work(group * share + index, item));
                }
            });
        }
    });
    results
        .into_iter()
        .map(|result| result.unwrap_or_else(|| Err("a picture was not read".to_string())))
        .collect()
}

/// The size of every still of a composition.
pub fn sizes(folder: &Path, stills: &[Still]) -> Result<Vec<(u32, u32)>, String> {
    each(stills, |_, still| size(folder, still))
}

/// Reads every still of a composition at the size given for it.
pub fn stills(folder: &Path, stills: &[Still], sizes: &[(u32, u32)]) -> Result<Vec<Picture>, String> {
    each(stills, |index, still| self::still(folder, still, sizes[index]))
}

/// Reads a still picture at `width` by `height`, and applies its colour filter and its mask.
fn still(folder: &Path, still: &Still, (width, height): (u32, u32)) -> Result<Picture, String> {
    let path = folder.join(&still.source);
    let failed = |error: frx7_base::Error| format!("`{}` could not be read: {error}", path.display());
    let (_, codecs) = registries();
    let read = codecs.still(&path).map_err(failed)?;
    let Some(plane) = read.planes.first() else {
        return Err(format!("`{}` did not decode to a picture", path.display()));
    };
    // Each of red, green, blue and alpha is scaled by itself.
    let mut pixels = vec![0_u8; (width * height * 4) as usize];
    let sited = (Siting::Middle, Siting::Middle);
    let mut scaler = Scaler::new((read.width, read.height), (width, height), 4, sited).map_err(failed)?;
    let rows = Rows {
        data: &plane.data,
        stride: plane.stride,
    };
    scaler.run(rows, &mut pixels, (width * 4) as usize).map_err(failed)?;
    apply_color_ops(&still.filter, &mut pixels);
    if let Some(mask) = &still.mask {
        let (w, h) = (f64::from(width), f64::from(height));
        for (row, line) in pixels.chunks_exact_mut((width * 4) as usize).enumerate() {
            let y = (row as f64 + 0.5) / h;
            // A fade that runs straight down is the same along a row.
            let straight = matches!(mask, Paint::Linear { angle, .. } if (*angle % 180.0).abs() < 1e-9);
            let row_alpha = straight.then(|| mask_alpha(mask, 0.5, y, w, h));
            for (column, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let alpha = row_alpha.unwrap_or_else(|| mask_alpha(mask, (column as f64 + 0.5) / w, y, w, h));
                pixel[3] = (f64::from(pixel[3]) * alpha).round() as u8;
            }
        }
    }
    let mut opaque = true;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha != 255 {
            opaque = false;
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
    }
    Ok(Picture {
        width,
        height,
        pixels,
        opaque,
    })
}
