//! Port of the Godot gogo suite's Scroll, input, focus and entrance groups
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//!
//! - `_scroll_expand_tests` 8969-9013 (5), `_scroll_units_tests` 9013-9053 (7),
//!   `_scroll_hug_tests` 9053-9098 (8), `_scroll_allow_bars_tests` 9098-9251 (26),
//!   `_scroll_tests` 9251-9359 (17), `_scroll_catch_press_tests` 9359-9415 (6),
//!   `_scroll_occlusion_tests` 9415-9477 (5), `_accessibility_tests` 9477-9629 (17),
//!   `_focus_visible_tests` 9629-9765 (23), `_stranded_probe_tests` 9765-9830 (7),
//!   `_input_order_tests` 9830-9859 (1), `_motion_group_tests` 9859-9940 (10),
//!   `_entrance_tests` 9960-10064 (9).
//!
//! ENGINE — Godot machinery with no Rust counterpart, not ported:
//! - `scroll.eats-twins` 9291, `scroll.twin-clears` 9302: emulated mouse twin bookkeeping.
//! - `scroll.catch-twin-pressed` 9394: the twin lands before the raw touch.
//! - `a11y.role-mapped.*` 9490, `a11y.internals-marked` 9507, `a11y.row-roster` 9532,
//!   `a11y.rows-not-internal` 9541, `a11y.click-activates` 9544: AccessKit roles and roster.
//! - `a11y.radio-script` 9563: GodotRadio script class.
//! - `a11y.live-enum` 9626: engine accessibility_live enum.
//! - `fvis.authored-replaces-ring` 9668, `fring.default-injected` .. `fring.shape-composes`
//!   9689-9716: the engine focus StyleBox ring.
//! - `input.twin-first` 9830-9859: engine touch-emulation dispatch order.
use gogo::input::{Event, EventKind, FLING_BOOST, Input, Phase, SCROLL_DEADZONE, Touch};
use gogo::layout::{FixedAdvance, Rect, SCROLL_BAR_EPX, layout, scroll_axes_of};
use gogo::page::{Kind, NodeId, PropKey, Rgba, Value};
use gogo::runtime::{FOCUSED, HELD, HOVERED, Resolved, Runtime, SELECTED, resolve, resolve_animated};
use gogo::{Compiled, Page, compile};

const GREEN: [u8; 4] = [0, 255, 0, 255];

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn warnings(c: &Compiled) -> Vec<String> {
    c.diags.iter().filter(|d| d.severity == gogo::Severity::Warning).map(|d| format!("{} {}", d.code, d.message)).collect()
}

fn errors(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().filter(|d| d.severity == gogo::Severity::Error).map(|d| format!("{} {}", d.code, d.message)).collect()
}

/// One page, one runtime, one input machine: compile → resolve → layout →
/// touch / tick, the way the shell drives a page.
struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    frame: Rect,
}

impl Fx {
    fn new(src: &str) -> Fx {
        Fx::sized(src, 400.0, 800.0)
    }

    fn sized(src: &str, w: f32, h: f32) -> Fx {
        let c = ok(src);
        let rt = Runtime::new(&c.page);
        Fx { page: c.page, rt, input: Input::new(), frame: Rect::new(0.0, 0.0, w, h) }
    }

    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node named {name}"))
    }

    fn lay(&self) -> (Resolved, Vec<Rect>) {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, self.frame, &FixedAdvance);
        (res, rects)
    }

    fn rect(&self, name: &str) -> Rect {
        self.lay().1[self.id(name) as usize]
    }

    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        let (res, rects) = self.lay();
        self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase, x, y, t, id: 1 })
    }

    fn tick(&mut self, dt: f32) -> bool {
        let (res, rects) = self.lay();
        self.input.tick(&self.page, &mut self.rt, &res, &rects, dt)
    }

    fn scroll(&self, name: &str) -> [f32; 2] {
        self.rt.scroll[self.id(name) as usize]
    }

    fn axes(&self, name: &str) -> (bool, bool) {
        let (res, _) = self.lay();
        scroll_axes_of(&self.page, &res, self.id(name))
    }

    fn background(&self, name: &str) -> Option<[u8; 4]> {
        let (res, _) = self.lay();
        res.color(&self.page, self.id(name), PropKey::Background).map(|c: Rgba| c.0)
    }

    /// A real flick up the list: the finger crosses the deadzone fast and
    /// lifts mid-motion, so a glide follows.
    fn flick_up(&mut self, x: f32, y: f32, t0: f64) {
        self.touch(Phase::Down, x, y, t0);
        self.touch(Phase::Move, x, y - 30.0, t0 + 0.01);
        self.touch(Phase::Move, x, y - 60.0, t0 + 0.02);
        self.touch(Phase::Up, x, y - 60.0, t0 + 0.03);
    }
}

/// The Godot `_scroll_tests` fixture (`Scroll sv { Column list { Button b1 } }`)
/// with a tall Rectangle standing in for the hand-set scroll range.
const TALL_LIST: &str = "View v { Scroll sv width=100, height=100, halign=left, valign=top { Column list { Button b1 text=\"one\"; Rectangle tall width=80, height=1000 } } }";

// ---------------------------------------------------------------- _scroll_expand_tests

/// `_scroll_expand_tests` 8969-9013: on the axis it does not scroll a
/// Scroll hands its content the viewport width (`bars=never` reserves no
/// lane), taller content keeps its own height and scrolls, and a declared
/// width on the content keeps its meaning.
#[test]
fn scroll_content_takes_the_viewport_width_and_keeps_a_declared_one() {
    let fx = Fx::new("View v { Scroll s bars=never, width=360, height=50, halign=left, valign=top { Panel p { Rectangle r min_size=200x100 } } }");
    let p = fx.rect("p");
    assert_eq!(p.w, 360.0, "{p:?}");
    assert_eq!(p.h, 100.0, "{p:?}");
    let fx = Fx::new("View v { Scroll s width=360, height=50, halign=left, valign=top { Panel p width=80 { Rectangle r min_size=10x10 } } }");
    let p = fx.rect("p");
    assert_eq!(p.w, 80.0, "{p:?}");
}

// ---------------------------------------------------------------- _scroll_units_tests

/// `_scroll_units_tests` 9013-9053. PORT `scroll.units-unpushed`,
/// `units-identity`, `fling-boost`: the pan deadzone is 16 epx (a 16-epx
/// travel is still a tap, one more pans) and a release glides at twice
/// the finger's speed. FLIP `units-pushed`, `units-deadzone`,
/// `units-native-px`, `units-reset` (gogo-design.md §7 "Units: epx only;
/// device px never appear in a `.gogo`"): there is no density to push into
/// the crate — the shell converts device px before gogo sees a touch — and
/// the raw `scroll_deadzone` px escape hatch is not a word (G0042).
#[test]
fn deadzone_is_sixteen_epx_and_the_release_glide_is_boosted_twice() {
    assert_eq!(SCROLL_DEADZONE, 16.0);
    assert_eq!(FLING_BOOST, 2.0);
    let mut fx = Fx::new(TALL_LIST);
    fx.touch(Phase::Down, 50.0, 80.0, 0.0);
    fx.touch(Phase::Move, 50.0, 64.0, 0.01);
    assert_eq!(fx.scroll("sv"), [0.0, 0.0], "16 epx of travel is still a tap");
    fx.touch(Phase::Move, 50.0, 63.0, 0.02);
    assert_eq!(fx.scroll("sv")[1], 1.0, "one more epx pans, from where the deadzone broke");
    // One finger sample of 50 epx over 0.05 s = 1000 epx/s; the glide
    // starts at 2000 (friction takes ~1.5 % in the first 10 ms).
    let mut fx = Fx::new(TALL_LIST);
    fx.touch(Phase::Down, 50.0, 95.0, 0.0);
    fx.touch(Phase::Move, 50.0, 45.0, 0.05);
    fx.touch(Phase::Up, 50.0, 45.0, 0.05);
    assert!(fx.input.is_gliding());
    let before = fx.scroll("sv")[1];
    fx.tick(0.01);
    let speed = (fx.scroll("sv")[1] - before) / 0.01;
    assert!(speed > 1900.0 && speed <= 2000.0, "boosted release speed: {speed}");
    // The px escape hatch is gone with the raw register.
    assert!(codes("View v { Scroll s scroll_deadzone=24 { Column c { Rectangle r 40x600 } } }").contains(&"G0042".to_string()));
}

// ---------------------------------------------------------------- _scroll_hug_tests

/// `_scroll_hug_tests` 9053-9098 (`scroll.hug-ok` .. `hug-quiet`): a hug
/// axis is content-sized, so it does not scroll (nothing left to scroll)
/// and the box measures its content there; the exact axis holds its number
/// and scrolls. Builds without a warning.
#[test]
fn hug_axis_is_content_sized_and_off_while_the_exact_axis_scrolls() {
    let src = "View v { Scroll s height=100, width=hug, halign=left, valign=top { Column c spacing=0 { Rectangle a 80x40, color=red; Rectangle b 120x40, color=red; Rectangle d 60x40, color=red } } }";
    let c = ok(src);
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
    let fx = Fx::new(src);
    assert_eq!(fx.axes("s"), (false, true));
    let s = fx.rect("s");
    assert_eq!((s.w, s.h), (120.0, 100.0), "{s:?}");
}

/// `_scroll_hug_tests` 9086-9089 `scroll.both-hug-warns`: both axes hugged
/// means the box fits its content and nothing scrolls — Godot warns.
#[test]
#[ignore = "GP41: not ported: Godot warns `nothing is left to scroll` on a both-hug Scroll; Rust builds it silently"]
fn a_both_hug_scroll_warns_that_nothing_scrolls() {
    let c = ok("View v { Scroll s width=hug, height=hug { Column c { Rectangle r 40x40, color=red } } }");
    let w = warnings(&c);
    assert!(w.len() == 1 && w[0].contains("nothing is left to scroll"), "{w:?}");
}

/// `_scroll_hug_tests` 9091-9097 `scroll.raw-mode-wins` — FLIP
/// (gogo-design.md §7: the raw register is gone, one vocabulary): Godot
/// let a raw `horizontal_scroll_mode=` own its axis over the hug; here
/// the raw word is not a property — the build is refused (G0042, E8)
/// naming `allow=`, and the same page without the word lets the hug
/// decide.
#[test]
fn a_raw_scroll_mode_word_is_refused_and_the_hug_decides() {
    let raw = "View v { Scroll s width=hug, height=100, horizontal_scroll_mode=1 { Column c { Rectangle r 40x40, color=red } } }";
    let c = compile(raw, &|_| None);
    assert!(gogo::has_errors(&c.diags) && c.diags.iter().any(|d| d.code == "G0042" && d.message.contains("allow=")), "{:#?}", c.diags);
    let fx = Fx::new("View v { Scroll s width=hug, height=100 { Column c { Rectangle r 40x40, color=red } } }");
    assert_eq!(fx.axes("s"), (false, true));
}

// ---------------------------------------------------------------- _scroll_allow_bars_tests

/// `_scroll_allow_bars_tests` 9101-9128 (`scroll.allow-ok`, `allow-off-axis`,
/// `allow-bars-never`, `allow-quiet`, `allow-hug-agrees`): `allow=` names
/// the pan axes (the other axis is off), `bars=never` keeps no bar lane out
/// of the content where the default keeps 4 epx on an overflowing axis, the
/// house shape builds clean, and `allow=` beside a bare hug it does not list
/// agrees with it.
#[test]
fn allow_names_the_pan_axes_and_bars_never_drops_the_lane() {
    let src = "View v { Scroll s height=100, allow=vertical, bars=never, halign=left, valign=top { Column c { Rectangle r 40x40, color=red } } }";
    let c = ok(src);
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
    let fx = Fx::new(src);
    assert_eq!(fx.axes("s"), (false, true));
    let never = Fx::new("View v { Scroll s width=100, height=100, allow=vertical, bars=never, halign=left, valign=top { Column c { Rectangle r 40x400, color=red } } }");
    assert_eq!(never.rect("c").w, 100.0, "{:?}", never.rect("c"));
    let auto = Fx::new("View v { Scroll s width=100, height=100, allow=vertical, halign=left, valign=top { Column c { Rectangle r 40x400, color=red } } }");
    assert_eq!(auto.rect("c").w, 100.0 - SCROLL_BAR_EPX, "{:?}", auto.rect("c"));
    let agree = Fx::new("View v { Scroll s height=100, width=hug, allow=vertical, halign=left, valign=top { Column c { Rectangle r 40x40, color=red } } }");
    assert_eq!(agree.axes("s"), (false, true));
}

/// `_scroll_allow_bars_tests` 9133-9134 `scroll.bars-reserve-ok`:
/// `bars=reserve` builds.
#[test]
fn bars_reserve_builds() {
    let c = ok("View v { Scroll s height=100, bars=reserve { Column c { Rectangle r 40x40, color=red } } }");
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

/// `_scroll_allow_bars_tests` 9135-9142 (`scroll.bars-reserve-modes`,
/// `bars-reserve-min-counts-bar`): `bars=reserve` alone paints BOTH
/// inferred scrolling axes (no hug, no `allow=` = both scroll) in reserve
/// mode, the lane is held even with nothing overflowing, and the minimum
/// counts it.
#[test]
#[ignore = "GP36: not ported: axis inference without allow= is vertical-only here and bars= reads `never` only (no reserve/always lane)"]
fn bars_reserve_holds_the_lane_on_both_inferred_axes() {
    let fx = Fx::new("View v { Scroll s width=100, height=100, bars=reserve, halign=left, valign=top { Column c { Rectangle r 40x40, color=red } } }");
    assert_eq!(fx.axes("s"), (true, true), "no hug, no allow=: both axes scroll");
    let c = fx.rect("c");
    assert_eq!((c.w, c.h), (100.0 - SCROLL_BAR_EPX, 100.0 - SCROLL_BAR_EPX), "the reserved lanes stay out of the content: {c:?}");
}

/// `_scroll_allow_bars_tests` 9145-9148 `scroll.allow-raw-refuses` — FLIP
/// (gogo-design.md §7: the raw register is gone): Godot refused the gogo
/// word and the raw `vertical_scroll_mode=` on one node ("one spelling per
/// Scroll"); here there is only one spelling and the raw word is not a
/// property (G0042).
#[test]
fn allow_beside_a_raw_scroll_mode_word_flags_the_raw_word() {
    let src = "View v { Scroll s allow=vertical, vertical_scroll_mode=1 { Column c { Rectangle r 40x40, color=red } } }";
    assert!(codes(src).contains(&"G0042".to_string()), "{:?}", codes(src));
}

/// `_scroll_allow_bars_tests` 9152-9155 `scroll.allow-bare-hug-refuses`,
/// 9218-9221 `scroll.cap-allow-off-refuses`, 9224-9227 `scroll.cap-fill-refuses`:
/// the Scroll contradictions Godot refuses at load — `allow=` naming a bare
/// hug axis, a capped axis left out of `allow=`, `max_*=fill` on a Scroll.
#[test]
#[ignore = "GP41: not ported: the Scroll contradiction lints (allow= on a bare hug axis, a capped axis left out of allow=, max_*=fill on a Scroll) build silently"]
fn scroll_contradictions_are_refused_at_build() {
    let bare = errors("View v { Scroll s width=hug, height=100, allow=both { Column c { Rectangle r 40x40, color=red } } }");
    assert!(bare.iter().any(|e| e.contains("sizes it to its content")), "{bare:?}");
    let off = errors("View v { Scroll s width=hug, max_width=300, height=100, allow=vertical { Column c { Rectangle r 40x40, color=red } } }");
    assert!(off.iter().any(|e| e.contains("leaves the horizontal axis off")), "{off:?}");
    let fcap = errors("View v { Scroll s width=hug, max_width=fill, height=100 { Column c { Rectangle r 40x40, color=red } } }");
    assert!(fcap.iter().any(|e| e.contains("doesn't go on a Scroll")), "{fcap:?}");
}

/// `_scroll_allow_bars_tests` 9158-9161 `scroll.allow-fenced`: `allow=` is
/// Scroll vocabulary — on a Panel Godot refuses it naming the pan axes.
#[test]
fn allow_on_a_panel_is_refused() {
    let fence = errors("View v { Panel p allow=vertical { Rectangle r 40x40, color=red } }");
    assert!(fence.iter().any(|e| e.contains("pan axes")), "{fence:?}");
}

/// `_scroll_allow_bars_tests` 9166-9216 (`scroll.cap-ok` .. `cap-in-container-granted`):
/// a capped hug axis scrolls PAST its cap — the box stops at the cap, the
/// axis scrolls, the bare hug axis stays off and fits the content, no
/// warning; content under the cap hugs; a declared floor holds; inside a
/// container the Scroll is granted its capped minimum.
#[test]
fn a_capped_hug_axis_scrolls_past_its_cap() {
    let src = "View v { Scroll s width=hug, height=hug, max_height=100, halign=left, valign=top { Column c spacing=0 { Rectangle a 80x40, color=red; Rectangle b 120x40, color=red; Rectangle d 60x40, color=red } } }";
    let c = ok(src);
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
    let fx = Fx::new(src);
    assert_eq!(fx.axes("s"), (false, true));
    let s = fx.rect("s");
    assert_eq!((s.w, s.h), (120.0, 100.0), "{s:?}");
    let small = Fx::new("View v { Scroll s width=hug, height=hug, max_height=200, halign=left, valign=top { Column c spacing=0 { Rectangle a 80x40, color=red } } }");
    assert_eq!(small.rect("s").h, 40.0, "{:?}", small.rect("s"));
    let floored = Fx::new("View v { Scroll s width=hug, height=hug, min_height=60, max_height=200, halign=left, valign=top { Column c spacing=0 { Rectangle a 80x40, color=red } } }");
    assert_eq!(floored.rect("s").h, 60.0, "{:?}", floored.rect("s"));
    let granted = Fx::sized("View screen { Layers z min_width=200, min_height=400 { Column col { Scroll s width=100, height=hug, max_height=100 { Column c spacing=0 { Rectangle a 80x40, color=red; Rectangle b 80x40, color=red; Rectangle d 80x40, color=red } } } } }", 200.0, 400.0);
    assert_eq!(granted.rect("s").h, 100.0, "{:?}", granted.rect("s"));
}

/// `_scroll_allow_bars_tests` 9230-9250 (`list.bars-ok`, `list.bars-never`,
/// `list.allow-fenced`): a List takes the same `bars=` word one level up
/// (its internal scroll goes bar-less), and `allow=` on a List redirects
/// to `layout` (the axis is the layout's).
#[test]
#[ignore = "GP28: not ported: List is a plain Column — no internal scroll to take `bars=`, no `allow=` → `layout` redirect"]
fn list_takes_bars_and_redirects_allow_to_layout() {
    let c = compile("View v { List picker bars=never { empty { } repeat rows by $item.id { ListItem { Label text=$item.title } } } }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    // Godot reads the internal GogoListScroll's modes (vertical SHOW_NEVER,
    // horizontal DISABLED); there is no such node to read here.
    let allow = errors("View v { List picker allow=vertical { empty { } repeat rows by $item.id { ListItem { Label text=$item.title } } } }");
    assert!(allow.iter().any(|e| e.contains("`layout`, not `allow=`")), "{allow:?}");
}

// ---------------------------------------------------------------- _scroll_tests

/// `_scroll_tests` 9251-9359 (all but the twin checks): a Scroll is the
/// pannable kind; a tap is tracked and passed through untouched; a drag
/// under the deadzone neither arms nor scrolls; crossing it arms the pan
/// and applies the travel of the crossing move; the release ends the pan;
/// a touch outside is ignored; a fast release glides; pulling past the top
/// shows a softened content offset (less than the finger travelled) and
/// the release springs it back to the edge; a mostly-sideways flick leaves
/// no x-state on a vertical-only list.
#[test]
fn scroll_tap_deadzone_pan_fling_overscroll_spring_and_axis_lock() {
    let mut fx = Fx::new(TALL_LIST);
    let sv = fx.id("sv");
    assert_eq!(fx.page.nodes[sv as usize].kind, Kind::Scroll);
    // tap: tracked, then passed through untouched
    fx.touch(Phase::Down, 50.0, 80.0, 0.0);
    assert!(fx.input.is_pressed());
    fx.touch(Phase::Up, 50.0, 80.0, 0.1);
    assert!(!fx.input.is_pressed() && !fx.input.is_gliding());
    assert_eq!(fx.scroll("sv"), [0.0, 0.0]);
    // a drag under the deadzone neither arms nor scrolls
    fx.touch(Phase::Down, 50.0, 80.0, 1.0);
    fx.touch(Phase::Move, 50.0, 70.0, 1.01);
    assert_eq!(fx.scroll("sv"), [0.0, 0.0]);
    // crossing the deadzone arms the pan and applies the travel
    fx.touch(Phase::Move, 50.0, 40.0, 1.02);
    assert!(fx.input.is_pressed());
    assert_eq!(fx.scroll("sv")[1], 30.0, "{:?}", fx.scroll("sv"));
    // release ends the pan
    fx.touch(Phase::Up, 50.0, 40.0, 1.03);
    assert!(!fx.input.is_pressed());
    fx.input.reset(&mut fx.rt);
    // touches outside the container are ignored entirely
    fx.touch(Phase::Down, 300.0, 300.0, 1.5);
    assert!(!fx.input.is_pressed());
    // release right after a fast drag glides
    fx.touch(Phase::Down, 50.0, 80.0, 2.0);
    fx.touch(Phase::Move, 50.0, 40.0, 2.01);
    fx.touch(Phase::Up, 50.0, 40.0, 2.02);
    assert!(fx.input.is_gliding());
    fx.input.reset(&mut fx.rt);
    // pulling past the top shows a softened content offset...
    fx.rt.scroll[sv as usize] = [0.0, 0.0];
    let top = fx.rect("list").y;
    fx.touch(Phase::Down, 50.0, 20.0, 3.0);
    fx.touch(Phase::Move, 50.0, 30.0, 3.01);
    fx.touch(Phase::Move, 50.0, 60.0, 3.02);
    let shown = fx.scroll("sv")[1];
    assert!(shown < 0.0, "pulled past the top: {shown}");
    assert!(fx.rect("list").y > top, "{:?}", fx.rect("list"));
    assert!(-shown < 30.0, "give must diminish: {shown} for 30 epx of pull");
    // ...and the release springs the excess back to the boundary (the
    // finger rested before lifting, so no glide joins the spring)
    fx.touch(Phase::Up, 50.0, 60.0, 3.5);
    for _ in 0..200 {
        fx.tick(0.016);
    }
    assert_eq!(fx.scroll("sv"), [0.0, 0.0]);
    assert!((fx.rect("list").y - top).abs() < 0.5, "{:?}", fx.rect("list"));
    assert!(!fx.input.is_gliding());
    // a flick with a sideways component leaves NO x-state on a vertical-only list
    fx.touch(Phase::Down, 20.0, 50.0, 4.0);
    fx.touch(Phase::Move, 60.0, 45.0, 4.01);
    fx.touch(Phase::Up, 60.0, 45.0, 4.02);
    for _ in 0..30 {
        fx.tick(0.016);
    }
    assert_eq!(fx.scroll("sv"), [0.0, 0.0]);
    assert!(!fx.input.is_gliding());
    assert_eq!(fx.rect("list").x, 0.0);
}

// ---------------------------------------------------------------- _scroll_catch_press_tests

/// `_scroll_catch_press_tests` 9359-9415 (`scroll.catch-ok`, `catch-arms`,
/// `catch-cancels-press`, `catch-releases-clean`, `catch-bg-restored`): a
/// tap that catches a gliding list stops the glide and must not press the
/// button under the finger — no `held`, no `when held` look, no activate
/// on the lift, nothing left pressed — where the same tap on a resting list
/// presses and activates.
#[test]
fn a_tap_that_catches_a_glide_presses_nothing_and_leaves_no_held_look() {
    let src = "View v { Scroll sv width=100, height=100, halign=left, valign=top { Column list { Button b text=\"tap\", height=400 { when held background=#00FF00FF }; Rectangle tall width=80, height=1000 } } }";
    let mut fx = Fx::new(src);
    let b = fx.id("b");
    fx.flick_up(50.0, 90.0, 0.0);
    assert!(fx.input.is_gliding());
    fx.tick(0.016);
    fx.touch(Phase::Down, 50.0, 50.0, 1.0);
    assert!(!fx.input.is_gliding(), "the touch catches the glide");
    assert!(!fx.rt.has(b, HELD), "the catch presses nothing");
    assert_ne!(fx.background("b"), Some(GREEN), "no held look under the catch");
    let ev = fx.touch(Phase::Up, 50.0, 50.0, 1.1);
    assert!(ev.iter().all(|e| e.kind != EventKind::Activate), "{ev:?}");
    assert!(!fx.rt.has(b, HELD) && !fx.input.is_pressed());
    assert_ne!(fx.background("b"), Some(GREEN), "the base look is back, no parked green");
    // control: the same tap on a resting list presses and activates
    fx.touch(Phase::Down, 50.0, 50.0, 2.0);
    assert!(fx.rt.has(b, HELD));
    assert_eq!(fx.background("b"), Some(GREEN));
    let ev = fx.touch(Phase::Up, 50.0, 50.0, 2.1);
    assert!(ev.iter().any(|e| e.kind == EventKind::Activate), "{ev:?}");
    assert_ne!(fx.background("b"), Some(GREEN));
}

// ---------------------------------------------------------------- _scroll_occlusion_tests

/// The Godot `_scroll_occlusion_tests` shape: a Scroll filling a 100×100
/// box and a `back` Button drawn over its lower-left as a LATER sibling,
/// outside the Scroll's subtree.
const OCCLUDED: &str = "View v { Layers root width=100, height=100, halign=left, valign=top { Scroll sv width=fill, height=fill { Column list { Button item text=\"i\", height=400; Rectangle tall width=80, height=1000 } }; Button back width=40, height=20, halign=left, valign=top, margin_left=10, margin_top=70 } }";

/// `_scroll_occlusion_tests` 9435-9440 `scroll.occluder-passes`: a
/// mid-glide tap on chrome drawn over the Scroll but outside its subtree is
/// never claimed — the glide continues underneath and the chrome presses.
#[test]
#[ignore = "GPAR4: any touch kills a fling — Godot leaves the glide alone when the tap lands on chrome drawn over the Scroll (D130) and presses that chrome"]
fn a_tap_on_overlay_chrome_never_claims_the_glide() {
    let mut fx = Fx::new(OCCLUDED);
    let back = fx.id("back");
    fx.flick_up(50.0, 60.0, 0.0);
    assert!(fx.input.is_gliding());
    fx.touch(Phase::Down, 30.0, 80.0, 1.0);
    assert!(fx.input.is_gliding(), "the glide continues underneath");
    assert!(fx.rt.has(back, HELD), "the overlay presses even while the list glides");
    fx.touch(Phase::Up, 30.0, 80.0, 1.1);
}

/// `_scroll_occlusion_tests` 9443-9475 (`scroll.occluder-catches-elsewhere`,
/// `occluder-no-pan`, `occluder-ignore-transparent`, `occluder-hidden-transparent`):
/// the same glide is caught on the list itself; a drag starting on overlay
/// chrome never arms the pan; hit-transparent (`interactive=false`) or
/// hidden chrome does not occlude — the touch reaches the list and catches.
#[test]
fn overlay_chrome_keeps_its_own_drag_and_transparent_or_hidden_chrome_lets_the_catch_through() {
    let mut fx = Fx::new(OCCLUDED);
    let item = fx.id("item");
    fx.flick_up(50.0, 60.0, 0.0);
    assert!(fx.input.is_gliding());
    fx.touch(Phase::Down, 50.0, 20.0, 1.0);
    assert!(!fx.input.is_gliding() && !fx.rt.has(item, HELD), "the glide is caught on the list");
    assert!(fx.touch(Phase::Up, 50.0, 20.0, 1.1).is_empty());
    let before = fx.scroll("sv");
    fx.touch(Phase::Down, 30.0, 80.0, 2.0);
    fx.touch(Phase::Move, 30.0, 40.0, 2.1);
    assert_eq!(fx.scroll("sv"), before, "a drag from overlay chrome never pans");
    fx.touch(Phase::Up, 30.0, 40.0, 2.2);
    for variant in ["interactive=false", "visibility=space_only"] {
        let src = OCCLUDED.replace("Button back ", &format!("Button back {variant}, "));
        let mut fx = Fx::new(&src);
        let item = fx.id("item");
        let back = fx.id("back");
        fx.flick_up(50.0, 60.0, 0.0);
        assert!(fx.input.is_gliding(), "{variant}");
        fx.touch(Phase::Down, 30.0, 80.0, 1.0);
        assert!(!fx.input.is_gliding(), "{variant}: the catch claims through the transparent chrome");
        assert!(!fx.rt.has(item, HELD) && !fx.rt.has(back, HELD), "{variant}: nothing pressed");
        assert!(fx.touch(Phase::Up, 30.0, 80.0, 1.1).is_empty(), "{variant}");
    }
}

// ---------------------------------------------------------------- _accessibility_tests

/// `_accessibility_tests` 9480-9487 `a11y.ok`, 9552-9557 `a11y.radio-ok`,
/// 9576-9582 `a11y.reduce-ok`: the fixtures the AccessKit checks stand on
/// build here too (a gogo-rendered Slider beside a Toggle, a `choice` of
/// Radios, a style with a timed held look).
#[test]
fn accessibility_fixtures_build() {
    ok("View v { Column c { Slider vol render=gogo, value=30, min=0, max=100, step=5; Toggle mute } }");
    ok("View v { Column c { choice size value=small { Radio small text=\"S\"; Radio big text=\"B\" } } }");
    ok("style RM {\n transition duration=0.4\n when held background=#FF0000FF\n}\nView v { Button rb .RM text=\"x\", min_size=44x44 }");
}

/// `_accessibility_tests` 9516-9525 `a11y.list-ok`: a List with keyed
/// `repeat` rows and `$item` bindings builds (the rows themselves are not
/// instantiated without `max=` — GP27's owed bindings — but the source is
/// accepted, which is all this Godot check asserts).
#[test]
fn a_list_with_keyed_repeat_rows_builds() {
    let c = compile("View v { List picker spacing=2 { empty { Label text=\"none\" } repeat tasks by $item.id { ListItem { Label text=$item.title } } } }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

/// `_accessibility_tests` 9568-9593 (`a11y.reduce-effective`,
/// `reduce-instant`, `reduce-auto-headless`): with the reduced-motion
/// override on, every mover runs the instant path — a 0.4 s `when held`
/// tween lands in one frame; with the override off the tween takes its time.
#[test]
#[ignore = "GP42: not ported: no reduced-motion / motion-speed override — the 0.4 s held tween is mid-flight after one frame"]
fn reduced_motion_lands_a_timed_response_in_one_frame() {
    let src = "style RM {\n transition duration=0.4\n when held background=#FF0000FF\n}\nView v { Button rb .RM text=\"x\", min_size=44x44, background=#000000FF }";
    let mut fx = Fx::new(src);
    let rb = fx.id("rb");
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    // Godot: GogoMotion.reduce_motion_override = 1 → effective_speed() == INF.
    fx.rt.set_state(rb, HELD, true);
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    fx.rt.tick(&mut fx.page, 0.016);
    let res = resolve_animated(&fx.page, &mut fx.rt);
    let bg = res.color(&fx.page, rb, PropKey::Background).map(|c| c.0);
    assert_eq!(bg, Some([255, 0, 0, 255]), "under reduced motion the tween lands at once");
}

/// `_accessibility_tests` 9596-9605 `a11y.name-translates`: a `/key`
/// suffix on `accessibility_name` translates like any string prop — with
/// the `xx` table `{undoA11y: "Undo-XX"}` active the name reads "Undo-XX".
#[test]
fn accessibility_name_translates_by_its_key() {
    static XX: &[(&str, &str)] = &[("undoA11y", "Undo")];
    let c = ok("View v { Button tb 44x44, text=\"U\", accessibility_name=\"Undo\"/undoA11y }");
    assert_eq!(c.keys.names, ["undoA11y"]);
    let tb = c.page.find("tb").unwrap();
    let v = c.page.prop(tb, PropKey::AccessibilityName);
    let locale = gogo::Locale::build(XX, &[(0, "Undo-XX")]);
    let shown = match v {
        Some(Value::VText { text, key }) => gogo::locale::display(&c.page, Some(&locale), text, key),
        other => panic!("{other:?}"),
    };
    assert_eq!(shown, "Undo-XX");
}

/// `_accessibility_tests` 9609-9618 (`a11y.kb-lint`, `a11y.kb-lint-silenced`):
/// a button no keyboard can reach warns naming the keyboard; the touch-only
/// acceptance `keyboard_target=off` silences it.
#[test]
#[ignore = "GP26: not ported: keyboard focus — no keyboard-reachability lint (the raw `focus_mode` word is only G0042 here)"]
fn a_focusless_button_warns_unless_accepted_as_touch_only() {
    let kb = compile("View v { Button kbtn text=\"x\", min_size=44x44, focus_mode=none }", &|_| None);
    let w = warnings(&kb);
    assert!(!w.is_empty() && w[0].contains("keyboard"), "{w:?}");
    let off = compile("View v { Button kbtn2 text=\"x\", min_size=44x44, focus_mode=none, keyboard_target=off }", &|_| None);
    assert!(warnings(&off).is_empty(), "{:?}", warnings(&off));
}

// ---------------------------------------------------------------- _focus_visible_tests

/// `_focus_visible_tests` 9632-9637 `fvis.ok`, 9658-9663 `fvis.key-latch` /
/// `fvis.exit-clears`, 9672-9683 `fvis.caret-ok` / `caret-indicates` /
/// `caret-clears`, 9688 `fring.ok`, 9721-9736 `fwithin.ok`: `focused` is
/// visible focus — the host sets the bit when an indicator is warranted (a
/// key-driven entry; any focus on a caret control), and the authored `when
/// focused` response follows the bit on and off. The ring and focus_within
/// fixtures build.
#[test]
fn focused_state_drives_the_authored_response_and_clears_on_exit() {
    let mut fx = Fx::new("View v { Button b text=\"x\" { when focused background=#00FF00FF } }");
    let b = fx.id("b");
    assert_ne!(fx.background("b"), Some(GREEN));
    fx.rt.set_state(b, FOCUSED, true);
    assert_eq!(fx.background("b"), Some(GREEN), "a key-driven focus lights the authored response");
    fx.rt.set_state(b, FOCUSED, false);
    assert_ne!(fx.background("b"), Some(GREEN), "exit clears it");
    // caret exception: a text field indicates on ANY focus — clicked included
    let mut fx = Fx::new("View v { Input field { when focused opacity=0.5 } }");
    let f = fx.id("field");
    fx.rt.set_state(f, FOCUSED, true);
    assert_eq!(fx.lay().0.num(&fx.page, f, PropKey::Opacity), Some(0.5));
    fx.rt.set_state(f, FOCUSED, false);
    assert_ne!(fx.lay().0.num(&fx.page, f, PropKey::Opacity), Some(0.5));
    ok("View v { Column c { Button r text=\"x\"; Button q text=\"y\", corner_radius=12 } }");
    ok(FOCUS_WITHIN);
}

/// `_focus_visible_tests` 9641-9657 (`fvis.pointer-focus-quiet`,
/// `fvis.key-upgrade`, `fvis.pointer-downgrade`): the visible-focus latch
/// reads input modality — a focus parked by a click shows no indicator, a
/// key press while focused upgrades it on, a pointer press anywhere turns
/// it off.
#[test]
#[ignore = "GP26: not ported: keyboard focus / input modality — the `when focused` response follows the bit regardless of how focus arrived"]
fn pointer_parked_focus_stays_quiet_until_a_key_drives() {
    let mut fx = Fx::new("View v { Button b text=\"x\" { when focused background=#00FF00FF } }");
    let b = fx.id("b");
    // Godot: GogoStates.nav_pointer(); focus_entered → no indicator.
    fx.rt.set_state(b, FOCUSED, true);
    assert_ne!(fx.background("b"), Some(GREEN), "a click-parked focus shows no indicator");
    // Godot: nav_keyboard() → the indicator upgrades on; nav_pointer() → off again.
}

/// The Godot settings-row fixture: a row that lights while the focus is on
/// or inside it, and a button outside it.
const FOCUS_WITHIN: &str = "View v { Column top {\n Panel rowA background=#00000000 {\n  when focus_within background=#FFFFFF22\n  Row r { Label rowLabel text=\"Music\"; Button inner text=\"s\" }\n }\n Button outside text=\"o\"\n} }";

/// `_focus_visible_tests` 9739-9760 (`fwithin.descendant-lights`,
/// `fwithin.outside-clears`, `fwithin.tap-quiet`, `fwithin.key-upgrade`):
/// `focus_within` lights the row while a key-driven focus is INSIDE it,
/// clears when the focus leaves, stays quiet for a tap into the row and
/// lights when the keys start driving.
#[test]
#[ignore = "GP26: not ported: focus routing — `focus_within` is never derived from a focused descendant (and there is no modality gate)"]
fn focus_within_lights_the_row_while_focus_is_inside() {
    let mut fx = Fx::new(FOCUS_WITHIN);
    let inner = fx.id("inner");
    let outside = fx.id("outside");
    fx.rt.set_state(inner, FOCUSED, true);
    assert_eq!(fx.background("rowA"), Some([255, 255, 255, 0x22]), "a focused descendant lights the row");
    fx.rt.set_state(inner, FOCUSED, false);
    fx.rt.set_state(outside, FOCUSED, true);
    assert_eq!(fx.background("rowA").map(|c| c[3]), Some(0), "focus outside clears it");
}

/// `_focus_visible_tests` 9764-9765 `fwithin.err-suggests`: an unknown
/// state word errors and the hint names `focus_within`.
#[test]
fn an_unknown_state_word_suggests_focus_within() {
    let c = compile("View v { Button t text=\"x\" { when focus_withn opacity=0.5 } }", &|_| None);
    assert!(c.diags.iter().any(|d| d.code == "G0036" && d.message.contains("focus_within")), "{:#?}", c.diags);
}

// ---------------------------------------------------------------- _stranded_probe_tests

/// The D77 idle-state walk: every node whose transient input condition
/// (`held`, `hovered`) is active right now. Standing states (`selected`,
/// `disabled`, `focused`) stay out by design.
fn stranded(fx: &Fx) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, n) in fx.page.nodes.iter().enumerate() {
        let name = fx.page.str(n.name);
        if fx.rt.has(i as NodeId, HELD) {
            out.push(format!("{name}: held"));
        }
        if fx.rt.has(i as NodeId, HOVERED) {
            out.push(format!("{name}: hovered"));
        }
    }
    out
}

/// `_stranded_probe_tests` 9765-9812 (`strand.ok`, `rest-clean`,
/// `pressed-reported`, `checked-filtered`, `release-clean`): the invariant
/// behind `gogo.stranded_conditions` (the probe itself is GP42 dev
/// tooling): a transient condition is active only while the gesture holds
/// it — clean at rest, `held` reported while the finger is down, `selected`
/// is a standing state and never a strand, clean again after the release.
#[test]
fn transient_conditions_are_active_only_while_the_finger_is_down() {
    let src = "style Look {\n when held background=#FFFFFF22\n when hovered background=#FFFFFF11\n when selected background=#E51400FF\n}\nView v { Column top { Button b .Look toggle=true, text=\"row\" } }";
    let mut fx = Fx::new(src);
    assert!(stranded(&fx).is_empty(), "{:?}", stranded(&fx));
    let r = fx.rect("b");
    fx.touch(Phase::Down, r.x + 5.0, r.y + 5.0, 0.0);
    assert_eq!(stranded(&fx), vec!["b: held".to_string()]);
    let b = fx.id("b");
    fx.rt.set_state(b, SELECTED, true);
    assert_eq!(stranded(&fx), vec!["b: held".to_string()], "selected is standing, never a strand");
    fx.touch(Phase::Up, r.x + 5.0, r.y + 5.0, 0.1);
    assert!(stranded(&fx).is_empty(), "{:?}", stranded(&fx));
}

/// `_stranded_probe_tests` 9814-9827 (`strand.hover-reported`,
/// `strand.hover-clean`): a real mouse move over a button sets `hovered`
/// (reported as a strand while it lasts) and leaving clears it.
#[test]
#[ignore = "GP26: not ported: hover — no pointer-flavor sampler ever sets `hovered` (the stranded_conditions probe itself is GP42)"]
fn hover_strands_report_and_clear() {
    let src = "style Look {\n when hovered background=#FFFFFF11\n}\nView v { Column top { Button b .Look text=\"row\" } }";
    let mut fx = Fx::new(src);
    let r = fx.rect("b");
    fx.touch(Phase::Move, r.x + 5.0, r.y + 5.0, 0.0);
    assert_eq!(stranded(&fx), vec!["b: hovered".to_string()]);
    fx.touch(Phase::Move, 390.0, 790.0, 0.1);
    assert!(stranded(&fx).is_empty(), "{:?}", stranded(&fx));
}

// ---------------------------------------------------------------- _motion_group_tests

/// The Godot `fixtures/state_view.gogo`, in the block form this compiler
/// takes: one timed arrangement fading a scrim and moving a bar.
const STATE_VIEW: &str = "arrangement collapsed\narrangement wide\n\ntransition duration=0.2, easing=cubic_out\n\nwhen collapsed {\n bar.margin_bottom = -40\n bar.height = 90\n scrim.opacity = 0.5\n}\nwhen wide {\n bar.height = 120\n}\n\nstyle Dot min_width=6, min_height=6 {\n when collapsed opacity=0.25\n}\n\nView screen {\n Rectangle scrim color=#000000FF\n Rectangle bar color=#FFFFFFFF, width=fill, valign=bottom, height=60\n Rectangle dot .Dot color=#FFFFFFFF, halign=left, valign=top\n Button goBtn 50x50, text=\"go\", halign=right, valign=top, on:activate=go(collapsed)\n Button toggleBtn 50x50, text=\"tg\", halign=right, valign=bottom, on:activate=toggle(collapsed)\n}\n";

/// `_motion_group_tests` 9868-9891 (`mgroup.starts-empty`, `states-build`,
/// `states-joins`, `states-leaves`) and 9915-9939 (`scroll-build`,
/// `scroll-joins`, `scroll-leaves`): the idle-gate contract — nothing pumps
/// on a fresh page; a timed arrangement flip pumps while its tween glides
/// and stops when it lands; a fling pumps past the lift and stops within
/// ~10 s of frame-sized steps (friction plus any edge spring).
#[test]
fn motion_pumps_run_exactly_while_something_moves() {
    let mut fx = Fx::new(STATE_VIEW);
    assert!(!fx.rt.animating() && !fx.input.is_gliding());
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    fx.rt.go(0);
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    assert!(fx.rt.animating(), "a timed arrangement flip pumps while gliding");
    fx.rt.tick(&mut fx.page, 0.5);
    assert!(!fx.rt.animating(), "the tween landed: the pump leaves");
    let mut fx = Fx::new("View v { Scroll sv width=100, height=100, halign=left, valign=top { Column list { Rectangle tall 80x600 } } }");
    fx.flick_up(50.0, 90.0, 0.0);
    assert!(fx.input.is_gliding(), "the fling armed");
    assert!(fx.tick(1.0 / 60.0), "and pumps past the lift");
    let mut frames = 1;
    while fx.input.is_gliding() && frames < 600 {
        fx.tick(1.0 / 60.0);
        frames += 1;
    }
    assert!(!fx.input.is_gliding(), "still gliding after {frames} frames");
}

/// `_motion_group_tests` 9894-9912 (`mgroup.driver-build`, `driver-joins`,
/// `driver-leaves`). FLIP for `driver-joins` (todo-gogo.md GP23 SHIPPED:
/// "the glide is the file's `transition` timing"): Godot's Toggle driver
/// glides the thumb 0.12 s on every value write; here a value write with
/// no `transition` in the file snaps and nothing pumps, and with one the
/// flip's tween pumps while it runs and leaves when it lands.
#[test]
fn a_toggle_value_write_pumps_only_by_the_files_transition() {
    let mut fx = Fx::new("View v { Toggle mute }");
    let t = fx.id("mute");
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    fx.rt.set_prop(t, PropKey::Value, Value::VNum(1.0));
    fx.rt.set_state(t, SELECTED, true);
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    assert!(!fx.rt.animating(), "no transition in the file: the write snaps, nothing pumps");
    let mut fx = Fx::new("View v { Toggle mute background=#000000FF { when selected background=#FF0000FF\n transition any -> selected duration=0.12 } }");
    let t = fx.id("mute");
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    fx.rt.set_prop(t, PropKey::Value, Value::VNum(1.0));
    fx.rt.set_state(t, SELECTED, true);
    let _ = resolve_animated(&fx.page, &mut fx.rt);
    assert!(fx.rt.animating(), "the file's transition glides the flip");
    fx.rt.tick(&mut fx.page, 0.5);
    assert!(!fx.rt.animating(), "the glide landed: the pump leaves");
}

// ---------------------------------------------------------------- _entrance_tests

/// The Godot entrance fixture: a root that plays `appear` on show.
const ENTRANCE: &str = "anim appear { box.opacity from=0.0, to=1.0, duration=0.5 }\nPanel viewRoot on:show=play(appear) {\n Rectangle box color=#FFFFFFFF, min_width=10, min_height=10\n}\n";

/// `_entrance_tests` 9971-10014 (`entrance.ok`, `plays`, `from-asserted`,
/// `suppressed`, `hidden-silent`, `show-flip-plays`): a root that lands on
/// screen visible plays its show anim once, with the `from=` asserted on
/// the first tick; a host that wants no entrances (the editor preview)
/// simply never asks for them and nothing plays on its own; a root built
/// `visibility=gone` is state, not a trigger — its show flip still plays.
#[test]
fn a_root_landing_visible_plays_its_show_anim_once() {
    let c = ok(ENTRANCE);
    let mut page = c.page;
    let bx = page.find("box").unwrap() as usize;
    let appear = page.intern_lookup("appear");
    let mut rt = Runtime::new(&page);
    rt.play_entrances(&page);
    assert!(rt.is_playing(appear), "appearing plays the show anim");
    rt.tick(&mut page, 0.016);
    assert!(rt.motion[bx].opacity < 0.99, "the from= is asserted: {}", rt.motion[bx].opacity);
    // suppressed: no entrance asked for, none plays
    let c = ok(ENTRANCE);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    assert!(!rt.tick(&mut page, 0.016));
    assert!(!rt.is_playing(appear));
    assert_eq!(rt.motion[bx].opacity, 1.0);
    // a hidden root is state, not a trigger; the SHOW flip plays
    let c = ok(&ENTRANCE.replace("viewRoot on:show", "viewRoot visibility=gone, on:show"));
    let mut page = c.page;
    let root = page.find("viewRoot").unwrap();
    let appear = page.intern_lookup("appear");
    let mut rt = Runtime::new(&page);
    rt.play_entrances(&page);
    assert!(!rt.is_playing(appear), "a hidden root plays no entrance");
    rt.show(&mut page, root, true);
    assert!(rt.is_playing(appear), "the show flip plays");
}

/// The Godot hide-restart fixture: a fade-in entrance and a two-channel
/// vanish on hide (the show anim deliberately leaves `translate.x` alone).
const DRIFT: &str = "anim fadein { box.opacity from=0.0, to=1.0, duration=0.5 }\nanim vanish duration_each=0.5 {\n box.translate.x to=-60\n and box.opacity to=0.0\n}\nPanel driftRoot on:show=play(fadein), on:hide=play(vanish) {\n Rectangle box color=#FFFFFFFF, min_width=10, min_height=10\n}\n";

/// `_entrance_tests` 10023-10050 (`visfx.drift-ok`, `visfx.drift-mid`): the
/// entrance completes to opacity 1 at x 0; a hide starts the vanish and a
/// quarter second in the box sits mid-journey at x −30, opacity 0.5.
#[test]
fn a_hide_mid_journey_reads_the_mid_values() {
    let c = ok(DRIFT);
    let mut page = c.page;
    let bx = page.find("box").unwrap() as usize;
    let root = page.find("driftRoot").unwrap();
    let mut rt = Runtime::new(&page);
    rt.play_entrances(&page);
    rt.tick(&mut page, 1.0);
    assert_eq!((rt.motion[bx].opacity, rt.motion[bx].translate[0]), (1.0, 0.0));
    rt.show(&mut page, root, false);
    rt.tick(&mut page, 0.25);
    let m = rt.motion[bx];
    assert!((m.translate[0] + 30.0).abs() < 1e-3 && (m.opacity - 0.5).abs() < 1e-3, "x={} a={}", m.translate[0], m.opacity);
}

/// `_entrance_tests` 10051-10060 `visfx.drift-restores-as-built`: a second
/// hide during the deferred hide restarts the vanish from mid-journey; when
/// it finishes the root is hidden and the box is back as built (x 0,
/// opacity 1, no motion armed) — the restarted player must not memorize
/// the mid-journey values as its base.
#[test]
#[ignore = "GPAR54: no as-built ledger — the finished hide leaves the box parked at the vanish end (x −60, opacity 0) instead of restoring x 0, opacity 1"]
fn a_restarted_hide_restores_the_as_built_values_on_finish() {
    let c = ok(DRIFT);
    let mut page = c.page;
    let bx = page.find("box").unwrap() as usize;
    let root = page.find("driftRoot").unwrap();
    let mut rt = Runtime::new(&page);
    rt.play_entrances(&page);
    rt.tick(&mut page, 1.0);
    rt.show(&mut page, root, false);
    rt.tick(&mut page, 0.25);
    rt.show(&mut page, root, false);
    rt.tick(&mut page, 1.0);
    let res = resolve(&page, &rt);
    assert!(res.gone(&page, root), "the root hid on finish");
    let m = rt.motion[bx];
    assert!(m.translate[0].abs() < 1e-3 && (m.opacity - 1.0).abs() < 1e-3, "as built: x={} a={}", m.translate[0], m.opacity);
}
