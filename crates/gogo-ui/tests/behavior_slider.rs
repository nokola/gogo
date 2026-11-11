//! Ported from ui/slider_phone_behavior.gd
//!
//! GP52 (user-approved 2026-09-03): `behavior slider by $value in
//! $min..$max, step=$step, key_unit=5%` — the contract names the input
//! behavior that answers a `control`. The behavior is the stock slider
//! law (the addon's slider_behavior.gd): a press anywhere on the
//! instance jumps the value and streams `edit`s over bare x/track-width
//! (shipped SliderPhone drew no thumb — UpdatePercent was x/width
//! verbatim), release commits `change` when the value actually changed,
//! arrows/Home/End/wheel step it. `key_unit=5%` is the
//! slider_phone_behavior override: ±5% of the range per tick on a
//! CONTINUOUS slider (the stock 1.0-when-step-0 unit would jump a 0..1
//! effect slider end to end); a stepped instance steps by `step=`.

use gogo_ui::compile;
use gogo_ui::KeptFrame;
use gogo_ui::input::{Event, EventKind, Input, Key, Mods, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{NodeId, Page, PropKey, Value};
use gogo_ui::runtime::{DISABLED, Resolved, Runtime, resolve};

const SLIDER_PHONE: &str = "control SliderPhone $value=3, $min=0, $max=10, $step=0 {\n    behavior slider by $value in $min..$max, step=$step, key_unit=5%\n    state_builtin hovered, focused, disabled, held\n    signal drag_started, drag_ended\n    Layers width=fill, height=60 {\n        slot track height=11, valign=bottom, margin_left=18, margin_right=18, margin_bottom=16 {\n            Rectangle width=fill, height=fill, color=#FFFFFF33\n            slot progress automatic=length by $value in $min..$max { Rectangle width=fill, height=fill, color=#1BA1E2FF }\n        }\n        Label lab text=$value\n    }\n}\n";

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
        assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        Fx { page, rt, input: Input::default(), kept: f }
    }

    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        self.kept = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.touch(&self.page, &mut self.rt, self.kept.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 })
    }

    fn key(&mut self, k: Key) -> Vec<Event> {
        self.kept = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, self.kept.frame(), k, Mods::NONE)
    }

    fn value(&self, node: NodeId) -> f32 {
        let res = resolve(&self.page, &self.rt);
        res.num(&self.page, node, PropKey::Value).unwrap_or(f32::NAN)
    }

    fn res(&self) -> Resolved {
        resolve(&self.page, &self.rt)
    }
}

fn edits(ev: &[Event]) -> Vec<f32> {
    ev.iter().filter_map(|e| if let EventKind::Edit(Value::VNum(v)) = e.kind { Some(v) } else { None }).collect()
}

fn changes(ev: &[Event]) -> Vec<f32> {
    ev.iter().filter_map(|e| if let EventKind::Change(Value::VNum(v)) = e.kind { Some(v) } else { None }).collect()
}

fn signals(page: &Page, ev: &[Event]) -> Vec<String> {
    ev.iter().filter_map(|e| match (&e.kind, &e.action) { (EventKind::Signal, Some(gogo_ui::page::Action::Emit(id))) => Some(page.str(*id).to_string()), _ => None }).collect()
}

/// The whole 60-epx row is the gesture surface (shipped rootGrid
/// captured the mouse): a press OUTSIDE the track still jumps the value,
/// the math is bare x over the track's width, edits stream while
/// dragging, and the release commits one `change`.
#[test]
fn a_press_anywhere_jumps_streams_edits_and_commits_change() {
    let mut fx = Fx::new(&format!("{SLIDER_PHONE}View v {{ SliderPhone s value=2 }}"));
    let s = fx.page.find("s").unwrap();
    // Track: x 18..382 (width 364). x=200 is the exact middle -> 5.0.
    let ev = fx.touch(Phase::Down, 200.0, 30.0, 0.0);
    assert_eq!(edits(&ev), vec![5.0], "{ev:?}");
    assert_eq!(signals(&fx.page, &ev), vec!["drag_started"], "the opening edge rides the press: {ev:?}");
    assert!(matches!(ev[0].kind, EventKind::Signal), "started BEFORE the first edit (emit, then apply): {ev:?}");
    assert_eq!(ev[0].node, s, "the event lands on the instance root");
    // Left of the track clamps to min; right of it to max.
    let ev = fx.touch(Phase::Move, 5.0, 30.0, 0.1);
    assert_eq!(edits(&ev), vec![0.0], "{ev:?}");
    let ev = fx.touch(Phase::Move, 395.0, 30.0, 0.2);
    assert_eq!(edits(&ev), vec![10.0], "{ev:?}");
    let ev = fx.touch(Phase::Up, 395.0, 30.0, 0.3);
    assert_eq!(changes(&ev), vec![10.0], "{ev:?}");
    assert_eq!(signals(&fx.page, &ev), vec!["drag_ended"], "{ev:?}");
    assert!(matches!(ev.last().unwrap().kind, EventKind::Signal), "the commit lands before the closing edge: {ev:?}");
    assert_eq!(fx.value(s), 10.0);
}

/// A cancelled gesture restores the grabbed value (the stock slider
/// law: never leave a half-drag behind).
#[test]
fn a_cancel_restores_the_grabbed_value() {
    let mut fx = Fx::new(&format!("{SLIDER_PHONE}View v {{ SliderPhone s value=2 }}"));
    let s = fx.page.find("s").unwrap();
    fx.touch(Phase::Down, 200.0, 30.0, 0.0);
    assert_eq!(fx.value(s), 5.0);
    let ev = fx.touch(Phase::Cancel, 200.0, 30.0, 0.1);
    assert!(changes(&ev).is_empty(), "{ev:?}");
    assert_eq!(signals(&fx.page, &ev), vec!["drag_ended"], "a host arming UI on drag_started always hears the end: {ev:?}");
    assert_eq!(fx.value(s), 2.0);
}

/// Arrows step ±5% of the range (`key_unit=5%`, the shipped
/// SliderPhone unit) on a continuous slider; a stepped instance steps
/// by its `step=` instead; Home/End jump to the ends. Each step commits
/// `change` directly.
#[test]
fn arrows_step_by_the_key_unit_and_step_wins() {
    let mut fx = Fx::new(&format!("{SLIDER_PHONE}View v {{ SliderPhone s value=2\n SliderPhone t value=2, step=2 }}"));
    let (s, t) = (fx.page.find("s").unwrap(), fx.page.find("t").unwrap());
    fx.key(Key::Tab);
    let ev = fx.key(Key::Right);
    assert_eq!(changes(&ev), vec![2.5], "5% of 0..10 is 0.5: {ev:?}");
    fx.key(Key::Left);
    assert_eq!(fx.value(s), 2.0);
    fx.key(Key::End);
    assert_eq!(fx.value(s), 10.0);
    fx.key(Key::Home);
    assert_eq!(fx.value(s), 0.0);
    // The stepped instance: `step=` beats `key_unit=` (the stock law).
    fx.key(Key::Tab);
    let ev = fx.key(Key::Right);
    assert_eq!(changes(&ev), vec![4.0], "{ev:?}");
    assert_eq!(fx.value(t), 4.0);
}

/// The drag signals belong to the FINGER: keys and wheel step without
/// them (the addon's key_action emitted none), and a contract that
/// declares no `signal` line gets no signal events at all.
#[test]
fn drag_signals_are_the_fingers_and_only_when_declared() {
    let mut fx = Fx::new(&format!("{SLIDER_PHONE}View v {{ SliderPhone s value=2 }}"));
    fx.key(Key::Tab);
    let ev = fx.key(Key::Right);
    assert!(signals(&fx.page, &ev).is_empty(), "{ev:?}");
    fx.kept = KeptFrame::fresh(&fx.page, resolve(&fx.page, &fx.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let ev = fx.input.wheel(&fx.page, &mut fx.rt, fx.kept.frame(), 200.0, 10.0, true);
    assert!(signals(&fx.page, &ev).is_empty(), "{ev:?}");

    let plain = "control Plain $value=0, $min=0, $max=10, $step=0 {\n    behavior slider by $value in $min..$max\n    state_builtin focused\n    Layers width=fill, height=60 { slot track width=fill, height=10 { Rectangle width=fill, height=fill, color=#FF0000FF } }\n}\n";
    let mut fx = Fx::new(&format!("{plain}View v {{ Plain p value=2 }}"));
    let ev = fx.touch(Phase::Down, 200.0, 30.0, 0.0);
    assert!(signals(&fx.page, &ev).is_empty(), "undeclared signals never raise: {ev:?}");
    let ev = fx.touch(Phase::Up, 200.0, 30.0, 0.1);
    assert!(signals(&fx.page, &ev).is_empty(), "{ev:?}");
}

/// A wheel tick steps the row under the pointer — anywhere on the
/// instance, not just the track (spec 5.5: wheel and arrows share one
/// path).
#[test]
fn the_wheel_steps_the_row_under_the_pointer() {
    let mut fx = Fx::new(&format!("{SLIDER_PHONE}View v {{ SliderPhone s value=2 }}"));
    let s = fx.page.find("s").unwrap();
    fx.kept = KeptFrame::fresh(&fx.page, resolve(&fx.page, &fx.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let ev = fx.input.wheel(&fx.page, &mut fx.rt, fx.kept.frame(), 200.0, 10.0, true);
    assert_eq!(changes(&ev), vec![2.5], "{ev:?}");
    assert_eq!(fx.value(s), 2.5);
}

/// The visual halves follow a drag: the `automatic=length` fill slot
/// re-derives its fraction, and a `text=$value` label rewrites at the
/// next `tick` (the input behavior holds only `&Page`, so the label
/// follow rides the frame door).
#[test]
fn the_fill_and_the_live_label_follow() {
    let mut fx = Fx::new(&format!("{SLIDER_PHONE}View v {{ SliderPhone s value=2 }}"));
    fx.touch(Phase::Down, 382.0, 30.0, 0.0);
    fx.touch(Phase::Up, 382.0, 30.0, 0.1);
    let progress = fx.page.find("s/progress").unwrap();
    let res = fx.res();
    let frac = res.auto.iter().find(|(n, _)| *n == progress).map(|(_, f)| *f);
    assert_eq!(frac, Some(1.0), "the fill follows to the end");
    fx.rt.tick(&mut fx.page, 0.016);
    let lab = fx.page.find("s/lab").unwrap();
    let shown = fx.rt.owned_text[lab.index()];
    assert_ne!(shown, gogo_ui::page::NO_STR, "the label rewrote");
    assert_eq!(fx.page.str(shown), "10");
}

/// A disabled instance answers nothing — finger, keys or wheel.
#[test]
fn a_disabled_instance_answers_nothing() {
    let mut fx = Fx::new(&format!("{SLIDER_PHONE}View v {{ SliderPhone s value=2 }}"));
    let s = fx.page.find("s").unwrap();
    fx.rt.set_state(s, DISABLED, true);
    let ev = fx.touch(Phase::Down, 200.0, 30.0, 0.0);
    assert!(edits(&ev).is_empty(), "{ev:?}");
    fx.kept = KeptFrame::fresh(&fx.page, resolve(&fx.page, &fx.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let ev = fx.input.wheel(&fx.page, &mut fx.rt, fx.kept.frame(), 200.0, 10.0, true);
    assert!(changes(&ev).is_empty(), "{ev:?}");
    // No write ever landed: the root still has no live Value prop.
    assert!(fx.value(s).is_nan(), "a write leaked through: {}", fx.value(s));
}

/// The fences: a `keys` claim on the behavior's groups (one owner), an
/// unknown behavior name, a widget host, a missing `slot track`, a
/// second `behavior` line, and missing wiring all refuse at compile.
#[test]
fn behavior_fences() {
    let fails = |src: &str| gogo_ui::has_errors(&compile(src, &|_| None).diags);
    let body = "state_builtin focused\n    Layers { slot track width=fill, height=10 { Rectangle width=fill, height=fill, color=#FF0000FF } }";
    assert!(fails(&format!("control C $value=0, $min=0, $max=10, $step=0 {{\n    behavior slider by $value in $min..$max\n    keys arrows\n    {body}\n}}\nView v {{ C c }}")), "keys on the behavior's groups");
    assert!(!fails(&format!("control C $value=0, $min=0, $max=10, $step=0 {{\n    behavior slider by $value in $min..$max\n    keys enter\n    {body}\n}}\nView v {{ C c }}")), "enter stays claimable");
    assert!(fails(&format!("control C $value=0, $min=0, $max=10, $step=0 {{\n    behavior knob by $value in $min..$max\n    {body}\n}}\nView v {{ C c }}")), "the vocabulary is closed");
    assert!(fails("widget W {\n    behavior slider by $value in $min..$max\n    Layers { Rectangle r 10x10 }\n}\nView v { W w }"), "a widget has no contract");
    assert!(fails("control C $value=0, $min=0, $max=10 {\n    behavior slider by $value in $min..$max\n    state_builtin focused\n    Layers { Rectangle r 10x10 }\n}\nView v { C c }"), "no track slot");
    assert!(fails(&format!("control C $value=0, $min=0, $max=10 {{\n    behavior slider by $value in $min..$max\n    behavior slider by $value in $min..$max\n    {body}\n}}\nView v {{ C c }}")), "one behavior line");
    assert!(fails(&format!("control C $value=0, $min=0, $max=10 {{\n    behavior slider\n    {body}\n}}\nView v {{ C c }}")), "the wiring is spelled");
}
