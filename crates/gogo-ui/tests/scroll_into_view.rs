//! `Runtime::scroll_into_view` (D116): the one door for "show me this row"
//! from code. The least move that puts the item fully inside the viewport,
//! per axis: nothing when it already is; past the far edge → that edge,
//! before the near edge → that edge, each plus the margin of air; clamped
//! at 0. Before, the brush list and the face strip each spelled their own
//! rule, and the list's moved the row to the bottom edge on every show.
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{KeptFrame, NodeId, Page, compile};

/// A 100-tall vertical Scroll over ten 40-tall rows.
const COLUMN: &str = "View v {\n Scroll s width=100, height=100, allow=vertical, bars=never {\n  Column c {\n   Rectangle r0 width=100, height=40, color=#FF0000FF\n   Rectangle r1 width=100, height=40, color=#FF0000FF\n   Rectangle r2 width=100, height=40, color=#FF0000FF\n   Rectangle r3 width=100, height=40, color=#FF0000FF\n   Rectangle r4 width=100, height=40, color=#FF0000FF\n   Rectangle r5 width=100, height=40, color=#FF0000FF\n   Rectangle r6 width=100, height=40, color=#FF0000FF\n   Rectangle r7 width=100, height=40, color=#FF0000FF\n   Rectangle r8 width=100, height=40, color=#FF0000FF\n   Rectangle r9 width=100, height=40, color=#FF0000FF\n  }\n }\n}\n";

/// A 100-wide horizontal Scroll over ten 40-wide items.
const STRIP: &str = "View v {\n Scroll s width=100, height=40, allow=horizontal, bars=never {\n  Row c spacing=0 {\n   Rectangle r0 width=40, height=40, color=#FF0000FF\n   Rectangle r1 width=40, height=40, color=#FF0000FF\n   Rectangle r2 width=40, height=40, color=#FF0000FF\n   Rectangle r3 width=40, height=40, color=#FF0000FF\n   Rectangle r4 width=40, height=40, color=#FF0000FF\n   Rectangle r5 width=40, height=40, color=#FF0000FF\n   Rectangle r6 width=40, height=40, color=#FF0000FF\n   Rectangle r7 width=40, height=40, color=#FF0000FF\n   Rectangle r8 width=40, height=40, color=#FF0000FF\n   Rectangle r9 width=40, height=40, color=#FF0000FF\n  }\n }\n}\n";

fn open(src: &str) -> (Page, Runtime) {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    let rt = Runtime::new(&c.page);
    (c.page, rt)
}

fn rects(page: &Page, rt: &Runtime) -> Vec<Rect> {
    KeptFrame::fresh(page, resolve(page, rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance).rects().to_vec()
}

fn node(page: &Page, name: &str) -> NodeId {
    page.find(name).unwrap_or_else(|| panic!("no node {name}"))
}

#[test]
fn a_row_below_lands_on_the_bottom_edge_one_above_on_the_top_edge_one_in_view_stays() {
    let (page, mut rt) = open(COLUMN);
    let (s, r7, r2) = (node(&page, "s"), node(&page, "r7"), node(&page, "r2"));

    // r7 (content y 280..320) is below the 100-tall viewport: bottom-aligned.
    let r = rects(&page, &rt);
    assert!(rt.scroll_into_view(s, r7, &r, 0.0), "a move was needed");
    assert_eq!(rt.scroll_of(s), [0.0, 220.0]);
    let r = rects(&page, &rt);
    assert_eq!(r[r7.index()].y + r[r7.index()].h, r[s.index()].y + r[s.index()].h, "r7's bottom meets the viewport bottom: {:?}", r[r7.index()]);

    // Already fully in view: no write, the offset is the user's.
    assert!(!rt.scroll_into_view(s, r7, &r, 0.0));
    assert_eq!(rt.scroll_of(s), [0.0, 220.0]);

    // r2 (content y 80..120) is above: top-aligned, with 10 of air above it.
    assert!(rt.scroll_into_view(s, r2, &r, 10.0));
    assert_eq!(rt.scroll_of(s), [0.0, 70.0]);
    let r = rects(&page, &rt);
    assert_eq!(r[r2.index()].y - r[s.index()].y, 10.0, "{:?}", r[r2.index()]);

    // The margin never scrolls before the start: r0 from here clamps at 0.
    let r0 = node(&page, "r0");
    assert!(rt.scroll_into_view(s, r0, &r, 30.0));
    assert_eq!(rt.scroll_of(s), [0.0, 0.0]);
    // ...and at 0 with r0 in view, a second ask writes nothing.
    let r = rects(&page, &rt);
    assert!(!rt.scroll_into_view(s, r0, &r, 30.0));
}

#[test]
fn the_other_axis_works_the_same_and_a_hidden_box_is_left_alone() {
    let (page, mut rt) = open(STRIP);
    let (s, r8) = (node(&page, "s"), node(&page, "r8"));
    // r8 (content x 320..360) past the 100-wide viewport, half an item of
    // the neighbor peeking in: 360 - 100 + 20.
    let r = rects(&page, &rt);
    assert!(rt.scroll_into_view(s, r8, &r, 20.0));
    assert_eq!(rt.scroll_of(s), [280.0, 0.0]);
    let r = rects(&page, &rt);
    assert_eq!(r[r8.index()].x + r[r8.index()].w + 20.0, r[s.index()].x + r[s.index()].w, "{:?}", r[r8.index()]);

    // A box with no size yet (gone) asks for nothing.
    let mut page = page;
    rt.set_visible(&mut page, r8, false);
    let r = rects(&page, &rt);
    assert!(!rt.scroll_into_view(s, r8, &r, 20.0));
    assert_eq!(rt.scroll_of(s), [280.0, 0.0]);
}
