//! Pins for the instance batch (docs/todo-gogo.md GP27, GP29, GP32): a
//! widget's own `anim` plays per instance (with `self.` and a scoped
//! `play()`), `repeat x max=N` builds N addressable items that code shows
//! by count and that lay out as their parent's children, and `on:pointer=`
//! hands the raw finger to the host in the node's own space.
use gogo::anim::Finish;
use gogo::input::{EventKind, Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Action, Value};
use gogo::runtime::{Runtime, resolve};
use gogo::{Compiled, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

/// GP32: two instances of a widget each get their own `delete` anim on
/// their own `rope`; `self.opacity` is the instance root; the instance's
/// `on:show=play(delete)` names ITS copy.
#[test]
fn widget_anims_play_per_instance() {
    let src = "widget Mark {\n Layers body { Rectangle rope 1x16 }\n anim delete {
  rope.scale.y to=0.5, duration=0
  and self.opacity to=0, duration=1
 }\n}\nView v {\n Mark a on:show=play(delete)\n Mark b\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let a_rope = page.find("a/rope").expect("a/rope") as usize;
    let b_rope = page.find("b/rope").expect("b/rope") as usize;
    let a_root = page.find("a").unwrap() as usize;
    let mut rt = Runtime::new(&page);
    let a_delete = page.intern_lookup("a/delete");
    let b_delete = page.intern_lookup("b/delete");
    assert!(a_delete != gogo::page::NO_STR && b_delete != gogo::page::NO_STR, "one anim per instance");
    rt.play(&page, a_delete, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    assert_eq!(rt.motion[a_rope].scale[1], 0.5);
    assert_eq!(rt.motion[b_rope].scale[1], 1.0, "b's rope is untouched");
    assert!((rt.motion[a_root].opacity - 0.5).abs() < 1e-3, "`self` is the instance root");
    // The instance's own `on:show=play(delete)` names a's copy.
    let handlers = &page.handlers[page.nodes[a_root].handlers.0 as usize..(page.nodes[a_root].handlers.0 + page.nodes[a_root].handlers.1) as usize];
    assert!(handlers.iter().any(|h| h.action == Action::Play(a_delete)), "{handlers:?}");
}

/// GP27: `repeat marks max=3 { Mark }` builds three hidden items; the
/// count shows them and hides `empty`; shown items take the Column's
/// slots with its spacing.
#[test]
fn repeat_items_show_by_count_and_lay_out_in_the_stack() {
    let src = "widget Mark {\n Layers mark height=10, width=fill { Rectangle box height=10, width=fill }\n}\nView v {\n Column col spacing=4, halign=left, valign=top, width=100 {\n  Rectangle head height=10\n  repeat marks max=3 {\n   empty { Label none text=\"no marks\", font_size=10 }\n   Mark\n  }\n  Rectangle tail height=10\n }\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let marks = page.find("marks").expect("the repeat node");
    let mut rt = Runtime::new(&page);
    let items = rt.items(&page, marks);
    assert_eq!(items.len(), 3);
    assert_eq!(page.str(page.nodes[items[1] as usize].name), "marks/1/item");
    assert!(page.find("marks/1/item/box").is_some(), "parts are addressable");
    let lay = |page: &gogo::Page, rt: &Runtime| {
        let res = resolve(page, rt);
        layout(page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance)
    };
    let r = lay(&page, &rt);
    let tail = page.find("tail").unwrap() as usize;
    let none = page.find("none").unwrap() as usize;
    assert_eq!(r[tail].y, 10.0 + 4.0 + 12.5 + 4.0, "at zero the `empty` label holds the slot");
    assert!(r[none].h > 0.0);
    rt.set_count(&mut page, marks, 2);
    let r = lay(&page, &rt);
    assert_eq!(r[tail].y, 10.0 + 4.0 + 10.0 + 4.0 + 10.0 + 4.0, "two items, spaced like siblings");
    assert_eq!(r[items[0] as usize].y, 14.0);
    assert_eq!(r[items[1] as usize].y, 28.0);
    let res = resolve(&page, &rt);
    assert!(res.gone(&page, items[2]));
    let empty = page.nodes[none].parent;
    assert!(res.gone(&page, empty), "`empty` hides once there are items");
    rt.set_count(&mut page, marks, 0);
    let res = resolve(&page, &rt);
    assert!(!res.gone(&page, empty));
}

/// GP29: an `on:pointer=` node gets down/move/up in its own space and
/// keeps the pointer; no `held`, no activate.
#[test]
fn pointer_reports_every_phase_in_node_space() {
    let src = "View v {\n Layers pad width=100, height=50, halign=left, valign=top, margin_left=20, margin_top=10, interactive=true, on:pointer=pad_touch\n}\n";
    let c = ok(src);
    let page = c.page;
    let pad = page.find("pad").unwrap();
    let mut rt = Runtime::new(&page);
    let mut input = Input::new();
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let mut touch = |phase: Phase, x: f32, y: f32, t: f64, rt: &mut Runtime| input.touch(&page, rt, &res, &rects, Touch { phase, x, y, t, id: 1 });
    let ev = touch(Phase::Down, 25.0, 15.0, 0.0, &mut rt);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].node, pad);
    assert_eq!(ev[0].kind, EventKind::Pointer { phase: Phase::Down, x: 5.0, y: 5.0 });
    assert!(matches!(ev[0].action, Some(Action::Named(id)) if page.str(id) == "pad_touch"));
    assert!(!rt.has(pad, gogo::runtime::HELD));
    // Moving off the node still reports (captured), in node space.
    let ev = touch(Phase::Move, 150.0, 100.0, 0.1, &mut rt);
    assert_eq!(ev[0].kind, EventKind::Pointer { phase: Phase::Move, x: 130.0, y: 90.0 });
    let ev = touch(Phase::Up, 150.0, 100.0, 0.2, &mut rt);
    assert_eq!(ev.len(), 1);
    assert!(matches!(ev[0].kind, EventKind::Pointer { phase: Phase::Up, .. }));
    let _ = Value::VNum(0.0);
}

/// `set_raise` (the Canvas.ZIndex port): a raised sibling paints after
/// (over) the others and takes the touch first; raise 0 restores the
/// file's order; a negative raise paints first (under).
#[test]
fn raise_reorders_paint_and_hit() {
    let src = "View v {
 Layers l halign=left, valign=top, width=100, height=100 {
  Rectangle a 40x40, halign=left, valign=top, interactive=true
  Rectangle b 40x40, halign=left, valign=top, margin_left=20, interactive=true
 }
}
";
    let c = ok(src);
    let page = c.page;
    let a = page.find("a").unwrap();
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    let order = |page: &gogo::Page, rt: &Runtime| -> Vec<gogo::page::NodeId> {
        let res = resolve(page, rt);
        let rects = layout(page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
        gogo::draw(page, &res, &rects)
            .iter()
            .filter_map(|d| if let gogo::Draw::Quad { rect, .. } = d { Some(rect.x as i32) } else { None })
            .collect::<Vec<i32>>()
            .iter()
            .map(|x| if *x == 0 { a } else { b })
            .collect()
    };
    let hit_at = |page: &gogo::Page, rt: &Runtime, x: f32| {
        let res = resolve(page, rt);
        let rects = layout(page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
        gogo::draw::hit(page, &res, &rects, x, 10.0, false)
    };
    assert_eq!(order(&page, &rt), vec![a, b], "file order: b paints over a");
    assert_eq!(hit_at(&page, &rt, 30.0), Some(b), "the overlap belongs to b");
    rt.set_raise(a, 1);
    assert_eq!(order(&page, &rt), vec![b, a], "raised: a paints over b");
    assert_eq!(hit_at(&page, &rt, 30.0), Some(a), "and takes the touch");
    rt.set_raise(a, 0);
    assert_eq!(order(&page, &rt), vec![a, b]);
    rt.set_raise(b, -1);
    assert_eq!(order(&page, &rt), vec![b, a], "a negative raise paints first (under)");
}
