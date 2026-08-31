//! Port of the Godot gogo suite's choice / Radio / Check groups
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//!
//! - `_choice_tests` 13498-13604 (25 checks): `choice` is erased into its
//!   parent, every Radio inside joins ONE group, `value=` picks the start,
//!   the pick reports the radio NAME, and the build-error battery.
//! - `_radio_body_tests` 13604-13752 (16): a Radio with a body is a blank
//!   toggle surface in the group; `when selected` drives the body's look.
//! - `_check_body_tests` 13752-13827 (12): the same for a Check, no group.
//! - `_widget_host_when_tests` 13827-13955 (13): `widget X for=Radio` fills
//!   a Radio and its widget-scope `when selected` answers the host.
//! - `_doc_cache_tests` 13955-14022 (11): ENGINE, see below.
//!
//! ENGINE (not ported):
//! - `_doc_cache_tests` (11 checks): Godot's parsed-document cache —
//!   same-object hits, eviction by include dep, theme-variant keys, editor
//!   source overrides. Rust compiles at build time; no runtime parse cache.
//! - The `focus` theme slot half of `radio.bodied-blank-chrome` /
//!   `check.bodied-blank-chrome` (focus rings are engine machinery); the
//!   pigment-slot half (no chrome in base or held) IS ported.
//!
//! Dialect: Godot's bare-root snippets (`Check c …`) sit under a `View`
//! here (the Rust root rule, G7003); `res://` and `on:` words unchanged.
use gogo::compile;
use gogo::draw::{Draw, draw};
use gogo::input::{Event, EventKind, Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Action, Kind, NO_CHOICE, PropKey, Rgba, Value};
use gogo::runtime::{HELD, Resolved, Runtime, SELECTED, resolve};
use gogo::{Compiled, Page};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn builds(src: &str) -> bool {
    !gogo::has_errors(&compile(src, &|_| None).diags)
}

/// A compiled page with a Runtime and an Input: tap by node name, read
/// the draw list, read a resolved color / opacity.
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
    fn lay(&self) -> (Resolved, Vec<Rect>) {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        (res, rects)
    }
    fn id(&self, name: &str) -> u32 {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }
    fn selected(&self, name: &str) -> bool {
        self.rt.has(self.id(name), SELECTED)
    }
    fn choice_of(&self, name: &str) -> u32 {
        self.page.nodes[self.id(name) as usize].choice
    }
    /// A finger down and up at the node's center (Godot's `_conf_press` pair).
    fn tap(&mut self, name: &str, t: f64) -> Vec<Event> {
        let n = self.id(name);
        let (res, rects) = self.lay();
        let r = rects[n as usize];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase: Phase::Down, x, y, t, id: 1 });
        self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase: Phase::Up, x, y, t: t + 0.1, id: 1 })
    }
    fn draws(&self) -> Vec<Draw> {
        let (res, rects) = self.lay();
        draw(&self.page, &res, &rects)
    }
    /// The node's resolved fill alpha; an unset color is 0.
    fn fill_alpha(&self, name: &str) -> u8 {
        let (res, _) = self.lay();
        res.color(&self.page, self.id(name), PropKey::Color).map_or(0, |c| c.0[3])
    }
    fn opacity(&self, name: &str) -> f32 {
        let (res, _) = self.lay();
        res.num(&self.page, self.id(name), PropKey::Opacity).unwrap_or(1.0)
    }
}

/// The `change` events in a batch as (handler action, radio name).
fn changes(page: &Page, ev: &[Event]) -> Vec<(Option<Action>, String)> {
    ev.iter()
        .filter_map(|e| match e.kind {
            EventKind::Change(Value::VIdent(id)) => Some((e.action, page.str(id).to_string())),
            EventKind::Change(Value::VBool(b)) => Some((e.action, b.to_string())),
            _ => None,
        })
        .collect()
}

fn named(page: &Page, action: Option<Action>) -> String {
    match action {
        Some(Action::Named(id)) | Some(Action::Emit(id)) => page.str(id).to_string(),
        other => format!("{other:?}"),
    }
}

fn count<F: Fn(&Draw) -> bool>(list: &[Draw], f: F) -> usize {
    list.iter().filter(|d| f(d)).count()
}

// ------------------------------------------------------------- _choice_tests

/// `_choice_tests` 13498-13525 (choice.ok, erased-flat, one-group,
/// value-initial, exclusive, onchanged-wired): a `choice` has no node of its
/// own — its Radios sit flat under the Column, all in one group; `value=`
/// picks the start; a tap moves the pick and reports the radio's name to a
/// handler named after the choice.
#[test]
fn choice_is_erased_flat_into_one_group() {
    let mut fx = Fx::new("View v {\n Column menu {\n  choice paintMode value=medium {\n   Radio small text=\"Small\"\n   Radio medium text=\"Medium\"\n   Radio large text=\"Large\"\n  }\n }\n}\n");
    let menu = fx.id("menu");
    let kids: Vec<Kind> = fx.page.children(menu).map(|c| fx.page.nodes[c].kind).collect();
    assert_eq!(kids, vec![Kind::Radio, Kind::Radio, Kind::Radio], "erased flat: {kids:?}");
    assert!(fx.page.nodes.iter().all(|n| n.kind != Kind::Choice), "no choice node survives");
    assert!(fx.choice_of("small") != NO_CHOICE && fx.choice_of("small") == fx.choice_of("medium") && fx.choice_of("medium") == fx.choice_of("large"), "one group");
    assert!(fx.selected("medium") && !fx.selected("small") && !fx.selected("large"), "value= picks the start");
    let ev = fx.tap("large", 0.0);
    assert!(fx.selected("large") && !fx.selected("medium"), "exclusive");
    let reports = changes(&fx.page, &ev);
    assert_eq!(reports.len(), 1, "{ev:?}");
    assert_eq!(named(&fx.page, reports[0].0), "paintMode", "a named choice reports to a handler of its own name: {ev:?}");
    assert_eq!(reports[0].1, "large", "the report carries the radio name");
}

/// `_choice_tests` 13527-13545 (nested-ok, nested-one-group, nested-value,
/// nested-erased): Radios inside Rows/Columns under one choice still form
/// one group; the choice's first child (a Row) sits directly under the Column.
#[test]
fn choice_groups_radios_through_nested_layout() {
    let fx = Fx::new("View v {\n Column col {\n  choice size value=large {\n   Row {\n    Column { Radio small text=\"Small\";  Radio medium text=\"Medium\" }\n    Column { Radio large text=\"Large\" }\n   }\n  }\n }\n}\n");
    assert!(fx.choice_of("small") != NO_CHOICE && fx.choice_of("small") == fx.choice_of("large"), "one group through the nest");
    assert!(fx.selected("large") && !fx.selected("small"));
    let col = fx.id("col");
    let first = fx.page.children(col).next().unwrap();
    assert_eq!(fx.page.nodes[first].kind, Kind::Row, "the Row is the Column's own child");
}

/// `_choice_tests` 13547-13552 (gap-inside): a `gap` between Radios in a
/// choice is a plain child of the parent stack — three children.
#[test]
fn gap_inside_choice_spaces_like_any_stack_child() {
    let fx = Fx::new("View v { Column col { choice v2 { Radio a text=\"A\"; gap 6; Radio b text=\"B\" } } }");
    let kids: Vec<Kind> = fx.page.children(fx.id("col")).map(|c| fx.page.nodes[c].kind).collect();
    assert_eq!(kids, vec![Kind::Radio, Kind::Gap, Kind::Radio], "{kids:?}");
}

/// `_choice_tests` 13554-13569 (emit-ok, emit-arg): `on:change=emit(picked)`
/// on the choice line relays the tapped radio's NAME.
#[test]
fn choice_on_change_relays_the_radio_name() {
    let mut fx = Fx::new("View v {\n Column {\n  choice value=b, on:change=emit(picked) {\n   Radio a text=\"A\"\n   Radio b text=\"B\"\n  }\n }\n}\n");
    let ev = fx.tap("a", 0.0);
    let reports = changes(&fx.page, &ev);
    assert_eq!(reports.len(), 1, "{ev:?}");
    assert!(matches!(reports[0].0, Some(Action::Emit(id)) if fx.page.str(id) == "picked"), "{ev:?}");
    assert_eq!(reports[0].1, "a");
}

/// `_choice_tests` 13571-13575 (check-ok): a Check is an independent on/off
/// — it belongs to no group.
#[test]
fn check_is_never_grouped() {
    let fx = Fx::new("View v { Check c text=\"on?\" }");
    assert_eq!(fx.page.nodes[fx.id("c") as usize].kind, Kind::Check);
    assert_eq!(fx.choice_of("c"), NO_CHOICE);
}

/// `_choice_tests` 13577-13604 error battery, the seven both runtimes
/// refuse: duplicate names (G0044, GPAR29), `value=` naming no Radio
/// (G0069), nested choice (G0070), choice as the file root (Rust: the root
/// rule G7003), an empty choice and one with no Radio (G0069), a size on
/// the choice line (G0070).
#[test]
fn choice_error_battery() {
    assert!(codes("View v { Column { choice v2 { Radio a text=\"1\"; Radio a text=\"2\" } } }").contains(&"G0044".to_string()), "duplicate radio names");
    let miss = codes("View v { Column { choice v2 value=mediun { Radio medium text=\"M\"; Radio small text=\"S\" } } }");
    assert!(miss.contains(&"G0069".to_string()), "value= names no Radio: {miss:?}");
    assert!(codes("View v { Column { choice v2 { choice w { Radio a text=\"A\" } } } }").contains(&"G0070".to_string()), "nested choice");
    assert!(codes("choice v { Radio a text=\"A\" }").contains(&"G7003".to_string()), "a choice cannot be the root");
    assert!(codes("View v { Column { choice v2 { } } }").contains(&"G0069".to_string()), "empty choice");
    assert!(codes("View v { Column { choice v2 { Label l text=\"x\" } } }").contains(&"G0069".to_string()), "no Radio inside");
    assert!(codes("View v { Column { choice v2 width=100 { Radio a text=\"A\" } } }").contains(&"G0070".to_string()), "a size on the choice line");
}

/// `_choice_tests` 13591 (err-radio-alone): a Radio outside any `choice` is
/// a build error in Godot — exclusivity needs a written home.
#[test]
fn radio_outside_choice_is_refused() {
    assert!(!builds("View v { Radio r text=\"x\" }"), "a lone Radio must not build");
}

/// `_choice_tests` 13592-13593 (err-check-inside): a Check inside a choice
/// is a build error in Godot naming `Radio` as the right spelling.
#[test]
fn check_inside_choice_is_refused() {
    assert!(!builds("View v { Column { choice v2 { Check c text=\"x\"; Radio a text=\"A\" } } }"), "a Check inside choice must not build");
}

/// `_choice_tests` 13594-13595 (err-unnamed-radio): a Radio in a choice
/// needs a name — the name IS what the pick reports.
#[test]
fn unnamed_radio_in_choice_is_refused() {
    assert!(!builds("View v { Column { choice v2 { Radio text=\"x\" } } }"), "an unnamed Radio must not build");
}

/// `_choice_tests` 13604 (err-reserved-widget): `widget choice { … }` is a
/// build error — a widget may not take a built-in word's name.
#[test]
fn widget_named_after_a_built_in_is_refused() {
    assert!(!builds("widget choice { Panel p }\nView c"), "a widget named `choice` must not build");
}

// --------------------------------------------------------- _radio_body_tests

const RADIO_BODY_SRC: &str = "View v {\n Column menu spacing=0 {\n  choice paintMode value=b {\n   Radio a 20x20, touch_target=off {\n    Circle aRing 12x12, halign=center, valign=top, border_color=#FFFFFFFF, border_thickness=1.5\n    when selected aRing.color=#E51400FF\n   }\n   Radio b 20x20, touch_target=off {\n    Circle bRing 12x12, halign=center, valign=top, border_color=#FFFFFFFF, border_thickness=1.5\n    when selected bRing.color=#E51400FF\n   }\n   Radio c 20x20, touch_target=off {\n    Circle cRing 12x12, halign=center, valign=top, border_color=#FFFFFFFF, border_thickness=1.5\n    when selected cRing.color=#E51400FF\n   }\n  }\n }\n}\n";

/// `_radio_body_tests` 13617-13650 (bodied-ok, blank-button, blank-chrome,
/// one-group, value-seat): a Radio with a body draws NO native ring and NO
/// plate — in the base state and while held — only the body's three
/// Circles; all three Radios share one group and `value=` seats b.
#[test]
fn bodied_radio_is_a_blank_toggle_in_the_group() {
    let mut fx = Fx::new(RADIO_BODY_SRC);
    let list = fx.draws();
    assert_eq!(count(&list, |d| matches!(d, Draw::Circle { .. })), 3, "only the body rings: {list:#?}");
    assert_eq!(count(&list, |d| matches!(d, Draw::Quad { .. })), 0, "no plate, no chrome: {list:#?}");
    assert_eq!(count(&list, |d| matches!(d, Draw::Text { .. })), 0, "no native label: {list:#?}");
    // Held adds no chrome either (the pigment slots share one blank slate).
    let a = fx.id("a");
    fx.rt.set_state(a, HELD, true);
    let held = fx.draws();
    assert_eq!(count(&held, |d| matches!(d, Draw::Quad { .. })), 0, "held paints nothing behind the body: {held:#?}");
    fx.rt.set_state(a, HELD, false);
    assert!(fx.choice_of("a") != NO_CHOICE && fx.choice_of("a") == fx.choice_of("b") && fx.choice_of("b") == fx.choice_of("c"), "one group");
    assert!(fx.selected("b") && !fx.selected("a") && !fx.selected("c"), "value= seats b");
}

/// `_radio_body_tests` 13652-13688 (when-seated, tap-exclusive, tap-changed,
/// when-flips, retap-repair, retap-quiet): the seated Radio's `when
/// selected` fills its ring; a tap on c moves the pick, reports once with
/// "c", and moves the fill; a re-tap of the pick keeps it and reports
/// nothing.
#[test]
fn bodied_radio_when_and_tap() {
    let mut fx = Fx::new(RADIO_BODY_SRC);
    assert!(fx.fill_alpha("bRing") > 230 && fx.fill_alpha("aRing") == 0, "b={} a={}", fx.fill_alpha("bRing"), fx.fill_alpha("aRing"));
    let ev = fx.tap("c", 0.0);
    assert!(fx.selected("c") && !fx.selected("b"), "tap is exclusive");
    let reports = changes(&fx.page, &ev);
    assert_eq!(reports.len(), 1, "{ev:?}");
    assert_eq!((named(&fx.page, reports[0].0), reports[0].1.as_str()), ("paintMode".to_string(), "c"), "{ev:?}");
    assert!(fx.fill_alpha("cRing") > 230 && fx.fill_alpha("bRing") == 0, "c={} b={}", fx.fill_alpha("cRing"), fx.fill_alpha("bRing"));
    let ev = fx.tap("c", 1.0);
    assert!(fx.selected("c"), "the pick stays pressed on a re-tap");
    assert!(changes(&fx.page, &ev).is_empty(), "a re-tap reports nothing: {ev:?}");
}

/// `_radio_body_tests` 13690-13703 (code-write-reports, code-rewrite-quiet):
/// selecting a Radio from code behaves like a tap — the old pick clears and
/// the change is reported once; writing the same value again is quiet.
#[test]
#[ignore = "GP37: not ported: Radio code writes report like a tap and clear the group (set_state leaves the old pick selected)"]
fn bodied_radio_code_write_reports_like_a_tap() {
    let mut fx = Fx::new(RADIO_BODY_SRC);
    fx.tap("c", 0.0);
    let a = fx.id("a");
    let before = fx.rt.changes.len();
    fx.rt.set_state(a, SELECTED, true);
    assert!(fx.selected("a") && !fx.selected("c"), "a code write clears the old pick: a={} c={}", fx.selected("a"), fx.selected("c"));
    assert_eq!(fx.rt.changes.len(), before + 1, "one genuine transition: {:?}", fx.rt.changes);
    fx.rt.set_state(a, SELECTED, true);
    assert_eq!(fx.rt.changes.len(), before + 1, "a same-value re-write is quiet: {:?}", fx.rt.changes);
}

/// `_radio_body_tests` 13706-13719 (bodied-mixed): a native Radio and a
/// bodied one in the same choice share one group; `value=` seats the native.
#[test]
fn native_and_bodied_radios_share_one_group() {
    let fx = Fx::new("View v {\n Column {\n  choice kind value=nat {\n   Radio nat text=\"native\"\n   Radio bod 52x50 { Circle mRing 10x10 }\n  }\n }\n}\n");
    assert!(fx.choice_of("nat") != NO_CHOICE && fx.choice_of("nat") == fx.choice_of("bod"));
    assert!(fx.selected("nat"));
}

/// `_radio_body_tests` 13721-13724 (bodied-text-err): `text=` on a Radio
/// with a body is a build error — the body replaces every native visual,
/// the label included.
#[test]
fn text_on_a_bodied_radio_is_refused() {
    assert!(!builds("View v { Column { choice v2 { Radio a text=\"x\" { Circle q 10x10 } } } }"), "text= on a bodied Radio must not build");
}

/// `_radio_body_tests` 13726-13740 (bodied-bg-recolors): `background=` on
/// a bodied Radio paints a flat plate of that color under the body.
#[test]
fn background_recolors_the_bodied_radio_slate() {
    let fx = Fx::new("View v {\n Column {\n  choice t {\n   Radio a 20x20, background=#FF0000FF { Circle tRing 10x10 }\n   Radio b 20x20 { Circle uRing 10x10 }\n  }\n }\n}\n");
    let list = fx.draws();
    let (_, rects) = fx.lay();
    let ra = rects[fx.id("a") as usize];
    assert!(list.iter().any(|d| matches!(d, Draw::Quad { rect, color, .. } if *rect == ra && *color == Rgba([255, 0, 0, 255]))), "{list:#?}");
    assert_eq!(count(&list, |d| matches!(d, Draw::Quad { .. })), 1, "the untinted Radio stays blank: {list:#?}");
}

// --------------------------------------------------------- _check_body_tests

const CHECK_BODY_SRC: &str = "View v {\n Column menu spacing=0 {\n  Check mute 20x20, touch_target=off, on:change=OnMuteChange {\n   Circle ring 12x12, halign=center, valign=top, border_color=#FFFFFFFF, border_thickness=1.5\n   when selected ring.color=#E51400FF\n  }\n }\n}\n";

/// `_check_body_tests` 13760-13777 (bodied-ok, blank-button, blank-chrome,
/// no-group): a Check with a body draws no native box and no plate — base
/// or held — only the body's Circle; it belongs to no group.
#[test]
fn bodied_check_is_a_blank_toggle_with_no_group() {
    let mut fx = Fx::new(CHECK_BODY_SRC);
    let list = fx.draws();
    assert_eq!(count(&list, |d| matches!(d, Draw::Circle { .. })), 1, "only the body ring: {list:#?}");
    assert_eq!(count(&list, |d| matches!(d, Draw::Quad { .. })), 0, "no box, no plate: {list:#?}");
    let mute = fx.id("mute");
    fx.rt.set_state(mute, HELD, true);
    let held = fx.draws();
    assert_eq!(count(&held, |d| matches!(d, Draw::Quad { .. })), 0, "held paints nothing behind the body: {held:#?}");
    fx.rt.set_state(mute, HELD, false);
    assert_eq!(fx.choice_of("mute"), NO_CHOICE, "no group");
}

/// `_check_body_tests` 13779-13818 (when-unseated, tap-on, when-flips,
/// toggled-report, tap-off, when-clears, toggled-off): unchecked at start
/// (ring unfilled); a tap checks it, fills the ring and reports `true` to
/// OnMuteChange; a second tap unchecks, clears the ring and reports `false`.
#[test]
fn bodied_check_taps_on_and_off() {
    let mut fx = Fx::new(CHECK_BODY_SRC);
    assert_eq!(fx.fill_alpha("ring"), 0, "unseated");
    let ev = fx.tap("mute", 0.0);
    assert!(fx.selected("mute"), "tap on");
    assert!(fx.fill_alpha("ring") > 230, "the when responds: {}", fx.fill_alpha("ring"));
    let reports = changes(&fx.page, &ev);
    assert_eq!(reports.len(), 1, "{ev:?}");
    assert_eq!((named(&fx.page, reports[0].0), reports[0].1.as_str()), ("OnMuteChange".to_string(), "true"), "{ev:?}");
    let ev = fx.tap("mute", 1.0);
    assert!(!fx.selected("mute"), "tap off — no group, no no-unpress repair");
    assert_eq!(fx.fill_alpha("ring"), 0, "the when clears");
    let reports = changes(&fx.page, &ev);
    assert_eq!(reports.len(), 1, "{ev:?}");
    assert_eq!((named(&fx.page, reports[0].0), reports[0].1.as_str()), ("OnMuteChange".to_string(), "false"), "{ev:?}");
}

/// `_check_body_tests` 13823-13826 (bodied-text-err): `text=` on a Check
/// with a body is a build error — the body IS the whole look.
#[test]
fn text_on_a_bodied_check_is_refused() {
    assert!(!builds("View v { Column { Check c text=\"x\" { Circle q 10x10 } } }"), "text= on a bodied Check must not build");
}

// --------------------------------------------------- _widget_host_when_tests

const HOST_LOOK_HEAD: &str = "widget HostLook for=Radio, $tint=#E51400FF {\n Row width=fill {\n  Circle dot 12x12, color=$tint, opacity=0\n }\n when selected dot.opacity=1\n}\n";

/// `_widget_host_when_tests` 13840-13876 (whost.ok, dots-resolve,
/// when-seated, when-tap-moves, exclusive): a `for=Radio` widget fills each
/// Radio with its own dot; the widget-scope `when selected` answers the
/// HOSTING Radio — `value=` lights a's dot, a tap on b moves it.
#[test]
fn for_radio_widget_when_answers_the_host() {
    let src = format!("{HOST_LOOK_HEAD}View v {{\n Column menu spacing=0 {{\n  choice pick value=a {{\n   Radio a 20x20, touch_target=off {{ HostLook lookA }}\n   Radio b 20x20, touch_target=off {{ HostLook lookB }}\n  }}\n }}\n}}\n");
    let mut fx = Fx::new(&src);
    let dot_a = fx.page.find("lookA/dot");
    let dot_b = fx.page.find("lookB/dot");
    assert!(dot_a.is_some() && dot_b.is_some() && dot_a != dot_b, "each instance owns a dot: {dot_a:?} {dot_b:?}");
    assert!(fx.opacity("lookA/dot") > 0.9 && fx.opacity("lookB/dot") < 0.01, "a={} b={}", fx.opacity("lookA/dot"), fx.opacity("lookB/dot"));
    fx.tap("b", 0.0);
    assert!(fx.opacity("lookB/dot") > 0.9 && fx.opacity("lookA/dot") < 0.01, "a={} b={}", fx.opacity("lookA/dot"), fx.opacity("lookB/dot"));
    assert!(fx.selected("b") && !fx.selected("a"), "exclusive");
}

/// `_widget_host_when_tests` 13881-13894 (override-ok): a Radio holding a
/// `for=Radio` widget may also write its own `when selected` — the file
/// builds and the widget's dot still resolves.
#[test]
fn host_with_its_own_when_builds() {
    let src = format!("{HOST_LOOK_HEAD}View v {{\n Column menu spacing=0 {{\n  choice pick value=a {{\n   Radio a 20x20, touch_target=off {{\n    HostLook lookA\n    when selected background=#00FF00FF\n   }}\n   Radio b 20x20, touch_target=off {{ HostLook lookB }}\n  }}\n }}\n}}\n");
    let fx = Fx::new(&src);
    assert!(fx.page.find("lookA/dot").is_some());
    let (res, _) = fx.lay();
    assert_eq!(res.color(&fx.page, fx.id("a"), PropKey::Background), Some(Rgba([0, 255, 0, 255])), "the host's own response applies");
}

/// `_widget_host_when_tests` 13895-13903 (override-replaces): the host's
/// own inline `when selected` REPLACES the widget's response for that
/// state (nearer scope wins, per state) — a's dot stays unlit.
#[test]
fn host_inline_when_replaces_the_widget_response() {
    let src = format!("{HOST_LOOK_HEAD}View v {{\n Column menu spacing=0 {{\n  choice pick value=a {{\n   Radio a 20x20, touch_target=off {{\n    HostLook lookA\n    when selected background=#00FF00FF\n   }}\n   Radio b 20x20, touch_target=off {{ HostLook lookB }}\n  }}\n }}\n}}\n");
    let fx = Fx::new(&src);
    assert!(fx.opacity("lookA/dot") < 0.01, "the widget's response is replaced: {}", fx.opacity("lookA/dot"));
}

/// `_widget_host_when_tests` 13906-13934 (outside-host-err, wrong-host-err,
/// no-for-when-err, bad-host-err, for-first-err): the `for=` contract —
/// a `for=Radio` widget outside a Radio or inside a Check, a widget-scope
/// `when` with no `for=`, `for=Column` (not a stateful host), and `for=`
/// not first on the widget line are all build errors. Every one is
/// collected so the failure lists them all.
#[test]
fn for_widget_contract_errors() {
    let cases: [(&str, String); 5] = [
        ("outside-host", format!("{HOST_LOOK_HEAD}View v {{ Column {{ HostLook stray }} }}")),
        ("wrong-host", format!("{HOST_LOOK_HEAD}View v {{ Column {{ Check c 20x20 {{ HostLook stray }} }} }}")),
        ("no-for-when", "widget Plain $t=\"\" {\n Row { Label text=$t }\n when selected opacity=0.5\n}\nView v { Column { Plain p } }".to_string()),
        ("bad-host", "widget Bad for=Column, $t=\"\" {\n Row { Label text=$t }\n}\nView v { Column { Bad p } }".to_string()),
        ("for-first", "widget Late min_height=44, for=Radio {\n Row { Label text=\"x\" }\n}\nView v { Column { Late p } }".to_string()),
    ];
    let built: Vec<&str> = cases.iter().filter(|(_, src)| builds(src)).map(|(name, _)| *name).collect();
    assert!(built.is_empty(), "these must not build: {built:?}");
}

/// `_widget_host_when_tests` 13937-13946 (dead-when-err): `when selected`
/// on a plain Row inside a widget body responds to a state nothing ever
/// raises — a build error, not a look that never shows.
#[test]
#[ignore = "GP41: not ported: state word on a kind that never raises it (`when selected` on a Row) is a Godot load error"]
fn dead_when_inside_a_widget_body_is_refused() {
    assert!(!builds("widget Trap $t=\"\" {\n Row {\n  Circle q 10x10, opacity=0\n  when selected q.opacity=1\n }\n}\nView v { Column { Trap p } }"), "a when nothing raises must not build");
}
