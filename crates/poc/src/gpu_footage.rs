//! Footage on the card: the YUV planes of a picture go in, coloured RGBA comes out, with the
//! arithmetic of `docs/spec/native-render-pipeline.md`, section 4. The CPU path in `yuv` and
//! `style::Grade` defines the result; this one agrees with it to within a level.

use crate::lut::Lut;
use crate::style::ColorOp;
use crate::yuv;
use vello::wgpu;

/// The most steps a colour filter may have to be applied here.
pub const MOST_STEPS: usize = 8;

/// The bytes of the filter as the shader reads it: a count, then two numbers for each step.
const FILTER_BYTES: usize = 16 + MOST_STEPS * 16;

const SHADER: &str = r"
struct Filter {
    steps: u32,
    // 1 when the colour goes through the table first.
    tabled: u32,
    // For each step: x says what it does, y by how much.
    step: array<vec4<f32>, 8>,
}

@group(0) @binding(0) var luma: texture_2d<f32>;
@group(0) @binding(1) var blue: texture_2d<f32>;
@group(0) @binding(2) var red: texture_2d<f32>;
@group(0) @binding(3) var<uniform> filter_steps: Filter;
@group(0) @binding(4) var table: texture_3d<f32>;

// The colour the table gives for a colour: between the eight points around it, along red
// first, then green, then blue.
fn tabled(c: vec3<f32>) -> vec3<f32> {
    let last = i32(textureDimensions(table).x) - 1;
    let place = c * f32(last);
    let below = clamp(vec3<i32>(floor(place)), vec3<i32>(0), vec3<i32>(last - 1));
    let share = place - vec3<f32>(below);
    let near = mix(
        mix(textureLoad(table, below, 0).rgb, textureLoad(table, below + vec3<i32>(1, 0, 0), 0).rgb, share.x),
        mix(textureLoad(table, below + vec3<i32>(0, 1, 0), 0).rgb, textureLoad(table, below + vec3<i32>(1, 1, 0), 0).rgb, share.x),
        share.y,
    );
    let far = mix(
        mix(textureLoad(table, below + vec3<i32>(0, 0, 1), 0).rgb, textureLoad(table, below + vec3<i32>(1, 0, 1), 0).rgb, share.x),
        mix(textureLoad(table, below + vec3<i32>(0, 1, 1), 0).rgb, textureLoad(table, below + vec3<i32>(1, 1, 1), 0).rgb, share.x),
        share.y,
    );
    // A whole level again before the filter sees it, as on the CPU.
    return round(clamp(mix(near, far, share.z), vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0) / 255.0;
}

// One triangle that covers the picture.
@vertex
fn corner(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

// The chroma of a pixel, spread between the samples around it. A sample stands at the left
// column of its block of two by two pixels, between the block's rows.
fn chroma(plane: texture_2d<f32>, at: vec2<i32>) -> f32 {
    let last = vec2<i32>(textureDimensions(plane)) - vec2<i32>(1, 1);
    let block = at / 2;
    let next = min(block.x + 1, last.x);
    let other = clamp(block.y + select(1, -1, at.y % 2 == 0), 0, last.y);
    let across = select(0.0, 0.5, at.x % 2 == 1);
    let near = mix(textureLoad(plane, vec2<i32>(block.x, block.y), 0).r, textureLoad(plane, vec2<i32>(next, block.y), 0).r, across);
    let far = mix(textureLoad(plane, vec2<i32>(block.x, other), 0).r, textureLoad(plane, vec2<i32>(next, other), 0).r, across);
    return near * 0.75 + far * 0.25;
}

@fragment
fn pixel(@builtin(position) place: vec4<f32>) -> @location(0) vec4<f32> {
    let at = vec2<i32>(place.xy);
    let y = textureLoad(luma, at, 0).r * 255.0 - 16.0;
    let cb = chroma(blue, at) * 255.0 - 128.0;
    let cr = chroma(red, at) * 255.0 - 128.0;
    var c = vec3<f32>(
        1.164384 * y + 1.792741 * cr,
        1.164384 * y - 0.213249 * cb - 0.532909 * cr,
        1.164384 * y + 2.112402 * cb,
    );
    // The colour is a whole level before the filter sees it, as on the CPU.
    c = round(clamp(c, vec3<f32>(0.0), vec3<f32>(255.0))) / 255.0;
    if filter_steps.tabled == 1u {
        c = tabled(c);
    }
    for (var index = 0u; index < filter_steps.steps; index++) {
        let step = filter_steps.step[index];
        if step.x == 0.0 {
            c = c * step.y;
        } else if step.x == 1.0 {
            c = (c - vec3<f32>(0.5)) * step.y + vec3<f32>(0.5);
        } else {
            let grey = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
            c = vec3<f32>(grey) + (c - vec3<f32>(grey)) * step.y;
        }
        c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(c, 1.0);
}
";

/// The program that turns planes into RGBA, made once for a renderer.
pub struct Converter {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

/// The planes of one video on the card, and where their RGBA goes.
pub struct Planes {
    /// The picture as the decoder hands it over: all of Y, then Cb, then Cr.
    pub data: Vec<u8>,
    pub filter: Vec<ColorOp>,
    width: u32,
    height: u32,
    textures: [wgpu::Texture; 3],
    group: wgpu::BindGroup,
    target: wgpu::TextureView,
}

/// The table a filter begins with, when it has one, and the steps after it.
fn parts(filter: &[ColorOp]) -> (Option<&Lut>, &[ColorOp]) {
    match filter {
        [ColorOp::Table(table), steps @ ..] => (Some(table), steps),
        steps => (None, steps),
    }
}

/// Whether a filter can be applied here.
pub fn takes(filter: &[ColorOp]) -> bool {
    let steps = parts(filter).1;
    steps.len() <= MOST_STEPS && !steps.iter().any(|step| matches!(step, ColorOp::Table(_)))
}

fn filter_bytes(filter: &[ColorOp]) -> [u8; FILTER_BYTES] {
    let (table, steps) = parts(filter);
    let mut bytes = [0_u8; FILTER_BYTES];
    bytes[..4].copy_from_slice(&(steps.len() as u32).to_le_bytes());
    bytes[4..8].copy_from_slice(&u32::from(table.is_some()).to_le_bytes());
    for (index, op) in steps.iter().enumerate() {
        // Saturation and greyness are one step: keep a share of the colour, the rest its grey.
        let (kind, amount) = match *op {
            ColorOp::Brightness(amount) => (0.0_f32, amount),
            ColorOp::Contrast(amount) => (1.0, amount),
            ColorOp::Saturate(amount) => (2.0, amount),
            ColorOp::Grayscale(amount) => (2.0, 1.0 - amount.clamp(0.0, 1.0)),
            ColorOp::Table(_) => continue,
        };
        let at = 16 + index * 16;
        bytes[at..at + 4].copy_from_slice(&kind.to_le_bytes());
        bytes[at + 4..at + 8].copy_from_slice(&amount.to_le_bytes());
    }
    bytes
}

impl Converter {
    pub fn new(device: &wgpu::Device) -> Converter {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("footage"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let plane = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("footage"),
            entries: &[
                plane(0),
                plane(1),
                plane(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("footage"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("footage"),
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
        Converter { pipeline, layout }
    }

    /// The planes of a video of `width` by `height` whose RGBA goes to `target`.
    pub fn planes(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        (width, height): (u32, u32),
        filter: &[ColorOp],
    ) -> Planes {
        let plane = |label: &str, width: u32, height: u32| {
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
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures = [
            plane("footage luma", width, height),
            plane("footage blue", width / 2, height / 2),
            plane("footage red", width / 2, height / 2),
        ];
        let steps = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("footage filter"),
            size: FILTER_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&steps, 0, &filter_bytes(filter));
        // The filter's table, or the smallest table there is when it has none: the shader
        // is given one either way and does not read the second.
        let table = parts(filter).0;
        let side = table.map_or(2, |table| table.size() as u32);
        let points = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("footage table"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: side,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        if let Some(table) = table {
            queue.write_texture(
                points.as_image_copy(),
                &table.bytes(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 16),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: side,
                },
            );
        }
        let points = points.create_view(&wgpu::TextureViewDescriptor::default());
        let views = textures
            .each_ref()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("footage"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: steps.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&points),
                },
            ],
        });
        Planes {
            data: vec![0; yuv::size(width as usize, height as usize)],
            filter: filter.to_vec(),
            width,
            height,
            textures,
            group,
            target: target.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }

    /// Sends the planes to the card and has it make the RGBA picture.
    pub fn convert(&self, device: &wgpu::Device, queue: &wgpu::Queue, planes: &Planes) {
        let (width, height) = (planes.width, planes.height);
        let (luma, chroma) = planes.data.split_at((width * height) as usize);
        let (blue, red) = chroma.split_at((width / 2 * (height / 2)) as usize);
        for ((texture, bytes), (width, height)) in
            planes
                .textures
                .iter()
                .zip([luma, blue, red])
                .zip([(width, height), (width / 2, height / 2), (width / 2, height / 2)])
        {
            queue.write_texture(
                texture.as_image_copy(),
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("footage"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &planes.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &planes.group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
}

const OUT_SHADER: &str = r"
@group(0) @binding(0) var frame: texture_2d<f32>;

@vertex
fn corner(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

// A pixel of the frame as whole levels, as the CPU reads it.
fn levels(at: vec2<i32>) -> vec3<f32> {
    return round(textureLoad(frame, at, 0).rgb * 255.0);
}

@fragment
fn luma(@builtin(position) place: vec4<f32>) -> @location(0) vec4<f32> {
    let c = levels(vec2<i32>(place.xy));
    let y = 16.0 + dot(c, vec3<f32>(0.182586, 0.614231, 0.062007));
    return vec4<f32>(y / 255.0, 0.0, 0.0, 1.0);
}

struct Chroma {
    @location(0) blue: vec4<f32>,
    @location(1) red: vec4<f32>,
}

// One sample for a block of two by two pixels, standing at the block's left column: that
// column counts twice, the columns beside it once, over both rows.
@fragment
fn chroma(@builtin(position) place: vec4<f32>) -> Chroma {
    let block = vec2<i32>(place.xy);
    let middle = block.x * 2;
    let left = max(middle - 1, 0);
    let top = block.y * 2;
    var c = vec3<f32>(0.0);
    for (var row = 0; row < 2; row++) {
        c += levels(vec2<i32>(left, top + row)) + 2.0 * levels(vec2<i32>(middle, top + row)) + levels(vec2<i32>(middle + 1, top + row));
    }
    c = c / 8.0;
    let cb = 128.0 + dot(c, vec3<f32>(-0.100644, -0.338572, 0.439216));
    let cr = 128.0 + dot(c, vec3<f32>(0.439216, -0.398942, -0.040274));
    return Chroma(vec4<f32>(cb / 255.0, 0.0, 0.0, 1.0), vec4<f32>(cr / 255.0, 0.0, 0.0, 1.0));
}
";

/// The program that turns a finished frame into the planes the encoder is given, on the
/// card: `docs/spec/native-render-pipeline.md`, section 2. `yuv::from_rgba` defines the result.
pub struct Outgoing {
    luma: wgpu::RenderPipeline,
    chroma: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    /// Y at full size, then Cb and Cr at half.
    planes: [wgpu::Texture; 3],
    views: [wgpu::TextureView; 3],
    width: u32,
    height: u32,
}

/// A row of a plane as it lies in a buffer the card copies into.
fn padded(width: u32) -> u32 {
    width.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
}

impl Outgoing {
    /// For frames drawn into `frame`, which is `width` by `height`, both even.
    pub fn new(device: &wgpu::Device, frame: &wgpu::TextureView, (width, height): (u32, u32)) -> Outgoing {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("planes out"),
            source: wgpu::ShaderSource::Wgsl(OUT_SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("planes out"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("planes out"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let plane: Option<wgpu::ColorTargetState> = Some(wgpu::TextureFormat::R8Unorm.into());
        let pipeline = |entry: &str, targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("planes out"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("corner"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let texture = |label: &str, width: u32, height: u32| {
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
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let planes = [
            texture("luma out", width, height),
            texture("blue out", width / 2, height / 2),
            texture("red out", width / 2, height / 2),
        ];
        Outgoing {
            luma: pipeline("luma", std::slice::from_ref(&plane)),
            chroma: pipeline("chroma", &[plane.clone(), plane]),
            group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("planes out"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(frame),
                }],
            }),
            views: planes
                .each_ref()
                .map(|plane| plane.create_view(&wgpu::TextureViewDescriptor::default())),
            planes,
            width,
            height,
        }
    }

    /// The bytes of a buffer that holds the three planes as the card copies them.
    pub fn buffer_size(&self) -> u64 {
        u64::from(padded(self.width)) * u64::from(self.height) + 2 * u64::from(padded(self.width / 2)) * u64::from(self.height / 2)
    }

    /// Adds to `encoder` the making of the planes from the frame and their copy into `buffer`.
    pub fn record(&self, encoder: &mut wgpu::CommandEncoder, buffer: &wgpu::Buffer) {
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        for (pipeline, attachments) in [
            (&self.luma, vec![attachment(&self.views[0])]),
            (&self.chroma, vec![attachment(&self.views[1]), attachment(&self.views[2])]),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("planes out"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.draw(0..3, 0..1);
        }
        let mut offset = 0;
        for (plane, (width, height)) in self.planes.iter().zip(self.sizes()) {
            encoder.copy_texture_to_buffer(
                plane.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row: Some(padded(width)),
                        rows_per_image: None,
                    },
                },
                plane.size(),
            );
            offset += u64::from(padded(width)) * u64::from(height);
        }
    }

    fn sizes(&self) -> [(u32, u32); 3] {
        let half = (self.width / 2, self.height / 2);
        [(self.width, self.height), half, half]
    }

    /// Takes the planes out of a buffer the card has filled, without the padding of its rows:
    /// all of Y, then Cb, then Cr.
    pub fn unpack(&self, mapped: &[u8], out: &mut [u8]) {
        let (mut from, mut to) = (0, 0);
        for (width, height) in self.sizes() {
            let (row, step) = (width as usize, padded(width) as usize);
            for line in 0..height as usize {
                out[to..to + row].copy_from_slice(&mapped[from + line * step..from + line * step + row]);
                to += row;
            }
            from += step * height as usize;
        }
    }
}
