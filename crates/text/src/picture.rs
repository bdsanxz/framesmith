//! The colour pictures of the emoji (`docs/spec/text.md`, section 12.4, points 4 to 6): a PNG read from memory by frx7,
//! its colours premultiplied, and scaled to the size it is drawn at. A picture is kept in a cache for the life of its
//! collection, so that an emoji repeated in a text is read once.

use std::collections::HashMap;
use std::sync::Arc;

use base::{Picture, PixelFormat};

use crate::colour::ColourFace;

/// A glyph's picture at one size: its size in pixels, its premultiplied RGBA8 pixels in rows with no padding, and the
/// origin offsets its data gives, in design units. A picture with no pixel in a side is kept with no pixel, and draws nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GlyphPicture {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Arc<Vec<u8>>,
    pub(crate) origin_x: i16,
    pub(crate) origin_y: i16,
}

/// The key of a picture in the cache: the face's blob, its index in the file, the glyph, and the size in pixels as bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    blob: u64,
    index: u32,
    glyph: u32,
    size: u32,
}

/// The pictures of the collections of one composition, by key.
pub(crate) type Cache = HashMap<Key, Arc<GlyphPicture>>;

/// Why a glyph has no picture to draw (`docs/spec/text.md`, section 12.4, point 9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Missing {
    /// No strike of the face has a `png ` graphic for the glyph.
    NoPicture,
    /// The graphic is there but frx7 refuses it, with its reason.
    Refused(String),
}

/// The face a picture is read from: its blob and index, the bytes of its file, and its colour face.
pub(crate) struct Source<'a> {
    pub(crate) blob: u64,
    pub(crate) index: u32,
    pub(crate) bytes: &'a [u8],
    pub(crate) colour: &'a ColourFace,
}

/// The picture of `glyph` at `size` pixels, read once and kept in `cache`.
pub(crate) fn glyph_picture(
    codecs: &codec::Registry,
    cache: &mut Cache,
    source: &Source<'_>,
    glyph: usize,
    size: f32,
) -> Result<Arc<GlyphPicture>, Missing> {
    let key = Key {
        blob: source.blob,
        index: source.index,
        glyph: glyph as u32,
        size: size.to_bits(),
    };
    if let Some(found) = cache.get(&key) {
        return Ok(found.clone());
    }
    let Some((ppem, graphic)) = source.colour.chosen_graphic(source.bytes, glyph, size) else {
        return Err(Missing::NoPicture);
    };
    let read = codecs
        .still_from_memory(graphic.data)
        .map_err(|error| Missing::Refused(error.to_string()))?;
    if read.format != PixelFormat::Rgba8 {
        return Err(Missing::Refused("the picture is not read as RGBA".to_string()));
    }
    let picture = scaled(&read, ppem, size)?;
    let made = Arc::new(GlyphPicture {
        width: picture.width,
        height: picture.height,
        pixels: Arc::new(picture.pixels),
        origin_x: graphic.origin_x,
        origin_y: graphic.origin_y,
    });
    cache.insert(key, made.clone());
    Ok(made)
}

/// A picture packed as rows of premultiplied pixels.
struct Packed {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// The straight RGBA picture as premultiplied pixels, brought to the size it is drawn at: `k` is the size over the strike's
/// `ppem`, and the target is each side times `k`, rounded half up (`docs/spec/text.md`, section 12.4, point 5).
fn scaled(read: &Picture, ppem: u16, size: f32) -> Result<Packed, Missing> {
    let factor = f64::from(size) / f64::from(ppem);
    let target_width = (f64::from(read.width) * factor + 0.5).floor() as u32;
    let target_height = (f64::from(read.height) * factor + 0.5).floor() as u32;
    if target_width == 0 || target_height == 0 {
        return Ok(Packed {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        });
    }
    let mut premultiplied = packed(read);
    for pixel in premultiplied.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in pixel.iter_mut().take(3) {
            *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let premultiplied = Packed {
        width: read.width,
        height: read.height,
        pixels: premultiplied,
    };
    if (target_width, target_height) == (read.width, read.height) {
        return Ok(premultiplied);
    }
    let source = unpacked(&premultiplied);
    let resized = pixel::resize(&source, (target_width, target_height)).map_err(|error| Missing::Refused(error.to_string()))?;
    let mut pixels = packed(&resized);
    // The filter may overshoot, so each colour is limited to its alpha, which keeps the pixel a premultiplied one.
    for pixel in pixels.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = pixel[3];
        for channel in pixel.iter_mut().take(3) {
            *channel = (*channel).min(alpha);
        }
    }
    Ok(Packed {
        width: target_width,
        height: target_height,
        pixels,
    })
}

/// The rows of a picture, packed one after another with no padding, four bytes a pixel.
fn packed(picture: &Picture) -> Vec<u8> {
    let plane = &picture.planes[0];
    let row = picture.width as usize * 4;
    let mut pixels = Vec::with_capacity(row * picture.height as usize);
    for line in 0..picture.height as usize {
        let start = line * plane.stride;
        pixels.extend_from_slice(&plane.data[start..start + row]);
    }
    pixels
}

/// A picture of RGBA pixels, packed, made from the packed pixels given.
fn unpacked(packed: &Packed) -> Picture {
    let mut picture = Picture::new(PixelFormat::Rgba8, packed.width, packed.height);
    let row = packed.width as usize * 4;
    let stride = picture.planes[0].stride;
    for line in 0..packed.height as usize {
        let start = line * row;
        picture.planes[0].data[line * stride..line * stride + row].copy_from_slice(&packed.pixels[start..start + row]);
    }
    picture
}

/// The top-left corner of a glyph's picture on the block, given the pen position of the glyph: `x` from the block's left
/// edge and `y` the baseline from its top (`docs/spec/text.md`, section 12.4, point 6). `None` when the glyph's record
/// cannot be read.
pub(crate) fn top_left(source: &Source<'_>, glyph: usize, picture: &GlyphPicture, size: f32, x: f32, y: f32) -> Option<(f32, f32)> {
    let scale = f64::from(size) / f64::from(source.colour.units_per_em);
    let corner = source.colour.box_corner(source.bytes, glyph)?;
    let (design_x, design_y) = match corner {
        Some((x_min, y_min)) => (f64::from(x) + f64::from(x_min) * scale, f64::from(y) - f64::from(y_min) * scale),
        None => (f64::from(x), f64::from(y)),
    };
    let lower_left_x = design_x + f64::from(picture.origin_x) * scale;
    let lower_left_y = design_y - f64::from(picture.origin_y) * scale;
    Some((lower_left_x as f32, (lower_left_y - f64::from(picture.height)) as f32))
}
