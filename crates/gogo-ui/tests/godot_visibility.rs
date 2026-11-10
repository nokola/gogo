//! Port of the Godot gogo suite (`FantasiaGodot/addons/gogo/test/run_tests.gd`)
//! groups `_visibility_tests` (11841-11897), `_overlay_tests` (11897-12005),
//! `_disabled_cascade_tests` (12005-12068), `_aspect_fit_tests`
//! (12068-12173), `_component_host_cap_tests` (12173-12245) and
//! `_component_host_seat_tests` (12245-12323).
//!
//! Dialect: Godot's `space_only` is spelled `hidden` here (same meaning —
//! not drawn, no input, keeps its slot; the WORD is GP39's open question),
//! Godot's `Layers` root is a `View` root (G7003), `texture=` is `source=`,
//! `autowrap_mode` is `wrap=true`.
//!
//! ENGINE (not ported):
//! - `_component_host_cap_tests` cap.host-meta-measures, cap.host-lint,
//!   cap.host-quiet: script-backed host measure gate (ENGINE_MEASURE_META).
//! - `_component_host_seat_tests` (all 6): root anchors vs container size
//!   flags; the shell seats a page root here.
use gogo::draw::{Draw, draw, hit};
use gogo::KeptFrame;
use gogo::layout::{FixedAdvance, Rect, TextMeasure, layout};
use gogo::page::{PropKey, Color_Straight_Srgb_4xU8_RGBA, Value};
use gogo::runtime::{DISABLED, Runtime, resolve};
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

/// resolve + layout at `w`×`h` epx with the fixed-advance text ruler.
fn lay(page: &Page, rt: &Runtime, w: f32, h: f32) -> KeptFrame {
    KeptFrame::fresh(page, resolve(page, rt), Rect::new(0.0, 0.0, w, h), &FixedAdvance)
}

fn rect_of(page: &Page, rects: &[Rect], name: &str) -> Rect {
    rects[(page.find(name).unwrap_or_else(|| panic!("no node {name}"))).index()]
}

fn size_of(page: &Page, rects: &[Rect], name: &str) -> (f32, f32) {
    let r = rect_of(page, rects, name);
    (r.w, r.h)
}

/// The first solid `Quad` painted in exactly this color (a Rectangle's fill).
fn quad_of(list: &[Draw], color: Color_Straight_Srgb_4xU8_RGBA) -> Option<Rect> {
    list.iter().find_map(|d| match d {
        Draw::Quad { rect, color: c, .. } if *c == color => Some(*rect),
        _ => None,
    })
}

fn text_alpha(page: &Page, list: &[Draw], word: &str) -> u8 {
    list.iter()
        .find_map(|d| match d {
            Draw::Text { text, color, .. } if page.str(*text) == word => Some(color.0[3]),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {word:?} in {list:#?}"))
}

fn about(alpha: u8, fraction: f32) -> bool {
    (alpha as i32 - (255.0 * fraction).round() as i32).abs() <= 1
}

fn set_visibility(page: &mut Page, rt: &mut Runtime, name: &str, word: &str) {
    let node = page.find(name).unwrap();
    let id = page.intern(word);
    rt.set_prop(node, PropKey::Visibility, Value::VIdent(id));
}

const BLUE: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0x44, 0x88, 0xCC, 0xFF]);
const ORANGE: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0xCC, 0x88, 0x44, 0xFF]);
const GREEN: Color_Straight_Srgb_4xU8_RGBA = Color_Straight_Srgb_4xU8_RGBA([0x22, 0xCC, 0x44, 0xFF]);

// ------------------------------------------------------------ visibility

/// `_visibility_tests` 11842-11843 (vis.bad-word). Godot refuses
/// `visibility=maybe` with an error naming the triad. Rust compiles it
/// clean and the node is simply visible.
#[test]
fn visibility_refuses_a_word_outside_the_triad() {
    assert!(fails("View v { Panel p visibility=maybe }"), "{:?}", codes("View v { Panel p visibility=maybe }"));
}

/// `_visibility_tests` 11844-11845 (vis.retired). The retired bool
/// `visible=` is not a word: gogo-design.md §7 dropped every retired-
/// spelling redirect (no alias table). Rust reports it as an unknown
/// property, G2001.
#[test]
fn retired_visible_bool_is_not_a_word() {
    let c = codes("View v { Panel p visible=false }");
    assert!(c.contains(&"G2001".to_string()), "{c:?}");
}

/// `_visibility_tests` 11846-11847 (vis.container-fence) — DEVIATION:
/// Godot refuses `space_only` under a Column/Row because the ENGINE's
/// containers skip hidden children in their own sorting (a "v1" engine
/// limit). This layout is pure and holds the slot under any parent, so
/// the fence would remove a capability — the node builds and keeps its
/// seat.
#[test]
fn slot_keeping_space_only_works_under_containers_too() {
    let src = "View v { Column c { Panel p visibility=space_only, min_size=10x10
 Rectangle after 10x10, color=red } }";
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let f = KeptFrame::fresh(&c.page, resolve(&c.page, &rt), Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    let rects = f.rects();
    let after = rects[(c.page.find("after").unwrap()).index()];
    assert_eq!(after.y, 10.0, "the seat holds its 10-epx slot: {rects:?}");
}

/// `_visibility_tests` 11849-11896 (vis.build-ok, hidden-flag, of,
/// space-keeps, toggle-stable, toggle-back, gone-releases). Both hidden
/// modes are not painted and take no touch; `gone` gives its room back
/// while `hidden` keeps holding its 200×48 seat, so flipping visible ↔
/// hidden never moves the hugging parent, and only `gone` shrinks it.
#[test]
fn hidden_keeps_its_seat_and_gone_releases_it() {
    let src = "View vhost {\n Layers vwrap width=hug, height=hug, halign=left, valign=top {\n  Rectangle va color=#4488CCFF, width=120, height=24, halign=left, valign=top\n  Rectangle vseat color=#CC8844FF, width=200, height=48, halign=left, valign=top, visibility=space_only\n  Rectangle vgone color=#22CC44FF, width=300, height=90, halign=left, valign=top, visibility=gone\n }\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let f = lay(&page, &rt, 400.0, 400.0);
    let (res, rects) = (f.look(), f.rects());
    let list = draw(&page, f.frame());
    // vis.hidden-flag: neither hidden mode paints or takes a touch.
    assert!(quad_of(&list, BLUE).is_some(), "{list:#?}");
    assert!(quad_of(&list, ORANGE).is_none(), "hidden painted: {list:#?}");
    assert!(quad_of(&list, GREEN).is_none(), "gone painted: {list:#?}");
    let vseat = page.find("vseat").unwrap();
    let vgone = page.find("vgone").unwrap();
    let va = page.find("va").unwrap();
    let under_finger = hit(&page, f.frame(), 150.0, 40.0, true);
    assert!(under_finger != Some(vseat) && under_finger != Some(vgone), "{under_finger:?}");
    // vis.of
    assert!(res.hidden(&page, vseat) && !res.gone(&page, vseat));
    assert!(res.gone(&page, vgone));
    assert!(!res.hidden(&page, va) && !res.gone(&page, va));
    // vis.space-keeps: gone released 300×90, hidden still holds 200×48.
    assert_eq!(size_of(&page, &rects, "vwrap"), (200.0, 48.0), "{rects:?}");
    // vis.toggle-stable
    set_visibility(&mut page, &mut rt, "vseat", "visible");
    let f = lay(&page, &rt, 400.0, 400.0);
    let rects = f.rects();
    assert_eq!(size_of(&page, &rects, "vwrap"), (200.0, 48.0), "{rects:?}");
    assert!(quad_of(&draw(&page, f.frame()), ORANGE).is_some());
    // vis.toggle-back
    set_visibility(&mut page, &mut rt, "vseat", "space_only");
    let f = lay(&page, &rt, 400.0, 400.0);
    let rects = f.rects();
    assert_eq!(size_of(&page, &rects, "vwrap"), (200.0, 48.0), "{rects:?}");
    assert!(quad_of(&draw(&page, f.frame()), ORANGE).is_none());
    // vis.gone-releases
    set_visibility(&mut page, &mut rt, "vseat", "gone");
    let f = lay(&page, &rt, 400.0, 400.0);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(size_of(&page, &rects, "vwrap"), (120.0, 24.0), "{rects:?}");
}

// --------------------------------------------------------------- overlay

/// `_overlay_tests` 11908-11910 (ovl.container-fence). Godot refuses
/// `overlay=true` under a Column/Row ("free parent") — a stack has no
/// place for a box that claims no slot. Rust compiles it.
#[test]
fn overlay_is_refused_under_a_stack() {
    let src = "View v { Column c { Panel p overlay=true, width=64, height=24 } }";
    assert!(fails(src), "{:?}", codes(src));
}

/// `_overlay_tests` 11911-11924 (ovl.space-only-conflict,
/// ovl.fill-undetermined, ovl.bad-value). Godot refuses three overlay
/// shapes at build: overlay + keep-the-slot hidden ("contradict"), an
/// overlay `width=fill` inside a hugging parent no other child sizes
/// ("nothing determines"), and `overlay=maybe`. Rust compiles all three
/// clean (and reads `overlay=maybe` as not-overlay).
#[test]
fn overlay_contradictions_are_refused() {
    let conflict = "View v { Rectangle r overlay=true, visibility=space_only, color=#FF0000FF, width=10, height=10 }";
    assert!(fails(conflict), "{:?}", codes(conflict));
    let undetermined = "View v {\n Layers hugbox width=hug, height=hug, halign=left, valign=top {\n  Rectangle r overlay=true, color=#FF0000FF, width=fill, height=10\n }\n}\n";
    assert!(fails(undetermined), "{:?}", codes(undetermined));
    let bad = "View v { Panel p overlay=maybe }";
    assert!(fails(bad), "{:?}", codes(bad));
}

/// `_overlay_tests` 11927-11951 (ovl.build-ok, no-upward, own-min-intact,
/// vis-composes, vis-composes-back). An overlay box sits on its parent
/// without sizing it: the hugging parent stays 120×24 with a 300×90
/// overlay inside, the overlay's own rectangle is still the full 300×90
/// (the cut is only what it tells its parent), and visibility flips on the
/// overlay change nothing upward.
#[test]
fn overlay_never_counts_upward_but_keeps_its_own_size() {
    let src = "View ohost {\n Layers owrap width=hug, height=hug, halign=left, valign=top {\n  Rectangle obase color=#4488CCFF, width=120, height=24, halign=left, valign=top\n  Rectangle oslop color=#CC8844FF, overlay=true, width=300, height=90, halign=left, valign=top, margin_left=-12, margin_top=-12\n }\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let f = lay(&page, &rt, 400.0, 400.0);
    let rects = f.rects();
    assert_eq!(size_of(&page, &rects, "owrap"), (120.0, 24.0), "{rects:?}");
    assert_eq!(rect_of(&page, &rects, "oslop"), Rect::new(-12.0, -12.0, 300.0, 90.0), "{rects:?}");
    assert!(quad_of(&draw(&page, f.frame()), ORANGE).is_some());
    set_visibility(&mut page, &mut rt, "oslop", "gone");
    let f = lay(&page, &rt, 400.0, 400.0);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(size_of(&page, &rects, "owrap"), (120.0, 24.0), "{rects:?}");
    set_visibility(&mut page, &mut rt, "oslop", "visible");
    let f = lay(&page, &rt, 400.0, 400.0);
    let rects = f.rects();
    assert_eq!(size_of(&page, &rects, "owrap"), (120.0, 24.0), "{rects:?}");
    assert!(quad_of(&draw(&page, f.frame()), ORANGE).is_some());
}

/// `_overlay_tests` 11955-11975 (ovl.nested-build, nested-inside-counts,
/// nested-no-upward). The cut binds at the overlay's OWN parent boundary
/// only: an overlay Layers still measures its inside (200×70, its own
/// overlay child excluded), while the grandparent hugs only the 50×20 base.
#[test]
fn overlay_inside_still_measures_like_any_box() {
    let src = "View nhost {\n Layers nwrap width=hug, height=hug, halign=left, valign=top {\n  Rectangle nbase color=#4488CCFF, width=50, height=20, halign=left, valign=top\n  Layers nouter overlay=true, width=hug, height=hug, halign=left, valign=top {\n   Rectangle ninner color=#22CC44FF, width=200, height=70, halign=left, valign=top\n   Rectangle nnested color=#CC8844FF, overlay=true, width=400, height=5, halign=left, valign=top\n  }\n }\n}\n";
    let c = ok(src);
    let f = lay(&c.page, &Runtime::new(&c.page), 400.0, 400.0);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(size_of(&c.page, &rects, "nouter"), (200.0, 70.0), "{rects:?}");
    assert_eq!(size_of(&c.page, &rects, "nwrap"), (50.0, 20.0), "{rects:?}");
}

/// `_overlay_tests` 11979-11992 (ovl.scroll-build, ovl.scroll-extent). An
/// overlay inside scroll content never widens the scrollable range: the
/// hugging content is 200 tall (the 60×200 row), not 900 (the overlay).
/// Godot pins the content's minimum (60, 200); here the placed content is
/// the viewport's 100 wide (`width=fill`) and 200 tall.
#[test]
fn overlay_never_widens_the_scroll_range() {
    let src = "View shost {\n Scroll sview width=100, height=80, halign=left, valign=top, allow=vertical, bars=never {\n  Layers scontent width=fill, height=hug {\n   Rectangle srow color=#4488CCFF, width=60, height=200, halign=left, valign=top\n   Rectangle sbig color=#CC8844FF, overlay=true, width=500, height=900, halign=left, valign=top\n  }\n }\n}\n";
    let c = ok(src);
    let f = lay(&c.page, &Runtime::new(&c.page), 400.0, 400.0);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(size_of(&c.page, &rects, "scontent"), (100.0, 200.0), "{rects:?}");
}

/// `_overlay_tests` 11996-12004 (ovl.lint-quiet). A fixed 40×40 box with a
/// 90×90 overlay child builds warning-free. Note: Rust has no D126
/// "exact loses to content" warning at all (a 90×90 NON-overlay child is
/// just as quiet), so this holds by absence, not by the lint seeing
/// through the cut.
#[test]
fn oversized_overlay_in_a_fixed_box_builds_quiet() {
    let c = compile("View lhost {\n Layers lbox width=40, height=40, halign=left, valign=top {\n  Rectangle lslop color=#CC8844FF, overlay=true, width=90, height=90, halign=left, valign=top\n }\n}\n", &|_| None);
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

// ------------------------------------------------------ disabled cascade

/// `_disabled_cascade_tests` 12014-12067 (dis.build-ok, cascades,
/// meta-cascades, when-fires, no-opt-out, two-layer, when-releases,
/// engine-own-adopted, dynamic-entry). `disabled` flows down: effective =
/// own OR any ancestor's; a child can't re-enable under a disabled
/// ancestor; re-enabling the ancestor restores each child's OWN flag; a
/// late-arriving child (a repeat item shown by code) inherits.
///
/// dis.when-fires is a FLIP by GPAR11 (DONE): Rust dims a disabled
/// control's subtree with a built-in `when disabled opacity=0.3` at the
/// bottom of the ladder, and opacity multiplies down the tree — so the
/// label's authored `when disabled opacity=0.5` lands at 0.3 × 0.5 = 0.15,
/// where Godot (no built-in dim) shows 0.5. dis.when-releases is the same
/// on both: 1.0 once the panel re-enables.
#[test]
fn disabled_cascades_down_and_own_flags_survive() {
    let src = "View dhost {\n Panel grp disabled=true, halign=left, valign=top {\n  Column dcol {\n   Button db1 text=\"one\"\n   Button db2 text=\"two\", disabled=true\n   Label dlbl text=\"grey me\" { when disabled { opacity = 0.5 } }\n   repeat late capacity=1 { Button nb text=\"late\" }\n  }\n }\n}\n";
    let c = ok(src);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let grp = page.find("grp").unwrap();
    let db1 = page.find("db1").unwrap();
    let db2 = page.find("db2").unwrap();
    let dlbl = page.find("dlbl").unwrap();
    let late = page.find("late").unwrap();
    // dis.cascades + dis.meta-cascades
    assert!(rt.disabled(&page, db1), "the panel's disabled did not reach the button");
    assert!(rt.disabled(&page, dlbl));
    // dis.when-fires (FLIP, GPAR11: 0.3 ancestor dim × 0.5 authored)
    let f = lay(&page, &rt, 400.0, 400.0);
    let list = draw(&page, f.frame());
    assert!(about(text_alpha(&page, &list, "grey me"), 0.3 * 0.5), "{}", text_alpha(&page, &list, "grey me"));
    // dis.no-opt-out
    rt.set_state(db1, DISABLED, false);
    assert!(rt.disabled(&page, db1), "a child re-enabled itself under a disabled ancestor");
    // dis.two-layer + dis.when-releases
    rt.set_state(grp, DISABLED, false);
    assert!(!rt.disabled(&page, db1) && rt.disabled(&page, db2), "db1={} db2={}", rt.disabled(&page, db1), rt.disabled(&page, db2));
    let f = lay(&page, &rt, 400.0, 400.0);
    let list = draw(&page, f.frame());
    assert_eq!(text_alpha(&page, &list, "grey me"), 255);
    // dis.engine-own-adopted: a code write is the own flag — it survives an
    // ancestor disable/enable round trip.
    rt.set_state(db1, DISABLED, true);
    rt.set_state(grp, DISABLED, true);
    rt.set_state(grp, DISABLED, false);
    assert!(rt.disabled(&page, db1), "direct own write lost");
    rt.set_state(db1, DISABLED, false);
    // dis.dynamic-entry: an item shown by code inside a disabled subtree
    // is disabled the moment it appears.
    rt.set_state(grp, DISABLED, true);
    rt.set_count(&mut page, late, 1);
    let nb = rt.items(&page, late)[0];
    assert!(!resolve(&page, &rt).gone(&page, nb));
    assert!(rt.disabled(&page, nb), "late child not held down");
}

// --------------------------------------------------------- aspect + fit

/// `_aspect_fit_tests` 12075-12086 (aspect.both-spelled, hug-fence,
/// cap-fence, container-fence, natural-non-image, bad-value). Godot
/// refuses six `aspect=` shapes at build: both axes spelled, `hug` with
/// it, a `max_*` cap with it, a container parent, `natural` on a
/// non-Image, a non-positive number. Rust refuses the same six as G1017
/// (GPAR80, round 4).
#[test]
fn aspect_fences_refuse_bad_shapes() {
    for src in [
        "View v { Rectangle r width=100, height=50, aspect=1 }",
        "View v { Panel p width=hug, aspect=1 }",
        "View v { Panel p width=hug, max_width=100, aspect=1 }",
        "View v { Column c { Rectangle r width=100, aspect=1 } }",
        "View v { Rectangle r aspect=natural }",
        "View v { Rectangle r aspect=-2 }",
    ] {
        assert!(fails(src), "{src}: {:?}", codes(src));
    }
}

/// `_aspect_fit_tests` 12087-12092 (fit.non-image, fit.fill-hint,
/// fit.bad-word). Godot refuses `fit=` on a non-content node, hints
/// `distort` for the CSS word `fill`, and names `contain` for an unknown
/// word. Rust refuses all three too — the kind table keeps `fit=` on an
/// Image, and G0057's closed word set names the good words.
#[test]
fn fit_fences_refuse_bad_shapes() {
    for src in ["View v { Panel p fit=cover }", "View v { Image i fit=fill }", "View v { Image i fit=middle }"] {
        assert!(fails(src), "{src}: {:?}", codes(src));
    }
}

/// `_aspect_fit_tests` 12093-12096 (fit.retired-stretch, fit.retired-
/// expand). The raw engine consts `stretch_mode=` / `expand_mode=` are not
/// words: gogo-design.md §7 dropped the raw register and every redirect;
/// Rust refuses both with G0041 naming `fit=contain|cover|distort`.
#[test]
fn retired_stretch_and_expand_are_refused() {
    let stretch = codes("View v { Image i stretch_mode=keep_aspect }");
    assert!(stretch.contains(&"G0041".to_string()), "{stretch:?}");
    let expand = codes("View v { Image i expand_mode=ignore_size }");
    assert!(expand.contains(&"G0041".to_string()), "{expand:?}");
}

const ASPECT_FIXTURE: &str = "View ahost {\n Rectangle fixed_drive color=#4488CCFF, width=200, aspect=2, halign=left, valign=top\n Rectangle fill_drive color=#CC8844FF, width=fill, aspect=4, margin_left=20, margin_right=20, valign=top\n Rectangle fitbox color=#22CC44FF, width=fill, height=fill, aspect=1\n Image cimg source=\"res://icon.svg\", 96x48, fit=cover, halign=left, valign=bottom\n Image dimg source=\"res://icon.svg\", 96x48, fit=distort, halign=right, valign=bottom\n Image plain_img source=\"res://icon.svg\", 96x48, halign=right, valign=top\n}\n";

/// `_aspect_fit_tests` 12098-12148 (aspect.build-ok, fit.cover,
/// fit.distort, fit.default-contain). The fixture builds; each `fit=` word
/// reaches the draw list on its Image, and `contain` is the default.
#[test]
fn fit_words_reach_the_draw_list_with_contain_as_default() {
    let c = ok(ASPECT_FIXTURE);
    let f = lay(&c.page, &Runtime::new(&c.page), 400.0, 300.0);
    let rects = f.rects();
    let list = draw(&c.page, f.frame());
    let fit_at = |name: &str| {
        let r = rect_of(&c.page, &rects, name);
        list.iter()
            .find_map(|d| match d {
                Draw::Image { rect, fit, .. } if *rect == r => Some(*fit),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no image at {r:?} in {list:#?}"))
    };
    assert_eq!(fit_at("cimg"), gogo::Fit::Cover);
    assert_eq!(fit_at("dimg"), gogo::Fit::Distort);
    assert_eq!(fit_at("plain_img"), gogo::Fit::Contain);
}

/// `_aspect_fit_tests` 12122-12127 (aspect.derives-fixed, aspect.derives-
/// fill). `aspect=` is a relation on ANY node's own rectangle: the spelled
/// axis drives, the other is derived — a 200-wide `aspect=2` Rectangle is
/// 200×100; a fill-width `aspect=4` one with 20-epx side margins in a
/// 400 frame is 360×90. Rust derives the same way on any box at place
/// (GPAR80, round 4).
#[test]
fn aspect_derives_the_unspelled_axis_on_any_node() {
    let c = ok(ASPECT_FIXTURE);
    let f = lay(&c.page, &Runtime::new(&c.page), 400.0, 300.0);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(size_of(&c.page, &rects, "fixed_drive"), (200.0, 100.0), "{rects:?}");
    assert_eq!(size_of(&c.page, &rects, "fill_drive"), (360.0, 90.0), "{rects:?}");
}

/// `_aspect_fit_tests` 12128-12131, 12143-12147 (aspect.fit-solves,
/// aspect.fit-centered, aspect.resize-live). Both axes `fill` + `aspect=1`
/// = the largest square that fits the 400×300 frame, centered: 300×300 at
/// x=50; the solve is live — at 240×300 the square is 240×240 and the
/// fill-width `aspect=4` box re-derives to 200×50.
#[test]
fn both_fill_aspect_is_the_largest_fitting_box_centered_and_live() {
    let c = ok(ASPECT_FIXTURE);
    let rt = Runtime::new(&c.page);
    let f = lay(&c.page, &rt, 400.0, 300.0);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(rect_of(&c.page, &rects, "fitbox"), Rect::new(50.0, 0.0, 300.0, 300.0), "{rects:?}");
    let f = lay(&c.page, &rt, 240.0, 300.0);
    let (_, rects) = (f.look(), f.rects());
    assert_eq!(size_of(&c.page, &rects, "fitbox"), (240.0, 240.0), "{rects:?}");
    assert_eq!(size_of(&c.page, &rects, "fill_drive"), (200.0, 50.0), "{rects:?}");
}

/// A text ruler that also knows one square bitmap (Godot's icon.svg is
/// square), for `aspect=natural` — the bitmap by its compiled id.
struct SquareIcon(gogo::ImageKey);

impl TextMeasure for SquareIcon {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        FixedAdvance.measure(text, font_size, max_width, font)
    }
    fn image_size(&self, key: gogo::ImageKey) -> Option<(f32, f32)> {
        (key == self.0).then_some((128.0, 128.0))
    }
}

/// `_aspect_fit_tests` 12153-12172 (aspect.natural-ok, aspect.natural-
/// hug). `aspect=natural` follows the bitmap's own ratio from the driving
/// axis and a hugging parent counts it: a square icon 200 wide makes the
/// hug wrapper 200×200.
#[test]
fn natural_aspect_sizes_the_hug_parent_from_the_bitmap() {
    let c = ok("View nhost {\n Layers nwrap width=hug, height=hug, halign=left, valign=top {\n  Image nimg source=\"res://icon.svg\", width=200, aspect=natural\n }\n}\n");
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let icon = SquareIcon(c.images.get("res://icon.svg").expect("the page's source= is interned"));
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &icon);
    assert_eq!(size_of(&c.page, &rects, "nwrap"), (200.0, 200.0), "{rects:?}");
    assert_eq!(size_of(&c.page, &rects, "nimg"), (200.0, 200.0), "{rects:?}");
}

// ------------------------------------------------- component host (cap)

/// `_component_host_cap_tests` 12234-12235 (cap.host-binds), the pure-
/// layout half of the toast-column bug: a component root `width=hug,
/// max_width=fill, halign=center` with 12-epx side margins and a wrapping
/// label, in a 360-wide frame, is never wider than the 336 the frame
/// allows and never the one-character sliver. Godot measured 336.0 with
/// Roboto; with `FixedAdvance` the hug stops at the widest wrapped line,
/// so this pins the relation. The label wraps (taller than one line).
#[test]
fn capped_hug_component_root_follows_the_frame_not_the_sliver() {
    let c = ok("Panel capHost background=#000000FF, padding=12, valign=top, width=hug, max_width=fill, halign=center, margin_left=12, margin_right=12 {\n Label capText text=\"a component label long enough to overflow the frame and wrap twice over\", font_size=16, wrap=true\n}\n");
    let f = lay(&c.page, &Runtime::new(&c.page), 360.0, 600.0);
    let (_, rects) = (f.look(), f.rects());
    let host = rect_of(&c.page, &rects, "capHost");
    assert!(host.w <= 336.0 + 0.01 && host.w > 200.0, "{host:?}");
    assert!(host.x >= 12.0 - 0.01 && host.right() <= 348.0 + 0.01, "{host:?}");
    let label = rect_of(&c.page, &rects, "capText");
    assert!(label.h > 16.0 * 1.25 + 0.5, "the label did not wrap: {label:?}");
}
