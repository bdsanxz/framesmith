//! The CPU backend: draws a frame into memory we own.

use crate::decode::Picture;
use crate::model::{Blend, Composition, Fit, GlyphRun, Opening};
use crate::noise;
use crate::paint::{self, Brush};
use crate::scene::{closing, Clip, Effect, Op};
use crate::style::{Paint, Rgba, Shadow};
use crate::Backend;
use std::collections::HashMap;
use std::sync::Arc;
use vello_cpu::kurbo::{Affine, Cap, Join, Point, Rect, RoundedRect, Shape, Stroke};
use vello_cpu::peniko::{BlendMode, Compose, Extend, ImageQuality, ImageSampler, Mix};
use vello_cpu::{Glyph, Image, ImageSource, Level, Pixmap, RenderContext, RenderSettings, Resources};

pub struct Cpu {
    context: RenderContext,
    pixmap: Pixmap,
    background: Rgba,
    frame: Rect,
    painter: Painter,
}

/// Draws drawing steps with the CPU, into any picture. The CPU renderer draws whole frames with
/// it; the GPU renderer draws with it the few things the graphics card is not given.
pub struct Painter {
    resources: Resources,
    /// The current picture of each video.
    pub videos: Vec<Option<Arc<Pixmap>>>,
    pub stills: Vec<Arc<Pixmap>>,
    /// The picture of each text's shadows, with where it lies in the text's box and the frame
    /// it was last drawn in.
    shadows: HashMap<(usize, usize), CachedShadow>,
    /// The picture of one tile of each repeated background, with the frame it was last drawn in.
    tiles: HashMap<TileKey, (Arc<Pixmap>, u64)>,
}

/// What tells one tile picture from another: the paint and the tile's size.
type TileKey = (usize, u64, u64);

/// A text's shadow picture, where it lies in the text's box, and the frame it was last drawn in.
type CachedShadow = (Arc<Pixmap>, (f64, f64), u64);

fn settings() -> RenderSettings {
    // One thread. The drawing library can share a frame out over cores, but not yet a frame
    // with a blur in it, and the film has those.
    RenderSettings {
        level: Level::try_detect().unwrap_or(Level::baseline()),
        num_threads: 0,
    }
}

fn pixmap_of(picture: &Picture) -> Pixmap {
    let mut pixmap = Pixmap::new(picture.width as u16, picture.height as u16);
    pixmap.data_as_u8_slice_mut().copy_from_slice(&picture.pixels);
    pixmap.set_may_have_transparency(!picture.opaque);
    pixmap
}

fn image(picture: &Arc<Pixmap>, quality: ImageQuality, alpha: f32) -> Image {
    sampled(picture, quality, alpha, Extend::Pad)
}

fn sampled(picture: &Arc<Pixmap>, quality: ImageQuality, alpha: f32, extend: Extend) -> Image {
    Image {
        image: ImageSource::Pixmap(Arc::clone(picture)),
        sampler: ImageSampler {
            x_extend: extend,
            y_extend: extend,
            quality,
            alpha,
        },
    }
}

/// The picture of one tile of a background, `width` by `height` in the element's pixels. The
/// picture has whole pixels, so it may be a little larger than the tile.
pub fn tile_picture(fill: &Paint, width: f64, height: f64) -> Pixmap {
    let (across, down) = (width.ceil().clamp(1.0, 8192.0), height.ceil().clamp(1.0, 8192.0));
    let mut context = RenderContext::new_with(across as u16, down as u16, settings());
    // The tile is stretched to the picture's whole pixels; it is drawn back at its own size.
    context.set_transform(Affine::scale_non_uniform(across / width, down / height));
    let whole = Rect::new(0.0, 0.0, width, height);
    match paint::brush(fill, whole, 1.0) {
        Brush::Solid(color) => context.set_paint(color),
        Brush::Gradient(gradient, placed) => {
            context.set_paint(gradient);
            context.set_paint_transform(placed);
        }
    }
    context.fill_rect(&whole);
    context.flush();
    let mut pixmap = Pixmap::new(across as u16, down as u16);
    context.render(&mut pixmap, &mut Resources::new());
    pixmap
}

fn mix(blend: Blend) -> Mix {
    match blend {
        Blend::Normal => Mix::Normal,
        Blend::Multiply => Mix::Multiply,
        Blend::Screen => Mix::Screen,
        Blend::Overlay => Mix::Overlay,
        Blend::Darken => Mix::Darken,
        Blend::Lighten => Mix::Lighten,
        Blend::ColorDodge => Mix::ColorDodge,
        Blend::ColorBurn => Mix::ColorBurn,
        Blend::HardLight => Mix::HardLight,
        Blend::SoftLight => Mix::SoftLight,
        Blend::Difference => Mix::Difference,
        Blend::Exclusion => Mix::Exclusion,
        Blend::Hue => Mix::Hue,
        Blend::Saturation => Mix::Saturation,
        Blend::Color => Mix::Color,
        Blend::Luminosity => Mix::Luminosity,
    }
}

/// Multiplies the colours of a picture. The picture holds them already multiplied by their
/// alpha, so a colour never rises above its alpha.
fn brighten(pixmap: &mut Pixmap, brightness: f32) {
    for pixel in pixmap.data_as_u8_slice_mut().as_chunks_mut::<4>().0 {
        let alpha = f32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (f32::from(*channel) * brightness).round().min(alpha) as u8;
        }
    }
}

/// Makes a picture that much clearer.
fn fade(pixmap: &mut Pixmap, opacity: f32) {
    for channel in pixmap.data_as_u8_slice_mut() {
        *channel = (f32::from(*channel) * opacity).round() as u8;
    }
}

/// A picture that is only moved by whole pixels is copied; any other is filtered.
fn quality(transform: Affine) -> ImageQuality {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    if [a, b, c, d] == [1.0, 0.0, 0.0, 1.0] && e.fract() == 0.0 && f.fract() == 0.0 {
        ImageQuality::Low
    } else {
        ImageQuality::Medium
    }
}

fn glyphs(context: &mut RenderContext, resources: &mut Resources, run: &GlyphRun, stroke: bool) -> Result<(), String> {
    let placed = run.glyphs.iter().map(|g| Glyph { id: g.id, x: g.x, y: g.y });
    let mut builder = context
        .glyph_run(resources, &run.font)
        .font_size(run.size)
        .hint(false)
        .normalized_coords(&run.coords);
    if let Some(skew) = run.skew {
        // The transform works on the outline after it is turned to point down, so a slant to the
        // right is a negative skew.
        builder = builder.glyph_transform(Affine::skew(-skew, 0.0));
    }
    let drawn = if stroke {
        builder.stroke_glyphs(placed)
    } else {
        builder.fill_glyphs(placed)
    };
    drawn.map_err(|error| format!("{error:?}"))
}

/// Draws the shadows of some words into a picture of their own. Returns it with the place of
/// its top left corner in the words' space.
fn shadow_picture(runs: &[GlyphRun], shadows: &[Shadow], resources: &mut Resources) -> Option<(Pixmap, (f64, f64))> {
    let mut bounds: Option<Rect> = None;
    for run in runs {
        let size = f64::from(run.size);
        for glyph in &run.glyphs {
            let (x, y) = (f64::from(glyph.x), f64::from(glyph.y));
            let around = Rect::new(x - 0.3 * size, y - 1.2 * size, x + 1.3 * size, y + 0.5 * size);
            bounds = Some(bounds.map_or(around, |bounds| bounds.union(around)));
        }
    }
    let reach = shadows.iter().map(|s| s.blur * 1.5 + s.x.abs().max(s.y.abs())).fold(0.0, f64::max);
    let bounds = bounds?.inflate(reach + 2.0, reach + 2.0).expand();
    let (width, height) = (bounds.width().min(8192.0) as u16, bounds.height().min(8192.0) as u16);
    let mut context = RenderContext::new_with(width, height, settings());
    for shadow in shadows.iter().rev() {
        context.set_transform(Affine::translate((shadow.x - bounds.x0, shadow.y - bounds.y0)));
        let deviation = (shadow.blur / 2.0) as f32;
        if deviation > 0.0 {
            context.push_filter_layer(paint::blur(deviation));
        }
        context.set_paint(paint::color(shadow.color, 1.0));
        for run in runs {
            glyphs(&mut context, resources, run, false).ok()?;
        }
        if deviation > 0.0 {
            context.pop_layer();
        }
    }
    context.flush();
    let mut pixmap = Pixmap::new(width, height);
    context.render(&mut pixmap, resources);
    Some((pixmap, (bounds.x0, bounds.y0)))
}

/// A box around the glyphs of a run, with room for what reaches past their advance.
fn run_bounds(run: &GlyphRun) -> Option<Rect> {
    let size = f64::from(run.size);
    let mut bounds: Option<Rect> = None;
    for glyph in &run.glyphs {
        let (x, y) = (f64::from(glyph.x), f64::from(glyph.y));
        let around = Rect::new(x - 0.3 * size, y - 1.2 * size, x + 1.3 * size, y + 0.5 * size);
        bounds = Some(bounds.map_or(around, |bounds| bounds.union(around)));
    }
    bounds
}

/// The effect of a group, and the part of the frame its flat picture covers, when any of it can
/// be seen. `shown` is the part of the frame that can be seen.
pub fn flat_of(group: &[Op], shown: Rect) -> Option<(Option<Effect>, Rect)> {
    let (effect, room) = match group.first() {
        Some(Op::Push {
            effect, finish, transform, ..
        }) => (*effect, finish.map_or(0.0, |finish| noise::room(&finish, *transform))),
        _ => (None, 0.0),
    };
    // A group that is turned may bring into view what lay outside it, so more of it is
    // drawn flat: up to a frame's width and height around the frame. A torn edge and a shadow
    // take from what lies just outside.
    let reach = match effect {
        Some(Effect::Turn(_)) => shown.inflate(shown.width(), shown.height()),
        _ => shown.inflate(room, room),
    };
    let flat = bounds(group)?.intersect(reach).expand();
    (flat.width() >= 1.0 && flat.height() >= 1.0).then_some((effect, flat))
}

/// Where a flat picture that covers `flat` of the frame is seen when it is turned by `matrix`,
/// inside `shown`, and the matrix that takes a pixel there back to its place on the flat
/// picture. Nothing when the picture cannot be drawn or seen.
pub fn turn_plan(flat: Rect, matrix: [[f64; 3]; 3], shown: Rect) -> Option<(Rect, [[f64; 3]; 3])> {
    let corners = [(flat.x0, flat.y0), (flat.x1, flat.y0), (flat.x1, flat.y1), (flat.x0, flat.y1)];
    let seen = corners.map(|(x, y)| through(matrix, x, y));
    // A corner at the eye or behind it: the picture cannot be drawn.
    if seen.iter().any(|(_, _, depth)| *depth < 0.05) {
        return None;
    }
    let bounds = seen
        .iter()
        .fold(Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN), |all, (x, y, _)| {
            Rect::new(all.x0.min(*x), all.y0.min(*y), all.x1.max(*x), all.y1.max(*y))
        });
    let bounds = bounds.intersect(shown).expand();
    if bounds.width() < 1.0 || bounds.height() < 1.0 {
        return None;
    }
    Some((bounds, inverse(matrix)?))
}

/// The part of the frame that `ops` can draw on, with room for their blurs.
pub fn bounds(ops: &[Op]) -> Option<Rect> {
    let mut all: Option<Rect> = None;
    // How far the blur of each open layer spreads what is drawn in it.
    let mut spread: Vec<f64> = Vec::new();
    for op in ops {
        let placed = match op {
            Op::Push {
                transform, blur, finish, ..
            } => {
                let [a, b, ..] = transform.as_coeffs();
                let room = finish.map_or(0.0, |finish| noise::room(&finish, *transform));
                spread.push(3.0 * f64::from(*blur) * a.hypot(b).max(1.0) + room);
                None
            }
            Op::Pop => {
                spread.pop();
                None
            }
            Op::Fill { transform, rect, .. } | Op::Still { transform, rect, .. } | Op::Video { transform, rect, .. } => {
                Some(transform.transform_rect_bbox(*rect))
            }
            Op::Border {
                transform, rect, width, ..
            } => Some(transform.transform_rect_bbox(rect.inflate(*width, *width))),
            Op::Shadow {
                transform,
                rect,
                deviation,
                ..
            } => Some(transform.transform_rect_bbox(rect.inflate(3.0 * deviation, 3.0 * deviation))),
            Op::Shape {
                transform,
                paths,
                stroke,
                shadow,
                ..
            } => {
                let mut around: Option<Rect> = None;
                for path in paths.iter() {
                    let own = path.bounding_box();
                    around = Some(around.map_or(own, |around| around.union(own)));
                }
                around.map(|around| {
                    let wide = stroke.unwrap_or(0.0);
                    let outline = around.inflate(wide, wide);
                    let shaded = shadow.map_or(outline, |shadow| {
                        let reach = 1.5 * shadow.blur + 1.0;
                        outline.union((outline + vello_cpu::kurbo::Vec2::new(shadow.x, shadow.y)).inflate(reach, reach))
                    });
                    transform.transform_rect_bbox(shaded)
                })
            }
            Op::Glyphs {
                transform, run, stroke, ..
            } => run_bounds(run).map(|around| {
                let wide = stroke.map_or(0.0, |(width, _)| width);
                transform.transform_rect_bbox(around.inflate(wide, wide))
            }),
            Op::TextShadow { transform, runs, node, .. } => {
                let reach = node
                    .text
                    .shadows
                    .iter()
                    .map(|s| s.blur * 1.5 + s.x.abs().max(s.y.abs()))
                    .fold(0.0, f64::max)
                    + 2.0;
                let mut around: Option<Rect> = None;
                for own in runs.iter().filter_map(run_bounds) {
                    around = Some(around.map_or(own, |around| around.union(own)));
                }
                around.map(|around| transform.transform_rect_bbox(around.inflate(reach, reach)))
            }
        };
        if let Some(placed) = placed {
            let margin: f64 = spread.iter().sum();
            let placed = placed.inflate(margin, margin);
            all = Some(all.map_or(placed, |all| all.union(placed)));
        }
    }
    all
}

impl Cpu {
    pub fn new(composition: &Composition, stills: &[Picture]) -> Cpu {
        let (width, height) = (composition.width, composition.height);
        Cpu {
            context: RenderContext::new_with(width, height, settings()),
            pixmap: Pixmap::new(width, height),
            background: composition.background,
            frame: Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            painter: Painter::new(stills),
        }
    }
}

impl Painter {
    pub fn new(stills: &[Picture]) -> Painter {
        Painter {
            resources: Resources::new(),
            videos: Vec::new(),
            stills: stills.iter().map(|picture| Arc::new(pixmap_of(picture))).collect(),
            shadows: HashMap::new(),
            tiles: HashMap::new(),
        }
    }

    /// The memory the next picture of `video` is decoded into.
    pub fn video_buffer(&mut self, video: usize, width: u32, height: u32) -> &mut [u8] {
        if self.videos.len() <= video {
            self.videos.resize_with(video + 1, || None);
        }
        let picture = self.videos[video].get_or_insert_with(|| {
            let mut pixmap = Pixmap::new(width as u16, height as u16);
            pixmap.set_may_have_transparency(false);
            Arc::new(pixmap)
        });
        // When the frame before has been rendered and let go of, this does not copy.
        Arc::make_mut(picture).data_as_u8_slice_mut()
    }

    pub fn release_video(&mut self, video: usize) {
        if let Some(slot) = self.videos.get_mut(video) {
            *slot = None;
        }
    }

    /// The picture of one tile of a repeated background.
    pub fn tile(&mut self, fill: &Paint, width: f64, height: f64, frame: u64) -> Arc<Pixmap> {
        let key = (std::ptr::from_ref::<Paint>(fill) as usize, width.to_bits(), height.to_bits());
        let (picture, used) = self
            .tiles
            .entry(key)
            .or_insert_with(|| (Arc::new(tile_picture(fill, width, height)), frame));
        *used = frame;
        Arc::clone(picture)
    }

    /// The picture of the shadows of some words, with the place of its corner in their space.
    pub fn text_shadow(
        &mut self,
        key: (usize, usize),
        runs: &[GlyphRun],
        shadows: &[Shadow],
        frame: u64,
    ) -> Option<(Arc<Pixmap>, (f64, f64))> {
        if !self.shadows.contains_key(&key) {
            let (pixmap, origin) = shadow_picture(runs, shadows, &mut self.resources)?;
            self.shadows.insert(key, (Arc::new(pixmap), origin, frame));
        }
        let (picture, origin, used) = self.shadows.get_mut(&key)?;
        *used = frame;
        Some((Arc::clone(picture), *origin))
    }

    /// Lets go of the pictures that have not been drawn for a while: they belong to elements
    /// that have left the frame.
    pub fn forget(&mut self, frame: u64) {
        if frame.is_multiple_of(256) {
            self.shadows.retain(|_, (_, _, used)| *used + 256 >= frame);
            self.tiles.retain(|_, (_, used)| *used + 256 >= frame);
        }
    }

    /// Draws `ops` into a picture of their own that shows `bounds` of the frame. With `alone`,
    /// the first step opens a layer that is drawn apart from the frame, so it cannot blend with
    /// what is under it: whoever puts the picture on the frame does that. The brightness of
    /// that layer is given to the picture here, and so are its torn edge, its uneven ink and
    /// its shadow, with its opacity after them.
    pub fn group(&mut self, ops: &[Op], bounds: Rect, frame: u64, alone: bool) -> Result<Pixmap, String> {
        let (width, height) = (bounds.width().clamp(1.0, 8192.0) as u16, bounds.height().clamp(1.0, 8192.0) as u16);
        let mut context = RenderContext::new_with(width, height, settings());
        self.paint(&mut context, Affine::translate((-bounds.x0, -bounds.y0)), ops, frame, alone)?;
        context.flush();
        let mut pixmap = Pixmap::new(width, height);
        context.render(&mut pixmap, &mut self.resources);
        if let (
            true,
            Some(Op::Push {
                transform,
                opacity,
                brightness,
                finish,
                ..
            }),
        ) = (alone, ops.first())
        {
            if *brightness != 1.0 {
                brighten(&mut pixmap, *brightness);
            }
            if let Some(finish) = finish {
                let corner = Point::new(bounds.x0, bounds.y0);
                noise::finish(pixmap.data_as_u8_slice_mut(), usize::from(width), corner, *transform, finish);
                if *opacity < 1.0 {
                    fade(&mut pixmap, *opacity);
                }
            }
        }
        Ok(pixmap)
    }

    /// Draws a group into a picture of its own and gives the picture the group's effect.
    /// `shown` is the part of the frame that can be seen. Returns the picture and the part of
    /// the frame it covers, or nothing when none of it can be seen.
    pub fn apart(&mut self, group: &[Op], shown: Rect, frame: u64, alone: bool) -> Result<Option<(Pixmap, Rect)>, String> {
        let Some((effect, flat)) = flat_of(group, shown) else {
            return Ok(None);
        };
        if let Some(Effect::Reveal {
            opening: Opening::Shader(_),
            ..
        }) = effect
        {
            return Err(
                "an element opens with a transition written as a shader: only the GPU renderer draws that, and not on an element that is itself blurred or brightened"
                    .to_string(),
            );
        }
        let picture = self.group(group, flat, frame, alone)?;
        Ok(match effect {
            None => Some((picture, flat)),
            Some(Effect::Turn(matrix)) => {
                turn_plan(flat, matrix, shown).map(|(bounds, back)| (turned(&picture, flat, back, bounds), bounds))
            }
            Some(Effect::Reveal { opening, progress, around }) => Some((opened(picture, flat, opening, progress, around), flat)),
        })
    }

    fn set_brush(context: &mut RenderContext, brush: Brush) {
        match brush {
            Brush::Solid(color) => context.set_paint(color),
            Brush::Gradient(gradient, placed) => {
                context.set_paint(gradient);
                context.set_paint_transform(placed);
            }
        }
    }

    /// Fills `rect`, with rounded corners when `radius` is above 0, with the paint in place.
    fn fill(context: &mut RenderContext, rect: Rect, radius: f64) {
        if radius > 0.0 {
            context.fill_path(&RoundedRect::from_rect(rect, radius).to_path(0.1));
        } else {
            context.fill_rect(&rect);
        }
    }
}

impl Painter {
    /// Draws `ops` into `context`, every one moved by `base`.
    pub fn paint(&mut self, context: &mut RenderContext, base: Affine, ops: &[Op], frame: u64, alone: bool) -> Result<(), String> {
        // The step to go on with after a group that was drawn as a picture of its own.
        let mut resume = 0;
        for (index, op) in ops.iter().enumerate() {
            if index < resume {
                continue;
            }
            match op {
                // A brightened group is drawn apart, so that the colours of its finished picture
                // can be multiplied, and is put on the frame as that picture.
                Op::Push {
                    blend,
                    brightness,
                    effect,
                    finish,
                    ..
                } if (*brightness != 1.0 || effect.is_some() || finish.is_some()) && !(alone && index == 0) => {
                    let end = closing(ops, index);
                    resume = end + 1;
                    let shown =
                        base.inverse()
                            .transform_rect_bbox(Rect::new(0.0, 0.0, f64::from(context.width()), f64::from(context.height())));
                    let Some((picture, bounds)) = self.apart(&ops[index..=end], shown, frame, true)? else {
                        continue;
                    };
                    let picture = Arc::new(picture);
                    let whole = Rect::new(0.0, 0.0, f64::from(picture.width()), f64::from(picture.height()));
                    context.set_transform(base * Affine::translate((bounds.x0, bounds.y0)));
                    context.set_paint(image(&picture, ImageQuality::Low, 1.0));
                    context.push_layer(None, Some(BlendMode::new(mix(*blend), Compose::SrcOver)), None, None, None);
                    context.fill_rect(&whole);
                    context.pop_layer();
                }
                Op::Push {
                    transform,
                    clip,
                    blend,
                    opacity,
                    blur,
                    finish,
                    ..
                } => {
                    // The opacity of a picture with a finish comes after the finish.
                    let opacity = if finish.is_some() { 1.0 } else { *opacity };
                    context.set_transform(base * *transform);
                    let clip = clip.as_ref().map(|clip| match clip {
                        Clip::Rounded(rect, radius) => RoundedRect::from_rect(*rect, *radius).to_path(0.1),
                        Clip::Outline(outline) => outline.clone(),
                    });
                    let mix = if alone && index == 0 { Mix::Normal } else { mix(*blend) };
                    let filter = (*blur > 0.0).then(|| paint::blur(*blur));
                    context.push_layer(
                        clip.as_ref(),
                        Some(BlendMode::new(mix, Compose::SrcOver)),
                        Some(opacity),
                        None,
                        filter,
                    );
                }
                Op::Pop => context.pop_layer(),
                Op::Fill {
                    transform,
                    rect,
                    radius,
                    paint: fill,
                    tile,
                    opacity,
                } => {
                    context.set_transform(base * *transform);
                    match tile {
                        // One tile laid side by side: its picture, repeated.
                        Some(tile) if tile.repeat => {
                            let (width, height) = (tile.rect.width(), tile.rect.height());
                            let picture = self.tile(fill, width, height, frame);
                            let placed = Affine::translate((tile.rect.x0, tile.rect.y0))
                                * Affine::scale_non_uniform(width / f64::from(picture.width()), height / f64::from(picture.height()));
                            context.set_paint(sampled(&picture, quality(base * *transform * placed), *opacity, Extend::Repeat));
                            context.set_paint_transform(placed);
                            Painter::fill(context, *rect, *radius);
                        }
                        // One tile drawn once: the paint of the tile, inside the box.
                        Some(tile) => {
                            let shown = tile.rect.intersect(*rect);
                            if shown.width() <= 0.0 || shown.height() <= 0.0 {
                                continue;
                            }
                            Painter::set_brush(context, paint::brush(fill, tile.rect, *opacity));
                            if *radius > 0.0 {
                                let corners = RoundedRect::from_rect(*rect, *radius).to_path(0.1);
                                context.push_layer(Some(&corners), None, None, None, None);
                                context.fill_rect(&shown);
                                context.pop_layer();
                            } else {
                                context.fill_rect(&shown);
                            }
                        }
                        None => {
                            Painter::set_brush(context, paint::brush(fill, *rect, *opacity));
                            Painter::fill(context, *rect, *radius);
                        }
                    }
                    context.reset_paint_transform();
                }
                Op::Shadow {
                    transform,
                    rect,
                    radius,
                    deviation,
                    color,
                    opacity,
                } => {
                    context.set_transform(base * *transform);
                    context.set_paint(paint::color(*color, *opacity));
                    context.fill_blurred_rounded_rect(rect, *radius as f32, *deviation as f32, false);
                }
                Op::Border {
                    transform,
                    rect,
                    radius,
                    width,
                    color,
                    opacity,
                } => {
                    context.set_transform(base * *transform);
                    context.set_paint(paint::color(*color, *opacity));
                    context.set_stroke(Stroke::new(*width));
                    let middle = RoundedRect::from_rect(rect.inset(-width / 2.0), (radius - width / 2.0).max(0.0));
                    context.stroke_path(&middle.to_path(0.1));
                }
                Op::Shape {
                    transform,
                    paths,
                    stroke,
                    color,
                    shadow,
                    opacity,
                } => {
                    if let Some(width) = stroke {
                        context.set_stroke(Stroke::new(*width).with_caps(Cap::Round).with_join(Join::Round));
                    }
                    let outlines = |context: &mut RenderContext| {
                        for path in paths.iter() {
                            match stroke {
                                Some(_) => context.stroke_path(path),
                                None => context.fill_path(path),
                            }
                        }
                    };
                    if let Some(shadow) = shadow {
                        // The same outlines, moved, blurred and in the shadow's colour. The blur is
                        // given in the outlines' units and made in the frame's pixels.
                        let [a, b, ..] = transform.as_coeffs();
                        let deviation = (shadow.blur / 2.0 * a.hypot(b)) as f32;
                        context.set_transform(base * *transform * Affine::translate((shadow.x, shadow.y)));
                        context.set_paint(paint::color(shadow.color, *opacity));
                        if deviation > 0.0 {
                            context.push_filter_layer(paint::blur(deviation));
                        }
                        outlines(context);
                        if deviation > 0.0 {
                            context.pop_layer();
                        }
                    }
                    context.set_transform(base * *transform);
                    context.set_paint(paint::color(*color, *opacity));
                    outlines(context);
                }
                Op::TextShadow {
                    transform,
                    key,
                    runs,
                    node,
                    opacity,
                } => {
                    let Some((picture, origin)) = self.text_shadow(*key, runs, &node.text.shadows, frame) else {
                        continue;
                    };
                    let placed = base * *transform * Affine::translate(origin);
                    context.set_transform(placed);
                    context.set_paint(image(&picture, quality(placed), *opacity));
                    context.fill_rect(&Rect::new(0.0, 0.0, f64::from(picture.width()), f64::from(picture.height())));
                }
                Op::Glyphs {
                    transform,
                    run,
                    color,
                    stroke,
                    opacity,
                } => {
                    context.set_transform(base * *transform);
                    context.set_paint(paint::color(*color, *opacity));
                    let drawn = glyphs(context, &mut self.resources, run, false);
                    drawn.map_err(|error| format!("text could not be drawn: {error}"))?;
                    if let Some((width, color)) = stroke {
                        context.set_paint(paint::color(*color, *opacity));
                        context.set_stroke(Stroke::new(*width).with_join(Join::Miter));
                        let drawn = glyphs(context, &mut self.resources, run, true);
                        drawn.map_err(|error| format!("text could not be outlined: {error}"))?;
                    }
                }
                Op::Still {
                    transform,
                    still,
                    rect,
                    radius,
                    opacity,
                    fit,
                } => {
                    let Some(picture) = self.stills.get(*still) else { continue };
                    let (placed, rect) = paint::fit(*rect, f64::from(picture.width()), f64::from(picture.height()), *fit);
                    context.set_transform(base * *transform);
                    context.set_paint(image(picture, quality(base * *transform * placed), *opacity));
                    context.set_paint_transform(placed);
                    Painter::fill(context, rect, *radius);
                    context.reset_paint_transform();
                }
                Op::Video {
                    transform,
                    video,
                    rect,
                    radius,
                    opacity,
                } => {
                    let Some(Some(picture)) = self.videos.get(*video) else { continue };
                    let placed = paint::fit(*rect, f64::from(picture.width()), f64::from(picture.height()), Fit::FILL).0;
                    context.set_transform(base * *transform);
                    context.set_paint(image(picture, quality(base * *transform * placed), *opacity));
                    context.set_paint_transform(placed);
                    Painter::fill(context, *rect, *radius);
                    context.reset_paint_transform();
                }
            }
        }
        Ok(())
    }
}

impl Backend for Cpu {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn video_buffer(&mut self, video: usize, width: u32, height: u32) -> &mut [u8] {
        self.painter.video_buffer(video, width, height)
    }

    fn release_video(&mut self, video: usize) {
        self.painter.release_video(video);
    }

    fn draw(&mut self, frame: u64, ops: &[Op]) -> Result<(), String> {
        let context = &mut self.context;
        context.reset();
        context.set_paint(paint::color(self.background, 1.0));
        context.fill_rect(&self.frame);
        self.painter.paint(context, Affine::IDENTITY, ops, frame, false)?;
        context.flush();
        self.painter.forget(frame);
        Ok(())
    }

    fn pixels(&mut self) -> Result<&[u8], String> {
        self.context.render(&mut self.pixmap, &mut self.painter.resources);
        // Lets go of the pictures the frame used, so the next one can be decoded in place.
        self.context.reset();
        Ok(self.pixmap.data_as_u8_slice())
    }
}

/// Applies a 3x3 matrix to a point: the first two results divided by the third, and the third.
fn through(matrix: [[f64; 3]; 3], x: f64, y: f64) -> (f64, f64, f64) {
    let at = |row: usize| matrix[row][0] * x + matrix[row][1] * y + matrix[row][2];
    let depth = at(2);
    (at(0) / depth, at(1) / depth, depth)
}

/// The inverse of a 3x3 matrix, when it has one.
fn inverse(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let minor = |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let cofactors = [
        [minor(1, 2, 1, 2), -minor(1, 2, 0, 2), minor(1, 2, 0, 1)],
        [-minor(0, 2, 1, 2), minor(0, 2, 0, 2), -minor(0, 2, 0, 1)],
        [minor(0, 1, 1, 2), -minor(0, 1, 0, 2), minor(0, 1, 0, 1)],
    ];
    let determinant: f64 = (0..3).map(|column| m[0][column] * cofactors[0][column]).sum();
    (determinant.abs() > 1e-12).then(|| std::array::from_fn(|row| std::array::from_fn(|column| cofactors[column][row] / determinant)))
}

/// The flat picture of a group, which covers `flat` of the frame, turned in space: the part
/// `bounds` of the frame, where `back` takes a pixel to its place on the flat picture. Each
/// pixel is taken from between the four pixels of the flat picture around its place.
fn turned(picture: &Pixmap, flat: Rect, back: [[f64; 3]; 3], bounds: Rect) -> Pixmap {
    let (width, height) = (bounds.width() as usize, bounds.height() as usize);
    let (from_width, from_height) = (i64::from(picture.width()), i64::from(picture.height()));
    let from = picture.data_as_u8_slice();
    let pixel = |x: i64, y: i64| -> [f64; 4] {
        if x < 0 || y < 0 || x >= from_width || y >= from_height {
            return [0.0; 4];
        }
        let at = (y * from_width + x) as usize * 4;
        [
            f64::from(from[at]),
            f64::from(from[at + 1]),
            f64::from(from[at + 2]),
            f64::from(from[at + 3]),
        ]
    };
    let mut out = Pixmap::new(width as u16, height as u16);
    let rows = |first: usize, part: &mut [u8]| {
        for (row, line) in part.chunks_exact_mut(width * 4).enumerate() {
            let y = bounds.y0 + (first + row) as f64 + 0.5;
            for (column, target) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let x = bounds.x0 + column as f64 + 0.5;
                // Where this pixel's middle lies on the flat picture, counted in its pixels.
                let (u, v, _) = through(back, x, y);
                let (u, v) = (u - flat.x0 - 0.5, v - flat.y0 - 0.5);
                if u <= -1.0 || v <= -1.0 || u >= from_width as f64 || v >= from_height as f64 {
                    continue;
                }
                let (left, top) = (u.floor(), v.floor());
                let (across, down) = (u - left, v - top);
                let (left, top) = (left as i64, top as i64);
                let (a, b, c, d) = (
                    pixel(left, top),
                    pixel(left + 1, top),
                    pixel(left, top + 1),
                    pixel(left + 1, top + 1),
                );
                for channel in 0..4 {
                    let upper = a[channel] + (b[channel] - a[channel]) * across;
                    let lower = c[channel] + (d[channel] - c[channel]) * across;
                    target[channel] = (upper + (lower - upper) * down).round() as u8;
                }
            }
        }
    };
    // The rows are shared out over a few threads when there are enough of them.
    let threads = (height / 64).clamp(1, 4);
    let band = height.div_ceil(threads);
    let data = out.data_as_u8_slice_mut();
    if threads == 1 {
        rows(0, data);
    } else {
        std::thread::scope(|scope| {
            for (part, band_rows) in data.chunks_mut(band * width * 4).enumerate() {
                let rows = &rows;
                scope.spawn(move || rows(part * band, band_rows));
            }
        });
    }
    out
}

/// The share of a pixel that shows through an opening that is `progress` open, where the
/// pixel lies `place` of the way (0 to 1) from where the opening begins to where it ends. The
/// edge is a tenth wide.
fn open_at(progress: f64, place: f64) -> f64 {
    const EDGE: f64 = 0.1;
    ((progress * (1.0 + EDGE) - place) / EDGE).clamp(0.0, 1.0)
}

/// The picture of a group, which covers `flat` of the frame, shown through an opening over
/// the element's box `around`.
fn opened(mut picture: Pixmap, flat: Rect, opening: Opening, progress: f64, around: Rect) -> Pixmap {
    let width = usize::from(picture.width());
    let centre = around.center();
    let reach = (around.width() / 2.0).hypot(around.height() / 2.0).max(1e-9);
    for (row, line) in picture.data_as_u8_slice_mut().chunks_exact_mut(width * 4).enumerate() {
        let y = flat.y0 + row as f64 + 0.5;
        for (column, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let x = flat.x0 + column as f64 + 0.5;
            let place = match opening {
                Opening::Wipe => (x - around.x0) / around.width().max(1e-9),
                Opening::Iris => (x - centre.x).hypot(y - centre.y) / reach,
                Opening::Cut => unreachable!("a hard edge is a cut of the box, not an opening"),
                Opening::Shader(_) => unreachable!("a shader is not opened in memory"),
            };
            let share = open_at(progress, place);
            if share < 1.0 {
                for channel in pixel.iter_mut() {
                    *channel = (f64::from(*channel) * share).round() as u8;
                }
            }
        }
    }
    picture
}
