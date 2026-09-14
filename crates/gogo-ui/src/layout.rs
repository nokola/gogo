//! The layout pass: `layout(page, resolved, viewport, text) -> Vec<Rect>`.
//! Pure — nothing is written back into the page. Measure and arrange, two
//! questions with two names:
//!
//! - `size_at(node, width)` — MEASURE: the box the node wants given the
//!   width its parent can offer (wrapped text and Flow rows need it).
//!   Hugging parents, cross axes and "the height at the width I got" ask
//!   this. Memoized per pass.
//! - `main_min(node, axis)` — how much a child gets when its parent was
//!   told its size and is splitting it: a `fill` child's `min_*` only (it
//!   takes its share of what's left), any other child its own size.
//!
//! A parent is either TOLD its size (`width=300`, `width=fill`, the
//! screen) or ASKS its children (`width=hug`, or an aligned axis with no
//! width word).
//! - Parent that asks its children ("how wide do you need to be?"): a
//!   `fill` child answers with its content, like any other child. There is
//!   nothing to share yet, so "fill" has nothing to say here.
//! - Parent that was told its size ("I'm 300 wide, split it"): fixed
//!   children take their size, `fill` children share what's left. A fill
//!   child can end up narrower than its text here; the smallest it goes is
//!   its `min_width`.
//!
//! - `star_split` — ARRANGE along an axis: fixed slots take their size,
//!   fills share what is left by weight, a fill under its floor takes the
//!   floor and the rest re-share (Godot BoxContainer / WPF star).
//!
//! Then `place` lays each child into its slot top-down. Semantics follow
//! the spec's Placement section: the sizing triad, alignment as position
//! only, margins as inset / offset / pin, `spacing`/`gap` in stacks,
//! weighted fills, Layers minimum = biggest child, Scroll content takes
//! the viewport.
//!
//! Two roads: `layout` lays the whole page out from scratch (the
//! reference, and what a one-off caller wants); `LayoutCache` keeps the
//! last answer and, after a change that reaches a few nodes, re-lays only
//! their subtrees up to the first box whose size cannot depend on its
//! children (B36-DES stage 3). The two must agree bit for bit — the
//! equivalence pin runs both after every step.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use crate::locale::Locale;
use crate::page::*;
use crate::frame::Frame;
use crate::runtime::{ResolveChanges, Resolved};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    pub fn inset(&self, left: f32, top: f32, right: f32, bottom: f32) -> Rect {
        Rect { x: self.x + left, y: self.y + top, w: (self.w - left - right).max(0.0), h: (self.h - top - bottom).max(0.0) }
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        Rect { x, y, w: (r - x).max(0.0), h: (b - y).max(0.0) }
    }
}

/// Text measurement the caller supplies (the crate never shapes text).
pub trait TextMeasure {
    /// Width and height of `text` at `font_size` epx; wrapped at
    /// `max_width` when given. `font` = the file key from `font=`.
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32);
    /// A bitmap's own size in pixels, for `aspect=natural` (`None` =
    /// unknown). `key` is the build-wide image id (`Draw::Image` carries
    /// the same), so a host answers from a table, not a string match.
    fn image_size(&self, _key: ImageKey) -> Option<(f32, f32)> {
        None
    }
    /// A number that changes whenever this measure would answer the same
    /// question differently (another epx, another default font, an image
    /// whose size the host learned since). Needed because the retained
    /// layout (`LayoutCache`) keeps measured sizes between frames and
    /// only re-measures what the page changed — it cannot see the host's
    /// side, so the host says so here and the cache takes the full road.
    /// A measure that never changes answers a constant.
    fn identity(&self) -> u64 {
        0
    }
}

/// A fixed-advance fake for tests and headless layout: 0.55 em per char,
/// 1.25 em line height, word wrap.
pub struct FixedAdvance;

impl TextMeasure for FixedAdvance {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, _font: Option<&str>) -> (f32, f32) {
        let advance = font_size * 0.55;
        let line_h = font_size * 1.25;
        let Some(max_w) = max_width else {
            let chars = text.chars().count() as f32;
            return (chars * advance, line_h);
        };
        let mut lines = 1.0;
        let mut line_w: f32 = 0.0;
        let mut widest: f32 = 0.0;
        for word in text.split(' ') {
            let w = word.chars().count() as f32 * advance;
            let with_space = if line_w > 0.0 { line_w + advance + w } else { w };
            if line_w > 0.0 && with_space > max_w {
                lines += 1.0;
                widest = widest.max(line_w);
                line_w = w;
            } else {
                line_w = with_space;
            }
        }
        widest = widest.max(line_w);
        (widest.min(max_w), lines * line_h)
    }
}

pub const DEFAULT_FONT_SIZE: f32 = 15.0;
/// A Button with no `padding` declared gets this air around its content.
pub const BUTTON_PADDING: [f32; 4] = [12.0, 6.0, 12.0, 6.0];
pub const SLIDER_HEIGHT: f32 = 32.0;
/// The Slider's knob; the pointer math parks its edges at the track's ends.
pub const SLIDER_KNOB_EPX: f32 = 16.0;
pub const INPUT_HEIGHT: f32 = 36.0;
/// The native Check/Radio mark (no body of its own): a 20-epx square / ring.
pub const CHECK_BOX_EPX: f32 = 20.0;
/// The native Toggle switch: a 44×22 pill.
pub const TOGGLE_PILL: (f32, f32) = (44.0, 22.0);
/// A bare Toggle's minimum height. Needed because a 22-epx pill is too
/// small a finger target — the box is at least this tall and the pill
/// draws centered in it (GPAR92, user 2026-08-31; the addon's toggle
/// template keeps the same floor).
pub const TOGGLE_TOUCH_EPX: f32 = 44.0;
/// Air between a native mark and its text.
pub const MARK_TEXT_GAP: f32 = 8.0;
/// A scroll bar's column (right) or row (bottom); Godot's default theme at 2× is 8 px.
pub const SCROLL_BAR_EPX: f32 = 4.0;

/// Does this kind place its children itself (a container), or do the
/// children place themselves in it (a free parent)? The addon's defaults
/// key off this split — its alias table (`gogo_resolver.gd` TYPE_ALIASES)
/// makes these kinds a Godot `Container`, and a Container honours size
/// flags where a plain `Control` ignores them. The Image centring default
/// (`image_center_default`) is one such flag, so it holds under a Panel
/// and not under a Layers or a Button (docs/todo-bugs.md B28-DES: the
/// Rust roads had lumped Panel with Layers). `Choice` is a stack here (a
/// scope in the addon); `List` bodies its statics in a VBoxContainer
/// (`controls/list_root.gd`). Not here: `GridRow` and `Repeat` are not
/// boxes — a cell's parent is the Grid, a repeat's items take the host's
/// slots — see `placing_parent_kind`.
pub fn is_container(kind: Kind) -> bool {
    matches!(kind, Kind::Row | Kind::Column | Kind::Choice | Kind::Panel | Kind::ListItem | Kind::Grid | Kind::Scroll | Kind::Flow | Kind::Tiles | Kind::List)
}

/// An Image sized on both axes with no alignment written is an exact
/// CENTRED box in a container parent (WPF Image parity, the addon's D28:
/// `img_center` sets Godot's shrink-centre size flags, which only a
/// Container honours — spec "Image", REFERENCE.md "Image"). Under a free
/// parent (Layers, View, Button) the addon writes no anchor and the Image
/// sits at the start edge, so the default is gated on `is_container`.
/// One helper for every road that places children — stack, general
/// (`place`, whose parent may be a Panel), Flow — so the scope cannot
/// drift per road again (todo-bugs.md B28: the clone page's thumbnail sat
/// at the top of its row because only the stack road centred). Tiles is
/// the one container road that skips it (`place_tiles`).
fn image_center_default(parent_kind: Kind, kind: Kind, p: &mut Placement) {
    if kind != Kind::Image || !is_container(parent_kind) {
        return;
    }
    if !(matches!(p.width, Len::Exact(_)) && matches!(p.height, Len::Exact(_))) {
        return;
    }
    if p.halign == Align::AlignUnset {
        p.halign = Align::Center;
    }
    if p.valign == Align::AlignUnset {
        p.valign = Align::Center;
    }
}

/// Which axis an `aspect=` derives the other from, once the sizing words
/// are read (`Lay::aspect_drive`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AspectDrive {
    /// The width is spelled (or, bare, the parent's offer): the height follows.
    Width,
    /// The height is spelled and the width is not: the width follows.
    Height,
    /// Both axes `fill` or both bare on a numeric ratio: the largest
    /// ratio box that fits, centred.
    Fit,
}

/// One `size_at` answer a node keeps: the question — the width offered
/// (`f32` bits; `u32::MAX` = unbounded) and whether it was the node's
/// FINAL width (`size_final`) — and the answer.
#[derive(Clone, Copy, Debug)]
struct Memo {
    avail: u32,
    final_w: bool,
    w: f32,
    h: f32,
}

struct Lay<'a> {
    page: &'a Page,
    res: &'a Resolved,
    text: &'a dyn TextMeasure,
    /// Per node: the rect as PLACED — before the reveal pass shrinks a
    /// revealed box (`reveal`). What `place_children` of a box starts
    /// from, so the retained road keeps this table and derives the shown
    /// one.
    rects: Vec<Rect>,
    /// Per node and axis: does the room this box hands its children come
    /// from a number the author wrote? True for an exact size or a cap on
    /// the box itself, passed on unchanged by a box that merely fills its
    /// parent, false for a hug box (sized by content) and for the viewport.
    written: Vec<[bool; 2]>,
    /// Per node: its `size_at` answers — a parent asks for each child's
    /// measure more than once per pass (a stack asks per width). Kept per
    /// node, not in one map, so the retained road can forget one node's
    /// answers when its content changed and keep everyone else's.
    memo: RefCell<Vec<Vec<Memo>>>,
    /// How many measures ran the long way (`size_at_uncached`) — the
    /// locality counter the equivalence pin reads.
    measured: Cell<u32>,
}

/// The tables a plain `layout` works in, kept by a caller that lays out
/// on its own again and again (a child page's natural-size measure) so
/// the second call allocates nothing. Nothing to read but through
/// `layout_into`.
#[derive(Default)]
pub struct LayoutBuffers {
    rects: Vec<Rect>,
    written: Vec<[bool; 2]>,
    memo: Vec<Vec<Memo>>,
}

/// The page laid out in `viewport` from scratch: one rect per node.
pub fn layout(page: &Page, res: &Resolved, viewport: Rect, text: &dyn TextMeasure) -> Vec<Rect> {
    let mut keep = LayoutBuffers::default();
    layout_into(page, res, viewport, text, &mut keep);
    keep.rects
}

/// `layout` in the caller's kept tables: the rects come back borrowed
/// from `keep`, and a call after the first allocates nothing once the
/// page stopped growing.
pub fn layout_into<'b>(page: &Page, res: &Resolved, viewport: Rect, text: &dyn TextMeasure, keep: &'b mut LayoutBuffers) -> &'b [Rect] {
    let n = page.nodes.len();
    let LayoutBuffers { mut rects, mut written, mut memo } = std::mem::take(keep);
    rects.clear();
    rects.resize(n, Rect::ZERO);
    written.clear();
    written.resize(n, [false; 2]);
    memo.iter_mut().for_each(Vec::clear);
    memo.resize_with(n, Vec::new);
    let mut lay = Lay::new(page, res, text, rects, written, memo);
    lay.place_root(viewport);
    // The reveal pass never reads a rect but its own node's, so it can
    // run in place here; the retained road keeps the placed table apart.
    for id in page.node_ids() {
        let placed = lay.rects[id];
        lay.rects[id] = lay.reveal(id, placed);
    }
    let (rects, written, memo) = lay.into_tables();
    *keep = LayoutBuffers { rects, written, memo };
    &keep.rects
}

/// The keys the layout pass reads off the look (`Resolved::prop` and its
/// wrappers) — besides the placement words, which live in
/// `Resolved::placement`. The retained road treats a change on any other
/// key as draw-only: a `when held background=` press lays out nothing.
/// Kept next to the pass so a new `res.num(..)` here grows this list.
pub(crate) fn layout_reads(key: PropKey) -> bool {
    key.is_placement()
        || matches!(
            key,
            PropKey::Text
                | PropKey::Wrap
                | PropKey::ClipText
                | PropKey::FontSize
                | PropKey::Font
                | PropKey::Spacing
                | PropKey::BorderThickness
                | PropKey::RevealX
                | PropKey::RevealY
                | PropKey::Overlay
                | PropKey::Cols
                | PropKey::Span
                | PropKey::FullRow
                | PropKey::Bars
                | PropKey::Allow
                | PropKey::Orientation
                | PropKey::Aspect
                | PropKey::Source
                | PropKey::Visibility
                | PropKey::ColSpan
                | PropKey::RowSpan
                | PropKey::RowHeight
                | PropKey::SeatOf
        )
}

/// Which axis a stack runs on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl<'a> Lay<'a> {
    fn new(page: &'a Page, res: &'a Resolved, text: &'a dyn TextMeasure, rects: Vec<Rect>, written: Vec<[bool; 2]>, memo: Vec<Vec<Memo>>) -> Lay<'a> {
        Lay { page, res, text, rects, written, memo: RefCell::new(memo), measured: Cell::new(0) }
    }

    /// The tables back, for a caller that keeps them (`layout_into`).
    fn into_tables(self) -> (Vec<Rect>, Vec<[bool; 2]>, Vec<Vec<Memo>>) {
        (self.rects, self.written, self.memo.into_inner())
    }

    /// The whole page into `viewport`, root down.
    fn place_root(&mut self, viewport: Rect) {
        if !self.page.nodes.is_empty() {
            // The viewport is not a number the author wrote.
            self.place(NodeId::new(0), viewport, [false, false]);
        }
    }

    /// GP40 reveal: the rect a node SHOWS given the one it was placed at.
    /// On a STRETCHED axis the shown extent is factor × the node's OWN
    /// extent, never the parent's grant (the shipped D93 rule) — hug axes
    /// already carry the scaled measure, a stretch clamps back. A `fill`
    /// axis has no own extent (a bare Rectangle measures 0): it shows the
    /// fraction of what it was GRANTED — the slider fill bar, `reveal_x`
    /// = the value's share of the track (user call 2026-09-05; before
    /// this the bar was 0 wide at every value and no pin looked). Reads
    /// no rect but `placed`, so the pass can run in any order.
    fn reveal(&self, id: NodeId, placed: Rect) -> Rect {
        let mut r = placed;
        let fx = self.res.num(self.page, id, PropKey::RevealX).map(|f| f.clamp(0.0, 1.0));
        let fy = self.res.num(self.page, id, PropKey::RevealY).map(|f| f.clamp(0.0, 1.0));
        if let Some(f) = fx.filter(|f| *f < 1.0) {
            if matches!(self.placement(id).width, Len::Fill(_)) {
                r.w *= f;
            } else {
                let (sw, _) = self.size_at(id, Some(placed.w));
                r.w = r.w.min(sw);
            }
        }
        if let Some(f) = fy.filter(|f| *f < 1.0) {
            if matches!(self.placement(id).height, Len::Fill(_)) {
                r.h *= f;
            } else {
                let (_, sh) = self.size_at(id, Some(placed.w));
                r.h = r.h.min(sh);
            }
        }
        r
    }

    fn node(&self, id: NodeId) -> &Node {
        &self.page.nodes[id]
    }

    fn placement(&self, id: NodeId) -> &Placement {
        &self.res.placement[id]
    }

    fn gone(&self, id: NodeId) -> bool {
        self.res.gone(self.page, id)
    }

    /// The children that take part in layout: not `gone`; a `repeat`
    /// stands for its shown items (they take the parent's slots as if
    /// written there — a Column's `spacing` spaces them), so an empty
    /// repeat is nothing on screen: no slot, no spacing.
    fn visible_children(&self, id: NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        for c in self.res.children(self.page, id) {
            if self.gone(c) {
                continue;
            }
            if self.node(c).kind == Kind::Repeat {
                out.extend(self.res.children(self.page, c).into_iter().filter(|g| !self.gone(*g)));
            } else {
                out.push(c);
            }
        }
        out
    }

    /// The children that count toward `id`'s minimum: `overlay=true` boxes
    /// sit on the parent without sizing it (touch-slop plates, badges).
    fn sizing_children(&self, id: NodeId) -> Vec<NodeId> {
        self.visible_children(id).into_iter().filter(|c| !self.res.bool(self.page, *c, PropKey::Overlay).unwrap_or(false)).collect()
    }

    /// The node's margins with `safe` edges read from the host insets.
    fn margins(&self, id: NodeId) -> [Option<f32>; 4] {
        let p = self.placement(id);
        let mut m = p.margin;
        for i in 0..4 {
            if p.margin_safe[i] {
                m[i] = Some(m[i].unwrap_or(0.0) + self.res.safe[i]);
            }
        }
        m
    }

    /// Padding plus border, per edge (left, top, right, bottom).
    fn inner_inset(&self, id: NodeId) -> [f32; 4] {
        let mut pad = self.placement(id).padding;
        for i in 0..4 {
            if self.placement(id).padding_safe[i] {
                pad[i] += self.res.safe[i];
            }
        }
        // The border is part of the box's edge for EVERY kind — draw
        // already insets text by it, so layout must too or the two passes
        // disagree by the border on ListItem/Slot (GPAR41; any box draws
        // a plate since E8).
        let border = self.res.num(self.page, id, PropKey::BorderThickness).unwrap_or(0.0);
        [pad[0] + border, pad[1] + border, pad[2] + border, pad[3] + border]
    }

    fn font_size(&self, id: NodeId) -> f32 {
        self.res.font_size(self.page, id).unwrap_or(DEFAULT_FONT_SIZE)
    }

    /// The string the node shows — in the runtime's language, so a keyed
    /// label is measured at the width of the translation it will draw.
    fn text_of(&self, id: NodeId) -> Option<&'a str> {
        match self.res.prop(self.page, id, PropKey::Text)? {
            Value::VText { text, key } => Some(crate::locale::display(self.page, self.res.locale.as_deref(), text, key)),
            _ => None,
        }
    }

    fn font_key(&self, id: NodeId) -> Option<&'a str> {
        self.res.font(self.page, id).map(|f| self.page.str(f))
    }

    fn spacing(&self, id: NodeId) -> f32 {
        self.res.num(self.page, id, PropKey::Spacing).unwrap_or(0.0)
    }

    // ----- measure -----------------------------------------------------------

    /// MEASURE: the box the node wants (its size under `hug`), given the
    /// width its parent can offer for wrapping (`None` = unbounded).
    fn size_at(&self, id: NodeId, avail_w: Option<f32>) -> (f32, f32) {
        self.size_at_flag(id, avail_w, false)
    }

    /// MEASURE at a width the node already GOT (its final rect width):
    /// the height question. Distinct from `size_at`, whose `avail_w` is
    /// the parent's ROOM — a capped hug re-binds against room, never
    /// against its own resolved width (GPAR76).
    fn size_final(&self, id: NodeId, w: f32) -> (f32, f32) {
        self.size_at_flag(id, Some(w), true)
    }

    fn size_at_flag(&self, id: NodeId, avail_w: Option<f32>, final_w: bool) -> (f32, f32) {
        let avail: u32 = avail_w.map_or(u32::MAX, f32::to_bits);
        // The borrow ends before the long road below asks the children
        // (which borrow the memo again).
        let kept: Option<(f32, f32)> = self.memo.borrow()[id.index()].iter().find(|m| m.avail == avail && m.final_w == final_w).map(|m| (m.w, m.h));
        if let Some(v) = kept {
            return v;
        }
        self.measured.set(self.measured.get() + 1);
        let (w, h) = self.size_at_uncached(id, avail_w, final_w);
        self.memo.borrow_mut()[id.index()].push(Memo { avail, final_w, w, h });
        (w, h)
    }

    /// The main-axis word a stack reads: no word on the main axis hugs
    /// (a Label in a Column is as tall as its text); an aligned axis hugs.
    fn main_len(&self, id: NodeId, axis: Axis) -> Len {
        let p = self.placement(id);
        let (len, _) = if axis == Axis::X { (p.width, p.halign) } else { (p.height, p.valign) };
        match len {
            // Plain hug carries no slot weight (0) — `hug(n)` alone is
            // the weighted slot (GP34).
            Len::Unset => Len::Hug(0.0),
            other => other,
        }
    }

    /// How much a child gets when its parent already has a width and is
    /// splitting it: a fill child its FLOOR — its content (a Button never
    /// goes under its label; Godot's BoxContainer clamps an expanding
    /// child to its minimum) or its declared `min_*`, whichever is more —
    /// then it takes its share of what's left; any other child its own
    /// size. `cross` = the other axis's known size, for wrapped text. (A
    /// parent that is still asking its children how wide to be uses
    /// `stack_min` instead.)
    fn main_min(&self, id: NodeId, axis: Axis, cross: Option<f32>) -> f32 {
        let p = self.placement(id);
        let (w, h) = self.size_at(id, if axis == Axis::Y { cross } else { None });
        let content = if axis == Axis::X { w } else { h };
        if let Len::Fill(_) = self.main_len(id, axis) {
            let declared = if axis == Axis::X { p.min_width } else { p.min_height };
            return declared.max(content);
        }
        content
    }

    /// The share weight along a stack's axis (0 = a fixed slot).
    fn fill_weight(&self, id: NodeId, axis: Axis) -> f32 {
        match self.main_len(id, axis) {
            Len::Fill(n) => n,
            // `hug(n)` (GP34): a weighted SLOT sharing the leftover like
            // `fill(n)`, holding a box that hugs its content.
            Len::Hug(n) => n,
            _ => 0.0,
        }
    }

    fn size_at_uncached(&self, id: NodeId, avail_w: Option<f32>, final_w: bool) -> (f32, f32) {
        let p = self.placement(id);
        // A HUG width with a cap measures its content NATURALLY (a wrap
        // label counts one unwrapped line) and the cap binds after — the
        // shipped order (GPAR76): the toast plate is
        // min(unwrapped + padding, frame − margins) and the text wraps
        // inside; the height then counts the folded lines at that bound
        // width. Without the natural-first pass the plate hugged its
        // widest WRAPPED line and never reached the cap.
        let hug_w = matches!(p.width, Len::Hug(_)) || (matches!(p.width, Len::Unset) && p.halign != Align::AlignUnset);
        let (cw, ch) = if !final_w && hug_w && (p.max_width.is_some() || p.max_width_fill) {
            let (nw, nh) = self.content_min(id, None);
            // The fill cap = the room minus this box's own margins (the
            // returned size adds them back, so reporting `avail` raw
            // would grow an unwritten ancestor by the margin width per
            // pass).
            let m = self.margins(id);
            let mw = m[0].unwrap_or(0.0) + m[2].unwrap_or(0.0);
            let cap = if p.max_width_fill { avail_w.map(|a| (a - mw).max(0.0)) } else { p.max_width };
            match cap {
                // Only when the cap BINDS: re-measure at the capped
                // width so the height counts the folded lines.
                Some(c) if nw.max(p.min_width) > c => {
                    // The bound width IS the minimum reported upward —
                    // reporting the raw natural would grow an unwritten
                    // ancestor to the unwrapped line and lift the cap.
                    let (_, h2) = self.content_min(id, Some(c));
                    (c, h2)
                }
                _ => (nw, nh),
            }
        } else {
            self.content_min(id, avail_w)
        };
        let w = match p.width {
            Len::Exact(n) => n,
            _ => {
                let mut w = cw.max(p.min_width);
                if let Some(cap) = p.max_width {
                    w = w.min(cap).max(p.min_width);
                }
                w
            }
        };
        let h = match p.height {
            Len::Exact(n) => n,
            _ => {
                let mut h = ch.max(p.min_height);
                if let Some(cap) = p.max_height {
                    h = h.min(cap).max(p.min_height);
                }
                h
            }
        };
        // GP40 reveal: a revealed axis takes only its shown fraction of
        // space — siblings close up (a disclosure collapse, not a clip
        // mask). Scaled LAST so mins and caps shape the whole box first.
        let fx = self.res.num(self.page, id, PropKey::RevealX).map(|f| f.clamp(0.0, 1.0)).unwrap_or(1.0);
        let fy = self.res.num(self.page, id, PropKey::RevealY).map(|f| f.clamp(0.0, 1.0)).unwrap_or(1.0);
        (w * fx, h * fy)
    }

    /// A child's contribution to a free parent's minimum: its minimum plus
    /// the margins it declares on that axis.
    /// What a free child asks its content-sized parent to hold: its size
    /// plus its margins. A positive margin is empty space the parent must
    /// keep; a NEGATIVE margin is the part that hangs out of the parent on
    /// purpose (the app-bar strip's 70 below the screen edge) and is not
    /// the parent's to hold, so it comes off the claim — never below zero.
    /// (WPF and CSS read it the same way; the Godot addon counted a
    /// negative margin as zero, D242, decided away 2026-08-30.)
    fn child_extent(&self, child: NodeId, avail_w: Option<f32>) -> (f32, f32) {
        let (w, h) = self.size_at(child, avail_w);
        let m = self.margins(child);
        let mw = m[0].unwrap_or(0.0) + m[2].unwrap_or(0.0);
        let mh = m[1].unwrap_or(0.0) + m[3].unwrap_or(0.0);
        ((w + mw).max(0.0), (h + mh).max(0.0))
    }

    fn content_min(&self, id: NodeId, avail_w: Option<f32>) -> (f32, f32) {
        let node = self.node(id);
        let inset = self.inner_inset(id);
        let inner_w = avail_w.map(|w| (w - inset[0] - inset[2]).max(0.0));
        let (cw, ch) = match node.kind {
            Kind::Label => {
                let wrap = self.res.bool(self.page, id, PropKey::Wrap).unwrap_or(false);
                let text = self.text_of(id).unwrap_or("");
                let p = self.placement(id);
                // Wrapping happens only where a width BINDS the text: `fill`
                // asks the parent for space, an exact number IS the box, an
                // UNALIGNED axis with no width word stretches over the frame
                // (the tip dialogs' paragraphs), and a cap (`max_width=`)
                // stops a hug. Only a HUG axis — declared, or an aligned
                // axis with no width — measures UNWRAPPED, "size to the
                // text", staying one line until a cap binds it.
                let hugs = matches!(p.width, Len::Hug(_)) || (matches!(p.width, Len::Unset) && p.halign != Align::AlignUnset);
                let bound = if !wrap {
                    None
                } else if !hugs {
                    match p.width {
                        Len::Exact(n) => Some(inner_w.map_or(n - inset[0] - inset[2], |i| i.min(n - inset[0] - inset[2])).max(0.0)),
                        _ => inner_w,
                    }
                } else {
                    let cap = if p.max_width_fill { inner_w } else { p.max_width.map(|c| (c - inset[0] - inset[2]).max(0.0)) };
                    cap.map(|c| inner_w.map_or(c, |i| c.min(i)))
                };
                let (w, h) = self.text.measure(text, self.font_size(id), bound, self.font_key(id));
                // A `clip_text=true` label demands no width — it cuts
                // itself to whatever it gets (the effect captions in
                // tiles). A wrapping label that FILLS its width bends to
                // the box but never below Godot's autowrap minimum, the
                // longest word (GPAR77): a hug Row's leftover then shows
                // a sliver, not nothing.
                let w = if self.res.bool(self.page, id, PropKey::ClipText).unwrap_or(false) {
                    0.0
                } else if wrap && matches!(p.width, Len::Fill(_)) {
                    self.text.measure(text, self.font_size(id), Some(0.0), self.font_key(id)).0
                } else {
                    w
                };
                (w, h)
            }
            // A seat's minimum is its occupant's, margins never counted (the
            // occupant is fitted to the socket rect); a hug occupant measures
            // unbounded (wrap-text on one line), a fill or exact one at the
            // socket's width.
            Kind::Seat => {
                let (mut w, mut h) = (0.0f32, 0.0f32);
                for c in self.sizing_children(id) {
                    let probe = match self.placement(c).width {
                        // fill asks the socket for its width — with none
                        // known it reports its WRAPPED minimum, never the
                        // one-line extent past the screen edge (D263,
                        // GPAR78).
                        Len::Fill(_) => inner_w.or(Some(0.0)),
                        Len::Exact(_) => inner_w,
                        _ => None,
                    };
                    let (cw, ch) = self.size_at(c, probe);
                    w = w.max(cw);
                    h = h.max(ch);
                }
                (w, h)
            }
            Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio => {
                let (mut w, mut h) = (0.0f32, 0.0f32);
                if let Some(text) = self.text_of(id) {
                    let (tw, th) = self.text.measure(text, self.font_size(id), None, self.font_key(id));
                    w = tw;
                    h = th;
                }
                for c in self.sizing_children(id) {
                    let (cw, ch) = self.child_extent(c, inner_w);
                    w = w.max(cw);
                    h = h.max(ch);
                }
                // The native mark (spec "Default look"); a templated one (a
                // body) is a blank surface sized by its body alone.
                if matches!(node.kind, Kind::Check | Kind::Toggle | Kind::Radio) && node.children.1 == 0 {
                    let (mw, mh) = if node.kind == Kind::Toggle { (TOGGLE_PILL.0, TOGGLE_TOUCH_EPX) } else { (CHECK_BOX_EPX, CHECK_BOX_EPX) };
                    w += mw + if self.text_of(id).is_some() { MARK_TEXT_GAP } else { 0.0 };
                    h = h.max(mh);
                }
                (w, h)
            }
            Kind::Column | Kind::Choice => self.stack_min(id, Axis::Y, inner_w),
            Kind::List => {
                let full = self.stack_min(id, Axis::Y, inner_w);
                let p = self.placement(id);
                let horizontal = list_is_row(self.page, id);
                let scroll_hug = if horizontal { matches!(p.width, Len::Hug(_)) } else { matches!(p.height, Len::Hug(_)) };
                if scroll_hug {
                    // `height=hug` (D123): the box IS the content — chosen
                    // explicitly, so the trade is the author's: a hug List
                    // does not scroll. `max_height=` (D124) bounds the
                    // growth (under `layout Row`: the authored max-WIDTH),
                    // and past the cap the internal scroll takes over.
                    let (mut w, mut h) = full;
                    if horizontal {
                        if let Some(cap) = p.max_width {
                            w = w.min(cap);
                        }
                    } else if let Some(cap) = p.max_height {
                        h = h.min(cap);
                    }
                    (w, h)
                } else {
                    // Viewport doctrine (D121/D127): FIXED authored content
                    // — statics, and the `empty` view while it shows —
                    // sizes the box like fixed content anywhere else; the
                    // rows are the viewport's business and report nothing
                    // (the box is the parent's offer).
                    let (w, mut h) = self.list_fixed_min(id, inner_w);
                    if horizontal {
                        // A strip's CROSS axis is content-sized (one row
                        // tall); only the scroll axis is the viewport's.
                        h = full.1;
                    }
                    (w, h)
                }
            }
            Kind::Row => self.stack_min(id, Axis::X, inner_w),
            Kind::Flow => self.flow_min(id, inner_w),
            Kind::Tiles => self.tiles_min(id, inner_w),
            Kind::View | Kind::Layers | Kind::Panel | Kind::ListItem | Kind::Repeat | Kind::Custom | Kind::Slot | Kind::Empty => {
                let (mut w, mut h) = (0.0f32, 0.0f32);
                for c in self.sizing_children(id) {
                    let (cw, ch) = self.child_extent(c, inner_w);
                    w = w.max(cw);
                    h = h.max(ch);
                }
                (w, h)
            }
            Kind::Scroll => {
                let (sx, sy) = self.scroll_axes(id);
                let p = self.placement(id);
                let (mut w, mut h) = (0.0f32, 0.0f32);
                for c in self.sizing_children(id) {
                    let (cw, ch) = self.child_extent(c, if sx { None } else { inner_w });
                    w = w.max(cw);
                    h = h.max(ch);
                }
                // A scrolling axis reports nothing (it scrolls past); a
                // capped hug axis stops at its cap.
                if sx {
                    w = p.max_width.map_or(0.0, |cap| w.min(cap));
                }
                if sy {
                    h = p.max_height.map_or(0.0, |cap| h.min(cap));
                }
                (w, h)
            }
            Kind::Grid => self.grid_min(id, inner_w),
            Kind::GridRow => (0.0, 0.0),
            Kind::Gap | Kind::Skip => (0.0, 0.0),
            Kind::Image => self.image_min(id, avail_w),
            Kind::Rectangle | Kind::Circle | Kind::Arc | Kind::Gradient => (0.0, 0.0),
            Kind::Slider => (0.0, SLIDER_HEIGHT),
            Kind::Input => (0.0, INPUT_HEIGHT),
        };
        (cw + inset[0] + inset[2], ch + inset[1] + inset[3])
    }

    /// The ratio (width / height) an `aspect=` asks for: the number
    /// written, or for `natural` the picture's own size from the host.
    /// `None` = no `aspect=`, or a `natural` whose picture the host has
    /// not measured yet (a `loading=lazy` bitmap still decoding, a runtime
    /// key not uploaded) — the box then has no ratio to follow. One
    /// accessor for the measure road (`image_min`) and the place road
    /// (`aspect_drive`), so both read the same picture (B26-DES).
    fn aspect_ratio(&self, id: NodeId) -> Option<f32> {
        match self.res.prop(self.page, id, PropKey::Aspect) {
            Some(Value::VNum(r)) if r > 0.0 => Some(r),
            Some(Value::VIdent(word)) if self.page.str(word) == "natural" => {
                let key: Option<ImageKey> = match self.res.prop(self.page, id, PropKey::Source) {
                    Some(Value::VImage(key)) if key != NO_IMAGE => Some(key),
                    _ => None,
                };
                key.and_then(|k| self.text.image_size(k)).filter(|(_, h)| *h > 0.0).map(|(w, h)| w / h)
            }
            _ => None,
        }
    }

    /// What `place` derives from an `aspect=`, given the sizing words: the
    /// ratio and which axis drives. `None` = nothing to derive at place
    /// time — no ratio (`aspect_ratio`), or a `natural` with neither axis
    /// spelled, which stays on the measure road (`image_min`: the width
    /// the parent offers drives, the height follows the ratio) — the
    /// catalog thumbnails (`effect_select.gogo`) rely on that.
    /// Numeric: the spelled axis drives; both `fill` or both bare fit
    /// (GPAR80, the addon's _aspect_fit rules). `natural`: the spelled
    /// axis drives (the addon's EXPAND_FIT_WIDTH_PROPORTIONAL when only
    /// the height is spelled, `gogo_resolver.gd` aspect_drive) — a `fill`
    /// height in a 50-high row makes an 80x40 picture 100 wide, where
    /// before it laid out 0 wide (todo-bugs.md B26). A `natural` with
    /// both axes `fill` is left on the measure road too (the addon fits;
    /// nothing spells it yet).
    fn aspect_drive(&self, id: NodeId) -> Option<(f32, AspectDrive)> {
        let ratio = self.aspect_ratio(id)?;
        let p = self.placement(id);
        let spelled = |len: Len| matches!(len, Len::Exact(_) | Len::Fill(_));
        let natural = !matches!(self.res.prop(self.page, id, PropKey::Aspect), Some(Value::VNum(_)));
        if natural {
            return match (spelled(p.width), spelled(p.height)) {
                (false, true) => Some((ratio, AspectDrive::Height)),
                (true, false) => Some((ratio, AspectDrive::Width)),
                _ => None,
            };
        }
        let both_free = matches!(p.width, Len::Fill(_)) && matches!(p.height, Len::Fill(_)) || p.width == Len::Unset && p.height == Len::Unset;
        let h_drives = matches!(p.height, Len::Exact(_)) || (matches!(p.height, Len::Fill(_)) && !spelled(p.width));
        Some((ratio, if both_free { AspectDrive::Fit } else if h_drives { AspectDrive::Height } else { AspectDrive::Width }))
    }

    /// An Image whose `aspect=` derives the WIDTH from a spelled height:
    /// the width once the height it gets is known. `image_min` cannot
    /// answer this — a measure is asked per width, never per height (a
    /// Row's cross axis is not in the question) — so the stack road asks
    /// here after it knows the child's height (the addon's Container
    /// grants the TextureRect's minimum, re-derived from its rect height).
    fn width_from_height(&self, id: NodeId, h: f32) -> Option<f32> {
        match self.aspect_drive(id)? {
            (ratio, AspectDrive::Height) => Some(h * ratio),
            _ => None,
        }
    }

    /// `aspect=natural` (the bitmap's ratio) or `aspect=<w/h>`: the axis
    /// the file sized drives, the other follows; with neither, the width
    /// the parent offers drives (WPF UniformToFill measure).
    fn image_min(&self, id: NodeId, avail_w: Option<f32>) -> (f32, f32) {
        let Some(ratio) = self.aspect_ratio(id) else { return (0.0, 0.0) };
        let p = self.placement(id);
        match (p.width, p.height) {
            (Len::Exact(w), _) => (w, w / ratio),
            (_, Len::Exact(h)) => (h * ratio, h),
            _ => match avail_w {
                Some(w) => (0.0, w / ratio),
                None => (0.0, 0.0),
            },
        }
    }

    /// A stack that is asking its children how big to be (`hug`): the sum
    /// of what they need along its axis, the widest across.
    fn stack_min(&self, id: NodeId, axis: Axis, inner_w: Option<f32>) -> (f32, f32) {
        let spacing = self.spacing(id);
        let children = self.sizing_children(id);
        let (mut main, mut cross) = (0.0f32, 0.0f32);
        for (i, c) in children.iter().enumerate() {
            let node = self.node(*c);
            // A parent that is asking its children how wide to be: every
            // child, fill included, answers with its content. (A parent
            // that was told its size splits it in `place_stack`.)
            let (m, x) = if node.kind == Kind::Gap {
                (if node.gap_fill { 0.0 } else { node.gap }, 0.0)
            } else {
                // Whole-px stack minimums are NOT ported (E10, user-
                // approved 2026-09-01): Godot's _box_min truncation only mirrors
                // its integer text metrics (Label ceils its measure), so
                // it is a no-op there; Rust measures fractionally end to
                // end, and truncating here made wrap labels wrap a line
                // early — behavior Godot never shows.
                let (w, h) = self.size_at(*c, if axis == Axis::Y { inner_w } else { None });
                if axis == Axis::Y { (h, w) } else { (w, h) }
            };
            main += m;
            if i > 0 {
                main += spacing;
            }
            cross = cross.max(x);
        }
        if axis == Axis::Y { (cross, main) } else { (main, cross) }
    }

    fn flow_min(&self, id: NodeId, inner_w: Option<f32>) -> (f32, f32) {
        let spacing = self.spacing(id);
        let children = self.visible_children(id);
        let sizes: Vec<(f32, f32)> = children.iter().map(|c| self.size_at(*c, None)).collect();
        let widest = sizes.iter().map(|s| s.0).fold(0.0, f32::max);
        match inner_w {
            None => {
                let tallest = sizes.iter().map(|s| s.1).fold(0.0, f32::max);
                (widest, tallest)
            }
            Some(avail) => {
                let mut rows = flow_rows(&sizes, avail.max(widest), spacing);
                let total_h: f32 = rows.iter().map(|r| r.height).sum::<f32>() + spacing * (rows.len().saturating_sub(1)) as f32;
                rows.clear();
                (widest, total_h)
            }
        }
    }

    // ----- placement ---------------------------------------------------------

    /// The kind of the box that places `id` — its declared parent, looked
    /// past the two nodes that are not boxes: a Grid's `row { }` (the
    /// cell's parent is the Grid, as in the addon, where a row is no node)
    /// and a `repeat` (its items take the host's slots, the way
    /// `visible_children` lifts them). `None` for the root.
    fn placing_parent_kind(&self, id: NodeId) -> Option<Kind> {
        let mut parent = self.node(id).parent;
        while parent != NO_NODE && parent.index() < self.page.nodes.len() {
            let kind = self.node(parent).kind;
            if !matches!(kind, Kind::GridRow | Kind::Repeat) {
                return Some(kind);
            }
            parent = self.node(parent).parent;
        }
        None
    }

    /// Lays `id` into `frame` (the space its parent offers, margins not yet
    /// applied) and recurses. `written`: per axis, whether the frame's
    /// extent is a number the author wrote (see `Lay::written`). The
    /// document root fills its viewport unless it says otherwise.
    fn place(&mut self, id: NodeId, frame: Rect, written: [bool; 2]) {
        let mut p = *self.placement(id);
        // The general road serves container parents too (Panel, ListItem,
        // a Scroll's content, a Grid's cells): their Image default holds
        // here as in a stack. The parent is the DECLARED one — a seated
        // occupant never reaches `place` (the Seat arm fits it).
        if let Some(parent_kind) = self.placing_parent_kind(id) {
            image_center_default(parent_kind, self.node(id).kind, &mut p);
        }
        let m = self.margins(id);
        // An `automatic=` slot: the value picks its extent or its seat
        // along the host's width (the sliders here are horizontal).
        let frame = match self.node(id).auto {
            Auto::Length(_) => {
                let f = self.res.auto_frac(self.page, id);
                Rect::new(frame.x, frame.y, frame.w * f, frame.h)
            }
            Auto::Position(_) => {
                let f = self.res.auto_frac(self.page, id);
                let (own_w, _) = self.size_at(id, Some(frame.w));
                p.width = Len::Hug(0.0);
                Rect::new(frame.x + (frame.w - own_w) * f, frame.y, own_w, frame.h)
            }
            // GP33 `automatic=angle` (the knob-thumb seat): the slot's
            // CENTER sits on the circle centered in its host, inside the
            // rim by half the slot's own extent, at from + fraction x
            // sweep (degrees, Godot's screen convention: 0 = right,
            // clockwise positive).
            Auto::Angle { from, sweep, .. } => {
                let f = self.res.auto_frac(self.page, id);
                let (ow, oh) = self.size_at(id, Some(frame.w));
                let radius = (frame.w.min(frame.h) / 2.0 - ow.max(oh) / 2.0).max(0.0);
                let theta = (from + f * sweep).to_radians();
                p.width = Len::Hug(0.0);
                p.height = Len::Hug(0.0);
                Rect::new(frame.x + frame.w / 2.0 + theta.cos() * radius - ow / 2.0, frame.y + frame.h / 2.0 + theta.sin() * radius - oh / 2.0, ow, oh)
            }
            _ => frame,
        };
        // Text is the one content a box cannot bend: a Label keeps the
        // width its text needs even inside a written box, and the written
        // box cuts it (draw) — otherwise the overflow would hide inside a
        // label rect that never pokes past, and nothing would clip it.
        let written = if self.node(id).kind == Kind::Label { [false, false] } else { written };
        let (min_w, _) = self.size_at(id, Some(frame.w));
        let (x, w) = free_axis(frame.x, frame.w, p.width, p.halign, m[0], m[2], min_w, p.min_width, p.max_width, p.max_width_fill, written[0]);
        // The height minimum at the width the node actually got (wrap
        // text, Flow rows).
        let (_, min_h) = self.size_final(id, w);
        let (y, h) = free_axis(frame.y, frame.h, p.height, p.valign, m[1], m[3], min_h, p.min_height, p.max_height, p.max_height_fill, written[1]);
        // An `aspect=` derives the unspelled axis from the driven one
        // (numeric or `natural` — `aspect_drive` says which axis; a
        // `natural` with no axis spelled already came out of `image_min`
        // above). Both axes `fill` (or both bare) on a number = the
        // largest ratio box that fits, centered (GPAR80, the addon's
        // _aspect_fit rules). The derived axis is re-seated as an exact
        // size, so `halign=left` on a fill-height Image puts the derived
        // width at the start edge instead of shrinking it to the 0 the
        // measure answered (todo-bugs.md B26: the mask tiles drew nothing).
        let (x, y, w, h) = match self.aspect_drive(id) {
            Some((ratio, AspectDrive::Fit)) => {
                let fw = h * ratio;
                let (w2, h2) = if fw <= w { (fw, h) } else { (w, w / ratio) };
                (x + (w - w2) / 2.0, y + (h - h2) / 2.0, w2, h2)
            }
            Some((ratio, AspectDrive::Height)) => {
                let w2 = h * ratio;
                let (x2, _) = free_axis(frame.x, frame.w, Len::Exact(w2), p.halign, m[0], m[2], w2, 0.0, None, false, written[0]);
                (x2, y, w2, h)
            }
            Some((ratio, AspectDrive::Width)) => {
                let h2 = w / ratio;
                let (y2, _) = free_axis(frame.y, frame.h, Len::Exact(h2), p.valign, m[1], m[3], h2, 0.0, None, false, written[1]);
                (x, y2, w, h2)
            }
            None => (x, y, w, h),
        };
        let rect = Rect::new(x, y, w, h);
        self.rects[id] = rect;
        // What this box hands its own children: a written size or cap is
        // the author's number; a box that merely fills passes on what it
        // was given; a hug box is sized by content, not by a number.
        let fills = |len: Len, align: Align| matches!(len, Len::Fill(_)) || (len == Len::Unset && align == Align::AlignUnset);
        self.written[id] = [
            matches!(p.width, Len::Exact(_)) || p.max_width.is_some() || p.max_width_fill || (fills(p.width, p.halign) && written[0]),
            matches!(p.height, Len::Exact(_)) || p.max_height.is_some() || p.max_height_fill || (fills(p.height, p.valign) && written[1]),
        ];
        self.place_children(id, rect);
    }

    fn place_children(&mut self, id: NodeId, rect: Rect) {
        let inset = self.inner_inset(id);
        let inner = rect.inset(inset[0], inset[1], inset[2], inset[3]);
        let kind = self.node(id).kind;
        match kind {
            Kind::Column | Kind::Choice => self.place_stack(id, inner, Axis::Y),
            Kind::List => self.place_list(id, inner),
            Kind::Row => self.place_stack(id, inner, Axis::X),
            Kind::Flow => self.place_flow(id, inner),
            Kind::Tiles => self.place_tiles(id, inner),
            Kind::Grid => self.place_grid(id, inner),
            Kind::Scroll => self.place_scroll(id, inner),
            // The occupant takes the socket's whole rect: its own alignment
            // and margins never bind here.
            Kind::Seat => {
                for c in self.visible_children(id) {
                    self.rects[c] = inner;
                    self.place_children(c, inner);
                }
            }
            _ => {
                let written = self.written[id];
                for c in self.visible_children(id) {
                    self.place(c, inner, written);
                }
            }
        }
    }

    fn place_stack(&mut self, id: NodeId, inner: Rect, axis: Axis) {
        let spacing = self.spacing(id);
        let children = self.visible_children(id);
        let extent = if axis == Axis::Y { inner.h } else { inner.w };
        let cross = if axis == Axis::Y { Some(inner.w) } else { None };
        // Each child's slot: (floor or fixed size, share weight).
        let slots: Vec<(f32, f32)> = children
            .iter()
            .map(|c| {
                let node = self.node(*c);
                if node.kind == Kind::Gap {
                    if node.gap_fill { (0.0, node.gap) } else { (node.gap, 0.0) }
                } else {
                    // A Row child whose `aspect=` derives its width from
                    // its height (`Image height=fill, aspect=natural`):
                    // the slot is that width. The measure road answers 0
                    // here — it is asked per width and never sees the
                    // row's height (todo-bugs.md B26) — so the height
                    // the child will get across the row is worked out
                    // first, the way the cross axis below does it.
                    let derived = if axis == Axis::X {
                        let cp = self.placement(*c);
                        let (_, cross_h) = free_axis(inner.y, inner.h, cp.height, cp.valign, None, None, 0.0, cp.min_height, cp.max_height, cp.max_height_fill, self.written[id][1]);
                        self.width_from_height(*c, cross_h)
                    } else {
                        None
                    };
                    (derived.unwrap_or_else(|| self.main_min(*c, axis, cross)), self.fill_weight(*c, axis))
                }
            })
            .collect();
        let sizes = star_split(extent - spacing * children.len().saturating_sub(1) as f32, &slots);
        let mut cursor = if axis == Axis::Y { inner.y } else { inner.x };
        for (i, c) in children.iter().enumerate() {
            let size = sizes[i];
            let node = self.node(*c);
            if node.kind == Kind::Gap {
                self.rects[*c] = if axis == Axis::Y { Rect::new(inner.x, cursor, inner.w, size) } else { Rect::new(cursor, inner.y, size, inner.h) };
                cursor += size + spacing;
                continue;
            }
            let mut p = *self.placement(*c);
            image_center_default(self.node(id).kind, node.kind, &mut p);
            // The cross axis is a free axis; the main axis is the slot. The
            // cross measure is taken at the width the child got.
            // `hug(n)` (GP34): the slot shares the leftover, the BOX
            // inside it hugs its content, seated per the main-axis align.
            let hug_w = matches!(if axis == Axis::Y { p.height } else { p.width }, Len::Hug(n) if n > 0.0);
            let (main_pos, main_size) = if hug_w {
                let m = slots[i].0.min(size);
                let a = if axis == Axis::Y { p.valign } else { p.halign };
                let off = match a {
                    Align::Center => (size - m) / 2.0,
                    Align::End => size - m,
                    _ => 0.0,
                };
                (cursor + off, m)
            } else {
                (cursor, size)
            };
            let rect = if axis == Axis::Y {
                let (min_w, _) = self.size_at(*c, Some(inner.w));
                // A Column child whose `aspect=` derives its width from
                // its height: the width follows the slot it got (B26).
                let min_w = self.width_from_height(*c, main_size).unwrap_or(min_w);
                let (x, w) = free_axis(inner.x, inner.w, p.width, p.halign, None, None, min_w, p.min_width, p.max_width, p.max_width_fill, self.written[id][0]);
                Rect::new(x, main_pos, w, main_size)
            } else {
                let (_, min_h) = self.size_final(*c, main_size);
                let (y, h) = free_axis(inner.y, inner.h, p.height, p.valign, None, None, min_h, p.min_height, p.max_height, p.max_height_fill, self.written[id][1]);
                Rect::new(main_pos, y, main_size, h)
            };
            self.rects[*c] = rect;
            self.place_children(*c, rect);
            cursor += size + spacing;
        }
    }

    fn place_flow(&mut self, id: NodeId, inner: Rect) {
        let spacing = self.spacing(id);
        let children = self.visible_children(id);
        let sizes: Vec<(f32, f32)> = children.iter().map(|c| self.size_at(*c, None)).collect();
        if self.res.ident(self.page, id, PropKey::Orientation) == Some("vertical") {
            // `orientation=vertical` (GPAR97, WrapPanel's other axis):
            // items run DOWN and wrap into columns — the same packing,
            // transposed (a column is as wide as its widest item).
            let swapped: Vec<(f32, f32)> = sizes.iter().map(|(w, h)| (*h, *w)).collect();
            let cols = flow_rows(&swapped, inner.h, spacing);
            let mut x = inner.x;
            for col in cols {
                // The transposed stretch: `height=fill` takes the
                // column's leftover (GP34).
                let count = col.end - col.first;
                let used: f32 = sizes[col.first..col.end].iter().map(|s| s.1).sum::<f32>() + spacing * count.saturating_sub(1) as f32;
                let weights: f32 = (col.first..col.end).map(|i| if let Len::Fill(n) = self.placement(children[i]).height { n } else { 0.0 }).sum();
                let extra = (inner.h - used).max(0.0);
                let mut y = inner.y;
                for i in col.first..col.end {
                    let c = children[i];
                    let (w, h) = sizes[i];
                    let mut p = *self.placement(c);
                    image_center_default(Kind::Flow, self.node(c).kind, &mut p);
                    let h = if let Len::Fill(n) = p.height {
                        let stretched = if weights > 0.0 { h + extra * n / weights } else { h };
                        p.max_height.map_or(stretched, |c| stretched.min(c))
                    } else {
                        h
                    };
                    let (cx, cw) = free_axis(x, col.height, p.width, p.halign, None, None, w, p.min_width, p.max_width, false, false);
                    let rect = Rect::new(cx, y, cw, h);
                    self.rects[c] = rect;
                    self.place_children(c, rect);
                    y += h + spacing;
                }
                x += col.height + spacing;
            }
            return;
        }
        let rows = flow_rows(&sizes, inner.w, spacing);
        let mut y = inner.y;
        for row in rows {
            // `width=fill` in a Flow stretches across its LINE (GP34, the
            // shipped expand flag): the row's leftover is shared by the
            // fill children by weight; everyone else keeps their measure.
            let count = row.end - row.first;
            let used: f32 = sizes[row.first..row.end].iter().map(|s| s.0).sum::<f32>() + spacing * count.saturating_sub(1) as f32;
            let weights: f32 = (row.first..row.end).map(|i| if let Len::Fill(n) = self.placement(children[i]).width { n } else { 0.0 }).sum();
            let extra = (inner.w - used).max(0.0);
            let mut x = inner.x;
            for i in row.first..row.end {
                let c = children[i];
                let (w, h) = sizes[i];
                // A Flow is a container (the addon's FlowContainer): the
                // Image centring default holds on the line's cross axis.
                let mut p = *self.placement(c);
                image_center_default(Kind::Flow, self.node(c).kind, &mut p);
                let w = if let Len::Fill(n) = p.width {
                    // The stretch stops at a numeric cap (the cap law:
                    // a box never grows past its cap).
                    let stretched = if weights > 0.0 { w + extra * n / weights } else { w };
                    p.max_width.map_or(stretched, |c| stretched.min(c))
                } else {
                    w
                };
                let (cy, ch) = free_axis(y, row.height, p.height, p.valign, None, None, h, p.min_height, p.max_height, false, false);
                let rect = Rect::new(x, cy, w, ch);
                self.rects[c] = rect;
                self.place_children(c, rect);
                x += w + spacing;
            }
            y += row.height + spacing;
        }
    }

    // ----- tiles -------------------------------------------------------------

    /// The Tiles lattice: unit = width / cols, cells abut (no gutters —
    /// `spacing=` says nothing here); a cell is `span` units (`full_row` =
    /// all of them), packed first-fit in declaration order, wrapping
    /// whole; a row is as tall as its tallest cell.
    fn tiles_cells(&self, id: NodeId, inner_w: f32) -> (Vec<(NodeId, Rect)>, f32) {
        let spacing = 0.0f32;
        let cols = self.res.num(self.page, id, PropKey::Cols).unwrap_or(1.0).max(1.0) as usize;
        let unit = (inner_w / cols as f32).max(0.0);
        let mut out: Vec<(NodeId, Rect)> = Vec::new();
        let mut x = 0.0f32;
        let mut in_row = 0usize;
        let mut y = 0.0f32;
        let mut row_h = 0.0f32;
        let mut row_first = 0usize;
        // Godot's `_pack` shape: x advances by each cell's own width and
        // a row wraps when the next cell would poke past the edge — span
        // cells step by `unit × span` (the lattice), an EXACT-width cell
        // packs at its own pitch (the shipped icon-size flip; GPAR75/115.
        // Godot takes the fixed width from a code `set_cell` only; here
        // any exact width on the cell means it — one rule, no code-only
        // channel).
        for c in self.visible_children(id) {
            let full = self.res.bool(self.page, c, PropKey::FullRow).unwrap_or(false);
            let span = if full { cols } else { (self.res.num(self.page, c, PropKey::Span).unwrap_or(1.0).max(1.0) as usize).min(cols) };
            let w = if full {
                inner_w
            } else if let Len::Exact(fw) = self.res.placement[c].width {
                fw
            } else {
                unit * span as f32 + spacing * (span as f32 - 1.0)
            };
            if in_row > 0 && x + w > inner_w + 0.01 {
                y += row_h + spacing;
                for (_, r) in &mut out[row_first..] {
                    r.h = row_h;
                }
                row_first = out.len();
                x = 0.0;
                in_row = 0;
                row_h = 0.0;
            }
            let (_, h) = self.size_final(c, w);
            out.push((c, Rect::new(x, y, w, h)));
            row_h = row_h.max(h);
            x += w + spacing;
            in_row += 1;
        }
        for (_, r) in &mut out[row_first..] {
            r.h = row_h;
        }
        let total_h = if out.is_empty() { 0.0 } else { y + row_h };
        (out, total_h)
    }

    fn tiles_min(&self, id: NodeId, inner_w: Option<f32>) -> (f32, f32) {
        // Minimum width is the widest cell's own minimum (FlowContainer's
        // rule); the height needs a granted width.
        let widest = self.visible_children(id).iter().map(|c| self.size_at(*c, None).0).fold(0.0, f32::max);
        match inner_w {
            None => (widest, 0.0),
            Some(w) => {
                let (_, h) = self.tiles_cells(id, w);
                (widest, h)
            }
        }
    }

    /// Tiles is a container in the addon, but its cells are fixed to the
    /// lattice (a cell IS its slot: unit × span wide, its own height), so
    /// the Image centring default (`image_center_default`) has no free
    /// axis to act on here and is not applied.
    fn place_tiles(&mut self, id: NodeId, inner: Rect) {
        let (cells, _) = self.tiles_cells(id, inner.w);
        for (c, r) in cells {
            let (_, own_h) = self.size_at(c, Some(r.w));
            // A cell is as tall as its own minimum, top-aligned in its row —
            // heights are content truth, `height=fill` does not stretch a
            // cell to the row.
            let h = own_h;
            let rect = Rect::new(inner.x + r.x, inner.y + r.y, r.w, h);
            self.rects[c] = rect;
            self.place_children(c, rect);
        }
    }

    /// The bars' lanes (bottom bar height, right bar width): Godot's
    /// ScrollContainer keeps a bar's lane out of the content when the
    /// content overflows that axis and `bars=` is not `never` — 8 device
    /// px on the phone theme, 4 epx here.
    fn scroll_bars(&self, id: NodeId, inner: Rect) -> (f32, f32) {
        let mode = self.res.ident(self.page, id, PropKey::Bars);
        if mode == Some("never") {
            return (0.0, 0.0);
        }
        let (sx, sy) = self.scroll_axes(id);
        // `always`/`reserve` hold the lane on every scrolling axis even
        // with nothing overflowing (GP36) — content never shifts when the
        // bar appears.
        if matches!(mode, Some("always") | Some("reserve")) {
            return (if sx { SCROLL_BAR_EPX } else { 0.0 }, if sy { SCROLL_BAR_EPX } else { 0.0 });
        }
        let (mut content_w, mut content_h) = (0.0f32, 0.0f32);
        for c in self.visible_children(id) {
            let (w, h) = self.size_at(c, if sx { None } else { Some(inner.w) });
            content_w = content_w.max(w);
            content_h = content_h.max(h);
        }
        let bottom = if sx && content_w > inner.w { SCROLL_BAR_EPX } else { 0.0 };
        let right = if sy && content_h > inner.h { SCROLL_BAR_EPX } else { 0.0 };
        (bottom, right)
    }

    fn place_scroll(&mut self, id: NodeId, inner: Rect) {
        let (sx, sy) = self.scroll_axes(id);
        let offset = self.res_scroll(id);
        let (bar_bottom, bar_right) = self.scroll_bars(id, inner);
        let inner = Rect::new(inner.x, inner.y, (inner.w - bar_right).max(0.0), (inner.h - bar_bottom).max(0.0));
        for c in self.visible_children(id) {
            let (min_w, min_h) = self.size_at(c, if sx { None } else { Some(inner.w) });
            // Content takes the viewport and scrolls past it when larger.
            let w = if sx { inner.w.max(min_w) } else { inner.w };
            let h = if sy { inner.h.max(min_h) } else { inner.h };
            let frame = Rect::new(inner.x - if sx { offset[0] } else { 0.0 }, inner.y - if sy { offset[1] } else { 0.0 }, w, h);
            let written = self.written[id];
            self.place(c, frame, [!sx && written[0], !sy && written[1]]);
        }
    }

    /// The List IS its own viewport (D121, the addon's internal
    /// GogoScroll): its body — statics, the `empty` view and the rows —
    /// stacks in a frame as tall as the content and pans under the box,
    /// so statics scroll WITH the rows, one doctrine both ways.
    fn place_list(&mut self, id: NodeId, inner: Rect) {
        let horizontal = list_is_row(self.page, id);
        let offset = self.res_scroll(id);
        // bars= (D138): the words a Scroll takes; an overflowing axis
        // reserves the bar's lane unless `never` (the AUTO mode).
        let probe = self.stack_min(id, Axis::Y, if horizontal { None } else { Some(inner.w) });
        let overflow = if horizontal { probe.0 > inner.w } else { probe.1 > inner.h };
        let lane = if overflow && self.res.ident(self.page, id, PropKey::Bars) != Some("never") { SCROLL_BAR_EPX } else { 0.0 };
        let inner = if horizontal {
            Rect::new(inner.x, inner.y, inner.w, (inner.h - lane).max(0.0))
        } else {
            Rect::new(inner.x, inner.y, (inner.w - lane).max(0.0), inner.h)
        };
        let frame = if horizontal {
            let w = self.stack_min(id, Axis::Y, None).0;
            Rect::new(inner.x - offset[0], inner.y, w.max(inner.w), inner.h)
        } else {
            let h = self.stack_min(id, Axis::Y, Some(inner.w)).1;
            Rect::new(inner.x, inner.y - offset[1], inner.w, h.max(inner.h))
        };
        self.place_stack(id, frame, Axis::Y);
    }

    /// Minimum of a List's non-repeating content: visible non-row
    /// children stacked with spacing — exact in the empty state, a
    /// floor always. Rows never size the box, and a `layout` strip is
    /// the rows' seat, skipped whole (D128).
    fn list_fixed_min(&self, id: NodeId, inner_w: Option<f32>) -> (f32, f32) {
        let spacing = self.spacing(id);
        let (mut w, mut h, mut count) = (0.0f32, 0.0f32, 0u32);
        for c in self.sizing_children(id) {
            let n = self.node(c);
            if n.kind == Kind::ListItem {
                continue;
            }
            if n.kind == Kind::Row && self.res.children(self.page, c).any(|g| self.node(g).kind == Kind::Repeat) {
                continue;
            }
            let (cw, ch) = self.child_extent(c, inner_w);
            w = w.max(cw);
            h += ch;
            count += 1;
        }
        if count > 1 {
            h += spacing * (count - 1) as f32;
        }
        (w, h)
    }

    fn res_scroll(&self, _id: NodeId) -> [f32; 2] {
        // Scroll offsets live in the Runtime; the caller folds them into
        // `Resolved` through `Resolved::scroll` when present.
        self.res.scroll.get(_id.index()).copied().unwrap_or([0.0, 0.0])
    }

    fn scroll_axes(&self, id: NodeId) -> (bool, bool) {
        scroll_axes_of(self.page, self.res, id)
    }

    // ----- grid --------------------------------------------------------------

    fn grid_tracks(&self, id: NodeId) -> Vec<Len> {
        let Some(cols) = self.res.prop(self.page, id, PropKey::Cols) else { return vec![Len::Fill(1.0)] };
        self.page
            .list(cols)
            .iter()
            // A `$ref` entry (`cols=[80, $w]`) is a theme number: deref
            // here or the track silently lays out as `fill` (GPAR114).
            .map(|v| match crate::runtime::deref(self.page, self.res.variant, *v) {
                Value::VNum(n) => Len::Exact(n),
                Value::VIdent(w) if self.page.str(w) == "hug" => Len::Hug(0.0),
                Value::VIdent(_) => Len::Fill(1.0),
                Value::VCall { name, args } => {
                    let weight = self.page.values.get(args.0 as usize).and_then(|a| if let Value::VNum(n) = a { Some(*n) } else { None }).unwrap_or(1.0);
                    if self.page.str(name) == "hug" { Len::Hug(weight) } else { Len::Fill(weight) }
                }
                _ => Len::Fill(1.0),
            })
            .collect()
    }

    /// Every cell of the grid with its lattice place and measured size —
    /// rows walk left to right, `skip` covers one track.
    fn grid_cells(&self, rows: &[NodeId], inner_w: Option<f32>) -> Vec<GridCell> {
        let mut out = Vec::new();
        for (ri, r) in rows.iter().enumerate() {
            let mut track = 0usize;
            for c in self.page.children(*r) {
                if self.node(c).kind == Kind::Skip {
                    track += 1;
                    continue;
                }
                let col_span = self.res.num(self.page, c, PropKey::ColSpan).unwrap_or(1.0).max(1.0) as usize;
                let row_span = self.res.num(self.page, c, PropKey::RowSpan).unwrap_or(1.0).max(1.0) as usize;
                if !self.gone(c) {
                    let (w, h) = self.size_at(c, inner_w);
                    out.push(GridCell { node: c, row: ri, col: track, col_span, row_span, w, h });
                }
                track += col_span;
            }
        }
        out
    }

    /// A row's declared height: its own `height=`, else the grid's
    /// `row_height=` default, else content (`Unset`).
    fn grid_row_len(&self, grid: NodeId, row: NodeId) -> Len {
        let own = self.placement(row).height;
        if !matches!(own, Len::Unset) {
            return own;
        }
        match self.res.prop(self.page, grid, PropKey::RowHeight) {
            Some(Value::VNum(v)) => Len::Exact(v),
            Some(Value::VIdent(w)) if self.page.str(w) == "fill" => Len::Fill(1.0),
            Some(Value::VCall { name, args }) if self.page.str(name) == "fill" => {
                let w = self.page.values.get(args.0 as usize).and_then(|a| if let Value::VNum(n) = a { Some(*n) } else { None }).unwrap_or(1.0);
                Len::Fill(w)
            }
            _ => Len::Unset,
        }
    }

    /// Track and row minimums: px and written heights as declared, hug
    /// from single-span cells, then each spanned cell's deficit spread
    /// equally over the hug tracks/rows it covers (fill next; an
    /// all-fixed span clips — fixed is fixed; the shipped GogoGrid
    /// `_base_sizes`).
    fn grid_measure(&self, id: NodeId, inner_w: Option<f32>) -> (Vec<Len>, Vec<f32>, Vec<NodeId>, Vec<Len>, Vec<f32>, Vec<GridCell>) {
        let spacing = self.spacing(id);
        let tracks = self.grid_tracks(id);
        let rows = self.visible_children(id);
        let row_len: Vec<Len> = rows.iter().map(|r| self.grid_row_len(id, *r)).collect();
        let cells = self.grid_cells(&rows, inner_w);
        let mut track_min = vec![0.0f32; tracks.len()];
        for (i, t) in tracks.iter().enumerate() {
            if let Len::Exact(n) = t {
                track_min[i] = *n;
            }
        }
        for cell in &cells {
            if cell.col_span == 1 && cell.col < tracks.len() && !matches!(tracks[cell.col], Len::Exact(_)) {
                track_min[cell.col] = track_min[cell.col].max(cell.w);
            }
        }
        for cell in &cells {
            if cell.col_span > 1 {
                spread(&mut track_min, cell.col, cell.col_span, cell.w, spacing, |i| !matches!(tracks.get(i), Some(Len::Exact(_)) | Some(Len::Fill(_))), |i| matches!(tracks.get(i), Some(Len::Fill(_))));
            }
        }
        let mut row_min = vec![0.0f32; rows.len()];
        for (ri, r) in rows.iter().enumerate() {
            row_min[ri] = match row_len[ri] {
                Len::Exact(n) => n,
                _ => self.placement(*r).min_height,
            };
        }
        for cell in &cells {
            if cell.row_span == 1 && cell.row < rows.len() && !matches!(row_len[cell.row], Len::Exact(_)) {
                row_min[cell.row] = row_min[cell.row].max(cell.h);
            }
        }
        for cell in &cells {
            if cell.row_span > 1 {
                spread(&mut row_min, cell.row, cell.row_span, cell.h, spacing, |i| matches!(row_len.get(i), Some(Len::Unset) | Some(Len::Hug(_))), |i| matches!(row_len.get(i), Some(Len::Fill(_))));
            }
        }
        (tracks, track_min, rows, row_len, row_min, cells)
    }

    fn grid_min(&self, id: NodeId, inner_w: Option<f32>) -> (f32, f32) {
        let spacing = self.spacing(id);
        let (tracks, track_min, _rows, _row_len, row_min, _cells) = self.grid_measure(id, inner_w);
        let w: f32 = track_min.iter().sum::<f32>() + spacing * tracks.len().saturating_sub(1) as f32;
        let h: f32 = row_min.iter().sum::<f32>() + spacing * row_min.len().saturating_sub(1) as f32;
        (w, h)
    }

    fn place_grid(&mut self, id: NodeId, inner: Rect) {
        let spacing = self.spacing(id);
        let (tracks, track_min, rows, row_len, row_min, cells) = self.grid_measure(id, Some(inner.w));
        // The grid was told its size and splits it, the way a stack does:
        // a fill track shares what's left (smallest 0), a fill row shares
        // too (smallest its `min_height`); the rest take their measured size.
        let track_slots: Vec<(f32, f32)> = tracks.iter().zip(&track_min).map(|(t, m)| if let Len::Fill(w) = t { (0.0, *w) } else { (*m, 0.0) }).collect();
        let track_size: Vec<f32> = star_split(inner.w - spacing * tracks.len().saturating_sub(1) as f32, &track_slots);
        let row_slots: Vec<(f32, f32)> = rows.iter().enumerate().map(|(ri, r)| if let Len::Fill(w) = row_len[ri] { (self.placement(*r).min_height, w) } else { (row_min[ri], 0.0) }).collect();
        let row_size: Vec<f32> = star_split(inner.h - spacing * rows.len().saturating_sub(1) as f32, &row_slots);
        let mut track_x = Vec::with_capacity(tracks.len());
        let mut x = inner.x;
        for s in &track_size {
            track_x.push(x);
            x += s + spacing;
        }
        let mut row_y = Vec::with_capacity(rows.len());
        let mut y = inner.y;
        for (ri, r) in rows.iter().enumerate() {
            row_y.push(y);
            self.rects[*r] = Rect::new(inner.x, y, inner.w, row_size[ri]);
            y += row_size[ri] + spacing;
        }
        for cell in &cells {
            if cell.col >= tracks.len() || cell.row >= rows.len() {
                continue;
            }
            let last_c = (cell.col + cell.col_span).min(tracks.len());
            let last_r = (cell.row + cell.row_span).min(rows.len());
            let cw: f32 = track_size[cell.col..last_c].iter().sum::<f32>() + spacing * (last_c - cell.col - 1) as f32;
            let ch: f32 = row_size[cell.row..last_r].iter().sum::<f32>() + spacing * (last_r - cell.row - 1) as f32;
            self.place(cell.node, Rect::new(track_x[cell.col], row_y[cell.row], cw, ch), [false, false]);
        }
    }
}

/// One grid cell with its lattice place and measured natural size.
struct GridCell {
    node: NodeId,
    row: usize,
    col: usize,
    col_span: usize,
    row_span: usize,
    w: f32,
    h: f32,
}

/// Spread a spanned cell's size deficit into the spannable slots: hug
/// slots take it equally; with none, fill slots; with none, the content
/// clips — fixed is fixed (the shipped GogoGrid `_base_sizes`).
fn spread(base: &mut [f32], at: usize, span: usize, need: f32, spacing: f32, hug: impl Fn(usize) -> bool, fill: impl Fn(usize) -> bool) {
    let last = (at + span).min(base.len());
    if last <= at {
        return;
    }
    let have: f32 = base[at..last].iter().sum::<f32>() + spacing * (last - at - 1) as f32;
    let deficit = need - have;
    if deficit <= 0.0 {
        return;
    }
    let takers: Vec<usize> = (at..last).filter(|i| hug(*i)).collect();
    let takers: Vec<usize> = if takers.is_empty() { (at..last).filter(|i| fill(*i)).collect() } else { takers };
    if takers.is_empty() {
        return;
    }
    for i in &takers {
        base[*i] += deficit / takers.len() as f32;
    }
}

/// ARRANGE along one axis (spec "Placement", the star rule): fixed slots
/// take their size, fill slots share what is left by weight, and a fill
/// slot whose share is under its floor takes the floor — the rest
/// re-share. `slots`: (size, or floor for a fill; fill weight, 0 = fixed).
/// `extent` = the room after spacing.
fn star_split(extent: f32, slots: &[(f32, f32)]) -> Vec<f32> {
    let mut out: Vec<f32> = slots.iter().map(|(m, _)| *m).collect();
    let mut fixed: Vec<bool> = slots.iter().map(|(_, w)| *w <= 0.0).collect();
    loop {
        let used: f32 = out.iter().zip(&fixed).filter(|(_, f)| **f).map(|(s, _)| *s).sum();
        let weight: f32 = slots.iter().zip(&fixed).filter(|(_, f)| !**f).map(|((_, w), _)| *w).sum();
        if weight <= 0.0 {
            break;
        }
        let left = (extent - used).max(0.0);
        let mut clamped = false;
        for (i, (floor, w)) in slots.iter().enumerate() {
            if fixed[i] {
                continue;
            }
            let share = left * w / weight;
            if share < *floor {
                out[i] = *floor;
                fixed[i] = true;
                clamped = true;
            } else {
                out[i] = share;
            }
        }
        if !clamped {
            break;
        }
    }
    out
}

struct FlowRow {
    first: usize,
    end: usize,
    height: f32,
}

fn flow_rows(sizes: &[(f32, f32)], avail: f32, spacing: f32) -> Vec<FlowRow> {
    let mut rows = Vec::new();
    let mut first = 0;
    let mut x = 0.0f32;
    let mut height = 0.0f32;
    for (i, (w, h)) in sizes.iter().enumerate() {
        let next = if i > first { x + spacing + w } else { *w };
        if i > first && next > avail {
            rows.push(FlowRow { first, end: i, height });
            first = i;
            x = *w;
            height = *h;
        } else {
            x = next;
            height = height.max(*h);
        }
    }
    if first < sizes.len() {
        rows.push(FlowRow { first, end: sizes.len(), height });
    }
    rows
}

/// One free axis (spec "Placement"): returns (position, size).
/// `start`/`extent` = the frame the parent offers; `content` = the node's
/// minimum on this axis; `floor`/`cap` = min_*/max_*; `cap_fill` =
/// `max_*=fill`.
#[allow(clippy::too_many_arguments)]
fn free_axis(start: f32, extent: f32, len: Len, align: Align, m_near: Option<f32>, m_far: Option<f32>, content: f32, floor: f32, cap: Option<f32>, cap_fill: bool, frame_written: bool) -> (f32, f32) {
    let near = m_near.unwrap_or(0.0);
    let far = m_far.unwrap_or(0.0);
    let filled = extent - near - far;
    // An aligned axis with no size key hugs: alignment carves the default fill.
    let len = if len == Len::Unset && align != Align::AlignUnset { Len::Hug(0.0) } else { len };
    // Both edges named on an unaligned axis with NO size word: the box
    // spans them. A written size is never stretched by margins — an exact
    // number IS the box, and a hug box is as wide as its content; margins
    // are gaps around it (decided 2026-08-31,
    // docs/todo-gogo-parity.md/GPAR39: `width=hug, margin=8` pins 8 in
    // from the near edge, Godot's reading). Aligned (the toast:
    // `width=hug, max_width=fill, halign=center, margins`), the margins
    // only inset the measuring frame.
    if m_near.is_some() && m_far.is_some() && matches!(len, Len::Unset | Len::Fill(_)) && align == Align::AlignUnset {
        return (start + near, if frame_written { filled.max(floor) } else { filled.max(floor).max(content) });
    }
    // Both margins on an aligned axis inset the measuring frame ONCE —
    // the align arms below must then see them as spent, or a left-aligned
    // capped box sits at 2× its margin (GPAR38; the centered case was
    // already right, the inset frame's midpoint carries the (near-far)/2
    // shift the addon's cap.fill-margin-shift pins).
    let (start, extent, near, far) = if align != Align::AlignUnset && m_near.is_some() && m_far.is_some() { (start + near, filled, 0.0, 0.0) } else { (start, extent, near, far) };
    // An unwritten size (stretch, fill) takes the room it is given. When
    // that room comes from nowhere the author wrote (the viewport, a chain
    // of unwritten boxes) it is never smaller than its content — the same
    // rule a stack's fill child follows. When the room IS a number the
    // author wrote (an exact size or a cap up the chain), that number is
    // the answer: a written box is hard for itself and for what fills it
    // (decided 2026-08-30 after the toolbar's app bar, filling an 80-tall
    // strip, grew to its 150-tall content and carried its bottom-anchored
    // buttons off the screen). The written box clips what pokes past (draw).
    let mut size = match len {
        Len::Unset | Len::Fill(_) => if frame_written { filled } else { filled.max(content) },
        Len::Exact(n) => n,
        Len::Hug(_) => content,
    };
    let cap = if cap_fill { Some(filled) } else { cap };
    if let Some(cap) = cap {
        if !matches!(len, Len::Exact(_)) {
            size = size.min(cap);
        }
    }
    size = size.max(floor);
    if matches!(len, Len::Unset | Len::Fill(_)) && cap.is_none() {
        // A filled axis: inset by the margins, the unnamed edge bound at 0.
        return (start + near, size);
    }
    let pos = match align {
        Align::Start => start + near,
        Align::End => start + extent - far - size,
        Align::Center => start + (extent - size) / 2.0,
        Align::AlignUnset => {
            // A pin: the named edge moves, the other follows at size distance.
            if m_near.is_some() {
                start + near
            } else if m_far.is_some() {
                start + extent - far - size
            } else if matches!(len, Len::Fill(_) | Len::Unset) {
                // A capped fill with no side named: centered (WPF Stretch+Max).
                start + (extent - size) / 2.0
            } else {
                start
            }
        }
    };
    (pos, size)
}

/// Which axes a Scroll pans: `allow=` when present, else inferred from
/// the triad (a hug axis is content-sized, nothing to scroll). The one
/// rule for layout and input.
pub fn scroll_axes_of(page: &Page, res: &Resolved, id: NodeId) -> (bool, bool) {
        if page.nodes[id].kind == Kind::List {
            // A List self-scrolls on its layout's axis (D121/D128): a
            // `layout Row` strip pans sideways, everything else scrolls
            // down; statics pan with the rows either way.
            return if list_is_row(page, id) { (true, false) } else { (false, true) };
        }
        if let Some(allow) = res.ident(page, id, PropKey::Allow) {
            return match allow {
                "horizontal" => (true, false),
                "both" => (true, true),
                _ => (false, true),
            };
        }
        // D138: no `allow=` — the sizing triad infers. A hug axis is
        // content-sized, nothing left to scroll (disabled); a capped hug
        // scrolls past its cap; every other mode scrolls (GP36).
        let p = &res.placement[id];
        let sx = !matches!(p.width, Len::Hug(_)) || p.max_width.is_some();
        let sy = !matches!(p.height, Len::Hug(_)) || p.max_height.is_some();
        (sx, sy)
}

// ----- the retained road (B36-DES stage 3) --------------------------------

/// Can nothing under this box change its size? `true` for an exact width
/// AND height, or — on a Scroll — a scrolling axis with no cap (its
/// measure reports nothing on that axis; the content scrolls past). This
/// is where the retained road's walk up from a changed node STOPS: a
/// change under such a box moves nothing outside it, because every road
/// that places its siblings or its ancestors asks only `size_at` of the
/// box (an exact axis answers the number whatever the content) and the
/// box's own rect follows its own words and the frame its parent hands
/// it. A `fill` box under an exact free parent would hold too, but a
/// fill child of a STACK floors its slot at its content (`main_min`), so
/// the rule stays with exact and scrolling axes. What is not a box at
/// all — a `repeat` (its rows take the host's slots) and a Grid `row`
/// (the Grid places the cells) — never stops the walk, and neither does
/// a Grid CELL: a cell past the track count is never placed at all
/// (`place_grid` skips it), which only the Grid knows, so the walk goes
/// on to the Grid and the Grid places its cells again.
fn content_free(page: &Page, res: &Resolved, id: NodeId) -> bool {
    let node = &page.nodes[id];
    if matches!(node.kind, Kind::Repeat | Kind::GridRow) {
        return false;
    }
    let parent: NodeId = res.placed_parent(page, id);
    if parent != NO_NODE && parent.index() < page.nodes.len() && page.nodes[parent].kind == Kind::GridRow {
        return false;
    }
    let p = &res.placement[id];
    let (sx, sy) = if node.kind == Kind::Scroll { scroll_axes_of(page, res, id) } else { (false, false) };
    let free_w = matches!(p.width, Len::Exact(_)) || (sx && p.max_width.is_none());
    let free_h = matches!(p.height, Len::Exact(_)) || (sy && p.max_height.is_none());
    free_w && free_h
}

/// Is `id` laid out at all right now? A box that is not placed keeps the
/// zero rect, and placing its children into that zero frame would write
/// rects the full road never writes. Placed = the page root (placed by
/// `place_root` whatever its own visibility word — a page starts
/// `visibility=gone` and is shown by `on:show`; only a CHILD's word
/// keeps it out, through its parent's `visible_children`), or a child
/// that is not `gone` of a placed parent — and, under a `repeat`, a live
/// row (a pool slot not shown is never placed).
fn shown(page: &Page, res: &Resolved, id: NodeId) -> bool {
    let n = page.nodes.len();
    let mut a = id;
    loop {
        if a.index() >= n {
            return false;
        }
        let p = res.placed_parent(page, a);
        if p == NO_NODE || p.index() >= n {
            return true;
        }
        if res.gone(page, a) {
            return false;
        }
        if page.nodes[p].kind == Kind::Repeat && !res.children(page, p).any(|c| c == a) {
            return false;
        }
        a = p;
    }
}

/// The nodes in one list and not the other.
pub(crate) fn sym_diff_nodes(a: &[NodeId], b: &[NodeId], out: &mut Vec<NodeId>) {
    out.extend(a.iter().filter(|x| !b.contains(x)));
    out.extend(b.iter().filter(|x| !a.contains(x)));
}

/// The nodes whose entry differs between two per-node lists (`auto`,
/// `presence`): entry by entry when the lists are the same length (a
/// slider glide moves one slot's fraction per frame — only that slot),
/// every node of both otherwise.
pub(crate) fn diff_pairs<T: PartialEq>(old: &[(NodeId, T)], new: &[(NodeId, T)], out: &mut Vec<NodeId>) {
    if old.len() == new.len() {
        for (a, b) in old.iter().zip(new) {
            if a.0 != b.0 || a.1 != b.1 {
                out.push(a.0);
                out.push(b.0);
            }
        }
    } else {
        out.extend(old.iter().chain(new).map(|(node, _)| *node));
    }
}

/// Do two override lists answer differently for any key `counts`? A
/// lookup takes the LAST entry for a key (`Resolved::override_of`), so
/// only that one counts per key.
fn keys_differ(old: &[(NodeId, Prop)], new: &[(NodeId, Prop)], counts: impl Fn(PropKey) -> bool) -> bool {
    let last = |list: &[(NodeId, Prop)], key: PropKey| list.iter().rev().find(|(_, p)| p.key == key).map(|(_, p)| p.value);
    old.iter().chain(new).any(|(_, p)| counts(p.key) && last(old, p.key) != last(new, p.key))
}

/// The keys a node's DESCENDANTS read off it (`Resolved::inherited`: a
/// Label takes the nearest ancestor's font words). A change on one of
/// these changes the subtree's picture, not only the node's own.
fn inherits(key: PropKey) -> bool {
    matches!(key, PropKey::Font | PropKey::FontSize | PropKey::FontColor)
}

/// A kept copy of the look's per-node tables — the placement and the
/// override list with its spans — and the compare that finds which
/// nodes changed since. Needed because the retained roads (layout, draw)
/// re-run only what changed, and comparing the tables themselves is what
/// makes that complete: every value layout and draw read is in here, so
/// nothing a resolve moves can slip by unmarked. The one change a
/// compare cannot see — a string rewritten in place — the runtime
/// reports (`ResolveChanges::rewritten`).
#[derive(Default)]
pub(crate) struct LookCopy {
    placement: Vec<Placement>,
    overrides: Vec<(NodeId, Prop)>,
    override_span: Vec<(u32, u32)>,
    override_tail: u32,
    /// Scratch for the tail compare: entry indices sorted by node, and
    /// one node's entries from each side.
    old_idx: Vec<u32>,
    new_idx: Vec<u32>,
    old_group: Vec<(NodeId, Prop)>,
    new_group: Vec<(NodeId, Prop)>,
}

impl LookCopy {
    pub(crate) fn take(&mut self, res: &Resolved) {
        self.placement.clone_from(&res.placement);
        self.overrides.clone_from(&res.overrides);
        self.override_span.clone_from(&res.override_span);
        self.override_tail = res.override_tail;
    }

    /// Whether the copy is of a page with `n` nodes.
    pub(crate) fn fits(&self, n: usize) -> bool {
        self.placement.len() == n && self.override_span.len() == n
    }

    /// Pushes onto `out` every node whose placement or effective override
    /// values differ between the copy and `res`. `layout_only`: an
    /// override difference counts only on a key layout reads
    /// (`layout_reads`) — a `when held background=` press marks nothing.
    /// `inherited`, when given, takes the nodes whose font words changed
    /// (`inherits`) — their whole subtree reads them. The ladder part is
    /// compared per node (its span); the tail (writes, tweens, anim lanes
    /// — no node order) is compared as a list and, when it differs, per
    /// node with each node's entries in their order: a node's lookups
    /// depend only on its own entries' order, and that order is fixed by
    /// the phases the tail is pushed in.
    pub(crate) fn diff(&mut self, res: &Resolved, layout_only: bool, out: &mut Vec<NodeId>, mut inherited: Option<&mut Vec<NodeId>>) {
        let n = self.placement.len().min(res.placement.len());
        for i in 0..n {
            let id = NodeId::from_index(i);
            if self.placement[i] != res.placement[i] {
                out.push(id);
                continue;
            }
            let (os, ol) = self.override_span[i];
            let (ns, nl) = res.override_span[i];
            let old = &self.overrides[os as usize..(os + ol) as usize];
            let new = &res.overrides[ns as usize..(ns + nl) as usize];
            if old != new {
                if !layout_only || keys_differ(old, new, layout_reads) {
                    out.push(id);
                }
                if let Some(inh) = inherited.as_deref_mut() {
                    if keys_differ(old, new, inherits) {
                        inh.push(id);
                    }
                }
            }
        }
        let old_tail = &self.overrides[self.override_tail as usize..];
        let new_tail = &res.overrides[res.override_tail as usize..];
        if old_tail == new_tail {
            return;
        }
        self.old_idx.clear();
        self.old_idx.extend(0..old_tail.len() as u32);
        self.old_idx.sort_by_key(|i| old_tail[*i as usize].0);
        self.new_idx.clear();
        self.new_idx.extend(0..new_tail.len() as u32);
        self.new_idx.sort_by_key(|i| new_tail[*i as usize].0);
        let (mut a, mut b) = (0usize, 0usize);
        loop {
            let na: Option<NodeId> = self.old_idx.get(a).map(|i| old_tail[*i as usize].0);
            let nb: Option<NodeId> = self.new_idx.get(b).map(|i| new_tail[*i as usize].0);
            let node: NodeId = match (na, nb) {
                (Some(x), Some(y)) => x.min(y),
                (Some(x), None) => x,
                (None, Some(y)) => y,
                (None, None) => break,
            };
            self.old_group.clear();
            while a < self.old_idx.len() && old_tail[self.old_idx[a] as usize].0 == node {
                self.old_group.push(old_tail[self.old_idx[a] as usize]);
                a += 1;
            }
            self.new_group.clear();
            while b < self.new_idx.len() && new_tail[self.new_idx[b] as usize].0 == node {
                self.new_group.push(new_tail[self.new_idx[b] as usize]);
                b += 1;
            }
            if self.old_group != self.new_group {
                if !layout_only || keys_differ(&self.old_group, &self.new_group, layout_reads) {
                    out.push(node);
                }
                if let Some(inh) = inherited.as_deref_mut() {
                    if keys_differ(&self.old_group, &self.new_group, inherits) {
                        inh.push(node);
                    }
                }
            }
        }
    }
}

/// What the last `LayoutCache::run` did — the equivalence pin reads it
/// to prove a change stayed local.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutStats {
    /// The whole page was laid out again.
    pub full: bool,
    /// Measures that ran the long way (`size_at_uncached`); 0 when the
    /// change touched no layout word.
    pub measured: u32,
    /// Boxes whose children were placed again.
    pub roots: u32,
}

/// The retained layout (B36-DES stage 3): the last answer kept between
/// frames — the placed rects, the shown rects, the `written` table and
/// every node's measures — and re-laid only where the look changed.
///
/// The rule: a node whose layout words changed (`LookCopy::diff` over
/// the keys `layout_reads`, a visibility fold change, a driver fraction,
/// a rewritten string) dirties itself; the walk up its PLACED ancestry
/// stops at the first clean box that `content_free` — that box's rect
/// and measure cannot have changed, so its children are placed again
/// from the rect it kept (`place_children`), the subtree's measures
/// forgotten first, and the shown rects derived for the subtree. A
/// Scroll whose offset moved re-places its children from the same rect
/// with every measure kept — an offset is read by placement only. If a
/// walk reaches the root the whole page is laid out again, as it is on
/// the first run, another viewport or measure (`TextMeasure::identity`),
/// a node-count, seat, repeat-row, theme, locale or inset change, or a
/// resolve the cache did not see (`ResolveChanges::serial`).
pub struct LayoutCache {
    on: bool,
    valid: bool,
    seen_serial: u64,
    viewport: Rect,
    identity: u64,
    variant: StrId,
    locale: Option<Arc<Locale>>,
    safe: [f32; 4],
    seat: Vec<NodeId>,
    repeat_rows: Vec<(NodeId, Vec<NodeId>)>,
    scroll: Vec<[f32; 2]>,
    hiding: Vec<NodeId>,
    presence: Vec<(NodeId, bool)>,
    auto: Vec<(NodeId, f32)>,
    look: LookCopy,
    /// Per node: the rect as placed (`Lay::rects`).
    placed: Vec<Rect>,
    /// Per node: the rect shown (`Lay::reveal` of `placed`) — the answer.
    rects: Vec<Rect>,
    written: Vec<[bool; 2]>,
    memo: Vec<Vec<Memo>>,
    // Scratch, kept so a steady frame allocates nothing.
    changed: Vec<NodeId>,
    dirty: Vec<bool>,
    roots: Vec<(NodeId, bool)>,
    stack: Vec<NodeId>,
    stats: LayoutStats,
}

impl Default for LayoutCache {
    fn default() -> LayoutCache {
        LayoutCache {
            on: true,
            valid: false,
            seen_serial: 0,
            viewport: Rect::ZERO,
            identity: 0,
            variant: NO_STR,
            locale: None,
            safe: [0.0; 4],
            seat: Vec::new(),
            repeat_rows: Vec::new(),
            scroll: Vec::new(),
            hiding: Vec::new(),
            presence: Vec::new(),
            auto: Vec::new(),
            look: LookCopy::default(),
            placed: Vec::new(),
            rects: Vec::new(),
            written: Vec::new(),
            memo: Vec::new(),
            changed: Vec::new(),
            dirty: Vec::new(),
            roots: Vec::new(),
            stack: Vec::new(),
            stats: LayoutStats::default(),
        }
    }
}

impl LayoutCache {
    /// The switch: off = every run is the full road (for the equivalence
    /// pin and for bisecting a layout that differs between the roads).
    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        self.valid = false;
    }

    pub fn stats(&self) -> LayoutStats {
        self.stats
    }

    /// The shown rects for `res` in `viewport` — the same answer `layout`
    /// gives, re-laid only where `res` differs from what the last run
    /// saw. `changes` is the runtime's report for `res`.
    pub fn run(&mut self, page: &Page, res: &Resolved, changes: &ResolveChanges, viewport: Rect, text: &dyn TextMeasure) -> &[Rect] {
        let n = page.nodes.len();
        let same_locale: bool = match (&self.locale, &res.locale) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        let seen_resolve: bool = changes.serial == self.seen_serial || changes.serial == self.seen_serial + 1;
        let full: bool = !self.on
            || !self.valid
            || changes.full
            || !seen_resolve
            || !self.look.fits(n)
            || res.placement.len() != n
            || res.scroll.len() != n
            || res.seat.len() != n
            || self.placed.len() != n
            || viewport != self.viewport
            || text.identity() != self.identity
            || res.variant != self.variant
            || !same_locale
            || res.safe != self.safe
            || res.seat != self.seat
            || res.repeat_rows != self.repeat_rows;
        if full {
            self.full(page, res, changes, viewport, text);
        } else {
            self.patch(page, res, changes, viewport, text);
        }
        &self.rects
    }

    fn full(&mut self, page: &Page, res: &Resolved, changes: &ResolveChanges, viewport: Rect, text: &dyn TextMeasure) {
        let n = page.nodes.len();
        self.placed.clear();
        self.placed.resize(n, Rect::ZERO);
        self.written.clear();
        self.written.resize(n, [false; 2]);
        self.memo.resize_with(n, Vec::new);
        for m in &mut self.memo {
            m.clear();
        }
        let mut lay = Lay::new(page, res, text, std::mem::take(&mut self.placed), std::mem::take(&mut self.written), std::mem::take(&mut self.memo));
        lay.place_root(viewport);
        self.rects.clear();
        self.rects.extend(page.node_ids().map(|id| lay.reveal(id, lay.rects[id])));
        self.stats = LayoutStats { full: true, measured: lay.measured.get(), roots: 1 };
        self.placed = lay.rects;
        self.written = lay.written;
        self.memo = lay.memo.into_inner();
        self.remember(res, changes, viewport, text);
        self.valid = true;
    }

    /// What the next run compares against.
    fn remember(&mut self, res: &Resolved, changes: &ResolveChanges, viewport: Rect, text: &dyn TextMeasure) {
        self.seen_serial = changes.serial;
        self.viewport = viewport;
        self.identity = text.identity();
        self.variant = res.variant;
        self.locale.clone_from(&res.locale);
        self.safe = res.safe;
        self.seat.clone_from(&res.seat);
        self.repeat_rows.clone_from(&res.repeat_rows);
        self.scroll.clone_from(&res.scroll);
        self.hiding.clone_from(&res.hiding);
        self.presence.clone_from(&res.presence);
        self.auto.clone_from(&res.auto);
        self.look.take(res);
    }

    fn patch(&mut self, page: &Page, res: &Resolved, changes: &ResolveChanges, viewport: Rect, text: &dyn TextMeasure) {
        let n = page.nodes.len();
        // 1. The nodes whose layout inputs changed.
        let mut changed: Vec<NodeId> = std::mem::take(&mut self.changed);
        changed.clear();
        self.look.diff(res, true, &mut changed, None);
        sym_diff_nodes(&self.hiding, &res.hiding, &mut changed);
        diff_pairs(&self.presence, &res.presence, &mut changed);
        diff_pairs(&self.auto, &res.auto, &mut changed);
        if changes.serial == self.seen_serial + 1 {
            changed.extend_from_slice(&changes.rewritten);
        }
        // A Scroll whose offset moved (against the table the last run
        // laid out by) re-places its own children; one that is a Grid
        // cell is re-placed by its Grid like any changed node
        // (`content_free` on cells).
        let mut roots: Vec<(NodeId, bool)> = std::mem::take(&mut self.roots);
        roots.clear();
        for i in 0..n {
            if self.scroll[i] != res.scroll[i] {
                let id = NodeId::from_index(i);
                let parent: NodeId = res.placed_parent(page, id);
                if parent != NO_NODE && parent.index() < n && page.nodes[parent].kind == Kind::GridRow {
                    changed.push(id);
                } else {
                    roots.push((id, false));
                }
            }
        }
        changed.retain(|c| c.index() < n);
        changed.sort();
        changed.dedup();
        if changed.is_empty() && roots.is_empty() {
            // Nothing layout reads moved: the tables equal the copies,
            // so there is nothing to copy either.
            self.stats = LayoutStats { full: false, measured: 0, roots: 0 };
            self.seen_serial = changes.serial;
            self.changed = changed;
            self.roots = roots;
            return;
        }
        // 2. Each changed node dirties itself; the walk up finds the box
        // to re-place from. Reaching the root = the whole page.
        let mut dirty: Vec<bool> = std::mem::take(&mut self.dirty);
        dirty.clear();
        dirty.resize(n, false);
        for c in &changed {
            dirty[c.index()] = true;
        }
        for c in &changed {
            let mut a: NodeId = res.placed_parent(page, *c);
            loop {
                if a == NO_NODE || a.index() >= n {
                    self.changed = changed;
                    self.dirty = dirty;
                    self.roots = roots;
                    self.full(page, res, changes, viewport, text);
                    return;
                }
                if !dirty[a.index()] && content_free(page, res, a) {
                    roots.push((a, true));
                    break;
                }
                a = res.placed_parent(page, a);
            }
        }
        // 3. One root per box (a box named for a scroll move and for a
        // change keeps the re-measure), none under another — the box
        // above re-places it anyway.
        roots.sort_by(|x, y| x.0.cmp(&y.0).then(y.1.cmp(&x.1)));
        roots.dedup_by_key(|r| r.0);
        let mut i = 0;
        while i < roots.len() {
            let r: NodeId = roots[i].0;
            let covered: bool = roots.iter().any(|(q, _)| *q != r && res.placed_is_under(page, r, *q));
            if covered {
                roots.remove(i);
            } else {
                i += 1;
            }
        }
        // 4. Forget the re-measured subtrees: their measures and rects
        // (a child gone now keeps the zero rect the full road gives it).
        // Every placed child, the repeat slots not shown too
        // (`PlacedTree::all_children`): a slot that left keeps no rect.
        self.memo.resize_with(n, Vec::new);
        for k in 0..roots.len() {
            let (r, remeasure) = roots[k];
            if !remeasure {
                continue;
            }
            self.stack.clear();
            self.stack.extend(res.tree.all_children(r));
            while let Some(d) = self.stack.pop() {
                self.memo[d.index()].clear();
                self.placed[d.index()] = Rect::ZERO;
                self.rects[d.index()] = Rect::ZERO;
                self.stack.extend(res.tree.all_children(d));
            }
        }
        // 5. Place each root's children again from the rect it kept, and
        // derive the subtree's shown rects.
        let mut lay = Lay::new(page, res, text, std::mem::take(&mut self.placed), std::mem::take(&mut self.written), std::mem::take(&mut self.memo));
        for k in 0..roots.len() {
            let r: NodeId = roots[k].0;
            if !shown(page, res, r) {
                continue;
            }
            let frame: Rect = lay.rects[r];
            lay.place_children(r, frame);
            self.stack.clear();
            self.stack.extend(res.tree.all_children(r));
            while let Some(d) = self.stack.pop() {
                self.rects[d.index()] = lay.reveal(d, lay.rects[d]);
                self.stack.extend(res.tree.all_children(d));
            }
        }
        self.stats = LayoutStats { full: false, measured: lay.measured.get(), roots: roots.len() as u32 };
        self.placed = lay.rects;
        self.written = lay.written;
        self.memo = lay.memo.into_inner();
        self.changed = changed;
        self.dirty = dirty;
        self.roots = roots;
        self.remember(res, changes, viewport, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;
    use crate::runtime::{Runtime, resolve};

    fn lay(src: &str, w: f32, h: f32) -> (Page, Vec<Rect>) {
        let c = compile(src, &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let rt = Runtime::new(&c.page);
        let res = resolve(&c.page, &rt);
        let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, w, h), &FixedAdvance);
        (c.page, rects)
    }

    fn rect(p: &Page, r: &[Rect], name: &str) -> Rect {
        r[p.find(name).unwrap().index()]
    }

    #[test]
    fn view_children_place_freely() {
        let (p, r) = lay("View v {\n Rectangle bg width=fill, height=fill\n Panel toast halign=center, valign=top, margin_top=20, width=200, height=40\n Button fab 44x44, halign=right, valign=bottom, margin=16\n Rectangle strip height=10, margin_bottom=-10\n Panel span margin_left=40, margin_right=40, valign=center, height=100\n}\n", 400.0, 800.0);
        assert_eq!(rect(&p, &r, "bg"), Rect::new(0.0, 0.0, 400.0, 800.0));
        assert_eq!(rect(&p, &r, "toast"), Rect::new(100.0, 20.0, 200.0, 40.0));
        assert_eq!(rect(&p, &r, "fab"), Rect::new(340.0, 740.0, 44.0, 44.0));
        // A negative pin on a hug/exact axis hangs past the parent.
        assert_eq!(rect(&p, &r, "strip"), Rect::new(0.0, 800.0, 400.0, 10.0));
        assert_eq!(rect(&p, &r, "span"), Rect::new(40.0, 350.0, 320.0, 100.0));
    }

    #[test]
    fn column_slots_spacing_gap_and_weighted_fill() {
        let (p, r) = lay("View v {\n Column col spacing=8 {\n  Label title text=\"Brushes\", font_size=20\n  gap 12\n  Rectangle a height=fill\n  Rectangle b height=fill(2)\n  Button ok halign=center, text=\"OK\", min_width=120\n }\n}\n", 300.0, 500.0);
        let title = rect(&p, &r, "title");
        assert_eq!((title.x, title.y, title.w), (0.0, 0.0, 300.0));
        assert_eq!(title.h, 25.0);
        // Used: 25 + 12 + 0 + 0 + ok(6+18.75+6=30.75) + 4 spacings = 32 → 99.75; leftover 400.25 → a 133.42, b 266.83.
        let a = rect(&p, &r, "a");
        let b = rect(&p, &r, "b");
        assert!((a.h * 2.0 - b.h).abs() < 0.01, "{a:?} {b:?}");
        assert!((a.y - (25.0 + 8.0 + 12.0 + 8.0)).abs() < 0.01);
        let ok = rect(&p, &r, "ok");
        assert_eq!(ok.w, 120.0);
        assert_eq!(ok.x, 90.0);
        assert!((ok.bottom() - 500.0).abs() < 0.01);
    }

    #[test]
    fn row_gap_fill_pushes_to_the_far_edge() {
        let (p, r) = lay("View v {\n Row row height=40 {\n  Label caption text=\"Size\"\n  gap fill\n  Button close text=\"X\"\n }\n}\n", 400.0, 100.0);
        let close = rect(&p, &r, "close");
        assert!((close.right() - 400.0).abs() < 0.01, "{close:?}");
        let caption = rect(&p, &r, "caption");
        assert_eq!(caption.x, 0.0);
        assert_eq!(caption.h, 40.0);
    }

    #[test]
    fn panel_padding_border_and_layers_minimum() {
        let (p, r) = lay("View v {\n Panel card padding=12, border_thickness=2, halign=left, valign=top, width=hug, height=hug {\n  Layers stack {\n   Rectangle big 100x50\n   Rectangle small 20x20, halign=right, valign=bottom\n  }\n }\n}\n", 400.0, 400.0);
        assert_eq!(rect(&p, &r, "card"), Rect::new(0.0, 0.0, 128.0, 78.0));
        assert_eq!(rect(&p, &r, "stack"), Rect::new(14.0, 14.0, 100.0, 50.0));
        assert_eq!(rect(&p, &r, "small"), Rect::new(94.0, 44.0, 20.0, 20.0));
    }

    #[test]
    fn capped_fill_centers_and_hug_caps_wrap_text() {
        let (p, r) = lay("View v {\n Column catalog width=fill, max_width=380, halign=center { Rectangle x height=10 }\n Label toast text=\"one two three four five six\", font_size=10, wrap=true, width=hug, max_width=fill, halign=center, valign=bottom, margin_left=150, margin_right=150\n}\n", 600.0, 400.0);
        assert_eq!(rect(&p, &r, "catalog"), Rect::new(110.0, 0.0, 380.0, 400.0));
        let toast = rect(&p, &r, "toast");
        // 26 chars × 5.5 = 143 unwrapped; frame 600 − 300 = 300 binds nothing → one line.
        assert_eq!(toast.h, 12.5);
        assert!((toast.x - (600.0 - toast.w) / 2.0).abs() < 0.01);
    }

    /// PR2: fills share what the fixed slots leave — `fill(2)`/`fill(3)`
    /// is 2:5 of the room no matter what the children hold — and a
    /// content-bearing fill still gets an equal share (the app bars).
    #[test]
    fn fills_share_the_room_by_weight_not_by_content() {
        let (p, r) = lay("View v {\n Column col spacing=0 {\n  Panel a height=fill(2) { Rectangle x height=300 }\n  Panel b height=fill(3) { Rectangle y height=10 }\n }\n Row bar valign=bottom, height=50 {\n  Panel back width=fill { Rectangle z 10x10, halign=left }\n  Rectangle mid width=160\n  Panel dots width=fill { Rectangle q 50x10, halign=right }\n }\n}\n", 360.0, 1000.0);
        assert_eq!(rect(&p, &r, "a").h, 400.0);
        assert_eq!(rect(&p, &r, "b").h, 600.0);
        assert_eq!(rect(&p, &r, "back").w, 100.0);
        assert_eq!(rect(&p, &r, "mid").x, 100.0);
        assert_eq!(rect(&p, &r, "dots").w, 100.0);
    }

    /// A parent that asks its children how wide to be: the fill label
    /// answers with its text, so the menu column grows to fit "Music
    /// volume" + slider instead of squeezing the label to a sliver
    /// (test.gogo).
    #[test]
    fn a_parent_asking_its_children_hears_a_fill_childs_content() {
        let (p, r) = lay("View v {
 Column menu halign=center, valign=center, spacing=8 {
  Button b text=\"New Game\", width=220, height=48
  Row row spacing=12 {
   Label vol text=\"Music volume\", font_size=20, width=fill
   Rectangle s width=200, height=20
  }
 }
}
", 720.0, 800.0);
        // "Music volume" = 12 chars × 11 = 132; row = 132 + 12 + 200 = 344.
        assert_eq!(rect(&p, &r, "menu").w, 344.0);
        assert_eq!(rect(&p, &r, "vol").w, 132.0);
        assert_eq!(rect(&p, &r, "s").x, (720.0 - 344.0) / 2.0 + 144.0);
    }

    /// A `repeat` with no items takes no slot and no spacing in a stack.
    #[test]
    fn an_empty_repeat_takes_no_slot() {
        let (p, r) = lay("View v {
 Column c spacing=4, valign=top {
  Label a text=\"Tasks\", font_size=10
  repeat tasks by $item.id { Panel { Label text=$item.title } }
  Label b text=\"Next\", font_size=10
 }
}
", 300.0, 300.0);
        assert_eq!(rect(&p, &r, "b").y, 12.5 + 4.0);
    }

    /// A fill under its `min_` floor takes the floor; the others re-share.
    #[test]
    fn a_fill_under_its_floor_takes_the_floor() {
        let (p, r) = lay("View v {\n Row row height=20 {\n  Rectangle a width=fill, min_width=150\n  Rectangle b width=fill\n  Rectangle c width=fill\n }\n}\n", 300.0, 100.0);
        assert_eq!(rect(&p, &r, "a").w, 150.0);
        assert_eq!(rect(&p, &r, "b").w, 75.0);
        assert_eq!(rect(&p, &r, "c").x, 225.0);
    }

    /// PR1: a wrapping fill-width label bends to what the button leaves
    /// instead of pushing the button off the edge (ad_plate).
    #[test]
    fn a_wrapping_fill_label_leaves_room_for_the_button() {
        let (p, r) = lay("View v {\n Row plate height=50 {\n  Label info text=\"one two three four five six seven eight nine ten eleven twelve\", font_size=10, width=fill, wrap=true\n  Button buy text=\"buy\", width=60\n }\n}\n", 200.0, 100.0);
        assert_eq!(rect(&p, &r, "info").w, 140.0);
        assert_eq!(rect(&p, &r, "buy"), Rect::new(140.0, 0.0, 60.0, 50.0));
        assert!(rect(&p, &r, "info").h > 12.5, "wrapped to more than one line");
    }

    /// PR6: a horizontal Scroll whose content is wider keeps the bottom
    /// bar's lane out of the content.
    #[test]
    fn a_horizontal_scroll_keeps_the_bottom_bar_lane() {
        let (p, r) = lay("View v {\n Scroll strip allow=horizontal, height=60, valign=top {\n  Row content { Rectangle a 150x40; Rectangle b 150x40 }\n }\n}\n", 200.0, 300.0);
        assert_eq!(rect(&p, &r, "content").h, 56.0);
        assert_eq!(rect(&p, &r, "content").w, 300.0);
    }

    #[test]
    fn scroll_content_takes_the_viewport_and_scrolls_past() {
        let (p, r) = lay("View v {\n Scroll list height=100 {\n  Column content { Rectangle a height=80; Rectangle b height=80 }\n }\n}\n", 200.0, 300.0);
        assert_eq!(rect(&p, &r, "list"), Rect::new(0.0, 0.0, 200.0, 100.0));
        // The auto scroll bar keeps its 4-epx column out of the content.
        assert_eq!(rect(&p, &r, "content"), Rect::new(0.0, 0.0, 196.0, 160.0));
        assert_eq!(rect(&p, &r, "b").y, 80.0);
    }

    #[test]
    fn grid_tracks_and_spans() {
        let (p, r) = lay("View v {\n Grid g cols=[hug, fill, 60], spacing=4, halign=left, valign=top, width=300, height=hug {\n  row { Label lbl text=\"Size\", font_size=10; Rectangle s height=20; Label val text=\"9\", font_size=10 }\n  row height=30 { Rectangle preview col_span=3 }\n }\n}\n", 400.0, 400.0);
        // hug track = "Size" 22 wide; fill = 300 − 22 − 60 − 8 = 210.
        assert_eq!(rect(&p, &r, "lbl").w, 22.0);
        assert_eq!(rect(&p, &r, "s"), Rect::new(26.0, 0.0, 210.0, 20.0));
        assert_eq!(rect(&p, &r, "val").x, 240.0);
        let preview = rect(&p, &r, "preview");
        assert_eq!(preview, Rect::new(0.0, 24.0, 300.0, 30.0));
        assert_eq!(rect(&p, &r, "g").h, 54.0);
    }

    #[test]
    fn flow_wraps_and_gone_leaves_layout() {
        let (p, r) = lay("View v {\n Flow f spacing=10, halign=left, valign=top, width=100, height=hug {\n  Rectangle a 40x20; Rectangle b 40x20; Rectangle c 40x20, visibility=gone; Rectangle d 40x20\n }\n}\n", 400.0, 400.0);
        assert_eq!(rect(&p, &r, "a"), Rect::new(0.0, 0.0, 40.0, 20.0));
        assert_eq!(rect(&p, &r, "b"), Rect::new(50.0, 0.0, 40.0, 20.0));
        assert_eq!(rect(&p, &r, "c"), Rect::ZERO);
        assert_eq!(rect(&p, &r, "d"), Rect::new(0.0, 30.0, 40.0, 20.0));
        assert_eq!(rect(&p, &r, "f").h, 50.0);
    }

    #[test]
    fn arrangement_pose_resizes_and_restores() {
        let src = "arrangement wide\nwhen wide { bar.width = 300 }\nView v { Panel bar width=100, height=20, halign=left, valign=top }\n";
        let c = compile(src, &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let mut rt = Runtime::new(&c.page);
        let res = resolve(&c.page, &rt);
        let r = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        assert_eq!(rect(&c.page, &r, "bar").w, 100.0);
        rt.go_id(ArrangementId::new(0));
        let res = resolve(&c.page, &rt);
        let r = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        assert_eq!(rect(&c.page, &r, "bar").w, 300.0);
        rt.go_id(crate::runtime::BASE);
        let res = resolve(&c.page, &rt);
        let r = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        assert_eq!(rect(&c.page, &r, "bar").w, 100.0);
    }
}

/// Visible nodes laid out WHOLLY outside `viewport` that the file did not
/// park there on purpose — the topmost such node of each subtree, so the
/// list stays short. Needed because a layout rule that grows a box can
/// shove its bottom- or right-anchored children past the screen edge while
/// a pixel diff of a dark page barely moves (the toolbar vanished and the
/// parity mean read 0.26 %, user-caught 2026-08-30): the parity run counts
/// these per page and a count that grew is a failure. "On purpose" = the
/// node left through a side it hangs past by a NEGATIVE margin (the
/// fullscreen tile parked at `margin_right=-80`); its subtree is parked
/// with it. Gone/hidden subtrees and a Scroll's content (meant to run past
/// the viewport) never count.
/// A List whose rows sit in a `layout Row` strip — the one form that
/// pans sideways (D128). The strip is the Row child holding the repeat.
fn list_is_row(page: &Page, id: NodeId) -> bool {
    page.children(id).any(|c| page.nodes[c].kind == Kind::Row && page.children(c).any(|g| page.nodes[g].kind == Kind::Repeat))
}

/// The D123 no-height nudge (the addon's runtime `starved` flag): the
/// viewport Lists whose rows got less than one row's room — usually a
/// hug ancestor starved one (a hug parent asks for minimums, and a
/// viewport's minimum is its fixed content only). The fix is the
/// author's: `height=` (a sized box) or `height=hug` (the content
/// shape) — hug lists are content-sized on purpose and never starve.
pub fn starved(page: &Page, f: Frame<'_>, rt: &crate::runtime::Runtime) -> Vec<NodeId> {
    let (res, rects): (&Resolved, &[Rect]) = (f.look(), f.rects());
    let mut out = Vec::new();
    for i in 0..page.nodes.len() {
        let id = NodeId::from_index(i);
        if page.nodes[i].kind != Kind::List || res.gone(page, id) || res.hidden(page, id) {
            continue;
        }
        let hugged = if list_is_row(page, id) { matches!(res.placement[i].width, Len::Hug(_)) } else { matches!(res.placement[i].height, Len::Hug(_)) };
        if hugged {
            continue;
        }
        // Statics' laid heights come off the box; a row's rect keeps its
        // natural stacked height (the content frame never squeezes it),
        // so the first row IS one row's ask.
        let rows = rt.list_rows(page, id);
        let mut fixed = 0.0f32;
        for c in res.children(page, id) {
            let n = &page.nodes[c];
            if matches!(n.kind, Kind::Repeat | Kind::ListItem) || res.gone(page, c) {
                continue;
            }
            fixed += rects[c].h;
        }
        let room = rects[i].h - fixed;
        let ask = rows.first().map(|r| rects[*r].h).unwrap_or(1.0).max(1.0);
        if room + 0.01 < ask {
            out.push(id);
        }
    }
    out
}

/// The nodes laid out fully outside `viewport` on frame `f` (a debug walk).
pub fn offscreen(page: &Page, f: Frame<'_>, viewport: Rect) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = Vec::new();
    if !page.nodes.is_empty() {
        offscreen_walk(page, f.look(), f.rects(), viewport, NodeId::new(0), &mut out);
    }
    out
}

fn offscreen_walk(page: &Page, res: &Resolved, rects: &[Rect], vp: Rect, id: NodeId, out: &mut Vec<NodeId>) {
    if res.gone(page, id) || res.hidden(page, id) {
        return;
    }
    let r = rects[id];
    if r.w > 0.0 && r.h > 0.0 {
        let m = res.placement[id].margin;
        let neg = |m: Option<f32>| m.map_or(false, |v| v < 0.0);
        let left = r.x + r.w <= vp.x;
        let right = r.x >= vp.x + vp.w;
        let top = r.y + r.h <= vp.y;
        let bottom = r.y >= vp.y + vp.h;
        if left || right || top || bottom {
            let parked = (left && neg(m[0])) || (right && neg(m[2])) || (top && neg(m[1])) || (bottom && neg(m[3]));
            if !parked {
                out.push(id);
            }
            // The subtree follows its topmost box either way.
            return;
        }
    }
    if page.nodes[id].kind == Kind::Scroll {
        return;
    }
    for c in page.children(id) {
        offscreen_walk(page, res, rects, vp, c as NodeId, out);
    }
}
