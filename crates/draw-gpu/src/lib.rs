//! The renderer on the graphics card (`docs/spec/card-renderer.md`). It draws the marks of a frame in their order:
//! each visible fill as two triangles whose corners lie on the whole pixel boundaries of the frame, and each glyph
//! of a paragraph of text as a quad of whole pixels whose coverage is a mask made by the processor's rasteriser
//! (section 9); the marks of an element that moves with its map, and those of one that fades in a texture of its own,
//! laid over the picture with its opacity (section 10). Its picture must agree with the processor's within the tolerance
//! of section 4.
//!
//! The card and the calls to it are frx7's `gpu` crate, the system boundary of `CLAUDE.md` rule 10; this crate uses
//! the graphics API only through the safe types that crate hands out.

use std::ops::Range;
use std::sync::{mpsc, Arc};

use draw::{
    blur_weights, Background, Blend, Colour, Cut, DisplayList, Error, Filter, Frame, GlyphPicture, Gradient, Group, Map, Mark, Picture,
    Renderer, Rgba, Ring, RingMark, Step, TextMark, Turn,
};
use draw_cpu::{background_layer, outline_layer, pictures_layer, placed_picture_layer, shadow_layer, still_layer};
use gpu::{wgpu, Card};
use layout::Rect;

mod head;
mod mask;

pub use mask::BOUND as MASK_BOUND;

use mask::{MaskKey, Masks};

/// The shader of the fills: `place_corner` for a corner, `paint` for its colour.
const SHADER: &str = include_str!("fill.wgsl");

/// The shader of the glyphs: `place_glyph_corner` for a corner, `paint_glyph` for its colour and coverage.
const GLYPH_SHADER: &str = include_str!("glyph.wgsl");

/// The shader of the layers of a background: `place_layer_corner` for a corner, `paint_layer` for its colour (section 11).
const PAINT_SHADER: &str = include_str!("paint.wgsl");
const TURN_SHADER: &str = include_str!("turn.wgsl");

/// The shader of a pass of the blur and brightness of a group: `place_filter_corner` for a corner, `paint_filtered` for each
/// pixel of the next texture (section 10.7).
const FILTER_SHADER: &str = include_str!("filter.wgsl");
/// The shader of a group laid with a blend: `place_blend_corner` for a corner, `paint_blended` for the colour of each pixel
/// (`docs/spec/blends.md`, section 6).
const BLEND_SHADER: &str = include_str!("blend.wgsl");

/// The format of the picture: 8 bits a channel, with no conversion between colour spaces, so that
/// the colour value `v` is drawn as `v / 255` and stored as `v`.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The numbers of one vertex: a corner in clip space (two), the whole pixel of a glyph's mask at its top left (two;
/// zero for a fill), then the colour (four).
const VERTEX_FLOATS: usize = 8;

/// Two triangles make one rectangle.
const CORNERS: u32 = 6;

/// The most pixels of a frame this renderer draws. It is the processor's limit, so that both
/// renderers refuse the same frames.
const MOST_PIXELS: u64 = 8192 * 8192;

/// The attributes of a vertex, in the layout of `VERTEX_FLOATS`: the corner, the origin, and the colour.
const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 8,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 16,
        shader_location: 2,
    },
];

/// A glyph's colour is blended over the picture by its coverage: the colour is premultiplied by it, and the
/// picture keeps what the coverage leaves clear (section 9.4). The alpha is blended the same way, so that a transparent
/// texture of a group takes the coverage of what is drawn into it; over the opaque picture it stays 1 (section 10.1).
const GLYPH_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

/// A layer of a cut keeps what is drawn into it only inside the shape (`docs/spec/box-edges.md`, section 7): each value of the
/// layer is multiplied by the shape's coverage, which the fragment gives in all four channels.
const KEEP_INSIDE: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::SrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::SrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

/// The renderer on the graphics card. It is started once, and draws any number of frames with the
/// card it was started on.
pub struct CardRenderer {
    card: Card,
    pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    picture_pipeline: wgpu::RenderPipeline,
    paint_pipeline: wgpu::RenderPipeline,
    turn_pipeline: wgpu::RenderPipeline,
    cut_pipeline: wgpu::RenderPipeline,
    filter_pipeline: wgpu::RenderPipeline,
    blend_pipeline: wgpu::RenderPipeline,
    blend_layout: wgpu::BindGroupLayout,
    glyph_layout: wgpu::BindGroupLayout,
    paint_layout: wgpu::BindGroupLayout,
    turn_layout: wgpu::BindGroupLayout,
    filter_layout: wgpu::BindGroupLayout,
    masks: Masks,
}

/// Where the draws of a frame go: the picture, or the texture of a group that fades, by its index in the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Picture,
    Layer(usize),
}

impl CardRenderer {
    /// Starts the renderer on the card frx7 can draw with. On a machine with no such card, or when
    /// the card could not be started, the error gives the reason and no picture is drawn.
    pub fn new() -> Result<CardRenderer, Error> {
        let card = Card::new().map_err(|error| Error::new(error.to_string()))?;
        let device = card.device();
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: (VERTEX_FLOATS * 4) as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        })];

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
                buffers: &buffers,
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

        let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glyph"),
            source: wgpu::ShaderSource::Wgsl(GLYPH_SHADER.into()),
        });
        let glyph_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph mask"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let glyph_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph"),
            bind_group_layouts: &[Some(&glyph_layout)],
            immediate_size: 0,
        });
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glyph"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: Some("place_glyph_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &glyph_shader,
                entry_point: Some("paint_glyph"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(GLYPH_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let picture_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("picture"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: Some("place_glyph_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &glyph_shader,
                entry_point: Some("paint_picture"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(GLYPH_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let paint_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("paint"),
            source: wgpu::ShaderSource::Wgsl(PAINT_SHADER.into()),
        });
        let paint_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("background layers"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let paint_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("paint"),
            bind_group_layouts: &[Some(&paint_layout)],
            immediate_size: 0,
        });
        let paint_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("paint"),
            layout: Some(&paint_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &paint_shader,
                entry_point: Some("place_layer_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &paint_shader,
                entry_point: Some("paint_layer"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(GLYPH_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let turn_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("turn"),
            source: wgpu::ShaderSource::Wgsl(TURN_SHADER.into()),
        });
        let turn_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("turned group"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let turn_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("turn"),
            bind_group_layouts: &[Some(&turn_layout)],
            immediate_size: 0,
        });
        let turn_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("turn"),
            layout: Some(&turn_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &turn_shader,
                entry_point: Some("place_turn_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &turn_shader,
                entry_point: Some("paint_turned"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(GLYPH_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let blend_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blend"),
            source: wgpu::ShaderSource::Wgsl(BLEND_SHADER.into()),
        });
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let blend_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blended group"),
            entries: &[texture_entry(0), texture_entry(1)],
        });
        let blend_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blend"),
            bind_group_layouts: &[Some(&blend_layout)],
            immediate_size: 0,
        });
        // The shader writes the whole colour of each pixel, so nothing is blended by the card's own blending.
        let blend_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blend"),
            layout: Some(&blend_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blend_shader,
                entry_point: Some("place_blend_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &blend_shader,
                entry_point: Some("paint_blended"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let cut_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cut"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: Some("place_glyph_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &glyph_shader,
                entry_point: Some("keep_inside"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(KEEP_INSIDE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let filter_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("filter"),
            source: wgpu::ShaderSource::Wgsl(FILTER_SHADER.into()),
        });
        let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("filtered group"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let filter_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("filter"),
            bind_group_layouts: &[Some(&filter_layout)],
            immediate_size: 0,
        });
        // Each pass writes every pixel of its texture: nothing is blended.
        let filter_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("filter"),
            layout: Some(&filter_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &filter_shader,
                entry_point: Some("place_filter_corner"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &filter_shader,
                entry_point: Some("paint_filtered"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(CardRenderer {
            card,
            blend_pipeline,
            blend_layout,
            pipeline,
            glyph_pipeline,
            picture_pipeline,
            paint_pipeline,
            turn_pipeline,
            cut_pipeline,
            filter_pipeline,
            glyph_layout,
            paint_layout,
            turn_layout,
            filter_layout,
            masks: Masks::new(),
        })
    }

    /// The bytes of coverage of the masks the card keeps across frames, at most `mask::BOUND` after each frame
    /// (`docs/spec/card-renderer.md`, section 9.6).
    pub fn mask_bytes(&self) -> u64 {
        self.masks.bytes()
    }

    /// Draws the marks of a frame: the background over the whole frame first, then each mark in order, each over
    /// what is there. One render pass carries all of them.
    fn draw_marks(&mut self, frame: &Frame) -> Result<Picture, Error> {
        let (width, height) = (frame.width, frame.height);
        let pixels = u64::from(width) * u64::from(height);
        let limit = self.card.device().limits().max_texture_dimension_2d;
        if pixels == 0 || pixels > MOST_PIXELS || width > limit || height > limit {
            return Err(Error::new(format!(
                "a frame of {width}x{height} pixels cannot be drawn on the card: it has none, more than {MOST_PIXELS}, or a side above {limit}"
            )));
        }

        self.masks.begin_frame();
        let (floats, draws, pictures, layers, paint) = self.plan(frame)?;
        let device = self.card.device();
        // The numbers of the background layers of the frame, read by the paint shader (section 11).
        let paint_group = (!paint.is_empty()).then(|| {
            let bytes: Vec<u8> = paint.iter().flat_map(|value| value.to_ne_bytes()).collect();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("background layers"),
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.card.queue().write_buffer(&buffer, 0, &bytes);
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("background layers"),
                layout: &self.paint_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            })
        });
        let bytes: Vec<u8> = floats.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame vertices"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.card.queue().write_buffer(&vertices, 0, &bytes);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("picture"),
            size: wgpu::Extent3d {
                width,
                height,
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
        // The texture of each group that fades starts transparent.
        for layer in &layers {
            begin_pass(&mut encoder, &layer.view, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
        }
        // One draw call for each quad, in the order of the frame: a later quad covers an earlier one. A pass is begun each
        // time the target changes, so that a group's texture is drawn before it is laid over its parent.
        let mut index = 0;
        let mut first = true;
        while index < draws.len() {
            let target = draws[index].target;
            // A blended group reads what is under it: that is copied first, outside any pass (blends.md, section 6).
            if let Source::Blended(layer) = draws[index].source {
                let Some((under, _)) = &layers[layer].blended else {
                    return Err(Error::new("a blended group was drawn with no copy of what is under it"));
                };
                let from = match target {
                    Target::Picture => &texture,
                    Target::Layer(parent) => &layers[parent].texture,
                };
                encoder.copy_texture_to_texture(from.as_image_copy(), under.as_image_copy(), from.size());
            }
            let (target_view, load) = match target {
                Target::Picture if first => (&view, wgpu::LoadOp::Clear(wgpu::Color::BLACK)),
                Target::Picture => (&view, wgpu::LoadOp::Load),
                Target::Layer(layer) => (&layers[layer].view, wgpu::LoadOp::Load),
            };
            if target == Target::Picture {
                first = false;
            }
            let mut pass = begin_pass(&mut encoder, target_view, load);
            pass.set_vertex_buffer(0, vertices.slice(..));
            let mut drawn = 0;
            // A blended group's quad is the first of a pass, after its copy.
            while index < draws.len() && draws[index].target == target && (drawn == 0 || !matches!(draws[index].source, Source::Blended(_)))
            {
                let draw = &draws[index];
                index += 1;
                drawn += 1;
                let [x, y, w, h] = draw.scissor;
                pass.set_scissor_rect(x, y, w, h);
                match &draw.source {
                    Source::Fill => {
                        pass.set_pipeline(&self.pipeline);
                    }
                    Source::Glyph(key) => {
                        let Some(placed) = self.masks.placed(key) else {
                            return Err(Error::new("a glyph mask was not kept for its draw"));
                        };
                        pass.set_pipeline(&self.glyph_pipeline);
                        pass.set_bind_group(0, &placed.bind_group, &[]);
                    }
                    Source::Picture(picture) => {
                        pass.set_pipeline(&self.picture_pipeline);
                        pass.set_bind_group(0, &pictures[*picture], &[]);
                    }
                    Source::Layer(layer) => {
                        pass.set_pipeline(&self.picture_pipeline);
                        pass.set_bind_group(0, &layers[*layer].bind_group, &[]);
                    }
                    Source::Turned(layer) => {
                        let Some(turned) = &layers[*layer].turned else {
                            return Err(Error::new("a turned group was drawn with no numbers"));
                        };
                        pass.set_pipeline(&self.turn_pipeline);
                        pass.set_bind_group(0, turned, &[]);
                    }
                    Source::Filtered(layer) => {
                        let Some(filtered) = &layers[*layer].filtered else {
                            return Err(Error::new("a pass of a filtered group was drawn with no numbers"));
                        };
                        pass.set_pipeline(&self.filter_pipeline);
                        pass.set_bind_group(0, filtered, &[]);
                    }
                    Source::Blended(layer) => {
                        let Some((_, blended)) = &layers[*layer].blended else {
                            return Err(Error::new("a blended group was drawn with no copy of what is under it"));
                        };
                        pass.set_pipeline(&self.blend_pipeline);
                        pass.set_bind_group(0, blended, &[]);
                    }
                    Source::Paint => {
                        let Some(group) = &paint_group else {
                            return Err(Error::new("a background layer was drawn with no numbers"));
                        };
                        pass.set_pipeline(&self.paint_pipeline);
                        pass.set_bind_group(0, group, &[]);
                    }
                    Source::KeepInside(key) => {
                        let Some(placed) = self.masks.placed(key) else {
                            return Err(Error::new("the mask of a cut was not kept for its draw"));
                        };
                        pass.set_pipeline(&self.cut_pipeline);
                        pass.set_bind_group(0, &placed.bind_group, &[]);
                    }
                }
                pass.draw(draw.vertices.clone(), 0..1);
            }
        }
        if first {
            begin_pass(&mut encoder, &view, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
        }

        // The rows of a texture copy are padded to a multiple of 256 bytes; the padding is left out.
        let row = (width * 4).next_multiple_of(256);
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("picture read back"),
            size: u64::from(row) * u64::from(height),
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
        let line = width as usize * 4;
        let mut bytes = Vec::with_capacity(pixels as usize * 4);
        for padded in mapped.chunks(row as usize).take(height as usize) {
            bytes.extend_from_slice(&padded[..line]);
        }
        drop(mapped);
        read.unmap();
        self.masks.end_frame();
        Picture::new(width, height, bytes).ok_or_else(|| Error::new("the picture does not have the bytes of its size"))
    }

    /// The vertices of a frame and its draw calls, in the order they are drawn: the background over the whole frame
    /// first, then each mark. A fill that lies outside the frame makes no quad, and so does a paragraph whose clip
    /// has no visible pixel.
    fn plan(&mut self, frame: &Frame) -> Result<Plan, Error> {
        let (width, height) = (frame.width, frame.height);
        let mut plan = Planned {
            floats: Vec::new(),
            draws: Vec::new(),
            pictures: Vec::new(),
            layers: Vec::new(),
            paint: Vec::new(),
            width,
            height,
        };
        let whole = Rect {
            left: 0,
            top: 0,
            width,
            height,
        };
        let scissor = [0, 0, width, height];
        push_fill(&mut plan, Target::Picture, scissor, whole, frame.background);
        self.plan_marks(&mut plan, Target::Picture, scissor, &frame.marks, Map::IDENTITY)?;
        Ok((plan.floats, plan.draws, plan.pictures, plan.layers, plan.paint))
    }

    /// The draws of marks into `target`, each with `map`, cut to `scissor` (left, top, width, height).
    fn plan_marks(&mut self, plan: &mut Planned, target: Target, scissor: [u32; 4], marks: &[Mark], map: Map) -> Result<(), Error> {
        for mark in marks {
            match mark {
                Mark::Step(Step::Fill { rect, colour }) => match map.whole_move() {
                    Some((across, down)) => {
                        let moved = Rect {
                            left: (i64::from(rect.left) + across).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                            top: (i64::from(rect.top) + down).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                            ..*rect
                        };
                        push_fill(plan, target, scissor, moved, *colour);
                    }
                    None => self.plan_mapped_fill(plan, target, scissor, *rect, *colour, map)?,
                },
                Mark::Text(text) => match map.whole_move() {
                    Some((across, down)) => self.plan_text(plan, target, scissor, &text.moved(across, down), None)?,
                    None => self.plan_text(plan, target, scissor, text, Some(map))?,
                },
                Mark::Group(group) => self.plan_group(plan, target, scissor, group, map)?,
                Mark::Background(background) => match map.whole_move() {
                    Some((across, down)) => push_background(plan, target, scissor, background, across, down),
                    None => {
                        let layer = background_layer(plan.width, plan.height, background, map)?;
                        self.push_layer(plan, target, scissor, layer)?;
                    }
                },
                Mark::Ring(ring) => self.plan_ring(plan, target, scissor, ring, map)?,
                Mark::Cut(cut) => self.plan_cut(plan, target, scissor, cut, map)?,
                Mark::BoxShadow(shadow) => {
                    // A box's shadow is drawn by the processor's renderer, and the card lays it (section 4.7, rule 6).
                    let layer = draw_cpu::box_shadow_layer(plan.width, plan.height, shadow, map)?;
                    self.push_layer(plan, target, scissor, layer)?;
                }
                // A picture moved by whole pixels is laid one texel for one at its whole pixel (section 12); under any other
                // map it is drawn through the map on the processor and laid as a layer, as a paragraph's pictures are.
                Mark::Picture(picture) => match map.whole_move() {
                    Some((across, down)) => {
                        let texels = GlyphPicture {
                            width: picture.width,
                            height: picture.height,
                            pixels: picture.pixels.clone(),
                            x: 0.0,
                            y: 0.0,
                        };
                        let left = (whole_pixel(picture.x) + across) as f32;
                        let top = (whole_pixel(picture.y) + down) as f32;
                        self.push_picture(plan, target, scissor, &texels, (left, top))?;
                    }
                    None => {
                        let layer = placed_picture_layer(plan.width, plan.height, picture, map)?;
                        self.push_layer(plan, target, scissor, layer)?;
                    }
                },
                Mark::Still(still) => {
                    // The processor draws the picture into a layer of the frame's size, which the card lays over its
                    // target (`docs/spec/card-renderer.md`, section 13).
                    let layer = still_layer(plan.width, plan.height, still, map)?;
                    self.push_layer(plan, target, scissor, layer)?;
                }
            }
        }
        Ok(())
    }

    /// The draws of a group (section 10.1): straight into `target` with an opacity of 1, otherwise into a texture of its
    /// own that is then laid over `target` with the opacity; nothing at opacity 0.
    fn plan_group(&mut self, plan: &mut Planned, target: Target, scissor: [u32; 4], group: &Group, map: Map) -> Result<(), Error> {
        if group.blend == Blend::Normal {
            return self.plan_unblended(plan, target, scissor, group, map);
        }
        self.plan_blended(plan, target, scissor, group, map)
    }

    /// The draws of a group laid with a blend (`docs/spec/blends.md`, section 6, rule 2): the group as it is drawn with no
    /// blend into a texture of its own, cleared to transparent, then one quad over `scissor` that writes the blend of that
    /// texture with a copy of what is under it, made just before the quad is drawn.
    fn plan_blended(&mut self, plan: &mut Planned, target: Target, scissor: [u32; 4], group: &Group, map: Map) -> Result<(), Error> {
        let layer = plan.layers.len();
        let mut blended = self.layer(plan.width, plan.height);
        blended.blended = Some(self.under(&blended, plan.width, plan.height));
        plan.layers.push(blended);
        self.plan_unblended(plan, Target::Layer(layer), scissor, group, map)?;
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        let corners = (0.0, 0.0, plan.width as f32, plan.height as f32);
        push_quad(
            &mut plan.floats,
            plan.width,
            plan.height,
            corners,
            [0.0, 0.0],
            [group.blend.index() as f32, 0.0, 0.0, 0.0],
        );
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Blended(layer),
        });
        Ok(())
    }

    /// The texture what lies under a blended group is copied into, and the bind group that reads the group's texture and it.
    fn under(&self, layer: &Layer, width: u32, height: u32) -> (wgpu::Texture, wgpu::BindGroup) {
        let device = self.card.device();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("under a blended group"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blended group"),
            layout: &self.blend_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&layer.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        (texture, bind_group)
    }

    /// The draws of a group by src-over, whatever its blend.
    fn plan_unblended(&mut self, plan: &mut Planned, target: Target, scissor: [u32; 4], group: &Group, map: Map) -> Result<(), Error> {
        let opacity = if group.opacity.is_nan() {
            0.0
        } else {
            group.opacity.clamp(0.0, 1.0)
        };
        if opacity == 0.0 {
            return Ok(());
        }
        let around = map;
        let map = map.then_after(group.map);
        if !map.is_finite() {
            return Err(Error::new("an element is moved by a map that is not finite"));
        }
        let scissor = match &group.clip {
            None => scissor,
            Some(clip) => match cut(scissor, clip, plan.width, plan.height) {
                Some(scissor) => scissor,
                None => return Ok(()),
            },
        };
        if group.shadow.is_some() {
            // A shadow that follows is drawn by the processor's renderer, with the finished picture over it, and the card lays
            // that picture, cut to the group's clip (`docs/spec/render-command.md`, section 4.7, rule 6).
            let layer = draw_cpu::shadowed_group_layer(plan.width, plan.height, group, around)?;
            return self.push_layer(plan, target, scissor, layer);
        }
        if let Some(turn) = group.turn.filter(|turn| !turn.is_none()) {
            return self.plan_turned(plan, target, scissor, group, map, opacity, turn.about(map));
        }
        let filtered = !group.filter.is_none();
        if opacity == 1.0 && !filtered {
            return self.plan_marks(plan, target, scissor, &group.marks, map);
        }
        let mut layer = plan.layers.len();
        plan.layers.push(self.layer(plan.width, plan.height));
        if filtered {
            // The blur reaches across the clip, which cuts the picture only after it (section 10.7).
            self.plan_marks(plan, Target::Layer(layer), [0, 0, plan.width, plan.height], &group.marks, map)?;
            layer = self.plan_filter(plan, layer, group.filter, map);
        } else {
            self.plan_marks(plan, Target::Layer(layer), scissor, &group.marks, map)?;
        }
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        let corners = (0.0, 0.0, plan.width as f32, plan.height as f32);
        let faded = opacity as f32;
        push_quad(&mut plan.floats, plan.width, plan.height, corners, [0.0, 0.0], [faded; 4]);
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Layer(layer),
        });
        Ok(())
    }

    /// The draws of a group turned in space (section 10.6): its marks with `map` into a texture of its own, cleared to
    /// transparent, which one quad over the frame lays through the inverse of `turn`, with the opacity. Nothing is drawn
    /// when the turn does not draw the group (`render-command.md` section 4.5, rule 10).
    #[allow(clippy::too_many_arguments)]
    fn plan_turned(
        &mut self,
        plan: &mut Planned,
        target: Target,
        scissor: [u32; 4],
        group: &Group,
        map: Map,
        opacity: f64,
        turn: Turn,
    ) -> Result<(), Error> {
        if !turn.draws(map) {
            return Ok(());
        }
        let Some(inverse) = turn.inverse() else {
            return Ok(());
        };
        let sin_a = turn.rotate_x.to_radians().sin();
        let (sin_b, cos_b) = turn.rotate_y.to_radians().sin_cos();
        let after = [turn.origin.0, turn.origin.1, opacity];
        let mut numbers = [0.0_f32; 20];
        for (row, values) in inverse.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                numbers[row * 4 + column] = *value as f32;
            }
            numbers[row * 4 + 3] = after[row] as f32;
        }
        numbers[12..15].copy_from_slice(&[sin_b as f32, (-sin_a * cos_b) as f32, turn.perspective as f32]);
        numbers[16] = plan.width as f32;
        numbers[17] = plan.height as f32;
        let mut layer = plan.layers.len();
        plan.layers.push(self.layer(plan.width, plan.height));
        let whole = [0, 0, plan.width, plan.height];
        self.plan_marks(plan, Target::Layer(layer), whole, &group.marks, map)?;
        if !group.filter.is_none() {
            layer = self.plan_filter(plan, layer, group.filter, map);
        }
        let turned = self.turn_numbers(&plan.layers[layer], &numbers);
        plan.layers[layer].turned = Some(turned);
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        let corners = (0.0, 0.0, plan.width as f32, plan.height as f32);
        push_quad(&mut plan.floats, plan.width, plan.height, corners, [0.0, 0.0], [1.0; 4]);
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Turned(layer),
        });
        Ok(())
    }

    /// The draw of a ring of a border (`docs/spec/box-edges.md`, section 7): the processor's coverage of the mapped ring as a
    /// mask, through which the ring's colour, premultiplied by its alpha, is blended.
    fn plan_ring(&mut self, plan: &mut Planned, target: Target, scissor: [u32; 4], mark: &RingMark, map: Map) -> Result<(), Error> {
        let key = self.masks.prepare_shape(
            self.card.device(),
            self.card.queue(),
            &self.glyph_layout,
            (mark.ring, None),
            map,
            (plan.width, plan.height),
        )?;
        let Some(placed) = self.masks.placed(&key) else {
            return Ok(());
        };
        let left = -placed.origin_x as f32;
        let top = -placed.origin_y as f32;
        let corners = (left, top, left + placed.width as f32, top + placed.height as f32);
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        push_quad(
            &mut plan.floats,
            plan.width,
            plan.height,
            corners,
            [left, top],
            premultiplied(mark.colour),
        );
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Glyph(key),
        });
        Ok(())
    }

    /// The draws of marks cut to a rounded rectangle (`docs/spec/box-edges.md`, section 7). A shape of whole pixels under a move
    /// by whole pixels cuts the scissor. Any other shape draws the marks into a texture of their own, keeps what lies inside
    /// the shape's mask, and lays the texture over `target`, both in the mask's box.
    fn plan_cut(&mut self, plan: &mut Planned, target: Target, scissor: [u32; 4], cut: &Cut, map: Map) -> Result<(), Error> {
        if let (Some((across, down)), Some(rect)) = (map.whole_move(), cut.shape.whole_rect()) {
            let moved = Rect {
                left: (i64::from(rect.left) + across).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                top: (i64::from(rect.top) + down).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                ..rect
            };
            return match self::cut(scissor, &moved, plan.width, plan.height) {
                Some(scissor) => self.plan_marks(plan, target, scissor, &cut.marks, map),
                None => Ok(()),
            };
        }
        let shape = Ring {
            outer: cut.shape,
            inner: None,
        };
        let key = self.masks.prepare_shape(
            self.card.device(),
            self.card.queue(),
            &self.glyph_layout,
            (shape, cut.own),
            map,
            (plan.width, plan.height),
        )?;
        let Some(placed) = self.masks.placed(&key) else {
            return Ok(());
        };
        let held = Rect {
            left: -placed.origin_x as i32,
            top: -placed.origin_y as i32,
            width: placed.width,
            height: placed.height,
        };
        let Some(scissor) = self::cut(scissor, &held, plan.width, plan.height) else {
            return Ok(());
        };
        let corners = (
            held.left as f32,
            held.top as f32,
            (held.left + held.width as i32) as f32,
            (held.top + held.height as i32) as f32,
        );
        let layer = plan.layers.len();
        plan.layers.push(self.layer(plan.width, plan.height));
        self.plan_marks(plan, Target::Layer(layer), scissor, &cut.marks, map)?;
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        push_quad(&mut plan.floats, plan.width, plan.height, corners, [corners.0, corners.1], [1.0; 4]);
        plan.draws.push(Draw {
            target: Target::Layer(layer),
            scissor,
            vertices: first..first + CORNERS,
            source: Source::KeepInside(key),
        });
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        push_quad(&mut plan.floats, plan.width, plan.height, corners, [0.0, 0.0], [1.0; 4]);
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Layer(layer),
        });
        Ok(())
    }

    /// The passes of the blur and brightness of a group whose marks are drawn in the texture `layer`, drawn with `map`
    /// (section 10.7): a pass across and a pass down that also brightens, or with no blur one pass that only brightens, each
    /// into a new texture. Gives the texture of the last pass.
    fn plan_filter(&mut self, plan: &mut Planned, layer: usize, filter: Filter, map: Map) -> usize {
        let weights = blur_weights(filter.sigma(map), plan.width.max(plan.height));
        let reach = (weights.len() / 2) as f32;
        let passes: &[(f32, f32)] = if reach > 0.0 { &[(0.0, 0.0), (1.0, 1.0)] } else { &[(0.0, 1.0)] };
        let weights: Vec<f32> = weights.iter().map(|weight| *weight as f32).collect();
        let mut from = layer;
        for &(direction, brightens) in passes {
            let into = plan.layers.len();
            let mut next = self.layer(plan.width, plan.height);
            next.filtered = Some(self.filter_numbers(
                &plan.layers[from],
                [direction, reach, filter.brightness as f32, brightens],
                &weights,
            ));
            plan.layers.push(next);
            let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
            let corners = (0.0, 0.0, plan.width as f32, plan.height as f32);
            push_quad(&mut plan.floats, plan.width, plan.height, corners, [0.0, 0.0], [1.0; 4]);
            plan.draws.push(Draw {
                target: Target::Layer(into),
                scissor: [0, 0, plan.width, plan.height],
                vertices: first..first + CORNERS,
                source: Source::Filtered(into),
            });
            from = into;
        }
        from
    }

    /// The bind group of a pass of a filter: the texture it reads, the numbers of the pass and the weights.
    fn filter_numbers(&self, from: &Layer, numbers: [f32; 4], weights: &[f32]) -> wgpu::BindGroup {
        let device = self.card.device();
        let buffer = |label: &str, values: &[f32], usage: wgpu::BufferUsages| {
            let bytes: Vec<u8> = values.iter().flat_map(|value| value.to_ne_bytes()).collect();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes.len() as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.card.queue().write_buffer(&buffer, 0, &bytes);
            buffer
        };
        let numbers = buffer("filter pass", &numbers, wgpu::BufferUsages::UNIFORM);
        let weights = buffer("filter weights", weights, wgpu::BufferUsages::STORAGE);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("filtered group"),
            layout: &self.filter_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&from.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: numbers.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: weights.as_entire_binding(),
                },
            ],
        })
    }

    /// The bind group that reads a turned group's texture and the numbers of its turn.
    fn turn_numbers(&self, layer: &Layer, numbers: &[f32; 20]) -> wgpu::BindGroup {
        let device = self.card.device();
        let bytes: Vec<u8> = numbers.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("turn"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.card.queue().write_buffer(&buffer, 0, &bytes);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("turned group"),
            layout: &self.turn_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&layer.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// A texture of the frame's size for a group that fades, and the bind group that reads it.
    fn layer(&self, width: u32, height: u32) -> Layer {
        let device = self.card.device();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("group"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("group"),
            layout: &self.glyph_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Layer {
            texture,
            view,
            bind_group,
            turned: None,
            filtered: None,
            blended: None,
        }
    }

    /// The draw of a fill under a map that is not a move by whole pixels: the processor's coverage of the mapped rectangle as
    /// a mask, through which the fill's colour is blended (section 10.3).
    #[allow(clippy::too_many_arguments)]
    fn plan_mapped_fill(
        &mut self,
        plan: &mut Planned,
        target: Target,
        scissor: [u32; 4],
        rect: Rect,
        colour: Colour,
        map: Map,
    ) -> Result<(), Error> {
        let key = self.masks.prepare_fill(
            self.card.device(),
            self.card.queue(),
            &self.glyph_layout,
            rect,
            map,
            (plan.width, plan.height),
        )?;
        let Some(placed) = self.masks.placed(&key) else {
            return Ok(());
        };
        let left = -placed.origin_x as f32;
        let top = -placed.origin_y as f32;
        let corners = (left, top, left + placed.width as f32, top + placed.height as f32);
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        push_quad(
            &mut plan.floats,
            plan.width,
            plan.height,
            corners,
            [left, top],
            colour_floats(colour),
        );
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Glyph(key),
        });
        Ok(())
    }

    /// The quads of a paragraph of text, each clipped to the paragraph's clip: its shadows first, the last listed first, so
    /// that the first listed is on top (`docs/spec/text.md`, section 6.6, rule 5); then one quad for each glyph that has
    /// coverage, in the order of the runs and of the glyphs in them; then its outline; then the colour pictures of the emoji.
    /// The shadows and the outline are pictures of the frame's size that the processor draws (`docs/spec/card-renderer.md`,
    /// section 9.9), and they make no glyph mask. Under a map that is not a move by whole pixels the glyphs are mapped
    /// (section 10.2) and the shadows, the outline and the emoji are the processor's pictures with the map (section 10.4).
    fn plan_text(&mut self, plan: &mut Planned, target: Target, scissor: [u32; 4], text: &TextMark, map: Option<Map>) -> Result<(), Error> {
        let (width, height) = (plan.width, plan.height);
        let scissor = match &text.clip {
            None => scissor,
            Some(clip) => match cut(scissor, clip, width, height) {
                Some(scissor) => scissor,
                None => return Ok(()),
            },
        };
        let layer_map = map.unwrap_or(Map::IDENTITY);
        for shadow in text.shadows.iter().rev() {
            let layer = shadow_layer(width, height, text, shadow, layer_map)?;
            self.push_layer(plan, target, scissor, layer)?;
        }
        for run in &text.runs {
            let colour = colour_floats(run.colour);
            for glyph in &run.glyphs {
                let (key, (x, y)) = self
                    .masks
                    .prepare(self.card.device(), self.card.queue(), &self.glyph_layout, run, glyph, map)?;
                let Some(placed) = self.masks.placed(&key) else {
                    continue;
                };
                // The mask's pixel of the origin lies on the whole pixel of the origin: the mask's top left is
                // the whole pixel of the origin less the mask's origin inside the mask.
                let left = (x.floor() - placed.origin_x as f64) as f32;
                let top = (y.floor() - placed.origin_y as f64) as f32;
                let right = left + placed.width as f32;
                let bottom = top + placed.height as f32;
                let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
                push_quad(&mut plan.floats, width, height, (left, top, right, bottom), [left, top], colour);
                plan.draws.push(Draw {
                    target,
                    scissor,
                    vertices: first..first + CORNERS,
                    source: Source::Glyph(key),
                });
            }
        }
        if let Some(outline) = &text.outline {
            let layer = outline_layer(width, height, text, outline, layer_map)?;
            self.push_layer(plan, target, scissor, layer)?;
        }
        if map.is_some() {
            if !text.pictures.is_empty() {
                let layer = pictures_layer(width, height, text, layer_map, (0.0, 0.0))?;
                self.push_layer(plan, target, scissor, layer)?;
            }
            return Ok(());
        }
        // The colour pictures of the emoji come after the glyph runs, in the order of the text (section 12.4, point 10).
        for picture in &text.pictures {
            let left = whole_pixel(picture.x) as f32;
            let top = whole_pixel(picture.y) as f32;
            self.push_picture(plan, target, scissor, picture, (left, top))?;
        }
        Ok(())
    }

    /// Draws a layer of premultiplied pixels of the frame's size over the whole frame, in `scissor`, after what is drawn before it.
    fn push_layer(&self, plan: &mut Planned, target: Target, scissor: [u32; 4], layer: Vec<u8>) -> Result<(), Error> {
        let picture = GlyphPicture {
            width: plan.width,
            height: plan.height,
            pixels: Arc::new(layer),
            x: 0.0,
            y: 0.0,
        };
        self.push_picture(plan, target, scissor, &picture, (0.0, 0.0))
    }

    /// Draws a colour picture with its top left corner at the whole pixel `(left, top)`, in `scissor`, after what is drawn
    /// before it. A picture with no pixel draws nothing.
    fn push_picture(
        &self,
        plan: &mut Planned,
        target: Target,
        scissor: [u32; 4],
        picture: &GlyphPicture,
        (left, top): (f32, f32),
    ) -> Result<(), Error> {
        if picture.width == 0 || picture.height == 0 {
            return Ok(());
        }
        let bind_group = self.picture_bind_group(picture)?;
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        let corners = (left, top, left + picture.width as f32, top + picture.height as f32);
        push_quad(&mut plan.floats, plan.width, plan.height, corners, [left, top], [1.0; 4]);
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Picture(plan.pictures.len()),
        });
        plan.pictures.push(bind_group);
        Ok(())
    }

    /// The texture of a colour picture, with its premultiplied pixels as they are, and the bind group that reads it. A
    /// picture with a side above the card's limit is refused, since no texture of it can be made.
    fn picture_bind_group(&self, picture: &GlyphPicture) -> Result<wgpu::BindGroup, Error> {
        let device = self.card.device();
        let limit = device.limits().max_texture_dimension_2d;
        if picture.width > limit || picture.height > limit {
            return Err(Error::new(format!(
                "a picture of {}x{} pixels cannot be drawn on the card: a side is above {limit}",
                picture.width, picture.height
            )));
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("emoji picture"),
            size: wgpu::Extent3d {
                width: picture.width,
                height: picture.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.card.queue().write_texture(
            texture.as_image_copy(),
            &picture.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(picture.width * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        let view = texture.create_view(&Default::default());
        Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("emoji picture"),
            layout: &self.glyph_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        }))
    }
}

impl Renderer for CardRenderer {
    fn name(&self) -> &'static str {
        "card"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        self.draw_marks(&Frame::from_display_list(list))
    }

    fn draws_text(&self) -> bool {
        true
    }

    fn draw_frame(&mut self, frame: &Frame) -> Result<Picture, Error> {
        self.draw_marks(frame)
    }
}

/// The vertices of a frame, the draw calls that paint them, the bind groups of the pictures the calls use, and the textures
/// of the groups that fade.
type Plan = (Vec<f32>, Vec<Draw>, Vec<wgpu::BindGroup>, Vec<Layer>, Vec<f32>);

/// A frame being planned: what `Plan` gathers, and the size of the frame.
struct Planned {
    floats: Vec<f32>,
    draws: Vec<Draw>,
    pictures: Vec<wgpu::BindGroup>,
    layers: Vec<Layer>,
    /// The numbers of the background layers, four floats at a time, in the layout of `paint.wgsl`.
    paint: Vec<f32>,
    width: u32,
    height: u32,
}

/// The texture of a group that fades, its view as a target, and the bind group that reads it.
struct Layer {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    /// For a group turned in space, the bind group of its texture and the numbers of its turn (section 10.6).
    turned: Option<wgpu::BindGroup>,
    /// For a pass of the blur and brightness of a group, the bind group of the texture it reads, its numbers and its weights
    /// (section 10.7).
    filtered: Option<wgpu::BindGroup>,
    /// For a group laid with a blend, the texture what lies under it is copied into, and the bind group that reads the
    /// group's texture and that copy (`docs/spec/blends.md`, section 6).
    blended: Option<(wgpu::Texture, wgpu::BindGroup)>,
}

/// One draw call of a frame: where it draws, the rectangle it is clipped to (left, top, width, height), the vertices of its
/// quad, and what it draws.
struct Draw {
    target: Target,
    scissor: [u32; 4],
    vertices: Range<u32>,
    source: Source,
}

/// What a draw call paints: a fill, a mask (of a glyph, or of a mapped fill), the texture of a colour picture, by its index in
/// the frame, or the texture of a group that fades, by its index in the frame.
enum Source {
    Fill,
    Glyph(MaskKey),
    Picture(usize),
    Layer(usize),
    /// A layer of a background, whose numbers the quad's origin points to.
    Paint,
    /// The texture of a group turned in space, laid through its turn (section 10.6).
    Turned(usize),
    /// A mask that keeps a cut's texture only inside its shape.
    KeepInside(MaskKey),
    /// A pass of the blur and brightness of a group, written into the texture of that index (section 10.7).
    Filtered(usize),
    /// The texture of a group laid with a blend over a copy of what is under it, written in place (`blends.md`, section 6).
    Blended(usize),
}

/// Begins a render pass on a view, with what it starts from.
fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("frame"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// The part of a scissor (left, top, width, height) that a clip holds, inside the frame; `None` when it holds no pixel.
fn cut([x, y, w, h]: [u32; 4], clip: &Rect, width: u32, height: u32) -> Option<[u32; 4]> {
    let visible = clip.visible(width, height)?;
    let left = x.max(visible.left);
    let top = y.max(visible.top);
    let right = (x + w).min(visible.right);
    let bottom = (y + h).min(visible.bottom);
    (left < right && top < bottom).then_some([left, top, right - left, bottom - top])
}

/// The whole pixel nearest to a coordinate, halves up.
fn whole_pixel(value: f32) -> i64 {
    (f64::from(value) + 0.5).floor() as i64
}

/// Adds one quad for each layer of a background moved by whole pixels, the last listed first, over the moved rectangle clipped
/// to the frame, with the layer's numbers (section 11). A tile with no size draws nothing.
fn push_background(plan: &mut Planned, target: Target, scissor: [u32; 4], background: &Background, across: i64, down: i64) {
    let rect = background.rect;
    let moved = Rect {
        left: (i64::from(rect.left) + across).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        top: (i64::from(rect.top) + down).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        ..rect
    };
    let Some(visible) = moved.visible(plan.width, plan.height) else {
        return;
    };
    let corners = (visible.left as f32, visible.top as f32, visible.right as f32, visible.bottom as f32);
    for layer in background.layers.iter().rev() {
        let resolved = layer.resolve(f64::from(rect.width), f64::from(rect.height));
        if !(resolved.width > 0.0 && resolved.height > 0.0) {
            continue;
        }
        let start = (plan.paint.len() / 4) as f32;
        let (kind, shape, half) = match resolved.gradient {
            Gradient::Flat => (0.0, [0.0; 4], 0.0),
            Gradient::Linear { sin, cos, cx, cy, half } => (1.0, [sin, cos, cx, cy], half),
            Gradient::Radial { cx, cy, rx, ry } => (2.0, [cx, cy, rx, ry], 0.0),
        };
        plan.paint.extend_from_slice(&[
            (f64::from(moved.left) + resolved.left) as f32,
            (f64::from(moved.top) + resolved.top) as f32,
            resolved.width as f32,
            resolved.height as f32,
            if resolved.repeat { 1.0 } else { 0.0 },
            kind,
            resolved.stops.len() as f32,
            0.0,
        ]);
        plan.paint.extend(shape.map(|value| value as f32));
        plan.paint.extend_from_slice(&[half as f32, 0.0, 0.0, 0.0]);
        for (place, colour) in &resolved.stops {
            plan.paint.extend_from_slice(&[*place as f32, 0.0, 0.0, 0.0]);
            plan.paint.extend(colour.map(|value| value as f32));
        }
        let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
        push_quad(&mut plan.floats, plan.width, plan.height, corners, [start, 0.0], [0.0; 4]);
        plan.draws.push(Draw {
            target,
            scissor,
            vertices: first..first + CORNERS,
            source: Source::Paint,
        });
    }
}

/// Adds the quad of a fill, clipped to the frame, and its draw call, cut to `scissor`.
fn push_fill(plan: &mut Planned, target: Target, scissor: [u32; 4], rect: Rect, colour: Colour) {
    let Some(visible) = rect.visible(plan.width, plan.height) else {
        return;
    };
    let first = (plan.floats.len() / VERTEX_FLOATS) as u32;
    let corners = (visible.left as f32, visible.top as f32, visible.right as f32, visible.bottom as f32);
    push_quad(
        &mut plan.floats,
        plan.width,
        plan.height,
        corners,
        [0.0, 0.0],
        colour_floats(colour),
    );
    plan.draws.push(Draw {
        target,
        scissor,
        vertices: first..first + CORNERS,
        source: Source::Fill,
    });
}

/// Adds the six vertices of a quad whose corners lie on the given pixel boundaries of the frame, each with its
/// origin and its colour. A corner in pixels from the top left is placed in clip space: the left edge of the frame
/// is -1, the right edge 1, the top 1 and the bottom -1.
fn push_quad(
    floats: &mut Vec<f32>,
    width: u32,
    height: u32,
    (left, top, right, bottom): (f32, f32, f32, f32),
    origin: [f32; 2],
    colour: [f32; 4],
) {
    let (frame_width, frame_height) = (width as f32, height as f32);
    let place = |x: f32, y: f32| [x / frame_width * 2.0 - 1.0, 1.0 - y / frame_height * 2.0];
    for (x, y) in [
        (left, top),
        (right, top),
        (right, bottom),
        (left, top),
        (right, bottom),
        (left, bottom),
    ] {
        floats.extend_from_slice(&place(x, y));
        floats.extend_from_slice(&origin);
        floats.extend_from_slice(&colour);
    }
}

/// A colour with its alpha as the four numbers of a vertex, each from 0 to 1, premultiplied by its alpha.
fn premultiplied(colour: Rgba) -> [f32; 4] {
    let alpha = f32::from(colour.alpha) / 255.0;
    [
        f32::from(colour.red) / 255.0 * alpha,
        f32::from(colour.green) / 255.0 * alpha,
        f32::from(colour.blue) / 255.0 * alpha,
        alpha,
    ]
}

/// A colour as the four numbers of a vertex, each from 0 to 1, with alpha 1.
fn colour_floats(colour: Colour) -> [f32; 4] {
    [
        f32::from(colour.red) / 255.0,
        f32::from(colour.green) / 255.0,
        f32::from(colour.blue) / 255.0,
        1.0,
    ]
}
