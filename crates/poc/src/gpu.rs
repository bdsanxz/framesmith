//! The GPU backend: draws a frame into a texture and reads it back into memory.
//!
//! The graphics card draws the fills, gradients, pictures, footage, text and layers of a frame.
//! It is not given what its drawing library cannot do: a group that is blurred, and a shadow
//! that follows an outline. The CPU painter draws each of those into a small picture, and the
//! card puts the picture in its place.

use crate::cpu::{self, Painter};
use crate::decode::Picture;
use crate::gpu_effects::{Effects, Params, Pass, Slot};
use crate::gpu_footage::{self, Converter, Outgoing, Planes};
use crate::model::{Blend, Composition, Fit, GlyphRun};
use crate::paint::{self, Brush};
use crate::scene::{closing, Clip, Effect, Op};
use crate::style::{ColorOp, Grade};
use crate::yuv;
use crate::Backend;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use vello::kurbo::{Affine, Cap, Join, Rect, RoundedRect, Stroke};
use vello::peniko::{
    BlendMode, Blob, Compose, Extend, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality, ImageSampler, Mix,
};
use vello::wgpu;
use vello::{AaConfig, AaSupport, Glyph, RenderParams, Renderer, RendererOptions, Scene};
use vello_cpu::Pixmap;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    scene: Scene,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Where the texture is copied to be read; each row is padded to the size the copy asks for.
    /// There is one for each frame that may be on its way at the same time.
    staging: Vec<wgpu::Buffer>,
    /// The buffers of the frames drawn and not yet read, oldest first, each with the work the
    /// card must finish before it can be read.
    waiting: VecDeque<(usize, wgpu::SubmissionIndex)>,
    /// The buffers no frame is in.
    spare: Vec<usize>,
    row_bytes: u32,
    padded_row_bytes: u32,
    pixels: Vec<u8>,
    params: RenderParams,
    frame: Rect,
    adapter: String,
    /// Draws what the card is not given, and holds the pictures in memory.
    painter: Painter,
    stills: Vec<ImageData>,
    videos: Vec<Option<Video>>,
    /// The pictures the painter keeps (tiles, text shadows), as the card knows them.
    kept: HashMap<usize, (Arc<Pixmap>, ImageData)>,
    /// Turns footage that comes as planes into RGBA on the card.
    converter: Converter,
    /// Turns finished frames into planes on the card, once the frames are wanted so.
    outgoing: Option<Outgoing>,
    /// The buffers the planes are copied to, in place of `staging`.
    plane_staging: Vec<wgpu::Buffer>,
    /// Turns and opens groups on the card.
    effects: Effects,
    /// The textures of the groups with an effect, in the order the groups come in a frame,
    /// each with its result as the drawing library knows it.
    slots: Vec<(Slot, ImageData)>,
    /// How many of them this frame has used.
    used: usize,
    /// The part of the frame a layer without a cut may draw on: the frame, or the flat picture
    /// of the group being drawn.
    reach: Rect,
    /// The flat picture of a group, moved to its texture's corner.
    moved: Scene,
}

/// How many frames may be on their way through the card at the same time: one being drawn
/// there while the next is described here.
const AHEAD: usize = 2;

/// The texture the current picture of one video is sent to.
struct Video {
    texture: wgpu::Texture,
    image: ImageData,
    width: u32,
    height: u32,
    changed: bool,
    /// The picture as YUV planes, when the card makes the RGBA itself.
    planes: Option<Planes>,
    /// Whether the painter's own RGBA copy is of the picture now in the planes.
    painted: bool,
}

/// The bytes of a picture in memory, for the card to read without a copy.
struct Bytes(Arc<Pixmap>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.0.data_as_u8_slice()
    }
}

/// A picture in memory as the card's drawing library takes it. Its colours are already
/// multiplied by their alpha.
fn image_data(picture: &Arc<Pixmap>) -> ImageData {
    ImageData {
        data: Blob::new(Arc::new(Bytes(Arc::clone(picture)))),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width: u32::from(picture.width()),
        height: u32::from(picture.height()),
    }
}

fn brush_of(image: &ImageData, quality: ImageQuality, alpha: f32, extend: Extend) -> ImageBrush {
    ImageBrush {
        image: image.clone(),
        sampler: ImageSampler {
            x_extend: extend,
            y_extend: extend,
            quality,
            alpha,
        },
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

fn mode(blend: Blend) -> BlendMode {
    let mix = match blend {
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
    };
    BlendMode::new(mix, Compose::SrcOver)
}

impl Gpu {
    pub fn new(composition: &Composition, stills: &[Picture]) -> Result<Gpu, String> {
        let (width, height) = (u32::from(composition.width), u32::from(composition.height));
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|error| format!("no graphics adapter was found: {error}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|error| format!("the graphics device could not be opened: {error}"))?;
        let options = RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        };
        let renderer = Renderer::new(&device, options).map_err(|error| format!("the GPU renderer could not start: {error}"))?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let row_bytes = width * 4;
        let padded_row_bytes = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let staging: Vec<wgpu::Buffer> = (0..AHEAD)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("frame read"),
                    size: u64::from(padded_row_bytes) * u64::from(height),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let painter = Painter::new(stills);
        let converter = Converter::new(&device);
        let effects = Effects::new(&device, &composition.shaders)?;
        Ok(Gpu {
            adapter: adapter.get_info().name,
            stills: painter.stills.iter().map(image_data).collect(),
            painter,
            videos: Vec::new(),
            kept: HashMap::new(),
            device,
            queue,
            converter,
            outgoing: None,
            plane_staging: Vec::new(),
            effects,
            slots: Vec::new(),
            used: 0,
            reach: Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            moved: Scene::new(),
            renderer,
            scene: Scene::new(),
            texture,
            view,
            staging,
            waiting: VecDeque::new(),
            spare: (0..AHEAD).collect(),
            row_bytes,
            padded_row_bytes,
            pixels: vec![0; (row_bytes * height) as usize],
            frame: Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            params: RenderParams {
                base_color: paint::color(composition.background, 1.0),
                width,
                height,
                antialiasing_method: AaConfig::Area,
            },
        })
    }

    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// A picture the painter keeps, as the card knows it: the same picture is sent once.
    fn kept(&mut self, picture: &Arc<Pixmap>) -> ImageData {
        let key = Arc::as_ptr(picture) as usize;
        let (_, image) = self.kept.entry(key).or_insert_with(|| (Arc::clone(picture), image_data(picture)));
        image.clone()
    }

    /// Fills `rect`, with rounded corners when `radius` is above 0.
    fn fill(scene: &mut Scene, transform: Affine, brush: &Brush, rect: Rect, radius: f64) {
        let (solid, gradient);
        let (brush, placed): (vello::peniko::BrushRef, Option<Affine>) = match brush {
            Brush::Solid(color) => {
                solid = *color;
                (solid.into(), None)
            }
            Brush::Gradient(colors, placed) => {
                gradient = colors;
                (gradient.into(), Some(*placed))
            }
        };
        if radius > 0.0 {
            scene.fill(Fill::NonZero, transform, brush, placed, &RoundedRect::from_rect(rect, radius));
        } else {
            scene.fill(Fill::NonZero, transform, brush, placed, &rect);
        }
    }

    /// Fills `rect` with a picture that lies at `placed` in the element's space.
    fn picture(scene: &mut Scene, transform: Affine, brush: &ImageBrush, placed: Affine, rect: Rect, radius: f64) {
        if radius > 0.0 {
            scene.fill(Fill::NonZero, transform, brush, Some(placed), &RoundedRect::from_rect(rect, radius));
        } else {
            scene.fill(Fill::NonZero, transform, brush, Some(placed), &rect);
        }
    }

    fn glyphs(scene: &mut Scene, transform: Affine, run: &GlyphRun, color: vello::peniko::Color, stroke: Option<&Stroke>) {
        let placed = run.glyphs.iter().map(|g| Glyph { id: g.id, x: g.x, y: g.y });
        // The slant of an italic the font does not have. Here the transform works on the outline
        // while it still points up, so a slant to the right is a positive skew.
        let slant = run.skew.map(|skew| Affine::skew(skew, 0.0));
        let builder = scene
            .draw_glyphs(&run.font)
            .font_size(run.size)
            .hint(false)
            .normalized_coords(bytemuck_coords(&run.coords))
            .transform(transform)
            .glyph_transform(slant)
            .brush(color);
        match stroke {
            Some(stroke) => builder.draw(stroke, placed),
            None => builder.draw(Fill::NonZero, placed),
        }
    }

    /// Has the CPU painter draw `ops` into a picture of their own, and puts the picture on the
    /// frame. `blend` is how the picture meets what is under it.
    fn by_painter(&mut self, ops: &[Op], blend: Blend, alone: bool, frame: u64) -> Result<(), String> {
        // Footage the card has only as planes is made RGBA for the painter here, once for each
        // picture: the painter draws from memory.
        for op in ops {
            let Op::Video { video, .. } = op else { continue };
            let Some(Some(slot)) = self.videos.get_mut(*video) else { continue };
            let Some(planes) = slot.planes.as_ref().filter(|_| !slot.painted) else {
                continue;
            };
            let (width, height) = (slot.width as usize, slot.height as usize);
            let grade = Grade::new(&planes.filter);
            let buffer = self.painter.video_buffer(*video, slot.width, slot.height);
            yuv::to_rgba(&planes.data, width, height, buffer, |row| grade.apply(row));
            slot.painted = true;
        }
        let Some((picture, bounds)) = self.painter.apart(ops, self.reach, frame, alone)? else {
            return Ok(());
        };
        let picture = Arc::new(picture);
        let brush = brush_of(&image_data(&picture), ImageQuality::Low, 1.0, Extend::Pad);
        let placed = Affine::translate((bounds.x0, bounds.y0));
        if blend == Blend::Normal {
            self.scene.draw_image(&brush, placed);
        } else {
            self.scene.push_layer(Fill::NonZero, mode(blend), 1.0, Affine::IDENTITY, &bounds);
            self.scene.draw_image(&brush, placed);
            self.scene.pop_layer();
        }
        Ok(())
    }

    /// Opens a layer for what follows: cut, blended and faded as the step says.
    fn open(&mut self, transform: Affine, clip: &Option<Clip>, blend: Blend, opacity: f32) {
        match clip {
            Some(Clip::Rounded(rect, radius)) => {
                let shape = RoundedRect::from_rect(*rect, *radius);
                self.scene.push_layer(Fill::NonZero, mode(blend), opacity, transform, &shape);
            }
            Some(Clip::Outline(outline)) => {
                self.scene.push_layer(Fill::NonZero, mode(blend), opacity, transform, outline);
            }
            None => {
                self.scene
                    .push_layer(Fill::NonZero, mode(blend), opacity, Affine::IDENTITY, &self.reach);
            }
        }
    }

    /// Draws a group that is turned in space or opens, on the card: the group flat into a
    /// texture of its own, that texture through the effect's pass, and the result on the
    /// frame. `ops` are the group's steps, from the one that opens it to the one that closes
    /// it. Returns whether it was drawn so; a group whose flat picture the card cannot hold
    /// is left to the painter.
    fn on_card(&mut self, ops: &[Op], blend: Blend, frame: u64) -> Result<bool, String> {
        let Some(Op::Push {
            transform, clip, opacity, ..
        }) = ops.first()
        else {
            return Ok(false);
        };
        // Nothing of the group can be seen: nothing is drawn.
        let Some((Some(effect), flat)) = cpu::flat_of(ops, self.frame) else {
            return Ok(true);
        };
        let most = f64::from(self.device.limits().max_texture_dimension_2d);
        if flat.width() > most || flat.height() > most {
            return Ok(false);
        }
        let turn: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let (pass, part, mut params) = match effect {
            Effect::Turn(matrix) => {
                let Some((bounds, back)) = cpu::turn_plan(flat, matrix, self.frame) else {
                    return Ok(true);
                };
                let params = Params {
                    back,
                    around: flat,
                    origin: (0.0, 0.0),
                    progress: 1.0,
                };
                (Pass::Turn, bounds, params)
            }
            Effect::Reveal { opening, progress, around } => {
                let params = Params {
                    back: turn,
                    around,
                    origin: (0.0, 0.0),
                    progress,
                };
                (Pass::Opening(opening), flat, params)
            }
        };
        params.origin = (flat.x0, flat.y0);
        // The group, flat, in a scene of its own. Its own blend waits for the frame.
        let (outer, reach) = (std::mem::replace(&mut self.scene, Scene::new()), self.reach);
        self.reach = flat;
        self.open(*transform, clip, Blend::Normal, *opacity);
        let walked = self.walk(&ops[1..ops.len() - 1], frame);
        self.scene.pop_layer();
        let inner = std::mem::replace(&mut self.scene, outer);
        self.reach = reach;
        walked?;
        self.moved.reset();
        self.moved.append(&inner, Some(Affine::translate((-flat.x0, -flat.y0))));
        if self.slots.len() <= self.used {
            let slot = self.effects.slot(&self.device, (self.params.width, self.params.height));
            let image = self.renderer.register_texture(slot.out.clone());
            self.slots.push((slot, image));
        }
        let (slot, image) = &mut self.slots[self.used];
        self.used += 1;
        let (view, (width, height)) = self.effects.flat(&self.device, slot, (flat.width() as u32, flat.height() as u32));
        let flat_params = RenderParams {
            base_color: paint::color([0, 0, 0, 0], 1.0),
            width,
            height,
            antialiasing_method: AaConfig::Area,
        };
        self.renderer
            .render_to_texture(&self.device, &self.queue, &self.moved, view, &flat_params)
            .map_err(|error| format!("the GPU could not draw a group: {error}"))?;
        let part = part.intersect(self.frame);
        let within = (part.x0 as u32, part.y0 as u32, part.width() as u32, part.height() as u32);
        if within.2 == 0 || within.3 == 0 {
            return Ok(true);
        }
        self.effects.run(&self.device, &self.queue, slot, pass, &params, within);
        self.renderer.mark_override_image_dirty(image);
        // The result is as large as the frame and lies on it pixel for pixel.
        let brush = brush_of(image, ImageQuality::Low, 1.0, Extend::Pad);
        if blend == Blend::Normal {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &part);
        } else {
            self.scene.push_layer(Fill::NonZero, mode(blend), 1.0, Affine::IDENTITY, &part);
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &part);
            self.scene.pop_layer();
        }
        Ok(true)
    }
}

/// The variable-font coordinates of a run, as the card's drawing library names them.
fn bytemuck_coords(coords: &[i16]) -> &[vello::NormalizedCoord] {
    coords
}

impl Gpu {
    /// The texture of footage `video`, made when it is first asked for.
    fn video(&mut self, video: usize, width: u32, height: u32) -> &mut Video {
        if self.videos.len() <= video {
            self.videos.resize_with(video + 1, || None);
        }
        if self.videos[video].is_none() {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("footage"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let image = self.renderer.register_texture(texture.clone());
            self.videos[video] = Some(Video {
                texture,
                image,
                width,
                height,
                changed: false,
                planes: None,
                painted: false,
            });
        }
        self.videos[video].as_mut().expect("the video was just made")
    }
}

impl Gpu {
    /// Adds what `ops` draw to the scene being described.
    fn walk(&mut self, ops: &[Op], frame: u64) -> Result<(), String> {
        let mut index = 0;
        while index < ops.len() {
            match &ops[index] {
                // A group that is turned or opens goes through a pass on the card. A blurred
                // or brightened group is the painter's, and so is one the card cannot hold.
                Op::Push {
                    blend,
                    blur,
                    brightness,
                    effect,
                    finish,
                    ..
                } if *blur > 0.0 || *brightness != 1.0 || effect.is_some() || finish.is_some() => {
                    let end = closing(ops, index);
                    let group = &ops[index..=end];
                    let plain = *blur == 0.0 && *brightness == 1.0 && finish.is_none();
                    if !(plain && effect.is_some() && self.on_card(group, *blend, frame)?) {
                        self.by_painter(group, *blend, true, frame)?;
                    }
                    index = end;
                }
                Op::Push {
                    transform,
                    clip,
                    blend,
                    opacity,
                    ..
                } => self.open(*transform, clip, *blend, *opacity),
                Op::Pop => self.scene.pop_layer(),
                Op::Fill {
                    transform,
                    rect,
                    radius,
                    paint: fill,
                    tile,
                    opacity,
                } => match tile {
                    Some(tile) if tile.repeat => {
                        let (width, height) = (tile.rect.width(), tile.rect.height());
                        let picture = self.painter.tile(fill, width, height, frame);
                        let placed = Affine::translate((tile.rect.x0, tile.rect.y0))
                            * Affine::scale_non_uniform(width / f64::from(picture.width()), height / f64::from(picture.height()));
                        let image = self.kept(&picture);
                        let brush = brush_of(&image, quality(*transform * placed), *opacity, Extend::Repeat);
                        Gpu::picture(&mut self.scene, *transform, &brush, placed, *rect, *radius);
                    }
                    Some(tile) => {
                        let shown = tile.rect.intersect(*rect);
                        if shown.width() > 0.0 && shown.height() > 0.0 {
                            let brush = paint::brush(fill, tile.rect, *opacity);
                            if *radius > 0.0 {
                                let corners = RoundedRect::from_rect(*rect, *radius);
                                self.scene.push_clip_layer(Fill::NonZero, *transform, &corners);
                                Gpu::fill(&mut self.scene, *transform, &brush, shown, 0.0);
                                self.scene.pop_layer();
                            } else {
                                Gpu::fill(&mut self.scene, *transform, &brush, shown, 0.0);
                            }
                        }
                    }
                    None => {
                        let brush = paint::brush(fill, *rect, *opacity);
                        Gpu::fill(&mut self.scene, *transform, &brush, *rect, *radius);
                    }
                },
                Op::Shadow {
                    transform,
                    rect,
                    radius,
                    deviation,
                    color,
                    opacity,
                } => {
                    self.scene
                        .draw_blurred_rounded_rect(*transform, *rect, paint::color(*color, *opacity), *radius, *deviation);
                }
                Op::Border {
                    transform,
                    rect,
                    radius,
                    width,
                    color,
                    opacity,
                } => {
                    let middle = RoundedRect::from_rect(rect.inset(-width / 2.0), (radius - width / 2.0).max(0.0));
                    self.scene
                        .stroke(&Stroke::new(*width), *transform, paint::color(*color, *opacity), None, &middle);
                }
                // A shadow that follows the outlines is a blur: the painter draws the shape.
                Op::Shape { shadow: Some(_), .. } => self.by_painter(&ops[index..=index], Blend::Normal, false, frame)?,
                Op::Shape {
                    transform,
                    paths,
                    stroke,
                    color,
                    opacity,
                    ..
                } => {
                    let brush = paint::color(*color, *opacity);
                    for path in paths.iter() {
                        match stroke {
                            Some(width) => {
                                let style = Stroke::new(*width).with_caps(Cap::Round).with_join(Join::Round);
                                self.scene.stroke(&style, *transform, brush, None, path);
                            }
                            None => self.scene.fill(Fill::NonZero, *transform, brush, None, path),
                        }
                    }
                }
                Op::TextShadow {
                    transform,
                    key,
                    runs,
                    node,
                    opacity,
                } => {
                    if let Some((picture, origin)) = self.painter.text_shadow(*key, runs, &node.text.shadows, frame) {
                        let placed = *transform * Affine::translate(origin);
                        let image = self.kept(&picture);
                        let brush = brush_of(&image, quality(placed), *opacity, Extend::Pad);
                        self.scene.draw_image(&brush, placed);
                    }
                }
                Op::Glyphs {
                    transform,
                    run,
                    color,
                    stroke,
                    opacity,
                } => {
                    Gpu::glyphs(&mut self.scene, *transform, run, paint::color(*color, *opacity), None);
                    if let Some((width, color)) = stroke {
                        let style = Stroke::new(*width).with_join(Join::Miter);
                        Gpu::glyphs(&mut self.scene, *transform, run, paint::color(*color, *opacity), Some(&style));
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
                    if let Some(image) = self.stills.get(*still) {
                        let (placed, rect) = paint::fit(*rect, f64::from(image.width), f64::from(image.height), *fit);
                        let brush = brush_of(image, quality(*transform * placed), *opacity, Extend::Pad);
                        Gpu::picture(&mut self.scene, *transform, &brush, placed, rect, *radius);
                    }
                }
                Op::Video {
                    transform,
                    video,
                    rect,
                    radius,
                    opacity,
                } => {
                    if let Some(Some(video)) = self.videos.get(*video) {
                        let placed = paint::fit(*rect, f64::from(video.width), f64::from(video.height), Fit::FILL).0;
                        let brush = brush_of(&video.image, quality(*transform * placed), *opacity, Extend::Pad);
                        Gpu::picture(&mut self.scene, *transform, &brush, placed, *rect, *radius);
                    }
                }
            }
            index += 1;
        }
        Ok(())
    }

    /// Has the card draw the scene into the frame's texture and copy the frame out.
    fn finish(&mut self) -> Result<(), String> {
        self.renderer
            .render_to_texture(&self.device, &self.queue, &self.scene, &self.view, &self.params)
            .map_err(|error| format!("the GPU could not draw the frame: {error}"))?;
        // The picture is copied out to a buffer of its own, so that the next frame can be drawn
        // into the texture while this one is still on its way.
        let slot = self
            .spare
            .pop()
            .ok_or_else(|| "a frame was drawn before the one before it was read".to_string())?;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        if let Some(outgoing) = &self.outgoing {
            outgoing.record(&mut encoder, &self.plane_staging[slot]);
            let work = self.queue.submit([encoder.finish()]);
            self.plane_staging[slot].slice(..).map_async(wgpu::MapMode::Read, |_| {});
            self.waiting.push_back((slot, work));
            return Ok(());
        }
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging[slot],
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row_bytes),
                    rows_per_image: None,
                },
            },
            self.texture.size(),
        );
        let work = self.queue.submit([encoder.finish()]);
        self.staging[slot].slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.waiting.push_back((slot, work));
        Ok(())
    }
}

impl Backend for Gpu {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn video_buffer(&mut self, video: usize, width: u32, height: u32) -> &mut [u8] {
        self.video(video, width, height).changed = true;
        self.painter.video_buffer(video, width, height)
    }

    fn takes_planes(&self, filter: &[ColorOp]) -> bool {
        gpu_footage::takes(filter)
    }

    fn video_planes(&mut self, video: usize, width: u32, height: u32, filter: &[ColorOp]) -> &mut [u8] {
        self.video(video, width, height);
        let Gpu {
            videos,
            converter,
            device,
            queue,
            ..
        } = self;
        let slot = videos[video].as_mut().expect("the video was just made");
        slot.changed = true;
        slot.painted = false;
        let planes = slot
            .planes
            .get_or_insert_with(|| converter.planes(device, queue, &slot.texture, (width, height), filter));
        &mut planes.data
    }

    fn release_video(&mut self, video: usize) {
        self.painter.release_video(video);
        if let Some(slot) = self.videos.get_mut(video).and_then(Option::take) {
            self.renderer.unregister_texture(slot.image);
        }
    }

    fn draw(&mut self, frame: u64, ops: &[Op]) -> Result<(), String> {
        for (index, video) in self.videos.iter_mut().enumerate() {
            let Some(video) = video.as_mut().filter(|video| video.changed) else {
                continue;
            };
            video.changed = false;
            if let Some(planes) = &video.planes {
                // The card makes the RGBA picture from the planes.
                self.converter.convert(&self.device, &self.queue, planes);
                self.renderer.mark_override_image_dirty(&video.image);
                continue;
            }
            let Some(Some(picture)) = self.painter.videos.get(index) else {
                continue;
            };
            self.queue.write_texture(
                video.texture.as_image_copy(),
                picture.data_as_u8_slice(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(video.width * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: video.width,
                    height: video.height,
                    depth_or_array_layers: 1,
                },
            );
            // Without this the renderer keeps the copy of the picture it made for the frame before.
            self.renderer.mark_override_image_dirty(&video.image);
        }
        self.scene.reset();
        self.used = 0;
        self.reach = self.frame;
        self.walk(ops, frame)?;
        self.painter.forget(frame);
        // A picture the painter has let go of is let go of here too.
        if frame.is_multiple_of(256) {
            self.kept.retain(|_, (picture, _)| Arc::strong_count(picture) > 2);
        }
        self.finish()
    }

    fn ahead(&self) -> usize {
        AHEAD
    }

    fn hand_over_planes(&mut self) -> bool {
        let (width, height) = (self.params.width, self.params.height);
        // Only between frames, and only for a frame that can be 4:2:0.
        if !self.waiting.is_empty() || !yuv::fits(width as usize, height as usize) {
            return false;
        }
        let outgoing = Outgoing::new(&self.device, &self.view, (width, height));
        self.plane_staging = (0..AHEAD)
            .map(|_| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("planes read"),
                    size: outgoing.buffer_size(),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            })
            .collect();
        self.pixels = vec![0; yuv::size(width as usize, height as usize)];
        self.outgoing = Some(outgoing);
        true
    }

    fn pixels(&mut self) -> Result<&[u8], String> {
        let (slot, work) = self
            .waiting
            .pop_front()
            .ok_or_else(|| "no frame is waiting to be read".to_string())?;
        // Only this frame's work is waited for: a later frame may still be on its way.
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(work),
                timeout: None,
            })
            .map_err(|error| format!("the GPU did not finish the frame: {error}"))?;
        if let Some(outgoing) = &self.outgoing {
            let buffer = &self.plane_staging[slot];
            {
                let mapped = buffer
                    .slice(..)
                    .get_mapped_range()
                    .map_err(|error| format!("the frame could not be read back: {error}"))?;
                outgoing.unpack(&mapped, &mut self.pixels);
            }
            buffer.unmap();
            self.spare.push(slot);
            return Ok(&self.pixels);
        }
        let slice = self.staging[slot].slice(..);
        {
            let mapped = slice
                .get_mapped_range()
                .map_err(|error| format!("the frame could not be read back: {error}"))?;
            let (row, padded) = (self.row_bytes as usize, self.padded_row_bytes as usize);
            if row == padded {
                self.pixels.copy_from_slice(&mapped);
            } else {
                for (target, source) in self.pixels.chunks_exact_mut(row).zip(mapped.chunks_exact(padded)) {
                    target.copy_from_slice(&source[..row]);
                }
            }
        }
        self.staging[slot].unmap();
        self.spare.push(slot);
        Ok(&self.pixels)
    }
}
