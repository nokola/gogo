//! GP26 step 1 — keyboard focus: who is reachable and how focus moves.
//! Reachable = a press surface (Button/Check/Toggle/Radio/ListItem),
//! Slider or Input, or a `control` whose contract declares
//! `state_builtin focused`; `keyboard_target=off` opts out, and
//! disabled/hidden boxes are skipped at traversal. Tab walks FILE order
//! (wrapping, Shift-Tab backward); an arrow moves to the nearest
//! reachable box in its direction — unless the focused control claims
//! the key (an Input's caret claims Left/Right). The FOCUSED bit is
//! D129 VISIBLE focus: a key grant shows the ring, a tap parks focus
//! with no ring — except on an Input, whose caret always warrants one.
use gogo::input::{Event, EventKind, Input, Key, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::NodeId;
use gogo::runtime::{FOCUSED, Runtime, resolve};
use gogo::{Page, compile};

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = compile(src, &|_| None);
        assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        Fx { page, rt, input: Input::new() }
    }
    fn key(&mut self, k: Key) -> Vec<Event> {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, &res, &rects, k)
    }
    fn tap(&mut self, node: &str) -> Vec<Event> {
        let id = self.page.find(node).unwrap();
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let r = rects[id as usize];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let mut ev = self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase: Phase::Down, x, y, t: 0.0, id: 1 });
        ev.extend(self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase: Phase::Up, x, y, t: 0.1, id: 1 }));
        ev
    }
    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap()
    }
    /// The one node holding the FOCUSED bit (None = no ring anywhere).
    fn ring(&self) -> Option<NodeId> {
        (0..self.page.nodes.len() as NodeId).find(|n| self.rt.has(*n, FOCUSED))
    }
}

const ROW_SRC: &str = "View v { Column c halign=left, valign=top { Button a text=\"A\", width=80, height=30; Slider s value=30, step=10, width=200; Input t width=200, height=36; Button b text=\"B\", width=80, height=30 } }";

/// Tab walks the reachable boxes in FILE order and wraps; Shift-Tab
/// walks backward. Nothing focused enters at the first (last for
/// Shift-Tab), and the ring follows — key focus is always visible.
#[test]
fn tab_cycles_reachable_boxes_in_file_order() {
    let mut fx = Fx::new(ROW_SRC);
    for name in ["a", "s", "t", "b", "a"] {
        fx.key(Key::Tab);
        assert_eq!(fx.input.focused(), Some(fx.id(name)), "at {name}");
        assert_eq!(fx.ring(), Some(fx.id(name)), "the ring rides key focus at {name}");
    }
    fx.key(Key::TabBack);
    assert_eq!(fx.input.focused(), Some(fx.id("b")), "Shift-Tab walks backward");
    let mut fresh = Fx::new(ROW_SRC);
    fresh.key(Key::TabBack);
    assert_eq!(fresh.input.focused(), Some(fresh.id("b")), "Shift-Tab enters at the last");
}

/// Disabled boxes, hidden boxes (own or an ancestor's `gone`), and
/// `keyboard_target=off` opt-outs are skipped; plain boxes (a Panel)
/// were never reachable.
#[test]
fn traversal_skips_disabled_hidden_and_opted_out() {
    let src = "View v { Column c halign=left, valign=top { Button d text=\"d\", width=80, height=30, disabled=true; Button g text=\"g\", width=80, height=30, visibility=gone; Column hide visibility=gone { Button inside text=\"i\", width=80, height=30 }; Button off text=\"o\", width=80, height=30, keyboard_target=off; Panel plain width=80, height=30; Button ok text=\"k\", width=80, height=30 } }";
    let mut fx = Fx::new(src);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("ok")), "only `ok` is reachable");
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("ok")), "a one-box cycle wraps onto itself");
}

/// Arrows move to the nearest reachable box in their direction — an
/// aligned neighbor beats a nearer diagonal one, and the edge of the
/// screen keeps focus put (the ring stays).
#[test]
fn arrows_move_focus_by_direction() {
    let src = "View v { Layers l width=fill, height=fill {\n Button tl text=\"1\", width=80, height=30, halign=left, valign=top\n Button tr text=\"2\", width=80, height=30, halign=right, valign=top\n Button bl text=\"3\", width=80, height=30, halign=left, valign=bottom\n Button br text=\"4\", width=80, height=30, halign=right, valign=bottom } }";
    let mut fx = Fx::new(src);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("tl")));
    fx.key(Key::Right);
    assert_eq!(fx.input.focused(), Some(fx.id("tr")), "right to the aligned neighbor");
    fx.key(Key::Down);
    assert_eq!(fx.input.focused(), Some(fx.id("br")));
    fx.key(Key::Left);
    assert_eq!(fx.input.focused(), Some(fx.id("bl")));
    fx.key(Key::Up);
    assert_eq!(fx.input.focused(), Some(fx.id("tl")));
    fx.key(Key::Up);
    assert_eq!(fx.input.focused(), Some(fx.id("tl")), "no box above: focus stays");
    assert_eq!(fx.ring(), Some(fx.id("tl")));
}

/// A focused Input claims Left/Right for its caret (focus stays on the
/// field); Up/Down still traverse out, and leaving commits the pending
/// text through the focus-loss door — once.
#[test]
fn an_inputs_caret_claims_left_right_and_leaving_commits() {
    let mut fx = Fx::new(ROW_SRC);
    fx.tap("t");
    fx.key(Key::Char('h'));
    fx.key(Key::Char('i'));
    let before = fx.input.focused();
    fx.key(Key::Left);
    assert_eq!(fx.input.focused(), before, "Left moves the caret, not focus");
    let ev = fx.key(Key::Down);
    assert_ne!(fx.input.focused(), before, "Down leaves the field");
    let commits = ev.iter().filter(|e| matches!(e.kind, EventKind::Change(_))).count();
    assert_eq!(commits, 1, "leaving commits once: {ev:#?}");
}

/// D129 visible focus: a tap PARKS focus on a Button without the ring
/// (the press-residue rule) — the next Tab continues from it, ring on.
/// A tapped Input is the caret exception: its ring shows at once.
#[test]
fn a_tap_parks_focus_without_the_ring() {
    let mut fx = Fx::new(ROW_SRC);
    fx.tap("a");
    assert_eq!(fx.input.focused(), Some(fx.id("a")), "the tap parks focus");
    assert_eq!(fx.ring(), None, "no ring residue on a tap");
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("s")), "Tab continues from the parked spot");
    assert_eq!(fx.ring(), Some(fx.id("s")));
    fx.tap("t");
    assert_eq!(fx.ring(), Some(fx.id("t")), "an Input's caret always warrants the ring");
}

/// A `control`'s contract gates the keyboard: `state_builtin focused`
/// makes the instance root reachable, a contract without it is skipped.
#[test]
fn a_controls_contract_gates_the_keyboard() {
    let src = "control Knob $value=0 {\n state_builtin focused\n Layers { Rectangle r width=30, height=30, color=white } }\ncontrol Dot $value=0 {\n Layers { Rectangle r2 width=30, height=30, color=white } }\nView v { Column c halign=left, valign=top { Knob k; Dot d; Button b text=\"B\", width=80, height=30 } }";
    let mut fx = Fx::new(src);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("k")), "the contract opts the root in");
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("b")), "a contract without `focused` is skipped");
}
