//! Port of the Godot gogo suite's motion groups
//! (FantasiaGodot/addons/gogo/test/run_tests.gd):
//! `_easing_tests` 5041-5063, `_anim_tests` 5063-5418, `_motion_tests`
//! 5418-5641, `_apply_style_tests` 5641-5705. Every Godot snippet is
//! inlined here in the Rust dialect (`Control` → `Layers`, px numbers →
//! epx: Godot's tests doubled epx to px with an `epx2` provider, Rust
//! keeps epx only, so 100 px there is 50 epx here).
//!
//! ENGINE — skipped, Godot machinery with no Rust counterpart:
//! - `offset_transform_enabled` arming/disarming (anim.play-armed,
//!   anim.play-holds-armed, motion.armed, motion.when-armed,
//!   motion.when-held-stays-armed): Godot Control transform flag (D10).
//! - `GogoVisFx` helper node presence (anim.visfx-attached,
//!   visfx.root-attached): script helper node in the scene tree.
//! - `rebuild_into` reload/retrack (visfx.root-rebuild,
//!   visfx.root-exactly-one-after-reload): scene-tree reload machinery.
//! - `apply_style` on CODE-BUILT Controls (applystyle.ok, .meta, .states,
//!   .epx-bindings, .anim-refused, .refused-untouched, .class-refused,
//!   .facade): engine nodes built by script; Rust has no code-built
//!   controls (names become generated code). The styles themselves are
//!   pinned below on DECLARED nodes.
//! - Dialect (decided 2026-08-31, E1): the fixtures' `modulate.a` lanes are
//!   spelled `opacity` — `modulate.a` is a Godot engine word and G0075 in
//!   Rust; the anim writes the same opacity a declaration or `when` writes.
use gogo::anim::{Easing, Finish, Mode, Motion};
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Action, AnimTrack, NO_STR, PropKey, Rgba, StrId, Value};
use gogo::runtime::{HELD, Runtime, resolve, resolve_animated};
use gogo::{Compiled, Page, compile};

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

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{} {}\n", d.code, d.message)).collect()
}

fn anim_id(page: &Page, name: &str) -> StrId {
    let id = page.intern_lookup(name);
    assert_ne!(id, NO_STR, "no anim named {name}");
    id
}

fn tracks<'p>(page: &'p Page, name: &str) -> Vec<&'p AnimTrack> {
    let id = anim_id(page, name);
    page.anims.iter().filter(|t| t.anim == id).collect()
}

fn near(got: f32, want: f32, tol: f32) -> bool {
    (got - want).abs() <= tol
}

// ------------------------------------------------------------ _easing_tests

/// `_easing_tests` 5041-5063: the WPF reference numbers (shared with
/// Fantasia.Core.Tests), a bare base name is ease-OUT, an unknown name
/// parses to nothing, and every curve starts at 0 and ends at 1.
#[test]
fn easing_reference_numbers_bare_name_is_out_unknown_is_none() {
    let ease = |name: &str, t: f32| Easing::parse(name).unwrap_or_else(|| panic!("no easing {name}")).apply(t);
    for (name, t, want) in [
        ("cubic_out", 0.25, 0.578125),
        ("cubic_out", 0.5, 0.875),
        ("cubic_in_out", 0.25, 0.0625),
        ("quad_out", 0.25, 0.4375),
        ("sine_out", 0.5, 0.5f32.sqrt()),
        ("expo_out", 0.5, 1.0 - 1.0 / (1f32.exp() + 1.0)),
        ("bounce_out", 0.5, 0.53125),
        ("bounce_out", 0.9, 0.9525),
        ("bounce_in(1,3)", 0.2, 1.0 / 3.0),
    ] {
        let got = ease(name, t);
        assert!(near(got, want, 1e-5), "{name}({t}) = {got}, want {want}");
    }
    assert_eq!(Easing::parse("cubic").unwrap().mode, Mode::Out, "bare base name = ease-out");
    assert!(Easing::parse("zoomies").is_none());
    for base in ["linear", "sine", "quad", "cubic", "quart", "quint", "circle", "expo", "power", "back", "bounce", "elastic"] {
        let name = format!("{base}_out");
        assert!(near(ease(&name, 0.0), 0.0, 1e-5), "{name}(0) = {}", ease(&name, 0.0));
        assert!(near(ease(&name, 1.0), 1.0, 1e-5), "{name}(1) = {}", ease(&name, 1.0));
    }
}

// -------------------------------------------------------------- _anim_tests

/// `_anim_tests` 5064-5073 (anim.parse, anim.parse-path-type,
/// anim.parse-call): `listBrush.translate.x` reads as one path — node
/// `listBrush`, channel `translate.x` — and `on:show=play(appear)` wires a
/// Play action naming the anim.
#[test]
fn dotted_path_is_node_plus_channel_and_play_call_wires() {
    let c = ok("anim appear {\n listBrush.translate.x from=-60, to=0, duration=0.5, easing=cubic_out\n}\nView v { Panel p on:show=play(appear) { Rectangle listBrush } }\n");
    let page = &c.page;
    let t = tracks(page, "appear");
    assert_eq!(t.len(), 1, "{t:#?}");
    assert_eq!(t[0].node, page.find("listBrush").unwrap());
    assert_eq!(page.str(t[0].channel), "translate.x");
    let p = &page.nodes[page.find("p").unwrap() as usize];
    let show = page.intern_lookup("show");
    let handlers: Vec<&gogo::page::OnHandler> = page.handlers[p.handlers.0 as usize..(p.handlers.0 + p.handlers.1) as usize].iter().collect();
    let play = handlers.iter().find(|h| h.event == show).map(|h| h.action);
    assert!(matches!(play, Some(Action::Play(id)) if page.str(id) == "appear"), "{handlers:#?}");
}

/// `_anim_tests` 5077-5096 (anim.line-duration-ok, anim.line-duration-
/// override, anim.line-duration-solo): a line's `duration=` beats its
/// group's `duration_each=`; a lone line with its own duration builds.
#[test]
fn line_duration_overrides_the_groups_duration_each() {
    let c = ok("anim appear {\n group duration_each=0.2 {\n  box.translate.x from=-60, to=0, duration=0.5, easing=cubic_out\n  and box.opacity from=0, to=1\n }\n}\nView v { Rectangle box }\n");
    let t = tracks(&c.page, "appear");
    assert_eq!(t.len(), 2, "{t:#?}");
    assert_eq!(t[0].duration, 0.5, "{t:#?}");
    assert_eq!(t[1].duration, 0.2, "{t:#?}");
    ok("anim a { box.opacity to=0, duration=0.3 }\nView v { Rectangle box }");
}

/// `_anim_tests` 5098 (anim.err-line-delay): `delay=` on an anim LINE
/// was retired (a pause is its own `then delay=` step) — the build refuses
/// it. Godot's message says "retired".
#[test]
#[ignore = "GP41: not ported: anim step validation — a line-level delay= is silently ignored"]
fn line_level_delay_is_refused() {
    assert!(fails("anim a { box.opacity to=0, duration=0.2, delay=0.1 }\nView v { Rectangle box }"), "{}", messages("anim a { box.opacity to=0, duration=0.2, delay=0.1 }\nView v { Rectangle box }"));
}

/// `_anim_tests` 5099-5101 (anim.err-duration-with-keys): a keyed line
/// owns its timeline — `duration=` beside `key` children is refused.
#[test]
#[ignore = "GP41: not ported: anim step validation — duration= beside key children is silently ignored"]
fn duration_beside_keys_is_refused() {
    let src = "anim a { box.opacity duration=0.4 { key t=0.4, v=0 } }\nView v { Rectangle box }";
    assert!(fails(src), "{}", messages(src));
}

/// `_anim_tests` 5104-5105 (anim.err-assign-flat): `box.opacity = 0`
/// inside an anim is an assignment, not a journey — refused with the
/// `to=` spelling.
#[test]
#[ignore = "GP41: not ported: anim step validation — an `=` assignment inside an anim is silently skipped"]
fn assignment_inside_anim_is_refused() {
    let src = "anim a { box.opacity = 0 }\nView v { Rectangle box }";
    assert!(fails(src), "{}", messages(src));
}

/// `_anim_tests` 5106-5119 (anim.err-assign-each, anim.err-assign-group):
/// the same assignment inside `each` / `each { group }` is refused.
#[test]
#[ignore = "GP40: not ported: `each` steps (G0062 warning, skipped)"]
fn assignment_inside_each_is_refused() {
    let a = "anim a {\n each box { opacity = 0 }\n}\nView v { Rectangle box }\n";
    assert!(fails(a), "{}", messages(a));
    let b = "anim a {\n each box {\n  group duration_each=0.2 { opacity = 0 }\n }\n}\nView v { Rectangle box }\n";
    assert!(fails(b), "{}", messages(b));
}

/// `_anim_tests` 5122-5125 (anim.err-rotation-word, anim.err-rotation-
/// self): the engine's `rotation` word is refused and the message names
/// gogo's `rotate` (Rust: G0063 listing the channels).
#[test]
fn rotation_word_is_refused_naming_rotate() {
    for src in ["anim a { box.rotation to=90, duration=0.2 }\nView v { Rectangle box }", "anim a { self.rotation to=90, duration=0.2 }\nView v { Rectangle box }"] {
        assert!(codes(src).contains(&"G0063".to_string()), "{}", messages(src));
        assert!(messages(src).contains("rotate"), "{}", messages(src));
    }
}

/// `_anim_tests` 5128-5144 (anim.group-line-duration, anim.after-waits-
/// longest-lane): `group second after=main` starts when the anchor
/// group's LONGEST lane ends (0.6 s here, a per-line duration included).
#[test]
#[ignore = "GP40: not ported: `after=` (G0062 error)"]
fn after_waits_for_the_anchor_groups_longest_lane() {
    let src = "anim a {\n group main duration_each=0.2 {\n  box.opacity to=0\n  and box.translate.x to=10, duration=0.6\n }\n group second after=main { box.scale to=1, duration=0.1 }\n}\nView v { Rectangle box }\n";
    let c = ok(src);
    let t = tracks(&c.page, "a");
    let scale = t.iter().find(|t| c.page.str(t.channel) == "scale").expect("scale track");
    assert!(near(scale.start, 0.6, 1e-5), "{t:#?}");
}

/// `_anim_tests` 5146-5169 (anim.bake-ok, anim.bake-one-each, anim.bake-
/// epx-length, anim.bake-key-easing, anim.bake-pinned-from, anim.bake-
/// keyframes): one target, two lanes; the translate lane pins its
/// `from=-60`, runs the header's 0.5 s, carries the cubic-out easing; the
/// keyed rotate lane pins 14 at t=0 (a jump) and glides to 0 by 0.8.
#[test]
fn baked_tracks_carry_from_duration_easing_and_keys() {
    let c = ok("anim appear duration_each=0.5 {\n box.translate.x from=-60, to=0, easing=cubic_out\n and box.rotate {\n  key t=0, v=14, jump=true\n  key t=0.8, v=0, easing=cubic_out\n }\n}\nView v { Rectangle box }\n");
    let page = &c.page;
    let t = tracks(page, "appear");
    let box_id = page.find("box").unwrap();
    assert!(t.iter().all(|t| t.node == box_id), "{t:#?}");
    let channels: Vec<&str> = t.iter().map(|t| page.str(t.channel)).collect();
    assert_eq!(channels, ["translate.x", "rotate", "rotate"], "{t:#?}");
    let tx = t[0];
    assert_eq!(tx.from, Some(-60.0), "pinned from");
    assert_eq!(tx.duration, 0.5);
    assert_eq!(page.str(tx.easing), "cubic_out");
    // The keyed lane: key 1 pins 14 at once, key 2 glides 0..0.8 to 0.
    assert_eq!((t[1].from, t[1].to, t[1].start, t[1].duration), (Some(14.0), 14.0, 0.0, 0.0), "{t:#?}");
    assert_eq!((t[2].from, t[2].to, t[2].start, t[2].duration), (None, 0.0, 0.0, 0.8), "{t:#?}");
    assert_eq!(page.str(t[2].easing), "cubic_out");
}

/// `_anim_tests` 5172-5175 (anim.err-target, anim.err-easing): a path to
/// no named node is G0060; an unknown easing word is G0065.
#[test]
fn unknown_target_and_unknown_easing_are_refused() {
    assert!(codes("anim a { nobody.translate.x to=0, duration=0.1 }\nView v { Rectangle box }").contains(&"G0060".to_string()));
    assert!(codes("anim a { box.translate.x to=0, duration=0.1, easing=zoomies }\nView v { Rectangle box }").contains(&"G0065".to_string()));
}

/// `_anim_tests` 5176-5177 (anim.err-to-with-keys): `to=` beside `key`
/// children is refused — the keys own the values.
#[test]
#[ignore = "GP41: not ported: anim step validation — to= beside key children is silently ignored"]
fn to_beside_keys_is_refused() {
    let src = "anim a { box.translate.x to=5 { key t=0, v=1 } }\nView v { Rectangle box }";
    assert!(fails(src), "{}", messages(src));
}

/// `_anim_tests` 5178-5179 (anim.err-no-duration): a line with no
/// `duration=` and no `duration_each=` in scope is refused.
#[test]
#[ignore = "GP41: not ported: anim step validation — no duration in scope builds a 0 s track"]
fn no_duration_in_scope_is_refused() {
    let src = "anim a { box.translate.x to=5 }\nView v { Rectangle box }";
    assert!(fails(src), "{}", messages(src));
}

/// `_anim_tests` 5180-5181 (anim.err-dotted-outside): a dotted anim line
/// outside any `anim` block is refused.
#[test]
fn dotted_line_outside_an_anim_is_refused() {
    assert!(fails("View v { Rectangle box }\nbox.translate.x to=5"), "{}", messages("View v { Rectangle box }\nbox.translate.x to=5"));
}

/// `_anim_tests` 5184-5219 (anim.play-build, anim.play-spawned, anim.play-
/// armed*, anim.play-midpoint, anim.play-running, anim.play-end, anim.play-
/// finished, anim.play-holds-armed*, anim.stop-restores; * = ENGINE): a
/// 0→100 translate is at 50 after half a second, 100 and finished after
/// the full second and HOLDS there; a second play stopped with restore
/// goes back to where the first play left the channel (100).
#[test]
fn play_runs_to_the_end_and_stop_restores_the_pre_play_value() {
    let c = ok("anim slide {\n box.translate.x from=0, to=100, duration=1.0\n}\nView host { Rectangle box }\n");
    let mut page = c.page;
    let b = page.find("box").unwrap() as usize;
    let slide = anim_id(&page, "slide");
    let mut rt = Runtime::new(&page);
    rt.play(&page, slide, Finish::Nothing);
    assert!(rt.is_playing(slide));
    rt.tick(&mut page, 0.5);
    assert!(near(rt.motion[b].translate[0], 50.0, 0.01), "{:?}", rt.motion[b]);
    assert!(rt.is_playing(slide));
    rt.tick(&mut page, 0.6);
    assert!(near(rt.motion[b].translate[0], 100.0, 0.01), "{:?}", rt.motion[b]);
    assert!(!rt.is_playing(slide));
    // stop(restore) on a second play: back to where play #1 left it.
    rt.play(&page, slide, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    rt.stop(slide, true);
    assert!(near(rt.motion[b].translate[0], 100.0, 0.01), "{:?}", rt.motion[b]);
}

/// `_anim_tests` 5227-5280 (animc.build, animc.bake-current-key,
/// animc.spawned, animc.mid, animc.handoff-spawned, animc.pins-live-value,
/// animc.continues-from-live, animc.takeover-strips-old, animc.restore-
/// true-base): `from=current` pins the LIVE value at play start; a new
/// run takes over the channel another player is moving — the old run
/// keeps its clock but writes nothing — and the new run's stop(restore)
/// lands on the TRUE base (0), not the 50 it read at its own start.
#[test]
fn from_current_pins_the_live_value_and_handoff_keeps_the_true_base() {
    let c = ok("anim go {\n hbox.translate.x from=0, to=100, duration=1.0\n}\nanim back {\n hbox.translate.x from=current, to=0, duration=1.0\n}\nView hhost { Rectangle hbox }\n");
    let mut page = c.page;
    let h = page.find("hbox").unwrap() as usize;
    let back_tracks = tracks(&page, "back");
    assert!(back_tracks[0].current && back_tracks[0].from.is_none(), "{back_tracks:#?}");
    let go = anim_id(&page, "go");
    let back = anim_id(&page, "back");
    let mut rt = Runtime::new(&page);
    rt.play(&page, go, Finish::Nothing);
    assert!(rt.is_playing(go));
    rt.tick(&mut page, 0.5);
    assert!(near(rt.motion[h].translate[0], 50.0, 0.01), "{:?}", rt.motion[h]);
    rt.play(&page, back, Finish::Nothing);
    assert!(rt.is_playing(back));
    rt.tick(&mut page, 0.0);
    assert!(near(rt.motion[h].translate[0], 50.0, 0.01), "pins the live value: {:?}", rt.motion[h]);
    // The takeover strips the old run's channel; its clock lives on (Godot
    // ticked the players one by one, Rust ticks them all — so this is
    // checked before the next half second runs `go`'s clock out).
    let old = rt.players.iter().find(|p| p.anim == go).expect("the old run keeps its clock");
    assert!(old.owns.is_empty() && rt.is_playing(go), "owns={:?}", old.owns);
    rt.tick(&mut page, 0.5);
    assert!(near(rt.motion[h].translate[0], 25.0, 0.01), "continues from the live value: {:?}", rt.motion[h]);
    rt.stop(back, true);
    assert!(near(rt.motion[h].translate[0], 0.0, 0.001), "restores the true base: {:?}", rt.motion[h]);
}

/// `_anim_tests` 5284-5288 (animc.err-to-current): `current` is a
/// `from=`-only word — `to=current` is refused (Rust: G0064, `to=` needs a
/// number).
#[test]
fn to_current_is_refused() {
    let src = "anim a { p.opacity to=current, duration=0.1 }\nPanel p";
    assert!(fails(src), "{}", messages(src));
}

/// `_anim_tests` 5289-5290 (animc.err-shift-retired): the retired
/// `current±N` arithmetic does not build (Rust: G0017, `-60` is not a
/// member of the line).
#[test]
fn current_arithmetic_is_refused() {
    let src = "anim a { p.opacity from=current-60, to=1, duration=0.1 }\nPanel p";
    assert!(fails(src), "{}", messages(src));
}

/// `_anim_tests` 5301-5355 (animd.build, animd.bakes-two-lanes,
/// animd.spawned, animd.up-mid, animd.top, animd.down-mid-glides,
/// animd.lands, animd.finished, animd.loop-spawned, animd.loop-wrap-glides,
/// animd.loop-running): a `then` leg with `to=` alone reads the live value
/// when it STARTS, so the down leg glides from -20 instead of snapping
/// from the setup 0; a `repeat=forever` copy replays the same values on
/// its second lap. expo(2) at half time = 0.268941; -20 epx × 0.731 = -14.62.
#[test]
fn delayed_open_start_reads_the_live_value_and_a_loop_replays_it() {
    let c = ok("anim bob {\n dbox.translate.y to=-20, duration=0.5, easing=expo_out\n then dbox.translate.y to=0, duration=0.5, easing=expo_in\n}\nanim bobLoop repeat=forever {\n dbox.translate.y to=-20, duration=0.5, easing=expo_out\n then dbox.translate.y to=0, duration=0.5, easing=expo_in\n}\nView dhost { Rectangle dbox }\n");
    let mut page = c.page;
    let d = page.find("dbox").unwrap() as usize;
    let lanes = tracks(&page, "bob");
    assert_eq!(lanes.len(), 2, "{lanes:#?}");
    assert!(near(lanes[0].start, 0.0, 1e-6) && near(lanes[1].start, 0.5, 1e-6), "{lanes:#?}");
    let bob = anim_id(&page, "bob");
    let mut rt = Runtime::new(&page);
    rt.play(&page, bob, Finish::Nothing);
    assert!(rt.is_playing(bob));
    rt.tick(&mut page, 0.25);
    assert!(near(rt.motion[d].translate[1], -14.62115, 0.005), "up-mid: {:?}", rt.motion[d]);
    rt.tick(&mut page, 0.25);
    assert!(near(rt.motion[d].translate[1], -20.0, 0.005), "top: {:?}", rt.motion[d]);
    rt.tick(&mut page, 0.25);
    assert!(near(rt.motion[d].translate[1], -14.62115, 0.005), "down-mid must glide from -20, not snap from 0: {:?}", rt.motion[d]);
    rt.tick(&mut page, 0.3);
    assert!(near(rt.motion[d].translate[1], 0.0, 0.001), "lands: {:?}", rt.motion[d]);
    assert!(!rt.is_playing(bob));

    let bob_loop = anim_id(&page, "bobLoop");
    rt.play(&page, bob_loop, Finish::Nothing);
    assert!(rt.is_playing(bob_loop));
    for _ in 0..7 {
        rt.tick(&mut page, 0.25); // lands at t=1.75: second lap, mid-down
    }
    assert!(near(rt.motion[d].translate[1], -14.62115, 0.005), "loop wrap glides: {:?}", rt.motion[d]);
    assert!(rt.is_playing(bob_loop));
    rt.stop(bob_loop, false);
}

/// `_anim_tests` 5358-5366 (anim.visfx-ok, anim.visfx-attached): a node
/// with `on:show=play(appear), on:hide=play(appear)` builds, and its hide
/// wire WORKS — `show(false)` starts the anim and the node goes only when
/// the anim finishes. (Godot pins the helper node's presence; Rust has no
/// helper node, so the wire is pinned by what it does.)
#[test]
fn show_and_hide_wires_on_a_panel_build_and_run() {
    let c = ok("anim appear { panel.opacity from=0, to=1, duration=0.2 }\nView v { Panel panel on:show=play(appear), on:hide=play(appear) }\n");
    let mut page = c.page;
    let panel = page.find("panel").unwrap();
    let appear = anim_id(&page, "appear");
    let mut rt = Runtime::new(&page);
    rt.show(&mut page, panel, false);
    assert!(rt.is_playing(appear), "the hide wire plays the anim first");
    assert!(!resolve(&page, &rt).gone(&page, panel), "still shown while the anim runs");
    rt.tick(&mut page, 0.25);
    assert!(resolve(&page, &rt).gone(&page, panel), "gone when the anim finishes");
}

/// `_anim_tests` 5367-5368 (anim.err-on-visible): `on:visible=` is not a
/// gogo event — refused (Rust G0049: raw signals are gone; the events
/// are show/hide).
#[test]
fn on_visible_is_refused() {
    let src = "anim a { p.opacity to=1, duration=0.1 }\nPanel p on:visible=play(a)";
    assert!(codes(src).contains(&"G0049".to_string()), "{}", messages(src));
}

/// `_anim_tests` 5369-5370 (anim.err-unknown-play): `play(nope)` naming
/// no anim is a build error.
#[test]
#[ignore = "NEW: Godot refuses `on:show=play(nope)` (unknown anim) vs Rust builds it clean and the play is a silent no-op"]
fn play_of_an_unknown_anim_is_refused() {
    let src = "View v { Panel p on:show=play(nope) }";
    assert!(fails(src), "{}", messages(src));
}

/// `_anim_tests` 5374-5394 (visfx.root-build, visfx.root-attached*,
/// visfx.root-rebuild*, visfx.root-exactly-one-after-reload*, visfx.root-
/// anims-baked; * = ENGINE) — fixture anim_view.gogo: `on:show`/`on:hide`
/// on the VIEW ROOT itself builds, its anims are baked, and the root's
/// own show wire plays when the root is shown.
#[test]
fn show_hide_on_the_view_root_builds_and_plays() {
    let c = ok("anim fadeIn duration_each=0.2, easing=cubic_out {\n viewRoot.opacity from=0, to=1\n}\n\nanim fadeOut duration_each=0.15 {\n viewRoot.opacity to=0\n}\n\nView viewRoot visibility=gone, on:show=play(fadeIn), on:hide=play(fadeOut) {\n Column {\n  Label title text=\"hi\"\n }\n}\n");
    let mut page = c.page;
    let fade_in = anim_id(&page, "fadeIn");
    assert!(page.anim_heads.iter().any(|h| h.anim == fade_in), "{:#?}", page.anim_heads);
    assert_eq!(tracks(&page, "fadeIn").len(), 1);
    assert_eq!(tracks(&page, "fadeOut").len(), 1);
    let root = page.find("viewRoot").unwrap();
    let mut rt = Runtime::new(&page);
    assert!(resolve(&page, &rt).gone(&page, root));
    rt.show(&mut page, root, true);
    assert!(rt.is_playing(fade_in));
    assert!(!resolve(&page, &rt).gone(&page, root));
    assert_eq!(rt.motion[root as usize].opacity, 0.0, "pinned from=0 at play start");
}

/// `_anim_tests` 5403-5418 (anim.public-build, anim.public-play,
/// anim.public-is-playing, anim.public-mid-flight, anim.public-stop-ends,
/// anim.public-stop-restores) — fixture play_stop.gogo: the public
/// play/is_playing/stop path reaches the running player: mid-flight the
/// box has moved, stop ends the play and restores the base (0).
#[test]
fn public_play_is_playing_and_stop_reach_the_player() {
    let c = ok("anim slide {\n box.translate.x from=0, to=100, duration=1.0\n}\n\nView playRoot {\n Rectangle box\n}\n");
    let mut page = c.page;
    let b = page.find("box").unwrap() as usize;
    let slide = anim_id(&page, "slide");
    let mut rt = Runtime::new(&page);
    rt.play(&page, slide, Finish::Nothing);
    assert!(rt.is_playing(slide));
    rt.tick(&mut page, 0.5);
    assert!(rt.motion[b].translate[0] > 0.0, "{:?}", rt.motion[b]);
    rt.stop(slide, true);
    assert!(!rt.is_playing(slide));
    assert!(near(rt.motion[b].translate[0], 0.0, 0.001), "{:?}", rt.motion[b]);
}

/// `_anim_tests` 5408-5413 (anim.public-restart-replaces, anim.public-
/// replaced-stopped) FLIPPED per GPAR52 DONE ("tracks move, players
/// stay"): Godot's restart STOPS the first player; Rust's second play
/// takes the channel over and the first player keeps its clock (so a
/// pending finish still lands on time) but owns nothing — only ONE writer
/// is left, and the restart starts over from the pinned `from`.
#[test]
fn restart_hands_the_channel_to_the_new_player_and_the_old_clock_lives_on() {
    let c = ok("anim slide {\n box.translate.x from=0, to=100, duration=1.0\n}\n\nView playRoot {\n Rectangle box\n}\n");
    let mut page = c.page;
    let b = page.find("box").unwrap() as usize;
    let slide = anim_id(&page, "slide");
    let mut rt = Runtime::new(&page);
    rt.play(&page, slide, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    rt.play(&page, slide, Finish::Nothing);
    let writers: Vec<usize> = rt.players.iter().filter(|p| p.anim == slide && !p.owns.is_empty()).map(|p| p.owns.len()).collect();
    assert_eq!(writers, vec![1], "exactly one player writes the channel: {:#?}", rt.players);
    assert_eq!(rt.players.iter().filter(|p| p.anim == slide).count(), 2, "the replaced player keeps its clock (GPAR52)");
    assert!(near(rt.motion[b].translate[0], 0.0, 0.001), "the restart re-pins from=0: {:?}", rt.motion[b]);
    rt.tick(&mut page, 0.5);
    assert!(near(rt.motion[b].translate[0], 50.0, 0.01), "{:?}", rt.motion[b]);
    rt.tick(&mut page, 0.6);
    assert!(!rt.is_playing(slide), "both clocks have run out");
}

// ------------------------------------------------------------ _motion_tests

/// `_motion_tests` 5421-5445 (motion.build, motion.armed*, motion.scale-
/// mid, motion.rotate-mid-deg, motion.opacity-mid, motion.identity-
/// disarms; * = ENGINE): keyed scale 0.9→1 and rotate 10→0 with a 0.5→1
/// opacity, all 0.2 s: halfway they read 0.95 / 5° / 0.75; at the end the
/// node is back at identity (scale 1, rotate 0).
#[test]
fn pop_reads_halfway_values_and_ends_at_identity() {
    let c = ok("anim pop {\n box.scale { key t=0, v=0.9; key t=0.2, v=1.0 }\n and box.rotate { key t=0, v=10; key t=0.2, v=0 }\n and box.opacity from=0.5, to=1.0, duration=0.2\n}\nView host { Rectangle box }\n");
    let mut page = c.page;
    let b = page.find("box").unwrap() as usize;
    let pop = anim_id(&page, "pop");
    let mut rt = Runtime::new(&page);
    rt.play(&page, pop, Finish::Nothing);
    rt.tick(&mut page, 0.1);
    assert!(near(rt.motion[b].scale[0], 0.95, 0.001), "{:?}", rt.motion[b]);
    assert!(near(rt.motion[b].rotate, 5.0, 0.001), "{:?}", rt.motion[b]);
    assert!(near(rt.motion[b].opacity, 0.75, 0.001), "{:?}", rt.motion[b]);
    rt.tick(&mut page, 0.2);
    assert!(!rt.is_playing(pop));
    assert_eq!(rt.motion[b].scale, [1.0, 1.0], "{:?}", rt.motion[b]);
    assert!(near(rt.motion[b].rotate, 0.0, 1e-6), "{:?}", rt.motion[b]);
}

/// `_motion_tests` 5449-5455 (motion.hidden-build) and 5470-5476
/// (motion.speed-build): the fixtures behind the two ignored tests below
/// build — pinned here so the build half of each Godot check still runs.
#[test]
fn hidden_and_speed_fixtures_build() {
    ok("anim off { box.translate.x to=40, duration=0.2 }\nView host { Rectangle box }\n");
    ok("anim slide { box.opacity from=0.0, to=1.0, duration=1.0 }\nanim spin repeat=forever { box.opacity { key t=0, v=0.25; key t=0.5, v=1.0 } }\nView host { Rectangle box }\n");
}

/// `_motion_tests` 5456-5466 (motion.hidden-resets):
/// a target hidden mid-anim is reset to identity when the anim ends even
/// though the end pose is not identity (the SlideOut pattern: hide first,
/// the release cleans up) — re-showing it later finds it at rest.
#[test]
#[ignore = "NEW: Godot resets a hidden target's motion to identity at anim end vs Rust leaves translate.x=40 on the hidden node (re-show finds it slid out)"]
fn hidden_target_is_reset_when_the_anim_ends() {
    let c = ok("anim off { box.translate.x to=40, duration=0.2 }\nView host { Rectangle box }\n");
    let mut page = c.page;
    let b = page.find("box").unwrap();
    let off = anim_id(&page, "off");
    let mut rt = Runtime::new(&page);
    rt.play(&page, off, Finish::Nothing);
    rt.tick(&mut page, 0.1);
    let hidden = page.intern("hidden");
    rt.set_prop(b, PropKey::Visibility, Value::VIdent(hidden));
    rt.tick(&mut page, 0.2);
    assert!(!rt.is_playing(off));
    assert_eq!(rt.motion[b as usize], Motion::IDENTITY, "{:?}", rt.motion[b as usize]);
}

/// `_motion_tests` 5470-5504 (motion.speed-build, motion.speed-2x,
/// motion.instant-completes, motion.instant-forever-holds-base): a GLOBAL
/// motion speed (`set_motion_speed`) multiplies time — ×2 reaches the
/// midpoint in a quarter second; infinite speed completes a one-shot in
/// one frame (final values, finished) while a `repeat=forever` anim holds
/// its base pose and keeps playing. The per-anim `speed=` header exists in
/// Rust (GP6); the runtime-wide knob does not.
#[test]
#[ignore = "GP42: not ported: motion speed / reduced motion (runtime-wide multiplier, INF = instant)"]
fn global_motion_speed_scales_time_and_infinite_completes_at_once() {
    let c = ok("anim slide { box.opacity from=0.0, to=1.0, duration=1.0 }\nanim spin repeat=forever { box.opacity { key t=0, v=0.25; key t=0.5, v=1.0 } }\nView host { Rectangle box }\n");
    let mut page = c.page;
    let b = page.find("box").unwrap() as usize;
    let slide = anim_id(&page, "slide");
    let spin = anim_id(&page, "spin");
    let mut rt = Runtime::new(&page);
    // No `Runtime::set_motion_speed` exists: with the per-anim speed at 1
    // the ×2 expectation below is what the knob would have to produce.
    rt.play(&page, slide, Finish::Nothing);
    rt.tick(&mut page, 0.25);
    assert!(near(rt.motion[b].opacity, 0.5, 0.001), "2x: {:?}", rt.motion[b]);
    rt.stop(slide, false);
    rt.play(&page, slide, Finish::Nothing);
    let alive = rt.tick(&mut page, 0.016);
    assert!(!alive && !rt.is_playing(slide) && near(rt.motion[b].opacity, 1.0, 1e-5), "instant completes: {:?}", rt.motion[b]);
    rt.play(&page, spin, Finish::Nothing);
    rt.tick(&mut page, 0.016);
    assert!(rt.is_playing(spin) && near(rt.motion[b].opacity, 0.25, 1e-5), "instant forever holds base: {:?}", rt.motion[b]);
    let _ = page.intern_lookup("spin");
}

/// `_motion_tests` 5507-5516 (motion.err-anim-position, motion.err-anim-
/// offset, motion.err-anim-raw-ot, motion.err-anim-self-geometry,
/// motion.err-translate-axis): the anim channel fence — layout words
/// (`position.x`, `offset_left`), raw offset-transform fields and a
/// `translate` with no axis are all refused (Rust: one G0063 listing the
/// channels).
#[test]
fn anim_channel_fence_refuses_layout_raw_and_axisless_words() {
    for path in ["box.position.x", "box.offset_left", "box.offset_transform_rotation", "self.position.x", "box.translate"] {
        let src = format!("anim a {{ {path} to=5, duration=0.1 }}\nView v {{ Rectangle box }}");
        assert!(codes(&src).contains(&"G0063".to_string()), "{path}: {}", messages(&src));
    }
}

/// `_motion_tests` 5517 (motion.err-static-scale): a motion word as a
/// STATIC prop (`Button b scale=2`) is refused — motion lives in `when`
/// and `anim` only.
#[test]
#[ignore = "GP41: not ported: load-time lints — a static `scale=2` builds clean and is silently dead"]
fn static_motion_word_is_refused() {
    assert!(fails("View v { Button b scale=2 }"), "{}", messages("View v { Button b scale=2 }"));
}

/// `_motion_tests` 5518-5523 (motion.err-static-raw-ot, motion.err-when-
/// raw-ot, motion.err-when-child-raw-ot, motion.err-when-translate-axis):
/// raw offset-transform fields and an axis-less `translate=` are refused
/// wherever they appear — static, in a `when`, on a child path. (Rust
/// today: G2001 warning for the first, second and fourth; the child path
/// `flash.offset_transform_rotation` gets no diagnostic at all.)
#[test]
fn raw_and_axisless_motion_keys_are_refused_everywhere() {
    for src in [
        "View v { Button b offset_transform_enabled=true }",
        "style S { when held offset_transform_rotation=1 }\nView v { Button b .S }",
        "style S { when held flash.offset_transform_rotation=1 }\nView v { Button b .S {\n Rectangle flash\n} }",
        "style S { when held translate=5 }\nView v { Button b .S }",
    ] {
        assert!(fails(src), "{src}\n{}", messages(src));
    }
}

/// `_motion_tests` 5524-5525 (motion.err-when-rotate-component):
/// `rotate.x=5` in a `when` is refused — rotate has no component. Rust
/// refuses it too, though it reads `rotate.` as a child path (G0060 "no
/// node named `rotate`") rather than naming the component.
#[test]
fn rotate_component_is_refused() {
    let src = "style S { when held rotate.x=5 }\nView v { Button b .S }";
    assert!(fails(src), "{}", messages(src));
}

/// `_motion_tests` 5526-5527 (motion.err-when-scale-type): `scale=red`
/// in a `when` is refused — a motion word takes a number.
#[test]
fn scale_of_a_non_number_is_refused() {
    let src = "style S { when held scale=red }\nView v { Button b .S }";
    assert!(fails(src), "{}", messages(src));
}

/// `_motion_tests` 5530-5538 (motion.err-when-anim-two-writers) and
/// 5623-5631 (motion.err-arrangement-anim-two-writers): D63 one-writer
/// guard — a condition HOLDING a motion word while a transition `anim=`
/// DRIVES the same property is refused at build (G4003), at node and at
/// view scope. PORT — G4003 shipped 2026-08-31 with E1 (one property, one
/// writer; `Compiler::one_writer_guard`).
#[test]
fn one_writer_per_property_at_node_and_view_scope() {
    let node = "anim press { self.scale to=0.5, duration=0.1 }\nstyle S {\n transition any -> held duration=0.1, anim=press\n when held scale=0.5\n}\nView v { Button b .S }\n";
    assert!(fails(node), "{}", messages(node));
    let view = "anim zoom { menu.scale to=0.9, duration=0.1 }\narrangement zoomed\nwhen zoomed { menu.scale=0.9 }\ntransition any -> zoomed duration=0.1, anim=zoom\nView v { Column menu { Button b text=\"m\" } }\n";
    assert!(fails(view), "{}", messages(view));
}

/// `_motion_tests` 5540-5548 (motion.when-anim-different-props-ok): an
/// anim popping `scale` beside a `when held rotate=` coexist — different
/// properties, no writer clash. (`self.` outside a widget is G0060 in Rust
/// — GP32 scopes `self` to a widget root — so the anim names the button.)
#[test]
fn anim_and_when_on_different_properties_coexist() {
    ok("anim press { b.scale to=0.9, duration=0.1 }\nstyle S {\n transition any -> held duration=0.1, anim=press\n when held rotate=3\n}\nView v { Button b .S }\n");
}

/// `_motion_tests` 5552-5585 (motion.when-build, motion.when-armed*,
/// motion.when-scale-mid, motion.when-scale-held, motion.when-held-stays-
/// armed*, motion.when-hand-back-releases, motion.when-quick-tap-disarms;
/// * = ENGINE): D63 — a motion word in a `when` is a standing response
/// timed by the transition rule: halfway through the 0.2 s rule the scale
/// reads 0.75, settled it holds 0.5, releasing brings it back to 1, and a
/// tap released before the first tick leaves no motion behind.
#[test]
fn when_held_scale_is_timed_by_the_transition_rule() {
    let c = ok("style Press width=100, height=48 {\n transition duration=0.2\n when held background=#FFFFFFFF, scale=0.5\n}\nView v { Button b .Press text=\"x\" }\n");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.set_state(b, HELD, true);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.1);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(r.motion[b as usize].scale[0], 0.75, 0.02), "mid: {:?}", r.motion[b as usize]);
    for _ in 0..20 {
        rt.tick(&mut page, 0.016);
    }
    let r = resolve_animated(&page, &mut rt);
    assert!(near(r.motion[b as usize].scale[1], 0.5, 0.001), "held: {:?}", r.motion[b as usize]);
    rt.set_state(b, HELD, false);
    resolve_animated(&page, &mut rt);
    for _ in 0..30 {
        rt.tick(&mut page, 0.016);
    }
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.motion[b as usize].scale, [1.0, 1.0], "released: {:?}", r.motion[b as usize]);
    // A tap released before the first tick: no motion parked behind.
    rt.set_state(b, HELD, true);
    rt.set_state(b, HELD, false);
    for _ in 0..30 {
        rt.tick(&mut page, 0.016);
    }
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.motion[b as usize], Motion::IDENTITY, "quick tap: {:?}", r.motion[b as usize]);
    assert!(!rt.animating());
}

/// `_motion_tests` 5589-5610 (motion.when-snap-build, motion.when-snap-
/// translate-epx, motion.when-snap-child, motion.when-snap-releases): with
/// no transition rule the response snaps — held: the button is 10 epx
/// over (Godot measured 20 px with its ×2 epx provider) and its child
/// `flash` is at half scale; released: both back at identity.
#[test]
fn when_held_motion_snaps_without_a_rule_and_reaches_a_child() {
    let c = ok("style Snap { when held translate.x=10, flash.scale=0.5 }\nView v { Button s .Snap text=\"y\" { Rectangle flash } }\n");
    let page = c.page;
    let s = page.find("s").unwrap();
    let flash = page.find("flash").unwrap() as usize;
    let mut rt = Runtime::new(&page);
    rt.set_state(s, HELD, true);
    let r = resolve(&page, &rt);
    assert!(near(r.motion[s as usize].translate[0], 10.0, 1e-6), "{:?}", r.motion[s as usize]);
    assert_eq!(r.motion[flash].scale, [0.5, 0.5], "{:?}", r.motion[flash]);
    rt.set_state(s, HELD, false);
    let r = resolve(&page, &rt);
    assert_eq!(r.motion[s as usize], Motion::IDENTITY, "{:?}", r.motion[s as usize]);
    assert_eq!(r.motion[flash], Motion::IDENTITY, "{:?}", r.motion[flash]);
}

/// `_motion_tests` 5614-5622 (motion.arrangement-holds-ok): a view-level
/// `when zoomed { menu.scale=0.9 }` builds and holds the scale while the
/// arrangement is active.
#[test]
fn an_arrangement_can_hold_motion() {
    let c = ok("arrangement zoomed\nwhen zoomed { menu.scale=0.9 }\ntransition any -> zoomed duration=0.1\nView v { Column menu { Button b text=\"m\" } }\n");
    let page = c.page;
    let menu = page.find("menu").unwrap() as usize;
    let mut rt = Runtime::new(&page);
    rt.go(0);
    let r = resolve(&page, &rt);
    assert_eq!(r.motion[menu].scale, [0.9, 0.9], "{:?}", r.motion[menu]);
}

// ------------------------------------------------------- _apply_style_tests

/// The apply_style.gogo fixture, minus its `Nudged` style (its anim says
/// `self.scale`, which outside a widget is G0060 in Rust — GP32).
const APPLY_STYLE_FIXTURE: &str = "style Flash background=#00000000 {\n when held background=#E51400FF\n}\n\nstyle Rounded background=#22222299, corner_radius=6\n\nstyle Reaching background=#00000000 {\n when held mark.opacity=0.5\n}\n\nstyle ButtonOnly for=Button, background=#00000000 {\n when held background=#FFFFFF99\n}\n";

/// `_apply_style_tests` 5652-5666 (applystyle.held-flash, applystyle.
/// release-restores) on a DECLARED Button: the `Flash` style's held
/// response paints #E51400 and the release restores the transparent base.
#[test]
fn flash_style_paints_on_held_and_restores_on_release() {
    let c = ok(&format!("{APPLY_STYLE_FIXTURE}\nView v {{ Button b .Flash text=\"x\" }}\n"));
    let page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_state(b, HELD, true);
    let r = resolve(&page, &rt);
    assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([0xE5, 0x14, 0x00, 0xFF])));
    rt.set_state(b, HELD, false);
    let r = resolve(&page, &rt);
    assert_eq!(r.color(&page, b, PropKey::Background).map(|c| c.0[3]), Some(0), "{:?}", r.color(&page, b, PropKey::Background));
}

/// `_apply_style_tests` 5670-5673 (applystyle.for-ok): a `for=Button`
/// style on a Button applies without complaint.
#[test]
#[ignore = "GP40: not ported: `for=` checked styles (`for` reaches the node as an unknown prop, G2001)"]
fn for_button_style_applies_to_a_button_cleanly() {
    let src = format!("{APPLY_STYLE_FIXTURE}\nView v {{ Button fb .ButtonOnly text=\"x\" }}\n");
    let c = compile(&src, &|_| None);
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

/// `_apply_style_tests` 5676-5680 (applystyle.epx-ok): the `Rounded`
/// style's epx-typed corner reaches the drawn quad (6 epx) with its
/// #222222 at alpha 0x99.
#[test]
fn rounded_style_reaches_the_drawn_quad() {
    let c = ok(&format!("{APPLY_STYLE_FIXTURE}\nView v {{ Button eb .Rounded text=\"x\" }}\n"));
    let page = c.page;
    let eb = page.find("eb").unwrap();
    let rt = Runtime::new(&page);
    let r = resolve(&page, &rt);
    let rects = layout(&page, &r, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let list = draw(&page, &r, &rects);
    let quad = list.iter().find_map(|d| match d {
        Draw::Quad { rect, color, corner, .. } if *rect == rects[eb as usize] => Some((*color, *corner)),
        _ => None,
    });
    assert_eq!(quad, Some((Rgba([0x22, 0x22, 0x22, 0x99]), 6.0)), "{list:#?}");
}

/// `_apply_style_tests` 5685-5686 (applystyle.unknown-refused) and
/// 5689-5690 (applystyle.reach-refused): a style name the document does
/// not define is refused (G5002), and a style whose response reaches a
/// node the control does not bring (`mark`) is refused (G0060).
#[test]
fn unknown_style_and_a_response_reaching_a_missing_node_are_refused() {
    let unknown = format!("{APPLY_STYLE_FIXTURE}\nView v {{ Button nb .Nope text=\"x\" }}\n");
    assert!(codes(&unknown).contains(&"G5002".to_string()), "{}", messages(&unknown));
    let reaching = format!("{APPLY_STYLE_FIXTURE}\nView v {{ Button nb .Reaching text=\"x\" }}\n");
    assert!(codes(&reaching).contains(&"G0060".to_string()), "{}", messages(&reaching));
}

