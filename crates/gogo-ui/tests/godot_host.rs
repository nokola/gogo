//! Port of the Godot gogo suite's HOST batch (run_tests.gd 10064-11116):
//! `_view_and_reload_tests` 10064-10191, `_view_retrack_tests` 10191-10247,
//! `_host_axes_tests` 10247-10352, `_hug_shrink_back_tests` 10352-10432,
//! `_width_first_tests` 10432-10478, `_button_content_kick_tests`
//! 10478-10537, `_component_oversize_tests` 10537-10698,
//! `_spacing_default_tests` 10698-10756, `_content_min_tests` 10756-11044,
//! `_custom_control_registry_tests` 11044-11077, `_store_guard_tests`
//! 11077-11101 (+ `_guard_scan` 11101-11116, a helper with no checks).
//!
//! A Godot "frame" here is one more `layout()` call: the Rust layout is a
//! pure function of page + runtime, so "write, await a frame, read" ports
//! as "write, lay out again, read". `custom_minimum_size` (the derived
//! minimum) ports as the rect a Flow gives its child — a Flow lays every
//! child at its measured minimum on both axes — or, for a root, the rect a
//! zero-size viewport leaves it (an unwritten size never goes under its
//! content, GPAR40).
//!
//! ENGINE — not ported (Godot scene machinery, no Rust counterpart):
//! - view.on-built-ran / reload.ok / reload.new-control /
//!   reload.state-preserved / reload.keeps-tree (10078, 10086-10092,
//!   10097): host-node hot reload keeps engine controls.
//! - view.root-container-host / view.panel-root-host-contract (10103,
//!   10113): host node CLASS contract (Container vs PanelContainer).
//! - epx.late-provider-raw / epx.late-provider-refeeds (10173, 10176):
//!   epx provider re-feeds engine font sizes.
//! - `_view_retrack_tests` (all 6, 10191-10247): reparent keeps epx
//!   registration; Rust has no node tree.
//! - host.no-flags / host.explain-mode (10276, 10277): engine size flags,
//!   explain meta. host.raw-panelcontainer-refused (10287): host class.
//!   host.panel-* (10305-10331, 8 checks) and host.view-builds /
//!   host.view-axis-zeroed (10345, 10347): script host set_size clamp,
//!   min-size ratchet — no ratchet in a pure layout (gogo-design §7).
//! - btnkick.add-derives / btnkick.late-child-wired (10529, 10534):
//!   runtime add_child; Rust pages are a static arena (GP27 caps repeats).
//! - oversize.not-before-confirm / fires / one-shot / transient-quiet /
//!   hidden-quiet / fires-on-show / host-quiet / seated-quiet /
//!   pair-both-fire / pair-none-stuck (10560-10693, 10 checks): the
//!   forgotten-`width=host` tripwire watches the ratchet's stale size;
//!   Rust has neither the ratchet nor a host axis, so nothing to warn.
//! - cmin.epx-scaled / cmin.epx-no-ratchet (10879, 10887): epx provider
//!   re-apply of engine minimums. vmin.formula-stamped /
//!   vmin.custom-stays-declared (10998, 11000): measure-formula meta.
//!   vmin.late-child-counts (11040): runtime add_child.
//! - reg.create / reg.bad-script-not-registered (11052, 11073): GDScript
//!   factory door. `_store_guard_tests` (all 3): asset-store packaging
//!   files (LICENSE, plugin.cfg, host-name scan).
use gogo::compile::CompileOptions;
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Align, Kind, PropKey, Rgba, Value};
use gogo::runtime::{Resolved, Runtime, resolve};
use gogo::{Compiled, NodeId, Page, compile, compile_with};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn lay(page: &Page, rt: &Runtime, w: f32, h: f32) -> (Resolved, Vec<Rect>) {
    let res = resolve(page, rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, w, h), &FixedAdvance);
    (res, rects)
}

fn rect(page: &Page, rects: &[Rect], name: &str) -> Rect {
    rects[page.find(name).unwrap_or_else(|| panic!("no node {name}")) as usize]
}

fn id(page: &Page, name: &str) -> NodeId {
    page.find(name).unwrap_or_else(|| panic!("no node {name}"))
}

/// Godot fixtures the batch loads, inlined (test/fixtures/*.gogo), in the
/// Rust dialect: `autowrap_mode=word_smart` → `wrap=true`; `width=host`
/// stays where the check is ABOUT the host axis (it must be refused).
const THEME_INC: &str = "$swatchTest = #EB6428FF\nstyle TestPanel spacing=8\n";
const VIEW_TEST: &str = "uses \"theme_inc.gogo\"\n\nView visibility=gone, halign=center, valign=bottom, margin_bottom=72 {\n    Column .TestPanel min_width=240 {\n        Label text=\"Level\"\n        Slider level 220x, min=1, max=300, value=80\n        Rectangle swatch color=$swatchTest\n    }\n}\n";
const BROKEN: &str = "View {\n    Label text=\"unterminated\n}\n";
const COMPONENT_BOX: &str = "Panel box width=120, height=40, padding=8, background=#112233FF {\n    Label inner text=\"x\"\n}\n";
const BARE_COMPONENT: &str = "Panel bare {\n    Label t text=\"x\", font_size=10\n}\n";
const HOST_AXES_COMPONENT: &str = "Panel strip width=host, height=host, min_width=40, min_height=10 {\n    Row width=fill, height=fill {\n        Label blurb width=fill, wrap=true,\n                text=\"a long line of words that wraps to one character per line when the box is a sliver\"\n    }\n}\n";
const HUG_SHRINK_COMPONENT: &str = "Panel card width=hug, height=hug, halign=left, valign=top {\n    Label words text=\"short\"\n}\n";
const LAYERS_SHRINK_COMPONENT: &str = "Panel frame width=fill, height=fill {\n    Layers width=fill, height=fill {\n        Panel chip width=hug, height=hug, halign=left, valign=top {\n            Label w text=\"short\"\n        }\n    }\n}\n";
const WIDTH_FIRST_COMPONENT: &str = "Panel toast width=hug, max_width=fill, halign=center, valign=top {\n    Label blurb wrap=true, horizontal_alignment=center,\n            text=\"a long line of words that folds into many rows when the frame is narrow and one row when it is wide\"\n}\n";

// ---------------------------------------------------------------- view + reload

/// `_view_and_reload_tests` 10064-10099 (view.ok, view.uses-theme,
/// view.root-props, view.pins-on-root, reload.bad-reports): the view
/// fixture builds through its `uses` include, the included `$value` reaches
/// the swatch, the root's own words (gone, bottom-pinned by 72) resolve,
/// and a file with an unterminated string does not build.
#[test]
fn view_fixture_builds_with_its_include_and_a_broken_file_does_not() {
    let loader = |rel: &str| if rel == "theme_inc.gogo" { Some(THEME_INC.to_string()) } else { None };
    let c = compile(VIEW_TEST, &loader);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (res, _) = lay(&page, &rt, 400.0, 800.0);
    let swatch = id(&page, "swatch");
    assert_eq!(res.color(&page, swatch, PropKey::Color), Some(Rgba([0xEB, 0x64, 0x28, 0xFF])), "the include's $swatchTest reaches the swatch");
    assert!(res.gone(&page, 0), "the root's `visibility=gone` holds");
    let root = res.placement[0];
    assert_eq!(root.valign, Align::End, "{root:?}");
    assert_eq!(root.margin[3], Some(72.0), "{root:?}");
    let bad = compile(BROKEN, &loader);
    assert!(gogo::has_errors(&bad.diags), "an unterminated string must not build: {:#?}", bad.diags);
}

/// `_view_and_reload_tests` 10115-10128 (view.panel-root-builds,
/// -named, -plate, -padding, -min): a Panel-rooted component IS the styled
/// box — the root node is the named box (no wrapper), its background,
/// per-side padding and declared 120×40 size are the box's own.
#[test]
fn panel_root_component_is_the_styled_box() {
    let c = ok(COMPONENT_BOX);
    let page = c.page;
    assert_eq!(page.find("box"), Some(0), "the root IS the box: no wrapper node");
    let rt = Runtime::new(&page);
    let (res, rects) = lay(&page, &rt, 400.0, 300.0);
    assert_eq!(res.color(&page, 0, PropKey::Background), Some(Rgba([0x11, 0x22, 0x33, 0xFF])));
    assert_eq!(res.placement[0].padding, [8.0, 8.0, 8.0, 8.0], "{:?}", res.placement[0]);
    assert_eq!((rects[0].w, rects[0].h), (120.0, 40.0), "{rects:?}");
}

/// `_view_and_reload_tests` 10140-10146 + 10165 (view.panel-root-bare-
/// builds, -bare-transparent, epx.late-provider-builds): a Panel root that
/// declares no background paints no plate at all — the D61 blank slate,
/// never an ambient gray.
#[test]
fn bare_panel_root_paints_no_plate() {
    let c = ok(BARE_COMPONENT);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (res, rects) = lay(&page, &rt, 400.0, 300.0);
    let list = draw(&page, &res, &rects);
    assert!(!list.iter().any(|d| matches!(d, Draw::Quad { .. })), "no plate for a bare Panel root: {list:#?}");
    assert!(list.iter().any(|d| matches!(d, Draw::Text { .. })), "the label still draws: {list:#?}");
}

// ---------------------------------------------------------------- host axes

/// `_host_axes_tests` 10249-10272 (host.root-only, host.root-only-height,
/// host.no-pins, host.no-cap, host.detached-builds) and
/// `_component_oversize_tests` 10612 (oversize.host-builds).
/// FLIP — gogo-design.md §7 "Removed": `width=host` / `height=host` are
/// gone (a pure layout has no ratchet to guard). Godot refused them off
/// the root (G7004) and accepted them on one; Rust refuses the word
/// everywhere with G0055, root included.
#[test]
fn host_axis_is_refused_everywhere() {
    let g0055 = "G0055".to_string();
    assert!(codes("Panel a {\n    Row b width=host {\n    }\n}\n").contains(&g0055));
    assert!(codes("View a {\n    Label b height=host, text=\"x\"\n}\n").contains(&g0055));
    assert!(codes("Panel a width=host, halign=center {\n}\n").contains(&g0055));
    assert!(codes("Panel a width=host, max_width=100 {\n}\n").contains(&g0055));
    // Godot: builds clean on a root (host.detached-builds). Rust: the word is gone.
    let root = codes("Panel a width=host, height=host, min_width=40, min_height=10 {\n    Label t text=\"x\"\n}\n");
    assert!(root.contains(&g0055), "{root:?}");
    let fixture = codes(HOST_AXES_COMPONENT);
    assert!(fixture.contains(&g0055), "{fixture:?}");
}

// ---------------------------------------------------------------- hug shrink back

/// `_hug_shrink_back_tests` 10352-10398 (shrink.builds, base-hugs, grows,
/// shrinks-back, hidden-write-measured-on-show, shrinks-back-again): a
/// hugging box follows its content BOTH ways — a longer text widens it, a
/// shorter one brings it back to the exact earlier width, and a write made
/// while the box is gone is measured the moment it shows again.
#[test]
fn hug_box_grows_and_shrinks_back_with_its_text() {
    let c = ok(HUG_SHRINK_COMPONENT);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let words = id(&page, "words");
    let base = lay(&page, &rt, 400.0, 300.0).1[0];
    assert!(base.w > 5.0 && base.w < 200.0, "{base:?}");
    rt.set_text(&mut page, words, "a considerably longer line of words than before");
    let grown = lay(&page, &rt, 400.0, 300.0).1[0];
    assert!(grown.w > base.w + 20.0, "{base:?} -> {grown:?}");
    rt.set_text(&mut page, words, "short");
    let back = lay(&page, &rt, 400.0, 300.0).1[0];
    assert!((back.w - base.w).abs() < 0.5, "{back:?} vs {base:?}");
    // Written while gone, measured on show.
    rt.set_visible(&mut page, 0, false);
    rt.set_text(&mut page, words, "a considerably longer line of words than before");
    rt.set_visible(&mut page, 0, true);
    let shown = lay(&page, &rt, 400.0, 300.0).1[0];
    assert!((shown.w - grown.w).abs() < 0.5, "{shown:?} vs {grown:?}");
    rt.set_text(&mut page, words, "short");
    let again = lay(&page, &rt, 400.0, 300.0).1[0];
    assert!((again.w - base.w).abs() < 0.5, "{again:?} vs {base:?}");
}

/// `_hug_shrink_back_tests` 10404-10428 (shrink.layers-builds, -base,
/// -grows, -shrinks-back): the same both-ways rule for a hug chip under
/// Layers (a free parent that never re-sorts its children).
#[test]
fn hug_chip_under_layers_follows_its_text_both_ways() {
    let c = ok(LAYERS_SHRINK_COMPONENT);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let w = id(&page, "w");
    let base = rect(&page, &lay(&page, &rt, 400.0, 300.0).1, "chip");
    assert!(base.w > 5.0 && base.w < 200.0, "{base:?}");
    rt.set_text(&mut page, w, "a considerably longer line of words than before");
    let grown = rect(&page, &lay(&page, &rt, 400.0, 300.0).1, "chip");
    assert!(grown.w > base.w + 20.0, "{base:?} -> {grown:?}");
    rt.set_text(&mut page, w, "short");
    let back = rect(&page, &lay(&page, &rt, 400.0, 300.0).1, "chip");
    assert!((back.w - base.w).abs() < 0.5, "{back:?} vs {base:?}");
}

// ---------------------------------------------------------------- width first

/// `_width_first_tests` 10432-10474 (wfirst.builds, narrow-wraps,
/// wide-one-line, no-overshoot): a hug toast with a wrapping label is
/// measured at the width it GETS — tall in a 60-wide frame, short in a
/// 500-wide one, and the wide answer never passes through the narrow
/// height on the way (one layout call, one answer). Godot measured one
/// line at 500 with Roboto; FixedAdvance folds the 100-char text to two —
/// the pin is the relation, shorter than narrow.
#[test]
fn wrap_label_in_a_hug_toast_measures_at_the_width_it_gets() {
    let c = ok(WIDTH_FIRST_COMPONENT);
    let page = c.page;
    let rt = Runtime::new(&page);
    let narrow = lay(&page, &rt, 60.0, 400.0).1[0];
    assert!(narrow.h > 40.0, "{narrow:?}");
    assert!(narrow.w <= 60.0, "{narrow:?}");
    let wide = lay(&page, &rt, 500.0, 400.0).1[0];
    assert!(wide.h < narrow.h, "{narrow:?} -> {wide:?}");
    // No overshoot: the only height the box ever holds at 500 is the settled one.
    let held: Vec<f32> = (0..3).map(|_| lay(&page, &rt, 500.0, 400.0).1[0].h).collect();
    assert!(held.iter().all(|h| *h <= narrow.h + 0.5 && *h == wide.h), "held {held:?} while settling from {} to {}", narrow.h, wide.h);
}

// ---------------------------------------------------------------- button content kick

/// `_button_content_kick_tests` 10478-10525 (btnkick.builds, named, base,
/// grows, shrinks, remove-shrinks): a Button sized by its content follows
/// a child label's text both ways, and lets the space go when the child
/// leaves. Godot removed the node; Rust has no node removal, so the child
/// goes `gone` — the layout reads the same absence.
#[test]
fn content_sized_button_follows_its_label_both_ways() {
    let c = ok("View a {\n    Button b halign=left, valign=top {\n        Label t text=\"short\"\n    }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let t = id(&page, "t");
    let base = rect(&page, &lay(&page, &rt, 400.0, 800.0).1, "b");
    assert!(base.w > 5.0, "{base:?}");
    rt.set_text(&mut page, t, "a considerably longer caption than before");
    let grown = rect(&page, &lay(&page, &rt, 400.0, 800.0).1, "b");
    assert!(grown.w > base.w + 20.0, "{base:?} -> {grown:?}");
    rt.set_text(&mut page, t, "short");
    let back = rect(&page, &lay(&page, &rt, 400.0, 800.0).1, "b");
    assert!((back.w - base.w).abs() < 0.5, "{back:?} vs {base:?}");
    rt.set_visible(&mut page, t, false);
    let without = rect(&page, &lay(&page, &rt, 400.0, 800.0).1, "b");
    assert!(without.w < base.w, "{without:?} vs base {base:?}");
}

// ---------------------------------------------------------------- component oversize

/// `_component_oversize_tests` 10552, 10630-10643, 10665
/// (oversize.builds, seated-builds, seated-grows-past, pair-builds): the
/// hug card fixture builds, and seated in a Column its width follows a
/// 2000-character unwrapped label right past the window — content truth,
/// not a fault (Godot exempts the seated shape from its warning).
#[test]
fn seated_hug_card_grows_past_the_window_with_its_text() {
    ok(HUG_SHRINK_COMPONENT);
    let c = ok("View v {\n    Column col {\n        Panel card width=hug, height=hug, halign=left, valign=top {\n            Label words text=\"short\"\n        }\n    }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let words = id(&page, "words");
    rt.set_text(&mut page, words, &"word ".repeat(400));
    let card = rect(&page, &lay(&page, &rt, 400.0, 800.0).1, "card");
    assert!(card.w > 400.0, "{card:?}");
}

// ---------------------------------------------------------------- spacing default

/// `_spacing_default_tests` 10706-10745 (gap0.ok, row-touch, row-spacing,
/// column-touch, flow-touch, flow-spacing): D244 — no gap unless written.
/// Row/Column/Flow children touch; `spacing=6` is the only source of a
/// uniform gap, on Flow too. (Godot built the Column as a detached root;
/// Rust files root at View/Panel, so it sits in one.)
#[test]
fn stacks_and_flow_touch_unless_spacing_is_written() {
    let c = ok("View v {\nColumn host {\n    Row r0 {\n        Rectangle a0 10x10\n        Rectangle b0 10x10\n    }\n    Row r6 spacing=6 {\n        Rectangle a6 10x10\n        Rectangle b6 10x10\n    }\n    Column c0 {\n        Rectangle ca 10x10\n        Rectangle cb 10x10\n    }\n    Flow f0 width=100 {\n        Rectangle fa 10x10\n        Rectangle fb 10x10\n    }\n    Flow f6 width=100, spacing=6 {\n        Rectangle f6a 10x10\n        Rectangle f6b 10x10\n    }\n}\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, r) = lay(&page, &rt, 400.0, 800.0);
    let x_in = |name: &str, parent: &str| rect(&page, &r, name).x - rect(&page, &r, parent).x;
    let y_in = |name: &str, parent: &str| rect(&page, &r, name).y - rect(&page, &r, parent).y;
    assert_eq!(x_in("b0", "r0"), 10.0, "{r:?}");
    assert_eq!(x_in("b6", "r6"), 16.0, "{r:?}");
    assert_eq!(y_in("cb", "c0"), 10.0, "{r:?}");
    assert_eq!(x_in("fb", "f0"), 10.0, "{r:?}");
    assert_eq!(x_in("f6b", "f6"), 16.0, "{r:?}");
}

/// `_spacing_default_tests` 10748-10754 (gap0.list-ok, gap0.list-touch):
/// List rows touch too. Godot's rows come keyed (`set_items`); Rust builds `repeat capacity=N` instances (GP27) and code shows two.
#[test]
fn list_rows_touch() {
    let c = ok("View v {\n    List l {\n        empty { }\n        repeat t capacity=2 {\n            ListItem { Label text=\"row\" }\n        }\n    }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let t = id(&page, "t");
    rt.set_count(&mut page, t, 2);
    let items = rt.items(&page, t);
    assert_eq!(items.len(), 2);
    let (_, r) = lay(&page, &rt, 400.0, 800.0);
    let first = r[items[0] as usize];
    let second = r[items[1] as usize];
    assert!(first.h > 0.0, "{first:?}");
    assert_eq!(second.y, first.bottom(), "rows touch: {first:?} {second:?}");
}

// ---------------------------------------------------------------- content minimums

/// The derived minimum of every named node in `body`, read as the rect a
/// Flow gives its child (a Flow lays each child at its measured minimum on
/// both axes). `body` = the node lines, one per Flow.
fn measured(body: &[&str]) -> (Page, Vec<Rect>) {
    let mut src = String::from("View v {\n  Column c {\n");
    for line in body {
        src.push_str("    Flow {\n");
        src.push_str(line);
        src.push_str("\n    }\n");
    }
    src.push_str("  }\n}\n");
    let c = ok(&src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, r) = lay(&page, &rt, 600.0, 2000.0);
    (page, r)
}

fn size(page: &Page, r: &[Rect], name: &str) -> (f32, f32) {
    let x = rect(page, r, name);
    (x.w, x.h)
}

/// `_content_min_tests` 10763-10809 (cmin.ok, derives, exact-clamps-axis,
/// free-axis-derives, both-exact-unwired, min-merges-via-max,
/// pinned-derives, childless-unwired): WPF content sizing on a Button —
/// per axis max(declared, content); an exact width clamps its axis and
/// the other still derives; both-exact is exactly the written size; a
/// `min_*` merges by max; the button's OWN margin is its placement in its
/// parent and never joins its minimum; a childless one is its floor alone.
#[test]
fn button_minimum_is_max_of_declared_and_content_per_axis() {
    let (page, r) = measured(&[
        "Button grows {\n    Row noHit {\n        Rectangle block 120x30\n    }\n}",
        "Button clampedW 50x {\n    Rectangle innerB 120x30\n}",
        "Button fixedBoth 50x90 {\n    Rectangle innerC 120x130\n}",
        "Button merged min_width=200, min_height=10 {\n    Rectangle innerD 120x30\n}",
        "Button empty min_width=40",
    ]);
    assert_eq!(size(&page, &r, "grows"), (120.0, 30.0), "{r:?}");
    assert_eq!(size(&page, &r, "clampedW"), (50.0, 30.0), "{r:?}");
    assert_eq!(size(&page, &r, "fixedBoth"), (50.0, 90.0), "{r:?}");
    assert_eq!(size(&page, &r, "merged"), (200.0, 30.0), "{r:?}");
    assert_eq!(size(&page, &r, "empty").0, 40.0, "{r:?}");
    // pinnedBtn: margins are an error under a Flow (G1014), so it sits
    // under Layers, where its right pin is placement in the parent.
    let c = ok("View v {\n  Layers z {\n    Button pinnedBtn halign=right, valign=top, margin_right=10 {\n        Rectangle innerE 120x30\n    }\n  }\n}\n");
    let page = c.page;
    let (_, r) = lay(&page, &Runtime::new(&page), 600.0, 400.0);
    let pinned = rect(&page, &r, "pinnedBtn");
    assert_eq!((pinned.w, pinned.h), (120.0, 30.0), "{pinned:?}");
    assert_eq!(pinned.right(), 590.0, "the parent reserves the 10: {pinned:?}");
}

/// `_content_min_tests` 10817-10829 (cmin.live-grow, hidden-excluded,
/// reshown): the wiring is live — a child that grows at runtime grows the
/// button, a child that leaves the layout (Godot `visible=false`, Rust
/// `gone`) takes its whole minimum with it, and it counts again on show.
#[test]
fn button_minimum_follows_a_live_child_and_skips_a_gone_one() {
    let c = ok("View v {\n  Column col {\n    Flow {\n      Button grows {\n        Row noHit {\n          Rectangle block 120x30\n        }\n      }\n    }\n  }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let block = id(&page, "block");
    assert_eq!(size(&page, &lay(&page, &rt, 600.0, 400.0).1, "grows"), (120.0, 30.0));
    rt.set_prop(block, PropKey::Width, Value::VNum(150.0));
    rt.set_prop(block, PropKey::Height, Value::VNum(40.0));
    assert_eq!(size(&page, &lay(&page, &rt, 600.0, 400.0).1, "grows"), (150.0, 40.0));
    rt.set_visible(&mut page, block, false);
    assert_eq!(size(&page, &lay(&page, &rt, 600.0, 400.0).1, "grows"), (0.0, 0.0), "a gone child leaves no minimum behind");
    rt.set_visible(&mut page, block, true);
    assert_eq!(size(&page, &lay(&page, &rt, 600.0, 400.0).1, "grows"), (150.0, 40.0));
}

/// `_content_min_tests` 10872 (cmin.epx-ok): the epx fixture builds. Its
/// two other checks (scaled / no-ratchet) are ENGINE — see the module doc.
#[test]
fn epx_fixture_builds() {
    let (page, r) = measured(&["Button merged2 min_width=200 {\n    Rectangle inner2 120x30\n}"]);
    assert_eq!(size(&page, &r, "merged2"), (200.0, 30.0), "{r:?}");
}

/// `_content_min_tests` 10893-10908 (cmin.widget-ok, widget-derives): a
/// widget whose root is a Button derives too, with the instance's values
/// (a `$w` size param) and its own `min_height` final.
#[test]
fn widget_root_button_derives_from_instance_values() {
    let c = ok("widget IconRow $w=40 {\n    Button signals {\n        Row {\n            Rectangle box size=$w\n        }\n    }\n}\nView v {\n  Column c {\n    Flow {\n      IconRow row1 w=60x24, min_height=44\n    }\n  }\n}\n");
    let page = c.page;
    let (_, r) = lay(&page, &Runtime::new(&page), 600.0, 400.0);
    assert_eq!(size(&page, &r, "row1"), (60.0, 44.0), "{r:?}");
}

/// `_content_min_tests` 10916-10946 (cmin.margin-ok, margin-begin,
/// margin-end, margin-fill-both, margin-negative-zero, margin-layers):
/// D242 — a free child's POSITIVE margins are reserved space and count
/// toward the parent's minimum like padding; a fill child adds only its
/// margins. Layers follows the same formula. FLIP on the negative half
/// (decided 2026-08-30, docs/todo-gogo-parity.md GPAR100): a negative
/// margin is the part that hangs out on purpose and comes OFF the claim
/// (Godot counted it as zero — mNeg 120; Rust 120 - 12 = 108).
#[test]
fn free_child_margins_count_toward_the_minimum() {
    let (page, r) = measured(&[
        "Button mBegin {\n    Rectangle mbA 120x30, margin_left=15\n}",
        "Button mEnd {\n    Rectangle mbB 120x30, valign=bottom, margin_bottom=7\n}",
        "Button mFill {\n    Rectangle mbC width=fill, height=30, margin_left=10, margin_right=20\n}",
        "Button mNeg {\n    Rectangle mbD 120x30, margin_left=-12\n}",
        "Layers mLayers width=hug, height=hug {\n    Rectangle mbE 40x40, margin_left=10, margin_top=5\n}",
    ]);
    assert_eq!(size(&page, &r, "mBegin"), (135.0, 30.0), "{r:?}");
    assert_eq!(size(&page, &r, "mEnd"), (120.0, 37.0), "{r:?}");
    assert_eq!(size(&page, &r, "mFill"), (30.0, 30.0), "{r:?}");
    assert_eq!(size(&page, &r, "mNeg"), (108.0, 30.0), "the hanging 12 is not claimed: {r:?}");
    assert_eq!(size(&page, &r, "mLayers"), (50.0, 45.0), "{r:?}");
}

/// `_content_min_tests` 10952-10964 (cmin.side-padding-ok,
/// side-padding-derives): per-side padding on an inner Panel is measured
/// air — the exact width clamps x, y derives 8 + 30 + 10.
#[test]
fn inner_panel_side_padding_counts_into_the_minimum() {
    let (page, r) = measured(&["Button padRow width=60 {\n    Panel padding_top=8, padding_bottom=10 {\n        Rectangle glyph 30x30\n    }\n}"]);
    assert_eq!(size(&page, &r, "padRow"), (60.0, 48.0), "{r:?}");
}

/// `_content_min_tests` 10978-11001 (cmin.nested-ok, nested-themed,
/// nested-direct, nested-engine-container, nested-script-container): D116
/// — a container between the button and its label must not change the
/// answer: a Button holding the label directly, through a Column, and
/// through Layers + Column all measure the themed label (font 30), within
/// a pixel. (Godot's Column was the three buttons' parent; a Flow reads
/// the same minimum on both axes.)
#[test]
fn nested_container_serves_the_same_themed_minimum() {
    let c = ok("theme {\n    font_size = 30\n}\nView {\n    Flow {\n        Button direct {\n            Label a text=\"hi again\"\n        }\n        Button viaColumn {\n            Column {\n                Label b text=\"hi again\"\n            }\n        }\n        Button viaLayers {\n            Layers {\n                Column {\n                    Label c text=\"hi again\"\n                }\n            }\n        }\n    }\n}\n");
    let page = c.page;
    let (_, r) = lay(&page, &Runtime::new(&page), 600.0, 400.0);
    let want = rect(&page, &r, "a");
    assert!(want.h > 30.0, "the themed font reached the measure: {want:?}");
    let direct = rect(&page, &r, "direct");
    assert_eq!(direct.h, want.h, "{direct:?} vs {want:?}");
    for name in ["viaColumn", "viaLayers"] {
        let b = rect(&page, &r, name);
        assert!((b.h - want.h).abs() <= 1.0 && (b.w - want.w).abs() <= 1.0, "{name}: {b:?} vs {want:?}");
    }
}

/// `_content_min_tests` 11009-11014 (cmin.nested-lint-agrees): the
/// touch-target lint reports the SAME height for all three buttons (the
/// number the live tree settles on). Rust has no 44-epx finger-target lint.
#[test]
#[ignore = "GP41: not ported: the 44-epx touch-target lint (`touch_target=off` silencer)"]
fn touch_target_lint_agrees_with_the_live_minimum() {
    let c = compile("theme {\n    font_size = 30\n}\nView {\n    Column {\n        Button direct {\n            Label a text=\"hi again\"\n        }\n        Button viaColumn {\n            Column {\n                Label b text=\"hi again\"\n            }\n        }\n        Button viaLayers {\n            Layers {\n                Column {\n                    Label c text=\"hi again\"\n                }\n            }\n        }\n    }\n}\n", &|_| None);
    let mut heights: Vec<(String, i32)> = Vec::new();
    for d in c.diags.iter().filter(|d| d.severity == gogo::Severity::Warning) {
        for n in ["direct", "viaColumn", "viaLayers"] {
            if d.message.contains(&format!("`{n}`")) {
                let h: i32 = d.message.split("height ").nth(1).and_then(|s| s.split(' ').next()).and_then(|s| s.parse().ok()).unwrap_or(-1);
                heights.push((n.to_string(), h));
            }
        }
    }
    assert_eq!(heights.len(), 3, "{:#?}", c.diags);
    assert!(heights.iter().all(|(_, h)| (h - heights[0].1).abs() <= 1), "{heights:?}");
}

/// `_content_min_tests` 11018-11036 (vmin.ok, answers-live,
/// exact-axis-owned, spike-visible, transient-forgotten): a View root's
/// minimum is answered from its live children — 120×80 for one block, an
/// exact `width=300` owns its axis against a 500-wide child, a child that
/// spikes to 3742 tall shows as 3742 and leaves no trace once it settles.
/// Read as the rect a zero-size viewport leaves the root (an unwritten size
/// never goes under its content).
#[test]
fn view_root_minimum_answers_live_and_forgets_a_transient() {
    let c = ok("View {\n    Rectangle vBlock 120x80, halign=left, valign=top\n}\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    let floor = |page: &Page, rt: &Runtime| {
        let r = lay(page, rt, 0.0, 0.0).1[0];
        (r.w, r.h)
    };
    assert_eq!(floor(&page, &rt), (120.0, 80.0));
    let fixed = ok("View vFixed width=300 {\n    Rectangle vWide 500x50, halign=left, valign=top\n}\n");
    assert_eq!(floor(&fixed.page, &Runtime::new(&fixed.page)), (300.0, 50.0), "an exact axis is owned by the declared size");
    let block = id(&page, "vBlock");
    rt.set_prop(block, PropKey::Height, Value::VNum(3742.0));
    assert_eq!(floor(&page, &rt).1, 3742.0);
    rt.set_prop(block, PropKey::Height, Value::VNum(80.0));
    assert_eq!(floor(&page, &rt).1, 80.0, "a settled child leaves no trace");
    // The root's own declared floor stays 0 throughout.
    assert_eq!((resolve(&page, &rt).placement[0].min_width, resolve(&page, &rt).placement[0].min_height), (0.0, 0.0));
}

// ---------------------------------------------------------------- custom controls

/// `_custom_control_registry_tests` 11058-11064, 11069 (reg.resolver-
/// builds, resolver-type, resolver-props, create-unknown-null): a node type
/// the host registers (`CompileOptions::code_controls`) builds like any
/// element — it keeps the registered type and its layout words — while an
/// unregistered name is refused loudly (G7011), never half-built.
#[test]
fn registered_code_control_builds_and_an_unknown_name_is_refused() {
    let options = CompileOptions { code_controls: &["TestCustomControl"], ..CompileOptions::default() };
    let c = compile_with("View v {\n    TestCustomControl demo width=200\n}\n", &|_| None, options);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let demo = id(&page, "demo");
    assert_eq!(page.nodes[demo as usize].kind, Kind::Custom);
    assert_eq!(page.str(page.nodes[demo as usize].type_name), "TestCustomControl");
    let (_, r) = lay(&page, &Runtime::new(&page), 400.0, 400.0);
    assert_eq!(r[demo as usize].w, 200.0, "{r:?}");
    let unknown = compile_with("View v {\n    NoSuchControl demo width=200\n}\n", &|_| None, options);
    assert!(unknown.diags.iter().any(|d| d.code == "G7011"), "{:#?}", unknown.diags);
    assert!(gogo::has_errors(&unknown.diags));
}
