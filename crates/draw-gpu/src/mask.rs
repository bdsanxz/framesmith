//! The coverage masks of the glyphs and of the mapped fills, kept by the card (`docs/spec/card-renderer.md`, sections 9.2,
//! 9.3, 9.6, 10.2 and 10.3).
//!
//! A mask is the coverage of one glyph at one size, skew, linear map and fraction of its origin, or of one mapped fill. The
//! processor's rasteriser, vello_cpu, fills the glyph or the rectangle into a box of pixels, and the card blends the colour
//! over the picture through that box. The masks kept are bounded by `BOUND`: the least recently used are let go after each
//! frame.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use draw::{Error, Glyph, GlyphRun, Map};
use layout::Rect;
use gpu::wgpu;
use vello_cpu::kurbo::Affine;
use vello_cpu::peniko::{Blob, Color, FontData};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::head;

/// The margin, in pixels, between the box of a glyph and the edge of its mask (section 9.3).
const MARGIN: f64 = 2.0;

/// The most bytes of coverage the card keeps across frames (section 9.6): 64 MiB.
pub const BOUND: u64 = 64 * 1024 * 1024;

/// What decides a mask. A glyph (sections 9.2 and 10.2): the identity of the font's shared data and its face, the glyph,
/// the size and the skew as exact bit patterns, the linear part of its map (`None` for a glyph that is not mapped, or is
/// moved by whole pixels), and the fraction of its origin's x and y as exact bit patterns. A mapped fill (section 10.3): the
/// rectangle, the map and the size of the frame, each exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MaskKey {
    Glyph {
        font: usize,
        face: u32,
        glyph: u32,
        size: u32,
        skew: u32,
        linear: Option<[u64; 4]>,
        fraction_x: u64,
        fraction_y: u64,
    },
    Fill {
        rect: (i32, i32, u32, u32),
        map: [u64; 6],
        frame: (u32, u32),
    },
}

impl MaskKey {
    /// The key of a glyph of a run, drawn as it is.
    pub fn of(run: &GlyphRun, glyph: &Glyph) -> MaskKey {
        MaskKey::Glyph {
            font: Arc::as_ptr(&run.font.data) as usize,
            face: run.font.index,
            glyph: glyph.id,
            size: run.size.to_bits(),
            skew: run.skew_degrees.to_bits(),
            linear: None,
            fraction_x: f64::from(glyph.x - glyph.x.floor()).to_bits(),
            fraction_y: f64::from(glyph.y - glyph.y.floor()).to_bits(),
        }
    }

    /// The key of a glyph of a run under a map that is not a move by whole pixels, with its mapped origin.
    pub fn mapped(run: &GlyphRun, glyph: &Glyph, map: Map, (x, y): (f64, f64)) -> MaskKey {
        MaskKey::Glyph {
            font: Arc::as_ptr(&run.font.data) as usize,
            face: run.font.index,
            glyph: glyph.id,
            size: run.size.to_bits(),
            skew: run.skew_degrees.to_bits(),
            linear: Some([map.a.to_bits(), map.b.to_bits(), map.c.to_bits(), map.d.to_bits()]),
            fraction_x: (x - x.floor()).to_bits(),
            fraction_y: (y - y.floor()).to_bits(),
        }
    }
}

/// A mask on the card: the pixel of the mask that lies on the whole pixel it is placed at (the glyph's origin, or the frame's
/// top left corner for a fill), the size of the mask, and the bind group that gives the card its coverage.
pub struct Placed {
    pub origin_x: i64,
    pub origin_y: i64,
    pub width: u32,
    pub height: u32,
    pub bind_group: wgpu::BindGroup,
}

struct Entry {
    /// The font's data, held so that the address in the key is not reused while the entry is kept; `None` for a fill.
    _font: Option<Arc<Vec<u8>>>,
    /// `None` for a glyph with no coverage (a space, or a size of zero): nothing is drawn for it.
    placed: Option<Placed>,
    /// The bytes of coverage the mask holds.
    bytes: u64,
    /// When it was last used, as its place in `Masks::order`.
    used: (u64, u64),
}

/// The masks of one card renderer, kept across frames up to `BOUND` bytes of coverage (section 9.6).
pub struct Masks {
    entries: HashMap<MaskKey, Entry>,
    /// The keys by when they were last used: the frame, then the order of use in it.
    order: BTreeMap<(u64, u64), MaskKey>,
    /// The frame being drawn, counted from 1, and the uses in it so far.
    frame: u64,
    uses: u64,
    /// The bytes of coverage kept.
    bytes: u64,
    resources: Resources,
}

/// The coverage of one glyph: the origin of the glyph in its mask, the size of the mask, and one byte of coverage
/// for each pixel of it, row by row.
struct Coverage {
    origin_x: i64,
    origin_y: i64,
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

impl Masks {
    pub fn new() -> Masks {
        Masks {
            entries: HashMap::new(),
            order: BTreeMap::new(),
            frame: 0,
            uses: 0,
            bytes: 0,
            resources: Resources::new(),
        }
    }

    /// Starts a frame: the masks it uses count as used after those of every frame before.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.uses = 0;
    }

    /// Lets go of the masks used least recently until what is kept is at most `BOUND` (section 9.6). Called after a frame has
    /// been read back, when no draw uses a mask any more.
    pub fn end_frame(&mut self) {
        while self.bytes > BOUND {
            let Some((_, key)) = self.order.pop_first() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes -= entry.bytes;
            }
        }
    }

    /// The bytes of coverage kept.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Marks a kept mask as used now.
    fn touch(&mut self, key: MaskKey) {
        let used = (self.frame, self.uses);
        self.uses += 1;
        if let Some(entry) = self.entries.get_mut(&key) {
            self.order.remove(&entry.used);
            entry.used = used;
            self.order.insert(used, key);
        }
    }

    /// Keeps a new mask, used now.
    fn keep(&mut self, key: MaskKey, font: Option<Arc<Vec<u8>>>, placed: Option<Placed>) {
        let bytes = placed.as_ref().map_or(0, |placed| u64::from(placed.width) * u64::from(placed.height));
        let used = (self.frame, self.uses);
        self.uses += 1;
        self.bytes += bytes;
        self.order.insert(used, key);
        self.entries.insert(
            key,
            Entry {
                _font: font,
                placed,
                bytes,
                used,
            },
        );
    }

    /// Makes the mask of a glyph, drawn with `map` (a map that is not a move by whole pixels), or as it is with `None`,
    /// unless it is kept, and returns its key and the place of the glyph's origin in the frame.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        run: &GlyphRun,
        glyph: &Glyph,
        map: Option<Map>,
    ) -> Result<(MaskKey, (f64, f64)), Error> {
        if !glyph.x.is_finite() || !glyph.y.is_finite() {
            return Err(Error::new(format!(
                "the glyph {} has an origin that is not a place on the frame",
                glyph.id
            )));
        }
        let (key, origin) = match map {
            None => (MaskKey::of(run, glyph), (f64::from(glyph.x), f64::from(glyph.y))),
            Some(map) => {
                let origin = map.apply(f64::from(glyph.x), f64::from(glyph.y));
                if !origin.0.is_finite() || !origin.1.is_finite() {
                    return Err(Error::new(format!(
                        "the glyph {} has an origin that is not a place on the frame",
                        glyph.id
                    )));
                }
                (MaskKey::mapped(run, glyph, map, origin), origin)
            }
        };
        if self.entries.contains_key(&key) {
            self.touch(key);
        } else {
            let fraction = (origin.0 - origin.0.floor(), origin.1 - origin.1.floor());
            let placed = match coverage(&mut self.resources, run, glyph, map, fraction)? {
                Some(coverage) => Some(upload(device, queue, layout, coverage)?),
                None => None,
            };
            self.keep(key, Some(run.font.data.clone()), placed);
        }
        Ok((key, origin))
    }

    /// Makes the mask of a fill under a map that is not a move by whole pixels, unless it is kept, and returns its key. The
    /// coverage is the processor's (`draw_cpu::fill_coverage`), placed at the frame pixel it gives.
    pub fn prepare_fill(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        rect: Rect,
        map: Map,
        (width, height): (u32, u32),
    ) -> Result<MaskKey, Error> {
        let bits = [map.a, map.b, map.c, map.d, map.e, map.f].map(f64::to_bits);
        let key = MaskKey::Fill {
            rect: (rect.left, rect.top, rect.width, rect.height),
            map: bits,
            frame: (width, height),
        };
        if self.entries.contains_key(&key) {
            self.touch(key);
            return Ok(key);
        }
        let placed = match draw_cpu::fill_coverage(rect, map, width, height)? {
            Some(found) => Some(upload(
                device,
                queue,
                layout,
                Coverage {
                    origin_x: -i64::from(found.left),
                    origin_y: -i64::from(found.top),
                    width: found.width,
                    height: found.height,
                    alpha: found.alpha,
                },
            )?),
            None => None,
        };
        self.keep(key, None, placed);
        Ok(key)
    }

    /// The mask of a key that `prepare` has made; `None` when it has no coverage.
    pub fn placed(&self, key: &MaskKey) -> Option<&Placed> {
        self.entries.get(key)?.placed.as_ref()
    }
}

/// Fills the glyph into its box with the processor's rasteriser, and reads the coverage of each pixel from the alpha
/// of the layer. The origin of the glyph lies at a whole pixel of the box plus `fraction`. Under a map, the glyph is drawn
/// with the map's linear part, and its box is the box of section 9.3 under that part (section 10.2).
fn coverage(
    resources: &mut Resources,
    run: &GlyphRun,
    glyph: &Glyph,
    map: Option<Map>,
    (fraction_x, fraction_y): (f64, f64),
) -> Result<Option<Coverage>, Error> {
    let size = run.size;
    if !size.is_finite() || size < 0.0 {
        return Err(Error::new(format!("a glyph of size {size} cannot be drawn on the card")));
    }
    if size == 0.0 {
        return Ok(None);
    }
    if run.font.index != 0 {
        return Err(Error::new(format!(
            "the card draws the first face of a font file, and the face {} was asked for",
            run.font.index
        )));
    }
    let head = head::read(&run.font.data).ok_or_else(|| {
        Error::new("the card cannot read the box of the glyphs of a font: it is a collection, or its head table is missing or cut short")
    })?;
    if head.units_per_em == 0 {
        return Err(Error::new("the card cannot draw a font whose units per em are 0"));
    }

    let scale = f64::from(size) / f64::from(head.units_per_em);
    let tall = f64::from(head.y_min.unsigned_abs().max(head.y_max.unsigned_abs())) * scale;
    let wide = f64::from(head.x_min.unsigned_abs().max(head.x_max.unsigned_abs())) * scale;
    let skew = f64::from(run.skew_degrees).to_radians().tan();
    let box_width = wide + skew.abs() * tall;
    let (reach_x, reach_y) = match map {
        None => (box_width, tall),
        Some(map) => (
            map.a.abs() * box_width + map.c.abs() * tall,
            map.b.abs() * box_width + map.d.abs() * tall,
        ),
    };
    let half_width = reach_x.ceil() + MARGIN;
    let half_height = reach_y.ceil() + MARGIN;
    let most = f64::from(u16::MAX);
    if !(2.0 * half_width + 2.0 <= most && 2.0 * half_height + 2.0 <= most) {
        return Err(Error::new(format!(
            "the glyph {} of size {size} is too large for the card",
            glyph.id
        )));
    }
    let origin_x = half_width as u32;
    let origin_y = half_height as u32;
    let width = 2 * origin_x + 2;
    let height = 2 * origin_y + 2;

    let mut context = RenderContext::new(width as u16, height as u16);
    context.set_paint(Color::from_rgba8(255, 255, 255, 255));
    let data: Arc<dyn AsRef<[u8]> + Send + Sync> = run.font.data.clone();
    let font = FontData::new(Blob::new(data), run.font.index);
    let transform = Affine::new([1.0, 0.0, -skew, 1.0, 0.0, 0.0]);
    let placed = match map {
        None => vello_cpu::Glyph {
            id: glyph.id,
            x: origin_x as f32 + fraction_x as f32,
            y: origin_y as f32 + fraction_y as f32,
        },
        Some(map) => {
            let at = (f64::from(origin_x) + fraction_x, f64::from(origin_y) + fraction_y);
            context.set_transform(Affine::new([map.a, map.b, map.c, map.d, at.0, at.1]));
            vello_cpu::Glyph {
                id: glyph.id,
                x: 0.0,
                y: 0.0,
            }
        }
    };
    context
        .glyph_run(resources, &font)
        .font_size(size)
        .glyph_transform(transform)
        .fill_glyphs(std::iter::once(placed))
        .map_err(|error| Error::new(format!("the glyphs of a run cannot be drawn: {error}")))?;
    context.flush();
    let mut pixmap = Pixmap::new(width as u16, height as u16);
    context.render(&mut pixmap, resources);
    let alpha: Vec<u8> = pixmap.data_as_u8_slice().as_chunks::<4>().0.iter().map(|pixel| pixel[3]).collect();

    // The outermost ring of the mask must be clear: any ink there means that the outline lies outside its box.
    let at = |x: u32, y: u32| alpha[(y * width + x) as usize] != 0;
    let ring = (0..width).any(|x| at(x, 0) || at(x, height - 1)) || (0..height).any(|y| at(0, y) || at(width - 1, y));
    if ring {
        return Err(Error::new(format!(
            "the outline of the glyph {} lies outside its box: the card draws no picture of it",
            glyph.id
        )));
    }
    if alpha.iter().all(|&value| value == 0) {
        return Ok(None);
    }
    Ok(Some(Coverage {
        origin_x: i64::from(origin_x),
        origin_y: i64::from(origin_y),
        width,
        height,
        alpha,
    }))
}

/// Uploads a coverage as a texture of one 8-bit channel, and gives it the bind group the glyph shader reads. A coverage with
/// no pixel at all is never uploaded: the callers draw nothing for it.
fn upload(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout, coverage: Coverage) -> Result<Placed, Error> {
    let limit = device.limits().max_texture_dimension_2d;
    if coverage.width > limit || coverage.height > limit {
        return Err(Error::new(format!(
            "a glyph mask of {}x{} pixels cannot be drawn on the card: a side is above {limit}",
            coverage.width, coverage.height
        )));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph mask"),
        size: wgpu::Extent3d {
            width: coverage.width,
            height: coverage.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &coverage.alpha,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(coverage.width),
            rows_per_image: None,
        },
        texture.size(),
    );
    let view = texture.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("glyph mask"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    Ok(Placed {
        origin_x: coverage.origin_x,
        origin_y: coverage.origin_y,
        width: coverage.width,
        height: coverage.height,
        bind_group,
    })
}
