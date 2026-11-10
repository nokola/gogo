//! `gogo-wgpu` draws a gogo UI with wgpu.
//!
//! `gogo-ui` lays a page out and answers with a list of draw commands;
//! `gogo-text` shapes the text and rasterizes its glyphs. This crate turns
//! the two into GPU work: [`UiRenderer::prepare_pages`] builds one buffer
//! of quads for the frame, and [`UiRenderer::render`] plays it into a
//! render pass the host began. It owns no window, no surface and no
//! device: the host hands those in, so the same renderer serves a phone
//! app, a desktop window and a test that draws into a texture.
//!
//! A frame, in the host's words:
//!
//! ```text
//! ui.prepare_pages(&device, &queue, &[(draws, &page)], epx, (width, height), &mut text, None);
//! let mut pass = encoder.begin_render_pass(&descriptor);
//! ui.render(&mut pass);
//! ```
//!
//! What crosses the API besides wgpu's own types:
//! - pictures the host uploads by `ImageKey` (`write_image_texture_only`);
//! - textures the host owns and the UI shows
//!   ([`Tex_Image_Straight_Srgb_4xU8_RGBA`], `external_image` /
//!   `set_external_image`);
//! - the host's own drawing played inside the UI's pass (`host_layer`);
//! - a page drawn into a texture of its own
//!   ([`Tex_Tile_Premul_Srgb_4xU8_RGBA`], `render_to_texture`);
//! - the timings of a frame's preparation, for a host that asks
//!   ([`PrepareTimings`]).

mod textures;
mod timings;
mod ui;

pub use textures::{Tex_Image_Straight_Srgb_4xU8_RGBA, Tex_Tile_Premul_Srgb_4xU8_RGBA};
pub use timings::{PrepareStage, PrepareTimings};
pub use ui::{DrawLists, ExternalImage, GradientTable, HostLayer, UiRenderer, STRETCH_RASTER_MAX_PX};
