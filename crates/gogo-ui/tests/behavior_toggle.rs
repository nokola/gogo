//! GP53 (user-approved 2026-09-06): `behavior toggle by $value in
//! $min..$max` — the stock toggle behavior (the addon's
//! toggle_behavior.gd) behind the effect pages' paint-target switch
//! (todo-bugs.md #4: its `script` line was dropped and the switch was
//! deaf). A TAP glides the value to the other end; a DRAG past the
//! wobble puts the thumb under the finger and settles to the nearer end;
//! Enter/Space flip, arrows are sides; `selected` follows the value
//! live; a write while the finger holds the control lands DIRECT, the
//! release's settle glides by the control's feel.

use gogo::compile;
use gogo::KeptFrame;
use gogo::input::{Event, EventKind, Input, Key, Mods, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect};
use gogo::page::{NodeId, Page, PropKey, Value};
use gogo::runtime::{Resolved, Runtime, SELECTED, resolve, resolve_animated};

/// The shipped HorizontalSwitch's shape: a 120x60 surface, the plate a
/// 60-wide thumb riding the full-width track, a 0.3 s feel.
const SWITCH: &str = "control Switch $value=0, $min=0, $max=1 {\n    behavior toggle by $value in $min..$max\n    state_builtin hovered, focused, disabled, selected, held\n    signal drag_started, drag_ended\n    transition duration=0.3, easing=cubic_out\n    Layers width=120, height=60 {\n        slot track width=fill, height=fill {\n            slot thumb automatic=position by $value in $min..$max, height=fill { Rectangle width=60, height=fill, color=#1BA1E2FF }\n        }\n    }\n}\n";

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    /// The frame the last touch or key saw (laid out fresh for each).
    kept: KeptFrame,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = compile(src, &|_| None);
        assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        Fx { page, rt, input: Input::default(), kept: f }
    }

    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        self.kept = KeptFrame::fresh(&self.page, self.res(), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.touch(&self.page, &mut self.rt, self.kept.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 })
    }

    fn key(&mut self, k: Key) -> Vec<Event> {
        self.kept = KeptFrame::fresh(&self.page, self.res(), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, self.kept.frame(), k, Mods::NONE)
    }

    fn value(&self, node: NodeId) -> f32 {
        let res = self.res();
        res.num(&self.page, node, PropKey::Value).unwrap_or(f32::NAN)
    }

    fn res(&self) -> Resolved {
        resolve(&self.page, &self.rt)
    }

    /// The thumb slot's shown fraction after the frame's arming pass
    /// (`resolve_animated` starts or drops the glide).
    fn thumb_shown(&mut self) -> f32 {
        let thumb = self.page.find("s/thumb").unwrap();
        let res = resolve_animated(&self.page, &mut self.rt);
        res.auto.iter().find(|(n, _)| *n == thumb).map(|(_, f)| *f).expect("the thumb is driven")
    }
}

fn edits(ev: &[Event]) -> Vec<f32> {
    ev.iter().filter_map(|e| if let EventKind::Edit(Value::VNum(v)) = e.kind { Some(v) } else { None }).collect()
}

fn changes(ev: &[Event]) -> Vec<bool> {
    ev.iter().filter_map(|e| if let EventKind::Change(Value::VBool(v)) = e.kind { Some(v) } else { None }).collect()
}

fn signals(page: &Page, ev: &[Event]) -> Vec<String> {
    ev.iter().filter_map(|e| match (&e.kind, &e.action) { (EventKind::Signal, Some(gogo::page::Action::Emit(id))) => Some(page.str(*id).to_string()), _ => None }).collect()
}

/// A tap anywhere on the switch flips it to the other end (shipped
/// Grid_MouseLeftButtonDown: the WHOLE control is one target), reports
/// `change(bool)` once, and the declared edges bracket it — started on
/// the press, ended after the commit.
#[test]
fn a_tap_flips_to_the_other_end_and_reports_the_side() {
    let mut fx = Fx::new(&format!("{SWITCH}View v {{ Switch s }}"));
    let s = fx.page.find("s").unwrap();
    let ev = fx.touch(Phase::Down, 30.0, 30.0, 0.0);
    assert_eq!(signals(&fx.page, &ev), vec!["drag_started"], "{ev:?}");
    assert!(edits(&ev).is_empty(), "a toggle's press writes nothing: {ev:?}");
    let ev = fx.touch(Phase::Up, 30.0, 30.0, 0.1);
    assert_eq!(changes(&ev), vec![true], "{ev:?}");
    assert_eq!(signals(&fx.page, &ev), vec!["drag_ended"], "{ev:?}");
    assert!(matches!(ev.last().unwrap().kind, EventKind::Signal), "the commit lands before the closing edge: {ev:?}");
    assert_eq!(fx.value(s), 1.0);
    assert!(fx.rt.has(s, SELECTED), "`selected` derives from the value");
    // Tapping the RIGHT half flips back — the half tapped never matters.
    fx.touch(Phase::Down, 100.0, 30.0, 0.2);
    let ev = fx.touch(Phase::Up, 100.0, 30.0, 0.3);
    assert_eq!(changes(&ev), vec![false], "{ev:?}");
    assert_eq!(fx.value(s), 0.0);
    assert!(!fx.rt.has(s, SELECTED));
}

/// Finger wobble under the 6-epx threshold stays a tap; past it the
/// thumb follows the finger (thumb-width compensated over the track —
/// the 60 plate spans 0..60 of the 120 track), `selected` flips live
/// as it crosses the middle, and the release settles to the nearer end.
#[test]
fn a_drag_follows_the_finger_and_settles_nearest() {
    let mut fx = Fx::new(&format!("{SWITCH}View v {{ Switch s }}"));
    let s = fx.page.find("s").unwrap();
    fx.touch(Phase::Down, 30.0, 30.0, 0.0);
    let ev = fx.touch(Phase::Move, 33.0, 30.0, 0.05);
    assert!(edits(&ev).is_empty(), "under the wobble: {ev:?}");
    // x=75: fraction (75 - 30) / 60 = 0.75.
    let ev = fx.touch(Phase::Move, 75.0, 30.0, 0.1);
    assert_eq!(edits(&ev), vec![0.75], "{ev:?}");
    assert!(fx.rt.has(s, SELECTED), "past the middle: selected, live");
    // Back to x=45: 0.25 — and selected drops.
    let ev = fx.touch(Phase::Move, 45.0, 30.0, 0.2);
    assert_eq!(edits(&ev), vec![0.25], "{ev:?}");
    assert!(!fx.rt.has(s, SELECTED));
    let ev = fx.touch(Phase::Up, 45.0, 30.0, 0.3);
    assert!(changes(&ev).is_empty(), "settled on the side it started: no change: {ev:?}");
    assert_eq!(fx.value(s), 0.0, "a drag settles to the nearer end, not the other one");
    assert_eq!(signals(&fx.page, &ev), vec!["drag_ended"]);
    // A drag that ends past the middle settles on.
    fx.touch(Phase::Down, 30.0, 30.0, 1.0);
    fx.touch(Phase::Move, 80.0, 30.0, 1.1);
    let ev = fx.touch(Phase::Up, 80.0, 30.0, 1.2);
    assert_eq!(changes(&ev), vec![true], "{ev:?}");
    assert_eq!(fx.value(s), 1.0);
}

/// A cancelled drag restores the grabbed side (never leave a half-drag
/// behind) and still closes with `drag_ended`.
#[test]
fn a_cancel_restores_the_grabbed_side() {
    let mut fx = Fx::new(&format!("{SWITCH}View v {{ Switch s }}"));
    let s = fx.page.find("s").unwrap();
    fx.touch(Phase::Down, 30.0, 30.0, 0.0);
    fx.touch(Phase::Move, 90.0, 30.0, 0.1);
    assert_eq!(fx.value(s), 1.0);
    assert!(fx.rt.has(s, SELECTED));
    let ev = fx.touch(Phase::Cancel, 90.0, 30.0, 0.2);
    assert!(changes(&ev).is_empty(), "{ev:?}");
    assert_eq!(signals(&fx.page, &ev), vec!["drag_ended"]);
    assert_eq!(fx.value(s), 0.0);
    assert!(!fx.rt.has(s, SELECTED), "the derived state restores with the value");
}

/// Keys are the addon's key_action: Enter/Space flip, Right/Up/End =
/// on, Left/Down/Home = off; the wheel is up = on, down = off. Each
/// commits `change` directly when the side changed.
#[test]
fn keys_and_wheel_pick_sides() {
    let mut fx = Fx::new(&format!("{SWITCH}View v {{ Switch s }}"));
    let s = fx.page.find("s").unwrap();
    fx.key(Key::Tab);
    let ev = fx.key(Key::Enter);
    assert_eq!(changes(&ev), vec![true], "{ev:?}");
    let ev = fx.key(Key::Char(' '));
    assert_eq!(changes(&ev), vec![false], "{ev:?}");
    fx.key(Key::Right);
    assert_eq!(fx.value(s), 1.0);
    let ev = fx.key(Key::End);
    assert!(changes(&ev).is_empty(), "already on: no change");
    fx.key(Key::Left);
    assert_eq!(fx.value(s), 0.0);
    fx.key(Key::Up);
    assert_eq!(fx.value(s), 1.0);
    fx.key(Key::Home);
    assert_eq!(fx.value(s), 0.0);
    fx.kept = KeptFrame::fresh(&fx.page, fx.res(), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let ev = fx.input.wheel(&fx.page, &mut fx.rt, fx.kept.frame(), 60.0, 30.0, true);
    assert_eq!(changes(&ev), vec![true], "{ev:?}");
    assert!(signals(&fx.page, &ev).is_empty(), "the edges belong to the finger");
}

/// The feel: a write while the finger HOLDS the control lands direct
/// (the thumb stays under the finger), the release's settle glides by
/// the control's `transition` — the shipped 0.3 s CubicOut slide.
#[test]
fn a_held_write_lands_direct_and_the_settle_glides() {
    let mut fx = Fx::new(&format!("{SWITCH}View v {{ Switch s }}"));
    assert_eq!(fx.thumb_shown(), 0.0);
    fx.touch(Phase::Down, 30.0, 30.0, 0.0);
    fx.touch(Phase::Move, 75.0, 30.0, 0.1);
    assert_eq!(fx.thumb_shown(), 0.75, "mid-drag: under the finger, no glide");
    fx.touch(Phase::Move, 90.0, 30.0, 0.15);
    assert_eq!(fx.thumb_shown(), 1.0);
    fx.touch(Phase::Up, 90.0, 30.0, 0.2);
    // Now the tap path: the settle GLIDES from the plate's rest.
    fx.touch(Phase::Down, 30.0, 30.0, 1.0);
    fx.touch(Phase::Up, 30.0, 30.0, 1.1);
    let shown = fx.thumb_shown();
    assert_eq!(shown, 1.0, "the frame the settle lands, the plate still shows its rest — the glide owns the travel");
    fx.rt.tick(&mut fx.page, 0.15);
    let mid = fx.thumb_shown();
    assert!(mid > 0.0 && mid < 1.0, "half-way through the 0.3 s feel: {mid}");
    fx.rt.tick(&mut fx.page, 0.3);
    assert_eq!(fx.thumb_shown(), 0.0, "settled on the other side");
}

/// The fences: the toggle behavior reads `slot thumb` and owns all four
/// key groups; the addon's `script` line is refused, never dropped; a
/// contract without `selected` derives nothing.
#[test]
fn toggle_fences_and_the_script_refusal() {
    let fails = |src: &str| gogo::has_errors(&compile(src, &|_| None).diags);
    let no_thumb = "control C $value=0, $min=0, $max=1 {\n    behavior toggle by $value in $min..$max\n    state_builtin focused\n    Layers width=120, height=60 { slot track width=fill, height=fill { Rectangle width=60, height=fill, color=#FF0000FF } }\n}\nView v { C c }";
    assert!(fails(no_thumb), "no thumb slot");
    let keys = format!("{}", SWITCH.replace("state_builtin", "keys enter\n    state_builtin"));
    assert!(fails(&format!("{keys}View v {{ Switch s }}")), "the toggle behavior owns enter");
    let script = SWITCH.replace("behavior toggle by $value in $min..$max", "script \"res://addons/gogo/controls/toggle_behavior.gd\"");
    let c = compile(&format!("{script}View v {{ Switch s }}"), &|_| None);
    assert!(gogo::has_errors(&c.diags), "`script` is the addon's word — refused, not dropped");
    assert!(c.diags.iter().any(|d| d.code == "G0082" && d.message.contains("`behavior <name>")), "{:#?}", c.diags);
    // No `selected` in the contract: the value flips, no state does.
    let plain = SWITCH.replace("state_builtin hovered, focused, disabled, selected, held", "state_builtin focused");
    let mut fx = Fx::new(&format!("{plain}View v {{ Switch s }}"));
    let s = fx.page.find("s").unwrap();
    fx.touch(Phase::Down, 30.0, 30.0, 0.0);
    fx.touch(Phase::Up, 30.0, 30.0, 0.1);
    assert_eq!(fx.value(s), 1.0);
    assert!(!fx.rt.has(s, SELECTED));
}
