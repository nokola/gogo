//! Port of the Godot gogo suite's widget / slot / control groups
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//! - `_widget_tests` 3061-3177 (22 checks): widgets + `$params`, the
//!   `signals` surface, the bare `slot`, per-instance name lookup.
//! - `_slot_tests` 3177-3406 (28 checks): named slot regions, fills,
//!   box props, padding, minimums, nested slots, the fill error family.
//! - `_control_tests` 3406-3760 (67 checks): the `control` construct, the
//!   `automatic=` fraction pipeline, `live()`, presence, the D134 coupling
//!   grammar and the diagnostics family.
//!
//! Dialect notes (translated before compiling, per the port brief):
//! `Slider signals min=…` needs a comma after the flag here
//! (`Slider signals, min=…` — Rust's parser wants `,` between a flag and
//! the next `key=`; Godot parses the bare form — listed in the report);
//! several Godot sources put three instances at file scope — Rust has one
//! root, so they sit in a `View v { … }`; `FADER_SRC` carries a
//! `slot.thumb.…` anim lane Rust refuses (GP32) — the PORT tests use the
//! lane-less `fader_static()`, the verbatim source is its own OPEN test.
//!
//! ENGINE (not ported — Godot machinery with no Rust counterpart):
//! - `lookup.rebuild-fresh` (3164-3170): scene-tree rebuild node identity.
//! - `ctl.root-class`, `ctl.signals-real`, `ctl.focus-granted`,
//!   `ctl.driver-attached` (3412-3419): GogoControlRoot script, has_signal,
//!   focus_mode, driver Node.
//! - `ctl.drag-claims` … `ctl.drag-ends` minus the pipeline checks
//!   (3436-3460: drag-claims, drag-value-direct, drag-state-on,
//!   drag-signal, autowire-contract-signal, drag-moves, drag-ends),
//!   `ctl.disabled-blocks-input` (3513), `ctl.set-fraction-snaps` (3561),
//!   `ctl.err-d7-code` (3591): fader_behavior.gd script-backed behavior
//!   (pointer routing, contract signals, `set_fraction`, the `script`
//!   line itself). Rust controls have no attached script — code-behind is
//!   Rust code (gogo-design.md §7 "Names become code").
use gogo::diag::Diagnostic;
use gogo::draw::{Draw, draw, hit};
use gogo::input::{Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Auto, Kind, NodeId, PropKey, Rgba, Value};
use gogo::runtime::{DISABLED, Resolved, Runtime, resolve, resolve_animated};
use gogo::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn diags(src: &str) -> Vec<Diagnostic> {
    compile(src, &|_| None).diags
}

fn codes(src: &str) -> Vec<String> {
    diags(src).iter().map(|d| d.code.clone()).collect()
}

/// Every diagnostic message joined — the Godot `build_err(...)` text.
fn err_text(src: &str) -> String {
    diags(src).iter().map(|d| format!("{} {}\n", d.code, d.message)).collect()
}

fn builds(src: &str) -> bool {
    !gogo::has_errors(&diags(src))
}

fn lay(page: &Page, rt: &Runtime) -> (Resolved, Vec<Rect>) {
    let res = resolve(page, rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    (res, rects)
}

fn rects_of(page: &Page) -> Vec<Rect> {
    lay(page, &Runtime::new(page)).1
}

/// The texts the draw list shows, in paint order.
fn drawn_texts(page: &Page) -> Vec<String> {
    let rt = Runtime::new(page);
    let (res, rects) = lay(page, &rt);
    draw(page, &res, &rects).iter().filter_map(|d| if let Draw::Text { text, .. } = d { Some(page.str(*text).to_string()) } else { None }).collect()
}

fn text_of(page: &Page, id: NodeId) -> String {
    match page.prop(id, PropKey::Text) {
        Some(Value::VText { text, .. }) => page.str(text).to_string(),
        other => panic!("node {id} has no text: {other:?}"),
    }
}

fn kids(page: &Page, id: NodeId) -> Vec<NodeId> {
    page.children(id).map(|i| i as NodeId).collect()
}

fn find(page: &Page, name: &str) -> NodeId {
    page.find(name).unwrap_or_else(|| panic!("no node named {name:?} in {:?}", page.nodes.iter().map(|n| page.str(n.name)).collect::<Vec<_>>()))
}

const LABELED_SLIDER: &str = "widget LabeledSlider $label=\"\", $min=0, $max=100, $value=0 {\n Column spacing=4 {\n  Label text=$label\n  Slider signals, min=$min, max=$max, value=$value\n }\n}\n";

const LOOKUP_WIDGETS: &str = "widget MiniSlider $caption=\"\" {\n Layers root interactive=true, touch_target=off, height=40 {\n  Label captionLabel text=$caption\n  Label valueLabel\n }\n}\nwidget Outer {\n Column o {\n  MiniSlider innerInst caption=\"in\"\n }\n}\n";

/// The Godot suite's `FADER_SRC` verbatim (run_tests.gd 3383-3403).
const FADER_SRC: &str = "control Fader $value=0, $min=0, $max=100, $step=0 {\n    script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n    state_builtin hovered, focused, disabled\n    state dragging\n    signal drag_started, drag_ended\n    transition duration=0.12, easing=cubic_out\n    anim releasePop {\n        slot.thumb.translate.y from=-3, to=0, duration=0.2\n    }\n    Layers {\n        slot track width=fill, height=24 {\n            slot progress automatic=length by $value in $min..$max { Rectangle color=#4488FFFF }\n            slot thumb automatic=position by $value in $min..$max { Circle 12x12, color=#FFFFFFFF }\n        }\n        slot label { Label text=live($value) }\n    }\n}\n";

/// `FADER_SRC` without the `slot.thumb` anim lane (GP32 owes the lane).
fn fader_static() -> String {
    FADER_SRC.replace("    anim releasePop {\n        slot.thumb.translate.y from=-3, to=0, duration=0.2\n    }\n", "")
}

// ------------------------------------------------------------ _widget_tests

/// `_widget_tests` 3063-3079 (widget.ok, root-is-instance-name,
/// passthrough, param, param-override) + `slot.ok`, `slot.child-placed`
/// (3084-3097): a widget instance takes the instance's name on the body
/// root, passes placement through, substitutes `$params` with the
/// instance's overrides, and bare instance children land at the bare
/// `slot`.
#[test]
fn widget_instance_params_passthrough_and_bare_slot() {
    let c = ok(&format!("{LABELED_SLIDER}LabeledSlider sizeSlider label=\"Size\", min=1, max=300, value=80, min_width=240\n"));
    let p = &c.page;
    let root = find(p, "sizeSlider");
    assert_eq!(p.nodes[root as usize].kind, Kind::Column, "{:?}", p.nodes[root as usize]);
    assert_eq!(p.nodes[root as usize].placement.min_width, 240.0, "{:?}", p.nodes[root as usize].placement);
    let children = kids(p, root);
    assert_eq!(children.len(), 2, "{children:?}");
    let (label, slider) = (children[0], children[1]);
    assert_eq!(p.nodes[label as usize].kind, Kind::Label);
    assert_eq!(text_of(p, label), "Size");
    assert_eq!(p.nodes[slider as usize].kind, Kind::Slider);
    assert_eq!(p.prop(slider, PropKey::Max), Some(Value::VNum(300.0)), "{:?}", p.node_props(slider));
    assert_eq!(p.prop(slider, PropKey::Value), Some(Value::VNum(80.0)), "{:?}", p.node_props(slider));
    // The `signals` surface: an instance-line `on:change=` binds to the
    // Slider inside, not to the Column root.
    let c = ok(&format!("{LABELED_SLIDER}View v {{ LabeledSlider sizeSlider label=\"Size\", min=1, max=300, value=80, on:change=sizeChanged }}\n"));
    let p = &c.page;
    let slider = kids(p, find(p, "sizeSlider"))[1];
    let handlers = &p.handlers[p.nodes[slider as usize].handlers.0 as usize..(p.nodes[slider as usize].handlers.0 + p.nodes[slider as usize].handlers.1) as usize];
    assert!(handlers.iter().any(|h| p.str(h.event) == "change" && matches!(h.action, gogo::page::Action::Named(n) if p.str(n) == "sizeChanged")), "{handlers:?}");

    let c = ok("widget Card $title=\"?\" {\n Column {\n  Label text=$title\n  slot\n }\n}\nCard c title=\"Box\" { Button inner text=\"In the slot\" }\n");
    let p = &c.page;
    let card = find(p, "c");
    let children = kids(p, card);
    assert_eq!(children.len(), 2, "{children:?}");
    assert_eq!(p.nodes[children[1] as usize].kind, Kind::Button);
    assert_eq!(text_of(p, children[1]), "In the slot");
    assert_eq!(children[1], find(p, "inner"), "an instance child keeps its own (unscoped) name");
}

/// `widget.autowire-signals-surface` 3080-3084: with no `on:` written the
/// Godot host's `OnSizeSliderChange` still fires — the `signals` Slider's
/// change auto-wires under the INSTANCE name. Rust's auto-wire is "the
/// node's own name is the handler", and the Slider inside the body is
/// unnamed (the instance name went to the Column root), so the change
/// event leaves with no route.
#[test]
#[ignore = "NEW: Godot auto-wires a `signals` node's change under the instance name; Rust emits the event with action None on an unnamed node"]
fn signals_node_change_auto_wires_under_the_instance_name() {
    let c = ok(&format!("{LABELED_SLIDER}View v {{ LabeledSlider sizeSlider label=\"Size\", min=1, max=300, value=80 }}\n"));
    let p = &c.page;
    let mut rt = Runtime::new(p);
    let mut input = Input::new();
    let (res, rects) = lay(p, &rt);
    let slider = kids(p, find(p, "sizeSlider"))[1];
    let s = rects[slider as usize];
    input.touch(p, &mut rt, &res, &rects, Touch { phase: Phase::Down, x: s.x + s.w * 0.5, y: s.y + s.h * 0.5, t: 0.0, id: 1 });
    let ev = input.touch(p, &mut rt, &res, &rects, Touch { phase: Phase::Up, x: s.x + s.w * 0.5, y: s.y + s.h * 0.5, t: 0.1, id: 1 });
    let change = ev.iter().find(|e| matches!(e.kind, gogo::EventKind::Change(_))).unwrap_or_else(|| panic!("{ev:?}"));
    let routable = change.action.is_some() || p.str(p.nodes[change.node as usize].name) == "sizeSlider";
    assert!(routable, "the change must reach a handler named after the instance: {change:?}, node name {:?}", p.str(p.nodes[change.node as usize].name));
}

/// `widget.cycle` 3098-3102: two widgets that instance each other are a
/// build error naming the cycle (G5003). Before the guard the compiler
/// overflowed its stack expanding A -> B -> A forever
/// (STATUS_STACK_OVERFLOW observed 2026-08-30) — `build.rs` died with no
/// message. Control: the same widget used twice, and filling its OWN
/// slot from an instance site, are not cycles.
#[test]
fn widget_cycle_is_a_build_error() {
    let text = err_text("widget A { Column { B x } }\nwidget B { Column { A y } }\nA root\n");
    assert!(text.contains("cycle") && text.contains("A -> B -> A"), "{text}");
    ok("widget W { Column { slot } }\nView v { Column c { W a { Label p text=\"a\" }; W b { W inner { Label q text=\"b\" } } } }");
}

/// `widget.no-slot-children` 3103-3106: children on an instance of a
/// widget with no `slot` have nowhere to go — Godot says so. Rust drops
/// them silently (the Column builds with zero children).
#[test]
#[ignore = "GP41: not ported: slot-fill diagnostics (instance children with no slot are dropped silently)"]
fn instance_children_with_no_slot_are_refused() {
    let text = err_text("widget W { Column }\nW w { Button b }\n");
    assert!(text.contains("no slot"), "{text}");
}

/// `_widget_tests` lookup family 3111-3145 (lookup.ok, inner,
/// per-instance, flat-unchanged, unknown-inner, unknown-instance,
/// nested-one-level): body names live under the instance as
/// `instance/name`, one map per instance, the flat instance name is
/// unchanged, and a missing half is a miss. (Godot: `Gogo.resolve_named`;
/// Rust: `Page::find`.) The three instances sit in a `View` here — Rust
/// has one root per file.
#[test]
fn widget_body_names_resolve_per_instance() {
    let c = ok(&format!("{LOOKUP_WIDGETS}View v {{\n MiniSlider a caption=\"A\"\n MiniSlider b caption=\"B\"\n Outer out\n}}\n"));
    let p = &c.page;
    let av = find(p, "a/valueLabel");
    let bv = find(p, "b/valueLabel");
    assert_eq!(p.nodes[av as usize].kind, Kind::Label);
    assert_eq!(p.nodes[bv as usize].kind, Kind::Label);
    assert_ne!(av, bv);
    assert_eq!(text_of(p, find(p, "a/captionLabel")), "A");
    assert_eq!(text_of(p, find(p, "b/captionLabel")), "B");
    let a = find(p, "a");
    assert_eq!(p.nodes[a as usize].kind, Kind::Layers, "the flat instance name is the body root");
    assert_eq!(p.nodes[av as usize].parent, a);
    assert_eq!(p.find("a/nosuch"), None);
    assert_eq!(p.find("nosuch/valueLabel"), None);
    // Nested: the inner INSTANCE root is one level down, its internals
    // behind its own prefix.
    let inner = find(p, "out/innerInst");
    assert_eq!(p.nodes[inner as usize].kind, Kind::Layers);
    assert_eq!(p.nodes[inner as usize].parent, find(p, "out"));
    let inner_caption = find(p, "out/innerInst/captionLabel");
    assert_eq!(text_of(p, inner_caption), "in");
    assert_eq!(p.nodes[inner_caption as usize].parent, inner);
}

/// `lookup.root-self` 3134: Godot resolves the body root's OWN name
/// (`a/root`) to the instance itself. Rust replaces the body root's name
/// with the instance name, so `a/root` is a miss — and a widget's own
/// `anim` cannot target its root by that name either (`root.opacity` is
/// G0060; `self.` is the only spelling).
#[test]
#[ignore = "NEW: Godot `a/root` is the instance; Rust drops the body root's own name (find = None, `root.x` in a widget anim = G0060)"]
fn body_root_own_name_is_the_instance() {
    let c = ok(&format!("{LOOKUP_WIDGETS}View v {{ MiniSlider a caption=\"A\" }}\n"));
    assert_eq!(c.page.find("a/root"), c.page.find("a"), "{:?}", c.page.nodes.iter().map(|n| c.page.str(n.name)).collect::<Vec<_>>());
}

/// `lookup.two-hops-miss` 3144: Godot documents `out/innerInst/captionLabel`
/// as a miss (one level per map). Rust names the node exactly that and
/// resolves it — the Rust side is the more useful one; recorded so the
/// difference is a decision, not an accident.
#[test]
#[ignore = "NEW: Godot `out/innerInst/captionLabel` is a documented miss; Rust resolves it (full `a/b/c` names)"]
fn two_hop_lookup_is_a_miss() {
    let c = ok(&format!("{LOOKUP_WIDGETS}View v {{ Outer out }}\n"));
    assert_eq!(c.page.find("out/innerInst/captionLabel"), None, "{:?}", c.page.nodes.iter().map(|n| c.page.str(n.name)).collect::<Vec<_>>());
}

/// `lookup.view-ok`, `lookup.view-inner` 3150-3159: the `lookup_view.gogo`
/// fixture (inlined) builds and `row/valueLabel` is the Label inside the
/// `row` instance. (`lookup.rebuild-fresh` is ENGINE — see the module doc.)
#[test]
fn lookup_view_fixture_names_its_inner_label() {
    let c = ok("widget MiniSlider $caption=\"\" {\n Layers root interactive=true, touch_target=off, height=40 {\n  Label captionLabel text=$caption\n  Label valueLabel\n }\n}\n\nView visibility=visible {\n Column {\n  MiniSlider row caption=\"Size\"\n }\n}\n");
    let p = &c.page;
    let v = find(p, "row/valueLabel");
    assert_eq!(p.nodes[v as usize].kind, Kind::Label);
    assert_eq!(p.nodes[v as usize].parent, find(p, "row"));
}

// -------------------------------------------------------------- _slot_tests

/// `_slot_tests` 3187-3208 (slotn.ok, region-node, region-passive,
/// default-content, default-slot-inline): a named slot with default
/// content builds as a REGION node (kind Slot, named `inst/slot`) that
/// clips its content and is transparent to input; bare instance children
/// still splice inline at the bare `slot`.
#[test]
fn named_slot_is_a_clipped_passive_region_holding_its_default() {
    let c = ok("widget Card {\n Column {\n  slot header { Label headerLabel text=\"Untitled\" }\n  slot\n }\n}\nCard c { Label text=\"body\" }\n");
    let p = &c.page;
    let root = find(p, "c");
    let children = kids(p, root);
    assert_eq!(children.len(), 2, "{children:?}");
    let header = children[0];
    assert_eq!(p.nodes[header as usize].kind, Kind::Slot, "{:?}", p.nodes[header as usize]);
    assert_eq!(header, find(p, "c/header"));
    let inside = kids(p, header);
    assert_eq!(inside.len(), 1, "{inside:?}");
    assert_eq!(p.nodes[inside[0] as usize].kind, Kind::Label);
    assert_eq!(text_of(p, inside[0]), "Untitled");
    assert_eq!(p.nodes[children[1] as usize].kind, Kind::Label);
    assert_eq!(text_of(p, children[1]), "body");
    // Passive + clipped: a touch over the region falls through; the draw
    // list clips to the region's rect.
    let rt = Runtime::new(p);
    let (res, rects) = lay(p, &rt);
    let h = rects[header as usize];
    assert_eq!(hit(p, &res, &rects, h.x + 1.0, h.y + 1.0, false), None);
    let list = draw(p, &res, &rects);
    assert!(list.iter().any(|d| matches!(d, Draw::ClipPush(r) if *r == h)), "{list:#?}");
}

/// `slotn.fill-ok`, `slotn.fill-replaces` 3211-3227: a `slot header { … }`
/// fill on the instance replaces the default content entirely.
#[test]
fn a_fill_replaces_the_default_content() {
    let c = ok("widget Card2 {\n Column {\n  slot header { Label text=\"Untitled\" }\n  slot\n }\n}\nCard2 c {\n slot header { Label text=\"Run stats\" }\n Label text=\"body\"\n}\n");
    let p = &c.page;
    let header = find(p, "c/header");
    let inside = kids(p, header);
    assert_eq!(inside.len(), 1, "{inside:?}");
    assert_eq!(text_of(p, inside[0]), "Run stats");
}

/// `slotn.box-ok`, `slotn.box-props` 3231-3246: the box family on a slot
/// line (background, corner_radius, padding, width/height) lands as the
/// region's own props.
#[test]
fn slot_line_box_props_land_on_the_region() {
    let c = ok("widget Track {\n Layers {\n  slot track width=fill, height=4, background=#332211FF, corner_radius=2, padding=3\n }\n}\nTrack t\n");
    let p = &c.page;
    let track = find(p, "t/track");
    assert_eq!(p.prop(track, PropKey::Background), Some(Value::VColor(Rgba([0x33, 0x22, 0x11, 0xFF]))), "{:?}", p.node_props(track));
    assert_eq!(p.prop(track, PropKey::CornerRadius), Some(Value::VNum(2.0)), "{:?}", p.node_props(track));
    let pl = p.nodes[track as usize].placement;
    assert_eq!(pl.padding, [3.0; 4], "{pl:?}");
    assert_eq!(pl.height, gogo::page::Len::Exact(4.0), "{pl:?}");
}

/// `slotn.content-prop-routed` 3247-3253: a CONTENT prop on a slot line
/// (`font_size=12`) is refused — "restyling content IS filling". Rust
/// accepts it without a word (the region carries a dead font_size).
#[test]
#[ignore = "GP41: not ported: slot-fill diagnostics (content prop on a slot line accepted silently)"]
fn content_prop_on_a_slot_line_is_refused() {
    let text = err_text("widget W3 {\n Column { slot label font_size=12 }\n}\nW3 w\n");
    assert!(text.contains("restyling content IS filling"), "{text}");
}

/// `slotn.automatic-needs-control` 3254-3257: `automatic=` belongs to a
/// `control` body, not a plain `widget`. Rust treats `control` as a
/// synonym of `widget` and accepts a complete coupling in either (the
/// Godot snippet's `automatic=length` alone is refused here too, but for
/// the missing `by … in …` — G5107 — so the fixture spells the clauses).
#[test]
#[ignore = "GP41: not ported: `automatic=` on a widget (not a control) is accepted (Rust does not distinguish the two words)"]
fn automatic_needs_a_control() {
    let text = err_text("widget W4 $v=1, $lo=0, $hi=2 { Column { slot p automatic=length by $v in $lo..$hi { Rectangle 10x10 } } }\nW4 w\n");
    assert!(text.contains("needs a `control`"), "{text}");
}

/// `slotn.pad-ok`, `slotn.pad-insets` 3260-3274: `padding=8` on a slot
/// line insets a fill-axis child by 8 on every side.
#[test]
fn slot_padding_insets_the_fill_child() {
    let c = ok("widget Pad {\n Layers {\n  slot area padding=8 { Rectangle inner color=#FFFFFFFF }\n }\n}\nPad p\n");
    let p = &c.page;
    let rects = rects_of(p);
    let area = rects[find(p, "p/area") as usize];
    let inner = rects[find(p, "p/inner") as usize];
    assert_eq!((inner.x, inner.right()), (area.x + 8.0, area.right() - 8.0), "area {area:?} inner {inner:?}");
    assert_eq!((inner.y, inner.bottom()), (area.y + 8.0, area.bottom() - 8.0), "area {area:?} inner {inner:?}");
}

/// `slotn.min-ok`, `slotn.min-derives` 3278-3291: a slot region's minimum
/// is its content plus padding (20x10 + 6 all round = 32x22), so a slot
/// in a Column does not collapse. Godot reads `get_minimum_size()`; here
/// the instance is aligned left/top so its Column hugs and the laid-out
/// rect IS the minimum.
#[test]
fn slot_minimum_is_content_plus_padding() {
    let c = ok("widget MinCard {\n Column {\n  slot icon padding=6 { Rectangle 20x10, color=#FFFFFFFF }\n }\n}\nView v { MinCard m halign=left, valign=top }\n");
    let p = &c.page;
    let rects = rects_of(p);
    let icon = rects[find(p, "m/icon") as usize];
    assert_eq!((icon.w, icon.h), (32.0, 22.0), "{rects:?}");
    let m = rects[find(p, "m") as usize];
    assert_eq!((m.w, m.h), (32.0, 22.0), "{rects:?}");
}

/// `slotn.nested-ok` 3310 (PORT: it builds) and `slotn.nested-survives`,
/// `slotn.fill-below-nested`, `slotn.nested-keeps-default` 3311-3321
/// (NEW): a slot declared INSIDE another slot's default content survives
/// a fill of the outer slot and draws above it — Godot: track children
/// = [tick1, tick2, slot thumb (with its Circle)]. Rust's fill clears the
/// declaration's children wholesale: track = [tick1, tick2], the `thumb`
/// slot is gone.
#[test]
#[ignore = "NEW: Godot keeps a nested slot declaration through a fill of its parent slot (3 children, thumb last); Rust drops it (2 children, no `b/thumb`)"]
fn nested_slot_declaration_survives_a_fill() {
    let c = ok("widget Bar {\n Layers {\n  slot track height=10 {\n   Rectangle bg color=#111111FF\n   slot thumb { Circle 8x8, color=#FFFFFFFF }\n  }\n }\n}\nBar b {\n slot track {\n  Rectangle tick1 2x8, color=#888888FF\n  Rectangle tick2 2x8, color=#888888FF\n }\n}\n");
    let p = &c.page;
    let track = find(p, "b/track");
    let children = kids(p, track);
    assert_eq!(children.len(), 3, "{:?}", children.iter().map(|i| (p.nodes[*i as usize].kind, p.str(p.nodes[*i as usize].name))).collect::<Vec<_>>());
    assert_eq!(children[0], find(p, "tick1"));
    assert_eq!(children[1], find(p, "tick2"));
    let thumb = children[2];
    assert_eq!(p.nodes[thumb as usize].kind, Kind::Slot);
    assert_eq!(thumb, find(p, "b/thumb"));
    assert_eq!(kids(p, thumb).len(), 1, "the nested slot keeps its default Circle");
}

/// `slotn.nested-ok` 3310 alone: the nested-slot source builds.
#[test]
fn nested_slot_source_builds() {
    ok("widget Bar {\n Layers {\n  slot track height=10 {\n   Rectangle bg color=#111111FF\n   slot thumb { Circle 8x8, color=#FFFFFFFF }\n  }\n }\n}\nBar b {\n slot track {\n  Rectangle tick1 2x8, color=#888888FF\n  Rectangle tick2 2x8, color=#888888FF\n }\n}\n");
}

/// `slotn.fill-instance-scope` 3324-3329: fill content is written in the
/// INSTANCE's scope — a widget `$param` is out of reach and the error
/// names it. Rust: G6001 "`$title` is not a theme value or widget
/// parameter".
#[test]
fn fill_content_cannot_see_the_widget_params() {
    let text = err_text("widget Card3 $title=\"t\" {\n Column { slot header { Label text=\"d\" } }\n}\nCard3 c { slot header { Label text=$title } }\n");
    assert!(text.contains("G6001") && text.contains("title"), "{text}");
}

/// `slotn.err-duplicate-decl` 3341-3349: two `slot a` declarations in one
/// body are refused. Rust: the region names collide — G0044 "node name
/// `w/a` is used twice".
#[test]
fn duplicate_slot_declaration_is_refused() {
    let text = err_text("widget W5 {\n Column {\n  slot a\n  slot a\n }\n}\nW5 w\n");
    assert!(text.contains("G0044") && text.contains("w/a"), "{text}");
}

/// `slotn.err-unknown-fill` 3333-3340 (D5: the slot list IS the fix),
/// `slotn.err-two-defaults` 3350, `slotn.err-filled-twice` 3359,
/// `slotn.err-fill-props` 3363, `slotn.err-bare-wrapper` 3367,
/// `slotn.err-no-default` 3371-3376, `slotn.err-zero-slots` 3378-3381:
/// the fill error family. Rust today: an unknown fill name, a second
/// default slot, a second fill of one slot (last wins), props on a fill
/// (dropped), a `slot { }` wrapper (becomes an unnamed Slot region), and
/// stray children with no default slot (dropped) all build silently.
#[test]
#[ignore = "GP41: not ported: slot-fill diagnostics (unknown fill, two defaults, filled twice, fill props, bare wrapper, no default, zero slots all build silently)"]
fn slot_fill_error_family() {
    let d5 = err_text("widget Gauge {\n Column { slot label { Label text=\"v\" }  slot }\n}\nGauge g { slot lable { Label text=\"x\" } }\n");
    assert!(d5.contains("no slot 'lable' on Gauge") && d5.contains("its slots: label"), "{d5}");
    assert!(err_text("widget W6 {\n Column {\n  slot\n  slot\n }\n}\nW6 w\n").contains("at most one default slot"));
    assert!(err_text("widget W7 { Column { slot a } }\nW7 w { slot a { Label text=\"1\" }  slot a { Label text=\"2\" } }\n").contains("filled twice"));
    assert!(err_text("widget W8 { Column { slot a } }\nW8 w { slot a height=4 { Label text=\"x\" } }\n").contains("a fill is content"));
    assert!(err_text("widget W9 { Column { slot } }\nW9 w { slot { Label text=\"x\" } }\n").contains("drop the `slot { }` wrapper"));
    let nodefault = err_text("widget W10 { Column { slot header { Label text=\"h\" } } }\nW10 w { Label text=\"stray\" }\n");
    assert!(nodefault.contains("has no default slot") && nodefault.contains("slot header"), "{nodefault}");
    assert!(err_text("widget W11 { Column }\nW11 w { Label text=\"stray\" }\n").contains("no slot — instance children have nowhere to go"));
}

// ----------------------------------------------------------- _control_tests

/// `ctl.ok` 3409: the Godot `FADER_SRC` builds verbatim. Rust refuses its
/// `slot.thumb.translate.y` anim lane (G0063 — the lane reads as a node
/// named `slot`).
#[test]
#[ignore = "GP32: not ported: `slot.<name>.<channel>` anim lanes inside a control body (G0063 today)"]
fn fader_source_builds_verbatim() {
    ok(&format!("{FADER_SRC}Fader gain 200x24, value=25\n"));
}

/// `ctl.param-live-read`, `ctl.live-initial` 3415-3417 and
/// `ctl.fraction-initial` 3421-3423: `value=25` on the instance reaches
/// the `live($value)` label as "25" and the two couplings as a 0.25
/// fraction — the length slot is a quarter of the 200 track (50), the
/// position slot sits a quarter along the free run (200 - 12) * 0.25 = 47.
#[test]
fn value_param_reaches_the_label_and_the_slot_fractions() {
    let c = ok(&format!("{}Fader gain 200x24, value=25\n", fader_static()));
    let p = &c.page;
    let progress = find(p, "gain/progress");
    let thumb = find(p, "gain/thumb");
    assert_eq!(p.nodes[progress as usize].auto, Auto::Length(0.25));
    assert_eq!(p.nodes[thumb as usize].auto, Auto::Position(0.25));
    let rects = rects_of(p);
    let track = rects[find(p, "gain/track") as usize];
    assert_eq!((track.x, track.w), (0.0, 200.0), "{rects:?}");
    assert_eq!(rects[progress as usize].w, 50.0, "{rects:?}");
    assert_eq!(rects[thumb as usize].x, 47.0, "{rects:?}");
    assert_eq!(rects[thumb as usize].w, 12.0, "{rects:?}");
    let texts = drawn_texts(p);
    assert!(texts.iter().any(|t| t.starts_with("25")), "{texts:?}");
}

/// `ctl.drag-thumb-follows` 3442, `ctl.live-follows-drag` 3443,
/// `ctl.live-immediate` 3462, `ctl.glide-arms` 3463, `ctl.glide-lands`
/// 3465: a write of `$value` moves the couplings — the label at once, the
/// fractions gliding by the control's default feel (0.12 s) and landing.
/// Written against the closest Rust surface (`Runtime::set_prop` on the
/// instance root); the fraction is solved once at compile time today.
#[test]
#[ignore = "GP33: not ported: `automatic=` slots driven live by a value write (fraction is compile-time)"]
fn value_write_moves_the_couplings_with_a_glide() {
    let c = ok(&format!("{}Fader gain 200x24, value=25\n", fader_static()));
    let mut page = c.page;
    let root = find(&page, "gain");
    let progress = find(&page, "gain/progress");
    let thumb = find(&page, "gain/thumb");
    let mut rt = Runtime::new(&page);
    rt.set_prop(root, PropKey::Value, Value::VNum(75.0));
    let res = resolve_animated(&page, &mut rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let texts: Vec<String> = draw(&page, &res, &rects).iter().filter_map(|d| if let Draw::Text { text, .. } = d { Some(page.str(*text).to_string()) } else { None }).collect();
    assert!(texts.iter().any(|t| t.starts_with("75")), "the label follows at once: {texts:?}");
    assert!(rt.animating(), "the fraction glides (0.12 s default feel)");
    rt.tick(&mut page, 0.2);
    assert!(!rt.animating(), "past the 0.12 s rule the glide has landed");
    let res = resolve_animated(&page, &mut rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    assert!((rects[progress as usize].w - 150.0).abs() < 1e-3, "{rects:?}");
    assert!((rects[thumb as usize].x - 141.0).abs() < 1e-3, "{rects:?}");
}

/// `ctl.key-increment` 3468-3474: a RIGHT key on the focused control steps
/// `$value` by one hundredth of the span (0 → 1). Rust has no key input
/// surface yet.
#[test]
#[ignore = "GP26: not ported: keyboard focus and key events (no key input surface)"]
fn key_right_increments_the_value() {
    let c = ok(&format!("{}Fader gain 200x24, value=0\n", fader_static()));
    let _ = c;
    panic!("GP26: no key input API — Godot: KEY_RIGHT on the focused Fader sets value 0 → 1");
}

/// `ctl.styled-ok` 3485: an instance with `when dragging` / `when disabled`
/// lines builds.
#[test]
fn styled_fader_instance_builds() {
    ok(&format!("{}View v {{ Fader vol 200x24, value=0 {{\n when dragging opacity=0.4\n when disabled opacity=0.1\n}} }}\n", fader_static()));
}

/// `ctl.custom-when-applies` 3497: a `when dragging` on the instance
/// (dragging = a custom `state` the control declares) styles the root
/// while the state holds. Rust has no custom state words — and the
/// instance-line `when` blocks are not kept at all (page.whens is empty).
#[test]
#[ignore = "GP37: not ported: custom `state <name>` words on a control (instance `when dragging` has no state to bind to)"]
fn custom_state_when_applies_while_dragging() {
    let c = ok(&format!("{}View v {{ Fader vol 200x24, value=0 {{\n when dragging opacity=0.4\n}} }}\n", fader_static()));
    assert_eq!(c.page.whens.len(), 1, "the instance's `when dragging` must exist as a when: {:?}", c.page.whens);
    panic!("GP37: no custom-state write — Godot: press on the track → dragging → root opacity 0.4");
}

/// `ctl.disabled-beats-custom` 3500-3501 (its expressible half): a
/// `when disabled opacity=0.1` written ON THE INSTANCE LINE applies once
/// the instance is disabled. Rust drops every `when` block written under
/// a widget instance (compile.rs expands the body and skips the
/// instance's `when`/`transition` items) — `page.whens` is empty and the
/// disabled opacity stays the default dim.
#[test]
fn instance_line_when_disabled_applies() {
    let c = ok("widget W { Panel p width=10, height=10, opacity=1 }\nView v { W w { when disabled opacity=0.1 } }\n");
    let p = &c.page;
    let w = find(p, "w");
    let mut rt = Runtime::new(p);
    rt.set_state(w, DISABLED, true);
    let res = resolve(p, &rt);
    assert_eq!(res.num(p, w, PropKey::Opacity), Some(0.1), "whens {:?}", p.whens);
}

/// `ctl.presence-ok`, `ctl.presence-initial-off` 3529-3532: a
/// `automatic=presence by marked` slot builds and, with the state off,
/// is gone from layout (nothing drawn for it).
#[test]
fn presence_slot_is_gone_while_its_state_is_off() {
    let c = ok("control Marker $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n state marked\n Layers {\n  slot track height=10 {\n   slot mark automatic=presence by marked { Circle 6x6, color=#FFFFFFFF }\n  }\n }\n}\nMarker m 100x10\n");
    let p = &c.page;
    let mark = find(p, "m/mark");
    assert_eq!(p.nodes[mark as usize].auto, Auto::Presence(false));
    let rt = Runtime::new(p);
    let (res, rects) = lay(p, &rt);
    assert!(res.gone(p, mark));
    let list = draw(p, &res, &rects);
    assert!(!list.iter().any(|d| matches!(d, Draw::Circle { .. })), "{list:#?}");
}

/// `ctl.presence-on`, `ctl.presence-read`, `ctl.presence-off` 3533-3537:
/// raising the `marked` state shows the mark, the state reads back, and
/// clearing it hides the mark again. Rust has no custom-state write.
#[test]
#[ignore = "GP33: not ported: `automatic=presence` driven by a state at runtime (and GP37: no custom state write/read)"]
fn presence_follows_the_state() {
    let c = ok("control Marker $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n state marked\n Layers {\n  slot track height=10 {\n   slot mark automatic=presence by marked { Circle 6x6, color=#FFFFFFFF }\n  }\n }\n}\nMarker m 100x10\n");
    let _ = c;
    panic!("GP33/GP37: no custom-state write — Godot: behavior_set_state(marked, true) → mark visible, read back true; false → hidden");
}

/// `ctl.live-default-round` 3542, `ctl.live-round-step` 3548 (PORT: both
/// build) and `ctl.live-default-round-text` 3545,
/// `ctl.live-round-step-text` 3551 (NEW): a float shown through `live()`
/// prints at 0.1 by default ("10.2"), `live(round($value, 1))` picks the
/// step ("11"). Rust prints the raw float ("10.234235") and `round(...)`
/// is an identity ("10.6").
#[test]
#[ignore = "NEW: Godot live() shows floats at 0.1 and honors round($v, step) (\"10.2\", \"11\"); Rust prints the raw float and ignores round (\"10.234235\", \"10.6\")"]
fn live_float_display_rounding() {
    let c = ok(&format!("{}Fader fr 100x24, value=10.234235\n", fader_static()));
    let texts = drawn_texts(&c.page);
    assert_eq!(texts, vec!["10.2".to_string()], "{texts:?}");
    let c = ok(&format!("{}Fader fr 100x24, value=10.6\n", fader_static().replace("live($value)", "live(round($value, 1))")));
    let texts = drawn_texts(&c.page);
    assert_eq!(texts, vec!["11".to_string()], "{texts:?}");
}

/// `ctl.live-default-round` 3542, `ctl.live-round-step` 3548 alone: the
/// two sources build and show a number.
#[test]
fn live_and_live_round_sources_build() {
    let c = ok(&format!("{}Fader fr 100x24, value=10.234235\n", fader_static()));
    assert_eq!(drawn_texts(&c.page).len(), 1);
    let c = ok(&format!("{}Fader fr 100x24, value=10.6\n", fader_static().replace("live($value)", "live(round($value, 1))")));
    assert_eq!(drawn_texts(&c.page).len(), 1);
}

/// `ctl.err-bare-round` 3553-3555: `round($value, 0.1)` outside `live()`
/// is refused ("goes inside live()"). Rust accepts it as an identity.
#[test]
#[ignore = "NEW: Godot refuses a bare round($value, 0.1) outside live(); Rust accepts it silently (identity)"]
fn bare_round_outside_live_is_refused() {
    let text = err_text(&format!("{}Fader fr 100x24\n", fader_static().replace("live($value)", "round($value, 0.1)")));
    assert!(text.contains("goes inside live()"), "{text}");
}

/// `ctl.set-fraction-ok` 3558: the `step=5` instance builds
/// (`ctl.set-fraction-snaps` is ENGINE: a script API).
#[test]
fn stepped_fader_builds() {
    ok(&format!("{}Fader fs 200x24, step=5\n", fader_static()));
}

/// `ctl.err-fill-width-thumb` 3570-3582: a `width=fill` child in a
/// position slot with no width of its own vanished silently (user-caught)
/// — Godot refuses it at build ("takes its width from its content"). Rust
/// still lays the slot AND the Circle out at width 0.
#[test]
#[ignore = "NEW: Godot refuses width=fill content in an unsized position slot; Rust lays the slot and its Circle out 0 wide, silently"]
fn fill_width_content_in_a_position_slot_is_refused() {
    let src = "control BadThumb $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n Layers {\n  slot track height=24 {\n   slot thumb automatic=position by $value in $min..$max { Circle width=fill, height=fill, color=#FFFFFFFF }\n  }\n }\n}\nBadThumb b 100x24\n";
    let text = err_text(src);
    assert!(text.contains("takes its width from its content"), "{text}; rects {:?}", rects_of(&compile(src, &|_| None).page));
}

/// `ctl.fill-width-with-slot-width` 3583-3592: the same content is fine
/// when the slot line sizes itself (`width=20`) — and the Circle then
/// fills those 20.
#[test]
fn fill_width_content_is_fine_in_a_sized_position_slot() {
    let c = ok("control OkThumb $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n Layers {\n  slot track height=24 {\n   slot thumb width=20, automatic=position by $value in $min..$max { Circle width=fill, height=fill, color=#FFFFFFFF }\n  }\n }\n}\nOkThumb b 100x24\n");
    let p = &c.page;
    let rects = rects_of(p);
    let thumb = find(p, "b/thumb");
    assert_eq!(rects[thumb as usize].w, 20.0, "{rects:?}");
    assert_eq!(rects[kids(p, thumb)[0] as usize].w, 20.0, "{rects:?}");
}

/// `ctl.err-d9` 3596-3603: `state hover` names a built-in — it must be
/// spelled `state_builtin`. Rust keeps no control state words.
#[test]
#[ignore = "GP37: not ported: control `state`/`state_builtin` words (a built-in name under `state` is not checked)"]
fn builtin_name_under_state_is_refused() {
    let text = err_text("control C1 $value=0 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n state hover\n Layers { }\n}\nC1 c\n");
    assert!(text.contains("state_builtin"), "{text}");
}

/// `ctl.err-d17-code`, `ctl.err-d17-fixit` 3604-3610: Godot refuses a
/// bare `text=$value` in a control body (G5103 "freeze", fixit →
/// `live($value)`). FLIP — gogo-design.md §7 "Code-behind values":
/// `live()` is not a FanRust word, every `$ref` is dereferenced at
/// resolve, so `text=$value` is the plain spelling: it builds clean and
/// shows the value. (todo-gogo.md Decisions 2026-08-29 `export $x`: "every
/// `$ref` is live in FanRust".)
#[test]
fn bare_param_ref_is_live_by_decision() {
    let src = format!("{}Fader f 100x24\n", fader_static().replace("text=live($value)", "text=$value"));
    let c = ok(&src);
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
    assert!(!codes(&src).contains(&"G5103".to_string()));
    assert_eq!(drawn_texts(&c.page), vec!["0".to_string()]);
}

/// `ctl.err-live-nonparam` 3611-3613, `ctl.err-live-outside` 3614-3615:
/// Godot allows `live()` only around a contract prop inside a control's
/// default tree. FLIP — gogo-design.md §7: `live()` is accepted as a
/// Godot form and means nothing extra (a `$ref` is always live), so
/// `live($theme_value)` and a `live()` outside any control both build;
/// an UNDEFINED `$ref` inside it is the ordinary G6001.
#[test]
fn live_is_a_plain_ref_read_anywhere() {
    let c = ok(&format!("$accent = #FF0000FF\n{}Fader f 100x24\n", fader_static().replace("live($value)", "live($accent)")));
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
    let c = ok("$x = \"five\"\nView v { Label l text=live($x) }");
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
    assert_eq!(drawn_texts(&c.page), vec!["five".to_string()]);
    assert!(codes(&format!("{}Fader f 100x24\n", fader_static().replace("live($value)", "live($accent)"))).contains(&"G6001".to_string()));
    assert!(codes("View v { Label l text=live($x) }").contains(&"G6001".to_string()));
}

/// `ctl.err-bind-redirect` 3616-3618: `bind($value)` is refused naming
/// `live($value)`. Rust compiles an unknown call to a `VCall` the Label
/// cannot show — the label draws NOTHING, with no diagnostic.
#[test]
#[ignore = "NEW: Godot refuses bind($value) naming live($value); Rust accepts any unknown call in text= and the label draws nothing"]
fn unknown_call_in_text_is_refused() {
    let src = format!("{}Fader f 100x24\n", fader_static().replace("live($value)", "bind($value)"));
    let text = err_text(&src);
    assert!(text.contains("live($value)"), "{text}; drawn {:?}", drawn_texts(&compile(&src, &|_| None).page));
}

/// `ctl.err-relayout-local` 3619-3628 (G5105: a position slot needs a free
/// host — a Column re-lays out) and `ctl.err-two-writers` 3717-3721
/// (G4003: `live()` and a `when` on one prop). Both fences are listed as
/// untracked in GPAR62; Rust builds both silently.
#[test]
fn relayout_and_two_writer_fences() {
    assert!(codes("control Bad $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n Column {\n  slot p automatic=position by $value in $min..$max { Circle 6x6 }\n }\n}\nBad b\n").contains(&"G5105".to_string()));
    let text = err_text(&format!("{}Fader f 100x24\n", fader_static().replace("Label text=live($value)", "Label text=\"x\", opacity=live($value) { when hovered opacity=0.5 }")));
    assert!(text.contains("two writers"), "{text}");
}

/// `ctl.err-feel-endpoints` 3629-3636: a control body's default feel is
/// the bare `transition duration=…` — one with endpoints
/// (`any -> dragging`) is refused. Rust keeps no control feel.
#[test]
#[ignore = "GP37: not ported: control body words (default-feel `transition` with endpoints is not checked)"]
fn default_feel_takes_no_endpoints() {
    let text = err_text("control C2 $value=0 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n transition any -> dragging duration=0.1\n Layers { }\n}\nC2 c\n");
    assert!(text.contains("default feel"), "{text}");
}

/// `ctl.err-unknown-kind` 3637-3643, `d134.phase-gone` 3691-3694,
/// `d134.presence-call-form` 3665-3672, `d134.by-missing` 3647,
/// `d134.in-missing` 3650: the `automatic=` set is closed and the coupling
/// spells `by` and `in` — an unknown kind, the retired `phase`, the call
/// form `presence(marked)`, a missing `by` and a missing `in` are all
/// refused (Rust: G5107 for each).
#[test]
fn automatic_kind_set_is_closed_and_the_coupling_is_spelled() {
    assert!(codes("control C3 $value=0 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n Layers { slot p automatic=wibble { } }\n}\nC3 c\n").contains(&"G5107".to_string()));
    assert!(codes(&format!("{}Fader f 100x24\n", fader_static().replace("automatic=length by $value in $min..$max", "automatic=phase by $value in $min..$max"))).contains(&"G5107".to_string()));
    assert!(codes("control M2 $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n state marked\n Layers { slot mark automatic=presence(marked) { Circle 6x6 } }\n}\nM2 m\n").contains(&"G5107".to_string()));
    let by_missing = err_text(&format!("{}Fader f 100x24\n", fader_static().replace(" by $value in $min..$max", "")));
    assert!(by_missing.contains("G5107") && by_missing.contains("by $value in $min..$max"), "{by_missing}");
    let in_missing = err_text(&format!("{}Fader f 100x24\n", fader_static().replace(" in $min..$max", "")));
    assert!(in_missing.contains("G5107"), "{in_missing}");
}

/// `d134.by-order` 3651, `d134.by-non-value` 3656, `d134.in-wrong-range`
/// 3659, `d134.in-bare-words` 3662, `d134.presence-range-refused` 3673,
/// `d134.presence-ref-driver` 3680, `d134.stray-by-clause` 3687: the
/// coupling is fully spelled and type-checked — `by` before `in`, `by`
/// drives from `$value`, the range is `$min..$max`, bare words are not
/// `$params`, a bool has no range, a presence driver is a state word (no
/// `$`), and a `by` on a slot with no `automatic=` is stray. PORT (E4):
/// the parser fixes the clause order and the compiler types every
/// clause (G5107).
#[test]
fn coupling_clauses_are_type_checked() {
    let f = fader_static();
    let case = |src: String, want: &str| {
        let text = err_text(&src);
        assert!(text.contains(want), "want {want:?} in {text:?}");
    };
    case(format!("{}Fader f 100x24\n", f.replace("by $value in $min..$max { Rectangle color=#4488FFFF }", "in $min..$max by $value { Rectangle color=#4488FFFF }")), "'by' comes before 'in'");
    case(format!("{}Fader f 100x24\n", f.replace("automatic=length by $value", "automatic=length by $min")), "v1 drives from `$value`");
    case(format!("{}Fader f 100x24\n", f.replace("automatic=length by $value in $min..$max", "automatic=length by $value in $min..$step")), "normalizes over `$min..$max`");
    case(format!("{}Fader f 100x24\n", f.replace("automatic=length by $value in $min..$max", "automatic=length by $value in min..max")), "two `$params`");
    case("control M3 $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n state marked\n Layers { slot mark automatic=presence by marked in $min..$max { Circle 6x6 } }\n}\nM3 m\n".to_string(), "a bool has no range");
    case("control M4 $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n Layers { slot mark automatic=presence by $value { Circle 6x6 } }\n}\nM4 m\n".to_string(), "names one (no `$`)");
    case(format!("{}Fader f 100x24\n", f.replace("slot label {", "slot label by $value {")), "this slot declares none");
}

/// `d134.dup-kwarg` 3695-3698: `automatic=length(from=1, from=2)` is
/// refused. Rust refuses it too — but as a parse error (G0021: named call
/// arguments are not in its grammar, GPAR32), so `length(from=1)` alone is
/// refused as well; the duplicate itself is not what is caught.
#[test]
fn duplicate_call_argument_does_not_build() {
    let src = format!("{}Fader f 100x24\n", fader_static().replace("automatic=length", "automatic=length(from=1, from=2)"));
    assert!(!builds(&src), "{:?}", diags(&src));
}

/// `d134.clause-continuation` 3699-3714: a long coupling wraps at a
/// trailing comma — `by … in …` on the next line still couples the slot
/// (position, width 20).
#[test]
fn coupling_clauses_continue_after_a_trailing_comma() {
    let c = ok("control CC $value=0, $min=0, $max=1 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n Layers {\n  slot track height=24 {\n   slot p width=20, automatic=position,\n     by $value in $min..$max { Circle 6x6 }\n  }\n }\n}\nCC c 100x24\n");
    let p = &c.page;
    let slot = find(p, "c/p");
    assert_eq!(p.nodes[slot as usize].auto, Auto::Position(0.0));
    assert_eq!(p.nodes[slot as usize].placement.width, gogo::page::Len::Exact(20.0));
}

/// `d134.g3002-note-quotes` 3715-3716: a `style … for=Fader,
/// slot.thumb.margin_left=40` on an automatic slot's placement is refused
/// (G3002) and the note quotes the coupling. Rust has no `for=` checked
/// styles or `slot.` paths (the line builds silently).
#[test]
#[ignore = "GP40: not ported: `for=` checked styles with `slot.<name>.<prop>` paths (G3002 note)"]
fn frozen_style_on_an_automatic_slot_is_refused() {
    let src = format!("{}style Frozen for=Fader, slot.thumb.margin_left=40\nFader f 100x24\n", fader_static());
    let d = diags(&src);
    let g3002 = d.iter().find(|d| d.code == "G3002").unwrap_or_else(|| panic!("{d:#?}"));
    assert!(g3002.message.contains("automatic=position by $value in $min..$max"), "{g3002:?}");
}

/// `ctl.err-slot-lane-motion-only` 3722-3725, `ctl.err-slot-lane-outside`
/// 3726-3729: a `slot.<name>.width` anim lane is refused (only MOTION
/// channels), and a `slot.` lane outside a control's own anims is refused.
/// Both are refused in Rust today — as G0063 (the lane itself is unknown:
/// `slot` reads as a node name, GP32). When GP32 lands the lane, the
/// motion-only rule needs its own reason here.
#[test]
fn slot_lanes_outside_motion_or_outside_a_control_are_refused() {
    let width_lane = format!("{}Fader f 100x24\n", FADER_SRC.replace("slot.thumb.translate.y from=-3, to=0", "slot.thumb.width from=0, to=10"));
    assert!(!builds(&width_lane), "{:?}", diags(&width_lane));
    let outside = "anim pop { slot.thumb.translate.y from=-3, to=0, duration=0.2 }\nView v { Rectangle r color=#111111FF }\n";
    assert!(!builds(outside), "{:?}", diags(outside));
}

/// `ctl.err-undeclared-contract-state` 3731-3733: a `when held` on a
/// control instance whose contract never declares `held` is refused, and
/// the error lists what IS available (`dragging`). Rust keeps no contract
/// state list (and drops instance-line whens — see
/// `instance_line_when_disabled_applies`).
#[test]
#[ignore = "GP37: not ported: control contract states (`when held` on a Fader instance builds silently)"]
fn undeclared_contract_state_is_refused_listing_the_available_ones() {
    let text = err_text(&format!("{}View v {{ Fader f 100x24 {{ when held opacity=0.5 }} }}\n", fader_static()));
    assert!(text.contains("isn't available") && text.contains("`dragging`"), "{text}");
}
