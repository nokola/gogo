//! GP40 color/non-motion anim lanes: any word a `transition` rule can
//! move, `play(anim)` can drive too — one list, two doors. Colors mix
//! under the one law (premultiplied Oklab, `gogo::color`); `.a` on a
//! color word moves alpha alone while the hue keeps following its
//! ladder; and one ownership law covers motion and prop lanes alike
//! (a playing anim owns its lane, newer players take over, `stop`
//! restores, a finished anim holds its end pose).

use gogo::Compiled;
use gogo::anim::Finish;
use gogo::color::mix_rgba;
use gogo::compile;
use gogo::page::{PropKey, Color_Straight_Srgb_4xU8_RGBA, StrId, Value};
use gogo::runtime::{HELD, Runtime, resolve, resolve_animated};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn fails(src: &str) -> bool {
    gogo::has_errors(&compile(src, &|_| None).diags)
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{}: {}\n", d.code, d.message)).collect()
}

fn anim_id(page: &gogo::Page, name: &str) -> StrId {
    page.intern_lookup(name)
}

/// A full `background` lane: the midpoint sits on the one mixing law's
/// line (not the byte midpoint), the finished anim HOLDS its end pose,
/// and a later code write yields to the held pose — one property, one
/// writer, the anim on top.
#[test]
fn a_background_lane_plays_holds_and_outranks_writes() {
    let c = ok("anim warn { b.background to=#FF0000FF, duration=1, easing=linear }\nView v { Panel b background=#000000FF }");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let warn = anim_id(&page, "warn");
    let mut rt = Runtime::new(&page);
    rt.play_id(&page, warn, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    let mid = resolve(&page, &rt).color(&page, b, PropKey::Background).unwrap();
    assert_eq!(mid, mix_rgba(Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255]), Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255]), 0.5), "the law's midpoint");
    assert_ne!(mid, Color_Straight_Srgb_4xU8_RGBA([128, 0, 0, 255]), "not the byte midpoint");
    rt.tick(&mut page, 0.6);
    assert!(!rt.is_playing_id(warn));
    assert_eq!(resolve(&page, &rt).color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255])), "the end pose holds");
    rt.set_prop(b, PropKey::Background, Value::VColor(Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255])));
    assert_eq!(resolve(&page, &rt).color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255])), "a held pose outranks a later code write");
}

/// `background.a` moves alpha alone: mid-play the fill is the declared
/// hue at the lane's alpha, and a code write of the WHOLE color mid-play
/// swaps the hue under the anim's alpha — the hue keeps following its
/// ladder.
#[test]
fn an_alpha_lane_keeps_the_ladder_hue() {
    let c = ok("anim fade { b.background.a to=0.2, duration=1, easing=linear }\nView v { Panel b background=#3355FFFF }");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let fade = anim_id(&page, "fade");
    let mut rt = Runtime::new(&page);
    rt.play_id(&page, fade, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    assert_eq!(resolve(&page, &rt).color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([0x33, 0x55, 0xFF, 153])), "declared hue, the lane's alpha (1 -> 0.2 at t=0.5 = 0.6)");
    rt.set_prop(b, PropKey::Background, Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255])));
    assert_eq!(resolve(&page, &rt).color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 153])), "a whole-color write swaps the hue under the anim's alpha");
}

/// The take-over law on a prop lane: a newer player takes `b.background`,
/// and its `stop(restore)` returns the value the FIRST play found —
/// bases hand over, exactly like the motion channels.
#[test]
fn a_newer_player_takes_the_lane_and_stop_restores() {
    let c = ok("anim go { b.background to=#FF0000FF, duration=1 }\nanim back { b.background to=#0000FFFF, duration=1 }\nView v { Panel b background=#112233FF }");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let (go, back) = (anim_id(&page, "go"), anim_id(&page, "back"));
    let mut rt = Runtime::new(&page);
    rt.play_id(&page, go, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    rt.play_id(&page, back, Finish::Nothing);
    rt.tick(&mut page, 0.1);
    rt.stop_id(back, true);
    assert_eq!(resolve(&page, &rt).color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([0x11, 0x22, 0x33, 255])), "restore lands on the first play's base");
}

/// Anim beats transition (user call): while a player owns `background`,
/// a `when` flip with a matching `transition` rule spawns NO tween on
/// that lane — two writers would fight — and the anim's value shows.
#[test]
fn an_owned_lane_spawns_no_tween() {
    let c = ok("anim glow { b.background to=#FF0000FF, duration=10 }\nView v { Panel b background=#000000FF { transition any -> held duration=1\n when held background=#FFFFFFFF } }");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let glow = anim_id(&page, "glow");
    let mut rt = Runtime::new(&page);
    let _ = resolve_animated(&page, &mut rt);
    rt.play_id(&page, glow, Finish::Nothing);
    rt.tick(&mut page, 1.0);
    rt.set_state(b, HELD, true);
    let r = resolve_animated(&page, &mut rt);
    assert!(!rt.tweens.iter().any(|t| t.node == b && t.key == PropKey::Background), "no tween on an owned lane");
    let shown = r.color(&page, b, PropKey::Background).unwrap();
    assert_ne!(shown, Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255]), "the anim's value shows, not the when's");
}

/// `g.stop_1_color.a` at full runtime: the resolved stop color is the
/// DECLARED stop's hue at the lane's alpha — the `.a` base falls back to
/// the `stops=` entry when nobody wrote the live key yet.
#[test]
fn a_stop_alpha_lane_reads_the_declared_stop() {
    let c = ok("anim glow { g.stop_1_color.a to=0.42, duration=1 }\nView v { Gradient g stops=[white, black] }");
    let mut page = c.page;
    let g = page.find("g").unwrap();
    let glow = anim_id(&page, "glow");
    let mut rt = Runtime::new(&page);
    rt.play_id(&page, glow, Finish::Nothing);
    rt.tick(&mut page, 1.1);
    assert_eq!(resolve(&page, &rt).color(&page, g, PropKey::StopColor(1)), Some(Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 107])), "white at 0.42");
    assert_eq!(resolve(&page, &rt).color(&page, g, PropKey::StopColor(2)), Some(Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255])), "the other stop untouched");
}

/// The lane fences: a word anims may not drive, `.a` on a non-color,
/// a word the target kind doesn't take, `key` children on a color lane,
/// and both spellings of one property in one anim.
#[test]
fn lane_fences() {
    let m = messages("anim a { b.text to=#FF0000FF, duration=1 }\nView v { Panel b }");
    assert!(m.contains("G2002") || m.contains("animatable"), "{m}");
    assert!(fails("anim a { b.width.a to=0.5, duration=1 }\nView v { Panel b }"), "`.a` needs a color word");
    assert!(fails("anim a { b.max to=5, duration=1 }\nView v { Panel b }"), "max is a Slider word, not a Rectangle's");
    assert!(fails("anim a { b.background { key t=0, v=0\n key t=1, v=1 } }\nView v { Panel b }"), "keys carry numbers, not colors");
    let m = messages("anim a { b.background to=#FF0000FF, duration=1\n and b.background.a to=0.5, duration=1 }\nView v { Panel b background=#000000FF }");
    assert!(m.contains("pick one spelling"), "{m}");
    // The happy spellings compile: a palette word, a numeric lane, `.a`.
    ok("anim a { b.background to=red, duration=1\n and b.corner_radius from=4, to=12, duration=1\n and b.border_color.a to=0.5, duration=1 }\nView v { Panel b background=#000000FF, border_thickness=1, border_color=#FFFFFFFF, corner_radius=4 }");
}
