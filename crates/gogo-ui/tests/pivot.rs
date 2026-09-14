//! `pivot=` — the 0..1 box point `scale` and `rotate` transform about
//! (2026-09-02, the Godot 4.7 offset-transforms comparison round). Unset
//! = the center, exactly the old behavior; `0x0` holds the top-left
//! still. One fixed point serves the node's own rect, its subtree's
//! shift, the rotation center AND the hit walk's moved rects — look and
//! touch agree by construction.

use gogo::compile;
use gogo::KeptFrame;
use gogo::draw::{Draw, draw, hit};
use gogo::layout::{FixedAdvance, Rect};
use gogo::page::{Color_Straight_Srgb_4xU8_RGBA, PropKey, Value};
use gogo::runtime::{Runtime, resolve};

fn fails(src: &str) -> bool {
    gogo::has_errors(&compile(src, &|_| None).diags)
}

/// The plate quad drawn for the node filled `color` (the fixture's way to
/// find one box in the draw list).
fn quad_of(draws: &[Draw], color: Color_Straight_Srgb_4xU8_RGBA) -> Rect {
    draws
        .iter()
        .find_map(|d| match d {
            Draw::Quad { rect, color: c, .. } if *c == color => Some(*rect),
            _ => None,
        })
        .expect("the box's quad")
}

/// `pivot=0x0` under `scale=2`: the top-left corner holds still — the
/// box grows right and down, and a child in the corner scales in place.
/// Without a `pivot=`, the same scale spreads about the center (the old
/// behavior, unchanged).
#[test]
fn scale_holds_the_pivot_point_still() {
    let c = compile("View v { Panel p width=100, height=50, halign=left, valign=top, background=#FF0000FF, pivot=0x0 { Rectangle c 10x10, halign=left, valign=top, color=#00FF00FF } }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let p = page.find("p").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_prop(p, PropKey::Scale, Value::VNum(2.0));
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let draws = draw(&page, f.frame());
    assert_eq!(quad_of(&draws, Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255])), Rect::new(0.0, 0.0, 200.0, 100.0), "the top-left corner held");
    assert_eq!(quad_of(&draws, Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255])), Rect::new(0.0, 0.0, 20.0, 20.0), "the corner child scales in place");

    // The same page WITHOUT pivot: the center holds (the old law).
    let c = compile("View v { Panel p width=100, height=50, halign=left, valign=top, background=#FF0000FF }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let p = page.find("p").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_prop(p, PropKey::Scale, Value::VNum(2.0));
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let draws = draw(&page, f.frame());
    assert_eq!(quad_of(&draws, Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255])), Rect::new(-50.0, -25.0, 200.0, 100.0), "no pivot = about the center");
}

/// `rotate` turns about the pivot: `1x1` puts the rotation center on the
/// bottom-right corner (a door swinging on its hinge).
#[test]
fn rotate_turns_about_the_pivot() {
    let c = compile("View v { Panel p width=100, height=50, halign=left, valign=top, background=#FF0000FF, pivot=1x1 }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let p = page.find("p").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_prop(p, PropKey::Rotate, Value::VNum(30.0));
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let draws = draw(&page, f.frame());
    let (cx, cy) = draws
        .iter()
        .find_map(|d| match d {
            Draw::RotatePush { cx, cy, .. } => Some((*cx, *cy)),
            _ => None,
        })
        .expect("a rotate bracket");
    assert_eq!((cx, cy), (100.0, 50.0), "the hinge is the bottom-right corner");
}

/// The finger lands on the pivoted picture: a button scaled 2x about its
/// top-left takes a tap in the grown half the un-pivoted box never
/// covered — `hit` reads the same fixed point the draw walk holds.
#[test]
fn hit_follows_the_pivot() {
    let c = compile("View v { Button b text=\"x\", width=100, height=50, halign=left, valign=top, pivot=0x0 }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_prop(b, PropKey::Scale, Value::VNum(2.0));
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    assert_eq!(hit(&page, f.frame(), 150.0, 75.0, false), Some(b), "the grown half takes the tap");
}

/// The finger lands on the ROTATED picture too: a button turned 90
/// degrees about its center takes a tap in the space its turned box
/// covers and the laid-out box never did — and refuses one inside the
/// laid-out footprint the turned box has left. The renderer applies the
/// innermost rotate bracket per quad; `hit` un-rotates the finger about
/// the same point, so look and touch agree.
#[test]
fn hit_follows_rotation() {
    let c = compile("View v { Button b text=\"x\", width=100, height=50, halign=left, valign=top }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_prop(b, PropKey::Rotate, Value::VNum(90.0));
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    // Turned 90 deg about the center (50, 25): the picture stands tall,
    // x in 25..75, y in -25..75.
    assert_eq!(hit(&page, f.frame(), 60.0, 65.0, false), Some(b), "inside the turned box, below the laid-out one");
    assert_eq!(hit(&page, f.frame(), 95.0, 45.0, false), None, "the laid-out corner the turned box left");

    // A pivoted rotation: the hinge is the top-left, the box swings so
    // its former top edge runs DOWN the y axis.
    let c = compile("View v { Button b text=\"x\", width=100, height=50, halign=left, valign=top, pivot=0x0 }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_prop(b, PropKey::Rotate, Value::VNum(90.0));
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    assert_eq!(hit(&page, f.frame(), -25.0, 50.0, false), Some(b), "swung across the hinge");
    assert_eq!(hit(&page, f.frame(), 60.0, 25.0, false), None, "the old footprint is empty");
}

/// The fences: a full 0..1 pair or nothing — a bare number, a half pair
/// or an out-of-box point is refused (G2005), and `pivot` is a look
/// word, not an anim lane.
#[test]
fn pivot_fences() {
    assert!(fails("View v { Panel p pivot=2x0.5 }"), "out of the box");
    assert!(fails("View v { Panel p pivot=0.5 }"), "a bare number");
    assert!(fails("View v { Panel p pivot=0.5x }"), "half a pair");
    assert!(!fails("View v { Panel p pivot=0x1 }"), "corners are legal");
    assert!(fails("anim a { p.pivot to=1, duration=1 }\nView v { Panel p }"), "not an anim lane");
}
