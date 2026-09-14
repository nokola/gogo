//! Two runtime rules the brush page found on device (todo-bugs.md #1 and
//! #2, 2026-09-06): a motion word DECLARED on a node line is its resting
//! transform (`rotate=90` turns the picture, a `when` still overrides
//! it), and a node that had NO box does not glide out of the origin when
//! a timed arrangement shows it — it appears at its laid-out rect.
use gogo::draw::{Draw, draw};
use gogo::KeptFrame;
use gogo::layout::{FixedAdvance, Rect};
use gogo::runtime::{Runtime, resolve, resolve_animated};
use gogo::{Compiled, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

const W: f32 = 400.0;
const H: f32 = 400.0;

fn frame(page: &mut gogo::Page, rt: &mut Runtime, dt: f32) -> KeptFrame {
    rt.tick(page, dt);
    let mut f = KeptFrame::fresh(page, resolve_animated(page, rt), Rect::new(0.0, 0.0, W, H), &FixedAdvance);
    f.glide(rt);
    f
}

#[test]
fn a_declared_rotate_turns_the_picture_and_a_when_overrides_it() {
    let c = ok("arrangement flat\nView v width=400, height=400 {\n Rectangle pic width=200, height=40, halign=center, valign=center, rotate=90, translate.x=5\n when flat { pic.rotate = 0 }\n}\n");
    let mut page = c.page;
    let pic = page.find("pic").unwrap();
    let mut rt = Runtime::new(&page);
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, W, H), &FixedAdvance);
    let res = f.look();
    assert_eq!(res.motion[pic.index()].rotate, 90.0, "the declared rotate is the resting transform");
    assert_eq!(res.motion[pic.index()].translate[0], 5.0, "so is a declared translate");
    let list: Vec<Draw> = draw(&page, f.frame());
    assert!(list.iter().any(|d| matches!(d, Draw::RotatePush { angle, .. } if (*angle - 90.0).abs() < 1e-3)), "the draw walk turns it: {list:?}");
    rt.go_id(gogo::ArrangementId::new(0));
    let f = frame(&mut page, &mut rt, 0.0);
    let (res, _) = (f.look(), f.rects());
    assert_eq!(res.motion[pic.index()].rotate, 0.0, "a `when` response overrides the declared word");
}

#[test]
fn a_node_that_had_no_box_appears_in_place_under_a_timed_flip() {
    let c = ok("arrangement open\ntransition any -> any duration=0.2\nwhen open { plate.visibility = visible }\nView v width=400, height=400 {\n Rectangle plate width=100, height=100, halign=right, valign=bottom, visibility=gone\n Rectangle bar width=100, height=20, halign=left, valign=top\n when open { bar.halign = right }\n}\n");
    let mut page = c.page;
    let plate = page.find("plate").unwrap().index();
    let bar = page.find("bar").unwrap().index();
    let mut rt = Runtime::new(&page);
    let f = frame(&mut page, &mut rt, 0.0);
    let (_, r) = (f.look(), f.rects());
    assert_eq!(r[plate], Rect::ZERO, "gone: no box");
    rt.go_id(gogo::ArrangementId::new(0));
    let f = frame(&mut page, &mut rt, 0.0);
    let (_, r) = (f.look(), f.rects());
    let f = frame(&mut page, &mut rt, 0.1);
    let r1 = f.rects();
    assert_eq!(r[plate], Rect::new(300.0, 300.0, 100.0, 100.0), "the plate shows at its laid-out rect on the first frame, not inflating from (0,0): {:?}", r[plate]);
    assert_eq!(r1[plate], r[plate]);
    assert!(r1[bar].x > 0.0 && r1[bar].x < 300.0, "a node that HAD a box still glides: {:?}", r1[bar]);
}
