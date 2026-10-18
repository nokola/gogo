//! The gogo draw list on wgpu: one instanced draw of UI quads per (clip,
//! texture) batch. Needed so a whole page costs one buffer write and a
//! handful of draw calls; the page's rects are epx and are scaled to device
//! pixels here (`gogo` never sees a device pixel). Text is shaped by the
//! `text` crate into glyph quads over its coverage atlas (texture 0);
//! images are RGBA textures the host's image table writes through
//! `write_image_texture_only` (the table records the size alongside). A gradient
//! arrives as one quad per stop pair (its `band`); a `RotatePush` turns
//! every quad until its pop about the given center, a `SwingPush` turns
//! them in 3D about a vertical hinge under a perspective camera (the
//! WP7 turnstile), and a `TextStretch` maps one glyph's ink box onto its
//! rect.

use crate::perf_stages::{Stage, Timer, note_ms};
use crate::tex_types::{Tex_Bake_Premul_Srgb_4xU8_RGBA, Tex_Display_Srgb_4xU8_RGBA};
use std::time::Instant;

use gogo::draw::{Draw, Fit, GradientKind, Sampling, TextAlign};
use gogo::layout::Rect;
use gogo::locale::Locale;
use gogo::page::{NO_STR, Page};
use gogo::ImageKey;
use text::TextEngine;

/// The frame's draw lists, in draw order, each with the page that owns
/// its strings — handed to `UiRenderer::prepare_pages` as a walk so the
/// host builds no vector of them per frame (DECISIONS.md D76). A slice or
/// an array of `(draws, page)` pairs is one (the pins, a one-page bake).
/// The page outlives the walk (it lives in the host); a list may be a
/// copy made for the call (a page mid-slide).
pub trait DrawLists {
    fn for_each_list<'a>(&'a self, f: &mut dyn FnMut(&[Draw], &'a Page));
}

impl DrawLists for [(&[Draw], &Page)] {
    fn for_each_list<'a>(&'a self, f: &mut dyn FnMut(&[Draw], &'a Page)) {
        for (draws, page) in self {
            f(draws, page);
        }
    }
}

impl<const N: usize> DrawLists for [(&[Draw], &Page); N] {
    fn for_each_list<'a>(&'a self, f: &mut dyn FnMut(&[Draw], &'a Page)) {
        self.as_slice().for_each_list(f);
    }
}

impl DrawLists for Vec<(&[Draw], &Page)> {
    fn for_each_list<'a>(&'a self, f: &mut dyn FnMut(&[Draw], &'a Page)) {
        self.as_slice().for_each_list(f);
    }
}

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
    /// The 3D swing: sin, cos of the angle, the hinge x (device px), the
    /// camera distance (device px; 0 = none).
    swing: [f32; 4],
    /// The swing's camera: on x, y (device px) — the swung node's center.
    eye: [f32; 4],
}

const NO_ROT: [f32; 4] = [0.0, 1.0, 0.0, 0.0];
const NO_WEDGE: [f32; 4] = [0.0, 0.0, 0.0, -1.0];
const NO_SWING: [f32; 4] = [0.0, 1.0, 0.0, 0.0];
const NO_EYE: [f32; 4] = [0.0; 4];

/// The largest size (device px) a `text_fit=stretch` glyph is rasterized
/// at: near the rect's larger side below this, so only the squashed axis
/// loses detail; a page-sized bubble stretches a glyph of this size
/// rather than asking the atlas for one bigger than a texture.
pub const STRETCH_RASTER_MAX_PX: f32 = 1024.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Screen {
    size: [f32; 2],
    _pad: [f32; 2],
}

/// Texture slot: 0 = the glyph coverage atlas, 1 = the emoji color atlas,
/// 2.. = images in `images` order.
type TexId = u32;
const ATLAS: TexId = 0;
/// The text engine's color atlas (emoji, DECISIONS.md D119): straight
/// RGBA, drawn as image quads with a white tint.
const COLOR_ATLAS: TexId = 1;

struct Batch {
    first: u32,
    count: u32,
    scissor: Option<[u32; 4]>,
    tex: TexId,
    /// Sampled by texel (`sampling=pixel`) instead of smoothed.
    pixel: bool,
    /// Every quad of it is one `fs_plain` draws (`UiRenderer::quad_is_plain`),
    /// so it takes a `PipelinePair`'s `plain` pipeline.
    plain: bool,
    /// A host layer's slot (`UiRenderer::host_layer`): no quads — the
    /// host's recorded bundle plays here instead.
    host: Option<u32>,
}

/// The two pipelines ONE target format is drawn through: `full` runs
/// `fs_main`, which draws every kind of quad; `plain` runs `fs_plain`,
/// which draws an image, a glyph or a rect without a border, with the
/// same math.
///
/// Needed because a phone GPU of 2014 pays for ALL of `fs_main` on every
/// pixel, the ones that use none of it included: through `full` alone the
/// screen pass is one 31-49 ms GPU job on the OnePlus A0001 and every
/// screen tops out near 20 frames a second; through the pair it is 6-10 ms
/// and a frame lands every screen refresh (DECISIONS.md D57,
/// docs/todo-perf-smudge.md/PS22). Images, glyphs and borderless rects are
/// 97 % and more of the pixels of every main screen.
struct PipelinePair {
    full: wgpu::RenderPipeline,
    plain: wgpu::RenderPipeline,
}

/// A measuring build (`scripts\build.ps1 -UiClassLog`) that logs, once per
/// screen, the quads and the share of the screen per quad class
/// (`UiRenderer::log_quad_classes`). `scripts\dist.ps1` clears it.
const UI_CLASS_LOG_BUILD: bool = option_env!("FANRUST_UI_CLASS_LOG").is_some();

/// The quad classes a `UI_CLASS_LOG_BUILD` counts per frame, in this order.
const QUAD_CLASSES: [&str; 8] = ["image", "glyph", "plain rect", "rounded rect no border", "bordered rect", "circle", "gradient", "arc"];

/// A measuring build (`scripts\build.ps1 -NoUi`) that draws NO UI: only
/// the images of externally-owned textures (the canvas, a brush preview)
/// reach the screen. Taps and swipes still land — only the drawing is
/// skipped.
///
/// Needed to read what a stroke costs with no UI on screen
/// (docs/todo-perf-smudge.md/PS8): this build against one with the UI, by
/// `scripts/perf_stroke_video.py` and `scripts/perf_stroke_atrace.py`.
/// `scripts\dist.ps1` clears it.
const NO_UI_BUILD: bool = option_env!("FANRUST_NO_UI").is_some();

struct Texture {
    bind_group: wgpu::BindGroup,
    /// The same view through the nearest-texel sampler.
    bind_group_pixel: wgpu::BindGroup,
    width: u32,
    height: u32,
    /// The texture itself, so a same-size re-upload writes in place
    /// (`write_image_texture_only`); None for the externals' entries and
    /// the atlas.
    texture: Option<wgpu::Texture>,
}

/// One externally-owned image the UI shows (`UiRenderer::external_image`).
struct External {
    /// The Image's `source=` id.
    key: ImageKey,
    /// The texture identity last registered; `None` until the first set.
    texture: Option<wgpu::Texture>,
}

/// The handle `UiRenderer::external_image` opens for one image key; the
/// per-frame `set_external_image` takes it, so showing a live texture
/// hashes no string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalImage(u32);

/// The handle `UiRenderer::host_layer` opens for one image key: where in
/// the UI's own pass a host's drawing plays (`set_host_layer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostLayer(u32);

pub struct UiRenderer {
    /// The screen's pipelines.
    pipelines: PipelinePair,
    /// The quad count per class (`QUAD_CLASSES`) last logged, so a
    /// `UI_CLASS_LOG_BUILD` logs a screen once, not every frame.
    classes_logged: [u32; 8],
    /// What every pipeline of this renderer shares — the screen's
    /// (`pipelines`) and the offscreen tile's, made on the first
    /// `render_to_texture` (a pipeline is bound to ONE target format).
    module: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    tile_pipelines: Option<PipelinePair>,
    screen_buffer: wgpu::Buffer,
    screen_bind: wgpu::BindGroup,
    /// The size the screen uniform holds on the GPU: written only when
    /// the size changes (D34) — a stroke's frames never change it, and
    /// on the GLES road every buffer write is an EGL context switch.
    screen_written: Option<(u32, u32)>,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Nearest-texel sampling for `sampling=pixel` Images.
    sampler_pixel: wgpu::Sampler,
    quad_buffer: wgpu::Buffer,
    quad_capacity: usize,
    quads: Vec<Quad>,
    batches: Vec<Batch>,
    /// The quads the GPU buffer holds: `prepare_pages` writes the buffer
    /// only when this frame's quads differ (a buffer write is an EGL
    /// context switch on the GLES road, D33) — an idle frame writes
    /// nothing. Emptied when the buffer is remade.
    quads_written: Vec<Quad>,
    /// How many times the quad buffer was written (the pins).
    quad_writes: u32,
    // The prepare pass's scratch, kept between frames so a frame
    // allocates nothing once warm (DECISIONS.md D76): the clip, rotation,
    // wedge and swing stacks, and one label's shaped glyphs.
    clips: Vec<[u32; 4]>,
    rots: Vec<[f32; 4]>,
    wedges: Vec<[f32; 4]>,
    swings: Vec<([f32; 4], [f32; 4])>,
    glyphs: Vec<text::GlyphQuad>,
    screen: (u32, u32),
    /// Slot 0 is the atlas (rebuilt when the engine's atlas changes size).
    textures: Vec<Texture>,
    /// Slots `release_image` emptied (they hold a 1x1 blank), taken again
    /// by the next picture so a page that comes and goes does not grow
    /// the table.
    free_slots: Vec<TexId>,
    atlas_texture: Option<wgpu::Texture>,
    atlas_size: u32,
    /// Slot 1: the engine's emoji atlas (rebuilt when it changes size).
    color_atlas_texture: Option<wgpu::Texture>,
    color_atlas_size: u32,
    /// Texture slot by `ImageKey` (the `source=` id); `None` = no pixels
    /// yet. Grows as the host interns pictures past the build's table.
    images: Vec<Option<TexId>>,
    /// Externally-owned textures (`external_image` / `set_external_image`):
    /// the image key and the texture identity last registered, so a
    /// same-texture frame makes no bind group. Indexed by `ExternalImage`.
    externals: Vec<External>,
    /// Host layers (`host_layer` / `set_host_layer`): the image key an
    /// Image names to seat the layer, and the bundle it plays. Indexed by
    /// `HostLayer`.
    host_layers: Vec<(ImageKey, Option<wgpu::RenderBundle>)>,
    /// The format of the screen's pass (the host's bundles are recorded
    /// for it).
    target_format: wgpu::TextureFormat,
    /// The gradient tables (D21): a row per gradient on screen, cached by
    /// content across frames.
    gradients: GradientTable,
    /// Commands with no texture to draw them (a missing image).
    pub skipped: u32,
    /// Off only in a pin that compares a culled frame with the whole one
    /// (`debug_set_culling`).
    cull: bool,
    /// Quads and images of this frame left out because they lie outside
    /// their clip (`quad_is_outside`).
    culled: u32,
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
        let sampler_pixel = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("ui pixel"), mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest, ..Default::default() });
        let gradients = GradientTable::new(device, GRAD_ROWS_INITIAL);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ui"), bind_group_layouts: &[Some(&screen_layout), Some(&tex_layout), Some(&gradients.layout)], immediate_size: 0 });
        let pipelines = Self::make_pipelines(device, &module, &pipeline_layout, target_format);
        if NO_UI_BUILD {
            log::info!("FANRUST_UI no-UI build: only externally-owned images (the canvas, a brush preview) are drawn");
        }
        let quad_capacity = 1024;
        let quad_buffer = Self::make_quad_buffer(device, quad_capacity);
        let mut r = UiRenderer {
            pipelines,
            classes_logged: [0; 8],
            module,
            pipeline_layout,
            tile_pipelines: None,
            screen_buffer,
            screen_bind,
            screen_written: None,
            tex_layout,
            sampler,
            sampler_pixel,
            quad_buffer,
            quad_capacity,
            quads: Vec::new(),
            batches: Vec::new(),
            quads_written: Vec::new(),
            quad_writes: 0,
            clips: Vec::new(),
            rots: Vec::new(),
            wedges: Vec::new(),
            swings: Vec::new(),
            glyphs: Vec::new(),
            screen: (1, 1),
            textures: Vec::new(),
            free_slots: Vec::new(),
            atlas_texture: None,
            atlas_size: 0,
            color_atlas_texture: None,
            color_atlas_size: 0,
            images: Vec::new(),
            externals: Vec::new(),
            host_layers: Vec::new(),
            target_format,
            gradients,
            skipped: 0,
            cull: true,
            culled: 0,
            draw_calls: 0,
        };
        // Slot 0: a 1×1 white atlas until the text engine uploads one.
        let (tex, entry) = r.make_texture(device, 1, 1, wgpu::TextureFormat::R8Unorm);
        r.textures.push(entry);
        r.atlas_texture = Some(tex);
        // Slot 1: the emoji atlas, the same way.
        let (tex, entry) = r.make_texture(device, 1, 1, wgpu::TextureFormat::Rgba8Unorm);
        r.textures.push(entry);
        r.color_atlas_texture = Some(tex);
        r
    }

    /// The pipeline pair for one target format — the screen's swapchain
    /// format, or a tile's.
    fn make_pipelines(device: &wgpu::Device, module: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> PipelinePair {
        PipelinePair { full: Self::make_pipeline(device, module, layout, format, "fs_main"), plain: Self::make_pipeline(device, module, layout, format, "fs_plain") }
    }

    /// One UI pipeline for one target format, with `fragment_entry` of
    /// ui.wgsl as its fragment stage.
    fn make_pipeline(device: &wgpu::Device, module: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat, fragment_entry: &str) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fragment_entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        })
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

    /// A slot over any 2D texture the UI may sample: one bind group per
    /// sampler (smooth, and the pixel one a `sampling=pixel` Image draws
    /// with).
    fn make_entry(&self, device: &wgpu::Device, tex: &wgpu::Texture, width: u32, height: u32) -> Texture {
        let view = tex.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui texture"),
            layout: &self.tex_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
        });
        let bind_group_pixel = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui texture pixel"),
            layout: &self.tex_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler_pixel) }],
        });
        Texture { bind_group, bind_group_pixel, width, height, texture: None }
    }

    /// Opens the image `key` (the `source=` id) for an externally-owned
    /// DISPLAY texture and hands back its handle — the D302 road for live
    /// GPU content in a gogo Image (D10): the owner keeps the texture and
    /// re-encodes it when it changes; the UI samples it like any uploaded
    /// image. Needed for the brush page's live preview (its display cache,
    /// an sRGB8 encode of the preview renderer). Open once, then
    /// `set_external_image` per frame — the handle remembers the texture
    /// identity, so a same-texture frame makes no bind group. Opening a
    /// key twice returns the same handle.
    pub fn external_image(&mut self, key: ImageKey) -> ExternalImage {
        if let Some(i) = self.externals.iter().position(|e| e.key == key) {
            return ExternalImage(i as u32);
        }
        self.externals.push(External { key, texture: None });
        ExternalImage((self.externals.len() - 1) as u32)
    }

    /// The format of the screen's pass: a host layer's bundle must be
    /// recorded for it.
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    /// Opens the image `key` as a HOST LAYER and hands back its handle: an
    /// Image with `source=` this key draws no picture — the bundle the
    /// host hands `set_host_layer` plays at that point of the UI's own
    /// pass, under everything drawn after it. Needed so a page can put
    /// the host's own drawing (the stickers over the photo) between the
    /// layers of the UI without a pass of its own: on the OnePlus every
    /// pass costs a whole-screen load and store, and a blend with what is
    /// already on screen (burn-in, light) needs the SAME pass. Opening a
    /// key twice returns the same handle.
    pub fn host_layer(&mut self, key: ImageKey) -> HostLayer {
        if let Some(i) = self.host_layers.iter().position(|(k, _)| *k == key) {
            return HostLayer(i as u32);
        }
        self.host_layers.push((key, None));
        HostLayer((self.host_layers.len() - 1) as u32)
    }

    /// What the layer plays from the next frame on (`None` = nothing). The
    /// bundle is recorded for `target_format`; the pass state it leaves is
    /// restored after it. A bundle may be kept and handed again — the
    /// buffers it reads can change between frames.
    pub fn set_host_layer(&mut self, layer: HostLayer, bundle: Option<wgpu::RenderBundle>) {
        self.host_layers[layer.0 as usize].1 = bundle;
    }

    /// The host layer an Image of `key` seats, if any.
    fn host_layer_of(&self, key: ImageKey) -> Option<u32> {
        self.host_layers.iter().position(|(k, _)| *k == key).map(|i| i as u32)
    }

    /// The texture slot `key` shows, if it has pixels.
    fn slot(&self, key: ImageKey) -> Option<TexId> {
        self.images.get(key.index()).copied().flatten()
    }

    /// Points `key` at texture slot `tex` (growing the table past the
    /// build's images for a host-interned key).
    fn set_slot(&mut self, key: ImageKey, tex: TexId) {
        if self.images.len() <= key.index() {
            self.images.resize(key.index() + 1, None);
        }
        self.images[key] = Some(tex);
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
        let key: ImageKey = external.key;
        let entry = self.make_entry(device, raw, width, height);
        match self.slot(key) {
            Some(slot) if slot != ATLAS => self.textures[slot as usize] = entry,
            _ => {
                let slot: TexId = self.take_slot(entry);
                self.set_slot(key, slot);
            }
        }
        self.externals[image.0 as usize].texture = Some(raw.clone());
        true
    }

    /// Drops the texture an opened image shows: the slot points at a
    /// blank 1x1 until the next `set_external_image`. Needed because the
    /// bind group here holds the texture alive — a released full-res crop
    /// photo (hundreds of MB) would otherwise stay on the GPU until the
    /// next open replaced it.
    pub fn release_external_image(&mut self, device: &wgpu::Device, image: ExternalImage) {
        let external: &External = &self.externals[image.0 as usize];
        if external.texture.is_none() {
            return;
        }
        let key: ImageKey = external.key;
        let (_, entry) = self.make_texture(device, 1, 1, wgpu::TextureFormat::Rgba8Unorm);
        if let Some(slot) = self.slot(key) {
            if slot != ATLAS {
                self.textures[slot as usize] = entry;
            }
        }
        self.externals[image.0 as usize].texture = None;
    }

    /// Test-only: how many opened images hold a texture right now — a
    /// page's close pins that its pictures left the GPU.
    pub fn debug_held_external_count(&self) -> usize {
        self.externals.iter().filter(|e| e.texture.is_some()).count()
    }

    fn write(queue: &wgpu::Queue, tex: &wgpu::Texture, data: &[u8], width: u32, height: u32, bytes_per_pixel: u32) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * bytes_per_pixel), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }

    /// Registers an RGBA8 image under `key` (the `source=` id) — a fresh
    /// texture; `write_image_texture_only` is the door in.
    fn add_image(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, key: ImageKey, rgba: &[u8], width: u32, height: u32) {
        // sRGB bytes in, sRGB bytes out: the surface is non-sRGB (the canvas
        // blit copies encoded bytes), so an sRGB texture here would be
        // decoded on sample and never re-encoded — every image too dark.
        let (tex, mut entry) = self.make_texture(device, width, height, wgpu::TextureFormat::Rgba8Unorm);
        Self::write(queue, &tex, rgba, width, height, 4);
        entry.texture = Some(tex);
        let slot: TexId = self.take_slot(entry);
        self.set_slot(key, slot);
    }

    /// Puts `entry` in a slot `release_image` emptied, else a new one.
    fn take_slot(&mut self, entry: Texture) -> TexId {
        match self.free_slots.pop() {
            Some(slot) => {
                self.textures[slot as usize] = entry;
                slot
            }
            None => {
                self.textures.push(entry);
                (self.textures.len() - 1) as TexId
            }
        }
    }

    /// Frees an uploaded picture's texture: `key` shows nothing until its
    /// pixels are uploaded again. Needed because built-in pictures would
    /// otherwise stay on the GPU for the whole session once any page
    /// showed them (the effects page's ~11 MB of tile photos, the sticker
    /// sheets) — the host calls this for pictures no live page shows. An
    /// opened external image is left alone (its owner releases it with
    /// `release_external_image`). Returns whether a texture was freed.
    pub fn release_image(&mut self, device: &wgpu::Device, key: ImageKey) -> bool {
        let Some(slot) = self.slot(key) else { return false };
        if slot == ATLAS || self.externals.iter().any(|e| e.key == key) {
            return false;
        }
        let (_, blank) = self.make_texture(device, 1, 1, wgpu::TextureFormat::Rgba8Unorm);
        self.textures[slot as usize] = blank;
        self.images[key] = None;
        self.free_slots.push(slot);
        true
    }

    /// Writes an image's pixels — the TEXTURE ONLY. Page code never calls
    /// this: the host's image table is the door (`ImageTable::upload_image`
    /// in app_wgpu), because layout needs the picture's size as well and a
    /// texture written without it draws into a 0-wide box (todo-bugs B26).
    /// The same-size texture is written IN PLACE, a new size (or a key
    /// never added) takes `add_image` — pictures that refresh while shown
    /// (the mask panel's tiles per effect pass, the preset strip per open)
    /// would otherwise pile up a fresh texture per refresh for the session.
    pub fn write_image_texture_only(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, key: ImageKey, rgba: &[u8], width: u32, height: u32) {
        let slot: Option<TexId> = self.images.get(key.index()).copied().flatten();
        if let Some(entry) = slot.map(|s| &self.textures[s as usize]) {
            if let Some(tex) = &entry.texture {
                if entry.width == width && entry.height == height {
                    Self::write(queue, tex, rgba, width, height, 4);
                    return;
                }
            }
        }
        self.add_image(device, queue, key, rgba, width, height);
    }

    /// Uploads the glyph atlases when the engine changed them.
    fn sync_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, text: &mut TextEngine) {
        if text.take_dirty() {
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
        if text.take_color_dirty() {
            let (data, size) = text.color_atlas();
            if size != self.color_atlas_size {
                let (tex, entry) = self.make_texture(device, size, size, wgpu::TextureFormat::Rgba8Unorm);
                self.textures[COLOR_ATLAS as usize] = entry;
                self.color_atlas_texture = Some(tex);
                self.color_atlas_size = size;
            }
            if let Some(tex) = &self.color_atlas_texture {
                Self::write(queue, tex, data, size, size, 4);
            }
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
    pub fn prepare_pages<L: DrawLists + ?Sized>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pages: &L, epx: f32, screen: (u32, u32), text: &mut TextEngine, locale: Option<&Locale>) {
        // A glyph that grows the atlas during the pass leaves every run
        // shaped before it this frame — the cache's included — with uvs into
        // the atlas that was: the pass runs again against the grown one. A
        // grow doubles the atlas, a few times in an app's life; the usual
        // frame pays nothing. User-seen: a button with no letters ("cancel")
        // until the next redraw (S24, 2026-09-14).
        let generation: u32 = text.generation;
        let pass_stage = Timer::new(Stage::UiPass);
        self.prepare_pass(device, pages, epx, screen, text, locale);
        if text.generation != generation {
            self.prepare_pass(device, pages, epx, screen, text, locale);
        }
        drop(pass_stage);
        let atlas_stage = Timer::new(Stage::UiAtlas);
        self.sync_atlas(device, queue, text);
        self.gradients.flush(queue);
        drop(atlas_stage);
        if UI_CLASS_LOG_BUILD {
            self.log_quad_classes();
        }
        let _upload_stage = Timer::new(Stage::UiUpload);

        if self.quads.len() > self.quad_capacity {
            self.quad_capacity = self.quads.len().next_power_of_two();
            self.quad_buffer = Self::make_quad_buffer(device, self.quad_capacity);
            self.quads_written.clear(); // a fresh buffer holds nothing yet
        }
        let unchanged: bool = bytemuck::cast_slice::<Quad, u8>(&self.quads) == bytemuck::cast_slice::<Quad, u8>(&self.quads_written);
        if !self.quads.is_empty() && !unchanged {
            queue.write_buffer(&self.quad_buffer, 0, bytemuck::cast_slice(&self.quads));
            self.quads_written.clone_from(&self.quads); // keeps its capacity: no allocation once warm
            self.quad_writes += 1;
        }
        if self.screen_written != Some(screen) {
            queue.write_buffer(&self.screen_buffer, 0, bytemuck::bytes_of(&Screen { size: [screen.0 as f32, screen.1 as f32], _pad: [0.0; 2] }));
            self.screen_written = Some(screen);
        }
    }

    /// One pass over the draw lists into `quads` and `batches` — shapes
    /// the text, claims the gradient rows; `prepare_pages` uploads after.
    fn prepare_pass<L: DrawLists + ?Sized>(&mut self, device: &wgpu::Device, pages: &L, epx: f32, screen: (u32, u32), text: &mut TextEngine, locale: Option<&Locale>) {
        self.quads.clear();
        self.batches.clear();
        self.skipped = 0;
        self.culled = 0;
        self.screen = screen;
        // Each rect's origin snaps to a whole device pixel here, at draw time,
        // so a fractional size upstream (`Image height=205.7`) never blurs
        // the glyph runs and quads below it; the layout keeps its fractions
        // (Godot snaps per node, compounding — this rounds each node once
        // from its true position).
        let px = |r: &Rect| [(r.x * epx).round(), (r.y * epx).round(), r.w * epx, r.h * epx];
        // The scratch stacks step out of self for the pass (push_quad and
        // close_batch borrow self) and go back at its end.
        let mut clips: Vec<[u32; 4]> = std::mem::take(&mut self.clips);
        let mut rots: Vec<[f32; 4]> = std::mem::take(&mut self.rots);
        let mut wedges: Vec<[f32; 4]> = std::mem::take(&mut self.wedges);
        let mut swings: Vec<([f32; 4], [f32; 4])> = std::mem::take(&mut self.swings);
        let mut glyphs: Vec<text::GlyphQuad> = std::mem::take(&mut self.glyphs);
        clips.clear();
        rots.clear();
        wedges.clear();
        swings.clear();
        let mut rot: [f32; 4] = NO_ROT;
        let mut wedge: [f32; 4] = NO_WEDGE;
        let mut swing: [f32; 4] = NO_SWING;
        let mut eye: [f32; 4] = NO_EYE;
        let mut batch_first = 0u32;
        let mut current: Option<[u32; 4]> = None;
        let mut current_tex: TexId = ATLAS;
        let mut current_pixel: bool = false;
        let mut text_secs: f32 = 0.0;
        pages.for_each_list(&mut |draws: &[Draw], page: &Page| {
        // Indexed: a Gradient entry owns the stop records after it.
        let mut i: usize = 0;
        while i < draws.len() {
            let d = &draws[i];
            i += 1;
            if NO_UI_BUILD && !self.drawn_without_ui(d) {
                continue;
            }
            match d {
                Draw::Quad { rect, color, corner, border, border_color } => {
                    self.push_quad(current, Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [corner * epx, border * epx, KIND_RECT, 0.0], uv: NO_UV, rot, wedge, swing, eye });
                }
                Draw::QuadCorners { rect, color, corners, border, border_color } => {
                    self.push_quad(current, Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [0.0, border * epx, KIND_RECT4, 0.0], uv: [corners[0] * epx, corners[1] * epx, corners[2] * epx, corners[3] * epx], rot, wedge, swing, eye });
                }
                Draw::Arc { rect, color, from, sweep, thickness, dash } => {
                    // uv: the dash mask (offset, dash, period) in radians —
                    // period 0 = solid. One quad per dash/gap pair.
                    let (offset, dash_len, period) = dash;
                    self.push_quad(current, Quad { rect: px(rect), color: color.to_f32(), border_color: [0.0; 4], params: [thickness * epx, from.to_radians(), KIND_ARC, sweep.to_radians()], uv: [offset.to_radians(), dash_len.to_radians(), period.to_radians(), 0.0], rot, wedge, swing, eye });
                }
                Draw::Circle { rect, color, border, border_color } => {
                    self.push_quad(current, Quad { rect: px(rect), color: color.to_f32(), border_color: border_color.to_f32(), params: [0.0, border * epx, KIND_CIRCLE, 0.0], uv: NO_UV, rot, wedge, swing, eye });
                }
                Draw::Gradient { rect, kind, angle, corner, period, stops } => {
                    let count: usize = *stops as usize;
                    let list: &[Draw] = &draws[i..(i + count).min(draws.len())];
                    i += count;
                    let k = match kind {
                        GradientKind::Radial => KIND_RADIAL,
                        GradientKind::Conic => KIND_CONIC,
                        GradientKind::Linear => KIND_LINEAR,
                    };
                    // ONE quad (D21): the stops go to the table row this
                    // gradient's content owns; the shader looks each pixel's
                    // pair up. params.x is the rounded-rect radius the
                    // shader masks EVERY quad by — a Gradient's
                    // `corner_radius` (Q9). The compass (0 = to top,
                    // clockwise) as the shader's direction angle (0 = to the
                    // right): a quarter turn back. uv: the row, the slot
                    // count, then the tiling period (start, length in run
                    // units; 0 length = no tiling).
                    let r = px(rect);
                    if self.cull && Self::quad_is_outside(&r, &rot, &swing, current, screen) {
                        self.culled += 1;
                        continue;
                    }
                    let n_slots: u32 = GradientTable::slots_for(r[2].max(r[3]), *period);
                    let row: u32 = self.gradients.row_for(device, list, n_slots, *kind, *period);
                    self.push_quad(current, Quad { rect: r, color: [0.0; 4], border_color: [0.0; 4], params: [corner * epx, 0.0, k, (angle - 90.0).to_radians()], uv: [row as f32, n_slots as f32, period.0, period.1], rot, wedge, swing, eye });
                }
                // A stop without its Gradient ahead of it (never emitted
                // by the draw pass; skipped, not drawn).
                Draw::GradientStop { .. } => {}
                Draw::Text { rect, text: text_id, key, font_size, color, halign, valign, wrap, font } => {
                    let text_start: Instant = Instant::now();
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
                    // Each glyph is judged on its own ink box: text may hang
                    // out of its rect (no wrap, a tall line), so the rect
                    // says nothing about what shows. The batch turns to the
                    // atlas at the first glyph that shows - text that is all
                    // outside must not split the batch around it.
                    glyphs.clear();
                    text.shape_into(s, size_px, max_w, font_key, x, y, box_w, h, &mut glyphs);
                    text_secs += text_start.elapsed().as_secs_f32();
                    for g in &glyphs {
                        let glyph_rect: [f32; 4] = [g.x, g.y, g.w, g.h];
                        if self.cull && Self::quad_is_outside(&glyph_rect, &rot, &swing, current, screen) {
                            self.culled += 1;
                            continue;
                        }
                        // An emoji is an image quad over the color atlas:
                        // its own colors, the text's alpha (the Text page's
                        // opacity slider fades it with the letters).
                        let (tex, kind, tint) = if g.color { (COLOR_ATLAS, KIND_IMAGE, [1.0, 1.0, 1.0, color[3]]) } else { (ATLAS, KIND_GLYPH, color) };
                        if current_tex != tex || current_pixel {
                            self.close_batch(&mut batch_first, current, current_tex, current_pixel);
                            current_tex = tex;
                            current_pixel = false;
                        }
                        self.quads.push(Quad { rect: glyph_rect, color: tint, border_color: [0.0; 4], params: [0.0, 0.0, kind, 0.0], uv: g.uv, rot, wedge, swing, eye });
                    }
                }
                Draw::TextStretch { rect, text: text_id, key, color, font, flip } => {
                    let r = px(rect);
                    if self.cull && Self::quad_is_outside(&r, &rot, &swing, current, screen) {
                        self.culled += 1;
                        continue;
                    }
                    let s = gogo::locale::display(page, locale, *text_id, *key);
                    let font_key = if *font == NO_STR { None } else { Some(page.str(*font)) };
                    let size_px = r[2].max(r[3]).clamp(8.0, STRETCH_RASTER_MAX_PX);
                    if let Some(g) = text.glyph(s, size_px, font_key) {
                        let color = color.to_f32();
                        // A stretched emoji is an image quad like a run's.
                        let (tex, kind, tint) = if g.color { (COLOR_ATLAS, KIND_IMAGE, [1.0, 1.0, 1.0, color[3]]) } else { (ATLAS, KIND_GLYPH, color) };
                        if current_tex != tex || current_pixel {
                            self.close_batch(&mut batch_first, current, current_tex, current_pixel);
                            current_tex = tex;
                            current_pixel = false;
                        }
                        // The ink box IS the quad; a flip swaps the uv ends.
                        let (u0, u1) = if flip.0 { (g.uv[2], g.uv[0]) } else { (g.uv[0], g.uv[2]) };
                        let (v0, v1) = if flip.1 { (g.uv[3], g.uv[1]) } else { (g.uv[1], g.uv[3]) };
                        self.push_quad(current, Quad { rect: r, color: tint, border_color: [0.0; 4], params: [0.0, 0.0, kind, 0.0], uv: [u0, v0, u1, v1], rot, wedge, swing, eye });
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
                    self.push_quad(current, Quad { rect: [r[0] + xlo, y, (xhi - xlo).max(0.0), h], color: color.to_f32(), border_color: [0.0; 4], params: [0.0, 0.0, KIND_RECT, 0.0], uv: NO_UV, rot, wedge, swing, eye });
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
                        self.push_quad(current, Quad { rect: [h.x, h.y, h.w, h.h], color, border_color: [0.0; 4], params: [0.0, 0.0, KIND_CIRCLE, 0.0], uv: NO_UV, rot, wedge, swing, eye });
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
                    self.push_quad(current, Quad { rect: [x, y, (1.5 * epx).max(1.0), h], color: color.to_f32(), border_color: [0.0; 4], params: [0.0, 0.0, KIND_RECT, 0.0], uv: NO_UV, rot, wedge, swing, eye });
                }
                Draw::Image { rect, image, tint, fit, window, sampling } => {
                    if let Some(layer) = self.host_layer_of(*image) {
                        // The host's bundle plays here, between what was
                        // drawn before and what comes after.
                        self.close_batch(&mut batch_first, current, current_tex, current_pixel);
                        if self.host_layers[layer as usize].1.is_some() {
                            self.batches.push(Batch { first: batch_first, count: 0, scissor: current, tex: ATLAS, pixel: false, plain: false, host: Some(layer) });
                        }
                        continue;
                    }
                    let Some(tex) = self.slot(*image) else {
                        self.skipped += 1;
                        continue;
                    };
                    // Judged on the Image's rect BEFORE the batch turns to
                    // its texture (every fit draws inside the rect): on a
                    // long list of pictures each texture is a draw call of
                    // its own, and the ones scrolled out were a third of a
                    // frame's CPU on an old phone (docs/todo-perf-smudge.md/PS29).
                    if self.cull && Self::quad_is_outside(&px(rect), &rot, &swing, current, screen) {
                        self.culled += 1;
                        continue;
                    }
                    let pixel: bool = *sampling == Sampling::Pixel;
                    if current_tex != tex || current_pixel != pixel {
                        self.close_batch(&mut batch_first, current, current_tex, current_pixel);
                        current_tex = tex;
                        current_pixel = pixel;
                    }
                    let t = &self.textures[tex as usize];
                    let r = px(rect);
                    let (tw, th) = (t.width as f32, t.height as f32);
                    // The window is the picture the fit maps (the whole
                    // texture when unset); its uv corners are the
                    // window's fraction of the texture.
                    let [wx, wy, ww, wh] = window.unwrap_or([0.0, 0.0, tw, th]);
                    let (fitted, uv) = match fit {
                        // contain: the bitmap letterboxed into the rect.
                        Fit::Contain => {
                            let scale = (r[2] / ww).min(r[3] / wh);
                            let (w, h) = (ww * scale, wh * scale);
                            ([r[0] + (r[2] - w) / 2.0, r[1] + (r[3] - h) / 2.0, w, h], [0.0, 0.0, 1.0, 1.0])
                        }
                        // cover: the rect filled, the bitmap's overflow cropped (centered).
                        Fit::Cover => {
                            let scale = (r[2] / ww).max(r[3] / wh);
                            let (vis_w, vis_h) = (r[2] / scale / ww, r[3] / scale / wh);
                            (r, [(1.0 - vis_w) / 2.0, (1.0 - vis_h) / 2.0, (1.0 + vis_w) / 2.0, (1.0 + vis_h) / 2.0])
                        }
                        Fit::Distort => (r, [0.0, 0.0, 1.0, 1.0]),
                    };
                    let uv = [(wx + uv[0] * ww) / tw, (wy + uv[1] * wh) / th, (wx + uv[2] * ww) / tw, (wy + uv[3] * wh) / th];
                    self.push_quad(current, Quad { rect: fitted, color: tint.to_f32(), border_color: [0.0; 4], params: [0.0, 0.0, KIND_IMAGE, 0.0], uv, rot, wedge, swing, eye });
                }
                Draw::ClipPush(rect) => {
                    self.close_batch(&mut batch_first, current, current_tex, current_pixel);
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
                    self.close_batch(&mut batch_first, current, current_tex, current_pixel);
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
                Draw::SwingPush { angle, pivot_x, cx, cy, focal } => {
                    // A nested swing keeps only the innermost (the page
                    // never nests them today).
                    let (sin, cos) = angle.to_radians().sin_cos();
                    swings.push(([sin, cos, pivot_x * epx, focal * epx], [cx * epx, cy * epx, 0.0, 0.0]));
                    (swing, eye) = *swings.last().unwrap();
                }
                Draw::SwingPop => {
                    swings.pop();
                    (swing, eye) = swings.last().copied().unwrap_or((NO_SWING, NO_EYE));
                }
            }
        }
        });
        self.close_batch(&mut batch_first, current, current_tex, current_pixel);
        self.clips = clips;
        self.rots = rots;
        self.wedges = wedges;
        self.swings = swings;
        self.glyphs = glyphs;
        note_ms(Stage::UiText, text_secs * 1000.0);
    }

    /// Adds `q` to this frame's quads unless it lies outside `clip`
    /// (`quad_is_outside`). Every plain quad of `prepare_pass` comes
    /// through here; text, images and gradients ask the same rule
    /// themselves, before the work a quad that shows needs.
    ///
    /// Needed so the rule is spelled once: a new kind of command that
    /// pushed its quad straight onto the list would be drawn off screen
    /// again.
    fn push_quad(&mut self, clip: Option<[u32; 4]>, q: Quad) {
        if self.cull && Self::quad_is_outside(&q.rect, &q.rot, &q.swing, clip, self.screen) {
            self.culled += 1;
            return;
        }
        self.quads.push(q);
    }

    /// Whether a quad at `rect` (device px) can show no pixel: it shares
    /// no area with `clip` - the scissor it would be drawn under, the
    /// whole screen when there is none. A clip of no area hides every
    /// quad under it.
    ///
    /// Needed because the GPU throws such a quad away only AFTER the CPU
    /// paid for it: its bytes in the quad upload and, for a picture with a
    /// texture of its own, a draw call. A list many screens long sends
    /// all of itself every frame - on the OnePlus A0001 the effect list's
    /// scrolled-out pictures were a third of the frame's CPU
    /// (docs/todo-perf-smudge.md/PS29).
    ///
    /// A TURNED or SWUNG quad is never outside: its rect is where it was
    /// laid out, not where the vertex stage puts it. A rect with a NaN in
    /// it is never outside either (every comparison with it is false).
    fn quad_is_outside(rect: &[f32; 4], rot: &[f32; 4], swing: &[f32; 4], clip: Option<[u32; 4]>, screen: (u32, u32)) -> bool {
        if *rot != NO_ROT || swing[3] > 0.0 {
            return false;
        }
        let [x, y, w, h] = match clip {
            Some(c) => [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32],
            None => [0.0, 0.0, screen.0 as f32, screen.1 as f32],
        };
        let (right, bottom) = ((x + w).min(screen.0 as f32), (y + h).min(screen.1 as f32));
        rect[0] >= right || rect[0] + rect[2] <= x || rect[1] >= bottom || rect[1] + rect[3] <= y
    }

    fn close_batch(&mut self, first: &mut u32, scissor: Option<[u32; 4]>, tex: TexId, pixel: bool) {
        let end = self.quads.len() as u32;
        if end > *first {
            self.push_batch_split_by_plain(*first, end, scissor, tex, pixel);
        }
        *first = end;
    }

    /// Whether a `NO_UI_BUILD` keeps `d`: an image of an externally-owned
    /// texture or a host layer, or a clip / rotate / wedge / swing push or pop — the state
    /// such an image is drawn under. A skipped Gradient's stops fall to the
    /// stray-stop arm, which draws nothing.
    fn drawn_without_ui(&self, d: &Draw) -> bool {
        match d {
            Draw::Image { image, .. } => self.externals.iter().any(|e| e.key == *image) || self.host_layer_of(*image).is_some(),
            Draw::ClipPush(_) | Draw::ClipPop | Draw::RotatePush { .. } | Draw::RotatePop | Draw::WedgePush { .. } | Draw::WedgePop | Draw::SwingPush { .. } | Draw::SwingPop => true,
            _ => false,
        }
    }

    /// Whether `fs_plain` draws `q` exactly as `fs_main` does: an image, a
    /// glyph, or a rect without a border (square or rounded corners),
    /// outside any wedge mask.
    ///
    /// Needed because `fs_plain` has no code for a border, a circle, a
    /// gradient, an arc, per-corner radii or a wedge mask: such a quad sent
    /// to it would draw as a flat rect. The pin
    /// `tests/ui_plain_pipeline.rs` holds the two shaders to the same
    /// pixels for the quads this admits.
    fn quad_is_plain(q: &Quad) -> bool {
        let kind: f32 = q.params[2];
        let borderless_rect: bool = kind == KIND_RECT && q.params[1] == 0.0;
        q.wedge[3] < 0.0 && (kind == KIND_IMAGE || kind == KIND_GLYPH || borderless_rect)
    }

    /// Logs, per quad class, the quads of this frame and the share of the
    /// screen their rects cover (overlaps count twice; clips are ignored) —
    /// once per screen: only when a class's quad count changed.
    ///
    /// Needed to see WHERE a screen's pixels are before moving a kind of
    /// quad between `fs_main` and `fs_plain` (DECISIONS.md D57).
    fn log_quad_classes(&mut self) {
        let mut counts: [u32; 8] = [0; 8];
        let mut areas: [f32; 8] = [0.0; 8];
        let (sw, sh) = (self.screen.0 as f32, self.screen.1 as f32);
        for q in &self.quads {
            let kind: f32 = q.params[2];
            let class: usize = if kind == KIND_IMAGE {
                0
            } else if kind == KIND_GLYPH {
                1
            } else if kind == KIND_RECT && q.params[1] == 0.0 {
                if q.params[0] == 0.0 { 2 } else { 3 }
            } else if kind == KIND_RECT || kind == KIND_RECT4 {
                4
            } else if kind == KIND_CIRCLE {
                5
            } else if kind == KIND_ARC {
                7
            } else {
                6
            };
            let w: f32 = (q.rect[0] + q.rect[2]).min(sw) - q.rect[0].max(0.0);
            let h: f32 = (q.rect[1] + q.rect[3]).min(sh) - q.rect[1].max(0.0);
            counts[class] += 1;
            areas[class] += w.max(0.0) * h.max(0.0);
        }
        if counts == self.classes_logged {
            return;
        }
        self.classes_logged = counts;
        let line: Vec<String> = (0..8).filter(|&c| counts[c] > 0).map(|c| format!("{} {} quads {:.1} %", QUAD_CLASSES[c], counts[c], 100.0 * areas[c] / (sw * sh))).collect();
        log::info!("FANRUST_UI classes (quads, share of the screen): {}", line.join("; "));
    }

    /// One batch per run of plain / not plain quads in `first..end`, in
    /// draw order, so each run takes its own pipeline of a `PipelinePair`.
    fn push_batch_split_by_plain(&mut self, first: u32, end: u32, scissor: Option<[u32; 4]>, tex: TexId, pixel: bool) {
        let mut run_first: u32 = first;
        let mut run_plain: bool = Self::quad_is_plain(&self.quads[first as usize]);
        for i in first + 1..=end {
            let plain: bool = i < end && Self::quad_is_plain(&self.quads[i as usize]);
            if i == end || plain != run_plain {
                self.batches.push(Batch { first: run_first, count: i - run_first, scissor, tex, pixel, plain: run_plain, host: None });
                run_first = i;
                run_plain = plain;
            }
        }
    }

    /// Records the quads into `pass` (the caller owns the pass).
    pub fn render(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        let calls: u32 = self.record(pass, &self.pipelines, false, true);
        self.draw_calls = calls;
    }

    /// Pin probe: `render` with EVERY batch through `fs_main` — what the
    /// split's pin (`tests/ui_plain_pipeline.rs`) compares a `render` to.
    pub fn debug_render_through_fs_main(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        let calls: u32 = self.record(pass, &self.pipelines, true, true);
        self.draw_calls = calls;
    }

    /// Pin probe: (plain batches, all batches) of the last `prepare`.
    pub fn debug_plain_batches(&self) -> (usize, usize) {
        (self.batches.iter().filter(|b| b.plain).count(), self.batches.len())
    }

    /// The prepared batches through `pipelines` (the screen's or a tile's):
    /// a plain batch through `plain`, the rest through `full` — or all of
    /// them through `full` when `full_only`. A host layer's bundle plays
    /// only with `host_layers` (the screen: the bundles are recorded for
    /// its format — a tile skips them). Returns the draw calls made.
    fn record(&self, pass: &mut wgpu::RenderPass<'_>, pipelines: &PipelinePair, full_only: bool, host_layers: bool) -> u32 {
        if self.batches.is_empty() {
            return 0;
        }
        let mut draw_calls: u32 = 0;
        let mut plain_set: bool = false;
        let bind = |pass: &mut wgpu::RenderPass<'_>| {
            pass.set_pipeline(&pipelines.full);
            pass.set_bind_group(0, &self.screen_bind, &[]);
            pass.set_bind_group(2, &self.gradients.bind_group, &[]);
            pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
        };
        bind(pass);
        for b in &self.batches {
            if let Some(layer) = b.host {
                if let (true, Some(bundle)) = (host_layers, &self.host_layers[layer as usize].1) {
                    pass.set_scissor_rect(0, 0, self.screen.0, self.screen.1);
                    pass.execute_bundles(std::iter::once(bundle));
                    draw_calls += 1;
                    // A bundle leaves the pass state reset: bind again.
                    bind(pass);
                    plain_set = false;
                }
                continue;
            }
            match b.scissor {
                Some([x, y, w, h]) if w > 0 && h > 0 => pass.set_scissor_rect(x, y, w, h),
                Some(_) => continue,
                None => pass.set_scissor_rect(0, 0, self.screen.0, self.screen.1),
            }
            let plain: bool = b.plain && !full_only;
            if plain != plain_set {
                pass.set_pipeline(if plain { &pipelines.plain } else { &pipelines.full });
                plain_set = plain;
            }
            let t = &self.textures[b.tex as usize];
            pass.set_bind_group(1, if b.pixel { &t.bind_group_pixel } else { &t.bind_group }, &[]);
            pass.draw(0..6, b.first..b.first + b.count);
            draw_calls += 1;
        }
        pass.set_scissor_rect(0, 0, self.screen.0, self.screen.1);
        draw_calls
    }

    /// The format a tile is stored in: the shader writes sRGB-ENCODED
    /// premultiplied values, so a plain Unorm target keeps them as written
    /// (an `*Srgb` format would encode them a second time) — the same rule
    /// that picks the shell's non-sRGB surface format.
    const TILE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    /// Draws `draws` (page epx — one node's list from `gogo::draw_subtree`)
    /// into a NEW texture of `size_px`: `origin_epx` lands on the tile's
    /// top-left, `epx` device px per epx — a text callout's bake tile at
    /// the painting's scale (todo-rust 2.8.3; shipped's SubViewport +
    /// TextBakeDrawer pair).
    ///
    /// Needed so a bake draws through the ONE code path the screen uses —
    /// the same walk, shaping, atlas and shader — so what the user sees is
    /// what bakes; a CPU rasterizer would be a second renderer to keep in
    /// step (not a perf choice: a bake runs once per keep).
    ///
    /// The tile is PREMULTIPLIED sRGB8, the pipeline's output over a
    /// transparent clear — the typed return keeps it out of the straight
    /// image decode. Uses the renderer's quad and screen buffers in queue
    /// order, so it may run any time between two frames (the frame's own
    /// `prepare` rewrites both). `locale`: the table the page laid out
    /// with (`prepare_pages`).
    #[allow(clippy::too_many_arguments)]
    pub fn render_to_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, draws: &[Draw], page: &Page, text: &mut TextEngine, locale: Option<&Locale>, epx: f32, origin_epx: (f32, f32), size_px: (u32, u32)) -> Tex_Bake_Premul_Srgb_4xU8_RGBA {
        let shifted: Vec<Draw> = draws.iter().map(|d| d.posed(-origin_epx.0, -origin_epx.1, 1.0)).collect();
        let size: (u32, u32) = (size_px.0.max(1), size_px.1.max(1));
        self.prepare_pages(device, queue, &[(shifted.as_slice(), page)], epx, size, text, locale);
        if self.tile_pipelines.is_none() {
            self.tile_pipelines = Some(Self::make_pipelines(device, &self.module, &self.pipeline_layout, Self::TILE_FORMAT));
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui tile"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::TILE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ui tile") });
        let calls: u32 = {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui tile"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.record(&mut pass, self.tile_pipelines.as_ref().expect("made above"), false, false)
        };
        self.draw_calls = calls;
        queue.submit([encoder.finish()]);
        Tex_Bake_Premul_Srgb_4xU8_RGBA::new(tex)
    }

    /// Pin probe: gradient table rows in use after the last frame.
    pub fn debug_gradient_rows(&self) -> usize {
        self.gradients.debug_rows_in_use()
    }

    /// Culling off or on (`quad_is_outside`), for the pin that holds a
    /// culled frame to the pixels of the whole one
    /// (`tests/ui_cull.rs`).
    pub fn debug_set_culling(&mut self, on: bool) {
        self.cull = on;
    }

    /// The quads and images the last `prepare` left out as outside their
    /// clip.
    pub fn debug_culled(&self) -> u32 {
        self.culled
    }

    pub fn quad_count(&self) -> usize {
        self.quads.len()
    }

    /// How many frames wrote the quad buffer so far (the pin: an unchanged
    /// frame writes nothing).
    pub fn debug_quad_writes(&self) -> u32 {
        self.quad_writes
    }

    pub fn has_image(&self, key: ImageKey) -> bool {
        self.slot(key).is_some()
    }
}

// ---- The gradient tables (D21) ---------------------------------------------------

/// Slots per index-map row: a gradient uses as many as its box has device
/// pixels along the run (see `slots_for`), at most this.
const GRAD_MAP_WIDTH: u32 = 1024;
/// Stops per table row (two texels each). The editor caps a gradient at
/// 60; a mirrored one doubles; more is cut.
const GRAD_MAX_STOPS: u32 = 128;
const GRAD_STOP_TEXELS: u32 = GRAD_MAX_STOPS * 2;
const GRAD_ROWS_INITIAL: u32 = 64;

/// One cached gradient: what its row holds, and whether this frame drew it.
struct GradEntry {
    hash: u64,
    row: u32,
    seen: bool,
}

/// The two tables a gradient quad reads (ui.wgsl group 2): the index map
/// (which stop pair covers each slot along the run) and the stops
/// (premultiplied Oklab color, then position). One row per gradient on
/// screen, owned across frames by content: a row refills only when its
/// stops, slot count or period change, so a still gradient uploads
/// nothing, and the frame's refills go up in ONE write per table. Never
/// filtered: both are read by exact texel.
///
/// Both tables are INTEGER textures (R8Uint, Rgba16Uint). Needed because a
/// float32 texture bound without a filter is an INCOMPLETE texture on GLES
/// 3.0 drivers without OES_texture_float_linear (the OnePlus A0001's
/// Adreno 330, 2026-09-06) and samples as opaque black — every gradient on
/// the phone was a black box while the desktop pins were green. Integer
/// textures can never filter, so every driver fetches them by texel. The
/// stops are 16-bit fixed point over -1..1 (step 3e-5, well under the
/// 8-bit encode), the position two 16-bit halves of a 32-bit fraction.
pub struct GradientTable {
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    map_tex: wgpu::Texture,
    stops_tex: wgpu::Texture,
    rows: u32,
    /// Staging, table-shaped: `rows × GRAD_MAP_WIDTH` bytes.
    map: Vec<u8>,
    /// Staging: `rows × GRAD_STOP_TEXELS` texels, fixed point (see `encode_unit`).
    stops: Vec<[u16; 4]>,
    entries: Vec<GradEntry>,
    free: Vec<u32>,
    next_row: u32,
    /// Rows written since the last flush (inclusive range).
    dirty: Option<(u32, u32)>,
    /// The positions of the row being filled (reused, never reallocated
    /// past its high-water mark).
    positions: Vec<f32>,
}

impl GradientTable {
    fn new(device: &wgpu::Device, rows: u32) -> GradientTable {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui gradients"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let (map_tex, stops_tex, bind_group) = Self::make_tables(device, &layout, rows);
        GradientTable {
            layout,
            bind_group,
            map_tex,
            stops_tex,
            rows,
            map: vec![0; (rows * GRAD_MAP_WIDTH) as usize],
            stops: vec![[0; 4]; (rows * GRAD_STOP_TEXELS) as usize],
            entries: Vec::new(),
            free: Vec::new(),
            next_row: 0,
            dirty: None,
            positions: Vec::new(),
        }
    }

    fn make_tables(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, rows: u32) -> (wgpu::Texture, wgpu::Texture, wgpu::BindGroup) {
        let make = |label: &str, width: u32, format: wgpu::TextureFormat| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height: rows, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let map_tex = make("ui gradient map", GRAD_MAP_WIDTH, wgpu::TextureFormat::R8Uint);
        let stops_tex = make("ui gradient stops", GRAD_STOP_TEXELS, wgpu::TextureFormat::Rgba16Uint);
        let map_view = map_tex.create_view(&Default::default());
        let stops_view = stops_tex.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui gradients"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&map_view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&stops_view) }],
        });
        (map_tex, stops_tex, bind_group)
    }

    /// How many slots a gradient over a box whose longer side is
    /// `max_side_px` uses: one per device pixel along the run, times 1.5
    /// for a diagonal run (the longest a box holds), at most the row.
    /// Tiled, the run is the period, so the fraction scales it.
    pub fn slots_for(max_side_px: f32, period: (f32, f32)) -> u32 {
        let run: f32 = if period.1 > 0.0 { max_side_px * 1.5 * period.1 } else { max_side_px * 1.5 };
        (run.ceil().max(2.0) as u32).min(GRAD_MAP_WIDTH)
    }

    /// The row holding `stops` at `n_slots`: the cached one, else a fresh
    /// fill. `stops` are the `Draw::GradientStop` records of one gradient.
    fn row_for(&mut self, device: &wgpu::Device, stops: &[Draw], n_slots: u32, kind: GradientKind, period: (f32, f32)) -> u32 {
        let hash: u64 = Self::hash(stops, n_slots, kind, period);
        if let Some(e) = self.entries.iter_mut().find(|e| e.hash == hash) {
            e.seen = true;
            return e.row;
        }
        let row: u32 = match self.free.pop() {
            Some(r) => r,
            None => {
                if self.next_row >= self.rows {
                    self.grow(device);
                }
                self.next_row += 1;
                self.next_row - 1
            }
        };
        self.fill(row, stops, n_slots);
        self.entries.push(GradEntry { hash, row, seen: true });
        row
    }

    /// FNV-1a over what the row holds: the stops' positions and colors,
    /// the slot count, the kind and the period.
    fn hash(stops: &[Draw], n_slots: u32, kind: GradientKind, period: (f32, f32)) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        eat(&n_slots.to_le_bytes());
        eat(&[kind as u8]);
        eat(&period.0.to_bits().to_le_bytes());
        eat(&period.1.to_bits().to_le_bytes());
        for d in stops {
            if let Draw::GradientStop { pos, color } = d {
                eat(&pos.to_bits().to_le_bytes());
                eat(&color.0);
            }
        }
        h
    }

    /// Writes one row: the stops as premultiplied Oklab + position texels,
    /// then the map slots. Beyond `GRAD_MAX_STOPS` the stops are cut.
    fn fill(&mut self, row: u32, stops: &[Draw], n_slots: u32) {
        self.positions.clear();
        let base: usize = (row * GRAD_STOP_TEXELS) as usize;
        for d in stops {
            if self.positions.len() as u32 >= GRAD_MAX_STOPS {
                break;
            }
            if let Draw::GradientStop { pos, color } = d {
                let i: usize = self.positions.len();
                let lab: [f32; 4] = gogo::color::oklab_pm_from_rgba(*color).0;
                self.stops[base + 2 * i] = [encode_unit(lab[0]), encode_unit(lab[1]), encode_unit(lab[2]), encode_unit(lab[3])];
                let (hi, lo) = encode_pos(*pos);
                self.stops[base + 2 * i + 1] = [hi, lo, 0, 0];
                self.positions.push(*pos);
            }
        }
        let map_base: usize = (row * GRAD_MAP_WIDTH) as usize;
        fill_map(&mut self.map[map_base..map_base + n_slots as usize], &self.positions);
        self.dirty = Some(match self.dirty {
            Some((lo, hi)) => (lo.min(row), hi.max(row)),
            None => (row, row),
        });
    }

    /// Twice the rows: new textures, the staging kept, every row re-uploaded.
    fn grow(&mut self, device: &wgpu::Device) {
        let rows: u32 = self.rows * 2;
        let (map_tex, stops_tex, bind_group) = Self::make_tables(device, &self.layout, rows);
        self.map_tex = map_tex;
        self.stops_tex = stops_tex;
        self.bind_group = bind_group;
        self.map.resize((rows * GRAD_MAP_WIDTH) as usize, 0);
        self.stops.resize((rows * GRAD_STOP_TEXELS) as usize, [0; 4]);
        self.dirty = Some((0, self.rows.max(1) - 1));
        self.rows = rows;
    }

    /// End of frame: the dirty rows go up as one write per table, rows no
    /// gradient used this frame are freed, the seen marks reset.
    fn flush(&mut self, queue: &wgpu::Queue) {
        if let Some((lo, hi)) = self.dirty.take() {
            let count: u32 = hi - lo + 1;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.map_tex, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: lo, z: 0 }, aspect: wgpu::TextureAspect::All },
                &self.map[(lo * GRAD_MAP_WIDTH) as usize..((hi + 1) * GRAD_MAP_WIDTH) as usize],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(GRAD_MAP_WIDTH), rows_per_image: Some(count) },
                wgpu::Extent3d { width: GRAD_MAP_WIDTH, height: count, depth_or_array_layers: 1 },
            );
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.stops_tex, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: lo, z: 0 }, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(&self.stops[(lo * GRAD_STOP_TEXELS) as usize..((hi + 1) * GRAD_STOP_TEXELS) as usize]),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(GRAD_STOP_TEXELS * 8), rows_per_image: Some(count) },
                wgpu::Extent3d { width: GRAD_STOP_TEXELS, height: count, depth_or_array_layers: 1 },
            );
        }
        let mut i: usize = 0;
        while i < self.entries.len() {
            if self.entries[i].seen {
                self.entries[i].seen = false;
                i += 1;
            } else {
                let gone = self.entries.swap_remove(i);
                self.free.push(gone.row);
            }
        }
    }

    /// Pin probes.
    pub fn debug_rows_in_use(&self) -> usize {
        self.entries.len()
    }
}

/// A premultiplied Oklab component (L, a, b or alpha, all within -1..1) as
/// 16-bit fixed point: 0 = -1, 65535 = +1. The shader's twin divides by
/// 32767.5 and subtracts 1.
fn encode_unit(v: f32) -> u16 {
    ((v.clamp(-1.0, 1.0) + 1.0) * 32767.5).round() as u16
}

/// A 0..1 stop position as the two 16-bit halves of a 32-bit fraction —
/// the shader rebuilds `(hi * 65536 + lo) / 4294967295`, exact to 1e-7.
fn encode_pos(pos: f32) -> (u16, u16) {
    let p: u32 = (pos.clamp(0.0, 1.0) as f64 * 4_294_967_295.0).round() as u32;
    ((p >> 16) as u16, (p & 0xFFFF) as u16)
}

/// Fills `map` (one byte per slot) with the stop pair covering each slot:
/// slot s covers run positions [s/n, (s+1)/n), and takes the pair whose
/// [p_i, p_i+1) contains s/n — the SAME floor the shader applies
/// (slot = floor(run × n)), so the two never disagree at a slot edge.
/// Before the first stop the first pair holds (the shader clamps t to 0),
/// past the last the last pair (t to 1); a zero-width pair contains
/// nothing and is never chosen — a hard edge.
fn fill_map(map: &mut [u8], positions: &[f32]) {
    let n: usize = map.len();
    let pairs: usize = positions.len().saturating_sub(1);
    if pairs == 0 {
        map.fill(0);
        return;
    }
    for (s, slot) in map.iter_mut().enumerate() {
        let run: f32 = s as f32 / n as f32;
        let mut pair: usize = if run < positions[0] { 0 } else { pairs - 1 };
        for i in 0..pairs {
            if positions[i] <= run && run < positions[i + 1] {
                pair = i;
                break;
            }
        }
        *slot = pair.min(255) as u8;
    }
}

#[cfg(test)]
mod gradient_table_tests {
    use super::*;

    /// The shader's slot for a run position — the CPU rule's twin.
    fn shader_slot(run: f32, n: usize) -> usize {
        ((run * n as f32).floor() as isize).clamp(0, n as isize - 1) as usize
    }

    #[test]
    fn the_map_names_the_pair_that_contains_each_slot_start() {
        let mut map = vec![0u8; 100];
        fill_map(&mut map, &[0.0, 0.94, 1.0]);
        assert!(map[..94].iter().all(|p| *p == 0) && map[94..].iter().all(|p| *p == 1), "{map:?}");
        // The shader's floor lands a pixel at 0.945 in slot 94 → pair 1,
        // and one at 0.935 in slot 93 → pair 0.
        assert_eq!(map[shader_slot(0.945, 100)], 1);
        assert_eq!(map[shader_slot(0.935, 100)], 0);
    }

    #[test]
    fn edges_hold_and_a_zero_width_pair_is_a_hard_edge() {
        let mut map = vec![0u8; 10];
        // Stops start at 0.3: slots before it take the first pair (the
        // shader clamps t to 0 → the first color holds).
        fill_map(&mut map, &[0.3, 0.6]);
        assert_eq!(map, vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        // A hard edge at 0.3: pair (0.3→0.3) is never chosen; slot 3 (run
        // 0.3) goes straight to the pair after it.
        fill_map(&mut map, &[0.0, 0.3, 0.3, 1.0]);
        assert_eq!(map, vec![0, 0, 0, 2, 2, 2, 2, 2, 2, 2]);
        // A lone pair: everything is it.
        fill_map(&mut map, &[0.0, 1.0]);
        assert!(map.iter().all(|p| *p == 0));
    }

    #[test]
    fn slots_follow_the_box_and_the_period() {
        assert_eq!(GradientTable::slots_for(100.0, (0.0, 0.0)), 150);
        assert_eq!(GradientTable::slots_for(100.0, (0.0, 0.25)), 38);
        assert_eq!(GradientTable::slots_for(2000.0, (0.0, 0.0)), GRAD_MAP_WIDTH);
        assert_eq!(GradientTable::slots_for(0.5, (0.0, 0.0)), 2);
    }

    #[test]
    fn the_fixed_point_encodings_round_trip_under_the_pins_tolerance() {
        for v in [-1.0f32, -0.4, -0.01, 0.0, 0.3, 0.999, 1.0] {
            let back: f32 = encode_unit(v) as f32 / 32767.5 - 1.0;
            assert!((back - v).abs() <= 1.0 / 32767.5, "{v} -> {back}");
        }
        for p in [0.0f32, 0.3, 0.4915, 0.5, 0.94, 1.0] {
            let (hi, lo) = encode_pos(p);
            let back: f32 = (hi as f32 * 65536.0 + lo as f32) / 4_294_967_295.0;
            assert!((back - p).abs() < 1e-6, "{p} -> {back}");
        }
    }

    #[test]
    fn the_hash_follows_the_content() {
        let a = [Draw::GradientStop { pos: 0.0, color: gogo::Color_Straight_Srgb_4xU8_RGBA([1, 2, 3, 4]) }, Draw::GradientStop { pos: 1.0, color: gogo::Color_Straight_Srgb_4xU8_RGBA([5, 6, 7, 8]) }];
        let mut b = a;
        let h = GradientTable::hash(&a, 10, GradientKind::Linear, (0.0, 0.0));
        assert_eq!(h, GradientTable::hash(&b, 10, GradientKind::Linear, (0.0, 0.0)));
        assert_ne!(h, GradientTable::hash(&a, 11, GradientKind::Linear, (0.0, 0.0)), "the slot count is content");
        assert_ne!(h, GradientTable::hash(&a, 10, GradientKind::Radial, (0.0, 0.0)));
        if let Draw::GradientStop { pos, .. } = &mut b[1] {
            *pos = 0.5;
        }
        assert_ne!(h, GradientTable::hash(&b, 10, GradientKind::Linear, (0.0, 0.0)));
    }
}
