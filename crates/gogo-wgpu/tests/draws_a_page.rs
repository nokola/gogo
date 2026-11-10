//! End to end on a real GPU: a `.gogo` page compiled, laid out with
//! real text measures, drawn into a texture by `UiRenderer`, read back.
//!
//! Needed because nothing else in this repository runs the shader: the
//! unit tests cover the gradient tables only, and a renderer whose quads
//! are right but whose pipeline draws nothing would pass them all.
//!
//! Ignored unless asked for: it needs a GPU adapter, which a hosted test
//! machine may not have, and a test that returned early there would count
//! as passed without having run. On a machine with a GPU:
//!
//!     cargo test -p gogo-wgpu -- --include-ignored

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gogo_text::{EpxMeasure, TextEngine};
use gogo_ui::layout::Rect;
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{Compiled, Draw, KeptFrame, Page, compile, draw, has_errors};
use gogo_wgpu::{PrepareStage, PrepareTimings, Tex_Tile_Premul_Srgb_4xU8_RGBA, UiRenderer};

const PAGE: &str = r#"
View card {
    Panel plate width=120, height=60, halign=left, valign=top, background=#E51400 {
        Label hello text="Hi", font_size=32, font_color=#FFFFFF, halign=center, valign=center
    }
}
"#;

const TILE: (u32, u32) = (160, 100);
const FONT_KEY: &str = "assets/Fonts/Roboto Regular.ttf";

/// A device on any backend built in (`WGPU_BACKEND` picks one, as in
/// wgpu's own tools).
fn device() -> (wgpu::Device, wgpu::Queue) {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    if let Some(backends) = wgpu::Backends::from_env() {
        desc.backends = backends;
    }
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("this test needs a GPU adapter");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("a device of that adapter")
}

/// The tile's pixels, RGBA, row by row.
fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, tile: &Tex_Tile_Premul_Srgb_4xU8_RGBA) -> Vec<u8> {
    let (w, h) = (tile.width(), tile.height());
    let padded: u32 = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("tile readback"), size: (padded * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tile.RGBA, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(10)) }).expect("the GPU answers within ten seconds");
    let mut pixels: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    {
        let data = slice.get_mapped_range().expect("the read is mapped");
        for y in 0..h as usize {
            let start: usize = y * padded as usize;
            pixels.extend_from_slice(&data[start..start + (w * 4) as usize]);
        }
    }
    buffer.unmap();
    pixels
}

/// Writes down what a host's timings are told, to pin the order of the
/// calls.
#[derive(Default)]
struct Told {
    log: Arc<Mutex<Vec<String>>>,
}

impl PrepareTimings for Told {
    fn stage_begins(&mut self, stage: PrepareStage) {
        self.log.lock().expect("the log").push(format!("begin {stage:?}"));
    }

    fn stage_ends(&mut self, stage: PrepareStage) {
        self.log.lock().expect("the log").push(format!("end {stage:?}"));
    }

    fn text_shaped(&mut self, seconds: f32) {
        assert!(seconds >= 0.0);
        self.log.lock().expect("the log").push("text".to_string());
    }
}

#[test]
#[ignore = "needs a GPU adapter: run with --include-ignored on a machine that has one"]
fn a_page_drawn_to_a_texture_shows_its_plate_and_its_text() {
    let compiled: Compiled = compile(PAGE, &|_| None);
    assert!(!has_errors(&compiled.diags), "{:?}", compiled.diags);
    let page: Page = compiled.page;

    let font: Vec<u8> = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Regular.ttf")).expect("the font");
    let text: RefCell<TextEngine> = RefCell::new(TextEngine::new(&[(FONT_KEY, font)]));
    let runtime: Runtime = Runtime::new(&page);
    let screen: Rect = Rect::new(0.0, 0.0, TILE.0 as f32, TILE.1 as f32);
    let measure = EpxMeasure { engine: &text, epx: 1.0, font: None, images: &[] };
    let kept: KeptFrame = KeptFrame::fresh(&page, resolve(&page, &runtime), screen, &measure);
    let plate: Rect = kept.frame().rects()[page.find("plate").expect("the plate").index()];
    let label: Rect = kept.frame().rects()[page.find("hello").expect("the label").index()];
    assert_eq!((plate.x, plate.y, plate.w, plate.h), (0.0, 0.0, 120.0, 60.0));
    let draws: Vec<Draw> = draw(&page, kept.frame());

    let (device, queue) = device();
    let mut ui: UiRenderer = UiRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    // A renderer can be built on one thread and used on another, with
    // timings installed or without.
    fn crosses_threads<T: Send + Sync>(_: &T) {}
    crosses_threads(&ui);
    let told = Told::default();
    let log = told.log.clone();
    ui.set_prepare_timings(Some(Box::new(told)));
    let tile: Tex_Tile_Premul_Srgb_4xU8_RGBA = ui.render_to_texture(&device, &queue, &draws, &page, &mut text.borrow_mut(), None, 1.0, (0.0, 0.0), TILE);
    assert_eq!((tile.width(), tile.height()), TILE);
    let pixels: Vec<u8> = read_back(&device, &queue, &tile);
    let at = |x: u32, y: u32| -> [u8; 4] {
        let i: usize = ((y * TILE.0 + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };

    // The plate's own color, away from the letters.
    assert_eq!(at(4, 4), [0xE5, 0x14, 0x00, 0xFF], "the plate's corner");
    // Nothing outside the plate: the clear shows, transparent.
    assert_eq!(at(140, 80), [0, 0, 0, 0], "outside the plate");
    // The letters: white ink somewhere inside the label's rect.
    let (x0, y0, x1, y1) = (label.x as u32, label.y as u32, (label.x + label.w) as u32, (label.y + label.h) as u32);
    let white: usize = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| at(x, y) == [0xFF, 0xFF, 0xFF, 0xFF]).count();
    assert!(white > 20, "the label's letters are drawn: {white} white pixels in {label:?}");

    // The timings heard every stage once, in order, with the text time
    // inside the walk.
    assert_eq!(log.lock().expect("the log").as_slice(), ["begin Pass", "text", "end Pass", "begin Atlas", "end Atlas", "begin Upload", "end Upload"]);
}
