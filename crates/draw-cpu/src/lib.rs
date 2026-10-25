//! The renderer on the processor (`docs/spec/render-command.md` section 4). It defines the correct
//! picture: the renderer on the card must agree with it within the tolerance of `PLAN.md`
//! section 5, and it is tested exactly, against checked-in pictures and worked values.
//!
//! The marks of a frame are carried out in order. A fill is written into the picture. A paragraph of text
//! is filled by vello_cpu into a layer of its own, which is then laid over the picture with each pixel's
//! coverage, the same integer rounding for every pixel, so that the picture does not depend on anything but
//! the frame. The marks of an element that moves are drawn with its map, and those of one that fades into a layer
//! of their own, which is laid over the picture with its opacity (`docs/spec/render-command.md`, section 4.5).

use std::sync::Arc;

use draw::{
    level, Background, Cut, DisplayList, Error, Frame, GlyphPicture, GlyphRun, Group, Map, Mark, Outline, Picture, Renderer,
    ResolvedLayer, RingMark, Shadow, Step, TextMark, Turn,
};
use layout::Rect;
use vello_cpu::kurbo::{self, Affine, Stroke};
use vello_cpu::peniko::{Blob, Color, FontData, ImageAlphaType, ImageQuality, ImageSampler};
use vello_cpu::{Glyph, Image, ImageSource, PixelMetadata, Pixmap, RenderContext, Resources};

mod edges;

pub use edges::{ring_coverage, rounded_coverage};

/// The most pixels of a frame this renderer holds (a frame of 8192 by 8192 has fewer).
const MOST_PIXELS: u64 = 8192 * 8192;

/// The renderer on the processor.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuRenderer;

impl Renderer for CpuRenderer {
    fn name(&self) -> &'static str {
        "processor"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        self.draw_frame(&Frame::from_display_list(list))
    }

    fn draws_text(&self) -> bool {
        true
    }

    fn draw_frame(&mut self, frame: &Frame) -> Result<Picture, Error> {
        let pixels = u64::from(frame.width) * u64::from(frame.height);
        if pixels == 0 || pixels > MOST_PIXELS {
            return Err(Error::new(format!(
                "a frame of {}x{} pixels cannot be drawn: it has none or more than {MOST_PIXELS}",
                frame.width, frame.height
            )));
        }
        let background = [frame.background.red, frame.background.green, frame.background.blue, 255];
        let mut bytes = Vec::with_capacity(pixels as usize * 4);
        for _ in 0..pixels {
            bytes.extend_from_slice(&background);
        }
        let whole = (0, 0, frame.width, frame.height);
        draw_marks(&mut bytes, frame.width, frame.height, &frame.marks, Map::IDENTITY, whole)?;
        Picture::new(frame.width, frame.height, bytes).ok_or_else(|| Error::new("the picture does not have the bytes of its size"))
    }
}

/// A region of the frame: left, top, right and bottom, the right and bottom edges not in it.
type Region = (u32, u32, u32, u32);

/// Draws marks over premultiplied pixels of the frame's size, each with `map`, changing only the pixels of `region`.
fn draw_marks(bytes: &mut [u8], width: u32, height: u32, marks: &[Mark], map: Map, region: Region) -> Result<(), Error> {
    for mark in marks {
        match mark {
            Mark::Step(step) => fill(bytes, width, height, step, map, region)?,
            Mark::Text(text) => match map.whole_move() {
                Some((across, down)) => draw_text(bytes, width, height, &text.moved(across, down), region)?,
                None => draw_mapped_text(bytes, width, height, text, map, region)?,
            },
            Mark::Group(group) => draw_group(bytes, width, height, group, map, region)?,
            Mark::Background(background) => draw_background(bytes, width, height, background, map, region)?,
            Mark::Ring(ring) => draw_ring(bytes, width, height, ring, map, region)?,
            Mark::Cut(cut) => draw_cut(bytes, width, height, cut, map, region)?,
        }
    }
    Ok(())
}

/// Draws the marks of an element that moves (`docs/spec/render-command.md`, section 4.5, rules 6 and 7): with its map put
/// after `map`, cut to its clip; with an opacity below 1 into a transparent layer, which is then laid over the picture with
/// each of its values times the opacity, rounded to the nearest level, halves up. At opacity 0 nothing is drawn.
fn draw_group(bytes: &mut [u8], width: u32, height: u32, group: &Group, map: Map, region: Region) -> Result<(), Error> {
    let opacity = if group.opacity.is_nan() {
        0.0
    } else {
        group.opacity.clamp(0.0, 1.0)
    };
    if opacity == 0.0 {
        return Ok(());
    }
    let map = map.then_after(group.map);
    if !map.is_finite() {
        return Err(Error::new("an element is moved by a map that is not finite"));
    }
    let region = match &group.clip {
        None => region,
        Some(clip) => match within(region, clip, width, height) {
            Some(region) => region,
            None => return Ok(()),
        },
    };
    if let Some(turn) = group.turn.filter(|turn| !turn.is_none()) {
        return draw_turned(bytes, width, height, group, map, opacity, turn.about(map), region);
    }
    if opacity == 1.0 {
        return draw_marks(bytes, width, height, &group.marks, map, region);
    }
    let mut layer = vec![0_u8; bytes.len()];
    draw_marks(&mut layer, width, height, &group.marks, map, region)?;
    let (left, top, right, bottom) = region;
    for row in top..bottom {
        let start = (row as usize * width as usize + left as usize) * 4;
        let end = (row as usize * width as usize + right as usize) * 4;
        let over = layer[start..end].as_chunks::<4>().0;
        for (pixel, source) in bytes[start..end].as_chunks_mut::<4>().0.iter_mut().zip(over) {
            if source[3] == 0 {
                continue;
            }
            let faded = source.map(|value| (f64::from(value) * opacity + 0.5).floor() as u32);
            let kept = 255 - faded[3];
            for channel in 0..4 {
                pixel[channel] = (faded[channel] + divide_by_255(u32::from(pixel[channel]) * kept)) as u8;
            }
        }
    }
    Ok(())
}

/// Draws a group turned in space (`docs/spec/render-command.md`, section 4.5, rule 10): its marks with `map` and its
/// opacity into a flat picture of the frame's size, which is then laid over the picture in `region` through the inverse
/// of `turn`, each value taken bilinearly from the four pixels around the point and rounded to the nearest level, halves
/// up. Nothing is drawn edge-on, or when a corner of the group's rectangle comes to the eye or behind it.
#[allow(clippy::too_many_arguments)]
fn draw_turned(
    bytes: &mut [u8],
    width: u32,
    height: u32,
    group: &Group,
    map: Map,
    opacity: f64,
    turn: Turn,
    region: Region,
) -> Result<(), Error> {
    if !turn.draws(map) {
        return Ok(());
    }
    let Some(inverse) = turn.inverse() else {
        return Ok(());
    };
    let mut flat = vec![0_u8; bytes.len()];
    draw_marks(&mut flat, width, height, &group.marks, map, (0, 0, width, height))?;
    if opacity < 1.0 {
        for value in &mut flat {
            *value = (f64::from(*value) * opacity + 0.5).floor() as u8;
        }
    }
    let value_at = |x: i64, y: i64, channel: usize| -> f64 {
        if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
            return 0.0;
        }
        f64::from(flat[(y as usize * width as usize + x as usize) * 4 + channel])
    };
    let (left, top, right, bottom) = region;
    for row in top..bottom {
        for column in left..right {
            let Some((x, y)) = turn.back(&inverse, (f64::from(column) + 0.5, f64::from(row) + 0.5)) else {
                continue;
            };
            let (x, y) = (x - 0.5, y - 0.5);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = (x - x0, y - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let taken = [0, 1, 2, 3].map(|channel| {
                let top_row = value_at(x0, y0, channel) * (1.0 - fx) + value_at(x0 + 1, y0, channel) * fx;
                let bottom_row = value_at(x0, y0 + 1, channel) * (1.0 - fx) + value_at(x0 + 1, y0 + 1, channel) * fx;
                ((top_row * (1.0 - fy) + bottom_row * fy + 0.5).floor()).clamp(0.0, 255.0) as u32
            });
            if taken[3] == 0 {
                continue;
            }
            let at = (row as usize * width as usize + column as usize) * 4;
            let pixel = &mut bytes[at..at + 4];
            let kept = 255 - taken[3];
            for channel in 0..4 {
                pixel[channel] = (taken[channel] + divide_by_255(u32::from(pixel[channel]) * kept)).min(255) as u8;
            }
        }
    }
    Ok(())
}

/// Lays a ring of a border over the picture, in `region` (`docs/spec/box-edges.md`, sections 3 and 5): each pixel takes the
/// ring's colour by src-over with the colour's alpha times the ring's coverage there, rounded to the nearest whole level.
fn draw_ring(
    bytes: &mut [u8],
    width: u32,
    height: u32,
    mark: &RingMark,
    map: Map,
    (left, top, right, bottom): Region,
) -> Result<(), Error> {
    let Some(coverage) = ring_coverage(mark.ring, map, width, height)? else {
        return Ok(());
    };
    let colour = mark.colour;
    let paint = [colour.red, colour.green, colour.blue, 255];
    for line in 0..coverage.height {
        let y = coverage.top + line;
        if y < top || y >= bottom {
            continue;
        }
        for column in 0..coverage.width {
            let x = coverage.left + column;
            if x < left || x >= right {
                continue;
            }
            let covered = u32::from(coverage.alpha[(line * coverage.width + column) as usize]);
            let share = divide_by_255(covered * u32::from(colour.alpha));
            if share == 0 {
                continue;
            }
            let at = (y as usize * width as usize + x as usize) * 4;
            let pixel = &mut bytes[at..at + 4];
            for channel in 0..4 {
                let value = u32::from(paint[channel]) * share + u32::from(pixel[channel]) * (255 - share);
                pixel[channel] = divide_by_255(value) as u8;
            }
        }
    }
    Ok(())
}

/// Draws marks cut to a rounded rectangle (`docs/spec/box-edges.md`, sections 3 and 4). A shape of whole pixels under a move by
/// whole pixels is a cut to its rectangle: only its pixels change. Any other shape draws the marks into a transparent layer,
/// weighs each value of the layer by the shape's coverage, rounded to the nearest whole level, and lays the layer over the
/// picture.
fn draw_cut(bytes: &mut [u8], width: u32, height: u32, cut: &Cut, map: Map, region: Region) -> Result<(), Error> {
    if let (Some((across, down)), Some(rect)) = (map.whole_move(), cut.shape.whole_rect()) {
        let moved = Rect {
            left: (i64::from(rect.left) + across).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            top: (i64::from(rect.top) + down).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            ..rect
        };
        return match within(region, &moved, width, height) {
            Some(region) => draw_marks(bytes, width, height, &cut.marks, map, region),
            None => Ok(()),
        };
    }
    let Some(coverage) = rounded_coverage(cut.shape, map, width, height)? else {
        return Ok(());
    };
    let held = Rect {
        left: coverage.left as i32,
        top: coverage.top as i32,
        width: coverage.width,
        height: coverage.height,
    };
    let Some(region) = within(region, &held, width, height) else {
        return Ok(());
    };
    let mut layer = vec![0_u8; bytes.len()];
    draw_marks(&mut layer, width, height, &cut.marks, map, region)?;
    let (left, top, right, bottom) = region;
    for y in top..bottom {
        for x in left..right {
            let covered = u32::from(coverage.alpha[((y - coverage.top) * coverage.width + (x - coverage.left)) as usize]);
            let at = (y as usize * width as usize + x as usize) * 4;
            for value in &mut layer[at..at + 4] {
                *value = divide_by_255(u32::from(*value) * covered) as u8;
            }
        }
    }
    composite(bytes, width, &layer, region);
    Ok(())
}

/// The part of `region` that a clip holds, inside the frame; `None` when it holds no pixel.
fn within((left, top, right, bottom): Region, clip: &Rect, width: u32, height: u32) -> Option<Region> {
    let visible = clip.visible(width, height)?;
    let region = (
        left.max(visible.left),
        top.max(visible.top),
        right.min(visible.right),
        bottom.min(visible.bottom),
    );
    (region.0 < region.2 && region.1 < region.3).then_some(region)
}

/// Writes one fill step into the picture, in `region`. Moved by whole pixels, every pixel of its moved rectangle becomes its
/// colour. Under any other map its coverage (`fill_coverage`) blends its colour over the picture.
fn fill(bytes: &mut [u8], width: u32, height: u32, step: &Step, map: Map, (left, top, right, bottom): Region) -> Result<(), Error> {
    let Step::Fill { rect, colour } = *step;
    let paint = [colour.red, colour.green, colour.blue, 255];
    if let Some((across, down)) = map.whole_move() {
        let moved = Rect {
            left: (i64::from(rect.left) + across).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            top: (i64::from(rect.top) + down).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            ..rect
        };
        let Some(visible) = moved.visible(width, height) else {
            return Ok(());
        };
        let (from_x, to_x) = (visible.left.max(left), visible.right.min(right));
        let row_length = width as usize;
        for row in visible.top.max(top)..visible.bottom.min(bottom) {
            if from_x >= to_x {
                break;
            }
            let start = (row as usize * row_length + from_x as usize) * 4;
            let end = (row as usize * row_length + to_x as usize) * 4;
            for pixel in bytes[start..end].as_chunks_mut::<4>().0 {
                pixel.copy_from_slice(&paint);
            }
        }
        return Ok(());
    }
    let Some(coverage) = fill_coverage(rect, map, width, height)? else {
        return Ok(());
    };
    for line in 0..coverage.height {
        let y = coverage.top + line;
        if y < top || y >= bottom {
            continue;
        }
        for column in 0..coverage.width {
            let x = coverage.left + column;
            if x < left || x >= right {
                continue;
            }
            let share = u32::from(coverage.alpha[(line * coverage.width + column) as usize]);
            if share == 0 {
                continue;
            }
            let at = (y as usize * width as usize + x as usize) * 4;
            let pixel = &mut bytes[at..at + 4];
            for channel in 0..4 {
                let value = u32::from(paint[channel]) * share + u32::from(pixel[channel]) * (255 - share);
                pixel[channel] = divide_by_255(value) as u8;
            }
        }
    }
    Ok(())
}

/// Lays the layers of a background over the picture, in `region` (`docs/spec/render-command.md`, section 4.6, rules 8 and
/// 9): the last listed first, each pixel taken at its centre. Moved by whole pixels, the layers fill the moved rectangle. Under
/// any other map, each pixel the mapped rectangle covers is taken back through the inverse of the map, and its colour is laid
/// with the share of the pixel the rectangle covers.
fn draw_background(
    bytes: &mut [u8],
    width: u32,
    height: u32,
    background: &Background,
    map: Map,
    (left, top, right, bottom): Region,
) -> Result<(), Error> {
    let rect = background.rect;
    if rect.width == 0 || rect.height == 0 {
        return Ok(());
    }
    let layers: Vec<ResolvedLayer> = background
        .layers
        .iter()
        .rev()
        .map(|layer| layer.resolve(f64::from(rect.width), f64::from(rect.height)))
        .collect();
    let mut lay_at = |x: u32, y: u32, point: (f64, f64), share: u32| {
        let at = (y as usize * width as usize + x as usize) * 4;
        let pixel = &mut bytes[at..at + 4];
        for layer in &layers {
            if let Some(colour) = layer.colour_at(point.0, point.1) {
                lay(pixel, colour, share);
            }
        }
    };
    if let Some((across, down)) = map.whole_move() {
        let moved = Rect {
            left: (i64::from(rect.left) + across).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            top: (i64::from(rect.top) + down).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            ..rect
        };
        let Some(visible) = moved.visible(width, height) else {
            return Ok(());
        };
        for y in visible.top.max(top)..visible.bottom.min(bottom) {
            for x in visible.left.max(left)..visible.right.min(right) {
                let point = (
                    f64::from(x) + 0.5 - f64::from(moved.left),
                    f64::from(y) + 0.5 - f64::from(moved.top),
                );
                lay_at(x, y, point, 255);
            }
        }
        return Ok(());
    }
    let Some(inverse) = map.inverse() else {
        return Ok(());
    };
    let Some(coverage) = fill_coverage(rect, map, width, height)? else {
        return Ok(());
    };
    for line in 0..coverage.height {
        let y = coverage.top + line;
        if y < top || y >= bottom {
            continue;
        }
        for column in 0..coverage.width {
            let x = coverage.left + column;
            if x < left || x >= right {
                continue;
            }
            let share = u32::from(coverage.alpha[(line * coverage.width + column) as usize]);
            if share == 0 {
                continue;
            }
            let (bx, by) = inverse.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
            lay_at(x, y, (bx - f64::from(rect.left), by - f64::from(rect.top)), share);
        }
    }
    Ok(())
}

/// Lays a premultiplied colour, times `share` of 255, over one pixel by src-over (`docs/spec/render-command.md`, section 4.6,
/// rule 8).
fn lay(pixel: &mut [u8], colour: [f64; 4], share: u32) {
    let source = colour.map(|value| u32::from(level(value * f64::from(share) / 255.0)));
    let kept = 255 - source[3];
    for channel in 0..4 {
        pixel[channel] = (source[channel] + divide_by_255(u32::from(pixel[channel]) * kept)).min(255) as u8;
    }
}

/// The layers of a background drawn with `map` into a transparent picture of the frame's size, for the card to lay over
/// its picture (`docs/spec/card-renderer.md`, section 11).
pub fn background_layer(width: u32, height: u32, background: &Background, map: Map) -> Result<Vec<u8>, Error> {
    let mut layer = vec![0_u8; width as usize * height as usize * 4];
    draw_background(&mut layer, width, height, background, map, (0, 0, width, height))?;
    Ok(layer)
}

/// The coverage of a mapped rectangle: the share of each pixel it covers, from 0 to 255, in a box of whole pixels of the
/// frame that holds the mapped rectangle, cut to the frame (`docs/spec/card-renderer.md`, section 10.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coverage {
    /// The frame pixel of the box's top left corner.
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    /// One byte for each pixel of the box, row by row.
    pub alpha: Vec<u8>,
}

/// Fills the rectangle mapped by `map` in opaque white with vello_cpu, in the box of whole pixels of the frame that holds it,
/// and gives the coverage of each pixel; `None` when the box holds no pixel of the frame. The processor and the card both
/// take a mapped fill from here.
pub fn fill_coverage(rect: Rect, map: Map, width: u32, height: u32) -> Result<Option<Coverage>, Error> {
    if !map.is_finite() {
        return Err(Error::new("a box is moved by a map that is not finite"));
    }
    if rect.width == 0 || rect.height == 0 {
        return Ok(None);
    }
    let (x0, y0) = (f64::from(rect.left), f64::from(rect.top));
    let (x1, y1) = (x0 + f64::from(rect.width), y0 + f64::from(rect.height));
    let corners = [map.apply(x0, y0), map.apply(x1, y0), map.apply(x1, y1), map.apply(x0, y1)];
    let least_x = corners.iter().map(|corner| corner.0).fold(f64::INFINITY, f64::min);
    let most_x = corners.iter().map(|corner| corner.0).fold(f64::NEG_INFINITY, f64::max);
    let least_y = corners.iter().map(|corner| corner.1).fold(f64::INFINITY, f64::min);
    let most_y = corners.iter().map(|corner| corner.1).fold(f64::NEG_INFINITY, f64::max);
    let left = least_x.floor().max(0.0);
    let top = least_y.floor().max(0.0);
    let right = most_x.ceil().min(f64::from(width));
    let bottom = most_y.ceil().min(f64::from(height));
    if left >= right || top >= bottom {
        return Ok(None);
    }
    let (box_width, box_height) = ((right - left) as u32, (bottom - top) as u32);
    let mut context = RenderContext::new(box_width as u16, box_height as u16);
    let mut resources = Resources::new();
    context.set_paint(Color::from_rgba8(255, 255, 255, 255));
    context.set_transform(Affine::translate((-left, -top)) * affine(map));
    context.fill_rect(&kurbo::Rect::new(x0, y0, x1, y1));
    context.flush();
    let mut pixmap = Pixmap::new(box_width as u16, box_height as u16);
    context.render(&mut pixmap, &mut resources);
    let alpha = pixmap.data_as_u8_slice().as_chunks::<4>().0.iter().map(|pixel| pixel[3]).collect();
    Ok(Some(Coverage {
        left: left as u32,
        top: top as u32,
        width: box_width,
        height: box_height,
        alpha,
    }))
}

/// The map as the rasteriser's affine map.
fn affine(map: Map) -> Affine {
    Affine::new([map.a, map.b, map.c, map.d, map.e, map.f])
}

/// Lays the glyph runs of a paragraph over the picture: its shadows first, the last listed first so that the first listed is on
/// top, then its glyphs with their outline and colour pictures, in one layer of the frame's size, which is composited with each
/// pixel's coverage, inside the paragraph's clip.
fn draw_text(bytes: &mut [u8], width: u32, height: u32, text: &TextMark, region: Region) -> Result<(), Error> {
    let region = match &text.clip {
        None => region,
        Some(clip) => match within(region, clip, width, height) {
            Some(region) => region,
            None => return Ok(()),
        },
    };
    for shadow in text.shadows.iter().rev() {
        let layer = shadow_layer(width, height, text, shadow, Map::IDENTITY)?;
        composite(bytes, width, &layer, region);
    }
    let layer = text_layer(width, height, text)?;
    composite(bytes, width, &layer, region);
    Ok(())
}

/// Lays a paragraph mapped by `map` over the picture, in `region` and its clip (`docs/spec/render-command.md`, section 4.5,
/// rule 7): its shadows, the last listed first, then one layer with its glyph runs, its outline over them and its colour
/// pictures over those, each drawn with the map.
fn draw_mapped_text(bytes: &mut [u8], width: u32, height: u32, text: &TextMark, map: Map, region: Region) -> Result<(), Error> {
    let region = match &text.clip {
        None => region,
        Some(clip) => match within(region, clip, width, height) {
            Some(region) => region,
            None => return Ok(()),
        },
    };
    for shadow in text.shadows.iter().rev() {
        let layer = shadow_layer(width, height, text, shadow, map)?;
        composite(bytes, width, &layer, region);
    }
    let mut context = RenderContext::new(width as u16, height as u16);
    let mut resources = Resources::new();
    context.set_transform(affine(map));
    for run in &text.runs {
        fill_run(&mut context, &mut resources, run, (0.0, 0.0))?;
    }
    context.flush();
    let mut layer = Pixmap::new(width as u16, height as u16);
    context.render(&mut layer, &mut resources);
    if let Some(outline) = &text.outline {
        lay_layer(layer.data_as_u8_slice_mut(), &outline_layer(width, height, text, outline, map)?);
    }
    if !text.pictures.is_empty() {
        lay_layer(layer.data_as_u8_slice_mut(), &pictures_layer(width, height, text, map, (0.0, 0.0))?);
    }
    composite(bytes, width, layer.data_as_u8_slice(), region);
    Ok(())
}

/// The colour pictures of the emoji of a paragraph, mapped by `map` and moved by `offset` in the paragraph's own coordinates,
/// as one layer of premultiplied pixels of the frame's size: each picture, with its corner at the whole pixel nearest to its
/// place, is drawn through the map by vello_cpu and sampled bilinearly (`docs/spec/render-command.md`, section 4.5, rule 7).
pub fn pictures_layer(width: u32, height: u32, text: &TextMark, map: Map, offset: (f32, f32)) -> Result<Vec<u8>, Error> {
    let mut context = RenderContext::new(width as u16, height as u16);
    let mut resources = Resources::new();
    for picture in &text.pictures {
        if picture.width == 0 || picture.height == 0 {
            continue;
        }
        if picture.width > u32::from(u16::MAX) || picture.height > u32::from(u16::MAX) {
            return Err(Error::new(format!(
                "a picture of {}x{} pixels cannot be drawn: a side is above {}",
                picture.width,
                picture.height,
                u16::MAX
            )));
        }
        let pixmap = Pixmap::from_parts(
            picture.pixels.as_ref().clone(),
            picture.width as u16,
            picture.height as u16,
            PixelMetadata {
                may_have_transparency: true,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
            },
        );
        let left = whole_pixel(picture.x + offset.0) as f64;
        let top = whole_pixel(picture.y + offset.1) as f64;
        context.set_transform(affine(map) * Affine::translate((left, top)));
        context.set_paint(Image {
            image: ImageSource::Pixmap(Arc::new(pixmap)),
            sampler: ImageSampler {
                quality: ImageQuality::Medium,
                ..ImageSampler::default()
            },
        });
        context.fill_rect(&kurbo::Rect::new(0.0, 0.0, f64::from(picture.width), f64::from(picture.height)));
    }
    context.flush();
    let mut layer = Pixmap::new(width as u16, height as u16);
    context.render(&mut layer, &mut resources);
    Ok(layer.data_as_u8_slice().to_vec())
}

/// The glyphs of a paragraph, its outline and its colour pictures, as one layer of premultiplied pixels of the frame's size:
/// the fills of the glyph runs, the outline laid over them, and the pictures of the emoji over those (`docs/spec/text.md`,
/// sections 6.6 and 12.4).
fn text_layer(width: u32, height: u32, text: &TextMark) -> Result<Vec<u8>, Error> {
    let mut context = RenderContext::new(width as u16, height as u16);
    let mut resources = Resources::new();
    for run in &text.runs {
        fill_run(&mut context, &mut resources, run, (0.0, 0.0))?;
    }
    context.flush();
    let mut layer = Pixmap::new(width as u16, height as u16);
    context.render(&mut layer, &mut resources);
    if let Some(outline) = &text.outline {
        lay_layer(
            layer.data_as_u8_slice_mut(),
            &outline_layer(width, height, text, outline, Map::IDENTITY)?,
        );
    }
    // The colour pictures of the emoji go into the same layer, after the glyph runs, in the order of the text
    // (`docs/spec/text.md`, section 12.4, point 10).
    for picture in &text.pictures {
        lay_picture(layer.data_as_u8_slice_mut(), width, height, picture);
    }
    Ok(layer.data_as_u8_slice().to_vec())
}

/// The outline of the glyph runs of a paragraph as a layer of premultiplied pixels of the frame's size: a line `outline.width`
/// pixels wide, centred on the outline of each glyph, in `outline.colour` (`docs/spec/text.md`, section 6.6, rule 6), stroked in
/// the paragraph's own coordinates and mapped by `map`.
pub fn outline_layer(width: u32, height: u32, text: &TextMark, outline: &Outline, map: Map) -> Result<Vec<u8>, Error> {
    let mut context = RenderContext::new(width as u16, height as u16);
    let mut resources = Resources::new();
    if !map.is_identity() {
        context.set_transform(affine(map));
    }
    let colour = outline.colour;
    context.set_paint(Color::from_rgba8(colour.red, colour.green, colour.blue, 255));
    context.set_stroke(Stroke::new(f64::from(outline.width)));
    for run in &text.runs {
        stroke_run(&mut context, &mut resources, run)?;
    }
    context.flush();
    let mut layer = Pixmap::new(width as u16, height as u16);
    context.render(&mut layer, &mut resources);
    Ok(layer.data_as_u8_slice().to_vec())
}

/// The shadow of a paragraph as a layer of premultiplied pixels of the frame's size (`docs/spec/text.md`, section 6.6, rule 5):
/// the alpha of the glyphs and of the colour pictures, moved by the shadow's offset and blurred, in the shadow's colour. The
/// outline does not cast a shadow. Under a map other than the identity, the offset is in the paragraph's own coordinates and
/// mapped with it, and the blur is scaled by the square root of how much the map scales an area (`docs/spec/render-command.md`,
/// section 4.5, rule 7).
pub fn shadow_layer(width: u32, height: u32, text: &TextMark, shadow: &Shadow, map: Map) -> Result<Vec<u8>, Error> {
    let mut context = RenderContext::new(width as u16, height as u16);
    let mut resources = Resources::new();
    let mapped = !map.is_identity();
    if mapped {
        context.set_transform(affine(map));
    }
    for run in &text.runs {
        fill_run(&mut context, &mut resources, run, (shadow.across, shadow.down))?;
    }
    context.flush();
    let mut layer = Pixmap::new(width as u16, height as u16);
    context.render(&mut layer, &mut resources);
    if mapped && !text.pictures.is_empty() {
        lay_layer(
            layer.data_as_u8_slice_mut(),
            &pictures_layer(width, height, text, map, (shadow.across, shadow.down))?,
        );
    }
    for picture in text.pictures.iter().filter(|_| !mapped) {
        let moved = GlyphPicture {
            width: picture.width,
            height: picture.height,
            pixels: picture.pixels.clone(),
            x: picture.x + shadow.across,
            y: picture.y + shadow.down,
        };
        lay_picture(layer.data_as_u8_slice_mut(), width, height, &moved);
    }
    let mut alpha: Vec<u8> = layer.data_as_u8_slice().as_chunks::<4>().0.iter().map(|pixel| pixel[3]).collect();
    let blur = if mapped {
        (f64::from(shadow.blur) * map.area_scale().sqrt()) as f32
    } else {
        shadow.blur
    };
    blur_alpha(&mut alpha, width as usize, height as usize, blur);
    let colour = shadow.colour;
    let mut picture = Vec::with_capacity(alpha.len() * 4);
    for alpha in alpha {
        let level = u32::from(alpha);
        picture.extend_from_slice(&[
            divide_by_255(u32::from(colour.red) * level) as u8,
            divide_by_255(u32::from(colour.green) * level) as u8,
            divide_by_255(u32::from(colour.blue) * level) as u8,
            alpha,
        ]);
    }
    Ok(picture)
}

/// Blurs an alpha plane of `width` by `height` pixels as notation 7.6 blurs a picture, with a standard deviation of half of
/// `blur`: three passes, each a horizontal average and then a vertical one, over a span of
/// `floor(sigma * 3 * sqrt(2 * pi) / 4 + 0.5)` pixels. A span below 2 leaves the plane as it is.
fn blur_alpha(alpha: &mut [u8], width: usize, height: usize, blur: f32) {
    let sigma = f64::from(blur) / 2.0;
    let span = (sigma * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0 + 0.5).floor() as usize;
    if span < 2 {
        return;
    }
    let mut column = vec![0_u8; height];
    for _ in 0..3 {
        for row in alpha.chunks_exact_mut(width) {
            average_line(row, span);
        }
        for x in 0..width {
            for (y, value) in column.iter_mut().enumerate() {
                *value = alpha[y * width + x];
            }
            average_line(&mut column, span);
            for (y, value) in column.iter().enumerate() {
                alpha[y * width + x] = *value;
            }
        }
    }
}

/// Averages a line of alpha over a span of `span` pixels: each pixel becomes the average of the `span` pixels that run from
/// `(span - 1) / 2` before it, a pixel outside the line counting as clear, rounded to the nearest whole level, halves up.
fn average_line(line: &mut [u8], span: usize) {
    let length = line.len() as isize;
    let mut prefix = vec![0_u32; line.len() + 1];
    for (at, value) in line.iter().enumerate() {
        prefix[at + 1] = prefix[at] + u32::from(*value);
    }
    let before = ((span - 1) / 2) as isize;
    let count = span as u32;
    for x in 0..length {
        let from = (x - before).clamp(0, length) as usize;
        let to = (x - before + span as isize).clamp(0, length) as usize;
        let sum = prefix[to] - prefix[from];
        line[x as usize] = ((sum + count / 2) / count) as u8;
    }
}

/// Lays a layer of premultiplied pixels over another of the same size, each pixel as a colour picture is laid over a layer.
fn lay_layer(layer: &mut [u8], over: &[u8]) {
    for (destination, source) in layer.as_chunks_mut::<4>().0.iter_mut().zip(over.as_chunks::<4>().0.iter()) {
        let kept = 255 - u32::from(source[3]);
        for channel in 0..4 {
            let under = u32::from(destination[channel]) * kept;
            destination[channel] = (u32::from(source[channel]) + divide_by_255(under)) as u8;
        }
    }
}

/// Lays a colour picture over a layer of premultiplied pixels, with its top-left corner at the whole pixel nearest to its
/// place, halves up. Each pixel is laid over what is there by the same src-over rounding as the layer (`divide_by_255`).
fn lay_picture(layer: &mut [u8], width: u32, height: u32, picture: &GlyphPicture) {
    let left = whole_pixel(picture.x);
    let top = whole_pixel(picture.y);
    let row_length = picture.width as usize * 4;
    for line in 0..picture.height as i64 {
        let y = top + line;
        if y < 0 || y >= i64::from(height) {
            continue;
        }
        for column in 0..picture.width as i64 {
            let x = left + column;
            if x < 0 || x >= i64::from(width) {
                continue;
            }
            let source = &picture.pixels[line as usize * row_length + column as usize * 4..][..4];
            let at = (y as usize * width as usize + x as usize) * 4;
            let destination = &mut layer[at..at + 4];
            let kept = 255 - u32::from(source[3]);
            for channel in 0..4 {
                let under = u32::from(destination[channel]) * kept;
                destination[channel] = (u32::from(source[channel]) + divide_by_255(under)) as u8;
            }
        }
    }
}

/// The whole pixel nearest to a coordinate, halves up.
fn whole_pixel(value: f32) -> i64 {
    (f64::from(value) + 0.5).floor() as i64
}

/// Fills the glyphs of one run in its colour, sheared by its skew about each glyph's origin, and moved by `offset`.
fn fill_run(context: &mut RenderContext, resources: &mut Resources, run: &GlyphRun, offset: (f32, f32)) -> Result<(), Error> {
    context.set_paint(Color::from_rgba8(run.colour.red, run.colour.green, run.colour.blue, 255));
    let (font, transform, glyphs) = run_parts(run, offset);
    context
        .glyph_run(resources, &font)
        .hint(false)
        .font_size(run.size)
        .glyph_transform(transform)
        .fill_glyphs(glyphs.into_iter())
        .map_err(|error| Error::new(format!("the glyphs of a run cannot be drawn: {error}")))
}

/// Strokes the outline of the glyphs of one run, with the paint and the stroke the context already has.
fn stroke_run(context: &mut RenderContext, resources: &mut Resources, run: &GlyphRun) -> Result<(), Error> {
    let (font, transform, glyphs) = run_parts(run, (0.0, 0.0));
    context
        .glyph_run(resources, &font)
        .hint(false)
        .font_size(run.size)
        .glyph_transform(transform)
        .stroke_glyphs(glyphs.into_iter())
        .map_err(|error| Error::new(format!("the outline of a run cannot be drawn: {error}")))
}

/// The font, the shear and the glyphs of one run, the glyphs moved by `offset`.
fn run_parts(run: &GlyphRun, offset: (f32, f32)) -> (FontData, Affine, Vec<Glyph>) {
    let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = run.font.data.clone();
    let font = FontData::new(Blob::new(bytes), run.font.index);
    let shear = f64::from(run.skew_degrees).to_radians().tan();
    let transform = Affine::new([1.0, 0.0, -shear, 1.0, 0.0, 0.0]);
    let glyphs = run
        .glyphs
        .iter()
        .map(|glyph| Glyph {
            id: glyph.id,
            x: glyph.x + offset.0,
            y: glyph.y + offset.1,
        })
        .collect();
    (font, transform, glyphs)
}

/// Lays a layer of premultiplied red, green, blue and alpha over the picture's premultiplied bytes, in the region (`left`,
/// `top`, `right`, `bottom`; the right and bottom edges are not in it). Each value of a pixel is its layer value plus the
/// picture's value times what the layer leaves clear, rounded to the nearest whole level; over an opaque picture the alpha
/// stays 255.
fn composite(bytes: &mut [u8], width: u32, layer: &[u8], (left, top, right, bottom): Region) {
    for row in top..bottom {
        for column in left..right {
            let at = (row as usize * width as usize + column as usize) * 4;
            let over = &layer[at..at + 4];
            let alpha = u32::from(over[3]);
            if alpha == 0 {
                continue;
            }
            let pixel = &mut bytes[at..at + 4];
            for channel in 0..4 {
                let kept = u32::from(pixel[channel]) * (255 - alpha);
                pixel[channel] = (u32::from(over[channel]) + divide_by_255(kept)) as u8;
            }
        }
    }
}

/// `value / 255`, rounded to the nearest whole number, for values up to 255 times 255.
fn divide_by_255(value: u32) -> u32 {
    let rounded = value + 128;
    (rounded + (rounded >> 8)) >> 8
}
