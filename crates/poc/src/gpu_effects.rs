//! Turned and opening groups on the card: a pass that reads the flat picture of a group and
//! writes it turned in space, or opened, into a texture as large as the frame. See
//! `docs/spec/native-render-pipeline.md`, section 8. The CPU painter defines the result.

use crate::model::{Opening, Shader};
use vello::kurbo::Rect;
use vello::wgpu;

/// What every pass is given: the flat picture, the numbers of the effect, and the functions an
/// opening may call.
const SHARED: &str = r"
struct Params {
    // Takes a pixel of the frame to its place on the flat picture, for a turn.
    back: mat3x3<f32>,
    // The element's box in the frame: left, top, width, height.
    around: vec4<f32>,
    // Where the flat picture's top left corner lies in the frame.
    origin: vec2<f32>,
    progress: f32,
}

@group(0) @binding(0) var flat: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;

// One triangle that covers the frame.
@vertex
fn corner(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

// A pixel of the flat picture, its colour multiplied by its alpha; nothing outside it.
fn texel(at: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(flat));
    if at.x < 0 || at.y < 0 || at.x >= size.x || at.y >= size.y {
        return vec4<f32>(0.0);
    }
    let colour = textureLoad(flat, at, 0);
    return vec4<f32>(colour.rgb * colour.a, colour.a);
}

// The flat picture at a spot counted in its pixels, from between the four pixels around it.
fn between(spot: vec2<f32>) -> vec4<f32> {
    let corner = spot - vec2<f32>(0.5);
    let low = floor(corner);
    let share = corner - low;
    let at = vec2<i32>(low);
    let upper = mix(texel(at), texel(at + vec2<i32>(1, 0)), share.x);
    let lower = mix(texel(at + vec2<i32>(0, 1)), texel(at + vec2<i32>(1, 1)), share.x);
    return mix(upper, lower, share.y);
}

// The element's flat picture at a place of its box, 0 to 1 across and down.
fn picture(place: vec2<f32>) -> vec4<f32> {
    return between(params.around.xy + place * params.around.zw - params.origin);
}

// The share shown by an opening that is `progress` open, `place` of the way from where it
// begins to where it ends. The edge is a tenth wide.
fn open_at(progress: f32, place: f32) -> f32 {
    return clamp((progress * 1.1 - place) / 0.1, 0.0, 1.0);
}

// A colour as the frame takes a texture: its alpha apart from the colour.
fn handed(colour: vec4<f32>) -> vec4<f32> {
    let c = clamp(colour, vec4<f32>(0.0), vec4<f32>(1.0));
    if c.a <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(min(c.rgb / c.a, vec3<f32>(1.0)), c.a);
}
";

const TURN: &str = r"
@fragment
fn pixel(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let seen = params.back * vec3<f32>(at.xy, 1.0);
    return handed(between(seen.xy / seen.z - params.origin));
}
";

/// The pass of an opening: `effect` is the opening's own function.
const OPENING: &str = r"
@fragment
fn pixel(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let place = (at.xy - params.around.xy) / params.around.zw;
    return handed(effect(place, params.around.zw, params.progress));
}
";

const WIPE: &str = r"
fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32> {
    return picture(place) * open_at(progress, place.x);
}
";

const IRIS: &str = r"
fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32> {
    return picture(place) * open_at(progress, length((place - vec2<f32>(0.5)) * size) / (length(size) * 0.5));
}
";

/// The bytes of `Params` as the shader reads them.
const PARAMS_BYTES: usize = 80;

/// Which pass a group goes through.
#[derive(Clone, Copy)]
pub enum Pass {
    Turn,
    Opening(Opening),
}

/// The numbers of one effect.
pub struct Params {
    /// For a turn: takes a pixel of the frame to its place on the flat picture, as rows.
    pub back: [[f64; 3]; 3],
    /// The element's box in the frame.
    pub around: Rect,
    /// Where the flat picture's top left corner lies in the frame.
    pub origin: (f64, f64),
    pub progress: f64,
}

impl Params {
    fn bytes(&self) -> [u8; PARAMS_BYTES] {
        let mut numbers = [0.0_f32; PARAMS_BYTES / 4];
        // A matrix is read column by column, each column four numbers long.
        for column in 0..3 {
            for row in 0..3 {
                numbers[column * 4 + row] = self.back[row][column] as f32;
            }
        }
        let around = [self.around.x0, self.around.y0, self.around.width(), self.around.height()];
        for (place, value) in around.iter().enumerate() {
            numbers[12 + place] = *value as f32;
        }
        numbers[16] = self.origin.0 as f32;
        numbers[17] = self.origin.1 as f32;
        numbers[18] = self.progress as f32;
        let mut bytes = [0_u8; PARAMS_BYTES];
        for (place, number) in numbers.iter().enumerate() {
            bytes[place * 4..place * 4 + 4].copy_from_slice(&number.to_le_bytes());
        }
        bytes
    }
}

/// The passes, made once for a renderer.
pub struct Effects {
    layout: wgpu::BindGroupLayout,
    turn: wgpu::RenderPipeline,
    wipe: wgpu::RenderPipeline,
    iris: wgpu::RenderPipeline,
    /// The openings the composition's authors wrote, in the composition's order.
    written: Vec<wgpu::RenderPipeline>,
}

/// The textures of one group: its flat picture, and the result as large as the frame.
pub struct Slot {
    flat: wgpu::Texture,
    flat_view: wgpu::TextureView,
    pub out: wgpu::Texture,
    out_view: wgpu::TextureView,
    params: wgpu::Buffer,
    group: wgpu::BindGroup,
}

fn texture(device: &wgpu::Device, label: &str, (width, height): (u32, u32), usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage,
        view_formats: &[],
    })
}

impl Effects {
    /// Makes the passes. `written` are the composition's own openings; one the card's compiler
    /// does not take is an error that names its file.
    pub fn new(device: &wgpu::Device, written: &[Shader]) -> Result<Effects, String> {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effect"),
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
        let ours = |name: &str, body: &str| {
            pipeline(device, &layout, name, &format!("{SHARED}{body}")).map_err(|error| format!("the pass `{name}` was not taken: {error}"))
        };
        let turn = ours("turn", TURN)?;
        let wipe = ours("wipe", &format!("{WIPE}{OPENING}"))?;
        let iris = ours("iris", &format!("{IRIS}{OPENING}"))?;
        let written = written
            .iter()
            .map(|shader| {
                // The author's text comes first, so the compiler counts its lines as the file does.
                pipeline(device, &layout, &shader.name, &format!("{}\n{SHARED}{OPENING}", shader.source))
                    .map_err(|error| format!("the transition `{}` is not a shader the card takes: {error}", shader.name))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Effects {
            layout,
            turn,
            wipe,
            iris,
            written,
        })
    }

    fn group(&self, device: &wgpu::Device, flat: &wgpu::TextureView, params: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(flat),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
            ],
        })
    }

    /// The textures of one group, for a frame of `frame` pixels.
    pub fn slot(&self, device: &wgpu::Device, frame: (u32, u32)) -> Slot {
        // The drawing library writes the flat picture; this pass reads it.
        let flat_usage = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let flat = texture(device, "group, flat", (64, 64), flat_usage);
        let flat_view = flat.create_view(&wgpu::TextureViewDescriptor::default());
        // This pass writes the result; the drawing library copies it to put it on the frame.
        let out_usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING;
        let out = texture(device, "group, with its effect", frame, out_usage);
        let out_view = out.create_view(&wgpu::TextureViewDescriptor::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effect"),
            size: PARAMS_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = self.group(device, &flat_view, &params);
        Slot {
            flat,
            flat_view,
            out,
            out_view,
            params,
            group,
        }
    }

    /// Makes the flat texture of `slot` at least `needed` pixels large, and gives its view and
    /// its size. It only grows, so a group that moves does not ask for a new one each frame.
    pub fn flat<'a>(&self, device: &wgpu::Device, slot: &'a mut Slot, needed: (u32, u32)) -> (&'a wgpu::TextureView, (u32, u32)) {
        let (width, height) = (slot.flat.width(), slot.flat.height());
        if needed.0 > width || needed.1 > height {
            let size = (needed.0.max(width).next_multiple_of(64), needed.1.max(height).next_multiple_of(64));
            slot.flat = texture(
                device,
                "group, flat",
                size,
                wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            slot.flat_view = slot.flat.create_view(&wgpu::TextureViewDescriptor::default());
            slot.group = self.group(device, &slot.flat_view, &slot.params);
        }
        (&slot.flat_view, (slot.flat.width(), slot.flat.height()))
    }

    /// Writes the flat picture of `slot`, through `pass`, into its result, inside `part` of
    /// the frame (left, top, width, height). The rest of the result is left clear.
    pub fn run(&self, device: &wgpu::Device, queue: &wgpu::Queue, slot: &Slot, pass: Pass, params: &Params, part: (u32, u32, u32, u32)) {
        let pipeline = match pass {
            Pass::Turn => &self.turn,
            Pass::Opening(Opening::Wipe) => &self.wipe,
            Pass::Opening(Opening::Iris) => &self.iris,
            Pass::Opening(Opening::Cut) => unreachable!("a hard edge is a cut of the box, not an opening"),
            Pass::Opening(Opening::Shader(written)) => &self.written[written],
        };
        queue.write_buffer(&slot.params, 0, &params.bytes());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("effect"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &slot.out_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &slot.group, &[]);
            pass.set_scissor_rect(part.0, part.1, part.2, part.3);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
}

/// A pass from its text, or what the card's compiler says against it.
fn pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, name: &str, text: &str) -> Result<wgpu::RenderPipeline, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(name),
        source: wgpu::ShaderSource::Wgsl(text.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("effect"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(name),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("corner"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("pixel"),
            targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    match pollster::block_on(scope.pop()) {
        Some(error) => Err(error.to_string()),
        None => Ok(pipeline),
    }
}
