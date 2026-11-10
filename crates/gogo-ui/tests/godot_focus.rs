//! GP26 steps 1-4 — keyboard focus: who is reachable, how focus moves,
//! what the focused control does with its keys (section 5.5), and
//! `hovered` (mouse only, D75; the chain reading — user call
//! 2026-09-02).
//!
//! Reachable = a press surface (Button/Check/Toggle/Radio/ListItem),
//! Slider or Input, or a `control` whose contract declares
//! `state_builtin focused`; `keyboard_target=off` opts out, and
//! disabled/hidden boxes are skipped at traversal. Tab walks FILE order
//! (wrapping, Shift-Tab backward); an arrow moves to the nearest
//! reachable box in its direction — unless the focused control claims
//! the key (an Input's caret claims Left/Right). The FOCUSED bit is
//! D129 VISIBLE focus: a key grant shows the ring, a tap parks focus
//! with no ring — except on an Input, whose caret always warrants one.
use gogo_ui::draw::{Draw, draw};
use gogo_ui::KeptFrame;
use gogo_ui::input::{Event, EventKind, Input, Key, Mods, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{BUTTON_FOCUS_RING, BUTTON_FOCUS_RING_EPX, NodeId, PropKey, Color_Straight_Srgb_4xU8_RGBA, Value};
use gogo_ui::runtime::{DISABLED, FOCUS_WITHIN, FOCUSED, HOVERED, Runtime, SELECTED, resolve};
use gogo_ui::{Page, compile};

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    /// The tap clock — see `tap`.
    now: f64,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = compile(src, &|_| None);
        assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        Fx { page, rt, input: Input::new(), now: 0.0 }
    }
    fn key(&mut self, k: Key) -> Vec<Event> {
        let f = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, f.frame(), k, Mods::NONE)
    }
    fn tap(&mut self, node: &str) -> Vec<Event> {
        let id = self.page.find(node).unwrap();
        let f = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let rects = f.rects();
        let r = rects[id.index()];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        // Taps sit a second apart: two same-spot taps within the
        // double-click window would word-select in a field.
        self.now += 1.0;
        let t = self.now;
        let mut ev = self.input.touch(&self.page, &mut self.rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Down, x, y, t, id: 1 });
        ev.extend(self.input.touch(&self.page, &mut self.rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Up, x, y, t: t + 0.1, id: 1 }));
        ev
    }
    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap()
    }
    /// The one node holding the FOCUSED bit (None = no ring anywhere).
    fn ring(&self) -> Option<NodeId> {
        self.page.node_ids().find(|n| self.rt.has(*n, FOCUSED))
    }
    fn hover_xy(&mut self, x: f32, y: f32) {
        let f = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.hover(&self.page, &mut self.rt, f.frame(), x, y);
    }
    fn hover_at(&mut self, name: &str) {
        let id = self.id(name);
        let f = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let rects = f.rects();
        let r = rects[id.index()];
        self.hover_xy(r.x + r.w / 2.0, r.y + r.h / 2.0);
    }
    fn value(&self, name: &str) -> f32 {
        let res = resolve(&self.page, &self.rt);
        res.num(&self.page, self.id(name), PropKey::Value).unwrap_or(-1.0)
    }
    fn draws(&self) -> Vec<Draw> {
        let f = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        draw(&self.page, f.frame())
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

/// `focus_within` lights the owner and every ancestor — the one state
/// that looks up — with the ring's visible-focus warrant: a tapped
/// Input (caret) lights its row, and focus leaving dims the chain.
#[test]
fn focus_within_lights_the_ancestor_chain() {
    let src = "View v { Panel row width=200, height=60, halign=left, valign=top { Input t2 width=180, height=36 }\n Button away text=\"x\", width=60, height=24, halign=left, valign=bottom }";
    let mut fx = Fx::new(src);
    fx.tap("t2");
    for name in ["t2", "row", "v"] {
        assert!(fx.rt.has(fx.id(name), FOCUS_WITHIN), "{name} lights while the field holds focus");
    }
    fx.tap("away");
    for name in ["t2", "row", "v"] {
        assert!(!fx.rt.has(fx.id(name), FOCUS_WITHIN), "{name} dims when focus leaves");
    }
    assert!(!fx.rt.has(fx.id("v"), FOCUS_WITHIN), "a parked Button lights nothing");
}

/// The D129 modality edge: ANY key press means keyboard driving — a
/// Button holding parked (ringless) focus gets its ring and its chain
/// lights, in place, even when the key itself did nothing.
#[test]
fn a_parked_focus_lights_on_the_next_key() {
    let mut fx = Fx::new(ROW_SRC);
    fx.tap("a");
    assert_eq!(fx.ring(), None, "parked: no ring");
    assert!(!fx.rt.has(fx.id("v"), FOCUS_WITHIN));
    fx.key(Key::Char('x'));
    assert_eq!(fx.ring(), Some(fx.id("a")), "the key upgrades the ring in place");
    assert!(fx.rt.has(fx.id("v"), FOCUS_WITHIN), "the chain lights with it");
}

/// D129: the Button family ships a default `when focused` ring — shown
/// for key focus only; authoring `when focused` replaces it (the D122
/// per-state doctrine).
#[test]
fn the_button_family_ships_a_default_focus_ring() {
    let mut fx = Fx::new("View v { Button b text=\"B\", width=80, height=30 }");
    let ringed = |list: &[Draw]| list.iter().any(|d| matches!(d, Draw::Quad { border, border_color, .. } if *border == BUTTON_FOCUS_RING_EPX && *border_color == BUTTON_FOCUS_RING));
    assert!(!ringed(&fx.draws()), "no ring at rest");
    fx.key(Key::Tab);
    assert!(ringed(&fx.draws()), "key focus draws the default ring: {:#?}", fx.draws());
    let mut own = Fx::new("View v { Button b text=\"B\", width=80, height=30 { when focused background=#00FF00FF } }");
    own.key(Key::Tab);
    let list = own.draws();
    assert!(!ringed(&list), "an authored `when focused` replaces the default ring");
    assert!(list.iter().any(|d| matches!(d, Draw::Quad { color, .. } if *color == Color_Straight_Srgb_4xU8_RGBA([0x00, 0xFF, 0x00, 0xFF]))), "the author's response shows: {list:#?}");
}

/// `disabled` beats everything, always — a control's own state implies
/// held-plus-meaning and beats `held`, but it can't out-rank the input
/// blocker (the addon's §5.3 law; was misported with custom on top).
#[test]
fn disabled_beats_a_controls_own_state() {
    let src = "control Chip $value=0 {\n state_builtin disabled\n state glowing\n Layers { Rectangle r width=40, height=20, color=white } }\nView v { Chip k { when glowing background=#112233FF\n when disabled background=#445566FF } }";
    let mut fx = Fx::new(src);
    let root = fx.id("k");
    fx.rt.set_custom(&fx.page, root, "glowing", true).unwrap();
    fx.rt.set_state(root, DISABLED, true);
    let res = resolve(&fx.page, &fx.rt);
    assert_eq!(res.prop(&fx.page, root, PropKey::Background), Some(Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0x44, 0x55, 0x66, 0xFF]))), "disabled wins the conflict");
}

/// Section 5.5: Enter/Space on the focused control run the completions
/// a finished tap runs — a Button fires `activate`, a Check flips
/// `selected` and reports `change` (genuine transitions only).
#[test]
fn enter_and_space_activate_the_focused_control() {
    let src = "View v { Column c halign=left, valign=top { Button go text=\"Go\", width=80, height=30, on:activate=Fire; Check save text=\"s\", width=120, height=30 } }";
    let mut fx = Fx::new(src);
    fx.key(Key::Tab);
    let ev = fx.key(Key::Enter);
    assert!(ev.iter().any(|e| e.node == fx.id("go") && matches!(e.kind, EventKind::Activate)), "{ev:#?}");
    fx.key(Key::Tab);
    let ev = fx.key(Key::Char(' '));
    assert!(fx.rt.has(fx.id("save"), SELECTED), "Space flips the Check");
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Change(Value::VBool(true)))), "{ev:#?}");
    let ev = fx.key(Key::Enter);
    assert!(!fx.rt.has(fx.id("save"), SELECTED));
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Change(Value::VBool(false)))), "flips back: {ev:#?}");
}

/// A focused Slider claims ALL the arrows (the shipped router: right/up
/// increment, left/down decrement, by `step=`), Home/End jump to the
/// ends; a step commits `change` directly — no `edit` — and one landing
/// on the current value reports nothing. Tab is the way off.
#[test]
fn a_focused_sliders_arrows_step_its_value() {
    let mut fx = Fx::new(ROW_SRC);
    fx.key(Key::Tab);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("s")));
    let ev = fx.key(Key::Right);
    assert_eq!(fx.value("s"), 40.0);
    assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Change(_))).count(), 1, "{ev:#?}");
    assert!(!ev.iter().any(|e| matches!(e.kind, EventKind::Edit(_))), "a step is change only");
    fx.key(Key::Down);
    assert_eq!(fx.value("s"), 30.0, "down decrements");
    assert_eq!(fx.input.focused(), Some(fx.id("s")), "arrows never leave the slider");
    fx.key(Key::End);
    assert_eq!(fx.value("s"), 100.0);
    assert!(fx.key(Key::End).is_empty(), "landing on the current value reports nothing");
    fx.key(Key::Home);
    assert_eq!(fx.value("s"), 0.0);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("t")), "Tab is the way off");
}

/// A focused Toggle's arrows pick its sides (right/up = on, left/down =
/// off — the shipped toggle_behavior), Enter flips and activates;
/// `change(bool)` once per genuine transition.
#[test]
fn a_focused_toggles_arrows_pick_its_sides() {
    let src = "View v { Column c halign=left, valign=top { Toggle mute text=\"m\", width=140, height=30 } }";
    let mut fx = Fx::new(src);
    fx.key(Key::Tab);
    let ev = fx.key(Key::Right);
    assert!(fx.rt.has(fx.id("mute"), SELECTED), "right switches on");
    assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Change(_))).count(), 1);
    assert!(fx.key(Key::Up).is_empty(), "already on — no change");
    let ev = fx.key(Key::Left);
    assert!(!fx.rt.has(fx.id("mute"), SELECTED), "left switches off");
    assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Change(_))).count(), 1);
    let ev = fx.key(Key::Enter);
    assert!(fx.rt.has(fx.id("mute"), SELECTED), "Enter flips");
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Activate)), "{ev:#?}");
}

/// The wheel rides the same logical pair (section 5.5: keyboard, reader
/// and wheel share one path): a tick over a Slider steps it and PARKS
/// focus there (a wheel is pointer driving — no ring); over a plain box
/// it does nothing.
#[test]
fn a_wheel_tick_steps_the_control_under_the_pointer() {
    let mut fx = Fx::new(ROW_SRC);
    let f = KeptFrame::fresh(&fx.page, resolve(&fx.page, &fx.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let rects = f.rects();
    let r = rects[(fx.id("s")).index()];
    let ev = fx.input.wheel(&fx.page, &mut fx.rt, f.frame(), r.x + r.w / 2.0, r.y + r.h / 2.0, true);
    assert_eq!(fx.value("s"), 40.0, "up increments");
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Change(_))), "{ev:#?}");
    assert_eq!(fx.input.focused(), Some(fx.id("s")), "the wheel takes focus");
    assert_eq!(fx.ring(), None, "parked — a wheel is pointer driving");
    let away = fx.input.wheel(&fx.page, &mut fx.rt, f.frame(), 350.0, 700.0, true);
    assert!(away.is_empty(), "nothing steppable there");
}

/// follow_focus (GPAR113): a key grant scrolls every Scroll above the
/// new owner just enough that its box is in view — and scrolls back
/// when focus returns to the top; never an overscroll.
#[test]
fn key_focus_scrolls_into_view() {
    let src = "View v { Scroll sc width=200, height=100, halign=left, valign=top { Column c spacing=0 { Button b1 text=\"1\", width=180, height=40; Button b2 text=\"2\", width=180, height=40; Button b3 text=\"3\", width=180, height=40; Button b4 text=\"4\", width=180, height=40 } } }";
    let mut fx = Fx::new(src);
    fx.key(Key::Tab);
    fx.key(Key::Tab);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("b3")));
    let sc = fx.id("sc");
    assert!(fx.rt.scroll[sc.index()][1] >= 20.0, "b3 (80..120) pulled into the 100-tall viewport: {:?}", fx.rt.scroll[sc.index()]);
    fx.key(Key::Tab);
    assert!(fx.rt.scroll[sc.index()][1] >= 60.0, "b4 too: {:?}", fx.rt.scroll[sc.index()]);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(fx.id("b1")), "wrap");
    assert_eq!(fx.rt.scroll[sc.index()][1], 0.0, "scrolled back up for b1");
}

/// The List keyboard cursor (GP28's tail): rows are reachable, arrows
/// walk them, Enter selects (`change` on the LIST, then the row's
/// `activate`) — one law with the tap.
#[test]
fn list_rows_walk_and_activate_by_keyboard() {
    let src = "View v { List picker width=200, height=fill, halign=left, valign=top { empty { Label text=\"none\" } repeat tasks capacity=8 { ListItem { Label text=\"\" } } } }";
    let mut fx = Fx::new(src);
    let rep = fx.id("tasks");
    fx.rt.set_count(&mut fx.page, rep, 3);
    let rows = fx.rt.items(&fx.page, rep);
    fx.key(Key::Tab);
    assert_eq!(fx.input.focused(), Some(rows[0]), "Tab skips the hidden template row");
    fx.key(Key::Down);
    assert_eq!(fx.input.focused(), Some(rows[1]), "arrows walk the rows");
    let ev = fx.key(Key::Enter);
    assert!(fx.rt.has(rows[1], SELECTED), "Enter selects in select=one");
    assert!(ev.iter().any(|e| e.node == fx.id("picker") && matches!(e.kind, EventKind::Change(_))), "change on the LIST: {ev:#?}");
    assert!(ev.iter().any(|e| e.node == rows[1] && matches!(e.kind, EventKind::Activate)), "then the row activates: {ev:#?}");
}

/// `hovered` lights the box under the mouse and EVERY ancestor (the
/// chain reading, user call 2026-09-02 — the game staple: the settings
/// row lights while its inner slider is under the mouse); moving off
/// dims the whole chain, moving to another box moves it.
#[test]
fn hover_follows_the_mouse_and_lights_the_chain() {
    let src = "View v { Panel row width=200, height=60, halign=left, valign=top { Slider sl value=30, step=10, width=180, height=30, halign=left, valign=top }\n Button a text=\"A\", width=80, height=30, halign=left, valign=bottom }";
    let mut fx = Fx::new(src);
    fx.hover_at("sl");
    for name in ["sl", "row", "v"] {
        assert!(fx.rt.has(fx.id(name), HOVERED), "{name} lights under the mouse");
    }
    fx.hover_at("a");
    assert!(!fx.rt.has(fx.id("sl"), HOVERED), "the chain moved off the slider");
    assert!(!fx.rt.has(fx.id("row"), HOVERED));
    assert!(fx.rt.has(fx.id("a"), HOVERED));
    fx.hover_xy(350.0, 400.0);
    assert!(!fx.rt.has(fx.id("a"), HOVERED), "empty space hovers nothing");
    assert!(!fx.rt.has(fx.id("v"), HOVERED));
}

/// D75: touch never hovers — a tap runs its whole gesture without a
/// single HOVERED bit, so no sticky hover look survives a finger.
#[test]
fn touch_never_hovers() {
    let mut fx = Fx::new(ROW_SRC);
    fx.tap("a");
    assert!(fx.page.node_ids().all(|n| !fx.rt.has(n, HOVERED)), "a finger leaves no hover");
}

/// The capture bracket: a mouse press freezes hover — slipping off a
/// held button keeps its hovered look and nothing new hovers; the
/// release re-checks the position (elsewhere clears it).
#[test]
fn a_mouse_press_captures_hover() {
    let mut fx = Fx::new(ROW_SRC);
    fx.hover_at("a");
    assert!(fx.rt.has(fx.id("a"), HOVERED));
    let f = KeptFrame::fresh(&fx.page, resolve(&fx.page, &fx.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let rects = f.rects();
    let r = rects[(fx.id("a")).index()];
    fx.input.touch(&fx.page, &mut fx.rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Down, x: r.x + r.w / 2.0, y: r.y + r.h / 2.0, t: 0.0, id: 1 });
    fx.hover_xy(350.0, 700.0);
    assert!(fx.rt.has(fx.id("a"), HOVERED), "the press holds the hovered look off-box");
    let rs = rects[(fx.id("s")).index()];
    fx.hover_xy(rs.x + rs.w / 2.0, rs.y + rs.h / 2.0);
    assert!(!fx.rt.has(fx.id("s"), HOVERED), "nothing new hovers mid-press");
    fx.input.touch(&fx.page, &mut fx.rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Up, x: 350.0, y: 700.0, t: 0.2, id: 1 });
    assert!(!fx.rt.has(fx.id("a"), HOVERED), "released elsewhere: hover clears");
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
