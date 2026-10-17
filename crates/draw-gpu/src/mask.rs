//! The coverage masks of the glyphs, kept by the card (`docs/spec/card-renderer.md`, sections 9.2, 9.3 and 9.6).
//!
//! A mask is the coverage of one glyph at one size, skew and fraction of its origin. The processor's rasteriser,
//! vello_cpu, fills the glyph into a box of pixels, and the card blends the colour of the run over the picture
//! through that box.

use std::collections::HashMap;
use std::sync::Arc;

use draw::{Error, Glyph, GlyphRun};
use gpu::wgpu;
use vello_cpu::kurbo::Affine;
use vello_cpu::peniko::{Blob, Color, FontData};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::head;

/// The margin, in pixels, between the box of a glyph and the edge of its mask (section 9.3).
const MARGIN: f64 = 2.0;

/// What decides a mask (section 9.2): the identity of the font's shared data and its face, the glyph, the size and
/// the skew as exact bit patterns, and the fraction of the origin's x and y as exact bit patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaskKey {
    font: usize,
    face: u32,
    glyph: u32,
    size: u32,
    skew: u32,
    fraction_x: u32,
    fraction_y: u32,
}

impl MaskKey {
    /// The key of a glyph of a run.
    pub fn of(run: &GlyphRun, glyph: &Glyph) -> MaskKey {
        MaskKey {
            font: Arc::as_ptr(&run.font.data) as usize,
            face: run.font.index,
            glyph: glyph.id,
            size: run.size.to_bits(),
            skew: run.skew_degrees.to_bits(),
            fraction_x: (glyph.x - glyph.x.floor()).to_bits(),
            fraction_y: (glyph.y - glyph.y.floor()).to_bits(),
        }
    }
}

/// A mask on the card: the pixel of the mask that lies on the whole pixel of the glyph's origin, the size of the
/// mask, and the bind group that gives the card its coverage.
pub struct Placed {
    pub origin_x: u32,
    pub origin_y: u32,
    pub width: u32,
    pub height: u32,
    pub bind_group: wgpu::BindGroup,
}

struct Entry {
    /// The font's data, held so that the address in the key is not reused while the entry is kept.
    _font: Arc<Vec<u8>>,
    /// `None` for a glyph with no coverage (a space, or a size of zero): nothing is drawn for it.
    placed: Option<Placed>,
}

/// The masks of one card renderer. They are kept for the life of the renderer, and none is evicted.
pub struct Masks {
    entries: HashMap<MaskKey, Entry>,
    resources: Resources,
}

/// The coverage of one glyph: the origin of the glyph in its mask, the size of the mask, and one byte of coverage
/// for each pixel of it, row by row.
struct Coverage {
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

impl Masks {
    pub fn new() -> Masks {
        Masks {
            entries: HashMap::new(),
            resources: Resources::new(),
        }
    }

    /// Makes the mask of a glyph, unless it is kept, and returns its key.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        run: &GlyphRun,
        glyph: &Glyph,
    ) -> Result<MaskKey, Error> {
        if !glyph.x.is_finite() || !glyph.y.is_finite() {
            return Err(Error::new(format!(
                "the glyph {} has an origin that is not a place on the frame",
                glyph.id
            )));
        }
        let key = MaskKey::of(run, glyph);
        if !self.entries.contains_key(&key) {
            let placed = match coverage(&mut self.resources, run, glyph)? {
                Some(coverage) => Some(upload(device, queue, layout, coverage)?),
                None => None,
            };
            self.entries.insert(
                key,
                Entry {
                    _font: run.font.data.clone(),
                    placed,
                },
            );
        }
        Ok(key)
    }

    /// The mask of a key that `prepare` has made; `None` when the glyph has no coverage.
    pub fn placed(&self, key: &MaskKey) -> Option<&Placed> {
        self.entries.get(key)?.placed.as_ref()
    }
}

/// Fills the glyph into its box with the processor's rasteriser, and reads the coverage of each pixel from the alpha
/// of the layer. The origin of the glyph lies at a whole pixel of the box plus the fraction of its key.
fn coverage(resources: &mut Resources, run: &GlyphRun, glyph: &Glyph) -> Result<Option<Coverage>, Error> {
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
    let half_width = box_width.ceil() + MARGIN;
    let half_height = tall.ceil() + MARGIN;
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
    let placed = std::iter::once(vello_cpu::Glyph {
        id: glyph.id,
        x: origin_x as f32 + (glyph.x - glyph.x.floor()),
        y: origin_y as f32 + (glyph.y - glyph.y.floor()),
    });
    context
        .glyph_run(resources, &font)
        .font_size(size)
        .glyph_transform(transform)
        .fill_glyphs(placed)
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
        origin_x,
        origin_y,
        width,
        height,
        alpha,
    }))
}

/// Uploads a coverage as a texture of one 8-bit channel, and gives it the bind group the glyph shader reads.
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
