//! The one color-mixing law: gogo mixes AUTHORED colors in premultiplied
//! Oklab. Gradient stops mix this way on the GPU (imaging_wgpu's ui.wgsl
//! mirrors `rgba_from_oklab_pm` exactly), `transition` and `anim` color
//! lerps mix this way on the CPU (`anim::lerp_value`).
//!
//! Needed because the obvious space looks wrong twice: a straight sRGB
//! component lerp darkens cross-hue midpoints (red→green passes through
//! muddy brown) and drags a fade toward a transparent color's rgb (a dark
//! fringe mid-fade). Oklab keeps lightness even across hues;
//! premultiplying keeps fades to transparent clean. CSS `color-mix()`
//! landed on the same default.
//!
//! The law covers mixes between two authored colors only — pixel-coverage
//! blends (the 1-px antialias ramps in the shader) are not color design
//! and stay as they are.

use crate::page::Color_Straight_Srgb_4xU8_RGBA;

/// A premultiplied-Oklab color: `[L·α, a·α, b·α, α]`. The mixing
/// representation — values lerp componentwise and nothing else; make one
/// with `oklab_pm_from_rgba`, leave through `rgba_from_oklab_pm`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OklabPm(pub [f32; 4]);

/// Decodes one sRGB byte to linear light (the IEC 61966-2-1 transfer).
fn linear_from_srgb(u: u8) -> f32 {
    let c = u as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Encodes linear light back to an sRGB byte, clamped and rounded.
fn srgb_from_linear(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let e = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (e * 255.0).round() as u8
}

/// An sRGB color into the mixing representation (Björn Ottosson's Oklab,
/// the L/a/b each premultiplied by alpha).
pub fn oklab_pm_from_rgba(c: Color_Straight_Srgb_4xU8_RGBA) -> OklabPm {
    let (r, g, b) = (linear_from_srgb(c.0[0]), linear_from_srgb(c.0[1]), linear_from_srgb(c.0[2]));
    let l = (0.412_221_47 * r + 0.536_332_54 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_55 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_84 * g + 0.629_978_7 * b).cbrt();
    let alpha = c.0[3] as f32 / 255.0;
    OklabPm([
        (0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s) * alpha,
        (1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s) * alpha,
        (0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s) * alpha,
        alpha,
    ])
}

/// The mixing representation back to sRGB bytes. Out-of-gamut mixes (a
/// saturated pair can leave [0,1] linear) clamp per channel; zero alpha
/// has no hue left to divide out and returns transparent black.
pub fn rgba_from_oklab_pm(c: OklabPm) -> Color_Straight_Srgb_4xU8_RGBA {
    let alpha = c.0[3];
    if alpha <= 0.0 {
        return Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT;
    }
    let (lv, av, bv) = (c.0[0] / alpha, c.0[1] / alpha, c.0[2] / alpha);
    let l = (lv + 0.396_337_78 * av + 0.215_803_76 * bv).powi(3);
    let m = (lv - 0.105_561_346 * av - 0.063_854_17 * bv).powi(3);
    let s = (lv - 0.089_484_18 * av - 1.291_485_5 * bv).powi(3);
    Color_Straight_Srgb_4xU8_RGBA([
        srgb_from_linear(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s),
        srgb_from_linear(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s),
        srgb_from_linear(-0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s),
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}

/// Mixes two authored colors at `t` (0 = `a`, 1 = `b`) — the law, in one
/// call. Endpoints return the input byte-exact so a finished tween or a
/// held anim end pose never drifts by a rounding step.
pub fn mix_rgba(a: Color_Straight_Srgb_4xU8_RGBA, b: Color_Straight_Srgb_4xU8_RGBA, t: f32) -> Color_Straight_Srgb_4xU8_RGBA {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let (la, lb) = (oklab_pm_from_rgba(a).0, oklab_pm_from_rgba(b).0);
    let mut m = [0.0f32; 4];
    for i in 0..4 {
        m[i] = la[i] + (lb[i] - la[i]) * t;
    }
    rgba_from_oklab_pm(OklabPm(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every corner of the cube survives the round trip within one byte
    /// step, and `mix_rgba`'s endpoints are byte-exact by construction.
    #[test]
    fn round_trips_and_exact_endpoints() {
        for &c in &[Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255]), Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255]), Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255]), Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255]), Color_Straight_Srgb_4xU8_RGBA([0, 0, 255, 255]), Color_Straight_Srgb_4xU8_RGBA([200, 0, 0, 255]), Color_Straight_Srgb_4xU8_RGBA([37, 141, 250, 128]), Color_Straight_Srgb_4xU8_RGBA([255, 128, 0, 3])] {
            let back = rgba_from_oklab_pm(oklab_pm_from_rgba(c));
            for i in 0..4 {
                assert!((back.0[i] as i32 - c.0[i] as i32).abs() <= 1, "{c:?} -> {back:?}");
            }
            let other = Color_Straight_Srgb_4xU8_RGBA([10, 200, 30, 200]);
            assert_eq!(mix_rgba(c, other, 0.0), c);
            assert_eq!(mix_rgba(c, other, 1.0), other);
        }
    }

    /// The two artifacts the law exists to kill: a fade to transparent
    /// keeps its hue (premultiplied — transparent BLACK contributes no
    /// black), and a black→white midpoint sits at the perceptual middle
    /// (99: Oklab L=0.5), not the sRGB byte middle.
    #[test]
    fn premultiplied_fades_and_even_midpoints() {
        let mid = mix_rgba(Color_Straight_Srgb_4xU8_RGBA([255, 80, 0, 255]), Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT, 0.5);
        assert_eq!(mid, Color_Straight_Srgb_4xU8_RGBA([255, 80, 0, 128]), "hue held through a fade-out");
        let gray = mix_rgba(Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255]), Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255]), 0.5);
        assert_eq!(gray, Color_Straight_Srgb_4xU8_RGBA([99, 99, 99, 255]));
        // Splitting a mix at its own sample changes nothing: the sample
        // lies ON the Lab line, so a stop inserted with the sampled color
        // leaves a gradient's picture untouched.
        let (a, b) = (Color_Straight_Srgb_4xU8_RGBA([200, 0, 0, 255]), Color_Straight_Srgb_4xU8_RGBA([0, 0, 200, 255]));
        let q = mix_rgba(a, mix_rgba(a, b, 0.5), 0.5);
        let direct = mix_rgba(a, b, 0.25);
        for i in 0..4 {
            assert!((q.0[i] as i32 - direct.0[i] as i32).abs() <= 1, "{q:?} vs {direct:?}");
        }
    }
}
