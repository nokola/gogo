//! Port of three Godot suite groups (FantasiaGodot/addons/gogo/test/run_tests.gd):
//! `_arrangement_tests` (lines 6682-6972: 55 `check` + 10 `close`, the
//! state_view.gogo and pose_timing.gogo fixtures), `_screen_tests`
//! (6972-7154: 38 checks, screen_view.gogo) and `_orientation_tests`
//! (7154-7282: 23 checks, orient_view.gogo). Fixtures are inlined below
//! (the Rust dialect needed no word changes; one wrapped node line was
//! joined). Godot snippets whose root was a bare `Rectangle bar` are
//! wrapped in `View v { … }` (a Rust file has a View/Panel root, G7003).
//!
//! One translation the dialect table does not list: Godot's ONE-LINE
//! top-level `when collapsed bar.margin_bottom=-40, scrim.opacity=0.5`
//! builds in Rust but poses nothing (the compiler reads only a when's
//! block children — pinned NEW in `one_line_top_level_when_poses`). The
//! runtime tests below spell those whens as `when collapsed { … }` blocks
//! so they certify the pose semantics rather than that parse gap.
//!
//! Godot's `set_screen_class("landscapeSmall")` is `set_screen(Landscape,
//! LandscapeSmall)` here: the Rust runtime names BOTH axes at once from
//! enums (screen ruler decision, todo-gogo.md Decisions 2026-08-30), so a
//! fixture that poses only one axis is driven with the other axis set to a
//! word the file never mentions.
//!
//! ENGINE (not ported):
//! - `arr.reload-ok`, `arr.reload-keeps-pose`, `arr.reload-reseeds-style-when`
//!   (6856-6865): hot reload re-tracks the scene tree; here the shell swaps pages.
use gogo_ui::input::{Input, Phase, Touch};
use gogo_ui::KeptFrame;
use gogo_ui::layout::{FixedAdvance, Rect, layout};
use gogo_ui::page::{Len, NO_NODE, NodeId, PropKey};
use gogo_ui::runtime::{BASE, HELD, Resolved, Runtime, SELECTED, resolve, resolve_animated};
use gogo_ui::{Compiled, Diagnostic, Orientation, Page, ScreenClass, Severity, compile};

const W: f32 = 400.0;
const H: f32 = 800.0;

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn diags(src: &str) -> Vec<Diagnostic> {
    compile(src, &|_| None).diags
}

fn codes(src: &str) -> Vec<String> {
    diags(src).iter().map(|d| d.code.clone()).collect()
}

fn refused(src: &str) -> bool {
    gogo_ui::has_errors(&diags(src))
}

fn has_code(src: &str, code: &str) -> bool {
    codes(src).iter().any(|c| c == code)
}

fn error_text(src: &str) -> String {
    diags(src).iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect::<Vec<_>>().join("\n")
}

fn lay(page: &Page, rt: &Runtime) -> KeptFrame {
    KeptFrame::fresh(page, resolve(page, rt), Rect::new(0.0, 0.0, W, H), &FixedAdvance)
}

/// The shown opacity of a node: the `when`/tween override, else 1.
fn opacity(page: &Page, res: &Resolved, node: NodeId) -> f32 {
    res.num(page, node, PropKey::Opacity).unwrap_or(1.0)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// One frame the way the shell runs it: advance, resolve with tweens,
/// lay out, glide. Returns the SHOWN rects.
fn frame(page: &mut Page, rt: &mut Runtime, dt: f32) -> KeptFrame {
    rt.tick(page, dt);
    let mut f = KeptFrame::fresh(page, resolve_animated(page, rt), Rect::new(0.0, 0.0, W, H), &FixedAdvance);
    f.glide(rt);
    f
}

/// A tap (down + up at the node's center) through the real input path.
fn tap(page: &Page, rt: &mut Runtime, input: &mut Input, node: NodeId, t: f64) {
    let f = lay(page, rt);
    let rects = f.rects();
    let r = rects[node.index()];
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    input.touch(page, rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Down, x, y, t, id: 1 });
    let f = lay(page, rt);
    input.touch(page, rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Up, x, y, t: t + 0.1, id: 1 });
}

fn set_class(page: &Page, rt: &mut Runtime, class: ScreenClass) {
    let orientation = match class {
        ScreenClass::LandscapeSmall | ScreenClass::LotsOfSpace => Orientation::Landscape,
        _ => Orientation::Portrait,
    };
    rt.set_screen(page, orientation, class);
}

// ---------------------------------------------------------------- fixtures

/// FantasiaGodot/addons/gogo/test/fixtures/state_view.gogo; its two
/// one-line whens written as blocks (see the module doc).
const STATE_VIEW: &str = "\
arrangement collapsed
arrangement wide

transition duration=0.2, easing=cubic_out

when collapsed {
    bar.margin_bottom = -40
    bar.height = 90
    scrim.opacity = 0.5
}
when wide {
    bar.height = 120
}

style Dot min_width=6, min_height=6 {
    when collapsed opacity=0.25
}

View screen {
    Rectangle scrim color=#000000FF
    Rectangle bar color=#FFFFFFFF, width=fill, valign=bottom, height=60
    Rectangle dot .Dot color=#FFFFFFFF, halign=left, valign=top
    Button goBtn text=\"go\", 50x50, halign=right, valign=top, on:activate=go(collapsed)
    Button toggleBtn text=\"tg\", 50x50, halign=right, valign=bottom, on:activate=toggle(collapsed)
}
";

/// fixtures/pose_timing.gogo, verbatim.
const POSE_TIMING: &str = "\
arrangement open

transition base -> open duration=0.5, easing=cubic_out
transition open -> base duration=0.5, easing=cubic_out

when open {
    slab.margin_top = 100
    plate.margin_top = 100
    tag.margin_top = 100
}

View screen {
    Rectangle slab 40x40, color=#FFFFFFFF, halign=left, valign=top
    Rectangle plate 40x40, color=#FFFFFFFF, halign=left, valign=top {
        transition base -> open duration=1.0, easing=cubic_out
        transition open -> base duration=1.0, easing=cubic_out
    }
    Rectangle tag 40x40, color=#FFFFFFFF, halign=left, valign=top {
        transition base -> open duration=0
        transition open -> base duration=0
    }
}
";

/// fixtures/screen_view.gogo; the wrapped `pane` line joined into one and
/// the one-line `when open` written as a block (see the module doc).
const SCREEN_VIEW: &str = "\
arrangement open

when screen.landscapeSmall {
    bar.margin_bottom = -10
    bar.height = 80
    tag.opacity = 0.25
    pane.halign = left
    pane.width = 80
    pane.margin_left = 20
}

when open {
    bar.margin_bottom = -40
    scrim.opacity = 0.5
}

when open screen.landscapeSmall {
    pane.margin_left = 40
    chip.opacity = 0.5
}

View screen {
    Rectangle scrim color=#000000FF
    Rectangle bar color=#FFFFFFFF, width=fill, valign=bottom, height=60
    Rectangle tag color=#FFFFFFFF, 10x10, halign=left, valign=top
    Rectangle chip color=#FFFFFFFF, 10x10, halign=right, valign=top
    Rectangle pane color=#FFFFFFFF, margin_left=10, margin_right=10, valign=top, height=30
}
";

/// fixtures/orient_view.gogo, verbatim.
const ORIENT_VIEW: &str = "\
arrangement open

when screen.landscape {
    bar.height = 30
    tag.opacity = 0.25
    mover.seat = sideSeat
}
when screen.landscape screen.lotsOfSpace {
    bar.height = 44
    chip.opacity = 0.5
}
when open screen.landscape {
    bar.height = 52
}

View orientView {
    Column home halign=left, valign=top {
        Row strip width=200, height=60, spacing=0 {
            seat sideSeat
        }
        Column moverHome width=200 {
            Panel mover width=40, height=20
        }
    }
    Rectangle bar width=100, height=20, halign=left, valign=top, margin_top=100, color=#FFFFFFFF
    Rectangle tag width=20, height=20, halign=right, valign=top, color=#FFFFFFFF
    Rectangle chip width=20, height=20, halign=right, valign=top, margin_top=30, color=#FFFFFFFF
}
";

// ------------------------------------------------- _arrangement_tests: build

/// `arr.err-when-line-timing` (6684): `duration=` on a `when` line is
/// refused and pointed at `transition`. Rust only warns (G2001 unknown
/// property) — the file builds with the timing silently dropped.
#[test]
fn when_line_timing_is_refused_and_points_to_transition() {
    let src = "style S {\n    when selected background=#E51400FF, duration=0.1\n}\nView v { Button b .S toggle=true }";
    assert!(refused(src), "{:#?}", diags(src));
    assert!(error_text(src).contains("transition"), "{}", error_text(src));
}

/// `arr.err-enter-in-when` (6687): an `enter { }` block inside a `when`
/// is refused (timing was folded into `transition` rules).
#[test]
fn enter_block_inside_a_when_is_refused() {
    let src = "style S {\n    when selected background=#FF0000FF {\n        enter duration=0.2\n    }\n}\nView v { Button b .S toggle=true }";
    assert!(has_code(src, "G0054"), "{:#?}", diags(src));
}

/// `arr.err-retired-state` (6745): the retired top-level `state` directive
/// does not build. Godot names the rename; Rust has no alias table
/// (gogo-design.md §7: retired-spelling redirects removed) and refuses
/// the word as an unknown root.
#[test]
fn retired_state_directive_does_not_build() {
    let src = "state s\nView v { Rectangle bar }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `arr.err-unknown-target` 6747, `arr.err-reserved-name` 6753,
/// `arr.err-not-dotted` 6757, `arr.err-duplicate` 6761, `arr.err-when-base`
/// 6766, `arr.err-transition-unknown` 6775, `arr.err-go-unknown` 6789,
/// `arr.err-toggle-base` 6792, `arr.err-toggle-unknown` 6794: the
/// arrangement declarations, `when` targets and `go`/`toggle` arguments a
/// file may not write. (`arrangement base` is refused by the parser as a
/// nameless line, G0031, before the compiler's G0032 reserved-name check
/// can see it; Godot says "reserved" — both refuse.)
#[test]
fn arrangement_build_errors_match() {
    assert!(has_code("arrangement s\nwhen s { foo.width=2 }\nView v { Rectangle bar }", "G0060"), "unknown when target: {:?}", codes("arrangement s\nwhen s { foo.width=2 }\nView v { Rectangle bar }"));
    assert!(refused("arrangement base\nView v { Rectangle bar }"), "base is reserved: {:?}", codes("arrangement base\nView v { Rectangle bar }"));
    assert!(has_code("arrangement s\nwhen s { width=2 }\nView v { Rectangle bar }", "G0059"), "a top-level when writes node.prop: {:?}", codes("arrangement s\nwhen s { width=2 }\nView v { Rectangle bar }"));
    assert!(has_code("arrangement s\narrangement s\nView v { Rectangle bar }", "G0033"), "duplicate arrangement");
    assert!(refused("when base { bar.opacity=1 }\nView v { Rectangle bar }"), "base is the declarations themselves, not a when");
    assert!(has_code("transition base -> nope duration=0.1\nView v { Rectangle bar }", "G0036"), "unknown transition endpoint");
    assert!(has_code("View v { Button b 50x50, on:activate=go(nope) }", "G0051"), "go(unknown)");
    assert!(has_code("arrangement a\nView v { Button b 50x50, on:activate=toggle(base) }", "G0050"), "toggle(base) toggles nothing");
    assert!(has_code("arrangement a\nView v { Button b 50x50, on:activate=toggle(nope) }", "G0051"), "toggle(unknown)");
}

/// Not a Godot check — found translating state_view.gogo:9 and
/// screen_view.gogo:9: the one-line top-level `when s bar.width=2` (the
/// form every Godot fixture and half its snippets use) builds in Rust and
/// poses NOTHING — the compiler reads a top-level when's block children
/// only, so the line's props vanish, and with them the target checks
/// (`when s foo.width=2` raises no G0060 either).
#[test]
fn one_line_top_level_when_poses() {
    let c = ok("arrangement s\nwhen s bar.width=2\nView v { Rectangle bar }");
    let bar = c.page.find("bar").unwrap();
    assert!(c.page.nodes[bar.index()].whens.1 > 0, "the when line reached the node: {:?}", c.page.nodes[bar.index()]);
    assert!(has_code("arrangement s\nwhen s foo.width=2\nView v { Rectangle bar }", "G0060"));
}

/// `arr.err-when-unknown` (6768): a misspelled arrangement in a `when`
/// gets a hint naming the nearest declared one (`settings`). Rust
/// refuses (G0036) but offers no suggestion, although gogo-design.md §7
/// promises "build error with a suggestion".
#[test]
fn misspelled_arrangement_hint_names_the_nearest() {
    let src = "arrangement settings\nwhen setings bar.opacity=1\nView v { Rectangle bar }";
    assert!(refused(src));
    assert!(error_text(src).contains("settings"), "{}", error_text(src));
}

/// `arr.err-builtin-name` 6749, `arr.err-retired-name` 6751: an
/// arrangement may not take a built-in state's name (`held`) or a retired
/// state spelling (`pressed`). Rust builds both.
#[test]
fn arrangement_may_not_shadow_a_state_word() {
    assert!(refused("arrangement held\nView v { Rectangle bar }"), "a built-in state's name");
    assert!(refused("arrangement pressed\nView v { Rectangle bar }"), "a retired spelling");
}

/// `arr.err-decl-with-props` (6755): `arrangement s bar.width=2` is
/// refused — an arrangement line declares a name, nothing else. Rust
/// builds it and drops the prop without a word.
#[test]
fn arrangement_line_declares_a_name_only() {
    let src = "arrangement s bar.width=2\nView v { Rectangle bar }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `arr.err-margin-shorthand` 6759, `screen.err-margin-word` 6980: a pose
/// takes per-edge margin pins, never the `margin=` shorthand. Rust
/// accepts `bar.margin=4` in a `when` and sets all four edges.
#[test]
fn posed_margin_shorthand_is_refused() {
    assert!(refused("arrangement s\nwhen s { bar.margin=4 }\nView v { Layers l { Rectangle bar halign=left } }"), "arrangement pose");
    assert!(refused("when screen.landscapeSmall { bar.margin=4 }\nView v { Layers l { Rectangle bar halign=left } }"), "screen pose");
}

/// `arr.err-center-margin` (6763): Godot refuses a posed `margin_left`
/// on a centered box ("pins no edge"). FLIP (GPAR94, round 6, recorded):
/// in Rust a margin on a centered or aligned axis is LOAD-BEARING — it
/// feeds a hug parent's claim (D241/D242) and a centered pair shifts the
/// center (GPAR38) — so the pose builds and means what it says.
#[test]
fn posed_margin_on_a_centered_axis_builds_here() {
    let src = "arrangement s\nwhen s { bar.margin_left=4 }\nView v { Layers l { Rectangle bar halign=center, width=20 } }";
    assert!(!refused(src), "{:#?}", diags(src));
}

/// `arr.err-when-empty` (6770): `when s { }` holds no responses and is
/// refused. Rust builds it.
#[test]
fn empty_when_block_is_refused() {
    let src = "arrangement s\nwhen s { }\nView v { Rectangle bar }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `arr.err-transition-dup` (6772): two bare view-level `transition`
/// rules cover the same change and are refused — two rules for one pair
/// would pick silently (GP31's duplicate fence, 2026-09-01).
#[test]
fn two_bare_transitions_are_refused() {
    let src = "arrangement s\ntransition duration=0.1\ntransition duration=0.2\nView v { Rectangle bar }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `arr.err-transition-no-arrow` (6777): `transition s duration=0.1` has
/// an endpoint but no arrow and is refused — in Rust too (G0034, E4).
#[test]
fn transition_endpoint_needs_the_arrow() {
    let src = "arrangement s\ntransition s duration=0.1\nView v { Rectangle bar }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `arr.err-transition-builtin-at-top` 6779, `arr.err-control-exact-pair`
/// 6781, `arr.err-arrangement-at-control` 6786, `ptim.err-one-sided` 6956,
/// `ptim.err-node-anim` 6960: where a transition rule may live — control
/// states inside the style/node block one side at a time, arrangement
/// endpoints at the view level unless a node names an EXACT pose pair,
/// and `anim=` only on the view rule. Rust checks none of the scope rules.
#[test]
#[ignore = "GP31: not ported: transition scope rules (control states in the block, exact pairs on nodes, anim= at the view)"]
fn transition_scope_rules_are_checked() {
    assert!(refused("transition any -> held duration=0.1\nView v { Rectangle bar }"), "a control state at the top level");
    assert!(refused("style S {\n    transition held -> selected duration=0.1\n    when held opacity=0.5\n}\nView v { Button b .S }"), "two control states in one rule");
    assert!(refused("arrangement s\nstyle S {\n    transition any -> s duration=0.1\n    when held opacity=0.5\n}\nView v { Button b .S }"), "a one-sided arrangement rule on a control");
    assert!(refused("arrangement open\nView v { Rectangle r 10x10 { transition open -> any duration=0.1 } }"), "a one-sided arrangement rule on a node");
    assert!(refused("arrangement open\nanim n { r.opacity to=1, duration=0.1 }\nView v { Rectangle r 10x10 { transition base -> open duration=0.1, anim=n } }"), "anim= on a node pair rule");
}

/// `arr.err-toggle-form` (6796): `toggle(a, b)` is not the toggle form.
/// Rust reads the first argument and drops the rest.
#[test]
fn toggle_takes_exactly_one_arrangement() {
    let src = "arrangement a\nView v { Button b 50x50, on:activate=toggle(a, b) }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `arr.toggle-mode-lint` (6803): `toggle=true` together with
/// `on:activate=toggle(a)` warns — the button's own on/off and the pose
/// drift apart once anything else flips. Rust has no such lint.
#[test]
#[ignore = "GP41: not ported: the toggle=true + toggle(arrangement) two-states lint"]
fn toggle_mode_plus_toggle_action_warns() {
    let ds = diags("arrangement a\nView v { Button b 50x50, toggle=true, on:activate=toggle(a) }");
    assert!(!gogo_ui::has_errors(&ds));
    assert!(ds.iter().any(|d| d.severity == Severity::Warning && d.message.contains("two separate")), "{ds:#?}");
}

/// `arr.toggle-lint-quiet` (6809): the momentary form (no `toggle=true`)
/// builds without that warning.
#[test]
fn momentary_toggle_wire_is_quiet() {
    let ds = diags("arrangement a\nView v { Button b 50x50, on:activate=toggle(a) }");
    assert!(!gogo_ui::has_errors(&ds), "{ds:#?}");
    assert!(!ds.iter().any(|d| d.message.contains("two separate")), "{ds:#?}");
}

// ---------------------------------------------- _arrangement_tests: runtime

/// `arr.build` 6822, `arr.base-pose` 6827, `arr.current-base` 6829,
/// `arr.current-flipped` 6832, `arr.tween-midway` 6836, `arr.pose-applied`
/// 6840, `arr.style-when-follows` 6845, `arr.reassert-on-units` 6850,
/// `arr.back-to-base` 6873 (state_view.gogo): flipping to `collapsed` moves
/// the bar 40 below the frame and to 90 tall through the bare 0.2 s rule,
/// fades the scrim to 0.5, a style-scoped `when collapsed` follows the
/// view's flip, a later relayout keeps the pose, and `base` brings
/// everything home. Godot read offsets (bottom 0 → 40, top -60 → -50);
/// here the bar's rect edges against the frame height say the same.
#[test]
fn arrangement_flip_poses_tweens_and_comes_home() {
    let c = ok(STATE_VIEW);
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let scrim = page.find("scrim").unwrap();
    let dot = page.find("dot").unwrap();
    let collapsed = gogo_ui::ArrangementId::from_index(page.arrangements.iter().position(|a| page.str(*a) == "collapsed").unwrap());
    let mut rt = Runtime::new(&page);
    let f = frame(&mut page, &mut rt, 0.0);
    let (_, rects) = (f.look(), f.rects());
    let b = rects[bar.index()];
    assert!(near(b.bottom(), H) && near(b.y, H - 60.0), "base pose: {b:?}");
    assert_eq!(rt.arrangement, BASE);

    rt.go_id(collapsed);
    assert_eq!(rt.arrangement, collapsed);
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.05);
    let (_, rects) = (f.look(), f.rects());
    let b = rects[bar.index()];
    assert!(b.bottom() > H && b.bottom() < H + 40.0, "midway through the 0.2 s rule: {b:?}");
    let f = frame(&mut page, &mut rt, 0.3);
    let (res, rects) = (f.look(), f.rects());
    let b = rects[bar.index()];
    assert!(near(b.bottom(), H + 40.0) && near(b.y, H - 50.0) && near(opacity(&page, &res, scrim), 0.5), "pose applied: {b:?} scrim={}", opacity(&page, &res, scrim));
    assert!(near(opacity(&page, &res, dot), 0.25), "the style-scoped when follows the view: {}", opacity(&page, &res, dot));

    // A relayout (a units change) keeps the ACTIVE pose, not the base.
    let res = resolve(&page, &rt);
    let again = layout(&page, &res, Rect::new(0.0, 0.0, W * 2.0, H), &FixedAdvance);
    let b = again[bar.index()];
    assert!(near(b.bottom(), H + 40.0) && near(b.y, H - 50.0), "re-assert on relayout: {b:?}");

    rt.go_id(BASE);
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.3);
    let (res, rects) = (f.look(), f.rects());
    let b = rects[bar.index()];
    assert!(near(b.bottom(), H) && near(opacity(&page, &res, scrim), 1.0) && near(opacity(&page, &res, dot), 1.0), "back to base: {b:?} scrim={} dot={}", opacity(&page, &res, scrim), opacity(&page, &res, dot));
}

/// `set_arrangement(name, instant := true)` (gogo_states.gd:717-731,
/// GP37): the pose flips and every `when <arrangement>` restyles, but the
/// timed view rule is skipped — no glide, nothing animating; the SAME
/// flip through plain `go` glides.
#[test]
fn go_instant_flips_without_the_motion() {
    let c = ok(STATE_VIEW);
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let collapsed = gogo_ui::ArrangementId::from_index(page.arrangements.iter().position(|a| page.str(*a) == "collapsed").unwrap());
    let mut rt = Runtime::new(&page);
    frame(&mut page, &mut rt, 0.0);
    rt.go_instant_id(collapsed);
    let f = frame(&mut page, &mut rt, 0.0);
    let (_, rects) = (f.look(), f.rects());
    assert!(!rt.animating(), "an instant flip arms nothing");
    let b = rects[bar.index()];
    assert!(near(b.bottom(), H + 40.0) && near(b.y, H - 50.0), "the pose is already applied: {b:?}");
}

/// `arrangement_settled` (gogo_states.gd:646,778, GP37): the awaitable
/// for hosts sequencing work after a pose change — fires once when the
/// flip's motion has landed; an instant flip settles right after its
/// resolve; nothing re-fires later.
#[test]
fn arrangement_settled_fires_once_after_the_motion() {
    let c = ok(STATE_VIEW);
    let mut page = c.page;
    let collapsed = gogo_ui::ArrangementId::from_index(page.arrangements.iter().position(|a| page.str(*a) == "collapsed").unwrap());
    let mut rt = Runtime::new(&page);
    frame(&mut page, &mut rt, 0.0);
    rt.go_id(collapsed);
    assert_eq!(rt.arrangement_settled(), None, "not before the flip resolves");
    frame(&mut page, &mut rt, 0.0);
    frame(&mut page, &mut rt, 0.05);
    assert_eq!(rt.arrangement_settled(), None, "mid-glide is not settled");
    frame(&mut page, &mut rt, 0.3);
    assert_eq!(rt.arrangement_settled(), Some(collapsed), "landed = settled");
    assert_eq!(rt.arrangement_settled(), None, "fires once");
    rt.go_instant_id(BASE);
    frame(&mut page, &mut rt, 0.0);
    assert_eq!(rt.arrangement_settled(), Some(BASE), "an instant flip settles right after its resolve");
}

/// `arr.go-flips` 6880, `arr.toggle-home` 6888, `arr.toggle-flips` 6891,
/// `arr.toggle-from-other-pose` 6895: the wired buttons flip the
/// arrangement through the real input path — `go(collapsed)` goes there,
/// `toggle(collapsed)` comes home when its pose is active and goes there
/// otherwise, including from a DIFFERENT pose.
#[test]
fn go_and_toggle_buttons_flip_the_arrangement() {
    let c = ok(STATE_VIEW);
    let page = c.page;
    let go_btn = page.find("goBtn").unwrap();
    let toggle_btn = page.find("toggleBtn").unwrap();
    let idx = |name: &str| gogo_ui::ArrangementId::from_index(page.arrangements.iter().position(|a| page.str(*a) == name).unwrap());
    let (collapsed, wide) = (idx("collapsed"), idx("wide"));
    let mut rt = Runtime::new(&page);
    let mut input = Input::new();
    tap(&page, &mut rt, &mut input, go_btn, 1.0);
    assert_eq!(rt.arrangement, collapsed, "go() flips");
    tap(&page, &mut rt, &mut input, toggle_btn, 2.0);
    assert_eq!(rt.arrangement, BASE, "toggle from its own pose comes home");
    tap(&page, &mut rt, &mut input, toggle_btn, 3.0);
    assert_eq!(rt.arrangement, collapsed, "toggle from base goes there");
    rt.go_id(wide);
    tap(&page, &mut rt, &mut input, toggle_btn, 4.0);
    assert_eq!(rt.arrangement, collapsed, "toggle from another pose goes there");
}

/// `arr.unknown-flip-safe` (6897): flipping to an arrangement the file
/// never declared is refused and the current pose stays. Rust accepts
/// any index, which drops the pose.
#[test]
fn unknown_arrangement_flip_keeps_the_pose() {
    let c = ok(STATE_VIEW);
    let page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    rt.go_id(gogo_ui::ArrangementId::new(0));
    rt.go_id(gogo_ui::ArrangementId::new(99));
    let f = lay(&page, &rt);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(rt.arrangement, gogo_ui::ArrangementId::new(0), "the pose is kept");
    assert!(near(rects[bar.index()].h, 90.0), "{:?}", rects[bar.index()]);
}

const DIR_STYLE: &str = "style T background=#00000000 {\n    transition any -> held duration=0.2\n    transition held -> any duration=0.1\n    when held opacity=0.5\n}\nView v { Button b .T }";
const MIRROR_STYLE: &str = "style M background=#00000000 {\n    transition any -> selected duration=0.2\n    when selected opacity=0.5\n}\nView v { Button b .M toggle=true }";

/// Runs Godot's `_process` ladder for the directional style (6705-6714):
/// held → 0.1 s → 0.2 s more → released → 0.05 s → 0.1 s more; returns
/// the opacity at each of the four reads.
fn dir_readings() -> [f32; 4] {
    let c = ok(DIR_STYLE);
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.set_state(b, HELD, true);
    resolve_animated(&page, &mut rt);
    let mut out = [0.0; 4];
    for (i, (dt, release)) in [(0.1, false), (0.2, false), (0.05, true), (0.1, false)].into_iter().enumerate() {
        if release {
            rt.set_state(b, HELD, false);
            resolve_animated(&page, &mut rt);
        }
        rt.tick(&mut page, dt);
        let res = resolve_animated(&page, &mut rt);
        out[i] = opacity(&page, &res, b);
    }
    out
}

/// Godot's mirror ladder (6731-6741): selected, 20 frames of 16 ms,
/// unselected, one frame + 0.1 s (the mid read), 20 more frames. Returns
/// (on, off-mid, off).
fn mirror_readings() -> (f32, f32, f32) {
    let c = ok(MIRROR_STYLE);
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.set_state(b, SELECTED, true);
    let mut res = resolve_animated(&page, &mut rt);
    for _ in 0..20 {
        rt.tick(&mut page, 0.016);
        res = resolve_animated(&page, &mut rt);
    }
    let on = opacity(&page, &res, b);
    rt.set_state(b, SELECTED, false);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.016);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.1);
    res = resolve_animated(&page, &mut rt);
    let mid = opacity(&page, &res, b);
    for _ in 0..20 {
        rt.tick(&mut page, 0.016);
        res = resolve_animated(&page, &mut rt);
    }
    (on, mid, opacity(&page, &res, b))
}

/// `arr.dir-build` 6700, `arr.dir-entered` 6709, `arr.dir-exited` 6714,
/// `arr.mirror-build` 6726, `arr.mirror-on` 6734, `arr.mirror-off` 6741:
/// control-scope rules on a style build, and the `when held` /
/// `when selected` look is fully there once the rule's time has passed
/// and fully gone after the return — including the return a lone
/// `any -> selected` rule mirrors.
#[test]
fn control_scope_transitions_land_on_both_sides() {
    let [_, entered, _, exited] = dir_readings();
    assert!(near(entered, 0.5), "entered: {entered}");
    assert!(near(exited, 1.0), "exited: {exited}");
    let (on, _, off) = mirror_readings();
    assert!(near(on, 0.5), "mirror on: {on}");
    assert!(near(off, 1.0), "mirror off: {off}");
}

/// `arr.dir-enter-mid` 6707, `arr.dir-exit-mid` 6712, `arr.mirror-off-mid`
/// 6738: the rules TIME the look — half way in at 0.1 of 0.2 s (0.75),
/// half way out at 0.05 of 0.1 s (0.75), and the mirrored return is
/// mid-flight after 0.116 s. Rust snaps: the Button writes no `opacity=`,
/// so the tween has no base value to start from (writing `opacity=1` on
/// the Button makes the same ladder read 0.75).
#[test]
fn control_scope_transitions_tween_from_the_default_value() {
    let [enter_mid, _, exit_mid, _] = dir_readings();
    assert!(near(enter_mid, 0.75), "enter mid: {enter_mid}");
    assert!(near(exit_mid, 0.75), "exit mid: {exit_mid}");
    let (_, mid, _) = mirror_readings();
    assert!(mid > 0.5 && mid < 1.0, "mirror off mid: {mid}");
}

/// `ptim.build` 6915, `ptim.view-timing` 6925, `ptim.view-landed` 6928,
/// `ptim.node-lands` 6931, `ptim.home` 6937, `ptim.retarget-no-snap` 6945,
/// `ptim.retarget-from-current` 6947 (pose_timing.gogo): the view-level
/// `base -> open 0.5 s cubic_out` rule carries `slab` (87.5 at 0.25 s,
/// landed at 0.55 s); every box is home after the return; a mid-flight
/// re-flip does not snap and the new tween starts FROM THE CURRENT value
/// (87.5 · cubic_out(0.1) home = 63.7875).
#[test]
fn view_rule_times_the_flip_and_retargets_mid_flight() {
    let c = ok(POSE_TIMING);
    let mut page = c.page;
    let slab = (page.find("slab").unwrap()).index();
    let plate = (page.find("plate").unwrap()).index();
    let tag = (page.find("tag").unwrap()).index();
    let mut rt = Runtime::new(&page);
    frame(&mut page, &mut rt, 0.0);
    rt.go_id(gogo_ui::ArrangementId::new(0));
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.25);
    let (_, r) = (f.look(), f.rects());
    assert!((r[slab].y - 87.5).abs() < 0.01, "view timing: {:?}", r[slab]);
    let f = frame(&mut page, &mut rt, 0.3);
    let (_, r) = (f.look(), f.rects());
    assert!(near(r[slab].y, 100.0), "view landed: {:?}", r[slab]);
    let f = frame(&mut page, &mut rt, 0.5);
    let (_, r) = (f.look(), f.rects());
    assert!(near(r[plate].y, 100.0), "node lands: {:?}", r[plate]);

    rt.go_id(BASE);
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 10.0);
    let (_, r) = (f.look(), f.rects());
    assert!(near(r[slab].y, 0.0) && near(r[plate].y, 0.0) && near(r[tag].y, 0.0), "home: {:?}/{:?}/{:?}", r[slab], r[plate], r[tag]);
    rt.go_id(gogo_ui::ArrangementId::new(0));
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.25);
    let (_, r) = (f.look(), f.rects());
    let mid = r[slab].y;
    rt.go_id(BASE);
    let f = frame(&mut page, &mut rt, 0.0);
    let (_, r) = (f.look(), f.rects());
    assert!((r[slab].y - mid).abs() < 0.01, "retarget: no snap at the flip: {} vs {mid}", r[slab].y);
    let f = frame(&mut page, &mut rt, 0.05);
    let (_, r) = (f.look(), f.rects());
    assert!((r[slab].y - 63.7875).abs() < 0.01, "retarget from the current value: {}", r[slab].y);
}

/// `ptim.zero-snaps` 6923, `ptim.node-slower` 6926, `ptim.node-still-going`
/// 6929 (pose_timing.gogo): a node's own EXACT pose-pair rule times its
/// share of the flip — `tag` (duration=0) snaps to 100 at flip time,
/// `plate` (1.0 s) is at 57.8125 when the view rule's `slab` is at 87.5
/// and still short of 99 at 0.55 s. Rust glides all three by the view
/// rule: tag 0 at the flip, plate 87.5 at 0.25 s, plate 100 at 0.55 s.
#[test]
fn node_pose_pair_rules_time_their_own_share() {
    let c = ok(POSE_TIMING);
    let mut page = c.page;
    let slab = (page.find("slab").unwrap()).index();
    let plate = (page.find("plate").unwrap()).index();
    let tag = (page.find("tag").unwrap()).index();
    let mut rt = Runtime::new(&page);
    frame(&mut page, &mut rt, 0.0);
    rt.go_id(gogo_ui::ArrangementId::new(0));
    let f = frame(&mut page, &mut rt, 0.0);
    let (_, r) = (f.look(), f.rects());
    assert!(near(r[tag].y, 100.0), "duration=0 snaps at flip time: {:?}", r[tag]);
    let f = frame(&mut page, &mut rt, 0.25);
    let (_, r) = (f.look(), f.rects());
    assert!((r[slab].y - 87.5).abs() < 0.01, "{:?}", r[slab]);
    assert!((r[plate].y - 57.8125).abs() < 0.01, "node slower: {:?}", r[plate]);
    let f = frame(&mut page, &mut rt, 0.3);
    let (_, r) = (f.look(), f.rects());
    assert!(r[plate].y < 99.0, "node still going at 0.55 s: {:?}", r[plate]);
}

// ------------------------------------------------------ _screen_tests: build

/// `screen.err-unknown-class` 6975, `screen.err-bare-prop` 6977,
/// `screen.err-unknown-node` 6979, `screen.align-on-untimed-arrangement`
/// 6990, `screen.err-pair-order` 7003, `screen.err-pair-unknown-arrangement`
/// 7005, `screen.err-pair-unknown-class` 7007,
/// `screen.err-pair-second-not-screen` 7009: the screen vocabulary is
/// closed, a screen `when` writes `node.prop`, an alignment pose on an
/// UNTIMED arrangement builds, and a composed pair is spelled
/// `<arrangement> screen.<class>` in that order.
#[test]
fn screen_when_build_rules_match() {
    assert!(has_code("when screen.bigPhone { bar.width=2 }\nView v { Rectangle bar }", "G0036"), "unknown class");
    assert!(has_code("when screen.landscapeSmall { width=2 }\nView v { Rectangle bar }", "G0059"), "bare prop");
    assert!(has_code("when screen.landscapeSmall { foo.width=2 }\nView v { Rectangle bar }", "G0060"), "unknown node");
    let untimed = "arrangement s\nwhen s { bar.halign=left }\nView v { Layers l width=100, height=100 { Rectangle bar margin_left=10, margin_right=10, height=20 } }";
    assert!(!refused(untimed), "{:#?}", diags(untimed));
    assert!(has_code("arrangement o\nwhen screen.landscapeSmall o { bar.opacity=0.5 }\nView v { Rectangle bar }", "G0038"), "pair order");
    assert!(has_code("when o screen.landscapeSmall { bar.opacity=0.5 }\nView v { Rectangle bar }", "G0036"), "pair with an undeclared arrangement");
    assert!(has_code("arrangement o\nwhen o screen.bigPhone { bar.opacity=0.5 }\nView v { Rectangle bar }", "G0036"), "pair with an unknown class");
    assert!(has_code("arrangement o\narrangement c\nwhen o c { bar.opacity=0.5 }\nView v { Rectangle bar }", "G0038"), "pair whose second half is not a screen word");
}

/// `screen.err-align-value` (6999): a posed `halign=up` is not a valid
/// alignment and is refused (the E9 per-axis G0057 fence — before it,
/// Rust built this and the value landed as unset).
#[test]
fn posed_alignment_value_is_checked() {
    let src = "when screen.landscapeSmall { bar.halign=up }\nView v { Layers l width=100, height=100 { Rectangle bar margin_left=10, margin_right=10, height=20 } }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `screen.err-dup-prop` 6983, `screen.err-pair-dup-prop` 7020: two `when`
/// blocks of the SAME condition may not both set one prop. Rust merges
/// them and the later line wins silently.
#[test]
fn two_blocks_of_one_condition_may_not_both_set_a_prop() {
    assert!(refused("when screen.landscapeSmall { bar.opacity=0.5 }\nwhen screen.landscapeSmall { bar.opacity=1 }\nView v { Rectangle bar }"), "class");
    assert!(refused("arrangement o\nwhen o screen.landscapeSmall { bar.opacity=0.5 }\nwhen o screen.landscapeSmall { bar.opacity=1 }\nView v { Rectangle bar }"), "pair");
}

/// `screen.err-container-seat` (6985): a pose may not re-pin a stack
/// child with a margin — the Column seats its children. Rust builds it
/// (the static-margin lint G1014 does not look into `when` blocks).
#[test]
fn posed_margin_on_a_stack_child_is_refused() {
    let src = "when screen.landscapeSmall { bar.margin_left=4 }\nView v { Column c { Rectangle bar width=20, height=20 } }";
    assert!(refused(src), "{:#?}", diags(src));
}

/// `screen.err-align-on-timed-arrangement` 6994,
/// `screen.err-align-timed-points-both` 6996, `screen.err-pair-align-tweened`
/// 7013, `screen.err-pair-align-any-tweened` 7016: Godot refuses an
/// alignment pose under an arrangement a `transition` rule times (G3003:
/// a re-anchor cannot animate). DECIDED the other way here (todo-gogo.md
/// GP25 SHIPPED: "the G3003/G3004 fences are moot — Rust glides what
/// Godot refused"): all four spellings build, and the box GLIDES to its
/// new anchor over the rule's duration.
#[test]
fn alignment_pose_under_a_timed_arrangement_builds_and_glides() {
    let body = "View v { Layers l width=100, height=100, halign=left, valign=top { Rectangle bar margin_left=10, margin_right=10, height=20 } }";
    let plain = format!("arrangement s\ntransition base -> s duration=0.2\nwhen s {{ bar.halign=left }}\n{body}");
    let pair = format!("arrangement o\ntransition base -> o duration=0.2\nwhen o screen.landscapeSmall {{ bar.halign=left }}\n{body}");
    let any_pair = format!("arrangement o\ntransition any -> base duration=0.2\nwhen o screen.landscapeSmall {{ bar.halign=left }}\n{body}");
    for src in [&pair, &any_pair] {
        assert!(!refused(src), "{:#?}", diags(src));
    }
    let c = ok(&plain);
    let mut page = c.page;
    let bar = (page.find("bar").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let f = frame(&mut page, &mut rt, 0.0);
    let (_, r) = (f.look(), f.rects());
    assert_eq!(r[bar].w, 80.0, "base spans the margins: {:?}", r[bar]);
    rt.go_id(gogo_ui::ArrangementId::new(0));
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.1);
    let (_, r) = (f.look(), f.rects());
    assert!(r[bar].w < 80.0 && r[bar].w > 0.0, "gliding toward the left-docked hug: {:?}", r[bar]);
    let f = frame(&mut page, &mut rt, 0.2);
    let (_, r) = (f.look(), f.rects());
    assert!(r[bar].w < 80.0, "landed on the re-declared axis: {:?}", r[bar]);
    assert!(!rt.animating());
}

/// `screen.err-pair-node-scope` (7018): Godot takes a composed `when
/// <arrangement> screen.<class>` at view level only. FLIP (GPAR94,
/// DECIDED by the user 2026-09-01): a `when` works the same wherever a
/// `when` is legal — adding a screen word to a node's condition must not
/// force the block to the top of the file, and a STYLE's blocks (which
/// attach to nodes) could otherwise never compose at all. The one-writer
/// fence still refuses two blocks on one condition writing one prop,
/// wherever the two sit, so conflicts stay visible.
#[test]
fn composed_pair_binds_at_node_scope_too() {
    let src = "arrangement o\nView v { Rectangle bar { when o screen.landscapeSmall { opacity=0.5 } } }";
    assert!(!refused(src), "{:#?}", diags(src));
}

// ---------------------------------------------------- _screen_tests: runtime

/// `screen.build` 7029, `screen.base-pose` 7035, `screen.base-pane-span`
/// 7038, `screen.current-empty` 7041, `screen.current-set` 7046,
/// `screen.pose-applied` 7047, `screen.composes-under-arrangement` 7061,
/// `screen.pair-screen-off` 7083, `screen.arrangement-home-keeps-class`
/// 7097, `screen.reassert-on-units` 7109 (screen_view.gogo): the class
/// pose lands instantly (bar 10 below the frame, 80 tall, tag dimmed),
/// under `open` the arrangement wins the contested `margin_bottom` (equal
/// rank — its block is the LATER one in the fixture) and the
/// class keeps `height` and the tag, a different class while open drops
/// the pair, leaving the arrangement lands on the SCREEN pose, a relayout
/// keeps it. Godot read anchor offsets; the rect edges say the same.
#[test]
fn screen_class_poses_compose_with_arrangements() {
    let c = ok(SCREEN_VIEW);
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let tag = page.find("tag").unwrap();
    let scrim = page.find("scrim").unwrap();
    let pane = page.find("pane").unwrap();
    let chip = page.find("chip").unwrap();
    let open = gogo_ui::ArrangementId::new(0);
    let mut rt = Runtime::new(&page);
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    let b = r[bar.index()];
    assert!(near(b.bottom(), H) && near(b.y, H - 60.0) && near(opacity(&page, &res, tag), 1.0), "base pose: {b:?}");
    let p = r[pane.index()];
    assert!(near(p.x, 10.0) && near(p.right(), W - 10.0), "base pane spans its margins: {p:?}");
    assert!(rt.screen.is_empty(), "no class named yet: {:?}", rt.screen);

    set_class(&page, &mut rt, ScreenClass::LandscapeSmall);
    assert!(rt.screen.contains(&page.intern_lookup("landscapeSmall")), "{:?}", rt.screen);
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    let b = r[bar.index()];
    assert!(near(b.bottom(), H + 10.0) && near(b.y, H - 70.0) && near(opacity(&page, &res, tag), 0.25), "class pose: {b:?} tag={}", opacity(&page, &res, tag));

    rt.go_id(open);
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.3);
    let (res, r) = (f.look(), f.rects());
    let b = r[bar.index()];
    assert!(near(b.bottom(), H + 40.0) && near(b.y, H - 40.0) && near(opacity(&page, &res, scrim), 0.5) && near(opacity(&page, &res, tag), 0.25), "composed: {b:?} scrim={} tag={}", opacity(&page, &res, scrim), opacity(&page, &res, tag));

    set_class(&page, &mut rt, ScreenClass::PortraitSmall);
    let f = frame(&mut page, &mut rt, 0.3);
    let (res, r) = (f.look(), f.rects());
    let p = r[pane.index()];
    assert!(near(p.x, 10.0) && near(p.right(), W - 10.0) && near(opacity(&page, &res, chip), 1.0), "pair lets go with the class: {p:?} chip={}", opacity(&page, &res, chip));

    set_class(&page, &mut rt, ScreenClass::LandscapeSmall);
    rt.go_id(BASE);
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.3);
    let (_, r) = (f.look(), f.rects());
    let b = r[bar.index()];
    assert!(near(b.bottom(), H + 10.0) && near(b.y, H - 70.0), "leaving the arrangement lands on the screen pose: {b:?}");
    let res = resolve(&page, &rt);
    let again = layout(&page, &res, Rect::new(0.0, 0.0, W * 2.0, H), &FixedAdvance);
    assert!(near(again[bar.index()].bottom(), H + 10.0), "re-assert on relayout: {:?}", again[bar.index()]);
}

/// `screen.axis-redeclare` 7052, `screen.pair-applied` 7072,
/// `screen.pair-reapplies` 7090, `screen.pair-arr-off` 7102
/// (screen_view.gogo): the pane's axis re-declare — `halign=left,
/// width=80, margin_left=20` on a box whose base also names
/// `margin_right=10` — docks it at x=20 (right edge 100); the composed
/// block's `margin_left=40` wins the class pose's 20 while `open` holds
/// (chip dims to 0.5), comes back after a class round trip, and drops to
/// the class pose's 20 when the arrangement leaves. Rust counts the near
/// margin twice on an aligned axis with both margins (x=40 / 80).
#[test]
fn pane_axis_redeclare_and_pair_margin() {
    let c = ok(SCREEN_VIEW);
    let mut page = c.page;
    let pane = page.find("pane").unwrap();
    let chip = page.find("chip").unwrap();
    let mut rt = Runtime::new(&page);
    set_class(&page, &mut rt, ScreenClass::LandscapeSmall);
    let f = lay(&page, &rt);
    let (_, r) = (f.look(), f.rects());
    let p = r[pane.index()];
    assert!(near(p.x, 20.0) && near(p.right(), 100.0), "axis re-declare: {p:?}");
    rt.go_id(gogo_ui::ArrangementId::new(0));
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.3);
    let (res, r) = (f.look(), f.rects());
    let p = r[pane.index()];
    assert!(near(p.x, 40.0) && near(p.right(), 120.0) && near(opacity(&page, &res, chip), 0.5), "pair applied: {p:?} chip={}", opacity(&page, &res, chip));
    set_class(&page, &mut rt, ScreenClass::PortraitSmall);
    frame(&mut page, &mut rt, 0.3);
    set_class(&page, &mut rt, ScreenClass::LandscapeSmall);
    let f = frame(&mut page, &mut rt, 0.3);
    let (res, r) = (f.look(), f.rects());
    let p = r[pane.index()];
    assert!(near(p.x, 40.0) && near(opacity(&page, &res, chip), 0.5), "pair re-applies: {p:?} chip={}", opacity(&page, &res, chip));
    rt.go_id(BASE);
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.3);
    let (res, r) = (f.look(), f.rects());
    let p = r[pane.index()];
    assert!(near(p.x, 20.0) && near(p.right(), 100.0) && near(opacity(&page, &res, chip), 1.0), "pair off with the arrangement: {p:?} chip={}", opacity(&page, &res, chip));
}

/// `screen.late-build-ok` 7116, `screen.late-build-posed` 7122: a view
/// built AFTER the class was named starts posed — here a fresh Runtime
/// told the class before its first frame.
#[test]
fn a_runtime_told_the_class_before_its_first_frame_starts_posed() {
    let c = ok(SCREEN_VIEW);
    let page = c.page;
    let bar = page.find("bar").unwrap();
    let tag = page.find("tag").unwrap();
    let mut rt = Runtime::new(&page);
    set_class(&page, &mut rt, ScreenClass::LandscapeSmall);
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].bottom(), H + 10.0) && near(opacity(&page, &res, tag), 0.25), "{:?} tag={}", r[bar.index()], opacity(&page, &res, tag));
}

/// `screen.clear-restores-base` 7129, `screen.clear-restores-span` 7132:
/// Godot clears the class to "" and every view returns to base. Rust's
/// `set_screen` takes enums — a view size always grades to SOME class
/// (screen ruler decision, todo-gogo.md Decisions 2026-08-30), so "no
/// class" is not a state; the same observable is a class the file never
/// poses: everything is base.
#[test]
fn a_class_the_file_never_poses_is_base() {
    let c = ok(SCREEN_VIEW);
    let page = c.page;
    let bar = page.find("bar").unwrap();
    let tag = page.find("tag").unwrap();
    let pane = page.find("pane").unwrap();
    let mut rt = Runtime::new(&page);
    set_class(&page, &mut rt, ScreenClass::LandscapeSmall);
    set_class(&page, &mut rt, ScreenClass::PortraitMedium);
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    let b = r[bar.index()];
    assert!(near(b.bottom(), H) && near(opacity(&page, &res, tag), 1.0), "base: {b:?} tag={}", opacity(&page, &res, tag));
    let p = r[pane.index()];
    assert!(near(p.x, 10.0) && near(p.right(), W - 10.0), "base span: {p:?}");
}

/// `screen.unknown-class-safe` 7136, `screen.unknown-class-kept` 7138,
/// `orient.wrong-vocab-kept` 7261, `orient.class-wrong-vocab-kept` 7263:
/// Godot's string setters refuse an unknown word or a word from the other
/// vocabulary and keep the stored value. Rust's `set_screen` takes the
/// `Orientation` and `ScreenClass` enums (screen ruler decision,
/// todo-gogo.md 2026-08-30): a wrong word cannot be written. Pinned as:
/// every enum pair is accepted, the active words are always one
/// orientation + one class the page knows, and a class the page never
/// mentions is simply not active.
#[test]
fn screen_words_are_enums_a_wrong_word_is_unrepresentable() {
    let c = ok(ORIENT_VIEW);
    let page = c.page;
    let mut rt = Runtime::new(&page);
    let classes = [ScreenClass::PortraitSmall, ScreenClass::PortraitMedium, ScreenClass::PortraitLarge, ScreenClass::LandscapeSmall, ScreenClass::LotsOfSpace];
    for o in [Orientation::Portrait, Orientation::Landscape] {
        for class in classes {
            rt.set_screen(&page, o, class);
            let words: Vec<&str> = rt.screen.iter().map(|w| page.str(*w)).collect();
            assert!(words.len() <= 2, "{words:?}");
            assert!(words.iter().all(|w| ["portrait", "landscape", "lotsOfSpace"].contains(w)), "only words the page knows are active: {words:?}");
        }
    }
    rt.set_screen(&page, Orientation::Portrait, ScreenClass::PortraitSmall);
    assert!(rt.screen.is_empty(), "words the page never mentions are not active: {:?}", rt.screen);
    rt.set_screen(&page, Orientation::Landscape, ScreenClass::LotsOfSpace);
    let mut both = vec![page.intern_lookup("landscape"), page.intern_lookup("lotsOfSpace")];
    both.sort();
    assert_eq!(rt.screen, both, "{:?}", rt.screen);
}

// ------------------------------------------------- _orientation_tests: build

/// `orient.err-prop-fence` 7159, `orient.err-place-fence` 7161,
/// `orient.err-seat-fence` 7173: an orientation and a class can hold at
/// once, so one target posed (prop, placement or seat) from BOTH single
/// axes is refused (G3005) — the composed spelling says the intersection.
/// Rust builds all three.
#[test]
fn one_target_posed_by_orientation_and_class_is_refused() {
    assert!(refused("when screen.landscape { bar.opacity=0.5 }\nwhen screen.lotsOfSpace { bar.opacity=1.0 }\nView v { Rectangle bar }"), "prop");
    assert!(refused("when screen.landscape { bar.width=30 }\nwhen screen.portraitSmall { bar.height=9 }\nView v { Layers l width=100, height=100 { Rectangle bar halign=left, valign=top, width=20, height=20 } }"), "placement");
    assert!(refused("when screen.landscape { mover.seat = s1 }\nwhen screen.lotsOfSpace { mover.seat = s2 }\nView v {\n    Column home {\n        Row r1 { seat s1 }\n        Row r2 { seat s2 }\n        Column mh { Panel mover width=10, height=10 }\n    }\n}\n"), "seat");
}

/// `orient.pair-builds` 7176, `orient.arr-pair-builds` 7180,
/// `orient.err-pair-order` 7184, `orient.err-unknown` 7187,
/// `orient.err-unknown-in-pair` 7190: the orientation+class pair and the
/// arrangement+orientation pair build, the screen pair is spelled
/// orientation first, and an unknown screen word is refused naming both
/// vocabularies (Godot: "orientations are screen.portrait /
/// screen.landscape"; Rust lists the classes and the orientations).
#[test]
fn orientation_pairs_build_and_unknown_words_name_both_vocabularies() {
    assert!(!refused("when screen.landscape screen.lotsOfSpace { bar.opacity=0.5 }\nView v { Rectangle bar }"));
    assert!(!refused("arrangement o\nwhen o screen.landscape { bar.opacity=0.5 }\nView v { Rectangle bar }"));
    assert!(has_code("when screen.lotsOfSpace screen.landscape { bar.opacity=0.5 }\nView v { Rectangle bar }", "G0038"), "orientation first");
    let unknown = "when screen.sideways { bar.opacity=0.5 }\nView v { Rectangle bar }";
    assert!(has_code(unknown, "G0036"));
    let text = error_text(unknown);
    assert!(text.contains("orientations are") && text.contains("portrait") && text.contains("landscape") && text.contains("classes are"), "{text}");
    assert!(has_code("when screen.landscap screen.lotsOfSpace { bar.opacity=0.5 }\nView v { Rectangle bar }", "G0036"), "unknown orientation in a pair");
}

/// `orient.err-three-conditions` (7194): a third `when` condition gets the
/// composition rule ("at most two conditions") and the either-or hint
/// (same-axis classes never AND). PORT (E4): Rust names both (G0038);
/// before E4 it was a bare G0017 "missing `,`" that named neither.
#[test]
fn a_third_when_condition_names_the_composition_rule() {
    let src = "arrangement previewAbove\nwhen previewAbove screen.landscape screen.lotsOfSpace { bar.opacity=0.5 }\nView v { Rectangle bar }";
    assert!(refused(src));
    let text = error_text(src);
    assert!(text.contains("two conditions") && text.contains("EITHER-OR"), "{text}");
}

// ----------------------------------------------- _orientation_tests: runtime

/// `orient.build` 7204, `orient.base` 7214, `orient.current-empty` 7218,
/// `orient.current-set` 7222, `orient.pose-applied` 7223,
/// `orient.seat-moved` 7226, `orient.pair-applied` 7232,
/// `orient.arr-off-keeps-pair` 7242, `orient.pair-class-off` 7247,
/// `orient.portrait-restores` 7254 (orient_view.gogo): the orientation
/// axis poses (bar 20 → 30, tag dims, the mover moves into `sideSeat`
/// inside the strip), the orientation+class pair outranks the single
/// (44, chip dims), leaving `open` lands back on the pair, dropping the
/// class lands on the single, and portrait drops everything landscape
/// carried (mover home).
#[test]
fn orientation_axis_poses_pairs_and_the_seat_move() {
    let c = ok(ORIENT_VIEW);
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let tag = page.find("tag").unwrap();
    let chip = page.find("chip").unwrap();
    let mover = page.find("mover").unwrap();
    let side_seat = page.find("sideSeat").unwrap();
    let strip = page.find("strip").unwrap();
    let mover_home = page.find("moverHome").unwrap();
    let mut rt = Runtime::new(&page);
    let inside = |inner: Rect, outer: Rect| inner.x >= outer.x && inner.y >= outer.y && inner.right() <= outer.right() && inner.bottom() <= outer.bottom();

    let f = lay(&page, &rt);

    let (res, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].h, 20.0) && near(opacity(&page, &res, tag), 1.0), "base: {:?}", r[bar.index()]);
    assert_eq!(res.seat[mover.index()], NO_NODE, "the mover is home");
    assert!(inside(r[mover.index()], r[mover_home.index()]), "mover {:?} in home {:?}", r[mover.index()], r[mover_home.index()]);
    assert!(rt.screen.is_empty());

    rt.set_screen(&page, Orientation::Landscape, ScreenClass::LandscapeSmall);
    assert!(rt.screen.contains(&page.intern_lookup("landscape")));
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].h, 30.0) && near(opacity(&page, &res, tag), 0.25), "orientation pose: {:?} tag={}", r[bar.index()], opacity(&page, &res, tag));
    assert_eq!(res.seat[mover.index()], side_seat, "seat moved");
    assert!(inside(r[mover.index()], r[strip.index()]), "mover {:?} in strip {:?}", r[mover.index()], r[strip.index()]);

    rt.set_screen(&page, Orientation::Landscape, ScreenClass::LotsOfSpace);
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].h, 44.0) && near(opacity(&page, &res, chip), 0.5), "pair over the single: {:?} chip={}", r[bar.index()], opacity(&page, &res, chip));

    rt.go_id(gogo_ui::ArrangementId::new(0));
    frame(&mut page, &mut rt, 0.3);
    rt.go_id(BASE);
    frame(&mut page, &mut rt, 0.0);
    let f = frame(&mut page, &mut rt, 0.3);
    let (_, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].h, 44.0), "arrangement off keeps the pair: {:?}", r[bar.index()]);

    rt.set_screen(&page, Orientation::Landscape, ScreenClass::LandscapeSmall);
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].h, 30.0) && near(opacity(&page, &res, chip), 1.0), "class off, single back: {:?} chip={}", r[bar.index()], opacity(&page, &res, chip));

    rt.set_screen(&page, Orientation::Portrait, ScreenClass::PortraitSmall);
    let f = lay(&page, &rt);
    let (res, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].h, 20.0) && near(opacity(&page, &res, tag), 1.0), "portrait restores: {:?}", r[bar.index()]);
    assert_eq!(res.seat[mover.index()], NO_NODE);
    assert!(inside(r[mover.index()], r[mover_home.index()]), "mover {:?} back in home {:?}", r[mover.index()], r[mover_home.index()]);
}

/// `orient.arr-pair-wins` (7238), re-read under the flattened ladder
/// (E2, 2026-08-31; GPAR47/GPAR49 closed): two composed pairs rank EQUAL
/// (two conditions each), so the later line in the file wins. In the
/// fixture `open screen.landscape` (52) sits below `screen.landscape
/// screen.lotsOfSpace` (44) and wins; swap the two blocks and the screen
/// pair wins — 44. Both orders pinned so the rule, not the fixture's
/// accident, is what passes.
#[test]
fn equal_count_pairs_tie_to_the_later_line() {
    let swapped = ORIENT_VIEW.replace(
        "when screen.landscape screen.lotsOfSpace {\n    bar.height = 44\n    chip.opacity = 0.5\n}\nwhen open screen.landscape {\n    bar.height = 52\n}\n",
        "when open screen.landscape {\n    bar.height = 52\n}\nwhen screen.landscape screen.lotsOfSpace {\n    bar.height = 44\n    chip.opacity = 0.5\n}\n",
    );
    assert_ne!(swapped, ORIENT_VIEW, "the swap must have happened");
    for (src, want) in [(ORIENT_VIEW, 52.0), (swapped.as_str(), 44.0)] {
        let c = ok(src);
        let page = c.page;
        let bar = page.find("bar").unwrap();
        let mut rt = Runtime::new(&page);
        rt.set_screen(&page, Orientation::Landscape, ScreenClass::LotsOfSpace);
        rt.go_id(gogo_ui::ArrangementId::new(0));
        let f = lay(&page, &rt);
        let (res, _) = (f.look(), f.rects());
        assert_eq!(res.placement[bar.index()].height, Len::Exact(want), "{:?}", res.placement[bar.index()]);
    }
}

/// The single-vs-single half of the same rule: an arrangement `when` and
/// a screen `when` posing one key rank equal (one condition each) — the
/// later line wins, whichever kind it is. Before the flatten the
/// arrangement always won; now moving the block moves the win. Pinned
/// for a placement key (height) and a style prop (opacity) in both
/// orders. Mirrors Godot `ladder.later-single-*`.
#[test]
fn arrangement_and_screen_singles_tie_to_the_later_line() {
    let screen_last = "arrangement open
when open {
    bar.height = 52
    tag.opacity = 0.2
}
when screen.landscape {
    bar.height = 30
    tag.opacity = 0.5
}
View v {
    Rectangle bar width=100, height=20, halign=left, valign=top, color=#FFFFFFFF
    Rectangle tag width=20, height=20, halign=right, valign=top, color=#FFFFFFFF
}
";
    let arrangement_last = "arrangement open
when screen.landscape {
    bar.height = 30
    tag.opacity = 0.5
}
when open {
    bar.height = 52
    tag.opacity = 0.2
}
View v {
    Rectangle bar width=100, height=20, halign=left, valign=top, color=#FFFFFFFF
    Rectangle tag width=20, height=20, halign=right, valign=top, color=#FFFFFFFF
}
";
    for (src, want_h, want_a) in [(screen_last, 30.0, 0.5), (arrangement_last, 52.0, 0.2)] {
        let c = ok(src);
        let page = c.page;
        let bar = page.find("bar").unwrap();
        let tag = page.find("tag").unwrap();
        let mut rt = Runtime::new(&page);
        rt.set_screen(&page, Orientation::Landscape, ScreenClass::LotsOfSpace);
        rt.go_id(gogo_ui::ArrangementId::new(0));
        let f = lay(&page, &rt);
        let (res, _) = (f.look(), f.rects());
        assert_eq!(res.placement[bar.index()].height, Len::Exact(want_h), "{:?}", res.placement[bar.index()]);
        assert!(near(opacity(&page, &res, tag), want_a), "opacity {}", opacity(&page, &res, tag));
        // Only one of the two holds → that one poses, order irrelevant.
        rt.go_id(BASE);
        let f = lay(&page, &rt);
        let (res, _) = (f.look(), f.rects());
        assert_eq!(res.placement[bar.index()].height, Len::Exact(30.0));
    }
}

/// `orient.clear-restores` (7266): Godot clears the orientation to "" and
/// the bar is 20 again. Rust has no empty orientation (enums, screen
/// ruler decision 2026-08-30): the same observable is portrait with a
/// class the file never poses — base.
#[test]
fn portrait_with_an_unposed_class_is_base() {
    let c = ok(ORIENT_VIEW);
    let page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_screen(&page, Orientation::Landscape, ScreenClass::LotsOfSpace);
    rt.set_screen(&page, Orientation::Portrait, ScreenClass::PortraitMedium);
    let f = lay(&page, &rt);
    let (_, r) = (f.look(), f.rects());
    assert!(near(r[bar.index()].h, 20.0), "{:?}", r[bar.index()]);
}

/// GPAR27 (round 6, 2026-08-31): `go` with an arrangement the page never
/// declared KEEPS the pose (the addon's printerr + early return). The
/// typed door (`Runtime::go(Arrangement::Wide)` on the emitted enum)
/// makes the misspelling a rustc error instead; `go_id` is the
/// runtime-compiled page's door and carries the guard.
#[test]
fn go_with_an_unknown_arrangement_keeps_the_pose() {
    let c = ok("arrangement wide\nView v { Panel p width=10, height=10 }\nwhen wide { p.width = 20 }\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.go_id(gogo_ui::ArrangementId::new(0));
    assert_eq!(rt.arrangement, gogo_ui::ArrangementId::new(0), "the declared arrangement lands");
    rt.go_id(gogo_ui::ArrangementId::new(7));
    assert_eq!(rt.arrangement, gogo_ui::ArrangementId::new(0), "an unknown index is refused and the pose keeps");
    rt.go_id(gogo_ui::runtime::BASE);
    assert_eq!(rt.arrangement, gogo_ui::runtime::BASE, "base is always reachable");
}
