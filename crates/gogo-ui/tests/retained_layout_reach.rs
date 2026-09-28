//! The retained layout lays out only what a change can move
//! (`LayoutCache`): a box moved by its margins right under the page root
//! places that box again and nothing else; a change inside a Column
//! measures the changed label and its ancestors, its siblings answer from
//! what they kept; and when the root grows to its content, every child of
//! the root is placed again. Every step's rects must equal a plain
//! `layout` bit for bit, so the savings cannot change what shows.
//!
//! Needed because a dragged box (a text callout, the canvas overlay under
//! a panning finger) moves by margins every frame, and the walk used to
//! lay the whole page out again whenever it reached the root: 341 boxes
//! and 182 measures a frame on the OnePlus for one callout drag.
use gogo::layout::LayoutStats;
use gogo::runtime::{Resolved, Runtime, resolve_animated};
use gogo::{FixedAdvance, NodeId, Page, PropKey, Rect, Value, compile, has_errors, layout};

const VIEWPORT: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };

const PAGE: &str = "View reach {
    Column list spacing=4 {
        Label a text=\"alpha\"
        Label b text=\"beta\"
        Label c text=\"gamma\"
    }
    Panel mover width=80, height=40, halign=left, valign=top {
        Label moverText text=\"drag me\"
    }
    Label caption text=\"caption\", valign=bottom
}
";

struct Host {
    page: Page,
    rt: Runtime,
}

impl Host {
    fn new() -> Host {
        let c = compile(PAGE, &|_| None);
        assert!(!has_errors(&c.diags), "{:#?}", c.diags);
        let rt = Runtime::new(&c.page);
        let mut host = Host { page: c.page, rt };
        let first: LayoutStats = host.frame("the first frame");
        assert!(first.full, "the first frame lays the whole page out");
        host
    }

    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node `{name}`"))
    }

    /// Resolve and lay out retained, check the rects against a plain
    /// `layout`, and hand back what the retained road did.
    fn frame(&mut self, what: &str) -> LayoutStats {
        let res: Resolved = resolve_animated(&self.page, &mut self.rt);
        let kept: Vec<Rect> = self.rt.layout_retained(&self.page, VIEWPORT, &FixedAdvance).to_vec();
        let fresh: Vec<Rect> = layout(&self.page, &res, VIEWPORT, &FixedAdvance);
        if let Some(i) = (0..fresh.len()).find(|i| kept[*i] != fresh[*i]) {
            panic!("{what}: node {i} (`{}`) is at {:?}, a full layout puts it at {:?}", self.page.str(self.page.nodes[i].name), kept[i], fresh[i]);
        }
        self.rt.debug_layout_stats()
    }
}

#[test]
fn a_box_moved_by_its_margins_under_the_page_root_places_only_that_box_again() {
    let mut host = Host::new();
    let mover: NodeId = host.id("mover");
    for (x, y) in [(50.0, 120.0), (51.5, 121.0), (200.0, 300.0)] {
        host.rt.set_prop(mover, PropKey::MarginLeft, Value::VNum(x));
        host.rt.set_prop(mover, PropKey::MarginTop, Value::VNum(y));
        let st: LayoutStats = host.frame("a margin move");
        assert!(!st.full, "a margin move laid the whole page out");
        assert_eq!(st.placed, 2, "a margin move placed {} boxes; the mover and its label are all it can move", st.placed);
        // Only the root, the mover and its label forgot their measures;
        // each is asked at up to three widths (its room, its final width,
        // unbounded). The Column and the caption answer from what they kept.
        assert!(st.measured <= 9, "a margin move measured {} times", st.measured);
    }
}

#[test]
fn a_change_inside_a_column_measures_its_ancestors_and_keeps_its_siblings_measures() {
    let mut host = Host::new();
    let b: NodeId = host.id("b");
    host.rt.set_text(&mut host.page, b, "beta, much longer now");
    let st: LayoutStats = host.frame("a label rewritten inside the column");
    assert!(!st.full, "a rewritten label laid the whole page out");
    // The Column, its three labels: the mover and the caption beside it
    // are not placed again.
    assert_eq!(st.placed, 4, "a rewritten label placed {} boxes", st.placed);
    // The label, the Column and the root forgot their measures, each asked
    // at up to three widths; the label's siblings answer from what they kept.
    assert!(st.measured <= 9, "a rewritten label measured {} times", st.measured);
    let a: NodeId = host.id("a");
    host.rt.set_prop(a, PropKey::FontSize, Value::VNum(30.0));
    let st: LayoutStats = host.frame("a sibling's font size");
    assert!(!st.full && st.placed == 4, "a sibling's font size: {st:?}");
}

#[test]
fn when_the_root_grows_to_its_content_every_child_of_it_is_placed_again() {
    let mut host = Host::new();
    let mover: NodeId = host.id("mover");
    let n: usize = host.page.nodes.len();
    host.rt.set_prop(mover, PropKey::Height, Value::VNum(2000.0));
    let st: LayoutStats = host.frame("the mover taller than the screen");
    assert!(!st.full, "the root growing laid the whole page out from scratch");
    assert_eq!(st.placed as usize, n - 1, "the root grew, so every box under it is placed again");
    let root: Rect = host.rt.layout_retained(&host.page, VIEWPORT, &FixedAdvance)[0];
    assert!(root.h >= 2000.0, "the root did not grow to its content: {root:?} — this pin needs a root that grows");
    host.rt.set_prop(mover, PropKey::Height, Value::VNum(40.0));
    let st: LayoutStats = host.frame("the mover back to its size");
    assert_eq!(st.placed as usize, n - 1, "the root shrank back, so every box under it is placed again");
}
