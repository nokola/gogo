//! The two kinds of texture that cross this crate's API, each under its
//! own type.
//!
//! Needed because a `wgpu::Texture` does not say what its texels hold. The
//! UI reads a host's picture as sRGB-encoded with STRAIGHT alpha, and it
//! writes its own offscreen tile sRGB-encoded and PREMULTIPLIED. Mixed up,
//! every half-transparent edge comes out too dark or too bright, and
//! nothing fails. With one type per kind the mix-up does not compile.
//!
//! The names follow `gogo-ui`'s `Color_Straight_Srgb_4xU8_RGBA`:
//! `Tex_<role>_<alpha>_<color space>_<channels x type>_<channel map>`, and
//! the wrapped texture is the field named by the channel map (`.RGBA`).
//! Reading that field is the one way out of the type, so a host can find
//! (and forbid, with a source scan) every place a texture leaves its
//! domain.

#![allow(non_camel_case_types, non_snake_case)]

/// A texture the host owns, for the UI to show as an Image: 8 bits per
/// channel, RGBA, color sRGB-ENCODED, alpha STRAIGHT (not premultiplied).
/// The same texels a decoded PNG holds.
///
/// Borrowed, not owned: the host keeps its texture and wraps it for the
/// call (`UiRenderer::set_external_image` keeps its own handle to it).
#[derive(Clone, Copy, Debug)]
pub struct Tex_Image_Straight_Srgb_4xU8_RGBA<'a> {
    pub RGBA: &'a wgpu::Texture,
}

impl<'a> Tex_Image_Straight_Srgb_4xU8_RGBA<'a> {
    /// Wraps `RGBA`. The caller is the one who knows what its texels
    /// hold: wrapping it says they are sRGB-encoded with straight alpha.
    ///
    /// WARNING: the texture's format must hand the texels to the shader
    /// as written (`Rgba8Unorm`). An `*Srgb` format decodes them on the
    /// way and the picture shows too dark; a debug build stops on it.
    pub fn new(RGBA: &'a wgpu::Texture) -> Tex_Image_Straight_Srgb_4xU8_RGBA<'a> {
        debug_assert!(!RGBA.format().is_srgb(), "a Tex_Image_Straight_Srgb_4xU8_RGBA's format must not decode sRGB: {:?}", RGBA.format());
        Tex_Image_Straight_Srgb_4xU8_RGBA { RGBA }
    }
}

/// The texture `UiRenderer::render_to_texture` makes: 8 bits per channel,
/// RGBA, color sRGB-ENCODED and PREMULTIPLIED by alpha, drawn over a
/// transparent clear.
///
/// Not a picture to show through `Tex_Image_Straight_Srgb_4xU8_RGBA`: read
/// as straight alpha, every half-transparent edge darkens twice. The
/// host's own blit has to un-premultiply first, or blend it as
/// premultiplied.
#[derive(Clone, Debug, PartialEq)]
pub struct Tex_Tile_Premul_Srgb_4xU8_RGBA {
    pub RGBA: wgpu::Texture,
}

impl Tex_Tile_Premul_Srgb_4xU8_RGBA {
    /// Wraps `RGBA`: the caller says its texels are sRGB-encoded and
    /// premultiplied. The renderer wraps what it drew; a host has no need
    /// to.
    pub fn new(RGBA: wgpu::Texture) -> Tex_Tile_Premul_Srgb_4xU8_RGBA {
        Tex_Tile_Premul_Srgb_4xU8_RGBA { RGBA }
    }

    /// The width in pixels.
    pub fn width(&self) -> u32 {
        self.RGBA.width()
    }

    /// The height in pixels.
    pub fn height(&self) -> u32 {
        self.RGBA.height()
    }
}
