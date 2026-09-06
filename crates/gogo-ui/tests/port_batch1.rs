//! Pins for the 2026-08-30 port batch (docs/todo-gogo.md GP1–GP8, GP19,
//! GP20): composed `when` pairs, the closed screen vocabulary, the CSS
//! gradient compass with every stop drawn, WPF bounce numbers, anim
//! headers (`repeat=forever`, `speed=`, `autoreverse=`), `key` timing,
//! `rotate` in the draw list, `stop` and `take_finished`, entrances
//! under a gone ancestor.
use gogo::anim::Finish;
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{NO_NODE, PropKey, State, Value};
use gogo::runtime::{Runtime, resolve, resolve_animated};
use gogo::{Compiled, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

/// GP1: `when wide screen.lotsOfSpace` holds only while BOTH do; GP2: an
/// unknown screen word is a build error.
#[test]
fn composed_when_needs_both_halves() {
    let c = ok("arrangement wide\nView v {\n Panel p width=100 { when wide screen.lotsOfSpace { width = 300 } }\n}\n");
    let page = c.page;
    let p = page.find("p").unwrap();
    let mut rt = Runtime::new(&page);
    let width = |page: &gogo::Page, rt: &Runtime| resolve(page, rt).placement[p.index()].width;
    assert_eq!(width(&page, &rt), gogo::page::Len::Exact(100.0));
    rt.go_id(gogo::ArrangementId::new(0));
    assert_eq!(width(&page, &rt), gogo::page::Len::Exact(100.0), "the arrangement alone must not pose");
    rt.set_screen(&page, gogo::Orientation::Landscape, gogo::ScreenClass::LotsOfSpace);
    assert_eq!(width(&page, &rt), gogo::page::Len::Exact(300.0));
    rt.go_id(gogo::BASE);
    assert_eq!(width(&page, &rt), gogo::page::Len::Exact(100.0), "the class alone must not pose");
    assert!(codes("View v { Panel p { when screen.huge { width = 1 } } }").contains(&"G0036".to_string()));
    assert!(codes("View v { Panel p { when screen.landscape screen.portrait { width = 1 } } }").contains(&"G0038".to_string()));
    // A pair (two conditions) outranks every single; the singles rank
    // equal — between them the later line in the file wins (E2 flatten,
    // 2026-08-31; pinned in godot_arrangement.rs).
    assert!(State::ArrangementScreen(gogo::ArrangementId::new(0), gogo::StrId::new(0)).priority() > State::Arrangement(gogo::ArrangementId::new(0)).priority());
    assert_eq!(State::ScreenPair(gogo::StrId::new(0), gogo::StrId::new(0)).priority(), State::ArrangementScreen(gogo::ArrangementId::new(0), gogo::StrId::new(0)).priority());
    assert_eq!(State::Arrangement(gogo::ArrangementId::new(0)).priority(), State::ScreenClass(gogo::StrId::new(0)).priority());
    assert_eq!(State::ScreenClass(gogo::StrId::new(0)).priority(), State::ScreenOrientation(gogo::StrId::new(0)).priority());
}

/// Each Gradient entry with the stops that follow it: (angle, positions).
fn gradient_stops(draws: &[Draw]) -> Vec<(f32, Vec<f32>)> {
    let mut out: Vec<(f32, Vec<f32>)> = Vec::new();
    let mut i: usize = 0;
    while i < draws.len() {
        if let Draw::Gradient { angle, stops, .. } = draws[i] {
            let positions: Vec<f32> = draws[i + 1..i + 1 + stops as usize].iter().filter_map(|d| if let Draw::GradientStop { pos, .. } = d { Some(*pos) } else { None }).collect();
            out.push((angle, positions));
            i += stops as usize;
        }
        i += 1;
    }
    out
}

/// GP3 + GP4: three stops with an `@0.94` plateau draw as ONE gradient
/// carrying all three (the plateau holds), and the compass angle travels
/// as written.
#[test]
fn gradient_draws_every_stop_with_the_compass_angle() {
    let c = ok("View v {\n Gradient g width=100, height=20, angle=0, stops=[#FFFFFF6B@0, #FFFFFF6B@0.94, #FFFFFF00@1]\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let draws = draw(&page, &res, &rects);
    assert_eq!(gradient_stops(&draws), vec![(0.0, vec![0.0, 0.94, 1.0])]);
    // Evenly spread when no position is written; a lone stop is one flat band.
    let c = ok("View v {\n Gradient g width=100, height=20, stops=[#FF0000FF, #00FF00FF, #0000FFFF]\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    assert_eq!(gradient_stops(&draw(&page, &res, &rects)), vec![(90.0, vec![0.0, 0.5, 1.0])]);
}

/// GP6 + GP7: a `repeat=forever` anim is still moving past its length and
/// replays its chain; `speed=2` halves the time; a `jump` key holds the
/// previous value until its own `t`; `autoreverse` comes home.
#[test]
fn anim_headers_and_key_timing() {
    let src = "View v {\n Rectangle a width=10, height=10\n Rectangle b width=10, height=10\n Rectangle c width=10, height=10\n}\n\
        anim blink repeat=forever {\n a.opacity {\n  key t=0, v=0, jump=true\n  key t=0.1, v=1, jump=true\n  key t=0.3, v=0, jump=true\n }\n}\n\
        anim fast speed=2 {\n b.translate.x from=0, to=100, duration=1\n}\n\
        anim pop {\n c.scale.x to=2, duration=0.5, autoreverse=true\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let a = (page.find("a").unwrap()).index();
    let b = (page.find("b").unwrap()).index();
    let cc = (page.find("c").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let blink = page.intern_lookup("blink");
    rt.play_id(&page, blink, Finish::Nothing);
    assert!(rt.tick(&mut page, 0.05));
    assert_eq!(rt.motion[a].opacity, 0.0, "a jump key holds the previous value until its own t");
    rt.tick(&mut page, 0.1);
    assert_eq!(rt.motion[a].opacity, 1.0);
    rt.tick(&mut page, 0.2);
    assert_eq!(rt.motion[a].opacity, 0.0);
    assert!(rt.tick(&mut page, 5.0), "forever: still moving long past the chain's length");
    assert!(rt.is_playing_id(blink));
    // The clock wrapped: 5.35 s into a 0.3 s loop is 0.25 → 1; 0.1 later is 0.05 → 0.
    assert_eq!(rt.motion[a].opacity, 1.0);
    rt.tick(&mut page, 0.1);
    assert_eq!(rt.motion[a].opacity, 0.0);

    let fast = page.intern_lookup("fast");
    rt.play_id(&page, fast, Finish::Nothing);
    rt.tick(&mut page, 0.25);
    assert!((rt.motion[b].translate[0] - 50.0).abs() < 1e-3, "speed=2: half way after a quarter second");

    let pop = page.intern_lookup("pop");
    rt.play_id(&page, pop, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    assert!((rt.motion[cc].scale[0] - 2.0).abs() < 1e-3);
    rt.tick(&mut page, 0.5);
    assert!((rt.motion[cc].scale[0] - 1.0).abs() < 1e-3, "autoreverse returns home");
    assert!(!rt.is_playing_id(pop));
}

/// GP5: the crop page's `bounces=1, bounciness=2.4` reach the track as
/// one easing word; a parameter the curve does not take is refused.
#[test]
fn easing_parameters_travel_with_the_easing() {
    let c = ok("View v { Rectangle a width=10, height=10 }\nanim show { a.translate.y from=-50, to=0, duration=1.1, easing=bounce_out, bounces=1, bounciness=2.4 }\n");
    let word = c.page.str(c.page.anims[0].easing).to_string();
    assert_eq!(word, "bounce_out(1,2.4)");
    assert!(codes("View v { Rectangle a width=10, height=10 }\nanim show { a.opacity to=1, duration=1, easing=cubic_out, bounces=1 }\n").contains(&"G0065".to_string()));
    assert!(codes("View v { Rectangle a width=10, height=10 }\nanim show { a.opacity to=1, duration=1, easing=nope }\n").contains(&"G0065".to_string()));
    // Transitions take the same words. (The rule sits on the node — a
    // control state's transitions live inside the style/node block, the
    // GP31 scope fence.)
    let c = ok("View v { Rectangle a width=10, height=10 { when held opacity=0.5\n transition any -> held duration=0.3, easing=elastic_out, oscillations=2, springiness=4 } }\n");
    assert_eq!(c.page.str(c.page.transitions[0].easing), "elastic_out(2,4)");
    assert!(codes("View v { Rectangle a width=10, height=10 }\nanim x repeat=twice { a.opacity to=1, duration=1 }\n").contains(&"G0066".to_string()));
}

/// GP8: a `when` rotate response and a rotate anim reach the draw list as
/// a `RotatePush` about the node's center.
#[test]
fn rotate_reaches_the_draw_list() {
    let c = ok("arrangement flipped\nView v {\n Rectangle arrow width=20, height=10, halign=left, valign=top\n}\nwhen flipped { arrow.rotate = 180 }\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.go_id(gogo::ArrangementId::new(0));
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 100.0, 100.0), &FixedAdvance);
    let draws = draw(&page, &res, &rects);
    let push = draws.iter().find_map(|d| if let Draw::RotatePush { angle, cx, cy } = d { Some((*angle, *cx, *cy)) } else { None });
    assert_eq!(push, Some((180.0, 10.0, 5.0)));
    assert!(draws.iter().any(|d| *d == Draw::RotatePop));
}

/// GP20: `stop` with restore puts the channel back where the play found
/// it and does not run the finish action; GP19: no entrance plays for a
/// node under a `gone` ancestor.
#[test]
fn stop_restores_and_entrances_respect_gone_ancestors() {
    let c = ok("View v {\n Rectangle a width=10, height=10\n Panel hidden visibility=gone { Rectangle inner width=10, height=10, on:show=play(enter) }\n}\nanim flash repeat=forever { a.opacity to=0.2, duration=1 }\nanim enter { inner.opacity from=0, to=1, duration=1 }\n");
    let mut page = c.page;
    let a = (page.find("a").unwrap()).index();
    let inner = (page.find("inner").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let flash = page.intern_lookup("flash");
    rt.play_id(&page, flash, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    assert!((rt.motion[a].opacity - 0.6).abs() < 1e-3);
    rt.stop_id(flash, true);
    assert_eq!(rt.motion[a].opacity, 1.0, "restored to the pre-play value");
    assert!(!rt.is_playing_id(flash));
    assert!(!rt.tick(&mut page, 0.1));

    rt.play_entrances(&page);
    let enter = page.intern_lookup("enter");
    assert!(!rt.is_playing_id(enter), "no entrance under a gone ancestor");
    assert_eq!(rt.motion[inner].opacity, 1.0);
    let _ = (NO_NODE, PropKey::Opacity, Value::VNum(0.0), resolve_animated);
}

/// The ruler behind `when screen.<class>`: a 320×680 phone is
/// portraitSmall, 1100×800 is lotsOfSpace, a phone on its side is
/// landscapeSmall — and a sideways screen grades by HEIGHT, so a squat
/// 700×500 window is still small.
#[test]
fn screen_classes_grade_view_sizes() {
    use gogo::{Orientation, ScreenClass};
    assert_eq!(ScreenClass::from_view(320.0, 680.0), ScreenClass::PortraitSmall);
    assert_eq!(ScreenClass::from_view(360.0, 740.0), ScreenClass::PortraitMedium);
    assert_eq!(ScreenClass::from_view(600.0, 900.0), ScreenClass::PortraitLarge);
    assert_eq!(ScreenClass::from_view(680.0, 320.0), ScreenClass::LandscapeSmall);
    assert_eq!(ScreenClass::from_view(700.0, 500.0), ScreenClass::LandscapeSmall);
    assert_eq!(ScreenClass::from_view(1100.0, 800.0), ScreenClass::LotsOfSpace);
    assert_eq!(Orientation::from_view(680.0, 320.0), Orientation::Landscape);
    assert_eq!(Orientation::from_view(320.0, 320.0), Orientation::Portrait, "a square screen is portrait");
}

/// GP20: an anim that runs out lands in `take_finished` exactly once, so
/// a host can sequence on its end; `stop()` never reports (WPF
/// Storyboard.Stop skips Completed).
#[test]
fn finished_anims_reach_the_host_once() {
    let c = ok("View v {\n Rectangle a 10x10\n}\nanim fade { a.opacity to=0, duration=0.2 }\nanim blink { a.scale to=0.5, duration=0.2 }\n");
    let mut page = c.page;
    let fade = page.intern_lookup("fade");
    let blink = page.intern_lookup("blink");
    let mut rt = Runtime::new(&page);
    rt.play_id(&page, fade, Finish::Nothing);
    rt.tick(&mut page, 0.1);
    assert!(rt.take_finished().is_empty(), "mid-flight is not finished");
    rt.tick(&mut page, 0.2);
    let done = rt.take_finished();
    assert_eq!(done.iter().map(|a| page.str(*a)).collect::<Vec<_>>(), ["fade"]);
    assert!(rt.take_finished().is_empty(), "drained");
    rt.play_id(&page, blink, Finish::Nothing);
    rt.stop_id(blink, true);
    rt.tick(&mut page, 1.0);
    assert!(rt.take_finished().is_empty(), "a stopped anim never reports");
}

/// GPAR53: `show(x, true)` replays the entrance only on a REAL flip (the
/// shipped visibility_changed gate) — an overlay calling it per frame
/// settles instead of restarting; a fresh flip after a hide plays again.
#[test]
fn show_replays_the_entrance_only_on_a_real_flip() {
    let c = ok("anim pop { p.opacity from=0, to=1, duration=1, easing=linear }\nView v { Layers p width=10, height=10, visibility=gone, on:show=play(pop) }");
    let mut page = c.page;
    let p = page.find("p").unwrap();
    let mut rt = Runtime::new(&page);
    rt.show(&mut page, p, true);
    rt.tick(&mut page, 0.5);
    rt.show(&mut page, p, true);
    rt.tick(&mut page, 0.25);
    let r = resolve_animated(&page, &mut rt);
    let shown = r.num(&page, p, PropKey::Opacity).unwrap_or(1.0);
    assert!((shown - 0.75).abs() < 1e-3, "the second show did not restart the entrance: {shown}");
    rt.show(&mut page, p, false);
    rt.show(&mut page, p, true);
    rt.tick(&mut page, 0.1);
    let r = resolve_animated(&page, &mut rt);
    let replay = r.num(&page, p, PropKey::Opacity).unwrap_or(1.0);
    assert!((replay - 0.1).abs() < 1e-3, "a real flip replays from the start: {replay}");
}
