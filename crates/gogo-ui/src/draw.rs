//! The draw list: `draw(page, resolved, rects) -> Vec<Draw>` in paint
//! order (a forward walk of the arena: parents before children, siblings
//! in file order, so later = on top). Opacity multiplies down the walk.
//! `gogo` never sees a device pixel; rects are epx.

use crate::anim::Motion;
use crate::layout::{CHECK_BOX_EPX, DEFAULT_FONT_SIZE, LookCopy, MARK_TEXT_GAP, Rect, SCROLL_BAR_EPX, TOGGLE_PILL};
use crate::page::*;
use crate::runtime::{DISABLED, ResolveChanges, Resolved, SELECTED};
use crate::tree::PlacedTree;

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

/// `sampling=` on an Image: `Smooth` blends between texels, `Pixel`
/// shows each texel as a hard block (the canvas zoomed far in).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampling {
    Smooth,
    Pixel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    /// A sweep around the box center (`fill=conic`).
    Conic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Draw {
    Quad { rect: Rect, color: Color_Straight_Srgb_4xU8_RGBA, corner: f32, border: f32, border_color: Color_Straight_Srgb_4xU8_RGBA },
    /// A plate whose corners differ (`corner_radius_top_left=` …): radii
    /// clockwise from top-left. The uniform case stays `Quad`.
    QuadCorners { rect: Rect, color: Color_Straight_Srgb_4xU8_RGBA, corners: [f32; 4], border: f32, border_color: Color_Straight_Srgb_4xU8_RGBA },
    /// A circular stroke: `from` degrees on the compass (0 = up,
    /// clockwise), `sweep` degrees, `thickness` the stroke width in epx.
    /// `dash` = (offset, dash, period) in degrees around the ring: the
    /// stroke shows where the angle from `from`, minus `offset`, falls in
    /// the first `dash` of every `period` — the renderer's mask, ONE quad
    /// however many dashes (period 0 = solid). A pattern with several
    /// dash/gap pairs is one Arc per pair, all over the same period.
    Arc { rect: Rect, color: Color_Straight_Srgb_4xU8_RGBA, from: f32, sweep: f32, thickness: f32, dash: (f32, f32, f32) },
    Circle { rect: Rect, color: Color_Straight_Srgb_4xU8_RGBA, border: f32, border_color: Color_Straight_Srgb_4xU8_RGBA },
    /// `image` is the build-wide image id (`source=`); the renderer
    /// indexes its texture slots by it. `window` is the texture rectangle
    /// shown (`window=`, texture pixels; `None` = all of it) — the fit
    /// maps that rectangle, not the whole texture, into `rect`.
    Image { rect: Rect, image: ImageKey, tint: Color_Straight_Srgb_4xU8_RGBA, fit: Fit, window: Option<[f32; 4]>, sampling: Sampling },
    /// A gradient over `rect`, drawn ONCE: the `stops` entries right after
    /// this one in the list are its [`Draw::GradientStop`]s, positions
    /// non-decreasing along the run (0..1). The renderer keeps the stops
    /// in a table and finds each pixel's pair by a lookup, so a gradient
    /// is one quad however many stops it has (D21) — never a quad per
    /// pair, which visited every pixel once per pair. Before the first
    /// stop and after the last the edge color holds; two stops at one
    /// position are a hard edge. `angle` is the CSS compass: 0 = to top,
    /// 90 = to right, clockwise. `corner` is the node's `corner_radius`
    /// (epx): the renderer masks the ramp to the rounded rect, half the
    /// box = a disc. `period` is (start, length) along the run in 0..1 of
    /// the box: length 0 = the run is the box; length > 0 = the ramp
    /// TILES — `repeat=repeat|mirror` with `from=`/`to=` makes that span
    /// one period and the stops are 0..1 of the period, repeated across
    /// the whole box (a linear fill only). `repeat=mirror` folds the stops
    /// into 0..0.5 and appends their mirror.
    Gradient { rect: Rect, kind: GradientKind, angle: f32, corner: f32, period: (f32, f32), stops: u32 },
    /// One stop of the [`Draw::Gradient`] just before it — a data record,
    /// never drawn on its own: the position along the run and the color
    /// as authored, the node's opacity applied (the renderer converts it
    /// to the mixing space once, when the stop table row fills).
    GradientStop { pos: f32, color: Color_Straight_Srgb_4xU8_RGBA },
    /// `text`/`key`: the literal as the file wrote it and its translation
    /// key (`NO_KEY` = none). The renderer shows `locale::display(page,
    /// locale, text, key)` — the same string layout measured.
    Text { rect: Rect, text: StrId, key: KeyId, font_size: f32, color: Color_Straight_Srgb_4xU8_RGBA, halign: TextAlign, valign: TextAlign, wrap: bool, font: StrId },
    /// A Label with `text_fit=stretch`: the text's ONE glyph, its ink box
    /// stretched to fill `rect` (each axis its own stretch), mirrored by
    /// `flip` (horizontal, vertical). The renderer rasterizes the glyph
    /// at a size near the rect's larger side and maps the ink box onto
    /// the rect; text of more than one glyph draws only the first.
    TextStretch { rect: Rect, text: StrId, key: KeyId, color: Color_Straight_Srgb_4xU8_RGBA, font: StrId, flip: (bool, bool) },
    /// The focused Input's caret. The renderer shapes the first `index`
    /// chars of the text to place it — glyph positions live where glyphs
    /// live, never in this layout-space list (GPAR112).
    Caret { rect: Rect, text: StrId, key: KeyId, font_size: f32, font: StrId, index: u32, color: Color_Straight_Srgb_4xU8_RGBA },
    /// The focused Input's selected span: the renderer measures the two
    /// prefixes (`lo`, `hi` in chars) and fills between them — pushed
    /// BEFORE the text so the glyphs draw over the highlight.
    Selection { rect: Rect, text: StrId, key: KeyId, font_size: f32, font: StrId, lo: u32, hi: u32, color: Color_Straight_Srgb_4xU8_RGBA },
    /// The two drag circles below a touch-born selection (`lo`/`hi`
    /// char indices — the renderer measures the prefixes, glyph
    /// positions live there). The input machine grab-checks the same
    /// `input::sel_handle_rect`, so the picture and the finger agree.
    SelHandles { rect: Rect, text: StrId, key: KeyId, font_size: f32, font: StrId, lo: u32, hi: u32, color: Color_Straight_Srgb_4xU8_RGBA },
    ClipPush(Rect),
    ClipPop,
    /// GP33 `automatic=arc`: everything until the pop shows only inside
    /// the angular wedge [from, from + span] about (cx, cy) — the fan
    /// mask behind a Knob's progress ring. Degrees, Godot's screen
    /// convention (0 = right, clockwise positive); span <= 0 hides all.
    WedgePush { cx: f32, cy: f32, from: f32, span: f32 },
    WedgePop,
    /// Everything until the matching `RotatePop` turns `angle` degrees
    /// clockwise about (`cx`, `cy`) epx — a node's `rotate` and its subtree.
    RotatePush { angle: f32, cx: f32, cy: f32 },
    RotatePop,
    /// Everything until the matching `SwingPop` turns `angle` degrees
    /// about the vertical axis through `pivot_x` (the node's LEFT edge —
    /// a door on its hinge), seen in perspective: a camera `focal` epx in
    /// front of (`cx`, `cy`), the node's center, so the swung-away side
    /// shrinks toward the middle. Silverlight's PlaneProjection, the WP7
    /// turnstile: `focal` = half the node's height over tan(57°/2)
    /// (`swing_focal`).
    SwingPush { angle: f32, pivot_x: f32, cx: f32, cy: f32, focal: f32 },
    SwingPop,
}

impl Draw {
    /// The same command moved by (`dx`, `dy`) epx with every color's alpha
    /// scaled by `alpha` — a page sliding in or fading out (the navigation's
    /// page poses). Needed because a page lays out ONCE at rest and the
    /// slide is a pose over that layout, not a relayout per frame.
    pub fn posed(&self, dx: f32, dy: f32, alpha: f32) -> Draw {
        let mv = |r: &Rect| Rect::new(r.x + dx, r.y + dy, r.w, r.h);
        let fade = |c: &Color_Straight_Srgb_4xU8_RGBA| Color_Straight_Srgb_4xU8_RGBA([c.0[0], c.0[1], c.0[2], (c.0[3] as f32 * alpha).round().clamp(0.0, 255.0) as u8]);
        match self {
            Draw::Quad { rect, color, corner, border, border_color } => Draw::Quad { rect: mv(rect), color: fade(color), corner: *corner, border: *border, border_color: fade(border_color) },
            Draw::QuadCorners { rect, color, corners, border, border_color } => Draw::QuadCorners { rect: mv(rect), color: fade(color), corners: *corners, border: *border, border_color: fade(border_color) },
            Draw::Arc { rect, color, from, sweep, thickness, dash } => Draw::Arc { rect: mv(rect), color: fade(color), from: *from, sweep: *sweep, thickness: *thickness, dash: *dash },
            Draw::Circle { rect, color, border, border_color } => Draw::Circle { rect: mv(rect), color: fade(color), border: *border, border_color: fade(border_color) },
            Draw::Image { rect, image, tint, fit, window, sampling } => Draw::Image { rect: mv(rect), image: *image, tint: fade(tint), fit: *fit, window: *window, sampling: *sampling },
            Draw::Gradient { rect, kind, angle, corner, period, stops } => Draw::Gradient { rect: mv(rect), kind: *kind, angle: *angle, corner: *corner, period: *period, stops: *stops },
            Draw::GradientStop { pos, color } => Draw::GradientStop { pos: *pos, color: fade(color) },
            Draw::Text { rect, text, key, font_size, color, halign, valign, wrap, font } => Draw::Text { rect: mv(rect), text: *text, key: *key, font_size: *font_size, color: fade(color), halign: *halign, valign: *valign, wrap: *wrap, font: *font },
            Draw::TextStretch { rect, text, key, color, font, flip } => Draw::TextStretch { rect: mv(rect), text: *text, key: *key, color: fade(color), font: *font, flip: *flip },
            Draw::Caret { rect, text, key, font_size, font, index, color } => Draw::Caret { rect: mv(rect), text: *text, key: *key, font_size: *font_size, font: *font, index: *index, color: fade(color) },
            Draw::Selection { rect, text, key, font_size, font, lo, hi, color } => Draw::Selection { rect: mv(rect), text: *text, key: *key, font_size: *font_size, font: *font, lo: *lo, hi: *hi, color: fade(color) },
            Draw::SelHandles { rect, text, key, font_size, font, lo, hi, color } => Draw::SelHandles { rect: mv(rect), text: *text, key: *key, font_size: *font_size, font: *font, lo: *lo, hi: *hi, color: fade(color) },
            Draw::ClipPush(rect) => Draw::ClipPush(mv(rect)),
            Draw::ClipPop => Draw::ClipPop,
            Draw::WedgePush { cx, cy, from, span } => Draw::WedgePush { cx: cx + dx, cy: cy + dy, from: *from, span: *span },
            Draw::WedgePop => Draw::WedgePop,
            Draw::RotatePush { angle, cx, cy } => Draw::RotatePush { angle: *angle, cx: cx + dx, cy: cy + dy },
            Draw::RotatePop => Draw::RotatePop,
            Draw::SwingPush { angle, pivot_x, cx, cy, focal } => Draw::SwingPush { angle: *angle, pivot_x: pivot_x + dx, cx: cx + dx, cy: cy + dy, focal: *focal },
            Draw::SwingPop => Draw::SwingPop,
        }
    }
}

/// The WP7 camera: Silverlight's PlaneProjection sees an element under a
/// 57° field of view, the camera as far in front as makes the element
/// fill it at zero depth — so the perspective of a swing looks the same
/// on a tile and on a page.
pub const SWING_FOV_DEG: f32 = 57.0;

/// The camera distance (epx) for a box `height` epx tall (`SwingPush`'s
/// `focal`).
pub fn swing_focal(height: f32) -> f32 {
    height.max(1.0) / 2.0 / (SWING_FOV_DEG / 2.0).to_radians().tan()
}

pub const DEFAULT_TEXT_COLOR: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0xFF, 0xFF, 0xFF, 0xFF]);

// The default look of the native kinds (spec "Default look"): the same
// numbers on every engine, so a bare `Button text="OK"` is one picture
// everywhere. A file restyles any of it with the ordinary words.
/// A text-only Button's plate: gray 0.1 at 0.6 (Godot's default theme).
pub const DEFAULT_BUTTON_BACKGROUND: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0x1A, 0x1A, 0x1A, 0x99]);
/// The Check box / Radio ring outline.
pub const MARK_BORDER_EPX: f32 = 1.5;
/// The Radio's inner dot when selected.
pub const RADIO_DOT_EPX: f32 = 10.0;
/// The Check's inner square when selected.
pub const CHECK_DOT_EPX: f32 = 10.0;
/// The Toggle's knob, inset 2 in the pill.
pub const TOGGLE_KNOB_EPX: f32 = 18.0;
/// The Toggle pill when off (`gray`); on = `accent`.
pub const TOGGLE_OFF: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0x70, 0x80, 0x90, 0xFF]);
/// The Slider's track: 4 epx, gray, with the filled part white at 0.6,
/// and a white 16-epx knob.
pub const SLIDER_TRACK_EPX: f32 = 4.0;
pub const SLIDER_KNOB_EPX: f32 = 16.0;
pub const SLIDER_TRACK: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0x5A, 0x5A, 0x5A, 0xFF]);
pub const SLIDER_FILL: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0xFF, 0xFF, 0xFF, 0x99]);
pub use crate::page::DISABLED_OPACITY;
/// The auto scroll bar's grabber (Godot's default theme, measured #737373).
pub const SCROLL_GRABBER: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0x73, 0x73, 0x73, 0xFF]);

pub fn draw(page: &Page, res: &Resolved, rects: &[Rect]) -> Vec<Draw> {
    let mut out = Vec::new();
    if page.nodes.is_empty() {
        return out;
    }
    walk(page, res, rects, NodeId::new(0), 1.0, None, &mut out, None, Xform::IDENTITY);
    out
}

/// The draw list of ONE node and its descendants, in the paint order
/// `draw` gives them — a text callout on its way into a bake tile
/// (todo-rust 2.8.3).
///
/// Needed so a bake draws a subtree through the one walk the screen uses:
/// what the user sees is what bakes, and the two cannot drift.
///
/// The root starts at full opacity — an ancestor's `opacity=` is not in
/// the list, only the subtree's own. Rects stay page epx; the renderer
/// shifts and scales them (`UiRenderer::render_to_texture`). Empty for a
/// node the layout has no rect for.
pub fn draw_subtree(page: &Page, res: &Resolved, rects: &[Rect], root: NodeId) -> Vec<Draw> {
    let mut out = Vec::new();
    draw_subtree_into(page, res, rects, root, &mut out);
    out
}

/// `draw_subtree` into a kept buffer (cleared first) — the frame road's
/// `above_children` split, allocation-free once warm.
pub fn draw_subtree_into(page: &Page, res: &Resolved, rects: &[Rect], root: NodeId, out: &mut Vec<Draw>) {
    out.clear();
    if rects.len() > root.index() {
        walk(page, res, rects, root, 1.0, None, out, None, Xform::IDENTITY);
    }
}

/// The page's draw list without ONE subtree — the half of a page that
/// paints UNDER its child pages when `draw_subtree` of the same node paints
/// over them.
///
/// Needed because a child page (a component the host lays out over its
/// parent page) paints after the whole parent, while shipped's XAML put
/// some parent chrome above the children — the clone page's back chevron
/// over its intro screen (CopyPastePage.xaml:163-167). That chrome leaves
/// this list and joins the draws after the children.
pub fn draw_except(page: &Page, res: &Resolved, rects: &[Rect], skip: NodeId) -> Vec<Draw> {
    let mut out = Vec::new();
    draw_except_into(page, res, rects, skip, &mut out);
    out
}

/// `draw_except` into a kept buffer (cleared first) — the frame road's
/// `above_children` split, allocation-free once warm.
pub fn draw_except_into(page: &Page, res: &Resolved, rects: &[Rect], skip: NodeId, out: &mut Vec<Draw>) {
    out.clear();
    if page.nodes.is_empty() {
        return;
    }
    walk(page, res, rects, NodeId::new(0), 1.0, Some(skip), out, None, Xform::IDENTITY);
}

/// The dashed border (`border_dash=`): one pass around the plate's
/// perimeter — top edge left→right, right edge down, bottom edge right→
/// left, left edge up — the pattern running continuously across the
/// corners like WPF's StrokeDashArray. The stroke sits INSIDE the box like
/// the solid border; each dash is a crisp axis-aligned quad (D257).
/// No dash mask: the stroke shows along its whole sweep.
pub const SOLID: (f32, f32, f32) = (0.0, 0.0, 0.0);

/// A dashed stroke around a ring — a Circle's `border_dash=` or an Arc's
/// `dash=`: lengths in stroke units measured along the stroke's CENTERLINE
/// (the box's half-side minus half the stroke), the pattern starting at
/// `from` and running clockwise; an odd list repeats to even out; the last
/// dash is cut where the sweep ends (shipped MoldCursor's DrawArc loop,
/// `end = min(a + dash, TAU)`). ONE `Draw::Arc` per dash/gap PAIR — the
/// renderer masks the dashes per pixel, so a `[4, 3]` ring is one quad
/// whatever its circumference (48 full-disc quads was measured at ~50x the
/// fill of a masked one).
fn dashed_arcs(out: &mut Vec<Draw>, rect: Rect, color: Color_Straight_Srgb_4xU8_RGBA, from: f32, sweep: f32, thickness: f32, pattern: &[f32]) {
    let mut lengths: Vec<f32> = pattern.iter().map(|n| n * thickness).collect();
    if lengths.len() % 2 == 1 {
        lengths.extend_from_slice(&pattern.iter().map(|n| n * thickness).collect::<Vec<f32>>());
    }
    let radius: f32 = rect.w.min(rect.h) / 2.0 - thickness / 2.0;
    if lengths.iter().any(|l| *l <= 0.0) || radius <= 0.0 {
        return;
    }
    let degrees_per_epx: f32 = 360.0 / (std::f32::consts::TAU * radius);
    let period: f32 = lengths.iter().sum::<f32>() * degrees_per_epx;
    let mut offset: f32 = 0.0;
    for pair in lengths.chunks(2) {
        let dash: f32 = pair[0] * degrees_per_epx;
        out.push(Draw::Arc { rect, color, from, sweep, thickness, dash: (offset, dash, period) });
        offset += (pair[0] + pair[1]) * degrees_per_epx;
    }
}

fn dash_quads(out: &mut Vec<Draw>, rect: Rect, border: f32, color: Color_Straight_Srgb_4xU8_RGBA, pattern: &[f32]) {
    let mut lengths: Vec<f32> = pattern.iter().map(|n| n * border).collect();
    if lengths.len() % 2 == 1 {
        lengths.extend_from_slice(&pattern.iter().map(|n| n * border).collect::<Vec<f32>>());
    }
    if lengths.iter().any(|l| *l <= 0.0) || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    // (start point, direction, length) per edge, clockwise from top-left.
    let edges: [((f32, f32), (f32, f32), f32); 4] = [
        ((rect.x, rect.y), (1.0, 0.0), rect.w),
        ((rect.x + rect.w, rect.y), (0.0, 1.0), rect.h),
        ((rect.x + rect.w, rect.y + rect.h), (-1.0, 0.0), rect.w),
        ((rect.x, rect.y + rect.h), (0.0, -1.0), rect.h),
    ];
    let mut i: usize = 0; // which pattern entry
    let mut left: f32 = lengths[0]; // what remains of it
    let mut on: bool = true;
    for (start, dir, len) in edges {
        let mut t: f32 = 0.0;
        while t < len {
            let run: f32 = left.min(len - t);
            if on && run > 0.0 {
                // The dash along the edge, `border` thick toward the inside.
                let (x0, y0) = (start.0 + dir.0 * t, start.1 + dir.1 * t);
                let (x1, y1) = (start.0 + dir.0 * (t + run), start.1 + dir.1 * (t + run));
                let quad: Rect = if dir.1 == 0.0 {
                    let top: f32 = if dir.0 > 0.0 { y0 } else { y0 - border };
                    Rect::new(x0.min(x1), top, (x1 - x0).abs(), border)
                } else {
                    let leftx: f32 = if dir.1 > 0.0 { x0 - border } else { x0 };
                    Rect::new(leftx, y0.min(y1), border, (y1 - y0).abs())
                };
                out.push(Draw::Quad { rect: quad, color, corner: 0.0, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
            }
            t += run;
            left -= run;
            if left <= 0.0001 {
                i = (i + 1) % lengths.len();
                left = lengths[i];
                on = !on;
            }
        }
    }
}

fn mul_alpha(c: Color_Straight_Srgb_4xU8_RGBA, opacity: f32) -> Color_Straight_Srgb_4xU8_RGBA {
    let mut c = c;
    c.0[3] = (c.0[3] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
    c
}

fn text_align(page: &Page, res: &Resolved, id: NodeId, key: PropKey, default: TextAlign) -> TextAlign {
    // Per-axis words (the compile G0057 fence refuses the rest at load).
    let horizontal = key == PropKey::TextHalign;
    match (res.ident(page, id, key), horizontal) {
        (Some("left"), true) | (Some("top"), false) => TextAlign::Start,
        (Some("center"), _) => TextAlign::Center,
        (Some("right"), true) | (Some("bottom"), false) => TextAlign::End,
        _ => default,
    }
}

/// Does a `when disabled` on this node (its own or a style's) write
/// `opacity`? Then the file decides the disabled look, not the built-in.
fn authored_disabled_opacity(page: &Page, id: NodeId) -> bool {
    let n = &page.nodes[id];
    page.whens[n.whens.0 as usize..(n.whens.0 + n.whens.1) as usize]
        .iter()
        .any(|w| w.state == State::StateDisabled && page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize].iter().any(|p| p.key == PropKey::Opacity))
}

/// Does this box cut its subtree at its own rectangle? Scroll viewports
/// and slot regions clip; so does `clip_contents=true`, and a Label with
/// `clip_text=true` cuts its own text. A WRITTEN size is hard too: a box
/// with an exact `width=`/`height=` or a `max_*` cap cuts whatever pokes
/// past it on that axis — the human wrote the number, so the box is
/// neither enlarged nor spilled. Two children are left alone: one hanging
/// out through a NEGATIVE margin (a badge, the app-bar labels — "stick
/// out on purpose") and an `overlay=true` one (outside the box's
/// accounting by declaration). A Label with a written size always cuts —
/// only the renderer can measure its text.
///
/// One answer for two consumers, so the picture and the finger never
/// disagree (GPAR37): draw pushes the clip and `hit` rejects a point a
/// clipping ancestor cut — a button's clipped half takes no taps.
pub(crate) fn clips(page: &Page, res: &Resolved, rects: &[Rect], id: NodeId) -> bool {
    let node = &page.nodes[id];
    let pl = &res.placement[id];
    let written_x = pl.max_width.is_some() || pl.max_width_fill || matches!(pl.width, Len::Exact(_));
    let written_y = pl.max_height.is_some() || pl.max_height_fill || matches!(pl.height, Len::Exact(_));
    let written_overflow = (written_x || written_y)
        && (node.kind == Kind::Label || {
            let own = rects[id];
            let neg = |m: Option<f32>| m.map_or(false, |v| v < 0.0);
            // Placed children: what is laid out inside this box decides
            // whether it clips, seat moves included (B25).
            res.children(page, id).any(|c| {
                let r = rects[c];
                let pc = &res.placement[c];
                if res.gone(page, c) || res.bool(page, c, PropKey::Overlay).unwrap_or(false) {
                    return false;
                }
                let px = written_x && ((r.x < own.x - 0.01 && !neg(pc.margin[0])) || (r.x + r.w > own.x + own.w + 0.01 && !neg(pc.margin[2])));
                let py = written_y && ((r.y < own.y - 0.01 && !neg(pc.margin[1])) || (r.y + r.h > own.y + own.h + 0.01 && !neg(pc.margin[3])));
                px || py
            })
        });
    matches!(node.kind, Kind::Scroll | Kind::List | Kind::Slot) || res.bool(page, id, PropKey::ClipContents).unwrap_or(false) || (node.kind == Kind::Label && res.bool(page, id, PropKey::ClipText).unwrap_or(false)) || written_overflow
}

/// The retained draw's bookkeeping for one walk (B36-DES stage 3,
/// `DrawCache`): what last frame's list holds per node, so a node whose
/// subtree is clean has its run COPIED from that list instead of walked,
/// and the tables the walk fills in for the next frame.
struct Patch<'p> {
    /// Last frame's list — the source of the copied runs.
    prev: &'p [Draw],
    /// Per node: its run in `prev` (start, end) — the node's own commands
    /// and its whole subtree's, pops included.
    span_old: &'p [(u32, u32)],
    /// Per node: the walk's inputs from outside the subtree last frame —
    /// the ancestors' opacity product and whether an ancestor's
    /// translate/scale moved the rects (a non-identity `Xform` in
    /// `walk`) — and
    /// the frame stamp of the last visit. A run is copied only when all
    /// three match: same opacity, no moved rects either time, visited
    /// last frame (a subtree under a box that was `gone` was not walked
    /// and holds a stale run).
    ctx_old: &'p [(f32, bool, u32)],
    /// Per node: does its subtree (itself included) hold a node whose
    /// look or rect changed? Only a clean subtree is copied.
    dirty_sub: &'p [bool],
    /// The placed tree, for stamping a copied subtree's nodes.
    tree: &'p PlacedTree,
    /// This frame's stamp.
    frame: u32,
    // Filled by the walk.
    span_new: Vec<(u32, u32)>,
    ctx_new: Vec<(f32, bool, u32)>,
    /// Per node: the `clips` answer (carried over for copied nodes) —
    /// the cache asks it for a changed node's parent next frame.
    clip: Vec<bool>,
    stack: Vec<NodeId>,
    /// Commands copied rather than emitted (the locality counter's
    /// other half).
    copied: usize,
}

impl Patch<'_> {
    /// Books a copied run: every node of `id`'s subtree that the last
    /// walk visited keeps last frame's span moved by `delta`, its
    /// context, and takes this frame's stamp. A node the last walk did
    /// not reach (under a `gone` or `space_only` box, a repeat slot not
    /// live) keeps its stale stamp and is not descended into — its run
    /// is not in the list, and when its box shows again it must be
    /// walked, never copied.
    fn shift_subtree(&mut self, id: NodeId, delta: i64) {
        let last: u32 = self.frame.wrapping_sub(1);
        self.stack.clear();
        self.stack.push(id);
        while let Some(d) = self.stack.pop() {
            let i = d.index();
            if self.ctx_old[i].2 != last {
                continue;
            }
            let (s, e) = self.span_old[i];
            self.span_new[i] = ((s as i64 + delta) as u32, (e as i64 + delta) as u32);
            self.ctx_new[i] = (self.ctx_old[i].0, self.ctx_old[i].1, self.frame);
            self.stack.extend(self.tree.children(d));
        }
    }
}

/// One node and its subtree onto `out`. `patch`: the retained road's
/// books (`None` = the plain walk); `xf`: what the ancestors' translate
/// and scale did to every rect under them (`Xform`) — a run under a
/// non-identity one is never copied: it was made from other rects.
#[allow(clippy::too_many_arguments)]
fn walk(page: &Page, res: &Resolved, rects: &[Rect], id: NodeId, opacity: f32, skip: Option<NodeId>, out: &mut Vec<Draw>, mut patch: Option<&mut Patch<'_>>, xf: Xform) {
    let start: usize = out.len();
    let under_motion: bool = !xf.is_identity();
    if let Some(p) = patch.as_deref_mut() {
        let (o, m, stamp) = p.ctx_old[id.index()];
        if !under_motion && !m && stamp == p.frame.wrapping_sub(1) && o == opacity && !p.dirty_sub[id.index()] {
            let (s, e) = p.span_old[id.index()];
            out.extend_from_slice(&p.prev[s as usize..e as usize]);
            p.copied += (e - s) as usize;
            p.shift_subtree(id, start as i64 - s as i64);
            return;
        }
        p.span_new[id.index()] = (start as u32, start as u32);
        p.ctx_new[id.index()] = (opacity, under_motion, p.frame);
        p.clip[id.index()] = false;
    }
    if skip == Some(id) || res.gone(page, id) || res.hidden(page, id) {
        return;
    }
    let node = &page.nodes[id];
    let motion = res.motion.get(id.index()).copied().unwrap_or(crate::anim::Motion::IDENTITY);
    let pivot = pivot_frac(page, res, id);
    let base: Rect = xf.apply(rects[id]);
    let rect = moved(base, &motion, pivot);
    // `scale` means the whole picture at that size (Godot's canvas
    // transform, GPAR43): the rects shrink in the child shift below, and
    // the sizes that come from props, not rects — glyph size, corner
    // radius, border width — are multiplied over this walk's draws at the
    // end. Without the second half, `when held scale=0.9` on a captioned
    // button keeps full-size text poking out of a 90 % plate.
    let scaled_from = if motion.scale != [1.0, 1.0] { Some(out.len()) } else { None };
    let state: u8 = res.states.get(id.index()).copied().unwrap_or(0);
    // One value per node: the ladder's `opacity` (declared, `when`, code)
    // with the built-in disabled dim as its bottom rung — an authored
    // `when disabled opacity=` replaces the dim, a plain `opacity=` does
    // not — and an anim that owns the channel on top of everything. The
    // ancestors' product multiplies down the tree; nothing else does.
    let own: f32 = if res.anim_owns(id, crate::anim::Channel::Opacity) {
        res.num(page, id, PropKey::Opacity).unwrap_or(1.0)
    } else if state & DISABLED != 0 && !authored_disabled_opacity(page, id) {
        page.disabled_opacity()
    } else {
        res.num(page, id, PropKey::Opacity).unwrap_or(1.0)
    };
    let opacity = opacity * own.clamp(0.0, 1.0);
    let color = |key: PropKey| res.color(page, id, key).map(|c| mul_alpha(c, opacity));
    let corner = res.num(page, id, PropKey::CornerRadius).unwrap_or(0.0);
    // Per-corner radii: the file-order walk decides — the uniform
    // `corner_radius=` writes all four, a per-corner key after it wins
    // that corner, a uniform after a per-corner key stomps it back.
    let corners = corner_set(page, res, id);
    let uniform = corners[0] == corners[1] && corners[1] == corners[2] && corners[2] == corners[3];
    // `border_dash=`: the dash/gap lengths in border_thickness units (WPF
    // StrokeDashArray; an odd list repeats to even out). A dashed plate
    // draws its fill with no border, then the dashes as crisp quads
    // walking the perimeter (D257: never antialiased line ends).
    let dash: Vec<f32> = res.prop(page, id, PropKey::BorderDash).map(|v| page.list(v).iter().filter_map(|x| if let Value::VNum(n) = x { Some(*n) } else { None }).collect()).unwrap_or_default();
    let plate_quad = |out: &mut Vec<Draw>, rect: Rect, color: Color_Straight_Srgb_4xU8_RGBA, border: f32, border_color: Color_Straight_Srgb_4xU8_RGBA| {
        if !dash.is_empty() && border > 0.0 {
            if uniform {
                out.push(Draw::Quad { rect, color, corner: corners[0], border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
            } else {
                out.push(Draw::QuadCorners { rect, color, corners, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
            }
            dash_quads(out, rect, border, border_color, &dash);
        } else if uniform {
            out.push(Draw::Quad { rect, color, corner: corners[0], border, border_color });
        } else {
            out.push(Draw::QuadCorners { rect, color, corners, border, border_color });
        }
    };
    let border = res.num(page, id, PropKey::BorderThickness).unwrap_or(0.0);
    let border_color = color(PropKey::BorderColor).unwrap_or(Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT);
    // Text color is `font_color=` alone — `color=` on a text box is refused
    // at build (G2002), so no fallback reads it.
    let text_color = || res.font_color(page, id).map(|c| mul_alpha(c, opacity)).unwrap_or(mul_alpha(DEFAULT_TEXT_COLOR, opacity));
    // Text sits inside the node's padding and border (WPF Padding).
    let pad = res.placement[id].padding;
    let text_rect = rect.inset(pad[0] + border, pad[1] + border, pad[2] + border, pad[3] + border);
    // A templated Check/Radio/Toggle (a body of its own) is a blank toggle
    // surface: no chrome unless the file paints it.
    let templated = node.kind.is_press_surface() && node.kind != Kind::Button && node.children.1 > 0;
    let clip = clips(page, res, rects, id);
    if let Some(p) = patch.as_deref_mut() {
        p.clip[id.index()] = clip;
    }
    if clip {
        out.push(Draw::ClipPush(rect));
    }
    // GP33 `automatic=arc`: the wedge [from, from + fraction x sweep] is
    // the mask this slot's content shows through — content-independent
    // (a Gradient ring under it reveals by value the same way), and hits
    // agree (`wedge_contains`). A full span needs no mask.
    let wedge = if let crate::page::Auto::Arc { from, sweep, .. } = node.auto {
        let span = sweep * res.auto_frac(page, id);
        if span < 360.0 {
            out.push(Draw::WedgePush { cx: rect.x + rect.w / 2.0, cy: rect.y + rect.h / 2.0, from, span });
            true
        } else {
            false
        }
    } else {
        false
    };
    let rotated = motion.rotate != 0.0;
    if rotated {
        out.push(Draw::RotatePush { angle: motion.rotate, cx: rect.x + rect.w * pivot.0, cy: rect.y + rect.h * pivot.1 });
    }
    // The `tap_pulse` turnstile (and an anim's `swing` lane): a door on
    // its left edge, the camera on the box's center.
    let swung = motion.swing != 0.0;
    if swung {
        out.push(Draw::SwingPush { angle: motion.swing, pivot_x: rect.x, cx: rect.x + rect.w / 2.0, cy: rect.y + rect.h / 2.0, focal: swing_focal(rect.h) });
    }

    // Any box has a plate (docs/gogo-design.md §8, E8 2026-08-31):
    // `background=` / a border on a Label, Image, Layers, stack or code
    // control draws behind its content exactly as on a Panel, padding
    // included. Nothing is drawn unless the file speaks a plate word, so
    // a plain box costs no quad. The kinds listed paint their own plate
    // (a Button's default chrome, a Slider's track, an Input's dark box)
    // or ARE their fill (Rectangle, Circle, Gradient).
    let paints_own_plate = matches!(node.kind, Kind::View | Kind::Panel | Kind::ListItem | Kind::Slot | Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio | Kind::Rectangle | Kind::Circle | Kind::Gradient | Kind::Slider | Kind::Input);
    if !paints_own_plate {
        if let Some(bg) = color(PropKey::Background) {
            plate_quad(out, rect, bg, border, border_color);
        } else if border > 0.0 {
            plate_quad(out, rect, Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT, border, border_color);
        }
    }

    match node.kind {
        Kind::View | Kind::Panel | Kind::ListItem | Kind::Slot => {
            // The plate: View/Panel start on the blank slate — transparent
            // unless the file speaks `background=`.
            if let Some(bg) = color(PropKey::Background) {
                plate_quad(out, rect, bg, border, border_color);
            } else if border > 0.0 {
                plate_quad(out, rect, Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT, border, border_color);
            }
        }
        Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio => {
            // Only a Button has a plate by default; a native mark draws its
            // box/ring/pill and nothing behind it.
            let native_mark = node.kind != Kind::Button && !templated;
            let bg = color(PropKey::Background).or_else(|| if templated || native_mark { None } else { Some(mul_alpha(DEFAULT_BUTTON_BACKGROUND, opacity)) });
            if let Some(bg) = bg {
                plate_quad(out, rect, bg, border, border_color);
            } else if border > 0.0 {
                plate_quad(out, rect, Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT, border, border_color);
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
                        out.push(Draw::Quad { rect: mark, color: if selected { accent } else { Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT }, corner: 2.0, border: MARK_BORDER_EPX, border_color: white });
                        if selected {
                            let inset = (CHECK_BOX_EPX - CHECK_DOT_EPX) / 2.0;
                            out.push(Draw::Quad { rect: mark.inset(inset, inset, inset, inset), color: white, corner: 1.0, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
                        }
                    }
                    Kind::Radio => {
                        out.push(Draw::Circle { rect: mark, color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT, border: MARK_BORDER_EPX, border_color: white });
                        if selected {
                            let inset = (CHECK_BOX_EPX - RADIO_DOT_EPX) / 2.0;
                            out.push(Draw::Circle { rect: mark.inset(inset, inset, inset, inset), color: accent, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
                        }
                    }
                    _ => {
                        let pill = if selected { accent } else { mul_alpha(TOGGLE_OFF, opacity) };
                        out.push(Draw::Quad { rect: mark, color: pill, corner: mh / 2.0, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
                        let inset = (mh - TOGGLE_KNOB_EPX) / 2.0;
                        // The knob sits where the finger left it (`value=` 0..1,
                        // written by a drag); with no value, at the selected end.
                        let fraction = res.num(page, id, PropKey::Value).map(|v| v.clamp(0.0, 1.0)).unwrap_or(if selected { 1.0 } else { 0.0 });
                        let x = mark.x + inset + (mark.right() - inset - TOGGLE_KNOB_EPX - (mark.x + inset)) * fraction;
                        out.push(Draw::Circle { rect: Rect::new(x, mark.y + inset, TOGGLE_KNOB_EPX, TOGGLE_KNOB_EPX), color: white, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
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
                    font_size: res.font_size(page, id).unwrap_or(DEFAULT_FONT_SIZE),
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
            let c = color(PropKey::Color).unwrap_or(mul_alpha(DEFAULT_TEXT_COLOR, opacity));
            plate_quad(out, rect, c, border, border_color);
        }
        Kind::Circle => {
            let c = color(PropKey::Color).unwrap_or(Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT);
            // A Circle in a non-square box is a CENTERED DISC — bounding
            // square min(width, height), centered both axes (shipped
            // semantics, gogo_circle.gd `_draw`). Needed because the
            // renderer paints a Circle as a max-rounded rect over its
            // whole quad — handed the full rect it would paint a pill.
            let d = rect.w.min(rect.h);
            let disc = Rect::new(rect.x + (rect.w - d) / 2.0, rect.y + (rect.h - d) / 2.0, d, d);
            if !dash.is_empty() && border > 0.0 {
                // A dashed ring: the disc's fill with no ring, then the
                // dashes as arcs (a circle's stroke is antialiased anyway —
                // D257 is about straight dashes feathering into their gaps).
                out.push(Draw::Circle { rect: disc, color: c, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
                dashed_arcs(out, disc, border_color, 0.0, 360.0, border, &dash);
            } else {
                out.push(Draw::Circle { rect: disc, color: c, border, border_color });
            }
        }
        Kind::Arc => {
            let c = color(PropKey::Color).unwrap_or(Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT);
            let from = res.num(page, id, PropKey::From).unwrap_or(0.0);
            let sweep = res.num(page, id, PropKey::Sweep).unwrap_or(360.0);
            let thickness = res.num(page, id, PropKey::Thickness).unwrap_or(1.0);
            let arc_dash: Vec<f32> = res.prop(page, id, PropKey::Dash).map(|v| page.list(v).iter().filter_map(|x| if let Value::VNum(n) = x { Some(*n) } else { None }).collect()).unwrap_or_default();
            if arc_dash.is_empty() {
                out.push(Draw::Arc { rect, color: c, from, sweep, thickness, dash: SOLID });
            } else {
                dashed_arcs(out, rect, c, from, sweep, thickness, &arc_dash);
            }
        }
        Kind::Gradient => {
            let stops = res.prop(page, id, PropKey::Stops).map(|v| page.list(v).to_vec()).unwrap_or_default();
            let kind = match res.ident(page, id, PropKey::GradientFill) {
                Some("radial") => GradientKind::Radial,
                Some("conic") => GradientKind::Conic,
                _ => GradientKind::Linear,
            };
            let repeat_word: Option<&str> = res.ident(page, id, PropKey::GradientRepeat);
            let mirror = repeat_word == Some("mirror");
            // `repeat=repeat|mirror` with a `from=`/`to=` span: the span
            // is one PERIOD and the ramp tiles across the box.
            let tiles = matches!(repeat_word, Some("mirror") | Some("repeat"));
            // No `angle=`: left → right (Godot's fill_from (0,0) → fill_to
            // (1,0)), which is 90 on the compass.
            let mut angle = res.num(page, id, PropKey::Angle).unwrap_or(90.0);
            // `from=`/`to=` place the ramp inside the box (0..1 points).
            // For a LINEAR fill both fold exactly into the angle (the
            // from→to direction) and a band remap along the crossing;
            // for radial/conic the compile accepts them but the renderer
            // still centers — a recorded render follow-up (GP40).
            let point = |key: PropKey| match res.prop(page, id, key) {
                Some(Value::VSize(Some(x), Some(y))) => Some((x, y)),
                _ => None,
            };
            let span = match (point(PropKey::From), point(PropKey::To)) {
                (Some(f), Some(t)) if kind == GradientKind::Linear && (f != t) => {
                    let d = ((t.0 - f.0) * rect.w, (t.1 - f.1) * rect.h);
                    let len = (d.0 * d.0 + d.1 * d.1).sqrt();
                    let dn = (d.0 / len, d.1 / len);
                    angle = dn.0.atan2(-dn.1).to_degrees();
                    let dot = |x: f32, y: f32| x * dn.0 + y * dn.1;
                    let projs = [dot(0.0, 0.0), dot(rect.w, 0.0), dot(0.0, rect.h), dot(rect.w, rect.h)];
                    let lo = projs.iter().cloned().fold(f32::INFINITY, f32::min);
                    let hi = projs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    let norm = |p: (f32, f32)| (dot(p.0 * rect.w, p.1 * rect.h) - lo) / (hi - lo).max(1e-6);
                    Some((norm(f), norm(t)))
                }
                _ => None,
            };
            // (color, position): `@pos` as written, else spread evenly —
            // the compiler holds `@` to all-or-none (G2005), so a list is
            // wholly pinned or wholly even, never a mix.
            let mut colored: Vec<(Color_Straight_Srgb_4xU8_RGBA, Option<f32>)> = stops.iter().filter_map(|v| match v {
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
            // The declared stops stay LIVE as numbered properties (GP40
            // step 4): a `when`/code write of `stop_<n>_color`/`_offset`
            // re-colors or moves that ONE stop (1-based, declaration
            // order). A driven offset clamps to stay non-decreasing —
            // the declared `@` order stays law (G2005); a mid-motion
            // write clamps instead of erroring.
            let mut live_off = false;
            for (i, s) in colored.iter_mut().enumerate() {
                if let Some(Value::VColor(c)) = res.prop(page, id, PropKey::StopColor(i as u8 + 1)) {
                    s.0 = mul_alpha(c, opacity);
                }
                if let Some(Value::VNum(p)) = res.prop(page, id, PropKey::StopOffset(i as u8 + 1)) {
                    s.1 = Some(p.clamp(0.0, 1.0));
                    live_off = true;
                }
            }
            if live_off {
                let mut last = 0.0;
                for s in colored.iter_mut() {
                    if let Some(p) = s.1 {
                        let c = p.max(last);
                        s.1 = Some(c);
                        last = c;
                    }
                }
            }
            // With `repeat=mirror` the ramp runs 0..0.5 and mirrors back
            // (unpinned stops spread over the half); the fold and its
            // mirror are emitted as two band runs.
            if mirror {
                for (i, (_, pos)) in colored.iter_mut().enumerate() {
                    *pos = Some(pos.unwrap_or(if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 * 0.5 }).min(0.5));
                }
            }
            let period: (f32, f32) = match span {
                Some((a, b)) if tiles && kind == GradientKind::Linear && b > a => (a, b - a),
                _ => (0.0, 0.0),
            };
            let remap = |t: f32| match span {
                Some(_) if period.1 > 0.0 => t, // period-local: the renderer tiles it
                Some((a, b)) => (a + t * (b - a)).clamp(0.0, 1.0),
                None => t,
            };
            let at = |i: usize| remap(colored[i].1.unwrap_or(0.0).clamp(0.0, 1.0));
            // ONE entry, then its stops (D21). The renderer holds the edge
            // colors past the first and last stop by itself; a lone stop
            // becomes the same color at both ends so there is a pair.
            if n > 0 {
                let head: usize = out.len();
                out.push(Draw::Gradient { rect, kind, angle, corner, period, stops: 0 });
                if n == 1 {
                    out.push(Draw::GradientStop { pos: 0.0, color: colored[0].0 });
                    out.push(Draw::GradientStop { pos: 1.0, color: colored[0].0 });
                } else {
                    for i in 0..n {
                        out.push(Draw::GradientStop { pos: at(i), color: colored[i].0 });
                    }
                    if mirror {
                        // The fold's mirror: the same stops back from 1.
                        for i in (0..n).rev() {
                            out.push(Draw::GradientStop { pos: 1.0 - at(i), color: colored[i].0 });
                        }
                    }
                }
                let count: u32 = (out.len() - head - 1) as u32;
                if let Draw::Gradient { stops, .. } = &mut out[head] {
                    *stops = count;
                }
            }
        }
        Kind::Image => {
            let src: ImageKey = match res.prop(page, id, PropKey::Source) {
                Some(Value::VImage(key)) => key,
                _ => NO_IMAGE,
            };
            if src != NO_IMAGE {
                let tint = color(PropKey::Color).unwrap_or(mul_alpha(Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255]), opacity));
                let fit = match res.ident(page, id, PropKey::Fit) {
                    Some("cover") => Fit::Cover,
                    Some("distort") => Fit::Distort,
                    _ => Fit::Contain,
                };
                let window: Option<[f32; 4]> = res.prop(page, id, PropKey::Window).and_then(|v| {
                    let nums: Vec<f32> = page.list(v).iter().filter_map(|x| if let Value::VNum(n) = x { Some(*n) } else { None }).collect();
                    (nums.len() == 4).then(|| [nums[0], nums[1], nums[2], nums[3]])
                });
                let sampling = if res.ident(page, id, PropKey::Sampling) == Some("pixel") { Sampling::Pixel } else { Sampling::Smooth };
                out.push(Draw::Image { rect, image: src, tint, fit, window, sampling });
            }
        }
        Kind::Label => {
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                if res.ident(page, id, PropKey::TextFit) == Some("stretch") {
                    let flip = match res.ident(page, id, PropKey::TextFlip) {
                        Some("horizontal") => (true, false),
                        Some("vertical") => (false, true),
                        Some("both") => (true, true),
                        _ => (false, false),
                    };
                    out.push(Draw::TextStretch { rect: text_rect, text, key, color: text_color(), font: res.font(page, id).unwrap_or(NO_STR), flip });
                } else {
                    out.push(Draw::Text {
                        rect: text_rect,
                        text,
                        key,
                        font_size: res.font_size(page, id).unwrap_or(DEFAULT_FONT_SIZE),
                        color: text_color(),
                        halign: text_align(page, res, id, PropKey::TextHalign, TextAlign::Start),
                        valign: text_align(page, res, id, PropKey::TextValign, TextAlign::Start),
                        wrap: res.bool(page, id, PropKey::Wrap).unwrap_or(false),
                        font: res.font(page, id).unwrap_or(NO_STR),
                    });
                }
            }
        }
        Kind::Slider => {
            // Track + thumb from `value`/`min`/`max` (the drag is not wired yet).
            let min = res.num(page, id, PropKey::Min).unwrap_or(0.0);
            let max = res.num(page, id, PropKey::Max).unwrap_or(100.0);
            let value = res.slider_value(page, id);
            let t = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
            let track = Rect::new(rect.x, rect.y + (rect.h - SLIDER_TRACK_EPX) / 2.0, rect.w, SLIDER_TRACK_EPX);
            let track_color = color(PropKey::Background).unwrap_or(mul_alpha(SLIDER_TRACK, opacity));
            let fill = color(PropKey::Color).unwrap_or(mul_alpha(SLIDER_FILL, opacity));
            // `slot.track.corner_radius=` (a for= style) lands on the
            // node's corner word; unset keeps the stadium end.
            let track_corner = if corner > 0.0 { corner } else { SLIDER_TRACK_EPX / 2.0 };
            out.push(Draw::Quad { rect: track, color: track_color, corner: track_corner, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
            out.push(Draw::Quad { rect: Rect::new(track.x, track.y, track.w * t, track.h), color: fill, corner: track_corner, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
            let knob = SLIDER_KNOB_EPX;
            out.push(Draw::Circle { rect: Rect::new(rect.x + (rect.w - knob) * t, rect.y + (rect.h - knob) / 2.0, knob, knob), color: mul_alpha(DEFAULT_TEXT_COLOR, opacity), border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
        }
        Kind::Input => {
            // Godot's default LineEdit plate: (0.1, 0.1, 0.1) at 0.6, no border.
            let bg = color(PropKey::Background).unwrap_or(mul_alpha(Color_Straight_Srgb_4xU8_RGBA([0x1A, 0x1A, 0x1A, 0x99]), opacity));
            out.push(Draw::Quad { rect, color: bg, corner, border, border_color });
            // The selection highlight, under the glyphs
            // (`selection_color=`; unset = white at 25% over the dark
            // chrome).
            if let Some((sn, lo, hi)) = res.selection {
                if sn == id {
                    let (stext, skey) = match res.prop(page, id, PropKey::Text) {
                        Some(Value::VText { text, key }) => (text, key),
                        _ => (NO_STR, NO_KEY),
                    };
                    let sel_color = color(PropKey::SelectionColor).unwrap_or(mul_alpha(Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 64]), opacity));
                    out.push(Draw::Selection { rect: text_rect.inset(8.0, 0.0, 8.0, 0.0), text: stext, key: skey, font_size: res.font_size(page, id).unwrap_or(DEFAULT_FONT_SIZE), font: res.font(page, id).unwrap_or(NO_STR), lo, hi, color: sel_color });
                    // Touch-born selections get drag circles, in the
                    // highlight's color made opaque (a faint wash works
                    // under glyphs, not as a grabbable dot).
                    if res.sel_handles {
                        let mut hc = sel_color;
                        hc.0[3] = 0xFF;
                        out.push(Draw::SelHandles { rect: text_rect.inset(8.0, 0.0, 8.0, 0.0), text: stext, key: skey, font_size: res.font_size(page, id).unwrap_or(DEFAULT_FONT_SIZE), font: res.font(page, id).unwrap_or(NO_STR), lo, hi, color: hc });
                    }
                }
            }
            let mut field_is_empty = true;
            if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::Text) {
                field_is_empty = page.str(text).is_empty() && key == NO_KEY;
                out.push(Draw::Text { rect: text_rect.inset(8.0, 0.0, 8.0, 0.0), text, key, font_size: res.font_size(page, id).unwrap_or(DEFAULT_FONT_SIZE), color: text_color(), halign: TextAlign::Start, valign: TextAlign::Center, wrap: false, font: res.font(page, id).unwrap_or(NO_STR) });
            }
            // `placeholder_text=` shows while the field is empty, at half
            // the ink's alpha (Godot's LineEdit look; GPAR112).
            if field_is_empty {
                if let Some(Value::VText { text, key }) = res.prop(page, id, PropKey::PlaceholderText) {
                    let mut dim = text_color();
                    dim.0[3] = (dim.0[3] as f32 * 0.5) as u8;
                    out.push(Draw::Text { rect: text_rect.inset(8.0, 0.0, 8.0, 0.0), text, key, font_size: res.font_size(page, id).unwrap_or(DEFAULT_FONT_SIZE), color: dim, halign: TextAlign::Start, valign: TextAlign::Center, wrap: false, font: res.font(page, id).unwrap_or(NO_STR) });
                }
            }
            // The caret, focused fields only (GPAR112); an empty field
            // carries the interned empty string (id 0).
            if let Some((cn, index)) = res.caret {
                if cn == id {
                    let (ctext, ckey) = match res.prop(page, id, PropKey::Text) {
                        Some(Value::VText { text, key }) => (text, key),
                        _ => (NO_STR, NO_KEY),
                    };
                    out.push(Draw::Caret { rect: text_rect.inset(8.0, 0.0, 8.0, 0.0), text: ctext, key: ckey, font_size: res.font_size(page, id).unwrap_or(DEFAULT_FONT_SIZE), font: res.font(page, id).unwrap_or(NO_STR), index, color: text_color() });
                }
            }
        }
        _ => {}
    }

    // Children move with their parent: they inherit this walk's transform
    // with the node's translate and scale folded in (scale about the
    // `pivot=` point, default the center — the same fixed point `moved`
    // held for the node itself). Carried down the placed tree, never
    // stamped over an index range: a repeat's cloned rows sit past every
    // ancestor's range (todo-bugs.md B37).
    let child_xf: Xform = if motion.translate != [0.0, 0.0] || motion.scale != [1.0, 1.0] { xf.then_motion(base, &motion, pivot) } else { xf };
    for c in res.paint_children(page, id) {
        walk(page, res, rects, c, opacity, skip, out, patch.as_deref_mut(), child_xf);
    }
    if matches!(node.kind, Kind::Scroll | Kind::List) && res.ident(page, id, PropKey::Bars) != Some("never") {
        // Godot's auto scroll bars: a gray grabber in the right column /
        // bottom row, as long as the viewport's share of the content —
        // which is the UNION of the children's rects (a Scroll's one
        // content box; a List's whole stacked body).
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        let mut any = false;
        let mut fold = |c: NodeId| {
            let r = xf.apply(rects[c]);
            if r.w <= 0.0 && r.h <= 0.0 {
                return;
            }
            any = true;
            lo[0] = lo[0].min(r.x);
            lo[1] = lo[1].min(r.y);
            hi[0] = hi[0].max(r.right());
            hi[1] = hi[1].max(r.bottom());
        };
        for c in res.children(page, id) {
            if page.nodes[c].kind == Kind::Repeat {
                res.children(page, c).for_each(&mut fold);
            } else {
                fold(c);
            }
        }
        let (content_w, content_h) = if any { (hi[0] - lo[0], hi[1] - lo[1]) } else { (0.0, 0.0) };
        let grabber = mul_alpha(SCROLL_GRABBER, opacity);
        let offset = res.scroll.get(id.index()).copied().unwrap_or([0.0, 0.0]);
        if content_h > rect.h {
            let len = (rect.h * rect.h / content_h).max(8.0);
            let y = rect.y + (rect.h - len) * (offset[1] / (content_h - rect.h)).clamp(0.0, 1.0);
            out.push(Draw::Quad { rect: Rect::new(rect.right() - SCROLL_BAR_EPX, y + 1.0, SCROLL_BAR_EPX, len - 1.0), color: grabber, corner: 0.0, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
        }
        if content_w > rect.w {
            let len = (rect.w * rect.w / content_w).max(8.0);
            let x = rect.x + (rect.w - len) * (offset[0] / (content_w - rect.w)).clamp(0.0, 1.0);
            out.push(Draw::Quad { rect: Rect::new(x + 1.0, rect.bottom() - SCROLL_BAR_EPX, len - 1.0, SCROLL_BAR_EPX), color: grabber, corner: 0.0, border: 0.0, border_color: Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT });
        }
    }
    if swung {
        out.push(Draw::SwingPop);
    }
    if rotated {
        out.push(Draw::RotatePop);
    }
    if wedge {
        out.push(Draw::WedgePop);
    }
    if clip {
        out.push(Draw::ClipPop);
    }
    if let Some(from) = scaled_from {
        // One factor for the scalar sizes because under a one-axis
        // `scale.x` a glyph or stroke has no single size matching both
        // axes: the axis average keeps strokes uniform while the box
        // distorts (Godot's transform would distort them too; no page
        // scales one axis). A scaled ancestor multiplies this range
        // again on its own way out, so nested scales compose.
        let k = (motion.scale[0] + motion.scale[1]) / 2.0;
        for d in &mut out[from..] {
            match d {
                Draw::Quad { corner, border, .. } => {
                    *corner *= k;
                    *border *= k;
                }
                Draw::Circle { border, .. } => *border *= k,
                Draw::Text { font_size, .. } => *font_size *= k,
                Draw::Caret { font_size, .. } => *font_size *= k,
                Draw::Selection { font_size, .. } => *font_size *= k,
                Draw::SelHandles { font_size, .. } => *font_size *= k,
                _ => {}
            }
        }
    }
    if let Some(p) = patch.as_deref_mut() {
        p.span_new[id.index()].1 = out.len() as u32;
    }
}

/// The axis-aligned transform a walk carries down from every moving
/// ancestor: what their `translate` and `scale` (about a pivot) did to
/// the rects under them, folded into one scale-then-translate, so a
/// descendant's rest rect maps to its drawn rect in one step. Needed
/// because the rows a repeat clones after the build live past every
/// ancestor's index range: shifting `id+1..subtree_end` left the font
/// strip's names standing while the settings sheet slid under them
/// (todo-bugs.md B37). The transform rides the placed tree instead, so
/// whatever the walk reaches under a mover moves with it — and no rect
/// array is copied per moving node.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Xform {
    scale: [f32; 2],
    translate: [f32; 2],
}

impl Xform {
    const IDENTITY: Xform = Xform { scale: [1.0, 1.0], translate: [0.0, 0.0] };

    fn is_identity(self) -> bool {
        self == Xform::IDENTITY
    }

    /// `r` as the ancestors' motion shows it.
    fn apply(self, r: Rect) -> Rect {
        Rect::new(r.x * self.scale[0] + self.translate[0], r.y * self.scale[1] + self.translate[1], r.w * self.scale[0], r.h * self.scale[1])
    }

    /// This transform followed by `m`'s translate and scale about the
    /// pivot point of `base` (the node's rect as this transform shows
    /// it) — what the node's children inherit. Scale about a point c:
    /// x' = c + (x - c) * s + t = x * s + (c * (1 - s) + t), so the fold
    /// stays one scale and one translate per axis.
    fn then_motion(self, base: Rect, m: &crate::anim::Motion, pivot: (f32, f32)) -> Xform {
        let (cx, cy) = (base.x + base.w * pivot.0, base.y + base.h * pivot.1);
        Xform {
            scale: [self.scale[0] * m.scale[0], self.scale[1] * m.scale[1]],
            translate: [self.translate[0] * m.scale[0] + cx * (1.0 - m.scale[0]) + m.translate[0], self.translate[1] * m.scale[1] + cy * (1.0 - m.scale[1]) + m.translate[1]],
        }
    }
}

/// The node's own rect under its motion channels — scale holds the
/// `pivot=` point still (default the center).
fn moved(r: Rect, m: &crate::anim::Motion, pivot: (f32, f32)) -> Rect {
    let w = r.w * m.scale[0];
    let h = r.h * m.scale[1];
    Rect::new(r.x + (r.w - w) * pivot.0 + m.translate[0], r.y + (r.h - h) * pivot.1 + m.translate[1], w, h)
}

/// The 0..1 `pivot=` fraction — where the fixed point of `scale` and
/// `rotate` sits in the node's box. Unset = the center. The compiler
/// fenced the value to a full 0..1 pair (G2005), so anything else here
/// is simply "unset".
fn pivot_frac(page: &Page, res: &Resolved, id: NodeId) -> (f32, f32) {
    match res.prop(page, id, PropKey::Pivot) {
        Some(Value::VSize(Some(x), Some(y))) => (x, y),
        _ => (0.5, 0.5),
    }
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
/// The rects as DRAWN: every node's motion (translate/scale/swing, the
/// same channels the draw walk applies) moved onto its own rect and its
/// subtree's. A swing lands as the door's projected span: each rect's
/// left and right edges through the WP7 camera (`SwingPush`), the height
/// kept — the near edge's, so a finger on the door's shrunken far end
/// still lands on it. `None` = nothing moves — the caller keeps the
/// borrowed slice and pays nothing.
pub(crate) fn moved_rects(page: &Page, res: &Resolved, rects: &[Rect]) -> Option<Vec<Rect>> {
    if res.motion.iter().all(|m| m.translate == [0.0, 0.0] && m.scale == [1.0, 1.0] && m.swing == 0.0) {
        return None;
    }
    let n: usize = page.nodes.len().min(rects.len());
    let mut out = rects.to_vec();
    // Membership is by the PLACED tree (`Resolved::tree`): a later
    // sibling's subtree shares a mover's index range (Node::subtree_end),
    // a node seated under the mover sits outside the range altogether
    // (B25), and a repeat's cloned rows sit past it (B37). Each mover
    // visits its own subtree along the tables — no child lists built
    // per call, no "is j under i" per pair (the 140 ms frame after a
    // press on the effect page, B36 trace, S24).
    for i in 0..n {
        let m = &res.motion[i];
        let mover = NodeId::from_index(i);
        if m.swing != 0.0 {
            let (sin, cos) = m.swing.to_radians().sin_cos();
            let door = out[i];
            let (hinge, cx, focal) = (door.x, door.x + door.w / 2.0, swing_focal(door.h));
            let project = |x: f32| -> f32 {
                let dx = x - hinge;
                let ratio = ((focal + dx * sin) / focal).max(0.05);
                cx + (hinge + dx * cos - cx) / ratio
            };
            for j in res.tree.subtree(mover) {
                if j.index() >= n {
                    continue;
                }
                let r = out[j.index()];
                let (l, rr) = (project(r.x), project(r.right()));
                out[j.index()] = Rect::new(l.min(rr), r.y, (rr - l).abs(), r.h);
            }
        }
        if m.translate == [0.0, 0.0] && m.scale == [1.0, 1.0] {
            continue;
        }
        // Ancestors sit at lower indices (rows are cloned after their
        // repeat), so nested motions compose in walk order like the draw
        // pass.
        let p = pivot_frac(page, res, mover);
        let (cx, cy) = (out[i].x + out[i].w * p.0, out[i].y + out[i].h * p.1);
        for j in res.tree.subtree(mover) {
            if j.index() >= n {
                continue;
            }
            let r = out[j.index()];
            out[j.index()] = Rect::new(cx + (r.x - cx) * m.scale[0] + m.translate[0], cy + (r.y - cy) * m.scale[1] + m.translate[1], r.w * m.scale[0], r.h * m.scale[1]);
        }
    }
    Some(out)
}

/// The shared wedge formula (drawing and hits agree by construction): is
/// the point's angle within [from_deg, from_deg + span_deg]? Degrees,
/// Godot's screen convention (0 = right, clockwise positive).
pub(crate) fn wedge_contains(dx: f32, dy: f32, from_deg: f32, span_deg: f32) -> bool {
    if span_deg <= 0.0 {
        return false;
    }
    if span_deg >= 360.0 || dx * dx + dy * dy < 1e-6 {
        return true;
    }
    (dy.atan2(dx).to_degrees() - from_deg).rem_euclid(360.0) <= span_deg
}

pub fn hit(page: &Page, res: &Resolved, rects: &[Rect], x: f32, y: f32, any: bool) -> Option<NodeId> {
    // The finger lands on the DRAWN picture: while an anim moves a box,
    // the un-moved layout rects would take taps where controls WILL be,
    // not where they are (GPAR50) — so hit reads the moved rects.
    let moved = moved_rects(page, res, rects);
    let rects = moved.as_deref().unwrap_or(rects);
    // A standing `rotate` can't ride the rect transform (a turned box is
    // no axis-aligned Rect), so the FINGER un-rotates instead: each
    // node's containment test reads the screen point turned back about
    // the nearest rotating ancestor-or-self — the renderer applies only
    // the INNERMOST rotate bracket per quad (ui.rs keeps a stack, uses
    // the top), so nearest-wins here IS the picture. Clip and wedge
    // checks keep the raw screen point: scissors and wedge masks cut in
    // screen space, rotation or not. Zero cost while nothing rotates.
    let pts: Option<Vec<(f32, f32)>> = if res.motion.iter().any(|m| m.rotate != 0.0) {
        // The turned point per ROTATING node first; then every node reads
        // its nearest rotating PLACED ancestor-or-self. Two passes, not
        // one index-ordered copy from the parent: a seated node's host
        // can sit at a higher index than the node (B25).
        let mut turned: Vec<Option<(f32, f32)>> = vec![None; page.nodes.len()];
        for i in 0..page.nodes.len().min(rects.len()) {
            let m = &res.motion[i];
            if m.rotate != 0.0 {
                let r = rects[i];
                let pv = pivot_frac(page, res, NodeId::from_index(i));
                let (cx, cy) = (r.x + r.w * pv.0, r.y + r.h * pv.1);
                let (s, c) = (-m.rotate).to_radians().sin_cos();
                let (dx, dy) = (x - cx, y - cy);
                turned[i] = Some((cx + dx * c - dy * s, cy + dx * s + dy * c));
            }
        }
        let mut pts = vec![(x, y); page.nodes.len()];
        for i in 0..page.nodes.len() {
            let mut a = NodeId::from_index(i);
            while a != NO_NODE {
                if let Some(p) = turned[a.index()] {
                    pts[i] = p;
                    break;
                }
                a = res.placed_parent(page, a);
            }
        }
        Some(pts)
    } else {
        None
    };
    hit_laid_at(page, res, rects, x, y, any, pts.as_deref())
}

/// `pts` = the per-node un-rotated screen point (see `hit`); `None` when
/// nothing rotates — the node containment reads it, everything else
/// (clips, wedges, ancestors) stays on the raw screen point.
fn hit_laid_at(page: &Page, res: &Resolved, rects: &[Rect], x: f32, y: f32, any: bool, pts: Option<&[(f32, f32)]>) -> Option<NodeId> {
    if page.nodes.is_empty() {
        return None;
    }
    let mut order: Vec<NodeId> = Vec::with_capacity(page.nodes.len());
    paint_order(page, res, NodeId::new(0), &mut order);
    for id in order.into_iter().rev() {
        let i = id.index();
        let (px, py) = pts.and_then(|p| p.get(i).copied()).unwrap_or((x, y));
        if !rects[i].contains(px, py) {
            continue;
        }
        let node = &page.nodes[i];
        // `interactive=true` claims input (the catcher plate), `false`
        // makes a control hit-invisible; else input-taking kinds — plus
        // a `behavior slider` instance root (GP52): its WHOLE row is
        // the gesture surface, paint or no paint under the finger.
        let interactive = res.bool(page, id, PropKey::Interactive);
        let takes_input = any || interactive == Some(true) || (interactive != Some(false) && (node.kind.is_press_surface() || matches!(node.kind, Kind::Scroll | Kind::List | Kind::Slider | Kind::Input) || page.behaviors.iter().any(|b| b.root == id)));
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
            // GP33: an arc wedge cuts input the way it cuts pixels — the
            // hit area is what's visible (§4.5).
            if let crate::page::Auto::Arc { from, sweep, .. } = page.nodes[a].auto {
                let ra = rects[a];
                if !wedge_contains(x - (ra.x + ra.w / 2.0), y - (ra.y + ra.h / 2.0), from, sweep * res.auto_frac(page, a)) {
                    ok = false;
                    break;
                }
            }
            // PLACED parents: the finger follows the drawn tree, so a
            // seated button takes taps when its declared parent is gone
            // and none when its seat host is (B25).
            let parent = res.placed_parent(page, a);
            if parent == NO_NODE {
                break;
            }
            // A clipping ancestor cuts input at its own rectangle the
            // same way it cuts pixels (GPAR37) — Scroll viewports,
            // `clip_contents=true` and written-size overflow alike.
            if clips(page, res, rects, parent) && !rects[parent].contains(x, y) {
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

// ----- the retained road (B36-DES stage 3) --------------------------------

/// What the last `DrawCache::run` did — the equivalence pin reads it to
/// prove a change stayed local.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawStats {
    /// The whole list was walked again.
    pub full: bool,
    /// Commands a walk emitted; the rest of the list was copied from
    /// last frame's.
    pub emitted: u32,
    /// Nodes whose look or rect changed since the last run (before the
    /// parents that read them join) — the reason for a big `emitted`.
    pub dirty: u32,
}

/// The retained draw list (B36-DES stage 3): the list is built again
/// every run, but a node whose subtree is clean has its run copied from
/// last frame's list (`Patch`) and only the changed subtrees are walked.
///
/// A node is dirty when its look changed (`LookCopy::diff`, every key),
/// its shown rect, motion, state bits, anim ownership, scroll offset,
/// hiding/presence/driver entry, the caret or selection on it, or a
/// string rewritten in place. A dirty node's ancestors are walked again
/// too (their run holds it) but their own commands are copied nowhere —
/// they are cheap, the subtrees beside the dirty one are the copies. A
/// changed node also dirties its parent when the parent's `clips` answer
/// moved (it reads the children's rects and words) or when a Scroll or
/// List's bar reads a child's changed rect. The paint order itself is
/// not patched: a seat move, a row change or a raise, like a node-count
/// change, a theme switch or a list the host altered, walks everything.
pub struct DrawCache {
    on: bool,
    valid: bool,
    seen_serial: u64,
    /// The visit stamp (`Patch::frame`), advanced per walk.
    frame: u32,
    /// The length of the list handed out last — a host that changed the
    /// list since gets a full walk, never a copy from a list this cache
    /// did not build.
    expected_len: usize,
    span: Vec<(u32, u32)>,
    ctx: Vec<(f32, bool, u32)>,
    clip: Vec<bool>,
    // What the next run compares against.
    rects: Vec<Rect>,
    motion: Vec<Motion>,
    states: Vec<u8>,
    anim_owned: Vec<u16>,
    anim_owned_props: Vec<(NodeId, PropKey)>,
    raised: Vec<(NodeId, i32)>,
    seat: Vec<NodeId>,
    repeat_rows: Vec<(NodeId, Vec<NodeId>)>,
    hiding: Vec<NodeId>,
    presence: Vec<(NodeId, bool)>,
    auto: Vec<(NodeId, f32)>,
    scroll: Vec<[f32; 2]>,
    caret: Option<(NodeId, u32)>,
    selection: Option<(NodeId, u32, u32)>,
    sel_handles: bool,
    variant: StrId,
    look: LookCopy,
    // Scratch, kept so a steady frame allocates nothing.
    next: Vec<Draw>,
    changed: Vec<NodeId>,
    /// Nodes whose font words changed: their whole subtree reads them
    /// (`Resolved::inherited`), so it is walked, never copied.
    inherited: Vec<NodeId>,
    dirty: Vec<bool>,
    /// Per node: its `clips` answer was already asked this run.
    clip_asked: Vec<bool>,
    rect_changed: Vec<bool>,
    dirty_sub: Vec<bool>,
    span_new: Vec<(u32, u32)>,
    ctx_new: Vec<(f32, bool, u32)>,
    clip_new: Vec<bool>,
    stack: Vec<NodeId>,
    stats: DrawStats,
}

impl Default for DrawCache {
    fn default() -> DrawCache {
        DrawCache {
            on: true,
            valid: false,
            seen_serial: 0,
            frame: 0,
            expected_len: 0,
            span: Vec::new(),
            ctx: Vec::new(),
            clip: Vec::new(),
            rects: Vec::new(),
            motion: Vec::new(),
            states: Vec::new(),
            anim_owned: Vec::new(),
            anim_owned_props: Vec::new(),
            raised: Vec::new(),
            seat: Vec::new(),
            repeat_rows: Vec::new(),
            hiding: Vec::new(),
            presence: Vec::new(),
            auto: Vec::new(),
            scroll: Vec::new(),
            caret: None,
            selection: None,
            sel_handles: false,
            variant: NO_STR,
            look: LookCopy::default(),
            next: Vec::new(),
            changed: Vec::new(),
            inherited: Vec::new(),
            dirty: Vec::new(),
            clip_asked: Vec::new(),
            rect_changed: Vec::new(),
            dirty_sub: Vec::new(),
            span_new: Vec::new(),
            ctx_new: Vec::new(),
            clip_new: Vec::new(),
            stack: Vec::new(),
            stats: DrawStats::default(),
        }
    }
}

impl DrawCache {
    /// The switch: off = every run walks the whole list (for the
    /// equivalence pin and for bisecting a list that differs between the
    /// roads).
    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        self.valid = false;
    }

    pub fn stats(&self) -> DrawStats {
        self.stats
    }

    /// Brings `out` — the list this cache handed out last time (or any
    /// list; one it did not build is walked whole) — up to date with
    /// `res` and the shown `rects`: the same list `draw` gives.
    /// `changes` is the runtime's report for `res`.
    pub fn run(&mut self, page: &Page, res: &Resolved, changes: &ResolveChanges, rects: &[Rect], out: &mut Vec<Draw>) {
        let n = page.nodes.len();
        let seen_resolve: bool = changes.serial == self.seen_serial || changes.serial == self.seen_serial + 1;
        let full: bool = !self.on
            || !self.valid
            || changes.full
            || !seen_resolve
            || self.span.len() != n
            || self.rects.len() != n
            || rects.len() != n
            || out.len() != self.expected_len
            || !self.look.fits(n)
            || res.motion.len() != n
            || res.states.len() != n
            || res.anim_owned.len() != n
            || res.seat.len() != n
            || res.scroll.len() != n
            || res.seat != self.seat
            || res.repeat_rows != self.repeat_rows
            || res.raised != self.raised
            || res.variant != self.variant;
        if full {
            self.emit(page, res, rects, out, true);
            self.remember(res, changes, rects);
            self.valid = true;
        } else {
            self.patch(page, res, changes, rects, out);
        }
    }

    fn patch(&mut self, page: &Page, res: &Resolved, changes: &ResolveChanges, rects: &[Rect], out: &mut Vec<Draw>) {
        let n = page.nodes.len();
        // 1. The nodes whose picture may have changed.
        let mut changed: Vec<NodeId> = std::mem::take(&mut self.changed);
        changed.clear();
        let mut inherited: Vec<NodeId> = std::mem::take(&mut self.inherited);
        inherited.clear();
        self.look.diff(res, false, &mut changed, Some(&mut inherited));
        self.rect_changed.clear();
        self.rect_changed.resize(n, false);
        for i in 0..n {
            let id = NodeId::from_index(i);
            if rects[i] != self.rects[i] {
                self.rect_changed[i] = true;
                changed.push(id);
            } else if res.motion[i] != self.motion[i] || res.states[i] != self.states[i] || res.anim_owned[i] != self.anim_owned[i] || res.scroll[i] != self.scroll[i] {
                changed.push(id);
            }
        }
        if res.anim_owned_props != self.anim_owned_props {
            changed.extend(self.anim_owned_props.iter().chain(&res.anim_owned_props).map(|(node, _)| *node));
        }
        crate::layout::sym_diff_nodes(&self.hiding, &res.hiding, &mut changed);
        crate::layout::diff_pairs(&self.presence, &res.presence, &mut changed);
        crate::layout::diff_pairs(&self.auto, &res.auto, &mut changed);
        if self.caret != res.caret {
            changed.extend(self.caret.iter().chain(&res.caret).map(|(node, _)| *node));
        }
        if self.selection != res.selection || self.sel_handles != res.sel_handles {
            changed.extend(self.selection.iter().chain(&res.selection).map(|(node, _, _)| *node));
        }
        if changes.serial == self.seen_serial + 1 {
            changed.extend_from_slice(&changes.rewritten);
        }
        changed.retain(|c| c.index() < n);
        changed.sort();
        changed.dedup();
        let dirty_count: u32 = changed.len() as u32;
        if changed.is_empty() {
            self.stats = DrawStats { full: false, emitted: 0, dirty: 0 };
            self.seen_serial = changes.serial;
            self.changed = changed;
            self.inherited = inherited;
            return;
        }
        if changed.len() * 2 > n {
            // Most of the page moved (a pan of a strip that IS the page:
            // every rect under the Scroll is new) — nothing worth copying,
            // and the parent and subtree passes below would only cost.
            // The plain walk, booked like a full one. On the S24's main
            // page the patched road ran slower than `draw` before this.
            self.changed = changed;
            self.inherited = inherited;
            self.emit(page, res, rects, out, true);
            self.stats.dirty = dirty_count;
            self.remember(res, changes, rects);
            return;
        }
        // 2. Dirty marks — a node with changed font words dirties its
        // whole placed subtree — then the parents that read a changed
        // child.
        self.dirty.clear();
        self.dirty.resize(n, false);
        for c in &changed {
            self.dirty[c.index()] = true;
        }
        for k in 0..inherited.len() {
            let root: NodeId = inherited[k];
            if root.index() >= n {
                continue;
            }
            self.stack.clear();
            self.stack.push(root);
            while let Some(d) = self.stack.pop() {
                self.dirty[d.index()] = true;
                self.stack.extend(res.tree.children(d));
            }
        }
        self.inherited = inherited;
        // A parent's `clips` is asked ONCE per run: it walks every child
        // with prop lookups, and asking it per changed child made a row
        // of tiles cost tiles × children each frame.
        self.clip_asked.clear();
        self.clip_asked.resize(n, false);
        for k in 0..changed.len() {
            let c: NodeId = changed[k];
            let p: NodeId = res.placed_parent(page, c);
            if p == NO_NODE || p.index() >= n {
                continue;
            }
            // A Scroll's or List's bar is as long as its children's
            // rects say (a List's rows sit under a `repeat`).
            let bar_owner: NodeId = if page.nodes[p].kind == Kind::Repeat { res.placed_parent(page, p) } else { p };
            if bar_owner != NO_NODE && bar_owner.index() < n && matches!(page.nodes[bar_owner].kind, Kind::Scroll | Kind::List) && self.rect_changed[c.index()] {
                self.dirty[bar_owner.index()] = true;
            }
            if !self.dirty[p.index()] && !self.clip_asked[p.index()] {
                self.clip_asked[p.index()] = true;
                if clips(page, res, rects, p) != self.clip[p.index()] {
                    self.dirty[p.index()] = true;
                }
            }
        }
        // 3. A dirty node's ancestors are walked again (their runs hold
        // it); everything beside the dirty subtrees is copied.
        self.dirty_sub.clear();
        self.dirty_sub.resize(n, false);
        for i in 0..n {
            if !self.dirty[i] {
                continue;
            }
            let mut a = NodeId::from_index(i);
            while a != NO_NODE && a.index() < n && !self.dirty_sub[a.index()] {
                self.dirty_sub[a.index()] = true;
                a = res.placed_parent(page, a);
            }
        }
        self.changed = changed;
        self.emit(page, res, rects, out, false);
        self.stats.dirty = dirty_count;
        self.remember(res, changes, rects);
    }

    /// One walk over the page into a fresh list, copying every clean
    /// subtree's run from `out` (last frame's list); `everything` = walk
    /// every node (the full road). `out` then holds the new list.
    fn emit(&mut self, page: &Page, res: &Resolved, rects: &[Rect], out: &mut Vec<Draw>, everything: bool) {
        let n = page.nodes.len();
        self.frame = self.frame.wrapping_add(1);
        if everything {
            self.dirty_sub.clear();
            self.dirty_sub.resize(n, true);
        }
        self.span.resize(n, (0, 0));
        self.ctx.resize(n, (1.0, false, 0));
        self.clip.resize(n, false);
        let mut span_new: Vec<(u32, u32)> = std::mem::take(&mut self.span_new);
        span_new.clone_from(&self.span);
        let mut ctx_new: Vec<(f32, bool, u32)> = std::mem::take(&mut self.ctx_new);
        ctx_new.clone_from(&self.ctx);
        let mut clip_new: Vec<bool> = std::mem::take(&mut self.clip_new);
        clip_new.clone_from(&self.clip);
        let mut next: Vec<Draw> = std::mem::take(&mut self.next);
        next.clear();
        let mut patch = Patch {
            prev: &out[..],
            span_old: &self.span,
            ctx_old: &self.ctx,
            dirty_sub: &self.dirty_sub,
            tree: &res.tree,
            frame: self.frame,
            span_new,
            ctx_new,
            clip: clip_new,
            stack: std::mem::take(&mut self.stack),
            copied: 0,
        };
        if n > 0 {
            walk(page, res, rects, NodeId::new(0), 1.0, None, &mut next, Some(&mut patch), Xform::IDENTITY);
        }
        let Patch { span_new, ctx_new, clip, stack, copied, .. } = patch;
        self.stats = DrawStats { full: everything, emitted: (next.len() - copied) as u32, dirty: n as u32 };
        self.span_new = std::mem::replace(&mut self.span, span_new);
        self.ctx_new = std::mem::replace(&mut self.ctx, ctx_new);
        self.clip_new = std::mem::replace(&mut self.clip, clip);
        self.stack = stack;
        std::mem::swap(out, &mut next);
        self.next = next;
        self.expected_len = out.len();
    }

    /// What the next run compares against.
    fn remember(&mut self, res: &Resolved, changes: &ResolveChanges, rects: &[Rect]) {
        self.seen_serial = changes.serial;
        self.rects.clear();
        self.rects.extend_from_slice(rects);
        self.motion.clone_from(&res.motion);
        self.states.clone_from(&res.states);
        self.anim_owned.clone_from(&res.anim_owned);
        self.anim_owned_props.clone_from(&res.anim_owned_props);
        self.raised.clone_from(&res.raised);
        self.seat.clone_from(&res.seat);
        self.repeat_rows.clone_from(&res.repeat_rows);
        self.hiding.clone_from(&res.hiding);
        self.presence.clone_from(&res.presence);
        self.auto.clone_from(&res.auto);
        self.scroll.clone_from(&res.scroll);
        self.caret = res.caret;
        self.selection = res.selection;
        self.sel_handles = res.sel_handles;
        self.variant = res.variant;
        self.look.take(res);
    }
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
        assert!(matches!(list[2], Draw::Quad { color, .. } if color == Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 128])), "{:?}", list[2]);
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

    /// A Gradient's `corner_radius` rides its draw (Q9): the renderer
    /// masks the ramp to the rounded rect — half the box makes the disc the
    /// brush-options preview ball needs. Without the key the corner is 0,
    /// the square.
    #[test]
    fn a_gradient_carries_its_corner_radius() {
        let src = "View v { Gradient g width=100, height=100, fill=radial, corner_radius=50, stops=[#FFFFFFFF, #FFFFFF00]; Gradient h width=100, height=100, stops=[#FF0000FF, #0000FFFF] }";
        let c = crate::compile(src, &|_| None);
        assert!(!crate::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = crate::runtime::Runtime::new(&page);
        let res = crate::runtime::resolve(&page, &rt);
        let rects = crate::layout::layout(&page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &crate::layout::FixedAdvance);
        let list = draw(&page, &res, &rects);
        let corners: Vec<f32> = list.iter().filter_map(|d| if let Draw::Gradient { corner, .. } = d { Some(*corner) } else { None }).collect();
        assert_eq!(corners, vec![50.0, 0.0], "{list:?}");
    }

    /// A Circle in a non-square box draws a CENTERED DISC — the bounding
    /// square is min(width, height), centered both axes (shipped
    /// semantics, gogo_circle.gd `_draw`: radius = min(size)/2 around
    /// size/2) — never a pill stretched to the box.
    #[test]
    fn circle_in_a_non_square_box_is_a_centered_disc() {
        let src = "View v { Circle c width=100, height=40, color=#FF0000FF, halign=left, valign=top }";
        let c = compile(src, &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let rt = Runtime::new(&c.page);
        let res = resolve(&c.page, &rt);
        let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
        let own = rects[c.page.find("c").unwrap().index()];
        assert_eq!((own.w, own.h), (100.0, 40.0), "{own:?}");
        let list = draw(&c.page, &res, &rects);
        let disc = list.iter().find_map(|d| if let Draw::Circle { rect, .. } = d { Some(*rect) } else { None }).unwrap();
        assert_eq!((disc.w, disc.h), (own.h, own.h), "{list:#?}");
        assert_eq!(disc.x, own.x + (own.w - own.h) / 2.0);
        assert_eq!(disc.y, own.y);
    }
}

/// The four corner radii after the file-order walk (static props, then
/// `when` overrides): the uniform key writes all four, a per-corner key
/// writes its one, later wins. Values follow `$refs` through the theme.
fn corner_set(page: &Page, res: &Resolved, id: NodeId) -> [f32; 4] {
    let mut c = [0.0f32; 4];
    let node = &page.nodes[id];
    let apply = |c: &mut [f32; 4], key: PropKey, value: Value| {
        let v = match crate::runtime::deref(page, res.variant, value) {
            Value::VNum(v) => v,
            _ => return,
        };
        match key {
            PropKey::CornerRadius => *c = [v; 4],
            PropKey::CornerRadiusTopLeft => c[0] = v,
            PropKey::CornerRadiusTopRight => c[1] = v,
            PropKey::CornerRadiusBottomRight => c[2] = v,
            PropKey::CornerRadiusBottomLeft => c[3] = v,
            _ => {}
        }
    };
    for p in &page.props[node.props.0 as usize..(node.props.0 + node.props.1) as usize] {
        apply(&mut c, p.key, p.value);
    }
    for (nid, p) in &res.overrides {
        if *nid == id {
            apply(&mut c, p.key, p.value);
        }
    }
    c
}
