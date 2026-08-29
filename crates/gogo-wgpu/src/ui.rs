//! The gogo draw list on wgpu: one instanced draw of UI quads per clip
//! region. Needed so a whole page costs one buffer write and a handful of
//! draw calls; the page's rects are epx and are scaled to device pixels
//! here (`gogo` never sees a device pixel). Limit (G5): `Text` and
//! `Image` commands are counted and skipped until the glyph/image atlas
//! lands (G6); gradients use their first and last stop only.

use gogo::draw::{Draw, GradientKind};
use gogo::layout::Rect;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Quad {
    rect: [f32; 4],
    color: [f32; 4],
    border_color: [f32; 4],
    /// corner, border, kind, angle.
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Screen {
    size: [f32; 2],
    _pad: [f32; 2],
}

struct Batch {
    first: u32,
    count: u32,
    /// Scissor in device pixels (x, y, w, h); `None` = the whole target.
    scissor: Option<[u32; 4]>,
}

pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    quad_buffer: wgpu::Buffer,
    quad_capacity: usize,
    quads: Vec<Quad>,
    batches: Vec<Batch>,
    screen: (u32, u32),
    /// Commands the G5 renderer cannot draw yet (text, images).
    pub skipped: u32,
    /// Draw calls recorded by the last `render`.
    pub draw_calls: u32,
}

const KIND_RECT: f32 = 0.0;
const KIND_CIRCLE: f32 = 1.0;
const KIND_LINEAR: f32 = 2.0;
const KIND_RADIAL: f32 = 3.0;

impl UiRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> UiRenderer {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("ui"), source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("ui screen"), size: std::mem::size_of::<Screen>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("ui"), layout: &layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: screen_buffer.as_entire_binding() }] });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ui"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: target_format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let quad_capacity = 256;
        let quad_buffer = Self::make_quad_buffer(device, quad_capacity);
        UiRenderer { pipeline, screen_buffer, bind_group, quad_buffer, quad_capacity, quads: Vec::new(), batches: Vec::new(), screen: (1, 1), skipped: 0, draw_calls: 0 }
    }

    fn make_quad_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor { label: Some("ui quads"), size: (capacity * std::mem::size_of::<Quad>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
    }

    /// Turns the draw list into instances (epx × `epx` = device px) and
    /// uploads them. `screen` = the target size in device pixels.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, draws: &[Draw], epx: f32, screen: (u32, u32)) {
        self.quads.clear();
        self.batches.clear();
        self.skipped = 0;
        self.screen = screen;
        let px = |r: &Rect| [r.x * epx, r.y * epx, r.w * epx, r.h * epx];
        let mut clips: Vec<[u32; 4]> = Vec::new();
        let mut batch_first = 0u32;
        let mut current: Option<[u32; 4]> = None;
        for d in draws {
            match d {
                Draw::Quad { rect, color, corner, border, border_color } => {
                    self.quads.push(Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [corner * epx, border * epx, KIND_RECT, 0.0] });
                }
                Draw::Circle { rect, color, border, border_color } => {
                    self.quads.push(Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [0.0, border * epx, KIND_CIRCLE, 0.0] });
                }
                Draw::Gradient { rect, kind, from, to, angle } => {
                    let k = if *kind == GradientKind::Radial { KIND_RADIAL } else { KIND_LINEAR };
                    self.quads.push(Quad { rect: px(rect), color: from.to_f32(), border_color: to.to_f32(), params: [0.0, 0.0, k, angle.to_radians()] });
                }
                Draw::Text { .. } | Draw::Image { .. } => self.skipped += 1,
                Draw::ClipPush(rect) => {
                    self.close_batch(&mut batch_first, current);
                    let r = px(rect);
                    let x = r[0].max(0.0).floor() as u32;
                    let y = r[1].max(0.0).floor() as u32;
                    let right = (r[0] + r[2]).min(screen.0 as f32).ceil() as u32;
                    let bottom = (r[1] + r[3]).min(screen.1 as f32).ceil() as u32;
                    let mut clip = [x, y, right.saturating_sub(x), bottom.saturating_sub(y)];
                    if let Some(outer) = current {
                        // Nested clips intersect.
                        let x2 = clip[0].max(outer[0]);
                        let y2 = clip[1].max(outer[1]);
                        let r2 = (clip[0] + clip[2]).min(outer[0] + outer[2]);
                        let b2 = (clip[1] + clip[3]).min(outer[1] + outer[3]);
                        clip = [x2, y2, r2.saturating_sub(x2), b2.saturating_sub(y2)];
                    }
                    clips.push(clip);
                    current = Some(clip);
                }
                Draw::ClipPop => {
                    self.close_batch(&mut batch_first, current);
                    clips.pop();
                    current = clips.last().copied();
                }
            }
        }
        self.close_batch(&mut batch_first, current);

        if self.quads.len() > self.quad_capacity {
            self.quad_capacity = self.quads.len().next_power_of_two();
            self.quad_buffer = Self::make_quad_buffer(device, self.quad_capacity);
        }
        if !self.quads.is_empty() {
            queue.write_buffer(&self.quad_buffer, 0, bytemuck::cast_slice(&self.quads));
        }
        queue.write_buffer(&self.screen_buffer, 0, bytemuck::bytes_of(&Screen { size: [screen.0 as f32, screen.1 as f32], _pad: [0.0; 2] }));
    }

    fn close_batch(&mut self, first: &mut u32, scissor: Option<[u32; 4]>) {
        let end = self.quads.len() as u32;
        if end > *first {
            self.batches.push(Batch { first: *first, count: end - *first, scissor });
        }
        *first = end;
    }

    /// Records the quads into `pass` (the caller owns the pass).
    pub fn render(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_calls = 0;
        if self.batches.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
        for b in &self.batches {
            match b.scissor {
                Some([x, y, w, h]) if w > 0 && h > 0 => pass.set_scissor_rect(x, y, w, h),
                Some(_) => continue,
                None => pass.set_scissor_rect(0, 0, self.screen.0, self.screen.1),
            }
            pass.draw(0..6, b.first..b.first + b.count);
            self.draw_calls += 1;
        }
        pass.set_scissor_rect(0, 0, self.screen.0, self.screen.1);
    }

    pub fn quad_count(&self) -> usize {
        self.quads.len()
    }
}
