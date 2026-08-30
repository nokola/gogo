//! The draw list: `draw(page, resolved, rects) -> Vec<Draw>` in paint
//! order (a forward walk of the arena: parents before children, siblings
//! in file order, so later = on top). Opacity multiplies down the walk.
//! `gogo` never sees a device pixel; rects are epx.

use crate::layout::{CHECK_BOX_EPX, DEFAULT_FONT_SIZE, MARK_TEXT_GAP, Rect, SCROLL_BAR_EPX, TOGGLE_PILL};
use crate::page::*;
use crate::runtime::{DISABLED, Resolved, SELECTED};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
}

/// `fit=` on an Image: how the bitmap maps into the node's own rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Contain,
    Cover,
    Distort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Draw {
    Quad { rect: Rect, color: Rgba, corner: f32, border: f32, border_color: Rgba },
    Circle { rect: Rect, color: Rgba, border: f32, border_color: Rgba },
    /// `image` is the `src` string id; the renderer maps it to a texture.
    Image { rect: Rect, image: StrId, tint: Rgba, fit: Fit },
    /// One segment of a gradient over the whole `rect`: `from` → `to`
    /// across the `band` (0..1 along the gradient's run; `(0, 1)` is the
    /// whole run), nothing painted outside it. A gradient with N stops is
    /// N−1 of these. `angle` is the CSS compass: 0 = to top, 90 = to
    /// right, clockwise.
    Gradient { rect: Rect, kind: GradientKind, from: Rgba, to: Rgba, angle: f32, band: (f32, f32) },
    /// `text`/`key`: the English literal and its translation key.
    Text { rect: Rect, text: StrId, key: StrId, font_size: f32, color: Rgba, halign: TextAlign, valign: TextAlign, wrap: bool, font: StrId },
    ClipPush(Rect),
    ClipPop,
    /// Everything until the matching `RotatePop` turns `angle` degrees
    /// clockwise about (`cx`, `cy`) epx — a node's `rotate` and its subtree.
    RotatePush { angle: f32, cx: f32, cy: f32 },
    RotatePop,
}

pub const DEFAULT_TEXT_COLOR: Rgba = Rgba([0xFF, 0xFF, 0xFF, 0xFF]);

// The default look of the native kinds (spec "Default look"): the same
// numbers on every engine, so a bare `Button text="OK"` is one picture
// everywhere. A file restyles any of it with the ordinary words.
/// A text-only Button's plate: gray 0.1 at 0.6 (Godot's default theme).
pub const DEFAULT_BUTTON_BACKGROUND: Rgba = Rgba([0x1A, 0x1A, 0x1A, 0x99]);
/// The Check box / Radio ring outline.
pub const MARK_BORDER_EPX: f32 = 1.5;
/// The Radio's inner dot when selected.
pub const RADIO_DOT_EPX: f32 = 10.0;
/// The Check's inner square when selected.
pub const CHECK_DOT_EPX: f32 = 10.0;
/// The Toggle's knob, inset 2 in the pill.
pub const TOGGLE_KNOB_EPX: f32 = 18.0;
/// The Toggle pill when off (`gray`); on = `accent`.
pub const TOGGLE_OFF: Rgba = Rgba([0x70, 0x80, 0x90, 0xFF]);
/// The Slider's track: 4 epx, gray, with the filled part white at 0.6,
/// and a white 16-epx knob.
pub const SLIDER_TRACK_EPX: f32 = 4.0;
pub const SLIDER_KNOB_EPX: f32 = 16.0;
pub const SLIDER_TRACK: Rgba = Rgba([0x5A, 0x5A, 0x5A, 0xFF]);
pub const SLIDER_FILL: Rgba = Rgba([0xFF, 0xFF, 0xFF, 0x99]);
/// Content of a `disabled` control dims to this.
pub const DISABLED_OPACITY: f32 = 0.3;
/// The auto scroll bar's grabber (Godot's default theme, measured #737373).
pub const SCROLL_GRABBER: Rgba = Rgba([0x73, 0x73, 0x73, 0xFF]);

pub fn draw(page: &Page, res: &Resolved, rects: &[Rect]) -> Vec<Draw> {
    let mut out = Vec::new();
    if page.nodes.is_empty() {
        return out;
    }
    walk(page, res, rects, 0, 1.0, &mut out);
    out
}

fn mul_alpha(c: Rgba, opacity: f32) -> Rgba {
    let mut c = c;
    c.0[3] = (c.0[3] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
    c
}

fn text_align(page: &Page, res: &Resolved, id: NodeId, key: PropKey, default: TextAlign) -> TextAlign {
    match res.ident(page, id, key) {
        Some("left") | Some("top") | Some("start") => TextAlign::Start,
        Some("center") => TextAlign::Center,
        Some("right") | Some("bottom") | Some("end") => TextAlign::End,
        _ => default,
    }
}

fn walk(page: &Page, res: &Resolved, rects: &[Rect], id: NodeId, opacity: f32, out: &mut Vec<Draw>) {
    if res.gone(page, id) || res.hidden(page, id) {
        return;
    }
    let node = &page.nodes[id as usize];
    let motion = res.motion.get(id as usize).copied().unwrap_or(crate::anim::Motion::IDENTITY);
    let rect = moved(rects[id as usize], &motion);
    let state: u8 = res.states.get(id as usize).copied().unwrap_or(0);
    // A disabled control dims its whole subtree (the file's `when disabled`
    // may say otherwise on top).
    let disabled_dim: f32 = if state & DISABLED != 0 && res.num(page, id, PropKey::Opacity).is_none() { DISABLED_OPACITY } else { 1.0 };
    let opacity = opacity * res.num(page, id, PropKey::Opacity).unwrap_or(1.0).clamp(0.0, 1.0) * motion.opacity.clamp(0.0, 1.0) * disabled_dim;
    let color = |key: PropKey| res.color(page, id, key).map(|c| mul_alpha(c, opacity));
    let corner = res.num(page, id, PropKey::CornerRadius).unwrap_or(0.0);
    let border = res.num(page, id, PropKey::BorderThickness).unwrap_or(0.0);
    let border_color = color(PropKey::BorderColor).unwrap_or(Rgba::TRANSPARENT);
    let text_color = || color(PropKey::FontColor).or_else(|| color(PropKey::Color)).unwrap_or(mul_alpha(DEFAULT_TEXT_COLOR, opacity));
    // Text sits inside the node's padding and border (WPF Padding).
    let pad = res.placement[id as usize].padding;
    let text_rect = rect.inset(pad[0] + border, pad[1] + border, pad[2] + border, pad[3] + border);
    // A templated Check/Radio/Toggle (a body of its own) is a blank toggle
    // surface: no chrome unless the file paints it.
    let templated = node.kind.is_press_surface() && node.kind != Kind::Button && node.children.1 > 0;
    // Scroll viewports and slot regions clip; so does `clip_contents=true`,
    // and a Label with `clip_text=true` cuts its own text.
    let clip = matches!(node.kind, Kind::Scroll | Kind::Slot) || res.bool(page, id, PropKey::ClipContents).unwrap_or(false) || (node.kind == Kind::Label && res.bool(page, id, PropKey::ClipText).unwrap_or(false));
    if clip {
        out.push(Draw::ClipPush(rect));
    }
    let rotated = motion.rotate != 0.0;
    if rotated {
        out.push(Draw::RotatePush { angle: motion.rotate, cx: rect.x + rect.w / 2.0, cy: rect.y + rect.h / 2.0 });
    }

    match node.kind {
        Kind::View | Kind::Panel | Kind::ListItem | Kind::Slot => {
            // The plate: View/Panel start on the blank slate — transparent
            // unless the file speaks `background=`.
            if let Some(bg) = color(PropKey::Background) {
                out.push(Draw::Quad { rect, color: bg, corner, border, border_color });
            } else if border > 0.0 {
                out.push(Draw::Quad { rect, color: Rgba::TRANSPARENT, corner, border, border_color });
            }
        }
        Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio => {
            // Only a Button has a plate by default; a native mark draws its
            // box/ring/pill and nothing behind it.
            let native_mark = node.kind != Kind::Button && !templated;
            let bg = color(PropKey::Background).or_else(|| if templated || native_mark { None } else { Some(mul_alpha(DEFAULT_BUTTON_BACKGROUND, opacity)) });
            if let Some(bg) = bg {
                out.push(Draw::Quad { rect, color: bg, corner, border, border_color });
            } else if border > 0.0 {
                out.push(Draw::Quad { rect, color: Rgba::TRANSPARENT, corner, border, border_color });
            }
            let mut text_rect = text_rect;
            let mut halign_default = TextAlign::Center;
            if native_mark {
                let selected = state & SELECTED != 0;
                let white = mul_alpha(DEFAULT_TEXT_COLOR, opacity);
                let accent = mul_alpha(palette_color("accent").unwrap_or(DEFAULT_TEXT_COLOR), opacity);
                let (mw, mh) = if node.kind == Kind::Toggle { TOGGLE_PILL } else { (CHECK_BOX_EPX, CHECK_BOX_EPX) };
                let mark = Rect::new(text_rect.x, text_rect.y + (text_rect.h - mh) / 2.0, mw, mh);
                match node.kind {
                    Kind::Check => {
                        out.push(Draw::Quad { rect: mark, color: if selected { accent } else { Rgba::TRANSPARENT }, corner: 2.0, border: MARK_BORDER_EPX, border_color: white });
                        if selected {
                            let inset = (CHECK_BOX_EPX - CHECK_DOT_EPX) / 2.0;
                            out.push(Draw::Quad { rect: mark.inset(inset, inset, inset, inset), color: white, corner: 1.0, border: 0.0, border_color: Rgba::TRANSPARENT });
                        }
                    }
                    Kind::Radio => {
                        out.push(Draw::Circle { rect: mark, color: Rgba::TRANSPARENT, border: MARK_BORDER_EPX, border_color: white });
                        if selected {
                            let inset = (CHECK_BOX_EPX - RADIO_DOT_EPX) / 2.0;
                            out.push(Draw::Circle { rect: mark.inset(inset, inset, inset, inset), color: accent, border: 0.0, border_color: Rgba::TRANSPARENT });
                        }
                    }
                    _ => {
                        let pill = if selected { accent } else { mul_alpha(TOGGLE_OFF, opacity) };
                        out.push(Draw::Quad { rect: mark, color: pill, corner: mh / 2.0, border: 0.0, border_color: Rgba::TRANSPARENT });
                        let inset = (mh - TOGGLE_KNOB_EPX) / 2.0;
                        // The knob sits where the finger left it (`value=` 0..1,
                        // written by a drag); with no value, at the selected end.
                        let fraction = res.num(page, id, PropKey::Value).map(|v| v.clamp(0.0, 1.0)).unwrap_or(if selected { 1.0 } else { 0.0 });
                        let x = mark.x + inset + (mark.right() - inset - TOGGLE_KNOB_EPX - (mark.x + inset)) * fraction;
                        out.push(Draw::Circle { rect: Rect::new(x, mark.y + inset, TOGGLE_KNOB_EPX, TOGGLE_KNOB_EPX), color: white, border: 0.0, border_color: Rgba::TRANSPARENT });
                    }
                }
                text_rect = text_rect.inset(mw + MARK_TEXT_GAP, 0.0, 0.0, 0.0);
                halign_default = TextAlign::Start;
            }
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                out.push(Draw::Text {
                    rect: text_rect,
                    text,
                    key,
                    font_size: res.num(page, id, PropKey::FontSize).unwrap_or(DEFAULT_FONT_SIZE),
                    color: text_color(),
                    halign: text_align(page, res, id, PropKey::TextHalign, halign_default),
                    valign: text_align(page, res, id, PropKey::TextValign, TextAlign::Center),
                    wrap: false,
                    font: res.font(page, id).unwrap_or(NO_STR),
                });
            }
        }
        Kind::Rectangle => {
            // A Rectangle with no color is white (Godot's ColorRect default;
            // the swatch squares in new_painting rely on it). A Circle with
            // none draws nothing (gogo_circle.gd).
            let c = color(PropKey::Color).or_else(|| color(PropKey::Background)).unwrap_or(mul_alpha(DEFAULT_TEXT_COLOR, opacity));
            out.push(Draw::Quad { rect, color: c, corner, border, border_color });
        }
        Kind::Circle => {
            let c = color(PropKey::Color).unwrap_or(Rgba::TRANSPARENT);
            out.push(Draw::Circle { rect, color: c, border, border_color });
        }
        Kind::Gradient => {
            let stops = res.prop(page, id, PropKey::Stops).map(|v| page.list(v).to_vec()).unwrap_or_default();
            let kind = match res.ident(page, id, PropKey::GradientFill) {
                Some("radial") => GradientKind::Radial,
                _ => GradientKind::Linear,
            };
            // No `angle=`: left → right (Godot's fill_from (0,0) → fill_to
            // (1,0)), which is 90 on the compass.
            let angle = res.num(page, id, PropKey::Angle).unwrap_or(90.0);
            // (color, position): `@pos` as written, else spread evenly.
            let mut colored: Vec<(Rgba, Option<f32>)> = stops.iter().filter_map(|v| match v {
                Value::VStop { color, pos } => Some((mul_alpha(*color, opacity), *pos)),
                Value::VColor(c) => Some((mul_alpha(*c, opacity), None)),
                _ => None,
            }).collect();
            let n = colored.len();
            for (i, (_, pos)) in colored.iter_mut().enumerate() {
                if pos.is_none() {
                    *pos = Some(if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 });
                }
            }
            let at = |i: usize| colored[i].1.unwrap_or(0.0).clamp(0.0, 1.0);
            match n {
                0 => {}
                1 => out.push(Draw::Gradient { rect, kind, from: colored[0].0, to: colored[0].0, angle, band: (0.0, 1.0) }),
                _ => {
                    // Before the first stop and after the last, the edge color holds.
                    if at(0) > 0.0 {
                        out.push(Draw::Gradient { rect, kind, from: colored[0].0, to: colored[0].0, angle, band: (0.0, at(0)) });
                    }
                    for i in 0..n - 1 {
                        if at(i + 1) > at(i) {
                            out.push(Draw::Gradient { rect, kind, from: colored[i].0, to: colored[i + 1].0, angle, band: (at(i), at(i + 1)) });
                        }
                    }
                    if at(n - 1) < 1.0 {
                        out.push(Draw::Gradient { rect, kind, from: colored[n - 1].0, to: colored[n - 1].0, angle, band: (at(n - 1), 1.0) });
                    }
                }
            }
        }
        Kind::Image | Kind::Icon => {
            let src = match res.prop(page, id, if node.kind == Kind::Image { PropKey::Source } else { PropKey::IconName }) {
                Some(Value::VText { text, .. }) => text,
                Some(Value::VIdent(id)) => id,
                _ => NO_STR,
            };
            if src != NO_STR {
                let tint = color(PropKey::Color).unwrap_or(mul_alpha(Rgba([255, 255, 255, 255]), opacity));
                let fit = match res.ident(page, id, PropKey::Fit) {
                    Some("cover") => Fit::Cover,
                    Some("distort") => Fit::Distort,
                    _ => Fit::Contain,
                };
                out.push(Draw::Image { rect, image: src, tint, fit });
            }
        }
        Kind::Label => {
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                out.push(Draw::Text {
                    rect: text_rect,
                    text,
                    key,
                    font_size: res.num(page, id, PropKey::FontSize).unwrap_or(DEFAULT_FONT_SIZE),
                    color: text_color(),
                    halign: text_align(page, res, id, PropKey::TextHalign, TextAlign::Start),
                    valign: text_align(page, res, id, PropKey::TextValign, TextAlign::Start),
                    wrap: res.bool(page, id, PropKey::Wrap).unwrap_or(false),
                    font: res.font(page, id).unwrap_or(NO_STR),
                });
            }
        }
        Kind::Slider => {
            // Track + thumb from `value`/`min`/`max` (the drag is not wired yet).
            let min = res.num(page, id, PropKey::Min).unwrap_or(0.0);
            let max = res.num(page, id, PropKey::Max).unwrap_or(100.0);
            let value = res.num(page, id, PropKey::Value).unwrap_or(min);
            let t = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
            let track = Rect::new(rect.x, rect.y + (rect.h - SLIDER_TRACK_EPX) / 2.0, rect.w, SLIDER_TRACK_EPX);
            let track_color = color(PropKey::Background).unwrap_or(mul_alpha(SLIDER_TRACK, opacity));
            let fill = color(PropKey::Color).unwrap_or(mul_alpha(SLIDER_FILL, opacity));
            out.push(Draw::Quad { rect: track, color: track_color, corner: SLIDER_TRACK_EPX / 2.0, border: 0.0, border_color: Rgba::TRANSPARENT });
            out.push(Draw::Quad { rect: Rect::new(track.x, track.y, track.w * t, track.h), color: fill, corner: SLIDER_TRACK_EPX / 2.0, border: 0.0, border_color: Rgba::TRANSPARENT });
            let knob = SLIDER_KNOB_EPX;
            out.push(Draw::Circle { rect: Rect::new(rect.x + (rect.w - knob) * t, rect.y + (rect.h - knob) / 2.0, knob, knob), color: mul_alpha(DEFAULT_TEXT_COLOR, opacity), border: 0.0, border_color: Rgba::TRANSPARENT });
        }
        Kind::Input => {
            // Godot's default LineEdit plate: (0.1, 0.1, 0.1) at 0.6, no border.
            let bg = color(PropKey::Background).unwrap_or(mul_alpha(Rgba([0x1A, 0x1A, 0x1A, 0x99]), opacity));
            out.push(Draw::Quad { rect, color: bg, corner, border, border_color });
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                out.push(Draw::Text { rect: text_rect.inset(8.0, 0.0, 8.0, 0.0), text, key, font_size: res.num(page, id, PropKey::FontSize).unwrap_or(DEFAULT_FONT_SIZE), color: text_color(), halign: TextAlign::Start, valign: TextAlign::Center, wrap: false, font: res.font(page, id).unwrap_or(NO_STR) });
            }
        }
        _ => {}
    }

    if motion.translate != [0.0, 0.0] || motion.scale != [1.0, 1.0] {
        // Children move with their parent: shift their rects by the same
        // transform (scale about this node's center).
        let mut moved_rects: Vec<Rect> = rects.to_vec();
        let (cx, cy) = (rects[id as usize].x + rects[id as usize].w / 2.0, rects[id as usize].y + rects[id as usize].h / 2.0);
        for i in (id as usize + 1)..node.subtree_end as usize {
            let r = rects[i];
            moved_rects[i] = Rect::new(cx + (r.x - cx) * motion.scale[0] + motion.translate[0], cy + (r.y - cy) * motion.scale[1] + motion.translate[1], r.w * motion.scale[0], r.h * motion.scale[1]);
        }
        for c in res.paint_children(page, id) {
            walk(page, res, &moved_rects, c, opacity, out);
        }
    } else {
        for c in res.paint_children(page, id) {
            walk(page, res, rects, c, opacity, out);
        }
    }
    if node.kind == Kind::Scroll && res.ident(page, id, PropKey::Bars) != Some("never") {
        // Godot's auto scroll bars: a gray grabber in the right column /
        // bottom row, as long as the viewport's share of the content.
        let content_w = res.children(page, id).iter().map(|c| rects[*c as usize].w).fold(0.0, f32::max);
        let content_h = res.children(page, id).iter().map(|c| rects[*c as usize].h).fold(0.0, f32::max);
        let grabber = mul_alpha(SCROLL_GRABBER, opacity);
        let offset = res.scroll.get(id as usize).copied().unwrap_or([0.0, 0.0]);
        if content_h > rect.h {
            let len = (rect.h * rect.h / content_h).max(8.0);
            let y = rect.y + (rect.h - len) * (offset[1] / (content_h - rect.h)).clamp(0.0, 1.0);
            out.push(Draw::Quad { rect: Rect::new(rect.right() - SCROLL_BAR_EPX, y + 1.0, SCROLL_BAR_EPX, len - 1.0), color: grabber, corner: 0.0, border: 0.0, border_color: Rgba::TRANSPARENT });
        }
        if content_w > rect.w {
            let len = (rect.w * rect.w / content_w).max(8.0);
            let x = rect.x + (rect.w - len) * (offset[0] / (content_w - rect.w)).clamp(0.0, 1.0);
            out.push(Draw::Quad { rect: Rect::new(x + 1.0, rect.bottom() - SCROLL_BAR_EPX, len - 1.0, SCROLL_BAR_EPX), color: grabber, corner: 0.0, border: 0.0, border_color: Rgba::TRANSPARENT });
        }
    }
    if rotated {
        out.push(Draw::RotatePop);
    }
    if clip {
        out.push(Draw::ClipPop);
    }
}

/// The node's own rect under its motion channels (scale about the center).
fn moved(r: Rect, m: &crate::anim::Motion) -> Rect {
    let w = r.w * m.scale[0];
    let h = r.h * m.scale[1];
    Rect::new(r.x + (r.w - w) / 2.0 + m.translate[0], r.y + (r.h - h) / 2.0 + m.translate[1], w, h)
}

/// The nodes in PAINT order (parents before children, siblings by their
/// raise): what `hit` scans backwards, so the touch lands on what the eye
/// sees on top.
fn paint_order(page: &Page, res: &Resolved, id: NodeId, out: &mut Vec<NodeId>) {
    out.push(id);
    for c in res.paint_children(page, id) {
        paint_order(page, res, c, out);
    }
}

/// The top-most node under (x, y) that takes input — walking the paint
/// order backwards (last painted first), honoring `visibility` and Scroll
/// clips. The D103 input model: input-taking KINDS answer by default,
/// everything passive is transparent, always — `interactive=true` declares
/// a surface, `false` hides one, and a wired handler alone never claims
/// input (a full-screen Panel with just `on:show=` must not eat taps).
/// `any`: return the top-most visible node of any kind instead.
pub fn hit(page: &Page, res: &Resolved, rects: &[Rect], x: f32, y: f32, any: bool) -> Option<NodeId> {
    if page.nodes.is_empty() {
        return None;
    }
    let mut order: Vec<NodeId> = Vec::with_capacity(page.nodes.len());
    paint_order(page, res, 0, &mut order);
    for id in order.into_iter().rev() {
        let i = id as usize;
        if !rects[i].contains(x, y) {
            continue;
        }
        let node = &page.nodes[i];
        // `interactive=true` claims input (the catcher plate), `false`
        // makes a control hit-invisible; else input-taking kinds only.
        let interactive = res.bool(page, id, PropKey::Interactive);
        let takes_input = any || interactive == Some(true) || (interactive != Some(false) && (node.kind.is_press_surface() || matches!(node.kind, Kind::Scroll | Kind::Slider | Kind::Input)));
        if !takes_input {
            continue;
        }
        // Hidden/gone, or inside a hidden/gone ancestor, or outside a Scroll ancestor's clip.
        let mut ok = true;
        let mut a = id;
        loop {
            if res.gone(page, a) || res.hidden(page, a) {
                ok = false;
                break;
            }
            let parent = page.nodes[a as usize].parent;
            if parent == NO_NODE {
                break;
            }
            if page.nodes[parent as usize].kind == Kind::Scroll && !rects[parent as usize].contains(x, y) {
                ok = false;
                break;
            }
            a = parent;
        }
        if ok {
            return Some(id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;
    use crate::layout::{FixedAdvance, layout};
    use crate::runtime::{HELD, Runtime, resolve};

    #[test]
    fn paint_order_states_and_hit() {
        let src = "View v {\n Rectangle bg color=charcoal, width=fill, height=fill\n Panel bar background=#222222FF, valign=top, height=56, padding=8 {\n  Row r spacing=8 {\n   Button clear text=\"Clear\", width=96, on:activate=clear { when held background=white/0.5 }\n   Label title text=\"FanRust\"\n  }\n }\n}\n";
        let c = compile(src, &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let mut rt = Runtime::new(&c.page);
        let res = resolve(&c.page, &rt);
        let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let list = draw(&c.page, &res, &rects);
        // bg, bar plate, button plate, button text, label text.
        assert_eq!(list.len(), 5, "{list:#?}");
        assert!(matches!(list[2], Draw::Quad { color, .. } if color == DEFAULT_BUTTON_BACKGROUND));
        let clear = c.page.find("clear").unwrap();
        assert_eq!(hit(&c.page, &res, &rects, 20.0, 20.0, false), Some(clear));
        assert_eq!(hit(&c.page, &res, &rects, 200.0, 400.0, false), None);
        rt.set_state(clear, HELD, true);
        let res = resolve(&c.page, &rt);
        let list = draw(&c.page, &res, &rects);
        assert!(matches!(list[2], Draw::Quad { color, .. } if color == Rgba([255, 255, 255, 128])), "{:?}", list[2]);
    }

    /// PR3 + PR8: the file's start states draw — a selected Radio shows
    /// the accent dot, a selected Toggle the accent pill, a disabled
    /// Button dims — and native marks draw no plate.
    #[test]
    fn start_states_and_native_marks() {
        let src = "View v {\n Column c {\n  choice size value=medium { Radio small text=\"Small\"; Radio medium text=\"Medium\" }\n  Toggle mute text=\"Mute\", selected=true\n  Check save text=\"Save\"\n  Button undo text=\"Undo\", disabled=true\n }\n}\n";
        let c = compile(src, &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let rt = Runtime::new(&c.page);
        let medium = c.page.find("medium").unwrap();
        let small = c.page.find("small").unwrap();
        assert!(rt.has(medium, crate::runtime::SELECTED));
        assert!(!rt.has(small, crate::runtime::SELECTED));
        assert!(rt.has(c.page.find("mute").unwrap(), crate::runtime::SELECTED));
        assert!(rt.has(c.page.find("undo").unwrap(), crate::runtime::DISABLED));
        let res = resolve(&c.page, &rt);
        let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let list = draw(&c.page, &res, &rects);
        let accent = palette_color("accent").unwrap();
        // small: ring + text; medium: ring + dot + text; mute: pill + knob + text; save: box + text; undo: plate + text.
        let circles = list.iter().filter(|d| matches!(d, Draw::Circle { .. })).count();
        assert_eq!(circles, 4, "{list:#?}");
        assert!(list.iter().any(|d| matches!(d, Draw::Circle { color, .. } if *color == accent)), "the medium dot");
        assert!(list.iter().any(|d| matches!(d, Draw::Quad { color, corner, .. } if *color == accent && *corner == TOGGLE_PILL.1 / 2.0)), "the on pill");
        assert!(!list.iter().any(|d| matches!(d, Draw::Quad { color, .. } if *color == DEFAULT_BUTTON_BACKGROUND)), "no plate under a mark; the undo plate is dimmed");
        assert!(list.iter().any(|d| matches!(d, Draw::Text { color, .. } if color.0[3] == (255.0 * DISABLED_OPACITY) as u8 + 1 || color.0[3] == (255.0 * DISABLED_OPACITY).round() as u8)), "undo text dimmed: {list:#?}");
    }
}
