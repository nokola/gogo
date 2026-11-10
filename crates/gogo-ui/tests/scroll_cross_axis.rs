//! A Scroll's content fits the viewport on the axis the Scroll does not pan.
//! One line too long to wrap then cuts off only itself; before, it widened
//! everything that fills the width, and the Scroll clipped all of it
//! (USER SAW a settings page in Bulgarian cut off on the right, every
//! full-width row running past the screen edge, 2026-09-29).
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{KeptFrame, compile};

/// Lays the page out in a `w` x `h` window and returns `names`' rects.
fn rects(src: &str, w: f32, h: f32, names: &[&str]) -> Vec<Rect> {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    let rt = Runtime::new(&c.page);
    let kept = KeptFrame::fresh(&c.page, resolve(&c.page, &rt), Rect::new(0.0, 0.0, w, h), &FixedAdvance);
    names.iter().map(|n| kept.rects()[c.page.find(n).unwrap_or_else(|| panic!("no node {n}")).index()]).collect()
}

/// The settings page's shape: a vertical Scroll over a padded column
/// holding one line that cannot wrap and a paragraph that can.
#[test]
fn a_vertical_scroll_keeps_its_content_to_the_window_width() {
    let src = "View v {\n Column width=fill, height=fill {\n  Scroll s width=fill, height=fill, allow=vertical, bars=never {\n   Panel pad width=fill, padding_left=20, padding_right=20 {\n    Column col width=fill {\n     Label long text=\"a line that is much wider than the phone it sits on and cannot wrap\"\n     Label para text=\"a paragraph that wraps at the width it is given by its column\", wrap=true, width=fill\n    }\n   }\n  }\n }\n}\n";
    let r = rects(src, 360.0, 780.0, &["v", "s", "pad", "col", "para"]);
    let (v, s, pad, col, para) = (r[0], r[1], r[2], r[3], r[4]);
    assert_eq!(v.w, 360.0, "the page is the window's width: {v:?}");
    assert_eq!(s.w, 360.0, "the Scroll is the window's width: {s:?}");
    assert_eq!(pad.w, 360.0, "the content is the viewport's width: {pad:?}");
    assert_eq!(col.x + col.w, 340.0, "the column ends at the right padding: {col:?}");
    assert!(para.x + para.w <= 340.0 && para.h > 30.0, "the paragraph wraps inside it: {para:?}");
}

/// A Scroll inside a box that hugs its content still tells the box how
/// wide the content is: the limit is what the box is offered, not zero.
#[test]
fn a_vertical_scroll_in_a_hugging_box_is_as_wide_as_its_content() {
    let src = "View v {\n Panel box width=hug, halign=left {\n  Scroll s height=100, allow=vertical, bars=never {\n   Rectangle r width=120, height=400, color=#FF0000FF\n  }\n }\n}\n";
    let r = rects(src, 360.0, 780.0, &["box", "s"]);
    assert_eq!(r[0].w, 120.0, "{:?}", r[0]);
    assert_eq!(r[1].w, 120.0, "{:?}", r[1]);
}

/// A Scroll that pans both ways holds neither axis: its content grows to
/// fit, and the Scroll pans over all of it.
#[test]
fn a_scroll_panning_both_ways_lets_its_content_grow() {
    let src = "View v {\n Scroll s width=fill, height=fill, allow=both, bars=never {\n  Panel pad width=fill, height=fill {\n   Rectangle big width=900, height=900, color=#FF0000FF\n  }\n }\n}\n";
    let r = rects(src, 360.0, 300.0, &["s", "pad"]);
    assert_eq!((r[0].w, r[0].h), (360.0, 300.0), "the Scroll stays the window's size: {:?}", r[0]);
    assert_eq!((r[1].w, r[1].h), (900.0, 900.0), "{:?}", r[1]);
}
