//! A gogo page in a window: the three crates of this repository working
//! together, with the page reloaded whenever its file is saved.
//!
//!     cargo run -p gogo-example-window
//!
//! Then open `examples/window/ui/hello.gogo` in an editor, change a color
//! or a text, and save: the window follows within a quarter of a second.
//! A mistake in the file prints its diagnostics here and the window keeps
//! the last page that compiled.
//!
//! Who does what:
//! - `gogo-ui` compiles the page, keeps its state (`Runtime`), lays it out
//!   and turns the mouse into events (`Input`);
//! - `gogo-text` measures and shapes the text (`TextEngine`);
//! - `gogo-wgpu` draws the frame's draw list (`UiRenderer`);
//! - this file owns the window, the GPU device and the frame loop, and
//!   says what a tap on the button does.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gogo_text::{EpxMeasure, TextEngine};
use gogo_ui::page::NodeId;
use gogo_ui::reload::same_shape;
use gogo_ui::{Draw, Event, EventKind, Input, KeptFrame, MOUSE_ID, Page, Phase, Rect, Runtime, Touch, compile_file, has_errors};
use gogo_wgpu::UiRenderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// How often the page file's save time is looked at.
const RELOAD_CHECK: Duration = Duration::from_millis(250);

const FONT_KEY: &str = "assets/Fonts/Roboto Regular.ttf";

/// The page file, read at run time so that an edit shows without a build.
fn page_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/hello.gogo"))
}

/// One page and everything that lives as long as it does.
struct Ui {
    page: Page,
    runtime: Runtime,
    input: Input,
    kept: KeptFrame,
    draws: Vec<Draw>,
    /// The page file's save time when it was last compiled.
    saved_at: Option<SystemTime>,
    /// What the app itself knows: how often the button was tapped.
    taps: u32,
}

impl Ui {
    /// Compiles the page file, or says why it could not.
    fn compile() -> Result<Page, String> {
        let compiled = compile_file(&page_path())?;
        if has_errors(&compiled.diags) {
            let lines: Vec<String> = compiled.diags.iter().map(|d| format!("{:?} {} at line {}: {}", d.severity, d.code, d.line, d.message)).collect();
            return Err(lines.join("\n"));
        }
        Ok(compiled.page)
    }

    fn saved_at() -> Option<SystemTime> {
        std::fs::metadata(page_path()).and_then(|m| m.modified()).ok()
    }

    fn new() -> Ui {
        let page: Page = Ui::compile().unwrap_or_else(|why| panic!("{} does not compile:\n{why}", page_path().display()));
        let runtime: Runtime = Runtime::new(&page);
        let mut ui = Ui { page, runtime, input: Input::new(), kept: KeptFrame::default(), draws: Vec::new(), saved_at: Ui::saved_at(), taps: 0 };
        ui.show_taps();
        ui
    }

    /// Compiles the file again when it was saved since the last look.
    /// Returns whether a new page is in.
    fn reload_if_saved(&mut self) -> bool {
        let saved_at: Option<SystemTime> = Ui::saved_at();
        if saved_at == self.saved_at {
            return false;
        }
        self.saved_at = saved_at;
        let mut page: Page = match Ui::compile() {
            Ok(page) => page,
            Err(why) => {
                eprintln!("the saved page does not compile, keeping the last one:\n{why}");
                return false;
            }
        };
        // The same nodes in the same order: the runtime's state (a held
        // button, a scroll offset, a running animation) carries over.
        // Another node list starts fresh.
        self.runtime = match same_shape(&self.page, &page) {
            Ok(()) => self.runtime.carried_to(&self.page, &mut page),
            Err(_) => Runtime::new(&page),
        };
        self.page = page;
        self.input = Input::new();
        self.kept = KeptFrame::default();
        self.show_taps();
        println!("reloaded {}", page_path().display());
        true
    }

    /// Writes the tap count into the label named `counter`, if the page
    /// has one.
    fn show_taps(&mut self) {
        if let Some(counter) = self.page.find("counter") {
            let text: String = if self.taps == 1 { "1 tap".to_string() } else { format!("{} taps", self.taps) };
            self.runtime.set_text(&mut self.page, counter, &text);
        }
    }

    /// What the page's events mean to this app: a completed tap on the
    /// node named `tapButton` counts.
    fn handle(&mut self, events: &[Event]) {
        let button: Option<NodeId> = self.page.find("tapButton");
        for event in events {
            if event.kind == EventKind::Activate && Some(event.node) == button {
                self.taps += 1;
                self.show_taps();
            }
        }
    }
}

/// The window and the GPU objects that draw into it.
struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: UiRenderer,
}

struct App {
    ui: Ui,
    text: RefCell<TextEngine>,
    gfx: Option<Gfx>,
    /// The mouse, in device pixels.
    cursor: (f32, f32),
    is_mouse_down: bool,
    started: Instant,
    last_frame: Instant,
}

impl App {
    fn new() -> App {
        let font: Vec<u8> = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Regular.ttf")).expect("the font file");
        App { ui: Ui::new(), text: RefCell::new(TextEngine::new(&[(FONT_KEY, font)])), gfx: None, cursor: (0.0, 0.0), is_mouse_down: false, started: Instant::now(), last_frame: Instant::now() }
    }

    /// Device pixels per epx: the screen's own scale, so a size written
    /// in a `.gogo` file is the same finger-sized thing on every screen.
    fn epx(&self) -> f32 {
        self.gfx.as_ref().map_or(1.0, |g| g.window.scale_factor() as f32)
    }

    /// One mouse event into the page, in epx.
    fn pointer(&mut self, phase: Phase) {
        if !self.ui.kept.is_laid_out() {
            return;
        }
        let epx: f32 = self.epx();
        let (x, y) = (self.cursor.0 / epx, self.cursor.1 / epx);
        let measure = EpxMeasure { engine: &self.text, epx, font: None, images: &[] };
        let events: Vec<Event> = if phase == Phase::Move && !self.is_mouse_down {
            self.ui.input.hover(&self.ui.page, &mut self.ui.runtime, self.ui.kept.frame(), x, y);
            Vec::new()
        } else {
            let touch = Touch { phase, x, y, t: self.started.elapsed().as_secs_f64(), id: MOUSE_ID };
            self.ui.input.touch(&self.ui.page, &mut self.ui.runtime, self.ui.kept.frame(), &measure, touch)
        };
        self.ui.handle(&events);
        if let Some(gfx) = &self.gfx {
            gfx.window.request_redraw();
        }
    }

    /// One frame: advance, lay out, draw.
    fn frame(&mut self) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        let now: Instant = Instant::now();
        let dt: f32 = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let epx: f32 = gfx.window.scale_factor() as f32;
        let size: (u32, u32) = (gfx.config.width, gfx.config.height);
        let ui: &mut Ui = &mut self.ui;

        // 1. Advance animations and flings.
        let mut is_animating: bool = ui.runtime.tick(&mut ui.page, dt);
        if ui.kept.is_laid_out() {
            is_animating |= ui.input.tick(&ui.page, &mut ui.runtime, ui.kept.frame(), dt);
        }

        // 2. Lay out for this window and take the draw list.
        let measure = EpxMeasure { engine: &self.text, epx, font: None, images: &[] };
        let viewport: Rect = Rect::new(0.0, 0.0, size.0 as f32 / epx, size.1 as f32 / epx);
        ui.draws.clear();
        ui.kept.frame_retained(&mut ui.runtime, &ui.page, viewport, &measure, &mut ui.draws, None);
        is_animating |= ui.runtime.is_moving();

        // 3. Draw.
        gfx.renderer.prepare_pages(&gfx.device, &gfx.queue, &[(ui.draws.as_slice(), &ui.page)], epx, size, &mut self.text.borrow_mut(), None);
        let frame = match gfx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            // A window that is hidden, being resized or lost its surface:
            // configure again and try on the next frame.
            _ => {
                gfx.surface.configure(&gfx.device, &gfx.config);
                gfx.window.request_redraw();
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = gfx.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("the window"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            gfx.renderer.render(&mut pass);
        }
        gfx.queue.submit([encoder.finish()]);
        gfx.queue.present(frame);
        if is_animating {
            gfx.window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attributes = Window::default_attributes().with_title("gogo: hello window").with_inner_size(LogicalSize::new(420.0, 640.0));
        let window: Arc<Window> = Arc::new(event_loop.create_window(attributes).expect("a window"));
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface: wgpu::Surface<'static> = instance.create_surface(window.clone()).expect("a surface for the window");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).expect("a GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("a device");
        let caps = surface.get_capabilities(&adapter);
        // A format that stores colors as written: the UI renderer writes
        // sRGB-encoded values, and an `*Srgb` format would encode them a
        // second time (everything too bright).
        let format: wgpu::TextureFormat = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: Default::default(),
        };
        surface.configure(&device, &config);
        let renderer: UiRenderer = UiRenderer::new(&device, format);
        window.request_redraw();
        self.gfx = Some(Gfx { window, surface, device, queue, config, renderer });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut() {
                    gfx.config.width = size.width.max(1);
                    gfx.config.height = size.height.max(1);
                    gfx.surface.configure(&gfx.device, &gfx.config);
                    gfx.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(gfx) = &self.gfx {
                    gfx.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                self.pointer(Phase::Move);
            }
            WindowEvent::CursorLeft { .. } => {
                self.ui.input.unhover(&self.ui.page, &mut self.ui.runtime);
                if let Some(gfx) = &self.gfx {
                    gfx.window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.is_mouse_down = state == ElementState::Pressed;
                self.pointer(if self.is_mouse_down { Phase::Down } else { Phase::Up });
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    /// Between events: look whether the page file was saved, then sleep
    /// until the next look. Frames come from `request_redraw`, so a still
    /// page draws nothing and uses no CPU.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.ui.reload_if_saved() {
            if let Some(gfx) = &self.gfx {
                gfx.window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + RELOAD_CHECK));
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("an event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("the event loop runs");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click on the button counts one tap and the label says so: the
    /// page file, the event road and this file's `handle` agree on the two
    /// node names. Runs with no window and no GPU.
    #[test]
    fn a_click_on_the_button_counts_a_tap() {
        let font: Vec<u8> = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Regular.ttf")).expect("the font file");
        let text: RefCell<TextEngine> = RefCell::new(TextEngine::new(&[(FONT_KEY, font)]));
        let mut ui: Ui = Ui::new();
        let measure = EpxMeasure { engine: &text, epx: 1.0, font: None, images: &[] };
        ui.kept.frame_retained(&mut ui.runtime, &ui.page, Rect::new(0.0, 0.0, 420.0, 640.0), &measure, &mut ui.draws, None);
        assert!(!ui.draws.is_empty(), "the page draws something");
        let button: NodeId = ui.page.find("tapButton").expect("the page names the button");
        let counter: NodeId = ui.page.find("counter").expect("the page names the counter");
        assert_eq!(ui.runtime.text_of(&ui.page, counter), Some("0 taps"));
        let r: Rect = ui.kept.rects()[button.index()];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        for (phase, t) in [(Phase::Down, 0.0), (Phase::Up, 0.05)] {
            let events: Vec<Event> = ui.input.touch(&ui.page, &mut ui.runtime, ui.kept.frame(), &measure, Touch { phase, x, y, t, id: MOUSE_ID });
            ui.handle(&events);
        }
        assert_eq!(ui.taps, 1);
        assert_eq!(ui.runtime.text_of(&ui.page, counter), Some("1 tap"));
    }
}
