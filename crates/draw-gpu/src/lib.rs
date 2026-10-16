//! The renderer on the graphics card (`docs/spec/card-renderer.md`). It draws the steps of a display
//! list in their order, each visible rectangle as two triangles whose corners lie on the whole pixel
//! boundaries of the frame, with no blending and no depth test. Its picture must agree with the
//! processor's within the tolerance of `card-renderer.md` section 4.
//!
//! The card and the calls to it are frx7's `gpu` crate, the system boundary of `CLAUDE.md` rule 10;
//! this crate uses the graphics API only through the safe types that crate hands out.

use std::ops::Range;
use std::sync::mpsc;

use draw::{Colour, DisplayList, Error, Picture, Renderer, Step};
use gpu::{wgpu, Card};
use layout::Rect;

/// The shader of the fills: `place_corner` for a corner, `paint` for its colour.
const SHADER: &str = include_str!("fill.wgsl");

/// The format of the picture: 8 bits a channel, with no conversion between colour spaces, so that
/// the colour value `v` is drawn as `v / 255` and stored as `v`.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The numbers of one vertex: a corner in clip space (two), then its colour (four).
const VERTEX_FLOATS: usize = 6;

/// Two triangles make one rectangle.
const CORNERS: u32 = 6;

/// The most pixels of a frame this renderer draws. It is the processor's limit, so that both
/// renderers refuse the same frames.
const MOST_PIXELS: u64 = 8192 * 8192;

/// The renderer on the graphics card. It is started once, and draws any number of frames with the
/// card it was started on.
pub struct CardRenderer {
    card: Card,
    pipeline: wgpu::RenderPipeline,
}

impl CardRenderer {
    /// Starts the renderer on the card frx7 can draw with. On a machine with no such card, or when
    /// the card could not be started, the error gives the reason and no picture is drawn.
    pub fn new() -> Result<CardRenderer, Error> {
        let card = Card::new().map_err(|error| Error::new(error.to_string()))?;
        let device = card.device();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fill"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fill"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fill"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("place_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (VERTEX_FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("paint"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(CardRenderer { card, pipeline })
    }
}

impl Renderer for CardRenderer {
    fn name(&self) -> &'static str {
        "card"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        let pixels = u64::from(list.width) * u64::from(list.height);
        let device = self.card.device();
        let limit = device.limits().max_texture_dimension_2d;
        if pixels == 0 || pixels > MOST_PIXELS || list.width > limit || list.height > limit {
            return Err(Error::new(format!(
                "a frame of {}x{} pixels cannot be drawn on the card: it has none, more than {MOST_PIXELS}, or a side above {limit}",
                list.width, list.height
            )));
        }

        let (floats, quads) = quads(list);
        let bytes: Vec<u8> = floats.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fill vertices"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.card.queue().write_buffer(&vertices, 0, &bytes);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("picture"),
            size: wgpu::Extent3d {
                width: list.width,
                height: list.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fill"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            // One draw call for each quad, in the order of the list: a later quad covers an earlier one.
            for quad in quads {
                pass.draw(quad, 0..1);
            }
        }

        // The rows of a texture copy are padded to a multiple of 256 bytes; the padding is left out.
        let row = (list.width * 4).next_multiple_of(256);
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("picture read back"),
            size: u64::from(row) * u64::from(list.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            texture.size(),
        );
        let index = self.card.queue().submit([encoder.finish()]);

        let (sender, received) = mpsc::channel();
        read.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.card.wait(index).map_err(|error| Error::new(error.to_string()))?;
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| Error::new(format!("the graphics card stopped: {error}")))?;
        received
            .recv()
            .map_err(|_| Error::new("the picture could not be read back from the card: the mapping did not report"))?
            .map_err(|error| Error::new(format!("the picture could not be read back from the card: {error}")))?;

        let mapped = read
            .slice(..)
            .get_mapped_range()
            .map_err(|error| Error::new(format!("the picture could not be read back from the card: {error}")))?;
        let line = list.width as usize * 4;
        let mut bytes = Vec::with_capacity(pixels as usize * 4);
        for padded in mapped.chunks(row as usize).take(list.height as usize) {
            bytes.extend_from_slice(&padded[..line]);
        }
        drop(mapped);
        read.unmap();
        Picture::new(list.width, list.height, bytes).ok_or_else(|| Error::new("the picture does not have the bytes of its size"))
    }
}

/// The vertices of the quads of a list, in the order they are drawn: the background over the whole
/// frame first, then each `Fill` step. A step that lies outside the frame makes no quad. Returns the
/// numbers of the vertices, as they go into the vertex buffer, and the range of vertices of each quad.
fn quads(list: &DisplayList) -> (Vec<f32>, Vec<Range<u32>>) {
    let whole = Rect {
        left: 0,
        top: 0,
        width: list.width,
        height: list.height,
    };
    let mut fills: Vec<(Rect, Colour)> = vec![(whole, list.background)];
    for step in &list.steps {
        let Step::Fill { rect, colour } = *step;
        fills.push((rect, colour));
    }

    let (width, height) = (list.width as f32, list.height as f32);
    let mut floats = Vec::new();
    let mut quads = Vec::new();
    for (rect, colour) in fills {
        let Some(visible) = rect.visible(list.width, list.height) else {
            continue;
        };
        let first = (floats.len() / VERTEX_FLOATS) as u32;
        let (left, top) = (visible.left as f32, visible.top as f32);
        let (right, bottom) = (visible.right as f32, visible.bottom as f32);
        // A corner in pixels from the top left, as clip space: the left edge of the frame is -1, the
        // right edge 1, the top 1 and the bottom -1. A pixel centre lies half a pixel inside its edges.
        let place = |x: f32, y: f32| [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0];
        let colour = [
            f32::from(colour.red) / 255.0,
            f32::from(colour.green) / 255.0,
            f32::from(colour.blue) / 255.0,
            1.0,
        ];
        for (x, y) in [
            (left, top),
            (right, top),
            (right, bottom),
            (left, top),
            (right, bottom),
            (left, bottom),
        ] {
            floats.extend_from_slice(&place(x, y));
            floats.extend_from_slice(&colour);
        }
        quads.push(first..first + CORNERS);
    }
    (floats, quads)
}
