//! Port of the Godot gogo suite, batch `controls`
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//!
//! - `_render_tests` 4130-4456 (61 checks): `render=` backend routing, the
//!   Slider composite's contract surface, the `render=` error family, the
//!   D15 fix-its, `theme { Kind … }` type entries, the hot-reload carry.
//! - `_slider_conformance_tests` 4519-4614 (7): the same scripted input
//!   against two Slider backends gives one trace.
//! - `_toggle_tests` 4614-4733 (24): the Toggle composite.
//! - `_wiring_tests` 4733-4787 (11): auto-wiring, `emit(...)`.
//! - `_error_tests` 4787-4827 (14): suggestions, dialect signposts, names.
//! - `_diagnostic_tests` 4838-5037 (43): coded errors and their rendering.
//!
//! Rust has ONE renderer (docs/DECISIONS.md D1; `page::UNREAD_BY_DESIGN`
//! keeps `render=` as an accepted, unread word), so Godot's checks that a
//! `render=` value picks or refuses a backend are FLIP: the word builds
//! everywhere, and what the composite would have drawn is read off the
//! draw list.
//!
//! ENGINE (not ported), with the reason:
//! - `render.composite-root`, `render.contract-slots`, `tgl.root-class`:
//!   script-backed host internals (slider_root.gd / toggle_root.gd).
//! - `render.thumb-min-native`: Godot theme grabber icon size.
//! - `render.err-shadow-nativelook`: NativeLook is composite-internal only.
//! - `render.carry-*`, `tgl.carry-*`: scene-tree reload state carry (GP42).
//! - `diag.g3001-head/arrow/caret/note/help/suggestion/line/json/edits-apply/
//!   fixed-builds/fixed-when-clean`, `diag.plain-*`, `diag.rich-*`,
//!   `diag.g3001-solo-one-edit/rewrites/fixed-builds`, `diag.g4003-two-spans/
//!   primary-lane/secondary-when/secondary-render`, `diag.legacy-line-shape`:
//!   rustc-style block, spans, JSON, machine edits, bbcode — editor and
//!   overlay rendering (GP43 CUT); a Rust `Diagnostic` is code + line +
//!   message.
//! - `diag.listener`: runtime diagnostics channel for the error overlay.
//!
//! Dialect notes: the Fader control
//! fixture is inlined WITHOUT its `anim releasePop { slot.thumb.… }` block
//! (the `slot.` anim lane is GP32-owed and fails G0063; no check in this
//! batch reads it).
use gogo::draw::{Draw, SLIDER_FILL, SLIDER_TRACK, TOGGLE_OFF, draw};
use gogo::input::{Event, EventKind, Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, TOGGLE_PILL, layout};
use gogo::page::{Action, Kind, PropKey, Rgba, Value};
use gogo::runtime::{DISABLED, HELD, Resolved, Runtime, SELECTED, resolve};
use gogo::{Compiled, Diagnostic, Page, compile};

/// The Godot suite's `FADER_SRC` (run_tests.gd:3385) in the Rust dialect,
/// minus the `anim releasePop` block (see the module doc).
const FADER_SRC: &str = "control Fader $value=0, $min=0, $max=100, $step=0 {
    script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"
    state_builtin hovered, focused, disabled
    state dragging
    signal drag_started, drag_ended
    transition duration=0.12, easing=cubic_out
    Layers {
        slot track width=fill, height=24 {
            slot progress automatic=length by $value in $min..$max { Rectangle color=#4488FFFF }
            slot thumb automatic=position by $value in $min..$max { Circle 12x12, color=#FFFFFFFF }
        }
        slot label { Label text=live($value) }
    }
}
";

fn build(src: &str) -> Compiled {
    compile(src, &|_| None)
}

fn ok(src: &str) -> Compiled {
    let c = build(src);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn diags(src: &str) -> Vec<Diagnostic> {
    build(src).diags
}

fn has_code(diags: &[Diagnostic], code: &str) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// compile → Runtime → resolve → layout → touch, on a 400×800 view.
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
    fn draws(&self) -> Vec<Draw> {
        let (res, rects) = self.lay();
        draw(&self.page, &res, &rects)
    }
    fn rect(&self, name: &str) -> Rect {
        let (_, rects) = self.lay();
        rects[self.node(name) as usize]
    }
    fn node(&self, name: &str) -> u32 {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }
    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        let (res, rects) = self.lay();
        self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase, x, y, t, id: 1 })
    }
    fn tap(&mut self, x: f32, y: f32, t: f64) -> Vec<Event> {
        let mut ev = self.touch(Phase::Down, x, y, t);
        ev.extend(self.touch(Phase::Up, x, y, t + 0.05));
        ev
    }
    /// The Slider's shown value (quantized, clamped).
    fn value(&self, name: &str) -> f32 {
        let (res, _) = self.lay();
        res.slider_value(&self.page, self.node(name))
    }
}

/// Event kinds and values as short words, for trace comparison.
fn trace(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .map(|e| match e.kind {
            EventKind::Press => "press".to_string(),
            EventKind::Release => "release".to_string(),
            EventKind::Activate => "activate".to_string(),
            EventKind::Change(Value::VNum(v)) => format!("change:{v}"),
            EventKind::Change(Value::VBool(b)) => format!("change:{b}"),
            EventKind::Change(_) => "change".to_string(),
            EventKind::Edit(Value::VNum(v)) => format!("edit:{v}"),
            EventKind::Edit(_) => "edit".to_string(),
            EventKind::Pointer { phase, .. } => format!("pointer:{phase:?}"),
        })
        .collect()
}

/// The three parts a Slider draws: (track quad, fill quad, knob circle).
fn slider_parts(list: &[Draw]) -> (Draw, Draw, Draw) {
    let quads: Vec<Draw> = list.iter().filter(|d| matches!(d, Draw::Quad { .. })).copied().collect();
    let knob = list.iter().find(|d| matches!(d, Draw::Circle { .. })).copied().unwrap_or_else(|| panic!("no knob: {list:#?}"));
    assert!(quads.len() >= 2, "track and fill: {list:#?}");
    (quads[0], quads[1], knob)
}

fn quad_color(d: Draw) -> Rgba {
    match d {
        Draw::Quad { color, .. } => color,
        other => panic!("not a quad: {other:?}"),
    }
}

fn quad_corner(d: Draw) -> f32 {
    match d {
        Draw::Quad { corner, .. } => corner,
        other => panic!("not a quad: {other:?}"),
    }
}

fn text_alpha(page: &Page, list: &[Draw], word: &str) -> u8 {
    list.iter()
        .find_map(|d| match d {
            Draw::Text { text, color, .. } if page.str(*text) == word => Some(color.0[3]),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {word:?} in {list:#?}"))
}

// ---------------------------------------------------------------- _render_tests

/// `_render_tests` 4133-4143 (`render.native-passthrough`,
/// `render.native-pin-silent`, `render.composite-builds`): a Slider with
/// `value=30, step=10` shows 30 whatever `render=` word it carries, and
/// `render=native` raises no warning.
#[test]
fn render_value_shows_under_every_render_word() {
    for src in ["View v { Slider s value=30, step=10, width=200 }", "View v { Slider s render=native, value=30, width=200 }", "View v { Slider s render=gogo, value=30, step=10, width=200 }"] {
        let fx = Fx::new(src);
        assert!(!build(src).diags.iter().any(|d| d.message.contains("render")), "{src}: {:#?}", build(src).diags);
        assert_eq!(fx.value("s"), 30.0, "{src}");
        let (_, fill, _) = slider_parts(&fx.draws());
        let Draw::Quad { rect, .. } = fill else { unreachable!() };
        assert!((rect.w - 60.0).abs() < 0.01, "the fill is 30 % of the 200 track: {fill:?}");
    }
}

/// `_render_tests` 4162-4170 (`render.native-look-content`,
/// `render.native-look-unstyled`, `render.label-empty`): an unstyled
/// Slider draws the built-in groove (the track in the default gray, the
/// fill, the knob) and no label text.
#[test]
fn render_bare_slider_draws_groove_and_no_label() {
    let fx = Fx::new("View v { Slider s render=gogo, value=30, step=10, width=200 }");
    let list = fx.draws();
    let (track, fill, knob) = slider_parts(&list);
    assert_eq!(quad_color(track), SLIDER_TRACK, "{list:#?}");
    assert_eq!(quad_color(fill), SLIDER_FILL, "{list:#?}");
    assert!(matches!(knob, Draw::Circle { color, .. } if color == Rgba([255, 255, 255, 255])), "{knob:?}");
    assert!(!list.iter().any(|d| matches!(d, Draw::Text { .. })), "the label is empty by default: {list:#?}");
    assert_eq!(list.len(), 3, "{list:#?}");
}

/// `_render_tests` 4155 (`render.contract-states`): `held` is a built-in
/// state of a Slider — a `when held` restyles its track while the finger
/// is down and not after. (`focused` is GP26: no keyboard focus yet.)
#[test]
fn render_slider_holds_held_while_pressed() {
    let mut fx = Fx::new("View v { Slider s width=200, height=32, halign=left, valign=top, min=0, max=100, value=50 { when held background=#FF0000FF } }");
    let s = fx.node("s");
    fx.touch(Phase::Down, 100.0, 16.0, 0.0);
    assert!(fx.rt.has(s, HELD));
    let (track, _, _) = slider_parts(&fx.draws());
    assert_eq!(quad_color(track), Rgba([255, 0, 0, 255]), "{track:?}");
    fx.touch(Phase::Up, 100.0, 16.0, 0.1);
    assert!(!fx.rt.has(s, HELD));
    let (track, _, _) = slider_parts(&fx.draws());
    assert_eq!(quad_color(track), SLIDER_TRACK, "{track:?}");
}

/// `_render_tests` 4157 (`render.compat-signals`): Godot's
/// `value_changed` / `drag_started` / `drag_ended` are `change` /
/// `press` / `release` here (GPAR13 DONE) — all three wire on a Slider and
/// bracket one drag.
#[test]
fn render_compat_signals_are_change_press_release() {
    let mut fx = Fx::new("View v { Slider s width=200, height=32, halign=left, valign=top, min=0, max=100, step=10, value=30, on:change=OnSChange, on:press=OnSDragStarted, on:release=OnSDragEnded }");
    let mut ev = fx.touch(Phase::Down, 100.0, 16.0, 0.0);
    ev.extend(fx.touch(Phase::Move, 150.0, 16.0, 0.1));
    ev.extend(fx.touch(Phase::Up, 150.0, 16.0, 0.2));
    let t = trace(&ev);
    assert_eq!(t.first().map(String::as_str), Some("press"), "{t:?}");
    assert!(t.contains(&"release".to_string()), "{t:?}");
    assert!(t.iter().any(|w| w.starts_with("change:")), "{t:?}");
    assert!(t.iter().position(|w| w == "release") < t.iter().position(|w| w.starts_with("change:")), "release before the change: {t:?}");
}

/// `_render_tests` 4159, 4176 (`render.param-defaults`,
/// `render.default-builds`): a Slider with nothing declared has `min` 0
/// and `max` 100 — a value of 50 fills half, 150 clamps to the end.
#[test]
fn render_param_defaults_min_zero_max_hundred() {
    let fx = Fx::new("View v { Slider d render=gogo, width=200 }");
    assert_eq!(fx.value("d"), 0.0, "no value = min");
    let fx = Fx::new("View v { Slider d width=200, value=50 }");
    assert_eq!(fx.value("d"), 50.0);
    let (track, fill, _) = slider_parts(&fx.draws());
    let (Draw::Quad { rect: tr, .. }, Draw::Quad { rect: fr, .. }) = (track, fill) else { unreachable!() };
    assert!((fr.w - tr.w / 2.0).abs() < 0.01, "half: {fill:?} of {track:?}");
    let fx = Fx::new("View v { Slider d width=200, value=150 }");
    assert_eq!(fx.value("d"), 100.0, "clamped to max");
}

/// `_render_tests` 4178 (`render.native-default-parity`): a fresh Godot
/// HSlider has `step` 1, so a Slider with no `step=` shows whole numbers
/// (`value=33.3` shows 33); `step=0` written in the file turns snapping
/// off (GPAR92, decided: Godot).
#[test]
fn render_default_step_is_one_like_range() {
    let fx = Fx::new("View v { Slider d width=200, value=33.3 }");
    assert_eq!(fx.value("d"), 33.0, "Godot: a fresh HSlider steps by 1");
}

/// `_render_tests` 4172 (`render.initial-quantize`): a declared value sits
/// on its step from the first frame (30 with step 10 stays 30; 34 lands
/// on 30).
#[test]
fn render_declared_value_quantizes_at_once() {
    assert_eq!(Fx::new("View v { Slider s render=gogo, value=30, step=10, width=200 }").value("s"), 30.0);
    assert_eq!(Fx::new("View v { Slider s render=gogo, value=34, step=10, width=200 }").value("s"), 30.0);
}

/// `_render_tests` 4188-4204 (`render.for-style-applies`,
/// `render.look-yields-unpainted`, `render.look-yields-painted`): a
/// `style … for=Slider` rounds the track to
/// 3 and leaves its default color; `slot.track.background=` paints it.
#[test]
#[ignore = "GP40: not ported: `style X for=<Control>` + `slot.<name>.<prop>` paths"]
fn render_for_slider_style_restyles_the_track() {
    let fx = Fx::new("style ThinSlider for=Slider\nView v { Slider s .ThinSlider render=gogo, value=30, width=200 }");
    let (track, _, _) = slider_parts(&fx.draws());
    assert!((quad_corner(track) - 3.0).abs() < 1e-3, "{track:?}");
    assert_eq!(quad_color(track), SLIDER_TRACK, "a corner alone does not paint the box: {track:?}");
    let fx = Fx::new("style RedTrack for=Slider, slot.track.background=#AA1111FF\nView v { Slider s .RedTrack render=gogo, value=30, width=200 }");
    let (track, _, _) = slider_parts(&fx.draws());
    assert_eq!(quad_color(track), Rgba([0xAA, 0x11, 0x11, 0xFF]), "{track:?}");
}

/// `_render_tests` 4207 (`render.fill-label`): a `slot label { Label }`
/// fill on a Slider instance lands — the label text is drawn with the
/// slider.
#[test]
fn render_slot_fill_label_is_drawn() {
    let fx = Fx::new("View v { Slider s render=gogo, value=30, width=200 { slot label { Label text=\"vol\" } } }");
    let list = fx.draws();
    assert!(list.iter().any(|d| matches!(d, Draw::Text { text, .. } if fx.page.str(*text) == "vol")), "{list:#?}");
}

/// `_render_tests` 4214-4222 (`render.autowire-builds` PORT,
/// `render.autowire-value-changed` FLIP): a named Slider with no
/// `on:change=` reports `change` under its own name (action `None` = the
/// node's name is the handler). Godot also fires the handler for a CODE
/// write of `value`; here a code write reports nothing — "the app knows
/// it wrote" (todo-gogo.md GP12 SHIPPED, owed line) — the value just
/// shows.
#[test]
fn render_autowire_by_name_and_code_writes_stay_silent() {
    let mut fx = Fx::new("View v { Slider sizeSlider render=gogo, value=10, step=1, width=200, height=32, halign=left, valign=top }");
    let s = fx.node("sizeSlider");
    fx.touch(Phase::Down, 100.0, 16.0, 0.0);
    let ev = fx.touch(Phase::Up, 100.0, 16.0, 0.1);
    let change = ev.iter().find(|e| matches!(e.kind, EventKind::Change(_))).unwrap_or_else(|| panic!("{ev:?}"));
    assert_eq!(change.node, s);
    assert_eq!(change.action, None, "auto-wired by the node's own name: {change:?}");
    fx.rt.set_prop(s, PropKey::Value, Value::VNum(42.0));
    assert_eq!(fx.value("sizeSlider"), 42.0, "the write shows");
}

/// `_render_tests` 4227-4318, `_toggle_tests` 4721-4726 — the `render=`
/// error family: `render.err-no-renderer`, `render.err-button-entry-render`,
/// `render.err-not-contract`, `render.err-bad-value`, `render.err-in-style`,
/// `render.err-user-control-native`, `render.d15-fixit`,
/// `render.d15-drop-for-exit`, `render.d15-fill-trigger`,
/// `render.entry-backend`, `render.entry-optout-whole`,
/// `render.err-entry-for-needs-backend`, `render.err-entry-variant-render`,
/// `tgl.err-render-native`, `tgl.err-entry-render-native`. Godot refuses a
/// `render=` word where its backend cannot exist. FLIP: Rust has one
/// renderer (docs/DECISIONS.md D1; `page::UNREAD_BY_DESIGN` — `render`
/// "accepted for old files that picked a renderer per node; one renderer
/// here"), so every one of these builds with no error, and a for=Slider
/// style or a slot fill on a Slider needs no `render=gogo` exit. (The
/// Godot fixtures' `slot.track.corner_radius=3` style path is dropped
/// here: slot paths are not built (GP40) and an unbuilt word is a build
/// error since E8, not a silent pass.)
#[test]
fn render_is_one_renderer_so_the_render_word_builds_everywhere() {
    let sources: Vec<String> = vec![
        "View v { Button b render=gogo, text=\"x\" }".to_string(),
        "theme { Button render=gogo }\nView v { Label l text=\"x\" }".to_string(),
        "View v { Label l render=gogo, text=\"x\" }".to_string(),
        "View v { Slider s render=fancy }".to_string(),
        "style Sneaky render=gogo\nView v { Slider s .Sneaky }".to_string(),
        format!("{FADER_SRC}View v {{ Fader f 100x24, render=native }}"),
        "style ThinSlider for=Slider\nView v { Slider s .ThinSlider value=30 }".to_string(),
        "style Ghost for=Slider { when hovered opacity=0.5 }\nView v { Slider s .Ghost value=30 }".to_string(),
        "View v { Slider s value=30 { slot label { Label text=\"x\" } } }".to_string(),
        "style ThinSlider for=Slider\ntheme {\n    Slider .ThinSlider render=gogo\n}\nView v { Slider s value=30 }".to_string(),
        "style ThinSlider for=Slider\ntheme {\n    Slider .ThinSlider render=gogo\n}\nView v { Slider s render=native, value=30 }".to_string(),
        "style ThinSlider for=Slider\ntheme { Slider .ThinSlider }\nView v { Slider s value=30 }".to_string(),
        "theme dark { Slider render=gogo }\nView v { Label l text=\"x\" }".to_string(),
        "View v { Toggle t render=native }".to_string(),
        "theme { Toggle render=native }\nView v { Label l text=\"x\" }".to_string(),
    ];
    for src in &sources {
        let c = build(src);
        assert!(!gogo::has_errors(&c.diags), "{src}\n{:#?}", c.diags);
    }
    // The Slider under a `render=` word still draws its one look.
    let fx = Fx::new("View v { Slider s render=native, value=30, width=200 }");
    assert_eq!(slider_parts(&fx.draws()).0, slider_parts(&Fx::new("View v { Slider s render=gogo, value=30, width=200 }").draws()).0);
}

/// `_render_tests` 4232-4237 (`render.button-for-applies-natively`): a
/// `style AppBtn for=Button { when held opacity=0.8 }` builds and its
/// held response dims the Button's text while pressed. (`for=` itself is
/// unchecked here — GP40 — it only earns a G2001 warning.)
#[test]
fn render_button_for_style_applies() {
    let src = "style AppBtn for=Button { when held opacity=0.8 }\nView v { Button b .AppBtn text=\"x\", min_size=44x44, halign=left, valign=top }";
    let mut fx = Fx::new(src);
    assert_eq!(text_alpha(&fx.page, &fx.draws(), "x"), 255);
    fx.touch(Phase::Down, 10.0, 10.0, 0.0);
    assert_eq!(text_alpha(&fx.page, &fx.draws(), "x"), 204, "0.8 while held");
}

/// `_render_tests` 4238-4245 (`render.err-button-for-slot-path`,
/// `render.err-button-inline-path`): a `slot.leading.…` path on a Button
/// (in a `for=Button` style or inline) is an error — a Button has no
/// slots. Rust only warns G2001 and builds.
#[test]
#[ignore = "GP40: not ported: `slot.<name>.<prop>` paths (Godot: `has no slots` error; Rust: G2001 warning, builds)"]
fn render_button_slot_paths_are_refused() {
    assert!(gogo::has_errors(&diags("style Bad for=Button, slot.leading.size=20\nView v { Button b .Bad text=\"x\" }")));
    assert!(gogo::has_errors(&diags("View v { Button b text=\"x\", slot.leading.size=20 }")));
}

/// `_render_tests` 4242 (`render.err-button-fill`): a `slot leading { … }`
/// fill inside a Button is an error — no slots to fill. Rust builds it and
/// lays the fill out as a child.
#[test]
#[ignore = "GP41: not ported: slot-fill diagnostics (Godot: `has no slots`; Rust: builds, the Label draws inside the Button)"]
fn render_button_slot_fill_is_refused() {
    assert!(gogo::has_errors(&diags("View v { Button b text=\"x\" { slot leading { Label text=\"i\" } } }")));
}

/// `_render_tests` 4249-4254, 4325-4327 (`render.button-entry-for-legal`,
/// `render.entry-style-applied`): a style named on a `theme { Kind .Style }`
/// entry applies to every instance of that kind — the Button dims to 0.8
/// while held, the Slider's track corner is 3.
#[test]
#[ignore = "GP40: not ported: theme kind default with `.Style` (the style name on a theme entry is dropped)"]
fn render_theme_entry_style_applies() {
    let mut fx = Fx::new("style AppBtn for=Button { when held opacity=0.8 }\ntheme { Button .AppBtn }\nView v { Button b text=\"x\", min_size=44x44, halign=left, valign=top }");
    fx.touch(Phase::Down, 10.0, 10.0, 0.0);
    assert_eq!(text_alpha(&fx.page, &fx.draws(), "x"), 204, "the entry's style dims while held");
    let fx = Fx::new("style ThinSlider for=Slider\ntheme {\n    Slider .ThinSlider render=gogo\n}\nView v { Slider s value=30, width=200 }");
    let (track, _, _) = slider_parts(&fx.draws());
    assert!((quad_corner(track) - 3.0).abs() < 1e-3, "{track:?}");
}

/// `_render_tests` 4270-4277 (`render.err-shadow-slider`): a user
/// `control Slider { … }` shadows the built-in and is refused. Rust
/// expands the widget silently — `Slider s` becomes the widget's Layers.
#[test]
#[ignore = "NEW: Godot refuses a `control Slider` shadowing the built-in vs Rust expands it silently (`s` becomes Kind::Layers)"]
fn render_shadowing_a_builtin_kind_is_refused() {
    let c = build("control Slider $value=0 {\n script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n Layers { slot track { } }\n}\nView v { Slider s value=1 }");
    assert!(gogo::has_errors(&c.diags), "shadowing a built-in is an error; got kind {:?}", c.page.nodes[c.page.find("s").unwrap() as usize].kind);
}

/// `_render_tests` 4283-4284 (`render.err-nativelook-in-user-tree`):
/// `NativeLook` is not a word a user tree may use — unknown node type
/// (G7011 here; Godot: "unknown widget type").
#[test]
fn render_nativelook_is_not_a_kind() {
    assert!(has_code(&diags("View v { NativeLook n part=groove }"), "G7011"));
}

/// `_render_tests` 4287-4298 (`render.widget-internal`): a `render=gogo`
/// inside a widget body is an instance line — the caller is unaffected
/// and `r/inner` is the Slider, value 30.
#[test]
fn render_widget_internal_slider_is_addressable() {
    let fx = Fx::new("widget VolumeRow {\n    Column {\n        Slider inner render=gogo, value=30, step=10\n    }\n}\nView v { VolumeRow r }");
    let inner = fx.node("r/inner");
    assert_eq!(fx.page.nodes[inner as usize].kind, Kind::Slider);
    assert_eq!(fx.value("r/inner"), 30.0);
}

/// `_render_tests` 4344-4362 (`render.entry-prop-default`,
/// `render.entry-prop-override`, `render.entry-native-props`): a
/// `theme { Slider step=25 }` entry is a defaults tier — `Slider a
/// value=30` snaps to 25, an instance `step=10` wins and 30 stays 30, and
/// the entry reaches a Slider with no `render=` word too.
#[test]
fn render_theme_entry_props_default_under_instance_overrides() {
    let fx = Fx::new("theme { Slider step=25, render=gogo }\nView v { Column c { Slider a value=30; Slider b step=10, value=30 } }");
    assert_eq!(fx.value("a"), 25.0);
    assert_eq!(fx.value("b"), 30.0);
    let fx = Fx::new("theme { Slider step=25 }\nView v { Slider s value=30, width=200 }");
    assert_eq!(fx.value("s"), 25.0);
}

/// `_render_tests` 4364-4381 (`render.entry-variant-look`): a
/// `theme dark { Slider .FatSlider }` entry overrides the look half in the
/// dark variant — the track corner is 9 there.
#[test]
#[ignore = "GP38: not ported: theme variant activation for kind entries (and GP40 `.Style` on an entry)"]
fn render_theme_variant_entry_look() {
    let mut fx = Fx::new("style ThinSlider for=Slider\nstyle FatSlider for=Slider, slot.track.corner_radius=9\ntheme {\n    Slider .ThinSlider render=gogo\n}\ntheme dark {\n    Slider .FatSlider\n}\nView v { Slider s value=30, width=200 }");
    fx.rt.variant = fx.page.intern("dark");
    let (track, _, _) = slider_parts(&fx.draws());
    assert!((quad_corner(track) - 9.0).abs() < 1e-3, "{track:?}");
}

/// `_render_tests` 4383-4391 (`render.entry-user-control`): a user
/// control takes theme entries too — `theme { Fader .Chunky }` rounds its
/// track to 5. Rust refuses a theme entry on any non-built-in kind with
/// G3006 ("not a control kind").
#[test]
#[ignore = "NEW: Godot lets `theme { <UserControl> … }` default a user control vs Rust refuses it with G3006 `not a control kind`"]
fn render_theme_entry_defaults_a_user_control() {
    let src = format!("{FADER_SRC}style Chunky for=Fader, slot.track.corner_radius=5\ntheme {{ Fader .Chunky }}\nView v {{ Fader f 100x24 }}");
    let c = build(&src);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

/// `_render_tests` 4398-4405, 4432-4435 (`render.err-entry-unknown-type`,
/// `render.err-entry-widget`, `render.err-entry-user-native`): a theme
/// entry names a control kind — `Fancy`, a `widget Card`, and (here) a
/// user control are all refused with G3006. Godot refuses the first two
/// for the same reason and the third for its `render=native`; see
/// `render_theme_entry_defaults_a_user_control` for the difference.
#[test]
fn render_theme_entry_needs_a_control_kind() {
    assert!(has_code(&diags("theme { Fancy .X }\nView v { Label l text=\"x\" }"), "G3006"));
    assert!(has_code(&diags("widget Card { Column { slot } }\ntheme { Card .X }\nView v { Label l text=\"x\" }"), "G3006"));
    assert!(gogo::has_errors(&diags(&format!("{FADER_SRC}theme {{ Fader render=native }}\nView v {{ Fader f 100x24 }}"))));
}

/// `_render_tests` 4406-4425 (`render.err-entry-named`,
/// `render.err-entry-wiring`, `render.err-entry-placement`,
/// `render.err-entry-size-arg`, `render.err-entry-dup`): a theme entry
/// has no name, no wiring, no placement, no size, and comes once per kind
/// — each is an error in Godot. Rust accepts every one silently: the name
/// and the `200x20` are dropped, `on:change=` and `width=200` land in the
/// kind defaults where nothing reads them (a Slider under
/// `theme { Slider width=200 }` lays out 0 wide), and the FIRST of two
/// entries wins.
#[test]
fn render_theme_entry_name_wiring_placement_size_and_dup_are_refused() {
    assert!(gogo::has_errors(&diags("theme { Slider vol value=1 }\nView v { Label l text=\"x\" }")), "has no name");
    assert!(gogo::has_errors(&diags("theme { Slider on:change=OnX }\nView v { Label l text=\"x\" }")), "no name to wire");
    assert!(gogo::has_errors(&diags("theme { Slider width=200 }\nView v { Label l text=\"x\" }")), "is placement");
    assert!(gogo::has_errors(&diags("theme { Slider 200x20 }\nView v { Label l text=\"x\" }")), "placement");
    assert!(gogo::has_errors(&diags("theme { Slider step=5 }\ntheme { Slider step=9 }\nView v { Label l text=\"x\" }")), "duplicate theme entry");
}

/// `_render_tests` 4426-4431 (`render.err-entry-style-mismatch`): a style
/// `for=Fader` named on a `theme { Slider … }` entry is an error — the
/// entry defaults Slider. Rust does not check `for=`.
#[test]
#[ignore = "GP40: not ported: `for=` checked styles (a for=Fader style on a Slider entry builds)"]
fn render_theme_entry_style_kind_must_match() {
    let src = format!("{FADER_SRC}style Chunky for=Fader, slot.track.corner_radius=5\ntheme {{ Slider .Chunky }}\nView v {{ Fader f 100x24 }}");
    assert!(gogo::has_errors(&diags(&src)));
}

/// `_render_tests` 4262-4264 (`render.user-control-redundant-true`): a
/// user control is always Gogo-drawn, so `render=gogo` on its instance is
/// redundant and legal — it builds.
#[test]
fn render_user_control_takes_a_redundant_render_gogo() {
    let fx = Fx::new(&format!("{FADER_SRC}View v {{ Fader f 100x24, render=gogo }}"));
    assert_eq!(fx.rect("f").w, 100.0);
}

// ------------------------------------------------- _slider_conformance_tests

/// `_slider_conformance_tests` 4520-4585 (`conf.builds`,
/// `conf.min-height-parity`, `conf.trace-parity`, `conf.end-value-parity`,
/// `conf.trace-nonempty`): two Sliders, one bare and one `render=gogo`,
/// build to the same box and answer the same scripted finger with the same
/// trace: a press at the half jumps to 50, drags to 3/4 (80 on the 10-step)
/// and past the end (100), the lift commits `change 100`; a code reset to
/// 30 shows 30; a whole gesture while `disabled` is mute. Godot's scenario
/// also stepped by keyboard and wheel (its trace was ≥ 10 entries) — GP26
/// / GP42 here, see `toggle_keyboard_steps`.
#[test]
fn conformance_two_sliders_one_trace() {
    let mut fx = Fx::new("View v { Layers z { Slider nat 200x20, min=0, max=100, step=10, value=30, halign=left, valign=top, margin_left=20, margin_top=20; Slider comp 200x20, render=gogo, min=0, max=100, step=10, value=30, halign=left, valign=top, margin_left=20, margin_top=200 } }");
    let nat = fx.rect("nat");
    let comp = fx.rect("comp");
    assert_eq!((nat.w, nat.h), (200.0, 20.0), "{nat:?}");
    assert_eq!(nat.h, comp.h, "same minimum height: {nat:?} {comp:?}");
    let run = |fx: &mut Fx, name: &str, t0: f64| -> Vec<String> {
        let org = fx.rect(name);
        let node = fx.node(name);
        let mut ev = fx.touch(Phase::Down, org.x + 100.0, org.y + 10.0, t0);
        ev.extend(fx.touch(Phase::Move, org.x + 146.0, org.y + 10.0, t0 + 0.1));
        ev.extend(fx.touch(Phase::Move, org.x + 192.0, org.y + 10.0, t0 + 0.2));
        ev.extend(fx.touch(Phase::Up, org.x + 192.0, org.y + 10.0, t0 + 0.3));
        fx.rt.set_prop(node, PropKey::Value, Value::VNum(30.0));
        assert_eq!(fx.value(name), 30.0, "the code reset shows");
        fx.rt.set_state(node, DISABLED, true);
        ev.extend(fx.touch(Phase::Down, org.x + 100.0, org.y + 10.0, t0 + 1.0));
        ev.extend(fx.touch(Phase::Move, org.x + 146.0, org.y + 10.0, t0 + 1.1));
        ev.extend(fx.touch(Phase::Up, org.x + 146.0, org.y + 10.0, t0 + 1.2));
        fx.rt.set_state(node, DISABLED, false);
        trace(&ev)
    };
    let tn = run(&mut fx, "nat", 0.0);
    let tc = run(&mut fx, "comp", 10.0);
    assert_eq!(tn, tc, "nat={tn:?} comp={tc:?}");
    assert_eq!(tn, vec!["edit:50", "edit:80", "edit:100", "change:100"], "{tn:?}");
    assert_eq!(fx.value("nat"), fx.value("comp"));
    assert_eq!(fx.value("nat"), 30.0, "disabled: the gesture moved nothing");
}

/// `_slider_conformance_tests` 4600-4612 (`conf.prog-snap-parity`,
/// `conf.prog-builds`): a code write snaps on the min-anchored lattice
/// (`min=5, step=10` → 5, 15, 25 — never 10/20): 12 lands on 15 on both
/// sliders.
#[test]
fn conformance_code_write_snaps_from_min() {
    let mut fx = Fx::new("View v { Column c { Slider pn min=5, max=25, step=10, value=5; Slider pc render=gogo, min=5, max=25, step=10, value=5 } }");
    let pn = fx.node("pn");
    let pc = fx.node("pc");
    fx.rt.set_prop(pn, PropKey::Value, Value::VNum(12.0));
    fx.rt.set_prop(pc, PropKey::Value, Value::VNum(12.0));
    assert_eq!(fx.value("pn"), 15.0);
    assert_eq!(fx.value("pc"), 15.0);
}

// ---------------------------------------------------------------- _toggle_tests

/// A bare Toggle at the view's top-left: its pill is `TOGGLE_PILL`, the
/// knob 18 inset 2.
fn toggle_fx() -> Fx {
    Fx::new("View v { Toggle mute halign=left, valign=top }")
}

/// `_toggle_tests` 4616-4630 (`tgl.builds`, `tgl.contract-slots`,
/// `tgl.initial-off`): a bare Toggle builds, draws its two parts — the
/// track pill and the thumb knob — and starts off: not selected, knob at
/// the left end.
#[test]
fn toggle_builds_off_with_track_and_thumb() {
    let fx = toggle_fx();
    let mute = fx.node("mute");
    assert!(!fx.rt.has(mute, SELECTED));
    let list = fx.draws();
    assert_eq!(list.len(), 2, "track + thumb: {list:#?}");
    assert!(matches!(list[0], Draw::Quad { color, corner, .. } if color == TOGGLE_OFF && corner == TOGGLE_PILL.1 / 2.0), "the off pill: {:?}", list[0]);
    assert!(matches!(list[1], Draw::Circle { rect, .. } if rect.x == 2.0 && rect.w == 18.0), "the knob at the off end: {:?}", list[1]);
}

/// `_toggle_tests` 4624 (`tgl.change-signal`): a Toggle reports
/// `change(bool)` — under its own name when no `on:change=` is written.
#[test]
fn toggle_reports_change() {
    let mut fx = toggle_fx();
    let ev = fx.tap(22.0, 11.0, 0.0);
    let change = ev.iter().find(|e| matches!(e.kind, EventKind::Change(_))).unwrap_or_else(|| panic!("{ev:?}"));
    assert_eq!(change.kind, EventKind::Change(Value::VBool(true)));
    assert_eq!(change.action, None, "auto-wired by name (OnMuteChange): {change:?}");
    let mut fx = Fx::new("View v { Toggle mute halign=left, valign=top, on:change=OnMuteChange }");
    let ev = fx.tap(22.0, 11.0, 0.0);
    assert!(ev.iter().any(|e| matches!(e.action, Some(Action::Named(id)) if fx.page.str(id) == "OnMuteChange")), "{ev:?}");
}

/// `_toggle_tests` 4626 (`tgl.min-touch-target`): the Toggle's minimum
/// height is a 44-epx finger target. Rust's bare Toggle is its 22-epx
/// pill.
#[test]
#[ignore = "NEW: Godot Toggle min height 44 (finger target) vs Rust 22 (the pill); may fall under the decided Rust look"]
fn toggle_min_height_is_a_finger_target() {
    let fx = toggle_fx();
    let r = fx.rect("mute");
    assert!(r.h >= 44.0 - 0.01, "{r:?}");
}

/// `_toggle_tests` 4630-4638 (`tgl.derive-on`,
/// `tgl.selected-prop-writes-value`): `selected` DERIVES from the value —
/// a code write of `value=1` selects (and reports once), a code write of
/// `selected=false` puts the value back to 0 (and reports).
#[test]
#[ignore = "GP37: not ported: Radio/Toggle code writes report and re-derive selected (a value write leaves `selected` alone)"]
fn toggle_code_writes_derive_selected_and_value() {
    let mut fx = toggle_fx();
    let mute = fx.node("mute");
    fx.rt.set_prop(mute, PropKey::Value, Value::VNum(1.0));
    assert!(fx.rt.has(mute, SELECTED), "selected derives from value=1");
    fx.rt.set_state(mute, SELECTED, false);
    let (res, _) = fx.lay();
    assert_eq!(res.num(&fx.page, mute, PropKey::Value), Some(0.0), "the bool write writes the value");
}

/// `_toggle_tests` 4647-4671 (`tgl.tap-on`, `tgl.tap-off`,
/// `tgl.drag-live-flip`, `tgl.drag-settles-on`, `tgl.drag-short-stays-on`):
/// a tap flips exactly once (on, then off); a drag right flips `selected`
/// LIVE when the knob crosses the middle and the lift settles the value at
/// 1; a short drag left that lifts right of the middle settles back to 1
/// and reports nothing.
#[test]
fn toggle_tap_and_drag_settle_by_side() {
    let mut fx = toggle_fx();
    let mute = fx.node("mute");
    let ev = fx.tap(22.0, 11.0, 0.0);
    assert!(fx.rt.has(mute, SELECTED));
    assert_eq!(trace(&ev).iter().filter(|w| w.starts_with("change:")).count(), 1, "one flip per tap: {ev:?}");
    assert_eq!(fx.lay().0.num(&fx.page, mute, PropKey::Value), Some(1.0));
    let ev = fx.tap(22.0, 11.0, 1.0);
    assert!(!fx.rt.has(mute, SELECTED));
    assert_eq!(trace(&ev), vec!["change:false", "activate"], "{ev:?}");
    // Drag right: crossing the midpoint flips live, release settles 1.
    fx.touch(Phase::Down, 8.0, 11.0, 2.0);
    fx.touch(Phase::Move, 40.0, 11.0, 2.1);
    assert!(fx.rt.has(mute, SELECTED), "flips live mid-drag");
    let ev = fx.touch(Phase::Up, 40.0, 11.0, 2.2);
    assert_eq!(fx.lay().0.num(&fx.page, mute, PropKey::Value), Some(1.0));
    assert_eq!(trace(&ev).iter().filter(|w| w.starts_with("change:")).count(), 1, "{ev:?}");
    // Drag left but lift right of the midpoint: stays on, no report.
    fx.touch(Phase::Down, 40.0, 11.0, 3.0);
    fx.touch(Phase::Move, 30.0, 11.0, 3.1);
    let ev = fx.touch(Phase::Up, 30.0, 11.0, 3.2);
    assert_eq!(fx.lay().0.num(&fx.page, mute, PropKey::Value), Some(1.0));
    assert!(fx.rt.has(mute, SELECTED));
    assert!(!ev.iter().any(|e| matches!(e.kind, EventKind::Change(_))), "no flip after a real drag that lands where it started: {ev:?}");
}

/// `_toggle_tests` 4673-4680 (`tgl.key-activate`, `tgl.key-increment`,
/// `tgl.key-decrement`) and the keyboard/wheel half of
/// `_slider_conformance_tests` 4551-4556: through focus, Enter flips a
/// Toggle, Right/Left pin its ends; on a Slider Right/Left step by `step`,
/// End/Home jump to max/min, wheel ticks step too.
#[test]
#[ignore = "GP26: not ported: keyboard focus and key input (Toggle: Enter flips, Right→on, Left→off; Slider: Right→40, Left→30, End→100, Home→0)"]
fn toggle_keyboard_steps() {
    panic!("GP26: no key input API — Godot expects Enter to flip a focused Toggle, Right/Left to pin its ends, and Slider arrows/Home/End/wheel to step");
}

/// `_toggle_tests` 4682-4685 (`tgl.disabled-mute`): a disabled Toggle
/// takes no tap — it stays off and reports nothing.
#[test]
fn toggle_disabled_is_mute() {
    let mut fx = toggle_fx();
    let mute = fx.node("mute");
    fx.rt.set_state(mute, DISABLED, true);
    let ev = fx.tap(22.0, 11.0, 0.0);
    assert!(ev.is_empty(), "{ev:?}");
    assert!(!fx.rt.has(mute, SELECTED));
}

/// `_toggle_tests` 4705-4709 (`tgl.initial-on`): `Toggle mute value=1`
/// starts ON — selected, the pill in accent — with no phantom change
/// (GPAR92, decided: Godot; was: the knob drew at the right end of a
/// GRAY pill, and the first tap recolored it without moving the knob).
#[test]
fn toggle_declared_value_one_starts_on() {
    let fx = Fx::new("View v { Toggle mute value=1, halign=left, valign=top }");
    let mute = fx.node("mute");
    assert!(fx.rt.has(mute, SELECTED), "value=1 means on");
    let list = fx.draws();
    assert!(!matches!(list[0], Draw::Quad { color, .. } if color == TOGGLE_OFF), "the pill is not the off gray: {:?}", list[0]);
}

/// `_toggle_tests` 4713-4719 (`tgl.for-style`): a
/// `style FatToggle for=Toggle, slot.track.corner_radius=4` restyles the
/// track pill's corner to 4.
#[test]
#[ignore = "GP40: not ported: `style X for=<Control>` + `slot.<name>.<prop>` paths"]
fn toggle_for_style_restyles_the_track() {
    let fx = Fx::new("style FatToggle for=Toggle, slot.track.corner_radius=4\nView v { Toggle t .FatToggle halign=left, valign=top }");
    let list = fx.draws();
    assert!((quad_corner(list[0]) - 4.0).abs() < 1e-3, "{:?}", list[0]);
}

/// `_toggle_tests` 4723-4725 (`tgl.render-gogo-silent`): `render=gogo` on
/// a Toggle is redundant and silent.
#[test]
fn toggle_render_gogo_is_silent() {
    let c = build("View v { Toggle t render=gogo }");
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

// ---------------------------------------------------------------- _wiring_tests

/// `_wiring_tests` 4735-4747 (`wire.ok`, `wire.auto`, `wire.explicit`): a
/// named Button with no `on:activate=` reports under its own name
/// (`action None` = `OnPaintButtonActivate` in the generated handler enum);
/// an explicit `on:activate=OnCustomActivate` names that.
#[test]
fn wiring_auto_by_name_and_explicit() {
    let mut fx = Fx::new("View v { Column c {\n    Button paintButton text=\"auto\"\n    Button other text=\"explicit\", on:activate=OnCustomActivate\n} }");
    let pb = fx.rect("paintButton");
    let ev = fx.tap(pb.x + 5.0, pb.y + 5.0, 0.0);
    assert_eq!(trace(&ev), vec!["activate"], "{ev:?}");
    assert_eq!(ev[0].action, None, "auto-wired: {ev:?}");
    let ot = fx.rect("other");
    let ev = fx.tap(ot.x + 5.0, ot.y + 5.0, 1.0);
    assert!(matches!(ev[0].action, Some(Action::Named(id)) if fx.page.str(id) == "OnCustomActivate"), "{ev:?}");
}

/// `_wiring_tests` 4749-4750 (`wire.missing-handler`): Godot refuses a
/// handler name the host lacks at load, with a suggestion. FLIP: names
/// become code (gogo-design.md §7 "Changed": `build.rs` emits the
/// `Handler` enum, "typos are rustc errors") — the gogo compile accepts
/// any handler word; the host's `match` is where a typo fails.
#[test]
fn wiring_unknown_handler_name_is_the_hosts_rustc_error() {
    let c = build("View v { Button b on:activate=OnPaintButtonPresed }");
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let b = c.page.find("b").unwrap();
    let h = c.page.handlers[c.page.nodes[b as usize].handlers.0 as usize];
    assert!(matches!(h.action, Action::Named(id) if c.page.str(id) == "OnPaintButtonPresed"));
}

/// `_wiring_tests` 4751-4752 (`wire.missing-signal`): a misspelled event
/// (`on:presed=`) is refused naming the events (G0049).
#[test]
fn wiring_unknown_event_is_refused() {
    let d = diags("View v { Button b on:presed=OnPaintButtonActivate }");
    assert!(has_code(&d, "G0049"), "{d:#?}");
    assert!(d.iter().any(|x| x.code == "G0049" && x.message.contains("activate")), "names the events: {d:#?}");
}

/// `_wiring_tests` 4757-4781 (`wire.emit-ok`, `wire.emit-relays`,
/// `wire.emit-widget`, `wire.emit-args-dropped`): `on:<event>=emit(Name)`
/// is a pure relay — a Button tap, a widget instance's tap (the wire binds
/// to its `signals` node) and a Slider `change` all report `Emit(relayed)`;
/// the emit carries the name alone, so the Slider's value never reaches
/// the host signal.
#[test]
fn wiring_emit_relays_from_button_widget_and_slider() {
    let mut fx = Fx::new("widget RelayRow $label=\"\" {\n    Button signals, text=$label\n}\nView v { Column c {\n    Button direct text=\"d\", on:activate=emit(relayed)\n    RelayRow row label=\"r\", on:activate=emit(relayed)\n    Slider amount min=0, max=10, on:change=emit(relayed)\n} }");
    let relayed = fx.page.intern("relayed");
    let is_relay = |e: &Event| e.action == Some(Action::Emit(relayed));
    let d = fx.rect("direct");
    let ev = fx.tap(d.x + 5.0, d.y + 5.0, 0.0);
    assert_eq!(ev.iter().filter(|e| is_relay(e)).count(), 1, "{ev:?}");
    let r = fx.rect("row");
    assert_eq!(fx.page.nodes[fx.node("row") as usize].kind, Kind::Button, "the instance name lands on the signals node");
    let ev = fx.tap(r.x + 5.0, r.y + 5.0, 1.0);
    assert_eq!(ev.iter().filter(|e| is_relay(e)).count(), 1, "{ev:?}");
    let a = fx.rect("amount");
    fx.touch(Phase::Down, a.x + 10.0, a.y + a.h / 2.0, 2.0);
    fx.touch(Phase::Move, a.x + a.w / 2.0, a.y + a.h / 2.0, 2.1);
    let ev = fx.touch(Phase::Up, a.x + a.w / 2.0, a.y + a.h / 2.0, 2.2);
    let change = ev.iter().find(|e| matches!(e.kind, EventKind::Change(_))).unwrap_or_else(|| panic!("{ev:?}"));
    assert!(is_relay(change), "{change:?}");
    assert!(matches!(change.action, Some(Action::Emit(_))), "the emit names the signal and nothing else: {change:?}");
}

/// `_wiring_tests` 4782-4783 (`wire.emit-missing-signal`): Godot refuses
/// `emit(relayd)` when the host has no such signal, with a suggestion.
/// FLIP: names become code (gogo-design.md §7) — `Emit` names join the
/// generated `Handler` enum and the host's `match` catches the typo.
#[test]
fn wiring_unknown_emit_name_is_the_hosts_rustc_error() {
    let c = build("View v { Button b on:activate=emit(relayd) }");
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    assert!(matches!(c.page.handlers[0].action, Action::Emit(id) if c.page.str(id) == "relayd"));
}

/// `_wiring_tests` 4784-4785 (`wire.emit-form`): `emit(relayed, 2)` is
/// malformed — the form is `emit(HostSignalName)`. Rust keeps the first
/// argument and drops the rest silently.
#[test]
#[ignore = "NEW: Godot refuses `emit(relayed, 2)` (form is emit(HostSignalName)) vs Rust keeps the first arg and drops the rest silently"]
fn wiring_emit_takes_exactly_one_name() {
    assert!(gogo::has_errors(&diags("View v { Button b on:activate=emit(relayed, 2) }")));
}

/// `wire.emit-ok` (run_tests.gd:4759): `Button signals text=$label` — a
/// reserved flag is its own delimiter, so no comma before the next member
/// (the rule the name and repeat's `as`/`by` clauses follow; GPAR98). A
/// comma after the flag stays legal.
#[test]
fn signals_flag_before_a_key_member_needs_no_comma() {
    let d = diags("widget RelayRow $label=\"\" {\n    Button signals text=$label\n}\nView v { RelayRow row label=\"r\" }");
    assert!(!gogo::has_errors(&d), "{d:#?}");
    let d = diags("widget RelayRow $label=\"\" {\n    Button signals, text=$label\n}\nView v { RelayRow row label=\"r\" }");
    assert!(!gogo::has_errors(&d), "the comma stays legal: {d:#?}");
}

// ----------------------------------------------------------------- _error_tests

/// `_error_tests` 4788-4791, 4805-4807, 4810-4818 (`err.type-suggest`,
/// `err.style-suggest`, `err.ref-suggest`, `err.suggest-runners-up`,
/// `err.suggest-options-fallback`): an unknown type / style / `$ref` is
/// refused WITH a suggestion ("did you mean 'Button'", runners-up in
/// "(or: …)", a hopeless typo lists "options:"). gogo-design.md §7
/// promises the same ("Unknown word = build error with a suggestion");
/// Rust's G7011 / G5002 / G6001 carry none.
#[test]
fn errors_suggest_the_nearest_word() {
    let d = diags("View v { Buttn optionsButton text=\"Brush\" }");
    assert!(d.iter().any(|x| x.message.contains("Button")), "did you mean 'Button': {d:#?}");
    assert!(d.iter().any(|x| x.message.contains("(or: ")), "runners-up: {d:#?}");
    let d = diags("style ToolButton min_height=36\nView v { Button b .ToolButon }");
    assert!(d.iter().any(|x| x.message.contains("ToolButton")), "did you mean 'ToolButton': {d:#?}");
    let d = diags("$accent = #000000FF\nView v { Rectangle r color=$acent }");
    assert!(d.iter().any(|x| x.message.contains("accent")), "did you mean 'accent': {d:#?}");
    let d = diags("$accent = #000000FF\nView v { Rectangle r color=$zzzz }");
    assert!(d.iter().any(|x| x.message.contains("options:") && x.message.contains("accent")), "{d:#?}");
}

/// `_error_tests` 4790-4791, 4795-4798 (`err.prop-suggest`,
/// `err.signpost-foreground`, `err.signpost-text-color`): an unknown
/// property is an ERROR with a suggestion (`textt` → `text`), and a word
/// from another dialect (`foreground=`, `text_color=`) is routed to the
/// gogo word (`font_color=`). Rust warns G2001 and builds.
#[test]
fn errors_unknown_property_is_refused_with_a_signpost() {
    let d = diags("View v { Button b textt=\"x\" }");
    assert!(gogo::has_errors(&d) && d.iter().any(|x| x.message.contains("text")), "{d:#?}");
    let d = diags("View v { Button b foreground=#FF0000FF }");
    assert!(gogo::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=")), "{d:#?}");
    let d = diags("View v { Button b text_color=#FF0000FF }");
    assert!(gogo::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=")), "{d:#?}");
}

/// `_error_tests` 4799-4803 (`err.signpost-color-static`,
/// `err.signpost-color-when`): `color=` on a node that owns no color (a
/// Button) is refused, statically and in a `when`, naming `font_color=`
/// / `background=` — else the whole control tints. Rust recolors the text.
#[test]
fn errors_color_on_a_non_owner_is_refused() {
    let d = diags("View v { Button b color=#FF0000FF }");
    assert!(gogo::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=") && x.message.contains("background=")), "{d:#?}");
    let d = diags("View v { Button b {\n    when held color=#FF0000FF\n} }");
    assert!(gogo::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=")), "{d:#?}");
}

/// `_error_tests` 4804-4805 (`err.color-on-owner-ok`): a node that OWNS a
/// color keeps `color=`, statically and in a `when`.
#[test]
fn errors_color_on_an_owner_is_fine() {
    let c = ok("View v { Rectangle r color=#111111FF {\n    when hovered color=#222222FF\n} }");
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

/// `_error_tests` 4819-4824 (`err.dup-name`, `err.reserved`,
/// `err.line-number`): two nodes named `x` are refused (G0044); a bare
/// `slot` outside a widget body is refused (the root must be View/Panel,
/// G7003; Godot: "widget body"); the error for a bad type on the third
/// line says line 3.
#[test]
fn errors_dup_name_reserved_word_and_line_number() {
    assert!(has_code(&diags("View v { Column c { Button x\n Button x } }"), "G0044"));
    assert!(gogo::has_errors(&diags("slot")));
    let d = diags("View v {\n\tButton ok\n\tBogus b\n}");
    let bad = d.iter().find(|x| x.code == "G7011").unwrap_or_else(|| panic!("{d:#?}"));
    assert_eq!(bad.line, 3, "{d:#?}");
}

// ------------------------------------------------------------ _diagnostic_tests

/// `_diagnostic_tests` 4849-4850, 4905-4906 (`diag.g3001-fires`,
/// `diag.g3001-solo-fires`): `anim=` on a `when` line is an error — a
/// `when` declares a look, the anim belongs on a `transition` (Godot
/// G3001; Rust's old G3001 — `$` defined twice — is G6002 now (GPAR61)). Rust warns G2001
/// and drops the word. (The Godot fixture's `self.scale` target is `b.scale`
/// here: `self` outside a widget is G0060.)
#[test]
#[ignore = "GP41: not ported: `anim=` on a `when` line is a load-time error (Godot G3001; Rust: G2001 warning, the word is dropped)"]
fn diag_anim_on_a_when_line_is_refused() {
    let d = diags("style MenuButton width=220, height=48 {\n    when held background=#FFFFFFff, anim=test\n}\nanim test {\n    b.scale to=5, duration=0.8\n}\nView v { Button b .MenuButton text=\"X\" }");
    assert!(gogo::has_errors(&d) && d.iter().any(|x| x.message.contains("anim")), "{d:#?}");
    let d = diags("anim t2 {\n    b.scale to=0.9, duration=0.1\n}\nView v { Button b {\n    when held anim=t2\n} }");
    assert!(gogo::has_errors(&d), "{d:#?}");
}

/// `_diagnostic_tests` 4923-4936 (`diag.g4003-fires`): one property with
/// two writers — a `when held scale=` holds it while a `transition …
/// anim=t` drives it — is an error naming both. Rust builds it.
#[test]
#[ignore = "GPAR62: G4003 two-writers fence not ported (a when and a transition anim on one prop build clean)"]
fn diag_two_writers_on_one_property_is_refused() {
    let d = diags("style MB width=100, height=48 {\n    when held background=#FFFFFFff\n    transition any -> held anim=t\n}\nanim t {\n    b.scale to=5, duration=0.8\n}\nView root {\n    Button b .MB text=\"X\" {\n        when held scale=0.96\n    }\n}");
    assert!(gogo::has_errors(&d), "{d:#?}");
}

/// `_diagnostic_tests` 4945-4955 (`diag.g7003-fires`, `diag.g7003-help`):
/// a file whose root is a `Column` is refused with G7003, and the message
/// names both doors, `View` and `Panel`. (Godot's help also mentions the
/// Panel's `background=` box; Rust's does not.)
#[test]
fn diag_root_must_be_view_or_panel() {
    let d = diags("Column menu {\n    Button a text=\"x\"\n}");
    let e = d.iter().find(|x| x.code == "G7003").unwrap_or_else(|| panic!("{d:#?}"));
    assert!(e.message.contains("View") && e.message.contains("Panel"), "{e:?}");
}

/// `_diagnostic_tests` 4958-4967 (`diag.g7002-fires`, `diag.g7002-caret`):
/// Godot refuses a `View` root under a Container host (G7002, "this root
/// node IS the view instance"). FLIP: gogo-design.md §7 removed the
/// host-class rules (G7002/G7005) — a pure layout has no host class; a
/// `View` root builds with nothing to match it against.
#[test]
fn diag_no_host_class_rules() {
    let c = ok("View v {\n    Button a text=\"x\"\n}");
    assert!(!has_code(&c.diags, "G7002"), "{:#?}", c.diags);
    assert_eq!(c.page.nodes[0].kind, Kind::View);
}

/// `_diagnostic_tests` 4970-4978 (`diag.g7001-fires`, `diag.g7001-spans`):
/// two roots in one file are refused (Godot G7001, two spans naming root
/// #2; Rust G7003 "a file has one root") and the error points at the
/// second root's line.
#[test]
fn diag_two_roots_point_at_the_second() {
    let d = diags("View a { Button x text=\"1\" }\nView b { Button y text=\"2\" }");
    let e = d.iter().find(|x| x.code == "G7003").unwrap_or_else(|| panic!("{d:#?}"));
    assert_eq!(e.line, 2, "{e:?}");
}

/// `_diagnostic_tests` 4982-4984 (`diag.legacy-uncoded`): Godot leaves an
/// unknown-type error UNCODED (its block starts `error: `). Rust codes it
/// G7011 — a code outside the addon's registry (GPAR61).
#[test]
#[ignore = "GPAR61: registries differ — Godot leaves unknown-type uncoded, Rust codes it G7011 (unregistered)"]
fn diag_unknown_type_is_uncoded() {
    let d = diags("View v { Buttn b text=\"x\" }");
    assert!(d.iter().any(|x| x.code.is_empty()), "{d:#?}");
}

/// `_diagnostic_tests` 4989-4992 (`diag.g0002`): an unclosed `{` is an
/// error — G0002 in both engines (GPAR61 aligned the code; the
/// unknown-unit lexer error moved to G0079).
#[test]
fn diag_unclosed_brace_is_refused() {
    let d = diags("View menu {\n    Button a\n");
    assert!(has_code(&d, "G0002"), "{d:#?}");
}

/// `_diagnostic_tests` 5008-5037 (`diag.registry-doc.*`, `diag.docs-dir`,
/// `diag.doc-registered.*`, `diag.code-registered.*`): the addon pins its
/// G-code registry three ways (registry↔docs↔source). The Rust half
/// (GPAR61): every code the source emits is in `diag::REGISTRY`, every
/// registered code is emitted somewhere, a vacated number never comes
/// back, and a code shared with the addon's PUBLISHED registry is shared
/// ON PURPOSE (`diag::SHARED_WITH_ADDON`) — a new Rust code can never
/// silently land on a published number with a different meaning. Owed:
/// the per-code teaching pages (docs/errors/) the addon serves.
#[test]
fn diag_source_and_registry_match_both_ways() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut codes: Vec<String> = Vec::new();
    for file in ["compile.rs", "parser.rs", "lexer.rs"] {
        let text = std::fs::read_to_string(root.join("src").join(file)).unwrap();
        let bytes = text.as_bytes();
        let mut i = 0;
        while let Some(at) = text[i..].find("\"G") {
            let start = i + at + 1;
            if start + 5 <= bytes.len() && bytes[start + 1..start + 5].iter().all(u8::is_ascii_digit) && bytes.get(start + 5) == Some(&b'"') {
                let code = text[start..start + 5].to_string();
                if !codes.contains(&code) {
                    codes.push(code);
                }
            }
            i = start;
        }
    }
    assert!(!codes.is_empty());
    let registered: Vec<&str> = gogo::diag::REGISTRY.iter().map(|(c, _)| *c).collect();
    let unregistered: Vec<&String> = codes.iter().filter(|c| !registered.contains(&c.as_str())).collect();
    assert!(unregistered.is_empty(), "codes emitted but not in diag::REGISTRY: {unregistered:?}");
    let unemitted: Vec<&&str> = registered.iter().filter(|c| !codes.contains(&c.to_string())).collect();
    assert!(unemitted.is_empty(), "codes in diag::REGISTRY that nothing emits: {unemitted:?}");
    let revived: Vec<&String> = codes.iter().filter(|c| gogo::diag::RETIRED.contains(&c.as_str())).collect();
    assert!(revived.is_empty(), "retired code numbers came back: {revived:?}");
    // The addon's published registry, when this checkout has it beside
    // the crate: the shared set is exactly the deliberate one.
    let addon = root.join("../../../FantasiaGodot/addons/gogo/gogo_diagnostics.gd");
    let Ok(text) = std::fs::read_to_string(&addon) else { return };
    let mut published: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("\"G") {
            if rest.len() >= 5 && rest.as_bytes()[..4].iter().all(u8::is_ascii_digit) && rest.as_bytes()[4] == b'"' {
                published.push(format!("G{}", &rest[..4]));
            }
        }
    }
    assert!(published.len() >= 50, "the addon registry parse broke: {} codes", published.len());
    let mut shared: Vec<&str> = registered.iter().copied().filter(|c| published.iter().any(|p| p == c)).collect();
    shared.sort_unstable();
    let mut declared: Vec<&str> = gogo::diag::SHARED_WITH_ADDON.to_vec();
    declared.sort_unstable();
    assert_eq!(shared, declared, "a code shared with the published registry is shared on purpose only — same meaning, listed in diag::SHARED_WITH_ADDON");
}
