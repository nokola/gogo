//! Port of the Godot gogo suite's state batch
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//! - `_state_tests` 5705-6358 (101 checks): style/node `when` responses,
//!   `transition` rules, held/selected/hovered/disabled, child targets,
//!   widget scoping, the retired spellings, the polish props.
//! - `_sweep_banner_tests` 6358-6393 (5): ENGINE, see below.
//! - `_touch_lint_tests` 6393-6529 (14): the 44-epx touch-target lint.
//! - `_exact_overflow_lint_tests` 6529-6604 (6): the D126 exact-size lint.
//! - `_explain_api_tests` 6604-6682 (19): the `gogo.explain()` readout.
//!
//! Godot snippets are translated to the Rust dialect before compiling
//! (`pressed`→`held`, `flash.visible=`→`flash.visibility=`,
//! `modulate.a`→`opacity`, `texture=`→`source=`, `focus_mode=none`
//! dropped — a Godot focus-ring word, keyboard focus is GP26). A top-level
//! anim's `self.` (Godot: the style's wearer) is written as the wearer's
//! name — Rust only knows `self` inside a widget's own anim.
//!
//! ENGINE (not ported, Godot machinery with no Rust counterpart):
//! - `state.sweep-raw-not-instant/-catches/-warned/-banner-suppressed`
//!   (6300-6320): raw `set_pressed_no_signal` sweep — no raw engine writes.
//! - `_sweep_banner_tests` (all 5): on-screen raw-write banner overlay.
//! - `state.theme-bg-shape` (5960): copies the theme StyleBox's corners.
//! - `state.padded-copies` (6056): per-state theme StyleBox copies.
//! - `explain.compiled/filter/regime/engine-default/dead-safe`
//!   (6609-6615): readout prints anchors, grow, mouse_filter, live Control.
use gogo::draw::{DEFAULT_BUTTON_BACKGROUND, DEFAULT_TEXT_COLOR, Draw, draw, hit};
use gogo::input::{Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Align, PropKey, Rgba, State};
use gogo::runtime::{DISABLED, HELD, HOVERED, Resolved, Runtime, SELECTED, resolve, resolve_animated};
use gogo::{Compiled, Page, Severity, compile};

const FRAME: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 400.0 };

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn fails(src: &str) -> bool {
    gogo::has_errors(&compile(src, &|_| None).diags)
}

fn warnings(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.clone()).collect()
}

/// The Godot suite's `build` + `_ready` + hand-driven `_process`: one page,
/// its runtime, one finger.
struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = ok(src);
        let rt = Runtime::new(&c.page);
        Fx { page: c.page, rt, input: Input::new() }
    }
    fn id(&self, name: &str) -> u32 {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }
    /// The frame's values with tweens applied (the Godot `_process` view).
    fn res(&mut self) -> Resolved {
        resolve_animated(&self.page, &mut self.rt)
    }
    fn lay(&mut self) -> (Resolved, Vec<Rect>) {
        let res = self.res();
        let rects = layout(&self.page, &res, FRAME, &FixedAdvance);
        (res, rects)
    }
    fn draws(&mut self) -> Vec<Draw> {
        let (res, rects) = self.lay();
        draw(&self.page, &res, &rects)
    }
    fn rect(&mut self, name: &str) -> Rect {
        let id = self.id(name) as usize;
        self.lay().1[id]
    }
    /// Godot's `for i in 20: st._process(0.016)`.
    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.rt.tick(&mut self.page, 0.016);
            self.res();
        }
    }
    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) {
        let (res, rects) = self.lay();
        self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase, x, y, t, id: 1 });
    }
    fn background(&mut self, name: &str) -> Option<Rgba> {
        let id = self.id(name);
        self.res().color(&self.page, id, PropKey::Background)
    }
    fn text_alpha(&mut self, word: &str) -> u8 {
        let list = self.draws();
        list.iter()
            .find_map(|d| match d {
                Draw::Text { text, color, .. } if self.page.str(*text) == word => Some(color.0[3]),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no text {word:?} in {list:#?}"))
    }
}

fn rgba(hex: u32) -> Option<Rgba> {
    Some(Rgba(hex.to_be_bytes()))
}

// ------------------------------------------------------------ _state_tests

/// `_state_tests` 5708-5747 (`state.ok` … `state.unchecked-restores`,
/// PORT): a style's `when` lines attach to the wearer, its `transition
/// any -> selected duration=0.1` is baked for that node, selecting through
/// the runtime applies the selected color, and deselecting hands the
/// background back to the default look (the Button never declared one).
#[test]
fn style_whens_attach_and_selected_round_trips() {
    let mut fx = Fx::new("style Gallery {\n    transition any -> selected duration=0.1, easing=cubic_out\n    when held background=#FFFFFF22\n    when selected background=#E51400FF\n}\nView v { Button row .Gallery toggle=true, text=\"paint\" }");
    let row = fx.id("row");
    let node = fx.page.nodes[row as usize];
    let states: Vec<State> = fx.page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize].iter().map(|w| w.state).collect();
    assert!(states.contains(&State::Held) && states.contains(&State::Selected), "{states:?}");
    let rule = fx.page.transitions.iter().find(|t| t.node == row && t.to == Some(State::Selected)).copied();
    assert!(matches!(rule, Some(t) if t.from.is_none() && (t.duration - 0.1).abs() < 1e-6), "{:?}", fx.page.transitions);
    assert_eq!(fx.background("row"), None);
    fx.rt.set_state(row, SELECTED, true);
    fx.ticks(21);
    assert_eq!(fx.background("row"), rgba(0xE51400FF));
    fx.rt.set_state(row, SELECTED, false);
    fx.ticks(21);
    assert_eq!(fx.background("row"), None, "the default look is back, no parked box");
    let list = fx.draws();
    assert!(list.iter().any(|d| matches!(d, Draw::Quad { color, .. } if *color == DEFAULT_BUTTON_BACKGROUND)), "{list:#?}");
}

/// `_state_tests` 5749-5754 (`state.node-override`, `-attached`, PORT): a
/// `when held` inside a Button's own body builds and lands on that node.
#[test]
fn node_level_when_builds_and_attaches() {
    let c = ok("View v { Button b text=\"x\" {\n    when held background=#B00020FF\n} }");
    let b = c.page.find("b").unwrap();
    let node = c.page.nodes[b as usize];
    assert_eq!(node.whens.1, 1, "{:?}", c.page.whens);
    assert_eq!(c.page.whens[node.whens.0 as usize].state, State::Held);
}

/// `_state_tests` 5764-5779 (`state.hover-ok`, `-mouse-applies`,
/// `-mouse-exits`, PORT): a `when hovered` response applies while the
/// node's hovered state is on and clears when it goes off. Godot drives the
/// state from `mouse_entered`; here the runtime bit stands in — the mouse
/// producer itself is GP26.
#[test]
fn hovered_response_follows_the_hovered_state() {
    let mut fx = Fx::new("style Row background=#00000000 {\n    when hovered background=#FFFFFF22\n}\nView v { Button b .Row text=\"row\" }");
    let b = fx.id("b");
    fx.rt.set_state(b, HOVERED, true);
    assert_eq!(fx.background("b"), rgba(0xFFFFFF22));
    fx.rt.set_state(b, HOVERED, false);
    assert_eq!(fx.background("b").map(|c| c.0[3]), Some(0), "{:?}", fx.background("b"));
}

/// `_state_tests` 5772-5800 (`state.hover-no-input-tap`,
/// `-unarmed-suppressed`, `-touch-suppressed`, `-drag-suppressed`): hover
/// answers a REAL mouse only (D75) — a finger, an emulated pointer or a
/// zero-motion event never hovers. Rust has no hover producer yet.
#[test]
#[ignore = "GP26: not ported: hover (no mouse producer; a finger must never set hovered)"]
fn hover_needs_real_mouse_evidence() {
    let mut fx = Fx::new("style Row background=#00000000 {\n    when hovered background=#FFFFFF22\n}\nView v { Button b .Row text=\"row\", halign=left, valign=top }");
    let b = fx.id("b");
    fx.touch(Phase::Down, 5.0, 5.0, 0.0);
    fx.touch(Phase::Move, 6.0, 5.0, 0.1);
    assert!(!fx.rt.has(b, HOVERED), "a finger never hovers");
    fx.touch(Phase::Up, 6.0, 5.0, 0.2);
    assert!(!fx.rt.has(b, HOVERED));
    assert_eq!(fx.background("b").map(|c| c.0[3]), Some(0));
    // A moving mouse hovers: the producer does not exist yet.
    let hovers_from_a_mouse_move = false;
    assert!(hovers_from_a_mouse_move, "no mouse-move path sets hovered");
}

/// `_state_tests` 5802-5845 (`state.hover-capture-ok` … `-touch-never-holds`):
/// hover follows a mouse press through a mid-drag exit and settles on the
/// release; a pointer that turned touch never holds. Rust has no hover.
#[test]
#[ignore = "GP26: not ported: hover capture through a press"]
fn hover_follows_a_mouse_press_capture() {
    let mut fx = Fx::new("style Hold background=#00000000 { when hovered background=#FFFFFF22 }\nView v { Button h .Hold text=\"hold\", halign=left, valign=top }");
    let h = fx.id("h");
    fx.rt.set_state(h, HOVERED, true);
    fx.touch(Phase::Down, 5.0, 5.0, 0.0);
    fx.touch(Phase::Move, 300.0, 300.0, 0.1);
    assert!(fx.rt.has(h, HOVERED), "capture: the mid-drag exit keeps hover");
    fx.touch(Phase::Up, 300.0, 300.0, 0.2);
    assert!(!fx.rt.has(h, HOVERED), "released away: hover clears");
}

/// `_state_tests` 5852-5862 + 5892-5901 (`state.fx-ok`, `state.fx2-ok`,
/// PORT): a style carrying a bare `transition duration=` plus a
/// `transition any -> held anim=` builds (the anim's `self.` is the wearer).
#[test]
fn press_anim_transitions_build() {
    ok("style Press {\n    transition duration=0.08\n    when held background=#FFFFFFFF\n    transition any -> held anim=shrink\n}\nanim shrink { b.scale to=0.2, duration=0.1 }\nView v { Button b .Press text=\"press\" }");
    ok("style Press2 {\n    transition duration=0.3\n    when held background=#FFFFFFFF\n    transition any -> held anim=shrink2\n}\nanim shrink2 { b.scale to=0.2, duration=0.1 }\nView v { Button b .Press2 text=\"press\" }");
}

/// `_state_tests` 5863-5884 (`state.fx-player-tracked`, `-anim-landed`,
/// `-scale-reverts`): the press plays the rule's anim to scale 0.2 and the
/// release tweens the scale home even after the anim finished.
#[test]
#[ignore = "GP31: not ported: `transition … anim=` never plays"]
fn press_anim_plays_and_the_exit_reverts_the_scale() {
    let mut fx = Fx::new("style Press {\n    transition duration=0.08\n    when held background=#FFFFFFFF\n    transition any -> held anim=shrink\n}\nanim shrink { b.scale to=0.2, duration=0.1 }\nView v { Button b .Press text=\"press\", halign=left, valign=top }");
    let b = fx.id("b") as usize;
    fx.touch(Phase::Down, 5.0, 5.0, 0.0);
    fx.rt.tick(&mut fx.page, 0.2);
    fx.res();
    assert!((fx.rt.motion[b].scale[0] - 0.2).abs() < 1e-3, "{:?}", fx.rt.motion[b].scale);
    fx.touch(Phase::Up, 5.0, 5.0, 0.3);
    fx.ticks(20);
    assert!((fx.rt.motion[b].scale[0] - 1.0).abs() < 1e-3, "{:?}", fx.rt.motion[b].scale);
}

/// `_state_tests` 5902-5924 (`state.fx2-mid-revert`, `-true-base-adopted`,
/// `-player-owns-prop`, `-rests-at-true-base`): a re-press landing while
/// the exit is still tweening scale home takes the property over from the
/// true base (1.0), and the final release rests at 1.0.
#[test]
#[ignore = "GP31: not ported: `transition … anim=` never plays"]
fn repress_mid_revert_adopts_the_true_base() {
    let mut fx = Fx::new("style Press2 {\n    transition duration=0.3\n    when held background=#FFFFFFFF\n    transition any -> held anim=shrink2\n}\nanim shrink2 { b.scale to=0.2, duration=0.1 }\nView v { Button b .Press2 text=\"press\", halign=left, valign=top }");
    let b = fx.id("b") as usize;
    fx.touch(Phase::Down, 5.0, 5.0, 0.0);
    fx.rt.tick(&mut fx.page, 0.2);
    fx.res();
    fx.touch(Phase::Up, 5.0, 5.0, 0.3);
    fx.rt.tick(&mut fx.page, 0.15);
    fx.res();
    assert!((fx.rt.motion[b].scale[0] - 0.6).abs() < 1e-3, "halfway home: {:?}", fx.rt.motion[b].scale);
    fx.touch(Phase::Down, 5.0, 5.0, 0.5);
    fx.rt.tick(&mut fx.page, 0.3);
    fx.res();
    assert!((fx.rt.motion[b].scale[0] - 0.2).abs() < 1e-3, "the new play owns the property: {:?}", fx.rt.motion[b].scale);
    fx.touch(Phase::Up, 5.0, 5.0, 0.9);
    fx.ticks(40);
    assert!((fx.rt.motion[b].scale[0] - 1.0).abs() < 1e-3, "{:?}", fx.rt.motion[b].scale);
}

/// `_state_tests` 5926-5931 (`state.node-prop-beats-style-state`, PORT):
/// a node with its own `background=` wearing a style with a `when held
/// background=` builds.
#[test]
fn node_prop_beside_a_style_when_builds() {
    ok("style S background=#111111FF { when held background=#222222FF }\nView v { Button b .S background=#333333FF }");
}

/// `_state_tests` 5932-5935 (`state.style-state-dropped`): the spec's
/// merge rule — a node's own static prop drops the STYLE's when-response
/// for that key, so pressing keeps #333333.
#[test]
#[ignore = "GPAR48: static node prop should drop a style's when-response (Rust applies the style's #222222)"]
fn static_node_prop_drops_the_styles_when_response() {
    let mut fx = Fx::new("style S background=#111111FF { when held background=#222222FF }\nView v { Button b .S background=#333333FF }");
    let b = fx.id("b");
    fx.rt.set_state(b, HELD, true);
    assert_eq!(fx.background("b"), rgba(0x333333FF));
}

const THEMED: &str = "style Tap width=100, height=48 {\n    transition duration=0.1\n    when held background=#FFFFFFFF\n}\nView v { Column m {\n    Button plain .Tap text=\"a\"\n    Button based .Tap text=\"b\", background=#FF0000FF {\n        when held background=#00FF00FF\n    }\n} }";

/// `_state_tests` 5943-6007 (`state.theme-bg-ok`, `-held`, `-teardown`,
/// `state.bg-override-green`, `-back-red`, `state.theme-bg-quick-tap`,
/// PORT): a press response on a Button that never declared a background
/// ends white and, released, leaves NO box behind (the default look is
/// back — the ui/test.gogo bug was a transparent box parked over the
/// theme's gray); a node-scope response beats the style's (red → green →
/// red); a tap released before the first tick leaves no box either.
#[test]
fn undeclared_background_press_round_trip() {
    let mut fx = Fx::new(THEMED);
    let plain = fx.id("plain");
    let based = fx.id("based");
    fx.rt.set_state(plain, HELD, true);
    fx.ticks(21);
    assert_eq!(fx.background("plain"), rgba(0xFFFFFFFF));
    fx.rt.set_state(plain, HELD, false);
    fx.ticks(21);
    assert_eq!(fx.background("plain"), None, "no box corpse after the press");
    fx.rt.set_state(based, HELD, true);
    fx.ticks(21);
    assert_eq!(fx.background("based"), rgba(0x00FF00FF));
    fx.rt.set_state(based, HELD, false);
    fx.ticks(21);
    assert_eq!(fx.background("based"), rgba(0xFF0000FF));
    fx.rt.set_state(plain, HELD, true);
    fx.rt.set_state(plain, HELD, false);
    fx.ticks(21);
    assert_eq!(fx.background("plain"), None, "quick tap: no box corpse");
}

/// `_state_tests` 5953-5959 (`state.theme-bg-base`, `-from`): the press
/// tween of an undeclared background starts FROM the theme's resting look
/// (the user sees a fade from gray to white), not from nothing.
#[test]
#[ignore = "NEW: Godot tweens the press from the theme's resting gray vs Rust snaps to white (no tween starts for an undeclared background)"]
fn undeclared_background_press_tweens_from_the_resting_look() {
    let mut fx = Fx::new(THEMED);
    let plain = fx.id("plain");
    fx.rt.set_state(plain, HELD, true);
    fx.res();
    assert_eq!(fx.rt.tweens.len(), 1, "a 0.1 s tween from the default plate: {:?}", fx.rt.tweens);
    fx.rt.tick(&mut fx.page, 0.016);
    let mid = fx.background("plain").unwrap();
    assert!(mid.0[0] < 255, "one tick in, not white yet: {mid:?}");
}

/// `_state_tests` 6014-6027 (`state.theme-bg-snap-ok`, `-on`, `-off`,
/// PORT): with no transition rule the press applies at once and the
/// release removes the response at once.
#[test]
fn instant_press_response_without_a_transition() {
    let mut fx = Fx::new("style Snap width=100, height=48 { when held background=#00FFFFFF }\nView v { Button s .Snap text=\"c\" }");
    let s = fx.id("s");
    fx.rt.set_state(s, HELD, true);
    assert_eq!(fx.background("s"), rgba(0x00FFFFFF));
    fx.rt.set_state(s, HELD, false);
    assert_eq!(fx.background("s"), None);
}

/// `_state_tests` 6032-6062 (`state.padded-ok`, `-takeover`, `-handback`,
/// PORT): a padded Button through a press keeps its 10-epx padding while
/// green and after the hand-back, when the default look returns.
#[test]
fn padded_button_keeps_its_padding_through_a_press() {
    let mut fx = Fx::new("View v { Button p text=\"x\", padding=10 {\n    when held background=#00FF00FF\n} }");
    let p = fx.id("p");
    fx.rt.set_state(p, HELD, true);
    let res = fx.res();
    assert_eq!(res.placement[p as usize].padding, [10.0; 4]);
    assert_eq!(fx.background("p"), rgba(0x00FF00FF));
    fx.rt.set_state(p, HELD, false);
    let res = fx.res();
    assert_eq!(res.placement[p as usize].padding, [10.0; 4]);
    assert_eq!(fx.background("p"), None);
}

/// `_state_tests` 6067-6081 (`state.theme-font-ok`, `-on`, `-off`, PORT):
/// `when held font_color=` paints the text red while held and hands the
/// color back to the default when the press ends.
#[test]
fn font_color_response_hands_back_to_the_default() {
    let mut fx = Fx::new("style Ink width=100, height=48 { when held font_color=#FF0000FF }\nView v { Button ink .Ink text=\"d\" }");
    let ink = fx.id("ink");
    let text_color = |fx: &mut Fx| -> Option<Rgba> {
        fx.draws().iter().find_map(|d| match d {
            Draw::Text { color, .. } => Some(*color),
            _ => None,
        })
    };
    fx.rt.set_state(ink, HELD, true);
    assert_eq!(fx.res().color(&fx.page, ink, PropKey::FontColor), rgba(0xFF0000FF));
    assert_eq!(text_color(&mut fx), rgba(0xFF0000FF));
    fx.rt.set_state(ink, HELD, false);
    assert_eq!(fx.res().color(&fx.page, ink, PropKey::FontColor), None);
    assert_eq!(text_color(&mut fx), Some(DEFAULT_TEXT_COLOR));
}

/// `_state_tests` 6086-6122 (`state.disabled-universal-ok`, `-label-snap`,
/// `-label-restores`, `-label-reapplies`, `-slider-ok`, `-slider-editable`,
/// `-button-native`, `-when-any-node`, PORT): `disabled` is universal — a
/// Label wearing `when disabled opacity=0.4` loads dimmed, clears to 1 and
/// dims again; a disabled Slider takes no finger; a disabled Button carries
/// the flag; `when disabled` is legal on a Rectangle.
#[test]
fn disabled_is_universal() {
    let mut fx = Fx::new("style Dim { when disabled opacity=0.4 }\nView v { Label tag .Dim text=\"hi\", disabled=true }");
    let tag = fx.id("tag");
    assert!(fx.rt.has(tag, DISABLED));
    assert_eq!(fx.text_alpha("hi"), 102);
    fx.rt.set_state(tag, DISABLED, false);
    assert_eq!(fx.text_alpha("hi"), 255);
    fx.rt.set_state(tag, DISABLED, true);
    assert_eq!(fx.text_alpha("hi"), 102);

    let mut fx = Fx::new("View v { Slider vol disabled=true }");
    let vol = fx.id("vol");
    assert!(fx.rt.disabled(&fx.page, vol));
    let r = fx.rect("vol");
    fx.touch(Phase::Down, r.x + r.w / 2.0, r.y + r.h / 2.0, 0.0);
    assert!(!fx.input.is_pressed() && !fx.rt.has(vol, HELD), "a disabled Slider is not editable");

    let fx = Fx::new("View v { Button dis text=\"x\", disabled=true }");
    assert!(fx.rt.has(fx.id("dis"), DISABLED));

    ok("View v { Rectangle r color=#111111FF {\n    when disabled opacity=0.5\n} }");
}

/// `_state_tests` 6127-6129 + 6180-6182 (`state.err-availability`,
/// `state.transition-only-availability`): `when held` and a `-> held`
/// endpoint are refused on a Rectangle (held is for press surfaces).
#[test]
#[ignore = "GP41: not ported: `held` availability lint (a Rectangle's `when held` builds)"]
fn held_is_for_press_surfaces_only() {
    assert!(fails("View v { Rectangle r {\n    when held color=#FF0000FF\n} }"));
    assert!(fails("View v { Rectangle r color=#111111FF {\n    transition any -> held duration=0.1\n} }"));
}

/// `_state_tests` 6130-6131 + 6324-6325 (`state.err-style-signpost`,
/// `state.child-enum-rejected`): `style=Tool` is refused with the `.Tool`
/// signpost; an unknown child prop with a bare-word value is refused.
#[test]
fn unknown_props_are_errors() {
    assert!(fails("style Tool min_height=36\nView v { Button b style=Tool }"));
    assert!(fails("View v { Button b {\n    when held flash.z_as_relative=maybe\n    Rectangle flash color=#FFFFFFFF\n} }"));
}

/// `_state_tests` 6132 + 6135-6136 (`state.err-enter-retired`,
/// `state.err-bare-retired`, `state.err-bare-style-retired`, FLIP): the
/// retired `enter { }` block and the bare `pressed prop=` line do not
/// build. gogo-design.md §7: retired-spelling redirects are gone, an
/// unknown word is a build error (Rust names no migration hint here —
/// see the report's doubts).
#[test]
fn retired_block_heads_do_not_build() {
    assert!(codes("View v { Button b {\n    enter duration=0.1\n} }").contains(&"G7011".to_string()));
    assert!(codes("View v { Button b {\n    pressed background=#FF0000FF\n} }").contains(&"G7011".to_string()));
    assert!(codes("style S {\n    pressed background=#FF0000FF\n}\nView v { Button b .S }").contains(&"G0026".to_string()));
}

/// `_state_tests` 6133-6134 (`state.err-style-child`, PORT): a style block
/// holds `when`/`transition` lines, never a node.
#[test]
fn style_body_refuses_child_nodes() {
    assert!(codes("style S min_height=1 {\n    Button b\n}\nView v { Button c .S }").contains(&"G0026".to_string()));
}

/// `_state_tests` 6183-6184 (`state.transition-retired-endpoint`, PORT):
/// `transition any -> pressed` is refused with the respelling to `held`.
#[test]
fn retired_state_endpoint_is_respelled() {
    let c = compile("View v { Button b {\n    transition any -> pressed duration=0.1\n} }", &|_| None);
    assert!(c.diags.iter().any(|d| d.code == "G0035" && d.message.contains("held")), "{:#?}", c.diags);
}

const PULSE: &str = "anim nudge {\n    saveButton.scale to=0.9, duration=0.12, easing=cubic_out\n    then saveButton.scale to=1, duration=0.1, autoreverse=true\n}\nView v { Column c halign=center, valign=center {\n    Button saveButton text=\"Save\" {\n        transition any -> held anim=nudge\n    }\n} }";

/// `_state_tests` 6142-6167 (`state.transition-only-ok`, `-machine`,
/// `-subscribed`, `-flips`, `-releases`, PORT): a transition rule's
/// endpoint alone is enough (D182) — no standing `when held` needed: the
/// rule is baked for the node, a finger down sets held, a lift clears it.
/// GPAR58 DECIDED: held follows the finger — a slide-off clears it too.
#[test]
fn transition_endpoint_alone_wires_held() {
    let mut fx = Fx::new(PULSE);
    let sb = fx.id("saveButton");
    assert!(fx.page.transitions.iter().any(|t| t.node == sb && t.to == Some(State::Held) && t.from.is_none()), "{:?}", fx.page.transitions);
    let r = fx.rect("saveButton");
    fx.touch(Phase::Down, r.x + 5.0, r.y + 5.0, 0.0);
    assert!(fx.rt.has(sb, HELD));
    fx.touch(Phase::Up, r.x + 5.0, r.y + 5.0, 0.1);
    assert!(!fx.rt.has(sb, HELD));
    fx.touch(Phase::Down, r.x + 5.0, r.y + 5.0, 1.0);
    fx.touch(Phase::Move, 0.0, 0.0, 1.1);
    assert!(!fx.rt.has(sb, HELD), "GPAR58: slide-off clears held");
    fx.touch(Phase::Up, 0.0, 0.0, 1.2);
}

/// `_state_tests` 6159-6160 + 6168-6172 (`state.transition-only-anim-plays`,
/// `-reverts`): the enter plays `anim=nudge` (scale dips below 1) and the
/// exit reverts the displaced scale to 1.
#[test]
#[ignore = "GP31: not ported: `transition … anim=` never plays"]
fn transition_endpoint_alone_plays_its_anim() {
    let mut fx = Fx::new(PULSE);
    let sb = fx.id("saveButton") as usize;
    let r = fx.rect("saveButton");
    fx.touch(Phase::Down, r.x + 5.0, r.y + 5.0, 0.0);
    fx.rt.tick(&mut fx.page, 0.06);
    fx.res();
    assert!(fx.rt.motion[sb].scale[0] < 0.999, "the nudge plays: {:?}", fx.rt.motion[sb].scale);
    fx.rt.tick(&mut fx.page, 0.5);
    fx.touch(Phase::Up, r.x + 5.0, r.y + 5.0, 1.0);
    fx.ticks(20);
    assert!((fx.rt.motion[sb].scale[0] - 1.0).abs() < 1e-3, "{:?}", fx.rt.motion[sb].scale);
}

/// `_state_tests` 6207-6209 (`widget.instances-named`,
/// `widget.body-names-scoped`, PORT): two instances of a widget are found
/// by their instance names; a body node's name is per instance
/// (`one/caption`), never a page-wide `caption`. The style is hoisted out
/// of the widget — a widget-scoped style is GP32, pinned below.
#[test]
fn widget_instances_are_named_and_body_names_scoped() {
    let c = ok("style Root background=#1F1F1FFF {\n    when held background=#E51400FF\n}\nwidget FxButton $text=\"\" {\n    Button signals .Root {\n        Label caption text=$text\n    }\n}\nView v { Row r {\n    FxButton one text=\"Paint\"\n    FxButton two text=\"Undo\"\n} }");
    assert!(c.page.find("one").is_some() && c.page.find("two").is_some());
    assert!(c.page.find("caption").is_none(), "body names are scoped");
    assert!(c.page.find("one/caption").is_some() && c.page.find("two/caption").is_some());
}

/// `_state_tests` 6187-6217 (`widget.fx-ok`, `-fx-states`,
/// `-fx-transition-anim`, `-fx-scoped-anim-baked`, `-fx-scoped-named`): a
/// widget's OWN `style` (with a `transition … anim=` naming the widget's
/// own anim) builds and lands per instance on the `signals` Button.
#[test]
#[ignore = "GP32: not ported: widget-scoped `style`/`transition` (unknown style `.Root`, G5002)"]
fn widget_scoped_style_and_transition_anim_land_per_instance() {
    let c = ok("widget FxButton $text=\"\" {\n    anim pressFx {\n        caption.translate.x { key t=0, v=0; key t=0.1, v=3; key t=0.2, v=0 }\n    }\n    style Root background=#1F1F1FFF {\n        transition any -> held duration=0.05, anim=pressFx\n        when held background=#E51400FF\n    }\n    Button signals .Root {\n        Label caption text=$text\n    }\n}\nView v { Row r {\n    FxButton one text=\"Paint\"\n    FxButton two text=\"Undo\"\n} }");
    let one = c.page.find("one").unwrap();
    let node = c.page.nodes[one as usize];
    assert!(node.whens.1 >= 1, "the style's when landed on the instance root");
    let rule = c.page.transitions.iter().find(|t| t.node == one && t.to == Some(State::Held)).copied();
    assert!(matches!(rule, Some(t) if c.page.str(t.anim).ends_with("pressFx")), "{:?}", c.page.transitions);
    assert!(c.page.anims.iter().any(|a| c.page.str(a.anim).ends_with("pressFx") && a.node == c.page.find("one/caption").unwrap()), "the scoped anim targets one's caption");
}

/// `_state_tests` 6222-6242 (`state.child-ok`, `-applied`, `-restored`,
/// PORT): a style's `when held flash.visibility=…, flash.opacity=…` drives
/// a named child of the wearer while held and restores it on release.
#[test]
fn held_response_drives_a_named_child() {
    let mut fx = Fx::new("style FlashLook background=#00000000 {\n    when held flash.visibility=visible, flash.opacity=0.5\n}\nView v { Panel p {\n    Button b .FlashLook text=\"press\" {\n        Rectangle flash color=#FFFFFFFF, visibility=gone\n    }\n} }");
    let b = fx.id("b");
    let flash = fx.id("flash");
    fx.rt.set_state(b, HELD, true);
    let res = fx.res();
    assert!(!res.gone(&fx.page, flash), "visible while held");
    assert_eq!(res.num(&fx.page, flash, PropKey::Opacity), Some(0.5));
    fx.rt.set_state(b, HELD, false);
    let res = fx.res();
    assert!(res.gone(&fx.page, flash), "gone again");
    assert_eq!(res.num(&fx.page, flash, PropKey::Opacity), None);
}

/// `_state_tests` 6245-6267 (`state.child-widget-ok`, `-isolated`,
/// `-restored`, PORT): each widget instance's held state flips ITS OWN
/// child — pressing `one` shows `one/glow` and leaves `two/glow` gone.
/// The style is hoisted out of the widget (a widget-scoped style is GP32).
#[test]
fn each_widget_instance_flips_its_own_child() {
    let mut fx = Fx::new("style L background=#00000000 { when held glow.visibility=visible }\nwidget FlashBtn $t=\"\" {\n    Button signals .L text=$t {\n        Rectangle glow color=#FFFFFF66, visibility=gone\n    }\n}\nView v { Row r {\n    FlashBtn one t=\"a\"\n    FlashBtn two t=\"b\"\n} }");
    let one = fx.id("one");
    let glow1 = fx.id("one/glow");
    let glow2 = fx.id("two/glow");
    fx.rt.set_state(one, HELD, true);
    let res = fx.res();
    assert!(!res.gone(&fx.page, glow1) && res.gone(&fx.page, glow2), "one={} two={}", res.gone(&fx.page, glow1), res.gone(&fx.page, glow2));
    fx.rt.set_state(one, HELD, false);
    assert!(fx.res().gone(&fx.page, glow1));
}

/// `_state_tests` 6271-6277 (`state.child-unknown-builds`,
/// `state.child-unknown-dropped`, FLIP): Godot builds and drops the prop
/// at `_ready`; Rust refuses at build with G0060 — gogo-design.md §7
/// "names become code … typos are rustc errors".
#[test]
fn unknown_when_target_is_a_build_error() {
    assert!(codes("View v { Button b {\n    when held nosuch.visibility=visible\n} }").contains(&"G0060".to_string()));
}

/// `_state_tests` 6284-6296 (`state.sweep-build`, PORT): the selected
/// style builds, and a selected write through the runtime restyles in
/// the same frame (Godot: "the write paths restyle same-frame"; its raw
/// engine-write sweep is ENGINE).
#[test]
fn selected_write_restyles_at_once() {
    let mut fx = Fx::new("style Sw background=#00000000 {\n    when selected background=#FFFFFF22\n}\nView v { Button b .Sw toggle=true }");
    let b = fx.id("b");
    fx.rt.set_state(b, SELECTED, true);
    assert_eq!(fx.background("b"), rgba(0xFFFFFF22));
}

/// `_state_tests` 6328-6342 (`prop.empty-resource-ok`, `prop.polish-ok`,
/// `prop.opacity`, `prop.font_size`, `prop.width`, PORT): an empty `$icon`
/// source builds (no load attempt); `opacity=0.4` dims the text to 102,
/// `font_size=11` reaches the draw list, `width=60` is the box's width.
#[test]
fn empty_source_and_polish_props() {
    ok("widget I $icon=\"\" { Image img source=$icon }\nView v { I inst }");
    let mut fx = Fx::new("View v { Button b text=\"t\", opacity=0.4, font_size=11, width=60, icon_size=30 }");
    assert_eq!(fx.text_alpha("t"), 102);
    let list = fx.draws();
    assert!(list.iter().any(|d| matches!(d, Draw::Text { font_size, .. } if *font_size == 11.0)), "{list:#?}");
    assert_eq!(fx.rect("b").w, 60.0);
}

/// `_state_tests` 6341 (`prop.icon_size`): `icon_size=30` is a known
/// Button prop (the icon's max width).
#[test]
fn icon_size_is_a_known_button_prop() {
    assert!(!codes("View v { Button b text=\"t\", icon_size=30 }").contains(&"G2001".to_string()));
}

// ------------------------------------------------------- _touch_lint_tests

/// `_touch_lint_tests` 6393-6511 (13 checks: `lint.small-warns`,
/// `-small-diag`, `-area-diag`, `-off-silences`, `-exact-passes`,
/// `-fill-axis-skipped`, `-slider-height-warns`, `-cross-axis-fill-quiet`,
/// `-declared-narrow-warns`, `-non-interactive-quiet`, `-err-bad-value`,
/// `-widget-exact-size-quiet`, `-content-button-quiet`): an interactive
/// control under 44 epx on a non-stretched axis warns "touch target";
/// `touch_target=off` silences; a bad value is an error; the Grid `areas=`
/// lint shares the channel.
#[test]
#[ignore = "GP41: not ported: the 44-epx touch-target lint (`touch_target=tiny` builds, `Button b 30x30` is silent)"]
fn touch_target_lint() {
    let touch = |src: &str| -> Vec<String> { warnings(src).into_iter().filter(|w| w.contains("touch target")).collect() };
    assert_eq!(touch("View v { Button b 30x30 }").len(), 1);
    assert!(warnings("View v { Grid g cols=[100, 100], areas=[\"a b: 40\"] {\n    Rectangle a color=red\n} }").iter().any(|w| w.contains("area `b`")));
    assert!(touch("View v { Button b 30x30, touch_target=off }").is_empty());
    assert!(touch("View v { Button b 44x44 }").is_empty());
    assert!(touch("View v { Row r { Button b width=fill, min_height=44 } }").is_empty());
    let slider = touch("View v { Column c { Slider s min_width=220 } }");
    assert!(slider.len() == 1 && slider[0].contains("height"), "{slider:?}");
    assert!(touch("View v { Column c { Slider s min=0, max=100, value=24, render=native, height=50 } }").is_empty());
    let narrow = touch("View v { Column c { Slider s width=20, height=50 } }");
    assert!(narrow.len() == 1 && narrow[0].contains("width"), "{narrow:?}");
    assert!(touch("View v { Rectangle r 10x10 }").is_empty());
    assert!(fails("View v { Button b touch_target=tiny }"));
    assert!(touch(APPBAR).is_empty());
    assert!(touch(APPLINK).is_empty());
}

const APPBAR: &str = "style TileLook background=#00000000 {\n    when held flash.visibility=visible\n}\nwidget Tile $label=\"\" {\n    Button signals .TileLook 50x90, keyboard_target=off {\n        Column spacing=0 {\n            gap 19\n        }\n        Label text=$label\n        Gradient flash fill=radial, stops=[#FFFFFF6B@0, #FFFFFF6B@0.94, #FFFFFF00@1],\n            visibility=gone, 35x35, halign=center, valign=top\n    }\n}\nView v { Row bar {\n    Tile one label=\"brush\"\n    Tile two label=\"undo\"\n} }";

const APPLINK: &str = "style LinkLook background=#00000000 {\n    when held background=#FFFFFF3C\n}\nwidget LinkRow $label=\"\" {\n    Button signals .LinkLook keyboard_target=off {\n        Panel padding=6 {\n            Row spacing=6 {\n                Rectangle color=#FFFFFFFF, 32x32\n                Label text=$label, font_size=12, vertical_alignment=center\n            }\n        }\n    }\n}\nView v { Column links {\n    LinkRow email label=\"send email to Sunny\"\n    LinkRow rate  label=\"rate and review\"\n} }";

/// `_touch_lint_tests` 6512-6516 (`lint.content-button-measured`, PORT):
/// a content-sized widget-root Button (a padded Panel holding a 32-epx
/// square and a label) measures its real parts — at least 44 × 44.
#[test]
fn content_sized_button_measures_its_parts() {
    let mut fx = Fx::new(APPLINK);
    let r = fx.rect("email");
    assert!(r.h >= 44.0 - 0.01 && r.w >= 44.0, "{r:?}");
}

// ---------------------------------------------- _exact_overflow_lint_tests

/// `_exact_overflow_lint_tests` 6531-6543, 6572-6581, 6596-6602
/// (`lint.exact-overflow-warns`, `-width`, `-native`, FLIP): Godot warns
/// "height=100 but content needs 144" because its box silently grows to
/// the content. GPAR37/GPAR40 DECIDED: a written size is hard — the box
/// keeps the number and CLIPS what pokes past it, so there is nothing to
/// warn about. Pins the held number and the overflowing content.
#[test]
fn written_size_holds_and_clips_instead_of_warning() {
    let mut fx = Fx::new("View v { Column menu halign=center, valign=center, spacing=12, height=100 {\n    Rectangle a 80x40, color=red\n    Rectangle b 80x40, color=red\n    Rectangle c 80x40, color=red\n} }");
    let menu = fx.rect("menu");
    let c = fx.rect("c");
    assert_eq!(menu.h, 100.0, "{menu:?}");
    assert!(c.y + c.h > menu.y + menu.h, "content 144 pokes past the 100 box: {c:?} in {menu:?}");
    let list = fx.draws();
    assert!(list.iter().any(|d| matches!(d, Draw::ClipPush(r) if r.h == 100.0)), "the written height clips: {list:#?}");

    let mut fx = Fx::new("View v { Row r spacing=0, width=50 {\n    Rectangle a 40x20, color=red\n    Rectangle b 40x20, color=red\n} }");
    let r = fx.rect("r");
    let b = fx.rect("b");
    assert_eq!(r.w, 50.0, "{r:?}");
    assert!(b.x + b.w > r.x + r.w, "content 80 pokes past the 50 box: {b:?} in {r:?}");

    let mut fx = Fx::new("View v { Button b text=\"a very very long caption\", width=20, height=44, touch_target=off }");
    let b = fx.rect("b");
    assert_eq!((b.w, b.h), (20.0, 44.0), "{b:?}");
    // The caption's rect stays inside the 20-wide box (the renderer cuts
    // the glyphs — GPAR37: "a capped Label always cuts its text").
    let list = fx.draws();
    let text = list.iter().find_map(|d| if let Draw::Text { rect, .. } = d { Some(*rect) } else { None }).unwrap();
    assert!(text.x >= b.x && text.x + text.w <= b.x + b.w, "{text:?} in {b:?}");
}

/// `_exact_overflow_lint_tests` 6545-6570, 6583-6594
/// (`lint.exact-overflow-fits-quiet`, `-scroll-quiet`, `-min-quiet`, PORT):
/// content that fits, a sized Scroll (its axis claims no content) and a
/// `min_height` (max(min, content) is its meaning) all build without a
/// warning; the min box grows to 144.
#[test]
fn stretchy_and_scrolled_sizes_stay_quiet() {
    let fits = "View v { Column menu spacing=12, height=200 {\n    Rectangle a 80x40, color=red\n    Rectangle b 80x40, color=red\n    Rectangle c 80x40, color=red\n} }";
    assert!(warnings(fits).is_empty(), "{:?}", warnings(fits));
    assert_eq!(Fx::new(fits).rect("menu").h, 200.0);
    let scrolled = "View v { Scroll s height=100 {\n    Column c spacing=12 {\n        Rectangle a 80x40, color=red\n        Rectangle b 80x40, color=red\n        Rectangle d 80x40, color=red\n    }\n} }";
    assert!(warnings(scrolled).is_empty(), "{:?}", warnings(scrolled));
    assert_eq!(Fx::new(scrolled).rect("s").h, 100.0);
    // Godot measured the detached root's minimum; here the Column is a
    // child of the View, so `valign=top` keeps its height unwritten (hug).
    let minh = "View v { Column menu spacing=12, min_height=100, valign=top {\n    Rectangle a 80x40, color=red\n    Rectangle b 80x40, color=red\n    Rectangle c 80x40, color=red\n} }";
    assert!(warnings(minh).is_empty(), "{:?}", warnings(minh));
    assert_eq!(Fx::new(minh).rect("menu").h, 144.0, "min grows to its content");
}

// ------------------------------------------------------ _explain_api_tests

/// `_explain_api_tests` 6606-6609 + 6622-6636 (`explain.build`,
/// `explain.declares`, `explain.cap-ok`, `-cap-fill`, `-cap-number`,
/// PORT): the facts the readout prints — a `halign=right, valign=bottom,
/// margin=16` fab sits in the bottom-right corner 16 in; `max_width=fill`
/// and `max_width=90` land as the placement's cap.
#[test]
fn explain_facts_placement_and_caps() {
    let mut fx = Fx::new("View v { Layers l { Button fab 50x50, halign=right, valign=bottom, margin=16 } }");
    let fab = fx.id("fab") as usize;
    let (res, rects) = fx.lay();
    let p = res.placement[fab];
    assert_eq!((p.halign, p.valign), (Align::End, Align::End), "{p:?}");
    // `margin=16` on a right/bottom-aligned exact box pins the near edges;
    // the far margins are dropped (compile.rs, the G1006 over-constraint rule).
    assert_eq!((p.margin[2], p.margin[3]), (Some(16.0), Some(16.0)), "{p:?}");
    assert_eq!(rects[fab], Rect::new(334.0, 334.0, 50.0, 50.0), "{:?}", rects[fab]);

    let mut fx = Fx::new("View v { Layers l {\n    Panel plate width=hug, max_width=fill, halign=center, valign=top,\n            margin_left=12, margin_right=12 { Label t text=\"x\" }\n    Panel numbox width=hug, max_width=90, valign=top { Label u text=\"y\" }\n} }");
    let plate = fx.id("plate") as usize;
    let numbox = fx.id("numbox") as usize;
    let res = fx.res();
    assert!(res.placement[plate].max_width_fill, "{:?}", res.placement[plate]);
    assert_eq!(res.placement[numbox].max_width, Some(90.0), "{:?}", res.placement[numbox]);
}

/// `_explain_api_tests` 6640-6656 (`explain.d133-ok`, `explain.aspect`,
/// `explain.disabled-inherited`, `explain.disabled-own`, PORT): `aspect=1.78`
/// is read as declared; a Button under a `disabled=true` Panel is disabled
/// by inheritance (its own flag off), the Panel by its own flag.
/// `space_only` is written `hidden` here (GP39, pinned below).
#[test]
fn explain_facts_aspect_and_disabled_cascade() {
    let mut fx = Fx::new(D133);
    let ratio = fx.id("ratio");
    let dbtn = fx.id("dbtn");
    let dgrp = fx.id("dgrp");
    let res = fx.res();
    assert_eq!(res.num(&fx.page, ratio, PropKey::Aspect), Some(1.78));
    assert!(fx.rt.disabled(&fx.page, dbtn) && !fx.rt.has(dbtn, DISABLED), "inherited from dgrp");
    assert!(fx.rt.has(dgrp, DISABLED), "own");
}

const D133: &str = "View v { Layers l {\n    Panel seat visibility=space_only, width=hug, valign=top { Label s text=\"x\" }\n    Rectangle ratio color=#FFFFFFFF, width=fill, aspect=1.78, valign=top\n    Panel dgrp disabled=true, valign=bottom { Column { Button dbtn text=\"y\" } }\n} }";

/// Not a Godot check (found while porting `explain.aspect`): a fill-width
/// Rectangle with `aspect=1.78` under a free parent should be 400 wide and
/// 400/1.78 tall — Rust lays it out 0 tall (`image_min` has no offered
/// width on a free axis).
/// SHIPPED round 4 (2026-08-31, GPAR80): a numeric `aspect=` derives the
/// unspelled axis on any box.
#[test]
fn aspect_shapes_a_fill_width_box() {
    let mut fx = Fx::new(D133);
    let r = fx.rect("ratio");
    assert!(r.h > 0.0 && (r.w / r.h - 1.78).abs() < 0.01, "{r:?}");
}

/// `_explain_api_tests` 6647-6648 (`explain.visibility`):
/// `visibility=space_only` — invisible, hit-inert, still holds its space.
#[test]
fn space_only_holds_its_space_invisibly() {
    let mut fx = Fx::new("View v { Layers l { Panel seat visibility=space_only, width=hug, valign=top { Label s text=\"x\" } } }");
    let seat = fx.rect("seat");
    assert!(seat.w > 0.0 && seat.h > 0.0, "holds its space: {seat:?}");
    let list = fx.draws();
    assert!(!list.iter().any(|d| matches!(d, Draw::Text { text, .. } if fx.page.str(*text) == "x")), "invisible: {list:#?}");
}

/// `_explain_api_tests` 6661-6680 (`explain.prov-ok`,
/// `explain.passive-default`, `explain.interactive-true`,
/// `explain.interactive-false`, PORT): the input provenance the readout
/// names — a plain Rectangle is transparent to input, `interactive=true`
/// makes a Rectangle a catcher, `interactive=false` makes a Button
/// hit-invisible. The three are spread to corners so each is hit alone.
#[test]
fn explain_facts_input_provenance() {
    let fx = Fx::new("View v { Layers l {\n    Rectangle plain 8x8, halign=left, valign=top\n    Rectangle catcher interactive=true, touch_target=off, 60x60, halign=right, valign=top\n    Button ghost text=\"x\", interactive=false, touch_target=off, halign=right, valign=bottom\n} }");
    let catcher = fx.id("catcher");
    let res = resolve(&fx.page, &fx.rt);
    let rects = layout(&fx.page, &res, FRAME, &FixedAdvance);
    assert_eq!(hit(&fx.page, &res, &rects, 4.0, 4.0, false), None, "passive default: transparent");
    assert_eq!(hit(&fx.page, &res, &rects, 396.0, 4.0, false), Some(catcher), "interactive=true claims input");
    assert_eq!(hit(&fx.page, &res, &rects, 396.0, 396.0, false), None, "interactive=false: hit-invisible");
}
