//! The gogo draw list on wgpu: one instanced draw of UI quads per (clip,
//! texture) batch. Needed so a whole page costs one buffer write and a
//! handful of draw calls; the page's rects are epx and are scaled to device
//! pixels here (`gogo` never sees a device pixel). Text is shaped by the
//! `text` crate into glyph quads over its coverage atlas (texture 0);
//! images are RGBA textures added by the shell (`add_image`). A gradient
//! arrives as one quad per stop pair (its `band`); a `RotatePush` turns
//! every quad until its pop about the given center.

use crate::tex_types::Tex_Display_Srgb_4xU8_RGBA;
use std::collections::HashMap;

use gogo::draw::{Draw, Fit, GradientKind, TextAlign};
use gogo::layout::Rect;
use gogo::locale::Locale;
use gogo::page::{NO_STR, Page};
use text::TextEngine;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Quad {
    rect: [f32; 4],
    color: [f32; 4],
    border_color: [f32; 4],
    /// corner, border, kind, angle.
    params: [f32; 4],
    /// Glyph/image: u0, v0, u1, v1. Gradient: band start, band end.
    uv: [f32; 4],
    /// sin, cos of the rotation and its center (device px); `[0, 1, 0, 0]` = none.
    rot: [f32; 4],
    /// Arc-wedge mask (GP33): center x, y (device px), from (radians),
    /// span (radians). span < 0 = none; span 0 hides the quad whole.
    wedge: [f32; 4],
}

const NO_ROT: [f32; 4] = [0.0, 1.0, 0.0, 0.0];
const NO_WEDGE: [f32; 4] = [0.0, 0.0, 0.0, -1.0];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Screen {
    size: [f32; 2],
    _pad: [f32; 2],
}

/// Texture slot: 0 = the glyph atlas, 1.. = images in `images` order.
type TexId = u32;
const ATLAS: TexId = 0;

struct Batch {
    first: u32,
    count: u32,
    scissor: Option<[u32; 4]>,
    tex: TexId,
}

struct Texture {
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// One externally-owned image the UI shows (`UiRenderer::external_image`).
struct External {
    /// The Image's `source=` string.
    key: String,
    /// The texture identity last registered; `None` until the first set.
    texture: Option<wgpu::Texture>,
}

/// The handle `UiRenderer::external_image` opens for one image key; the
/// per-frame `set_external_image` takes it, so showing a live texture
/// hashes no string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalImage(u32);

pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    screen_bind: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    quad_buffer: wgpu::Buffer,
    quad_capacity: usize,
    quads: Vec<Quad>,
    batches: Vec<Batch>,
    screen: (u32, u32),
    /// Slot 0 is the atlas (rebuilt when the engine's atlas changes size).
    textures: Vec<Texture>,
    atlas_texture: Option<wgpu::Texture>,
    atlas_size: u32,
    /// Image key (the `source=` string) → texture slot.
    images: HashMap<String, TexId>,
    /// Externally-owned textures (`external_image` / `set_external_image`):
    /// the image key and the texture identity last registered, so a
    /// same-texture frame makes no bind group. Indexed by `ExternalImage`.
    externals: Vec<External>,
    /// Commands with no texture to draw them (a missing image).
    pub skipped: u32,
    pub draw_calls: u32,
}

const KIND_RECT: f32 = 0.0;
const KIND_CIRCLE: f32 = 1.0;
const KIND_LINEAR: f32 = 2.0;
const KIND_RADIAL: f32 = 3.0;
const KIND_GLYPH: f32 = 4.0;
const KIND_CONIC: f32 = 6.0;
const KIND_ARC: f32 = 7.0;
/// A rect whose four corner radii differ (`corner_radius_top_left=` …);
/// the radii ride the uv slot (unused for untextured quads).
const KIND_RECT4: f32 = 8.0;
const KIND_IMAGE: f32 = 5.0;
const NO_UV: [f32; 4] = [0.0; 4];

impl UiRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> UiRenderer {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("ui"), source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()) });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui screen"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("ui screen"), size: std::mem::size_of::<Screen>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let screen_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("ui screen"), layout: &screen_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: screen_buffer.as_entire_binding() }] });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("ui"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ui"), bind_group_layouts: &[Some(&screen_layout), Some(&tex_layout)], immediate_size: 0 });
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
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4],
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
        let quad_capacity = 1024;
        let quad_buffer = Self::make_quad_buffer(device, quad_capacity);
        let mut r = UiRenderer {
            pipeline,
            screen_buffer,
            screen_bind,
            tex_layout,
            sampler,
            quad_buffer,
            quad_capacity,
            quads: Vec::new(),
            batches: Vec::new(),
            screen: (1, 1),
            textures: Vec::new(),
            atlas_texture: None,
            atlas_size: 0,
            images: HashMap::new(),
            externals: Vec::new(),
            skipped: 0,
            draw_calls: 0,
        };
        // Slot 0: a 1×1 white atlas until the text engine uploads one.
        let (tex, entry) = r.make_texture(device, 1, 1, wgpu::TextureFormat::R8Unorm);
        r.textures.push(entry);
        r.atlas_texture = Some(tex);
        r
    }

    fn make_quad_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor { label: Some("ui quads"), size: (capacity * std::mem::size_of::<Quad>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
    }

    fn make_texture(&self, device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat) -> (wgpu::Texture, Texture) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let entry = self.make_entry(device, &tex, width, height);
        (tex, entry)
    }

    /// A slot over any 2D texture the UI may sample: one bind group with
    /// the shared sampler.
    fn make_entry(&self, device: &wgpu::Device, tex: &wgpu::Texture, width: u32, height: u32) -> Texture {
        let view = tex.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui texture"),
            layout: &self.tex_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
        });
        Texture { bind_group, width, height }
    }

    /// Opens the image `key` (the `source=` string) for an externally-owned
    /// DISPLAY texture and hands back its handle — the D302 road for live
    /// GPU content in a gogo Image (D10): the owner keeps the texture and
    /// re-encodes it when it changes; the UI samples it like any uploaded
    /// image. Needed for the brush page's live preview (its display cache,
    /// an sRGB8 encode of the preview renderer). Open once, then
    /// `set_external_image` per frame: the handle is an index, so the
    /// steady state hashes no key. Opening a key twice returns the same
    /// handle.
    pub fn external_image(&mut self, key: &str) -> ExternalImage {
        if let Some(i) = self.externals.iter().position(|e| e.key == key) {
            return ExternalImage(i as u32);
        }
        self.externals.push(External { key: key.to_string(), texture: None });
        ExternalImage((self.externals.len() - 1) as u32)
    }

    /// Shows `texture` as the opened image. Another texture replaces the
    /// slot (the ping-pong renderers swap); the SAME texture again is a
    /// no-op — no bind group per frame. Returns true when it (re)registered.
    /// Takes the display newtype: the raw texture never leaves this crate
    /// (the D202 domain guard).
    pub fn set_external_image(&mut self, device: &wgpu::Device, image: ExternalImage, texture: &Tex_Display_Srgb_4xU8_RGBA) -> bool {
        let raw: &wgpu::Texture = &texture.RGBA;
        let external: &External = &self.externals[image.0 as usize];
        if external.texture.as_ref() == Some(raw) {
            return false;
        }
        let (width, height) = (raw.width(), raw.height());
        let entry = self.make_entry(device, raw, width, height);
        match self.images.get(&external.key).copied() {
            Some(slot) if slot != ATLAS => self.textures[slot as usize] = entry,
            _ => {
                self.textures.push(entry);
                self.images.insert(external.key.clone(), (self.textures.len() - 1) as TexId);
            }
        }
        self.externals[image.0 as usize].texture = Some(raw.clone());
        true
    }

    fn write(queue: &wgpu::Queue, tex: &wgpu::Texture, data: &[u8], width: u32, height: u32, bytes_per_pixel: u32) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * bytes_per_pixel), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }

    /// Registers an RGBA8 image under `key` (the `source=` string).
    pub fn add_image(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, key: &str, rgba: &[u8], width: u32, height: u32) {
        // sRGB bytes in, sRGB bytes out: the surface is non-sRGB (the canvas
        // blit copies encoded bytes), so an sRGB texture here would be
        // decoded on sample and never re-encoded — every image too dark.
        let (tex, entry) = self.make_texture(device, width, height, wgpu::TextureFormat::Rgba8Unorm);
        Self::write(queue, &tex, rgba, width, height, 4);
        self.textures.push(entry);
        self.images.insert(key.to_string(), (self.textures.len() - 1) as TexId);
    }

    /// Uploads the glyph atlas when the engine changed it.
    fn sync_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, text: &mut TextEngine) {
        if !text.take_dirty() {
            return;
        }
        let (data, size) = text.atlas();
        if size != self.atlas_size {
            let (tex, entry) = self.make_texture(device, size, size, wgpu::TextureFormat::R8Unorm);
            self.textures[ATLAS as usize] = entry;
            self.atlas_texture = Some(tex);
            self.atlas_size = size;
        }
        if let Some(tex) = &self.atlas_texture {
            Self::write(queue, tex, data, size, size, 1);
        }
    }

    /// Turns the draw list into instances (epx × `epx` = device px), shapes
    /// text, uploads. `screen` = the target size in device pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, draws: &[Draw], epx: f32, screen: (u32, u32), page: &Page, text: &mut TextEngine) {
        self.prepare_pages(device, queue, &[(draws, page)], epx, screen, text, None);
    }

    /// Several pages' draw lists in paint order, each with the page that
    /// owns its strings. `locale`: the language keyed text shows — the same
    /// table the pages laid out with, so a label is shaped at the width it
    /// was measured (`None` = the literals).
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_pages(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pages: &[(&[Draw], &Page)], epx: f32, screen: (u32, u32), text: &mut TextEngine, locale: Option<&Locale>) {
        self.quads.clear();
        self.batches.clear();
        self.skipped = 0;
        self.screen = screen;
        // Each rect's origin snaps to a whole device pixel here, at draw time,
        // so a fractional size upstream (`Image height=205.7`) never blurs
        // the glyph runs and quads below it; the layout keeps its fractions
        // (Godot snaps per node, compounding — this rounds each node once
        // from its true position).
        let px = |r: &Rect| [(r.x * epx).round(), (r.y * epx).round(), r.w * epx, r.h * epx];
        let mut clips: Vec<[u32; 4]> = Vec::new();
        let mut rots: Vec<[f32; 4]> = Vec::new();
        let mut rot: [f32; 4] = NO_ROT;
        let mut wedges: Vec<[f32; 4]> = Vec::new();
        let mut wedge: [f32; 4] = NO_WEDGE;
        let mut batch_first = 0u32;
        let mut current: Option<[u32; 4]> = None;
        let mut current_tex: TexId = ATLAS;
        for (draws, page) in pages {
        for d in draws.iter() {
            match d {
                Draw::Quad { rect, color, corner, border, border_color } => {
                    self.quads.push(Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [corner * epx, border * epx, KIND_RECT, 0.0], uv: NO_UV, rot, wedge });
                }
                Draw::QuadCorners { rect, color, corners, border, border_color } => {
                    self.quads.push(Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [0.0, border * epx, KIND_RECT4, 0.0], uv: [corners[0] * epx, corners[1] * epx, corners[2] * epx, corners[3] * epx], rot, wedge });
                }
                Draw::Arc { rect, color, from, sweep, thickness } => {
                    self.quads.push(Quad { rect: px(rect), color: color.to_f32(), border_color: [0.0; 4], params: [thickness * epx, from.to_radians(), KIND_ARC, sweep.to_radians()], uv: NO_UV, rot, wedge });
                }
                Draw::Circle { rect, color, border, border_color } => {
                    self.quads.push(Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [0.0, border * epx, KIND_CIRCLE, 0.0], uv: NO_UV, rot, wedge });
                }
                Draw::Gradient { rect, kind, from, to, angle, band, corner } => {
                    let k = match kind {
                        GradientKind::Radial => KIND_RADIAL,
                        GradientKind::Conic => KIND_CONIC,
                        GradientKind::Linear => KIND_LINEAR,
                    };
                    // The compass (0 = to top, clockwise) as the shader's
                    // direction angle (0 = to the right): a quarter turn back.
                    // Stops ride the color slots as premultiplied Oklab
                    // (gogo's one mixing law): the shader lerps them raw
                    // and converts the mix back to sRGB per pixel.
                    // params.x is the rounded-rect radius the shader masks
                    // EVERY quad by — a Gradient's `corner_radius` (Q9).
                    self.quads.push(Quad { rect: px(rect), color: gogo::color::oklab_pm_from_rgba(*from).0, border_color: gogo::color::oklab_pm_from_rgba(*to).0, params: [corner * epx, 0.0, k, (angle - 90.0).to_radians()], uv: [band.0, band.1, 0.0, 0.0], rot, wedge });
                }
                Draw::Text { rect, text: text_id, key, font_size, color, halign, valign, wrap, font } => {
                    if current_tex != ATLAS {
                        self.close_batch(&mut batch_first, current, current_tex);
                        current_tex = ATLAS;
                    }
                    let s = gogo::locale::display(page, locale, *text_id, *key);
                    let font_key = if *font == NO_STR { None } else { Some(page.str(*font)) };
                    let r = px(rect);
                    // Truncated like Godot's `int(epx * size)` (see `EpxMeasure`).
                    let size_px = (font_size * epx).floor().max(1.0);
                    let max_w = if *wrap { Some(r[2]) } else { None };
                    let (_, th) = text.measure(s, size_px, max_w, font_key);
                    let y = match valign {
                        TextAlign::Start => r[1],
                        TextAlign::Center => r[1] + (r[3] - th) / 2.0,
                        TextAlign::End => r[1] + r[3] - th,
                    };
                    let (x, box_w, h) = match (wrap, halign) {
                        (true, _) => (r[0], r[2], *halign as u8),
                        (false, TextAlign::Start) => (r[0], r[2], 0),
                        (false, _) => (r[0], r[2], *halign as u8),
                    };
                    let color = color.to_f32();
                    for g in text.shape(s, size_px, max_w, font_key, x, y, box_w, h) {
                        self.quads.push(Quad { rect: [g.x, g.y, g.w, g.h], color, border_color: [0.0; 4], params: [0.0, 0.0, KIND_GLYPH, 0.0], uv: g.uv, rot, wedge });
                    }
                }
                Draw::Selection { rect, text: text_id, key, font_size, font, lo, hi, color } => {
                    // The highlight between the two shaped prefixes —
                    // measured here because glyph positions live here.
                    let s = gogo::locale::display(page, locale, *text_id, *key);
                    let font_key = if *font == NO_STR { None } else { Some(page.str(*font)) };
                    let r = px(rect);
                    let size_px = (font_size * epx).floor().max(1.0);
                    let byte = |i: u32| s.char_indices().nth(i as usize).map(|(b, _)| b).unwrap_or(s.len());
                    let (xlo, _) = text.measure(&s[..byte(*lo)], size_px, None, font_key);
                    let (xhi, _) = text.measure(&s[..byte(*hi)], size_px, None, font_key);
                    let h = size_px * 1.2;
                    let y = r[1] + (r[3] - h) / 2.0;
                    self.quads.push(Quad { rect: [r[0] + xlo, y, (xhi - xlo).max(0.0), h], color: color.to_f32(), border_color: [0.0; 4], params: [0.0, 0.0, KIND_RECT, 0.0], uv: NO_UV, rot, wedge });
                }
                Draw::SelHandles { rect, text: text_id, key, font_size, font, lo, hi, color } => {
                    // The grab circles below the selection's ends —
                    // measured here like the highlight (glyph positions
                    // live here); the input machine grab-checks the
                    // same `sel_handle_rect` in epx.
                    let s = gogo::locale::display(page, locale, *text_id, *key);
                    let font_key = if *font == NO_STR { None } else { Some(page.str(*font)) };
                    let r = px(rect);
                    let size_px = (font_size * epx).floor().max(1.0);
                    let byte = |i: u32| s.char_indices().nth(i as usize).map(|(b, _)| b).unwrap_or(s.len());
                    let (xlo, _) = text.measure(&s[..byte(*lo)], size_px, None, font_key);
                    let (xhi, _) = text.measure(&s[..byte(*hi)], size_px, None, font_key);
                    let color = color.to_f32();
                    for x in [r[0] + xlo, r[0] + xhi] {
                        let h = gogo::input::sel_handle_rect(r[1] + r[3], x, gogo::input::SEL_HANDLE_EPX * epx);
                        self.quads.push(Quad { rect: [h.x, h.y, h.w, h.h], color, border_color: [0.0; 4], params: [0.0, 0.0, KIND_CIRCLE, 0.0], uv: NO_UV, rot, wedge });
                    }
                }
                Draw::Caret { rect, text: text_id, key, font_size, font, index, color } => {
                    // A thin solid bar after the shaped prefix — shaping
                    // happens here because glyph positions live here.
                    let s = gogo::locale::display(page, locale, *text_id, *key);
                    let font_key = if *font == NO_STR { None } else { Some(page.str(*font)) };
                    let r = px(rect);
                    let size_px = (font_size * epx).floor().max(1.0);
                    let byte = s.char_indices().nth(*index as usize).map(|(b, _)| b).unwrap_or(s.len());
                    let (pw, _) = text.measure(&s[..byte], size_px, None, font_key);
                    let h = size_px * 1.2;
                    let x = r[0] + pw;
                    let y = r[1] + (r[3] - h) / 2.0;
                    self.quads.push(Quad { rect: [x, y, (1.5 * epx).max(1.0), h], color: color.to_f32(), border_color: [0.0; 4], params: [0.0, 0.0, KIND_RECT, 0.0], uv: NO_UV, rot, wedge });
                }
                Draw::Image { rect, image, tint, fit } => {
                    let Some(tex) = self.images.get(page.str(*image)).copied() else {
                        self.skipped += 1;
                        continue;
                    };
                    if current_tex != tex {
                        self.close_batch(&mut batch_first, current, current_tex);
                        current_tex = tex;
                    }
                    let t = &self.textures[tex as usize];
                    let r = px(rect);
                    let (tw, th) = (t.width as f32, t.height as f32);
                    let (fitted, uv) = match fit {
                        // contain: the bitmap letterboxed into the rect.
                        Fit::Contain => {
                            let scale = (r[2] / tw).min(r[3] / th);
                            let (w, h) = (tw * scale, th * scale);
                            ([r[0] + (r[2] - w) / 2.0, r[1] + (r[3] - h) / 2.0, w, h], [0.0, 0.0, 1.0, 1.0])
                        }
                        // cover: the rect filled, the bitmap's overflow cropped (centered).
                        Fit::Cover => {
                            let scale = (r[2] / tw).max(r[3] / th);
                            let (vis_w, vis_h) = (r[2] / scale / tw, r[3] / scale / th);
                            (r, [(1.0 - vis_w) / 2.0, (1.0 - vis_h) / 2.0, (1.0 + vis_w) / 2.0, (1.0 + vis_h) / 2.0])
                        }
                        Fit::Distort => (r, [0.0, 0.0, 1.0, 1.0]),
                    };
                    self.quads.push(Quad { rect: fitted, color: tint.to_f32(), border_color: [0.0; 4], params: [0.0, 0.0, KIND_IMAGE, 0.0], uv, rot, wedge });
                }
                Draw::ClipPush(rect) => {
                    self.close_batch(&mut batch_first, current, current_tex);
                    let r = px(rect);
                    let x = r[0].max(0.0).floor() as u32;
                    let y = r[1].max(0.0).floor() as u32;
                    let right = (r[0] + r[2]).min(screen.0 as f32).ceil() as u32;
                    let bottom = (r[1] + r[3]).min(screen.1 as f32).ceil() as u32;
                    let mut clip = [x, y, right.saturating_sub(x), bottom.saturating_sub(y)];
                    if let Some(outer) = current {
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
                    self.close_batch(&mut batch_first, current, current_tex);
                    clips.pop();
                    current = clips.last().copied();
                }
                Draw::RotatePush { angle, cx, cy } => {
                    let (sin, cos) = angle.to_radians().sin_cos();
                    rots.push([sin, cos, cx * epx, cy * epx]);
                    rot = *rots.last().unwrap();
                }
                Draw::WedgePush { cx, cy, from, span } => {
                    // The mask discards fragments outside the wedge; a
                    // nested wedge keeps only the innermost (the page
                    // never nests them today).
                    wedges.push([cx * epx, cy * epx, from.to_radians(), span.max(0.0).to_radians()]);
                    wedge = *wedges.last().unwrap();
                }
                Draw::WedgePop => {
                    wedges.pop();
                    wedge = wedges.last().copied().unwrap_or(NO_WEDGE);
                }
                Draw::RotatePop => {
                    rots.pop();
                    rot = rots.last().copied().unwrap_or(NO_ROT);
                }
            }
        }
        }
        self.close_batch(&mut batch_first, current, current_tex);
        self.sync_atlas(device, queue, text);

        if self.quads.len() > self.quad_capacity {
            self.quad_capacity = self.quads.len().next_power_of_two();
            self.quad_buffer = Self::make_quad_buffer(device, self.quad_capacity);
        }
        if !self.quads.is_empty() {
            queue.write_buffer(&self.quad_buffer, 0, bytemuck::cast_slice(&self.quads));
        }
        queue.write_buffer(&self.screen_buffer, 0, bytemuck::bytes_of(&Screen { size: [screen.0 as f32, screen.1 as f32], _pad: [0.0; 2] }));
    }

    fn close_batch(&mut self, first: &mut u32, scissor: Option<[u32; 4]>, tex: TexId) {
        let end = self.quads.len() as u32;
        if end > *first {
            self.batches.push(Batch { first: *first, count: end - *first, scissor, tex });
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
        pass.set_bind_group(0, &self.screen_bind, &[]);
        pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
        for b in &self.batches {
            match b.scissor {
                Some([x, y, w, h]) if w > 0 && h > 0 => pass.set_scissor_rect(x, y, w, h),
                Some(_) => continue,
                None => pass.set_scissor_rect(0, 0, self.screen.0, self.screen.1),
            }
            pass.set_bind_group(1, &self.textures[b.tex as usize].bind_group, &[]);
            pass.draw(0..6, b.first..b.first + b.count);
            self.draw_calls += 1;
        }
        pass.set_scissor_rect(0, 0, self.screen.0, self.screen.1);
    }

    pub fn quad_count(&self) -> usize {
        self.quads.len()
    }

    pub fn has_image(&self, key: &str) -> bool {
        self.images.contains_key(key)
    }
}
