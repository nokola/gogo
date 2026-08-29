//! The layout pass: `layout(page, resolved, viewport, text) -> Vec<Rect>`.
//! Pure — nothing is written back into the page. Two walks: minimums
//! bottom-up (`min_size`), placement top-down (`place`). Semantics follow
//! the spec's Placement section: the sizing triad, alignment as position
//! only, margins as inset / offset / pin, `spacing`/`gap` in stacks,
//! weighted fills, Layers minimum = biggest child, Scroll content takes
//! the viewport.

use crate::page::*;
use crate::runtime::Resolved;

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
    /// `max_width` when given.
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32);
}

/// A fixed-advance fake for tests and headless layout: 0.55 em per char,
/// 1.25 em line height, word wrap.
pub struct FixedAdvance;

impl TextMeasure for FixedAdvance {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32) {
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
pub const INPUT_HEIGHT: f32 = 36.0;
pub const CHECK_SIZE: (f32, f32) = (48.0, 32.0);
pub const ICON_SIZE: f32 = 24.0;

struct Lay<'a> {
    page: &'a Page,
    res: &'a Resolved,
    text: &'a dyn TextMeasure,
    rects: Vec<Rect>,
}

pub fn layout(page: &Page, res: &Resolved, viewport: Rect, text: &dyn TextMeasure) -> Vec<Rect> {
    let mut lay = Lay { page, res, text, rects: vec![Rect::ZERO; page.nodes.len()] };
    if !page.nodes.is_empty() {
        lay.place(0, viewport, true);
    }
    lay.rects
}

/// Which axis a stack runs on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl<'a> Lay<'a> {
    fn node(&self, id: NodeId) -> &Node {
        &self.page.nodes[id as usize]
    }

    fn placement(&self, id: NodeId) -> &Placement {
        &self.res.placement[id as usize]
    }

    fn gone(&self, id: NodeId) -> bool {
        self.res.gone(self.page, id)
    }

    fn visible_children(&self, id: NodeId) -> Vec<NodeId> {
        self.page.children(id).map(|i| i as NodeId).filter(|c| !self.gone(*c)).collect()
    }

    /// Padding plus border, per edge (left, top, right, bottom).
    fn inner_inset(&self, id: NodeId) -> [f32; 4] {
        let mut pad = self.placement(id).padding;
        let kind = self.node(id).kind;
        if kind == Kind::Button && pad == [0.0; 4] {
            pad = BUTTON_PADDING;
        }
        let border = if matches!(kind, Kind::Panel | Kind::Button | Kind::View) { self.res.num(self.page, id, PropKey::BorderThickness).unwrap_or(0.0) } else { 0.0 };
        [pad[0] + border, pad[1] + border, pad[2] + border, pad[3] + border]
    }

    fn font_size(&self, id: NodeId) -> f32 {
        self.res.num(self.page, id, PropKey::FontSize).unwrap_or(DEFAULT_FONT_SIZE)
    }

    fn text_of(&self, id: NodeId) -> Option<&'a str> {
        match self.res.prop(self.page, id, PropKey::Text)? {
            Value::VText { text, .. } => Some(self.page.str(text)),
            _ => None,
        }
    }

    fn spacing(&self, id: NodeId) -> f32 {
        self.res.num(self.page, id, PropKey::Spacing).unwrap_or(0.0)
    }

    // ----- minimums ----------------------------------------------------------

    /// The node's minimum box (its own size under `hug`), given the width
    /// its parent can offer for wrapping (`None` = unbounded).
    fn min_size(&self, id: NodeId, avail_w: Option<f32>) -> (f32, f32) {
        let p = self.placement(id);
        let (cw, ch) = self.content_min(id, avail_w);
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
        (w, h)
    }

    /// A child's contribution to a free parent's minimum: its minimum plus
    /// the margins it declares on that axis.
    fn child_extent(&self, child: NodeId, avail_w: Option<f32>) -> (f32, f32) {
        let (w, h) = self.min_size(child, avail_w);
        let m = self.placement(child).margin;
        let mw = m[0].unwrap_or(0.0).max(0.0) + m[2].unwrap_or(0.0).max(0.0);
        let mh = m[1].unwrap_or(0.0).max(0.0) + m[3].unwrap_or(0.0).max(0.0);
        (w + mw, h + mh)
    }

    fn content_min(&self, id: NodeId, avail_w: Option<f32>) -> (f32, f32) {
        let node = self.node(id);
        let inset = self.inner_inset(id);
        let inner_w = avail_w.map(|w| (w - inset[0] - inset[2]).max(0.0));
        let (cw, ch) = match node.kind {
            Kind::Label => {
                let wrap = self.res.bool(self.page, id, PropKey::Wrap).unwrap_or(false);
                let text = self.text_of(id).unwrap_or("");
                // Hug measures unwrapped (D254); wrapping needs a bound width.
                let bound = if wrap { inner_w.or(self.placement(id).max_width) } else { None };
                self.text.measure(text, self.font_size(id), bound)
            }
            Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio => {
                let (mut w, mut h) = (0.0f32, 0.0f32);
                if let Some(text) = self.text_of(id) {
                    let (tw, th) = self.text.measure(text, self.font_size(id), None);
                    w = tw;
                    h = th;
                }
                for c in self.visible_children(id) {
                    let (cw, ch) = self.child_extent(c, inner_w);
                    w = w.max(cw);
                    h = h.max(ch);
                }
                if matches!(node.kind, Kind::Check | Kind::Toggle | Kind::Radio) {
                    w += CHECK_SIZE.0;
                    h = h.max(CHECK_SIZE.1);
                }
                (w, h)
            }
            Kind::Column | Kind::Choice | Kind::List => self.stack_min(id, Axis::Y, inner_w),
            Kind::Row => self.stack_min(id, Axis::X, inner_w),
            Kind::Flow | Kind::Tiles => self.flow_min(id, inner_w),
            Kind::View | Kind::Layers | Kind::Panel | Kind::ListItem | Kind::Repeat | Kind::Custom => {
                let (mut w, mut h) = (0.0f32, 0.0f32);
                for c in self.visible_children(id) {
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
                for c in self.visible_children(id) {
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
            Kind::Image => (0.0, 0.0),
            Kind::Icon => (ICON_SIZE, ICON_SIZE),
            Kind::Rectangle | Kind::Circle | Kind::Gradient => (0.0, 0.0),
            Kind::Slider => (0.0, SLIDER_HEIGHT),
            Kind::Input => (0.0, INPUT_HEIGHT),
        };
        (cw + inset[0] + inset[2], ch + inset[1] + inset[3])
    }

    fn stack_min(&self, id: NodeId, axis: Axis, inner_w: Option<f32>) -> (f32, f32) {
        let spacing = self.spacing(id);
        let children = self.visible_children(id);
        let (mut main, mut cross) = (0.0f32, 0.0f32);
        for (i, c) in children.iter().enumerate() {
            let node = self.node(*c);
            let (w, h) = if node.kind == Kind::Gap {
                let g = if node.gap_fill { 0.0 } else { node.gap };
                if axis == Axis::Y { (0.0, g) } else { (g, 0.0) }
            } else {
                self.min_size(*c, if axis == Axis::Y { inner_w } else { None })
            };
            let (m, x) = if axis == Axis::Y { (h, w) } else { (w, h) };
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
        let sizes: Vec<(f32, f32)> = children.iter().map(|c| self.min_size(*c, None)).collect();
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

    /// Lays `id` into `frame` (the space its parent offers, margins not yet
    /// applied) and recurses. `root`: the document root ignores the free-
    /// fill default? No — the root fills its viewport unless it says otherwise.
    fn place(&mut self, id: NodeId, frame: Rect, root: bool) {
        let p = *self.placement(id);
        let (min_w, _) = self.min_size(id, Some(frame.w));
        let (x, w) = free_axis(frame.x, frame.w, p.width, p.halign, p.margin[0], p.margin[2], min_w, p.min_width, p.max_width, p.max_width_fill, root);
        // The height minimum at the width the node actually got (wrap
        // text, Flow rows).
        let (_, min_h) = self.min_size(id, Some(w));
        let (y, h) = free_axis(frame.y, frame.h, p.height, p.valign, p.margin[1], p.margin[3], min_h, p.min_height, p.max_height, p.max_height_fill, root);
        let rect = Rect::new(x, y, w, h);
        self.rects[id as usize] = rect;
        self.place_children(id, rect);
    }

    fn place_children(&mut self, id: NodeId, rect: Rect) {
        let inset = self.inner_inset(id);
        let inner = rect.inset(inset[0], inset[1], inset[2], inset[3]);
        let kind = self.node(id).kind;
        match kind {
            Kind::Column | Kind::Choice | Kind::List => self.place_stack(id, inner, Axis::Y),
            Kind::Row => self.place_stack(id, inner, Axis::X),
            Kind::Flow | Kind::Tiles => self.place_flow(id, inner),
            Kind::Grid => self.place_grid(id, inner),
            Kind::Scroll => self.place_scroll(id, inner),
            _ => {
                for c in self.visible_children(id) {
                    self.place(c, inner, false);
                }
            }
        }
    }

    fn place_stack(&mut self, id: NodeId, inner: Rect, axis: Axis) {
        let spacing = self.spacing(id);
        let children = self.visible_children(id);
        let extent = if axis == Axis::Y { inner.h } else { inner.w };
        // Minimums along the axis and the fill weights.
        let mut mins = Vec::with_capacity(children.len());
        let mut weights = Vec::with_capacity(children.len());
        let mut used = 0.0f32;
        let mut weight_sum = 0.0f32;
        for (i, c) in children.iter().enumerate() {
            let node = self.node(*c);
            let p = *self.placement(*c);
            let (m, weight) = if node.kind == Kind::Gap {
                if node.gap_fill { (0.0, node.gap) } else { (node.gap, 0.0) }
            } else {
                let (w, h) = self.min_size(*c, Some(inner.w));
                let len = if axis == Axis::Y { p.height } else { p.width };
                let m = if axis == Axis::Y { h } else { w };
                let weight = match len {
                    Len::Fill(n) => n,
                    _ => 0.0,
                };
                (m, weight)
            };
            mins.push(m);
            weights.push(weight);
            used += m;
            if i > 0 {
                used += spacing;
            }
            weight_sum += weight;
        }
        let leftover = (extent - used).max(0.0);
        let mut cursor = if axis == Axis::Y { inner.y } else { inner.x };
        for (i, c) in children.iter().enumerate() {
            let size = mins[i] + if weight_sum > 0.0 { leftover * weights[i] / weight_sum } else { 0.0 };
            let node = self.node(*c);
            if node.kind == Kind::Gap {
                self.rects[*c as usize] = if axis == Axis::Y { Rect::new(inner.x, cursor, inner.w, size) } else { Rect::new(cursor, inner.y, size, inner.h) };
                cursor += size + spacing;
                continue;
            }
            let p = *self.placement(*c);
            // The cross axis is a free axis; the main axis is the slot.
            let rect = if axis == Axis::Y {
                let (min_w, _) = self.min_size(*c, Some(inner.w));
                let (x, w) = free_axis(inner.x, inner.w, p.width, p.halign, None, None, min_w, p.min_width, p.max_width, p.max_width_fill, false);
                Rect::new(x, cursor, w, size)
            } else {
                let (_, min_h) = self.min_size(*c, None);
                let (y, h) = free_axis(inner.y, inner.h, p.height, p.valign, None, None, min_h, p.min_height, p.max_height, p.max_height_fill, false);
                Rect::new(cursor, y, size, h)
            };
            self.rects[*c as usize] = rect;
            self.place_children(*c, rect);
            cursor += size + spacing;
        }
    }

    fn place_flow(&mut self, id: NodeId, inner: Rect) {
        let spacing = self.spacing(id);
        let children = self.visible_children(id);
        let sizes: Vec<(f32, f32)> = children.iter().map(|c| self.min_size(*c, None)).collect();
        let rows = flow_rows(&sizes, inner.w, spacing);
        let mut y = inner.y;
        for row in rows {
            let mut x = inner.x;
            for i in row.first..row.end {
                let c = children[i];
                let (w, h) = sizes[i];
                let p = *self.placement(c);
                let (cy, ch) = free_axis(y, row.height, p.height, p.valign, None, None, h, p.min_height, p.max_height, false, false);
                let rect = Rect::new(x, cy, w, ch);
                self.rects[c as usize] = rect;
                self.place_children(c, rect);
                x += w + spacing;
            }
            y += row.height + spacing;
        }
    }

    fn place_scroll(&mut self, id: NodeId, inner: Rect) {
        let (sx, sy) = self.scroll_axes(id);
        let offset = self.res_scroll(id);
        for c in self.visible_children(id) {
            let (min_w, min_h) = self.min_size(c, if sx { None } else { Some(inner.w) });
            // Content takes the viewport and scrolls past it when larger.
            let w = if sx { inner.w.max(min_w) } else { inner.w };
            let h = if sy { inner.h.max(min_h) } else { inner.h };
            let frame = Rect::new(inner.x - if sx { offset[0] } else { 0.0 }, inner.y - if sy { offset[1] } else { 0.0 }, w, h);
            self.place(c, frame, false);
        }
    }

    fn res_scroll(&self, _id: NodeId) -> [f32; 2] {
        // Scroll offsets live in the Runtime; the caller folds them into
        // `Resolved` through `Resolved::scroll` when present.
        self.res.scroll.get(_id as usize).copied().unwrap_or([0.0, 0.0])
    }

    /// Which axes a Scroll pans: `allow=` when present, else inferred from
    /// the triad (a hug axis is content-sized, nothing to scroll).
    fn scroll_axes(&self, id: NodeId) -> (bool, bool) {
        if let Some(allow) = self.res.ident(self.page, id, PropKey::Allow) {
            return match allow {
                "horizontal" => (true, false),
                "both" => (true, true),
                _ => (false, true),
            };
        }
        let p = self.placement(id);
        let sx = !matches!(p.width, Len::Hug(_)) || p.max_width.is_some();
        let sy = !matches!(p.height, Len::Hug(_)) || p.max_height.is_some();
        // Default (no allow=, no hug): vertical only, the WPF ScrollViewer reading.
        if sx && sy && !matches!(p.width, Len::Exact(_) | Len::Fill(_)) {
            return (false, true);
        }
        (sx && matches!(p.width, Len::Exact(_) | Len::Fill(_)) && matches!(p.height, Len::Hug(_)), sy)
    }

    // ----- grid --------------------------------------------------------------

    fn grid_tracks(&self, id: NodeId) -> Vec<Len> {
        let Some(cols) = self.res.prop(self.page, id, PropKey::Cols) else { return vec![Len::Fill(1.0)] };
        self.page
            .list(cols)
            .iter()
            .map(|v| match *v {
                Value::VNum(n) => Len::Exact(n),
                Value::VIdent(w) if self.page.str(w) == "hug" => Len::Hug(1.0),
                Value::VIdent(_) => Len::Fill(1.0),
                Value::VCall { name, args } => {
                    let weight = self.page.values.get(args.0 as usize).and_then(|a| if let Value::VNum(n) = a { Some(*n) } else { None }).unwrap_or(1.0);
                    if self.page.str(name) == "hug" { Len::Hug(weight) } else { Len::Fill(weight) }
                }
                _ => Len::Fill(1.0),
            })
            .collect()
    }

    /// Cells of one grid row: (node, first track, span).
    fn row_cells(&self, row: NodeId) -> Vec<(NodeId, usize, usize)> {
        let mut cells = Vec::new();
        let mut track = 0;
        for c in self.page.children(row).map(|i| i as NodeId) {
            let node = self.node(c);
            if node.kind == Kind::Skip {
                track += 1;
                continue;
            }
            let span = self.res.num(self.page, c, PropKey::ColSpan).unwrap_or(1.0).max(1.0) as usize;
            if !self.gone(c) {
                cells.push((c, track, span));
            }
            track += span;
        }
        cells
    }

    /// Track minimums from single-span cells, plus each row's minimum height.
    fn grid_measure(&self, id: NodeId, inner_w: Option<f32>) -> (Vec<Len>, Vec<f32>, Vec<f32>) {
        let tracks = self.grid_tracks(id);
        let mut track_min = vec![0.0f32; tracks.len()];
        for (i, t) in tracks.iter().enumerate() {
            if let Len::Exact(n) = t {
                track_min[i] = *n;
            }
        }
        let rows = self.visible_children(id);
        let mut row_min = Vec::with_capacity(rows.len());
        for r in &rows {
            let mut h = 0.0f32;
            for (c, first, span) in self.row_cells(*r) {
                let (w, ch) = self.min_size(c, inner_w);
                h = h.max(ch);
                if span == 1 && first < track_min.len() && !matches!(tracks[first], Len::Exact(_)) {
                    track_min[first] = track_min[first].max(w);
                }
            }
            let rp = self.placement(*r);
            let h = match rp.height {
                Len::Exact(n) => n,
                _ => h.max(rp.min_height),
            };
            row_min.push(h);
        }
        (tracks, track_min, row_min)
    }

    fn grid_min(&self, id: NodeId, inner_w: Option<f32>) -> (f32, f32) {
        let spacing = self.spacing(id);
        let (tracks, track_min, row_min) = self.grid_measure(id, inner_w);
        let w: f32 = track_min.iter().sum::<f32>() + spacing * tracks.len().saturating_sub(1) as f32;
        let h: f32 = row_min.iter().sum::<f32>() + spacing * row_min.len().saturating_sub(1) as f32;
        (w, h)
    }

    fn place_grid(&mut self, id: NodeId, inner: Rect) {
        let spacing = self.spacing(id);
        let (tracks, mut track_size, mut row_size) = self.grid_measure(id, Some(inner.w));
        // Fill tracks split the leftover by weight.
        let used: f32 = track_size.iter().sum::<f32>() + spacing * tracks.len().saturating_sub(1) as f32;
        let weight_sum: f32 = tracks.iter().map(|t| if let Len::Fill(w) = t { *w } else { 0.0 }).sum();
        if weight_sum > 0.0 {
            let leftover = (inner.w - used).max(0.0);
            for (i, t) in tracks.iter().enumerate() {
                if let Len::Fill(w) = t {
                    track_size[i] += leftover * w / weight_sum;
                }
            }
        }
        let rows = self.visible_children(id);
        let row_used: f32 = row_size.iter().sum::<f32>() + spacing * rows.len().saturating_sub(1) as f32;
        let row_weight: f32 = rows.iter().map(|r| if let Len::Fill(w) = self.placement(*r).height { w } else { 0.0 }).sum();
        if row_weight > 0.0 {
            let leftover = (inner.h - row_used).max(0.0);
            for (i, r) in rows.iter().enumerate() {
                if let Len::Fill(w) = self.placement(*r).height {
                    row_size[i] += leftover * w / row_weight;
                }
            }
        }
        let mut track_x = Vec::with_capacity(tracks.len());
        let mut x = inner.x;
        for s in &track_size {
            track_x.push(x);
            x += s + spacing;
        }
        let mut y = inner.y;
        for (ri, r) in rows.iter().enumerate() {
            let rh = row_size[ri];
            self.rects[*r as usize] = Rect::new(inner.x, y, inner.w, rh);
            for (c, first, span) in self.row_cells(*r) {
                if first >= tracks.len() {
                    continue;
                }
                let last = (first + span).min(tracks.len());
                let cx = track_x[first];
                let cw: f32 = track_size[first..last].iter().sum::<f32>() + spacing * (last - first - 1) as f32;
                self.place(c, Rect::new(cx, y, cw, rh), false);
            }
            y += rh + spacing;
        }
    }
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
fn free_axis(start: f32, extent: f32, len: Len, align: Align, m_near: Option<f32>, m_far: Option<f32>, content: f32, floor: f32, cap: Option<f32>, cap_fill: bool, _root: bool) -> (f32, f32) {
    let near = m_near.unwrap_or(0.0);
    let far = m_far.unwrap_or(0.0);
    let filled = extent - near - far;
    // An aligned axis with no size key hugs: alignment carves the default fill.
    let len = if len == Len::Unset && align != Align::AlignUnset { Len::Hug(1.0) } else { len };
    // Both edges named: the box spans them.
    if m_near.is_some() && m_far.is_some() && !matches!(len, Len::Exact(_)) {
        return (start + near, filled.max(floor));
    }
    let mut size = match len {
        Len::Unset | Len::Fill(_) => filled,
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
        r[p.find(name).unwrap() as usize]
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

    #[test]
    fn scroll_content_takes_the_viewport_and_scrolls_past() {
        let (p, r) = lay("View v {\n Scroll list height=100 {\n  Column content { Rectangle a height=80; Rectangle b height=80 }\n }\n}\n", 200.0, 300.0);
        assert_eq!(rect(&p, &r, "list"), Rect::new(0.0, 0.0, 200.0, 100.0));
        assert_eq!(rect(&p, &r, "content"), Rect::new(0.0, 0.0, 200.0, 160.0));
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
        rt.go(0);
        let res = resolve(&c.page, &rt);
        let r = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        assert_eq!(rect(&c.page, &r, "bar").w, 300.0);
        rt.go(crate::runtime::BASE);
        let res = resolve(&c.page, &rt);
        let r = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        assert_eq!(rect(&c.page, &r, "bar").w, 100.0);
    }
}
