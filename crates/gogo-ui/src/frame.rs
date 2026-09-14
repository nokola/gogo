//! One frame's look and rects, as one thing.
//!
//! A draw or a hit needs a look (`Resolved`) and the rects that look was
//! laid out to. Handed over as two values, nothing ties them together: a
//! host once drew from a look taken BEFORE the glide, so the settings
//! sheet flashed at its seat for a frame (B38); a look and rects of two
//! different frames would draw stale boxes or read past the slice. So
//! the pair travels as a `Frame`, and only this crate makes one — from
//! the runtime's road (`KeptFrame::frame_retained`) or, for pins, the
//! fresh road (`KeptFrame::fresh`). `draw`, `hit`, the input machine and
//! the debug walks take a `Frame`, never a look and a slice on their own
//! (D43).

use crate::draw::Draw;
use crate::layout::{Rect, TextMeasure, layout};
use crate::page::{NodeId, Page};
use crate::runtime::{Resolved, Runtime};

/// A look and the rects it was laid out to, borrowed together. Two
/// borrows, `Copy`: pass it by value. Made only inside this crate, so a
/// consumer can trust the pair came from one road on one frame.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    look: &'a Resolved,
    rects: &'a [Rect],
}

impl<'a> Frame<'a> {
    /// The one door — the runtime's roads and `KeptFrame` use it; a look
    /// and rects sized for different pages are a bug on the road.
    pub(crate) fn new(look: &'a Resolved, rects: &'a [Rect]) -> Frame<'a> {
        debug_assert!(rects.is_empty() || look.motion.is_empty() || rects.len() == look.motion.len(), "a look and rects of different pages: {} nodes vs {} rects", look.motion.len(), rects.len());
        Frame { look, rects }
    }

    /// The look: every node's resolved props, states, motion and tree.
    pub fn look(self) -> &'a Resolved {
        self.look
    }

    /// The rects the look was laid out to, one per node (page epx).
    pub fn rects(self) -> &'a [Rect] {
        self.rects
    }
}

/// The frame a host keeps: its own copy of the look (the input machine
/// reads it while the runtime is borrowed mutably) and the rects the
/// road laid out. The road writes both in one call, so they cannot
/// drift apart; the buffers are reused, nothing allocates once warm.
#[derive(Default)]
pub struct KeptFrame {
    look: Resolved,
    rects: Vec<Rect>,
}

impl KeptFrame {
    /// The frame road into this kept frame — `Runtime::frame_retained`
    /// into `rects` and `draws`, then the look copied in (`clone_from`,
    /// buffers reused). `split`: the `above_children` road, see there.
    pub fn frame_retained(&mut self, rt: &mut Runtime, page: &Page, viewport: Rect, text: &dyn TextMeasure, draws: &mut Vec<Draw>, split: Option<(NodeId, &mut Vec<Draw>)>) {
        rt.frame_retained(page, viewport, text, &mut self.rects, draws, split);
        self.look.clone_from(rt.resolved());
    }

    /// The fresh road, for pins: `look` (a `resolve` or `resolve_animated`)
    /// laid out in `viewport` from scratch. Not a host's road — a host's
    /// frame comes from `frame_retained`, with the glide and the anim
    /// motion of the frame; this is the plain look, and the host crates'
    /// lint (`frame_road_lint.rs`) keeps them off it.
    pub fn fresh(page: &Page, look: Resolved, viewport: Rect, text: &dyn TextMeasure) -> KeptFrame {
        let rects: Vec<Rect> = layout(page, &look, viewport, text);
        KeptFrame { look, rects }
    }

    /// The glide over this frame's rects (`Runtime::glide_in_place`): the
    /// rect tweens of the frame, for a pin that laid out fresh and wants
    /// the shown rects.
    pub fn glide(&mut self, rt: &mut Runtime) {
        rt.glide_in_place(&mut self.rects);
    }

    /// The borrowed pair — what `draw`, `hit` and the input machine take.
    pub fn frame(&self) -> Frame<'_> {
        Frame::new(&self.look, &self.rects)
    }

    pub fn look(&self) -> &Resolved {
        &self.look
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    /// True once a road laid this frame out; a host with no frame yet
    /// (or hidden, see `clear`) has nothing to hit or draw.
    pub fn is_laid_out(&self) -> bool {
        !self.rects.is_empty()
    }

    /// Forgets the rects (a hidden child page): `is_laid_out` goes false;
    /// the buffers stay for the next road.
    pub fn clear(&mut self) {
        self.rects.clear();
    }
}
