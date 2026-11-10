//! GP53 (user-approved 2026-09-06): a HOST-registered input behavior —
//! the gogo user's own `Behavior`, named to the compiler through
//! `CompileOptions::behaviors` (its spec: the slots it reads, the keys
//! it owns) and seated on the page's `Input` with `Input::register`. The
//! runtime keeps the gesture law (`held`, `edit`/`change`, restore on
//! cancel, the declared edges); the behavior is the pointer math.

use gogo_ui::input::{Event, EventKind, Input, Key, Mods, Phase, Touch};
use gogo_ui::KeptFrame;
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{NodeId, Page, PropKey, Value};
use gogo_ui::runtime::{HELD, Runtime, resolve};
use gogo_ui::{Behavior, BehaviorSpec, Claim, CompileOptions, Cx, KEYS_ARROWS, KeyAction, Payload, compile_with};

/// A vertical fader: the value follows y over the track, bottom = min.
/// Arrows step; Home/End are NOT its (left to the contract's `keys`).
#[derive(Default)]
struct Fader;

static FADER_SPEC: BehaviorSpec = BehaviorSpec { name: "fader", slots: &["track"], keys: KEYS_ARROWS, payload: Payload::Number };

impl Fader {
    fn apply(cx: &mut Cx, y: f32) {
        let track = cx.slot(0);
        if track.h > 0.0 {
            cx.set_fraction((track.y + track.h - y) / track.h);
        }
    }
}

impl Behavior for Fader {
    fn spec(&self) -> &'static BehaviorSpec {
        &FADER_SPEC
    }

    fn began(&mut self, cx: &mut Cx, _x: f32, y: f32) -> Claim {
        Self::apply(cx, y);
        Claim::Take
    }

    fn moved(&mut self, cx: &mut Cx, _x: f32, y: f32) {
        Self::apply(cx, y);
    }

    fn key(&mut self, cx: &mut Cx, action: KeyAction) -> bool {
        match action {
            KeyAction::Increment => cx.set_value(cx.value() + cx.key_step()),
            KeyAction::Decrement => cx.set_value(cx.value() - cx.key_step()),
            _ => return false,
        }
        true
    }
}

/// A behavior that takes only a press on its LEFT third and passes the
/// rest on (a knob that turns from its handle only).
#[derive(Default)]
struct Edge;

static EDGE_SPEC: BehaviorSpec = BehaviorSpec { name: "edge", slots: &["track"], keys: 0, payload: Payload::Number };

impl Behavior for Edge {
    fn spec(&self) -> &'static BehaviorSpec {
        &EDGE_SPEC
    }

    fn began(&mut self, cx: &mut Cx, x: f32, _y: f32) -> Claim {
        if x < cx.width / 3.0 {
            cx.set_value(cx.max);
            Claim::Take
        } else {
            Claim::Pass
        }
    }
}

const FADER: &str = "control VFader $value=5, $min=0, $max=10, $step=0 {\n    behavior fader by $value in $min..$max, key_unit=10%\n    state_builtin focused, held\n    keys home_end\n    signal drag_started, drag_ended\n    Layers width=40, height=200 {\n        slot track width=fill, height=fill { Rectangle width=fill, height=fill, color=#333333FF }\n    }\n}\n";
const EDGE: &str = "control Handle $value=0, $min=0, $max=1 {\n    behavior edge by $value in $min..$max\n    state_builtin held\n    Layers width=90, height=30 {\n        slot track width=fill, height=fill { Rectangle width=fill, height=fill, color=#333333FF }\n    }\n}\n";

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    /// The frame the last touch or key saw (laid out fresh for each).
    kept: KeptFrame,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = compile_with(src, &|_| None, CompileOptions { behaviors: &[FADER_SPEC, EDGE_SPEC], ..CompileOptions::default() });
        assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let mut input = Input::new();
        input.register(Box::new(Fader));
        input.register(Box::new(Edge));
        Fx { page, rt, input, kept: f }
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

/// The host behavior's math drives the value; the runtime's law wraps
/// it: `held`, the edges, `edit` per changed write, `change` on release,
/// restore on cancel.
#[test]
fn a_registered_machine_drives_the_value_under_the_runtimes_law() {
    let mut fx = Fx::new(&format!("{FADER}View v {{ VFader f }}"));
    let f = fx.page.find("f").unwrap();
    // Track 0..200 tall; y=50 is 3/4 up -> 7.5.
    let ev = fx.touch(Phase::Down, 20.0, 50.0, 0.0);
    assert_eq!(signals(&fx.page, &ev), vec!["drag_started"], "{ev:?}");
    assert_eq!(edits(&ev), vec![7.5], "{ev:?}");
    assert!(fx.rt.has(f, HELD), "the runtime keeps `held`");
    let ev = fx.touch(Phase::Move, 20.0, 200.0, 0.1);
    assert_eq!(edits(&ev), vec![0.0], "{ev:?}");
    let ev = fx.touch(Phase::Up, 20.0, 200.0, 0.2);
    assert_eq!(changes(&ev), vec![0.0], "{ev:?}");
    assert_eq!(signals(&fx.page, &ev), vec!["drag_ended"], "{ev:?}");
    assert!(!fx.rt.has(f, HELD));
    fx.touch(Phase::Down, 20.0, 100.0, 1.0);
    assert_eq!(fx.value(f), 5.0);
    let ev = fx.touch(Phase::Cancel, 20.0, 100.0, 1.1);
    assert!(changes(&ev).is_empty());
    assert_eq!(fx.value(f), 0.0, "a cancel restores what the finger found");
}

/// The keys the spec OWNS reach the behavior (`key_unit=10%` = 1.0 per
/// arrow); a group it leaves — Home/End here — stays the contract's to
/// claim with `keys` and reaches the code-behind as a `Key` event.
#[test]
fn the_machine_answers_its_key_groups_and_leaves_the_rest() {
    let mut fx = Fx::new(&format!("{FADER}View v {{ VFader f }}"));
    let f = fx.page.find("f").unwrap();
    fx.key(Key::Tab);
    let ev = fx.key(Key::Up);
    assert_eq!(changes(&ev), vec![6.0], "{ev:?}");
    fx.key(Key::Down);
    assert_eq!(fx.value(f), 5.0);
    let ev = fx.key(Key::End);
    assert!(changes(&ev).is_empty(), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Key { key: Key::End, .. }) && e.node == f), "the claimed key reaches the code-behind: {ev:?}");
}

/// `Claim::Pass` lets the press go on as if there were no behavior: no
/// `held`, no write, no edges.
#[test]
fn a_passed_press_is_not_the_machines() {
    let mut fx = Fx::new(&format!("{EDGE}View v {{ Handle h }}"));
    let h = fx.page.find("h").unwrap();
    let ev = fx.touch(Phase::Down, 60.0, 15.0, 0.0);
    assert!(ev.is_empty(), "{ev:?}");
    assert!(!fx.rt.has(h, HELD));
    fx.touch(Phase::Up, 60.0, 15.0, 0.1);
    assert!(fx.value(h).is_nan(), "no write landed");
    fx.touch(Phase::Down, 10.0, 15.0, 1.0);
    assert!(fx.rt.has(h, HELD));
    assert_eq!(fx.value(h), 1.0);
}

/// The compile fences read the host's spec: an unknown name lists every
/// behavior the build knows, a missing slot the spec reads refuses, and
/// a `keys` claim on a group the behavior owns refuses.
#[test]
fn host_machine_fences() {
    let opts = CompileOptions { behaviors: &[FADER_SPEC, EDGE_SPEC], ..CompileOptions::default() };
    let c = compile_with("control C $value=0, $min=0, $max=1 {\n    behavior knob by $value in $min..$max\n    Layers { slot track 10x10 { Rectangle 10x10 } }\n}\nView v { C c }", &|_| None, opts);
    let msg = c.diags.iter().find(|d| d.code == "G0082").map(|d| d.message.clone()).unwrap_or_default();
    assert!(msg.contains("`slider`") && msg.contains("`toggle`") && msg.contains("`fader`") && msg.contains("`edge`"), "{msg}");
    let c = compile_with("control C $value=0, $min=0, $max=1 {\n    behavior fader by $value in $min..$max\n    Layers { Rectangle r 10x10 }\n}\nView v { C c }", &|_| None, opts);
    assert!(c.diags.iter().any(|d| d.code == "G0082" && d.message.contains("`slot track`")), "{:#?}", c.diags);
    let c = compile_with("control C $value=0, $min=0, $max=1 {\n    behavior fader by $value in $min..$max\n    keys arrows\n    Layers { slot track 10x10 { Rectangle 10x10 } }\n}\nView v { C c }", &|_| None, opts);
    assert!(c.diags.iter().any(|d| d.code == "G0082" && d.message.contains("`fader` behavior already owns arrows")), "{:#?}", c.diags);
}
