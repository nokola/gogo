//! Pins for the instance batch (docs/todo-gogo.md GP27, GP29, GP32): a
//! widget's own `anim` plays per instance (with `self.` and a scoped
//! `play()`), `repeat x capacity=N` builds N addressable items that code shows
//! by count and that lay out as their parent's children, and `on:pointer=`
//! hands the raw finger to the host in the node's own space.
use gogo::anim::Finish;
use gogo::KeptFrame;
use gogo::input::{EventKind, Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Action, PropKey, Value};
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
    let a_rope = (page.find("a/rope").expect("a/rope")).index();
    let b_rope = (page.find("b/rope").expect("b/rope")).index();
    let a_root = (page.find("a").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let a_delete = page.intern_lookup("a/delete");
    let b_delete = page.intern_lookup("b/delete");
    assert!(a_delete != gogo::page::NO_STR && b_delete != gogo::page::NO_STR, "one anim per instance");
    rt.play_id(&page, a_delete, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    assert_eq!(rt.motion[a_rope].scale[1], 0.5);
    assert_eq!(rt.motion[b_rope].scale[1], 1.0, "b's rope is untouched");
    assert!((rt.motion[a_root].opacity - 0.5).abs() < 1e-3, "`self` is the instance root");
    // The instance's own `on:show=play(delete)` names a's copy.
    let handlers = &page.handlers[page.nodes[a_root].handlers.0 as usize..(page.nodes[a_root].handlers.0 + page.nodes[a_root].handlers.1) as usize];
    assert!(handlers.iter().any(|h| h.action == Action::Play(a_delete)), "{handlers:?}");
}

/// GP27: `repeat marks capacity=3 { Mark }` reserves three slots at the
/// first count; the count shows them and hides `empty`; shown rows take
/// the Column's slots with its spacing.
#[test]
fn repeat_items_show_by_count_and_lay_out_in_the_stack() {
    let src = "widget Mark {\n Layers mark height=10, width=fill { Rectangle box height=10, width=fill }\n}\nView v {\n Column col spacing=4, halign=left, valign=top, width=100 {\n  Rectangle head height=10\n  repeat marks capacity=3 {\n   empty { Label none text=\"no marks\", font_size=10 }\n   Mark\n  }\n  Rectangle tail height=10\n }\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let marks = page.find("marks").expect("the repeat node");
    let mut rt = Runtime::new(&page);
    let lay = |page: &gogo::Page, rt: &Runtime| {
        let res = resolve(page, rt);
        layout(page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance)
    };
    let r = lay(&page, &rt);
    let tail = (page.find("tail").unwrap()).index();
    let none = (page.find("none").unwrap()).index();
    assert_eq!(r[tail].y, 10.0 + 4.0 + 12.5 + 4.0, "at zero the `empty` label holds the slot");
    assert!(r[none].h > 0.0);
    rt.set_count(&mut page, marks, 2);
    let items = rt.items(&page, marks);
    assert_eq!(items.len(), 2, "the third slot stays parked until the count wants it");
    assert_eq!(page.str(page.nodes[items[1].index()].name), "marks/1/item");
    assert!(page.find("marks/1/item/box").is_some(), "parts are addressable");
    let r = lay(&page, &rt);
    assert_eq!(r[tail].y, 10.0 + 4.0 + 10.0 + 4.0 + 10.0 + 4.0, "two items, spaced like siblings");
    assert_eq!(r[items[0].index()].y, 14.0);
    assert_eq!(r[items[1].index()].y, 28.0);
    let res = resolve(&page, &rt);
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
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let mut touch = |phase: Phase, x: f32, y: f32, t: f64, rt: &mut Runtime| input.touch(&page, rt, f.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 });
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
        let f = KeptFrame::fresh(page, resolve(page, rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
        gogo::draw(page, f.frame())
            .iter()
            .filter_map(|d| if let gogo::Draw::Quad { rect, .. } = d { Some(rect.x as i32) } else { None })
            .collect::<Vec<i32>>()
            .iter()
            .map(|x| if *x == 0 { a } else { b })
            .collect()
    };
    let hit_at = |page: &gogo::Page, rt: &Runtime, x: f32| {
        let f = KeptFrame::fresh(page, resolve(page, rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
        gogo::draw::hit(page, f.frame(), x, 10.0, false)
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

fn dcodes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

/// GP32 (scoped styles): a widget's own `style` block dresses its body's
/// nodes and SHADOWS a page style of the same name — and stays invisible
/// outside: the label at the page level wears the page `tone`.
#[test]
fn widget_scoped_styles_shadow_and_stay_inside() {
    let src = "style tone font_size=20\nwidget Card {\n style tone font_size=10\n Layers body { Label t .tone text=\"x\" }\n}\nView v {\n Card c\n Label out .tone text=\"y\"\n}\n";
    let c = ok(src);
    let page = &c.page;
    let rt = Runtime::new(page);
    let res = resolve(page, &rt);
    let t_id = page.find("c/t").expect("c/t");
    let out_id = page.find("out").expect("out");
    assert_eq!(res.num(page, t_id, PropKey::FontSize), Some(10.0), "inside the body the scoped style wins");
    assert_eq!(res.num(page, out_id, PropKey::FontSize), Some(20.0), "outside, the page style");
}

/// GP32: a scoped-only style is unknown outside its widget (the scope is
/// popped with the body), and a scoped style carrying `for=` refuses —
/// `for=` styles are top-level declarations.
#[test]
fn scoped_styles_stay_scoped_and_take_no_for() {
    let outside = dcodes("widget Card {\n style tone font_size=10\n Layers body { Label t .tone text=\"x\" }\n}\nView v {\n Card c\n Label out .tone text=\"y\"\n}\n");
    assert_eq!(outside.iter().filter(|c| *c == "G5002").count(), 1, "only the OUTSIDE wearer errors: {outside:?}");
    let for_style = dcodes("widget Card {\n style s for=Button, background=red\n Layers body { Rectangle r 4x4 }\n}\nView v { Card c }\n");
    assert!(for_style.contains(&"G3006".to_string()), "{for_style:?}");
}

/// GP32 (widget-scope transition): a `for=` widget's `transition` rule
/// times the HOSTING Button's flips — the rule lands on the Button node,
/// where the state actually changes.
#[test]
fn a_host_widgets_transition_times_the_hosts_flip() {
    let src = "widget Face for=Button {\n Layers skin { Rectangle dot 4x4 }\n transition base -> held duration=0.5\n}\nView v {\n Button b { Face f }\n}\n";
    let c = ok(src);
    let b = c.page.find("b").expect("b");
    assert!(
        c.page.transitions.iter().any(|t| t.node == b && (t.duration - 0.5).abs() < 1e-6),
        "the rule watches the host: {:?}",
        c.page.transitions
    );
}

/// GP32 (control transition): a `control` declaration's `transition` is
/// its default feel — endpoint-less, the weakest rule on each instance's
/// root; endpoints there refuse.
#[test]
fn a_controls_transition_is_its_default_feel() {
    let src = "control Pad {\n Layers body { Rectangle r 4x4 }\n transition duration=0.15\n}\nView v {\n Pad p\n}\n";
    let c = ok(src);
    let p = c.page.find("p").expect("p");
    assert!(
        c.page.transitions.iter().any(|t| t.node == p && t.from.is_none() && t.to.is_none() && (t.duration - 0.15).abs() < 1e-6),
        "{:?}",
        c.page.transitions
    );
    let endpoints = dcodes("control Pad {\n Layers body { Rectangle r 4x4 }\n transition base -> held duration=0.1\n}\nView v { Pad p }\n");
    assert!(endpoints.contains(&"G0034".to_string()), "{endpoints:?}");
}

/// GP32 (slot lanes): a control's own anim drives its slot region's
/// motion channel — `slot.thumb.translate.y` moves the region.
#[test]
fn slot_lanes_move_the_region_in_a_controls_own_anim() {
    let src = "control Fader {\n Layers body { slot thumb { Rectangle tt 4x4 } }\n anim pop { slot.thumb.translate.y from=-3, to=0, duration=1 }\n}\nView v {\n Fader f\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let thumb = (page.find("f/thumb").expect("f/thumb")).index();
    let pop = page.intern_lookup("f/pop");
    assert!(pop != gogo::page::NO_STR, "the instance's own anim");
    let mut rt = Runtime::new(&page);
    rt.play_id(&page, pop, Finish::Nothing);
    rt.tick(&mut page, 0.25);
    assert!(rt.motion[thumb].translate[1] < -1.0, "the slot region rides the lane: {:?}", rt.motion[thumb]);
}

/// GP32 (slot-lane fences): outside a control's own anims the lane
/// refuses (a widget's too — internals stay invisible); a non-motion word
/// refuses; a missing slot names the control's slots.
#[test]
fn slot_lanes_refuse_outside_and_non_motion() {
    let page_scope = dcodes("View v {\n Rectangle a 4x4\n}\nanim pop { slot.a.translate.y to=0, duration=1 }\n");
    assert!(page_scope.contains(&"G0063".to_string()), "{page_scope:?}");
    let widget_scope = dcodes("widget W {\n Layers body { slot s { Rectangle r 4x4 } }\n anim pop { slot.s.opacity to=0, duration=1 }\n}\nView v { W w }\n");
    assert!(widget_scope.contains(&"G0063".to_string()), "{widget_scope:?}");
    let non_motion = dcodes("control Fader {\n Layers body { slot thumb { Rectangle tt 4x4 } }\n anim pop { slot.thumb.width to=0, duration=1 }\n}\nView v { Fader f }\n");
    assert!(non_motion.contains(&"G0063".to_string()), "{non_motion:?}");
    let missing = dcodes("control Fader {\n Layers body { slot thumb { Rectangle tt 4x4 } }\n anim pop { slot.nope.opacity to=0, duration=1 }\n}\nView v { Fader f }\n");
    assert!(missing.contains(&"G0060".to_string()), "{missing:?}");
}
