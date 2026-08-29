//! The draw list: `draw(page, resolved, rects) -> Vec<Draw>` in paint
//! order (a forward walk of the arena: parents before children, siblings
//! in file order, so later = on top). Opacity multiplies down the walk.
//! `gogo` never sees a device pixel; rects are epx.

use crate::layout::{DEFAULT_FONT_SIZE, Rect};
use crate::page::*;
use crate::runtime::Resolved;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
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
    Image { rect: Rect, image: StrId, tint: Rgba },
    /// Two-stop gradient (first/last stop) — the renderer's G5 form.
    Gradient { rect: Rect, kind: GradientKind, from: Rgba, to: Rgba, angle: f32 },
    /// `text`/`key`: the English literal and its translation key.
    Text { rect: Rect, text: StrId, key: StrId, font_size: f32, color: Rgba, halign: TextAlign, valign: TextAlign, wrap: bool },
    ClipPush(Rect),
    ClipPop,
}

pub const DEFAULT_TEXT_COLOR: Rgba = Rgba([0xFF, 0xFF, 0xFF, 0xFF]);
pub const DEFAULT_BUTTON_BACKGROUND: Rgba = Rgba([0x3C, 0x3C, 0x3C, 0xFF]);

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
    let rect = rects[id as usize];
    let opacity = opacity * res.num(page, id, PropKey::Opacity).unwrap_or(1.0).clamp(0.0, 1.0);
    let color = |key: PropKey| res.color(page, id, key).map(|c| mul_alpha(c, opacity));
    let corner = res.num(page, id, PropKey::CornerRadius).unwrap_or(0.0);
    let border = res.num(page, id, PropKey::BorderThickness).unwrap_or(0.0);
    let border_color = color(PropKey::BorderColor).unwrap_or(Rgba::TRANSPARENT);

    match node.kind {
        Kind::View | Kind::Panel | Kind::ListItem => {
            // The plate: View/Panel start on the blank slate — transparent
            // unless the file speaks `background=`.
            if let Some(bg) = color(PropKey::Background) {
                out.push(Draw::Quad { rect, color: bg, corner, border, border_color });
            } else if border > 0.0 {
                out.push(Draw::Quad { rect, color: Rgba::TRANSPARENT, corner, border, border_color });
            }
        }
        Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio => {
            let bg = color(PropKey::Background).unwrap_or(mul_alpha(DEFAULT_BUTTON_BACKGROUND, opacity));
            out.push(Draw::Quad { rect, color: bg, corner, border, border_color });
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                out.push(Draw::Text {
                    rect,
                    text,
                    key,
                    font_size: res.num(page, id, PropKey::FontSize).unwrap_or(DEFAULT_FONT_SIZE),
                    color: color(PropKey::Color).unwrap_or(mul_alpha(DEFAULT_TEXT_COLOR, opacity)),
                    halign: text_align(page, res, id, PropKey::TextHalign, TextAlign::Center),
                    valign: text_align(page, res, id, PropKey::TextValign, TextAlign::Center),
                    wrap: false,
                });
            }
        }
        Kind::Rectangle => {
            let c = color(PropKey::Color).or_else(|| color(PropKey::Background)).unwrap_or(Rgba::TRANSPARENT);
            out.push(Draw::Quad { rect, color: c, corner, border, border_color });
        }
        Kind::Circle => {
            let c = color(PropKey::Color).unwrap_or(Rgba::TRANSPARENT);
            out.push(Draw::Circle { rect, color: c, border, border_color });
        }
        Kind::Gradient => {
            let stops = res.prop(page, id, PropKey::Stops).map(|v| page.list(v).to_vec()).unwrap_or_default();
            let stop_color = |v: &Value| match v {
                Value::VStop { color, .. } => Some(*color),
                Value::VColor(c) => Some(*c),
                _ => None,
            };
            let from = stops.first().and_then(stop_color).unwrap_or(Rgba::TRANSPARENT);
            let to = stops.last().and_then(stop_color).unwrap_or(from);
            let kind = match res.ident(page, id, PropKey::Fill) {
                Some("radial") => GradientKind::Radial,
                _ => GradientKind::Linear,
            };
            out.push(Draw::Gradient { rect, kind, from: mul_alpha(from, opacity), to: mul_alpha(to, opacity), angle: res.num(page, id, PropKey::Angle).unwrap_or(0.0) });
        }
        Kind::Image | Kind::Icon => {
            let src = match res.prop(page, id, if node.kind == Kind::Image { PropKey::Src } else { PropKey::IconName }) {
                Some(Value::VText { text, .. }) => text,
                Some(Value::VIdent(id)) => id,
                _ => NO_STR,
            };
            if src != NO_STR {
                let tint = color(PropKey::Color).unwrap_or(mul_alpha(Rgba([255, 255, 255, 255]), opacity));
                out.push(Draw::Image { rect, image: src, tint });
            }
        }
        Kind::Label => {
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                out.push(Draw::Text {
                    rect,
                    text,
                    key,
                    font_size: res.num(page, id, PropKey::FontSize).unwrap_or(DEFAULT_FONT_SIZE),
                    color: color(PropKey::Color).unwrap_or(mul_alpha(DEFAULT_TEXT_COLOR, opacity)),
                    halign: text_align(page, res, id, PropKey::TextHalign, TextAlign::Start),
                    valign: text_align(page, res, id, PropKey::TextValign, TextAlign::Start),
                    wrap: res.bool(page, id, PropKey::Wrap).unwrap_or(false),
                });
            }
        }
        Kind::Slider => {
            // Track + thumb from `value`/`min`/`max` (G7 wires the drag).
            let min = res.num(page, id, PropKey::Min).unwrap_or(0.0);
            let max = res.num(page, id, PropKey::Max).unwrap_or(100.0);
            let value = res.num(page, id, PropKey::Value).unwrap_or(min);
            let t = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
            let track = Rect::new(rect.x, rect.y + rect.h / 2.0 - 2.0, rect.w, 4.0);
            let track_color = color(PropKey::Background).unwrap_or(mul_alpha(Rgba([0x5A, 0x5A, 0x5A, 0xFF]), opacity));
            let fill = color(PropKey::Color).unwrap_or(mul_alpha(Rgba([0x1B, 0xA1, 0xE2, 0xFF]), opacity));
            out.push(Draw::Quad { rect: track, color: track_color, corner: 2.0, border: 0.0, border_color: Rgba::TRANSPARENT });
            out.push(Draw::Quad { rect: Rect::new(track.x, track.y, track.w * t, track.h), color: fill, corner: 2.0, border: 0.0, border_color: Rgba::TRANSPARENT });
            let thumb = 20.0;
            out.push(Draw::Circle { rect: Rect::new(rect.x + (rect.w - thumb) * t, rect.y + rect.h / 2.0 - thumb / 2.0, thumb, thumb), color: fill, border: 0.0, border_color: Rgba::TRANSPARENT });
        }
        Kind::Input => {
            let bg = color(PropKey::Background).unwrap_or(mul_alpha(Rgba([0x2B, 0x2B, 0x2B, 0xFF]), opacity));
            out.push(Draw::Quad { rect, color: bg, corner, border: border.max(1.0), border_color: if border > 0.0 { border_color } else { mul_alpha(Rgba([0x7F, 0x7F, 0x7F, 0xFF]), opacity) } });
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                out.push(Draw::Text { rect: rect.inset(8.0, 0.0, 8.0, 0.0), text, key, font_size: res.num(page, id, PropKey::FontSize).unwrap_or(DEFAULT_FONT_SIZE), color: color(PropKey::Color).unwrap_or(mul_alpha(DEFAULT_TEXT_COLOR, opacity)), halign: TextAlign::Start, valign: TextAlign::Center, wrap: false });
            }
        }
        _ => {}
    }

    let clip = node.kind == Kind::Scroll;
    if clip {
        out.push(Draw::ClipPush(rect));
    }
    for c in page.children(id) {
        walk(page, res, rects, c as NodeId, opacity, out);
    }
    if clip {
        out.push(Draw::ClipPop);
    }
}

/// The top-most node under (x, y) that takes input: a press surface, a
/// node with handlers, or a Scroll — walking the arena backwards (last
/// painted first), honoring `visibility` and Scroll clips. `any`: return
/// the top-most visible node of any kind instead.
pub fn hit(page: &Page, res: &Resolved, rects: &[Rect], x: f32, y: f32, any: bool) -> Option<NodeId> {
    for i in (0..page.nodes.len()).rev() {
        let id = i as NodeId;
        if !rects[i].contains(x, y) {
            continue;
        }
        let node = &page.nodes[i];
        let takes_input = any || node.kind.is_press_surface() || node.handlers.1 > 0 || matches!(node.kind, Kind::Scroll | Kind::Slider | Kind::Input);
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
}
