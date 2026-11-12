//! Pins for E1 — one property, one writer (docs/todo-gogo-parity.md
//! GPAR1, GPAR2, GPAR3, GPAR54, GPAR62; decided 2026-08-31): an anim
//! writes the SAME opacity a declaration or a `when` writes (no multiply),
//! `color.a` fades the fill alone, `stop(restore)` returns to the ladder,
//! an anim outranks a `when` while it plays and its end pose stands until
//! the next writer, the D63 one-writer guard (G4003) refuses the static
//! conflict at build, and `modulate.a` is not a gogo word.
use gogo_ui::anim::{Channel, Finish};
use gogo_ui::KeptFrame;
use gogo_ui::draw::{Draw, draw};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{PropKey, Value};
use gogo_ui::runtime::{HELD, Runtime, resolve, resolve_animated};
use gogo_ui::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn anim_id(page: &Page, name: &str) -> gogo_ui::page::StrId {
    page.anim_heads.iter().map(|h| h.anim).find(|a| page.str(*a) == name).unwrap_or_else(|| panic!("no anim {name}"))
}

/// The alpha the draw list paints for `node`'s own plate (its Quad); 0 when
/// the list has no plate for it — a plate faded all the way out draws
/// nothing and is left out (tests/draws_nothing.rs).
fn plate_alpha(page: &Page, rt: &mut Runtime, node: &str) -> u8 {
    let f = KeptFrame::fresh(page, resolve_animated(page, rt), Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let rects = f.rects();
    let id = page.find(node).unwrap();
    let list = draw(page, f.frame());
    list.iter()
        .find_map(|d| match d {
            Draw::Quad { rect, color, .. } if *rect == rects[id.index()] => Some(color.0[3]),
            _ => None,
        })
        .unwrap_or(0)
}

fn about(alpha: u8, fraction: f32) -> bool {
    (alpha as i32 - (255.0 * fraction).round() as i32).abs() <= 1
}

/// GPAR1: `opacity=0.5` + an anim to 1 reaches FULL opacity — the anim
/// writes the one opacity channel, it does not multiply the declaration
/// (the accent flash pulsed to 50 % before). While the anim plays it owns
/// the channel; when it ends its end pose stands.
#[test]
fn an_anim_assigns_opacity_instead_of_multiplying() {
    let c = ok("anim flash {\n r.opacity to=1, duration=0.2\n}\nView v {\n Rectangle r color=#FF0000FF, opacity=0.5, width=100, height=40, halign=left, valign=top\n}\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    { let a = plate_alpha(&page, &mut rt, "r"); assert!(about(a, 0.5), "declared: alpha {a}"); }
    rt.play_id(&page, anim_id(&page, "flash"), Finish::Nothing);
    let mut p = page.clone();
    rt.tick(&mut p, 0.1);
    let mid = plate_alpha(&page, &mut rt, "r");
    assert!(mid > 128 + 20 && mid < 255 - 20, "half way up from 0.5 toward 1, not toward 0.5×1: {mid}");
    rt.tick(&mut p, 0.2);
    { let a = plate_alpha(&page, &mut rt, "r"); assert!(about(a, 1.0), "the anim reaches 1, and the end pose stands: alpha {a}"); }
    assert!(!rt.is_playing_id(anim_id(&page, "flash")));
    let res = resolve(&page, &rt);
    assert!(res.anim_owns(page.find("r").unwrap(), Channel::Opacity), "a finished anim's end pose is held");
}

/// GPAR2: `scrim.color.a` in an anim moves the FILL's alpha; the node's
/// opacity stays untouched, and so does the text over it (the file-ops
/// dim landed at 0.69 × 0.69 before).
#[test]
fn color_a_fades_the_fill_not_the_node() {
    let c = ok("anim appear {\n scrim.color.a from=0, to=0.69, duration=0\n}\nView v {\n Layers box width=200, height=100, halign=left, valign=top {\n Rectangle scrim color=#000000B0, width=fill, height=fill\n Label l text=\"x\", font_size=12, halign=left, valign=top\n }\n}\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.play_id(&page, anim_id(&page, "appear"), Finish::Nothing);
    let mut p = page.clone();
    rt.tick(&mut p, 0.05);
    let f = KeptFrame::fresh(&page, resolve_animated(&page, &mut rt), Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let (res, rects) = (f.look(), f.rects());
    let list = draw(&page, f.frame());
    let scrim = page.find("scrim").unwrap();
    let plate = list.iter().find_map(|d| match d {
        Draw::Quad { rect, color, .. } if *rect == rects[scrim.index()] => Some(color.0[3]),
        _ => None,
    }).unwrap();
    assert!(about(plate, 0.69), "the fill's alpha is the anim's 0.69, not 0.69 × 0.69: {plate}");
    let text = list.iter().find_map(|d| match d {
        Draw::Text { color, .. } => Some(color.0[3]),
        _ => None,
    }).unwrap();
    assert_eq!(text, 255, "the text over the scrim keeps full opacity: {list:#?}");
    assert!(!res.anim_owns(scrim, Channel::Opacity) && res.anim_owns(scrim, Channel::ColorA));
}

/// GPAR3 + GPAR54: `stop(restore)` puts back what the ladder says without
/// the anim — the declared 0.7 with nothing held, the `when held` 0.5 while
/// that state holds — even after a multi-track chain and a re-play.
#[test]
fn stop_restore_returns_to_the_ladder() {
    let c = ok("anim chain {\n b.opacity from=1, to=0.2, duration=0.1\n then b.opacity to=0.8, duration=0.1\n then b.opacity to=0, duration=0.1\n}\nView v {\n Button b text=\"x\", background=#FF0000FF, opacity=0.7, width=100, height=40, halign=left, valign=top { when held opacity=0.5 }\n}\n");
    let page = c.page;
    let chain = anim_id(&page, "chain");
    let mut rt = Runtime::new(&page);
    let mut p = page.clone();
    rt.play_id(&page, chain, Finish::Nothing);
    rt.tick(&mut p, 0.25);
    assert!(rt.is_playing_id(chain));
    rt.stop_id(chain, true);
    { let a = plate_alpha(&page, &mut rt, "b"); assert!(about(a, 0.7), "back to the declared 0.7 — not the first track's pinned from=1, not the anim's current value: alpha {a}"); }
    // A held state is the ladder now: restore lands there.
    let b = page.find("b").unwrap();
    rt.set_state(b, HELD, true);
    let _ = resolve_animated(&page, &mut rt);
    rt.play_id(&page, chain, Finish::Nothing);
    rt.tick(&mut p, 0.15);
    rt.play_id(&page, chain, Finish::Nothing);
    rt.tick(&mut p, 0.05);
    rt.stop_id(chain, true);
    { let a = plate_alpha(&page, &mut rt, "b"); assert!(about(a, 0.5), "the `when held` 0.5 is what the ladder says: alpha {a}"); }
}

/// An anim outranks a standing `when` while it plays (both write the one
/// scale channel; today the `when` overwrote the anim every frame), its
/// end pose stands after it finishes, and the next writer of that
/// property — the `when` flipping — takes the channel back.
#[test]
fn an_anim_outranks_a_when_while_it_plays_and_holds_until_the_next_writer() {
    let c = ok("anim pop {\n b.scale to=1.2, duration=0.2\n}\nView v {\n Button b text=\"x\", width=100, height=40, halign=left, valign=top { when held scale=0.9 }\n}\n");
    let page = c.page;
    let b = page.find("b").unwrap();
    let pop = anim_id(&page, "pop");
    let mut rt = Runtime::new(&page);
    let mut p = page.clone();
    rt.set_state(b, HELD, true);
    let res = resolve_animated(&page, &mut rt);
    assert_eq!(res.motion[b.index()].scale, [0.9, 0.9], "the when holds 0.9");
    rt.play_id(&page, pop, Finish::Nothing);
    rt.tick(&mut p, 0.1);
    let res = resolve_animated(&page, &mut rt);
    assert!(res.motion[b.index()].scale[0] > 0.95, "the anim owns scale while it plays (from 0.9 toward 1.2): {:?}", res.motion[b.index()]);
    rt.tick(&mut p, 0.2);
    let res = resolve_animated(&page, &mut rt);
    assert!((res.motion[b.index()].scale[0] - 1.2).abs() < 0.001, "the end pose stands after the anim ends: {:?}", res.motion[b.index()]);
    // The next writer: the `when held` flips off — its scale response is
    // that property's writer, so the held end yields to the ladder (1.0).
    rt.set_state(b, HELD, false);
    let res = resolve_animated(&page, &mut rt);
    assert_eq!(res.motion[b.index()].scale, [1.0, 1.0], "the flip took the channel back: {:?}", res.motion[b.index()]);
    // A code write is a writer too.
    rt.play_id(&page, pop, Finish::Nothing);
    rt.tick(&mut p, 0.3);
    rt.set_prop(b, PropKey::Scale, Value::VNum(1.0));
    let res = resolve_animated(&page, &mut rt);
    assert_eq!(res.motion[b.index()].scale, [1.0, 1.0], "{:?}", res.motion[b.index()]);
}

/// Spec, Handoff law: a FINISHED anim's held end pose is a standing value
/// the author chose — a re-play captures it as its base, so `stop(restore)`
/// after the re-play returns to it, not to the declaration.
#[test]
fn a_held_end_pose_is_the_base_of_a_replay() {
    let c = ok("anim vanish {\n r.opacity to=0, duration=0.1\n}\nView v {\n Rectangle r color=#FF0000FF, width=100, height=40, halign=left, valign=top\n}\n");
    let page = c.page;
    let vanish = anim_id(&page, "vanish");
    let mut rt = Runtime::new(&page);
    let mut p = page.clone();
    rt.play_id(&page, vanish, Finish::Nothing);
    rt.tick(&mut p, 0.2);
    { let a = plate_alpha(&page, &mut rt, "r"); assert!(about(a, 0.0), "faded out and held: alpha {a}"); }
    rt.play_id(&page, vanish, Finish::Nothing);
    rt.tick(&mut p, 0.05);
    rt.stop_id(vanish, true);
    { let a = plate_alpha(&page, &mut rt, "r"); assert!(about(a, 0.0), "restore returns to the held end pose, not to the declared 1: alpha {a}"); }
}

/// GPAR62 / D63 (Godot G4003): a `when` holding a motion word on a target
/// while a `transition … anim=` in the same scope drives the same channel
/// on it is refused at build — node scope and view scope alike; a
/// different property, or a `when` in another scope, is fine.
#[test]
fn one_writer_guard_refuses_a_when_and_a_transition_anim_on_one_channel() {
    let node = "anim press {\n b.scale to=0.8, duration=0.1\n}\nView v {\n Button b text=\"x\" { when held scale=0.9; transition any -> held anim=press }\n}\n";
    assert!(codes(node).contains(&"G4003".to_string()), "{:?}", codes(node));
    let view = "arrangement wide\nanim spread {\n bar.translate.x to=40, duration=0.1\n}\ntransition any -> wide anim=spread\nwhen wide { bar.translate.x = 20 }\nView v {\n Rectangle bar width=10, height=10\n}\n";
    assert!(codes(view).contains(&"G4003".to_string()), "{:?}", codes(view));
    let other_prop = "anim press {\n b.translate.x to=8, duration=0.1\n}\nView v {\n Button b text=\"x\" { when held scale=0.9; transition any -> held anim=press }\n}\n";
    assert!(!codes(other_prop).contains(&"G4003".to_string()), "{:?}", codes(other_prop));
    let other_scope = "arrangement wide\nanim press {\n b.scale to=0.8, duration=0.1\n}\nwhen wide { b.scale = 0.9 }\nView v {\n Button b text=\"x\" { transition any -> held anim=press }\n}\n";
    assert!(!codes(other_scope).contains(&"G4003".to_string()), "a view-scope when and a node-scope transition are different scopes: {:?}", codes(other_scope));
}

/// `modulate.a` is a Godot engine word, not a gogo one: refused on a node,
/// in a `when`-less anim lane — naming `opacity` — and `color.a` stays the
/// fill's alpha.
#[test]
fn modulate_a_is_not_a_word() {
    let lane = codes("anim a {\n r.modulate.a to=0, duration=0.1\n}\nView v { Rectangle r }\n");
    assert!(lane.contains(&"G0075".to_string()), "{lane:?}");
    let prop = codes("View v { Rectangle r modulate.a=0.5 }");
    assert!(prop.contains(&"G0075".to_string()), "{prop:?}");
    assert!(!codes("anim a {\n r.color.a to=0, duration=0.1\n}\nView v { Rectangle r color=#FF0000FF }\n").iter().any(|c| c == "G0075" || c == "G0063"));
}
