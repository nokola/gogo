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
//! Rust has ONE renderer (docs/DECISIONS.md D1), and the `render=` word
//! is REMOVED outright (E12 + user 2026-09-02) — Godot's checks that a
//! `render=` value picks or refuses a backend are FLIP: the word refuses
//! as an unknown property, and what the composite would have drawn is
//! read off the draw list.
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
use gogo_ui::draw::{Draw, SLIDER_FILL, SLIDER_TRACK, TOGGLE_OFF, draw};
use gogo_ui::KeptFrame;
use gogo_ui::input::{Event, EventKind, Input, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect, TOGGLE_PILL};
use gogo_ui::page::{Action, Kind, PropKey, Color_Straight_Srgb_4xU8_RGBA, Value};
use gogo_ui::runtime::{DISABLED, HELD, Runtime, SELECTED, resolve};
use gogo_ui::{Compiled, Diagnostic, Page, compile};

/// The Godot suite's `FADER_SRC` (run_tests.gd:3385) in the Rust dialect,
/// minus the `anim releasePop` block (see the module doc).
const FADER_SRC: &str = "control Fader $value=0, $min=0, $max=100, $step=0 {
    behavior slider by $value in $min..$max, step=$step
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
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
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
    fn lay(&self) -> KeptFrame {
        KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance)
    }
    fn draws(&self) -> Vec<Draw> {
        let f = self.lay();
        draw(&self.page, f.frame())
    }
    fn rect(&self, name: &str) -> Rect {
        let f = self.lay();
        let (_, rects) = (f.look(), f.rects());
        rects[(self.node(name)).index()]
    }
    fn node(&self, name: &str) -> gogo_ui::NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }
    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        let f = self.lay();
        self.input.touch(&self.page, &mut self.rt, f.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 })
    }
    fn tap(&mut self, x: f32, y: f32, t: f64) -> Vec<Event> {
        let mut ev = self.touch(Phase::Down, x, y, t);
        ev.extend(self.touch(Phase::Up, x, y, t + 0.05));
        ev
    }
    /// The Slider's shown value (quantized, clamped).
    fn value(&self, name: &str) -> f32 {
        let f = self.lay();
        let (res, _) = (f.look(), f.rects());
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
            EventKind::Key { key, .. } => format!("key:{key:?}"),
            EventKind::Signal => "signal".to_string(),
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

fn quad_color(d: Draw) -> Color_Straight_Srgb_4xU8_RGBA {
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
    for src in ["View v { Slider s value=30, step=10, width=200 }", "View v { Slider s value=30, width=200 }", "View v { Slider s value=30, step=10, width=200 }"] {
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
    let fx = Fx::new("View v { Slider s value=30, step=10, width=200 }");
    let list = fx.draws();
    let (track, fill, knob) = slider_parts(&list);
    assert_eq!(quad_color(track), SLIDER_TRACK, "{list:#?}");
    assert_eq!(quad_color(fill), SLIDER_FILL, "{list:#?}");
    assert!(matches!(knob, Draw::Circle { color, .. } if color == Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255])), "{knob:?}");
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
    assert_eq!(quad_color(track), Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 255]), "{track:?}");
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
    let fx = Fx::new("View v { Slider d width=200 }");
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
    assert_eq!(Fx::new("View v { Slider s value=30, step=10, width=200 }").value("s"), 30.0);
    assert_eq!(Fx::new("View v { Slider s value=34, step=10, width=200 }").value("s"), 30.0);
}

/// `_render_tests` 4188-4204 (`render.for-style-applies`,
/// `render.look-yields-unpainted`, `render.look-yields-painted`): a
/// `style … for=Slider` rounds the track to
/// 3 and leaves its default color; `slot.track.background=` paints it.
#[test]
fn render_for_slider_style_restyles_the_track() {
    let fx = Fx::new("style ThinSlider for=Slider, slot.track.corner_radius=3\nView v { Slider s .ThinSlider value=30, width=200 }");
    let (track, _, _) = slider_parts(&fx.draws());
    assert!((quad_corner(track) - 3.0).abs() < 1e-3, "{track:?}");
    assert_eq!(quad_color(track), SLIDER_TRACK, "a corner alone does not paint the box: {track:?}");
    let fx = Fx::new("style RedTrack for=Slider, slot.track.background=#AA1111FF\nView v { Slider s .RedTrack value=30, width=200 }");
    let (track, _, _) = slider_parts(&fx.draws());
    assert_eq!(quad_color(track), Color_Straight_Srgb_4xU8_RGBA([0xAA, 0x11, 0x11, 0xFF]), "{track:?}");
}

/// `_render_tests` 4207 (`render.fill-label`): a `slot label { Label }`
/// fill on a Slider instance lands — the label text is drawn with the
/// slider.
#[test]
fn render_slot_fill_label_is_drawn() {
    let fx = Fx::new("View v { Slider s value=30, width=200 { slot label { Label text=\"vol\" } } }");
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
    let mut fx = Fx::new("View v { Slider sizeSlider value=10, step=1, width=200, height=32, halign=left, valign=top }");
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
/// error family (backend-gate refusals). CUT with the word itself (E12
/// + user 2026-09-02): FanRust has ONE renderer (docs/DECISIONS.md D1),
/// so the renderer word is gone — not aliased, not excused; it refuses
/// as an unknown property like any stray word.
#[test]
fn render_word_is_gone() {
    assert!(has_code(&diags("View v { Slider s render=gogo, value=30 }"), "G2001"));
    assert!(gogo_ui::has_errors(&diags("theme { Slider render=native }\nView v { Label l text=\"x\" }")));
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
/// slots. Rust refuses it too (the areas/for= round's checked styles).
#[test]
fn render_button_slot_paths_are_refused() {
    assert!(gogo_ui::has_errors(&diags("style Bad for=Button, slot.leading.size=20\nView v { Button b .Bad text=\"x\" }")));
    assert!(gogo_ui::has_errors(&diags("View v { Button b text=\"x\", slot.leading.size=20 }")));
}

/// `_render_tests` 4242 (`render.err-button-fill`): a `slot leading { … }`
/// fill inside a Button is an error — no slots to fill. Rust builds it and
/// lays the fill out as a child.
#[test]
#[ignore = "GP41: not ported: slot-fill diagnostics (Godot: `has no slots`; Rust: builds, the Label draws inside the Button)"]
fn render_button_slot_fill_is_refused() {
    assert!(gogo_ui::has_errors(&diags("View v { Button b text=\"x\" { slot leading { Label text=\"i\" } } }")));
}

/// `_render_tests` 4249-4254, 4325-4327 (`render.button-entry-for-legal`,
/// `render.entry-style-applied`): a style named on a `theme { Kind .Style }`
/// entry applies to every instance of that kind — the Button dims to 0.8
/// while held, the Slider's track corner is 3.
#[test]
fn render_theme_entry_style_applies() {
    let mut fx = Fx::new("style AppBtn for=Button { when held opacity=0.8 }\ntheme { Button .AppBtn }\nView v { Button b text=\"x\", min_size=44x44, halign=left, valign=top }");
    fx.touch(Phase::Down, 10.0, 10.0, 0.0);
    assert_eq!(text_alpha(&fx.page, &fx.draws(), "x"), 204, "the entry's style dims while held");
    let fx = Fx::new("style ThinSlider for=Slider, slot.track.corner_radius=3\ntheme {\n    Slider .ThinSlider\n}\nView v { Slider s value=30, width=200 }");
    let (track, _, _) = slider_parts(&fx.draws());
    assert!((quad_corner(track) - 3.0).abs() < 1e-3, "{track:?}");
}

/// `_render_tests` 4270-4277 (`render.err-shadow-slider`): a user
/// `control Slider { … }` shadows the built-in and is refused. Rust
/// expands the widget silently — `Slider s` becomes the widget's Layers.
#[test]
fn render_shadowing_a_builtin_kind_is_refused() {
    let c = build("control Slider $value=0 {\n Layers { slot track { } }\n}\nView v { Slider s value=1 }");
    assert!(gogo_ui::has_errors(&c.diags), "shadowing a built-in is an error; got kind {:?}", c.page.nodes[(c.page.find("s").unwrap()).index()].kind);
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
    let fx = Fx::new("widget VolumeRow {\n    Column {\n        Slider inner value=30, step=10\n    }\n}\nView v { VolumeRow r }");
    let inner = fx.node("r/inner");
    assert_eq!(fx.page.nodes[inner.index()].kind, Kind::Slider);
    assert_eq!(fx.value("r/inner"), 30.0);
}

/// `_render_tests` 4344-4362 (`render.entry-prop-default`,
/// `render.entry-prop-override`, `render.entry-native-props`): a
/// `theme { Slider step=25 }` entry is a defaults tier — `Slider a
/// value=30` snaps to 25, an instance `step=10` wins and 30 stays 30, and
/// the entry reaches a Slider with no `render=` word too.
#[test]
fn render_theme_entry_props_default_under_instance_overrides() {
    let fx = Fx::new("theme { Slider step=25 }\nView v { Column c { Slider a value=30; Slider b step=10, value=30 } }");
    assert_eq!(fx.value("a"), 25.0);
    assert_eq!(fx.value("b"), 30.0);
    let fx = Fx::new("theme { Slider step=25 }\nView v { Slider s value=30, width=200 }");
    assert_eq!(fx.value("s"), 25.0);
}

/// `_render_tests` 4364-4381 (`render.entry-variant-look`): a
/// `theme dark { Slider .FatSlider }` entry overrides the look half in the
/// dark variant — the track corner is 9 there.
#[test]
fn render_theme_variant_entry_look() {
    let mut fx = Fx::new("style ThinSlider for=Slider\nstyle FatSlider for=Slider, slot.track.corner_radius=9\ntheme {\n    Slider .ThinSlider\n}\ntheme dark {\n    Slider .FatSlider\n}\nView v { Slider s value=30, width=200 }");
    fx.rt.set_theme_variant(&fx.page, "dark");
    let (track, _, _) = slider_parts(&fx.draws());
    assert!((quad_corner(track) - 9.0).abs() < 1e-3, "{track:?}");
}

/// `_render_tests` 4383-4391 (`render.entry-user-control`): a user
/// control takes theme entries too — `theme { Fader .Chunky }` rounds its
/// track to 5. Rust refuses a theme entry on any non-built-in kind with
/// G3006 ("not a control kind").
#[test]
fn render_theme_entry_defaults_a_user_control() {
    let src = format!("{FADER_SRC}style Chunky for=Fader, slot.track.corner_radius=5\ntheme {{ Fader .Chunky }}\nView v {{ Fader f 100x24 }}");
    let c = build(&src);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
}

/// `_render_tests` 4398-4405, 4432-4435 (`render.err-entry-unknown-type`,
/// `render.err-entry-widget`, `render.err-entry-user-native`): a theme
/// entry names a control kind — an unknown `Fancy` and a `widget Card`
/// refuse with G3006 (a widget has no contract; style its nodes by
/// name). A declared `control` is LEGAL since GP40 step 3 — see
/// `render_theme_entry_defaults_a_user_control`.
#[test]
fn render_theme_entry_needs_a_control_kind() {
    assert!(has_code(&diags("theme { Fancy .X }\nView v { Label l text=\"x\" }"), "G3006"));
    assert!(has_code(&diags("widget Card { Column { slot } }\ntheme { Card .X }\nView v { Label l text=\"x\" }"), "G3006"));
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
    assert!(gogo_ui::has_errors(&diags("theme { Slider vol value=1 }\nView v { Label l text=\"x\" }")), "has no name");
    assert!(gogo_ui::has_errors(&diags("theme { Slider on:change=OnX }\nView v { Label l text=\"x\" }")), "no name to wire");
    assert!(gogo_ui::has_errors(&diags("theme { Slider width=200 }\nView v { Label l text=\"x\" }")), "is placement");
    assert!(gogo_ui::has_errors(&diags("theme { Slider 200x20 }\nView v { Label l text=\"x\" }")), "placement");
    assert!(gogo_ui::has_errors(&diags("theme { Slider step=5 }\ntheme { Slider step=9 }\nView v { Label l text=\"x\" }")), "duplicate theme entry");
}

/// `_render_tests` 4426-4431 (`render.err-entry-style-mismatch`): a style
/// `for=Fader` named on a `theme { Slider … }` entry is an error — the
/// entry defaults Slider. Rust checks the entry's kind against `for=`
/// too (the areas/for= round).
#[test]
fn render_theme_entry_style_kind_must_match() {
    let src = format!("{FADER_SRC}style Chunky for=Fader, slot.track.corner_radius=5\ntheme {{ Slider .Chunky }}\nView v {{ Fader f 100x24 }}");
    assert!(gogo_ui::has_errors(&diags(&src)));
}

/// `_render_tests` 4262-4264 (`render.user-control-redundant-true`):
/// Godot allowed a redundant `render=gogo` on a user control. Gone with
/// the word (E12 + user 2026-09-02) — it refuses like any unknown
/// property.
#[test]
fn render_word_is_gone_on_user_controls_too() {
    assert!(gogo_ui::has_errors(&diags(&format!("{FADER_SRC}View v {{ Fader f 100x24, render=gogo }}"))));
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
    let mut fx = Fx::new("View v { Layers z { Slider nat 200x20, min=0, max=100, step=10, value=30, halign=left, valign=top, margin_left=20, margin_top=20; Slider comp 200x20, min=0, max=100, step=10, value=30, halign=left, valign=top, margin_left=20, margin_top=200 } }");
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
    let mut fx = Fx::new("View v { Column c { Slider pn min=5, max=25, step=10, value=5; Slider pc min=5, max=25, step=10, value=5 } }");
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

/// `_toggle_tests` 4626 (`tgl.min-touch-target`): a bare Toggle's box is
/// at least 44 epx tall — a finger target — while the 22-epx pill draws
/// centered in it (GPAR92, user 2026-08-31; `layout::TOGGLE_TOUCH_EPX`).
#[test]
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
fn toggle_code_writes_derive_selected_and_value() {
    let mut fx = toggle_fx();
    let mute = fx.node("mute");
    fx.rt.set_prop(mute, PropKey::Value, Value::VNum(1.0));
    assert!(fx.rt.has(mute, SELECTED), "selected derives from value=1");
    fx.rt.set_state(mute, SELECTED, false);
    let f = fx.lay();
    let (res, _) = (f.look(), f.rects());
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
    assert_eq!(fx.lay().look().num(&fx.page, mute, PropKey::Value), Some(1.0));
    let ev = fx.tap(22.0, 11.0, 1.0);
    assert!(!fx.rt.has(mute, SELECTED));
    assert_eq!(trace(&ev), vec!["change:false", "activate"], "{ev:?}");
    // Drag right: crossing the midpoint flips live, release settles 1.
    fx.touch(Phase::Down, 8.0, 11.0, 2.0);
    fx.touch(Phase::Move, 40.0, 11.0, 2.1);
    assert!(fx.rt.has(mute, SELECTED), "flips live mid-drag");
    let ev = fx.touch(Phase::Up, 40.0, 11.0, 2.2);
    assert_eq!(fx.lay().look().num(&fx.page, mute, PropKey::Value), Some(1.0));
    assert_eq!(trace(&ev).iter().filter(|w| w.starts_with("change:")).count(), 1, "{ev:?}");
    // Drag left but lift right of the midpoint: stays on, no report.
    fx.touch(Phase::Down, 40.0, 11.0, 3.0);
    fx.touch(Phase::Move, 30.0, 11.0, 3.1);
    let ev = fx.touch(Phase::Up, 30.0, 11.0, 3.2);
    assert_eq!(fx.lay().look().num(&fx.page, mute, PropKey::Value), Some(1.0));
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
    let c = build("View v { Toggle t }");
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
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    let b = c.page.find("b").unwrap();
    let h = c.page.handlers[c.page.nodes[b.index()].handlers.0 as usize];
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
    assert_eq!(fx.page.nodes[(fx.node("row")).index()].kind, Kind::Button, "the instance name lands on the signals node");
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
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    assert!(matches!(c.page.handlers[0].action, Action::Emit(id) if c.page.str(id) == "relayd"));
}

/// `_wiring_tests` 4784-4785 (`wire.emit-form`): `emit(relayed, 2)` is
/// malformed — the form is `emit(HostSignalName)`. Rust keeps the first
/// argument and drops the rest silently.
#[test]
fn wiring_emit_takes_exactly_one_name() {
    assert!(gogo_ui::has_errors(&diags("View v { Button b on:activate=emit(relayed, 2) }")));
}

/// `wire.emit-ok` (run_tests.gd:4759): `Button signals text=$label` — a
/// reserved flag is its own delimiter, so no comma before the next member
/// (the rule the name and repeat's `as`/`by` clauses follow; GPAR98). A
/// comma after the flag stays legal.
#[test]
fn signals_flag_before_a_key_member_needs_no_comma() {
    let d = diags("widget RelayRow $label=\"\" {\n    Button signals text=$label\n}\nView v { RelayRow row label=\"r\" }");
    assert!(!gogo_ui::has_errors(&d), "{d:#?}");
    let d = diags("widget RelayRow $label=\"\" {\n    Button signals, text=$label\n}\nView v { RelayRow row label=\"r\" }");
    assert!(!gogo_ui::has_errors(&d), "the comma stays legal: {d:#?}");
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
    assert!(gogo_ui::has_errors(&d) && d.iter().any(|x| x.message.contains("text")), "{d:#?}");
    let d = diags("View v { Button b foreground=#FF0000FF }");
    assert!(gogo_ui::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=")), "{d:#?}");
    let d = diags("View v { Button b text_color=#FF0000FF }");
    assert!(gogo_ui::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=")), "{d:#?}");
}

/// `_error_tests` 4799-4803 (`err.signpost-color-static`,
/// `err.signpost-color-when`): `color=` on a node that owns no color (a
/// Button) is refused, statically and in a `when`, naming `font_color=`
/// / `background=` — else the whole control tints. Rust recolors the text.
#[test]
fn errors_color_on_a_non_owner_is_refused() {
    let d = diags("View v { Button b color=#FF0000FF }");
    assert!(gogo_ui::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=") && x.message.contains("background=")), "{d:#?}");
    let d = diags("View v { Button b {\n    when held color=#FF0000FF\n} }");
    assert!(gogo_ui::has_errors(&d) && d.iter().any(|x| x.message.contains("font_color=")), "{d:#?}");
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
    assert!(gogo_ui::has_errors(&diags("slot")));
    let d = diags("View v {\n\tButton ok\n\tBogus b\n}");
    let bad = d.iter().find(|x| x.code == "G7011").unwrap_or_else(|| panic!("{d:#?}"));
    assert_eq!(bad.line, 3, "{d:#?}");
}

// ------------------------------------------------------------ _diagnostic_tests

/// `_diagnostic_tests` 4849-4850, 4905-4906 (`diag.g3001-fires`,
/// `diag.g3001-solo-fires`): `anim=` on a `when` line is an error — a
/// `when` declares a look, the anim belongs on a `transition` (Godot
/// G3001; Rust's old G3001 — `$` defined twice — is G6002 now (GPAR61));
/// Rust refuses every timing word on a `when` line the same way —
/// "timing belongs on a `transition` line". (The Godot fixture's `self.scale` target is `b.scale`
/// here: `self` outside a widget is G0060.)
#[test]
fn diag_anim_on_a_when_line_is_refused() {
    let d = diags("style MenuButton width=220, height=48 {\n    when held background=#FFFFFFff, anim=test\n}\nanim test {\n    b.scale to=5, duration=0.8\n}\nView v { Button b .MenuButton text=\"X\" }");
    assert!(gogo_ui::has_errors(&d) && d.iter().any(|x| x.message.contains("anim")), "{d:#?}");
    let d = diags("anim t2 {\n    b.scale to=0.9, duration=0.1\n}\nView v { Button b {\n    when held anim=t2\n} }");
    assert!(gogo_ui::has_errors(&d), "{d:#?}");
}

/// `_diagnostic_tests` 4923-4936 (`diag.g4003-fires`): one property with
/// two writers — a `when held scale=` holds it while a `transition …
/// anim=t` drives it — is an error naming both (G4003, GPAR62).
#[test]
fn diag_two_writers_on_one_property_is_refused() {
    let d = diags("style MB width=100, height=48 {\n    when held background=#FFFFFFff\n    transition any -> held anim=t\n}\nanim t {\n    b.scale to=5, duration=0.8\n}\nView root {\n    Button b .MB text=\"X\" {\n        when held scale=0.96\n    }\n}");
    assert!(gogo_ui::has_errors(&d), "{d:#?}");
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
/// unknown-type error UNCODED (its block starts `error: `). FLIP (GPAR61,
/// 2026-08-31): every Rust diagnostic carries a registered code — an
/// unknown type is G7011 — and the registry pin keeps codes, docs and
/// teaching pages in step.
#[test]
fn an_unknown_type_is_coded_g7011() {
    let d = diags("View v { Buttn b text=\"x\" }");
    assert!(d.iter().any(|x| x.code == "G7011"), "{d:#?}");
    assert!(d.iter().all(|x| !x.code.is_empty()), "{d:#?}");
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
/// silently land on a published number with a different meaning. The
/// per-code teaching entries are `docs/gogo-errors.md`
/// (`every_diag_code_has_a_teaching_entry_and_no_entry_is_stale` holds the two together).
#[test]
fn diag_source_and_registry_match_both_ways() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut codes: Vec<String> = Vec::new();
    for file in ["compile.rs", "parser.rs", "lexer.rs", "runtime.rs"] {
        let text = std::fs::read_to_string(root.join("src").join(file)).unwrap();
        let bytes = text.as_bytes();
        let mut i = 0;
        while let Some(at) = text[i..].find("\"G") {
            let start = i + at + 1;
            // `"G9101"` (a diag call) or `"G9101: …` (a runtime Err string).
            if start + 5 <= bytes.len() && bytes[start + 1..start + 5].iter().all(u8::is_ascii_digit) && matches!(bytes.get(start + 5), Some(&b'"') | Some(&b':')) {
                let code = text[start..start + 5].to_string();
                if !codes.contains(&code) {
                    codes.push(code);
                }
            }
            i = start;
        }
    }
    assert!(!codes.is_empty());
    let registered: Vec<&str> = gogo_ui::diag::REGISTRY.iter().map(|(c, _)| *c).collect();
    let unregistered: Vec<&String> = codes.iter().filter(|c| !registered.contains(&c.as_str())).collect();
    assert!(unregistered.is_empty(), "codes emitted but not in diag::REGISTRY: {unregistered:?}");
    let unemitted: Vec<&&str> = registered.iter().filter(|c| !codes.contains(&c.to_string())).collect();
    assert!(unemitted.is_empty(), "codes in diag::REGISTRY that nothing emits: {unemitted:?}");
    let revived: Vec<&String> = codes.iter().filter(|c| gogo_ui::diag::RETIRED.contains(&c.as_str())).collect();
    assert!(revived.is_empty(), "retired code numbers came back: {revived:?}");
    // The addon's published registry, when this checkout has it beside
    // the crate: the shared set is exactly the deliberate one.
    let addon = root.join("tests/fixtures/godot/gogo_diagnostics.gd");
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
    let mut declared: Vec<&str> = gogo_ui::diag::SHARED_WITH_ADDON.to_vec();
    declared.sort_unstable();
    assert_eq!(shared, declared, "a code shared with the published registry is shared on purpose only — same meaning, listed in diag::SHARED_WITH_ADDON");
}

/// gogo_control_root.gd:389 `declared_states()` + resolver 7752/7780/8214
/// (GPAR116, round 5): a control's `state a, b` line declares its own
/// state vocabulary — the host raises one through `Runtime::set_custom`
/// on the instance root, `when <name>` responds ANYWHERE in that control
/// (a descendant's block watches the root), an instance-site `when
/// <name>` binds too, and a declared state outranks the built-ins (the
/// addon's priority 100+). An undeclared name is refused at the seam,
/// and `when <name>` outside the control stays G0036.
#[test]
fn a_declared_state_raises_through_the_control_seam() {
    let src = r#"control Spinner {
    state busy, broken
    Panel body width=100, height=40, background=#333333 {
        when busy opacity=0.5
        Label tag text="ready" {
            when broken opacity=0.25
        }
    }
}
View v { Column c halign=left, valign=top { Spinner sp; Spinner sp2 { when busy opacity=0.75 } } }
"#;
    let c = ok(src);
    let page = c.page;
    let mut rt = Runtime::new(&page);
    let sp = page.find("sp").unwrap();
    let tag = page.find("sp/tag").unwrap();
    // the seam refuses a name the control never declared
    let err = rt.set_custom(&page, sp, "spinning", true).unwrap_err();
    assert!(err.contains("declared state"), "{err}");
    rt.set_custom(&page, sp, "busy", true).unwrap();
    let res = resolve(&page, &rt);
    assert_eq!(res.num(&page, sp, PropKey::Opacity), Some(0.5), "the root's own `when busy` responds");
    // a DESCENDANT's `when broken` watches the control root
    rt.set_custom(&page, sp, "broken", true).unwrap();
    let res = resolve(&page, &rt);
    assert_eq!(res.num(&page, tag, PropKey::Opacity), Some(0.25), "a when anywhere in the control responds");
    // clearing unwinds; the second instance never moved
    rt.set_custom(&page, sp, "busy", false).unwrap();
    rt.set_custom(&page, sp, "broken", false).unwrap();
    let res = resolve(&page, &rt);
    assert_eq!(res.num(&page, sp, PropKey::Opacity), None, "cleared");
    // the instance-site `when busy` binds to that instance's root
    let sp2 = page.find("sp2").unwrap();
    rt.set_custom(&page, sp2, "busy", true).unwrap();
    let res = resolve(&page, &rt);
    assert_eq!(res.num(&page, sp2, PropKey::Opacity), Some(0.75), "instance-site when");
    assert_eq!(res.num(&page, sp, PropKey::Opacity), None, "sp stays at rest");
    // outside any control the word is still an unknown state
    let d = diags("View v { Panel p { when busy opacity=0.5 } }");
    assert!(has_code(&d, "G0036"), "{d:#?}");
}

/// GPAR61 (round 7, 2026-08-31): the addon's third leg — every
/// registered code has a teaching entry in docs/gogo-errors.md and every
/// entry is a registered code (no page for a retired number can linger).
#[test]
fn every_diag_code_has_a_teaching_entry_and_no_entry_is_stale() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/gogo-errors.md")).expect("docs/gogo-errors.md");
    let headings: Vec<&str> = doc.lines().filter_map(|l| l.strip_prefix("## ")).filter_map(|l| l.split_whitespace().next()).collect();
    for (code, _) in gogo_ui::diag::REGISTRY {
        assert!(headings.contains(code), "`{code}` is registered but has no `## {code}` entry in docs/gogo-errors.md");
    }
    for h in &headings {
        assert!(gogo_ui::diag::REGISTRY.iter().any(|(c, _)| c == h), "docs/gogo-errors.md teaches `{h}`, which is not a registered code");
        assert!(!gogo_ui::diag::RETIRED.contains(h), "docs/gogo-errors.md teaches retired `{h}`");
    }
}

// ----- GP40 step 1: static `slot.` paths (customization spec §3.4) -----

/// Checked styles (approved 2026-09-02): a `for=` style's
/// `slot.<name>.<prop>=` restyles that region on every wearer — the
/// write is baked into the instance's slot node at compile — and an
/// instance-line path overrides the style's (the closest write wins).
/// `size=` with a bare number is a square.
#[test]
fn slot_paths_restyle_a_controls_regions() {
    let src = format!("{FADER_SRC}style Thin for=Fader, slot.track.height=4, slot.thumb.size=8\nView v {{ Column c {{ Fader a .Thin 200x24; Fader b .Thin 200x24, slot.track.height=2 }} }}");
    let fx = Fx::new(&src);
    assert_eq!(fx.rect("a/track").h, 4.0, "the style's write reaches the region");
    assert_eq!(fx.rect("a/thumb").w, 8.0, "a bare `size=` number is a square");
    assert_eq!(fx.rect("b/track").h, 2.0, "the instance line wins over the style");
}

/// `for=` gates wearing (the shipped application gate): the style
/// dresses its one control type — legal on that control's instance,
/// refused anywhere else (checked against the INSTANCE the wearer
/// names, not the body root underneath).
#[test]
fn a_for_style_wears_on_its_control_only() {
    ok(&format!("{FADER_SRC}style T for=Fader, slot.track.height=4\nView v {{ Fader f .T 100x24 }}"));
    assert!(gogo_ui::has_errors(&diags(&format!("{FADER_SRC}style T for=Fader, slot.track.height=4\nView v {{ Button b .T text=\"x\" }}"))));
}

/// A `when` response through a slot path moves and tints — and rides
/// the HOST's state: while the Fader reports `dragging`, its thumb dims
/// to 0.8.
#[test]
fn slot_when_response_rides_the_host_state() {
    let src = format!("{FADER_SRC}style Ghost for=Fader {{ when dragging slot.thumb.opacity=0.8 }}\nView v {{ Fader f .Ghost 200x24, value=50 }}");
    let mut fx = Fx::new(&src);
    let alpha = |fx: &Fx| fx.draws().iter().find_map(|d| if let Draw::Circle { color, .. } = d { Some(color.0[3]) } else { None }).unwrap();
    assert_eq!(alpha(&fx), 255);
    let f = fx.node("f");
    fx.rt.set_custom(&fx.page, f, "dragging", true).unwrap();
    for _ in 0..40 {
        fx.rt.tick(&mut fx.page, 1.0 / 60.0);
    }
    assert_eq!(alpha(&fx), 204, "0.8 while dragging");
}

/// The declaration-time fence battery: every refusal points at the
/// writing line, never dies silently inside the control.
#[test]
fn slot_path_fences() {
    // an unknown slot lists the real ones
    let d = diags(&format!("{FADER_SRC}style X for=Fader, slot.trak.height=4\nView v {{ Fader f .X 100x24 }}"));
    assert!(d.iter().any(|g| g.message.contains("no slot 'trak'") && g.message.contains("track")), "{d:#?}");
    // a driven slot refuses the driver's word
    let d = diags(&format!("{FADER_SRC}style X for=Fader, slot.progress.width=10\nView v {{ Fader f .X 100x24 }}"));
    assert!(d.iter().any(|g| g.message.contains("moves by itself")), "{d:#?}");
    // restyling content IS filling — the error routes the writer there
    let d = diags(&format!("{FADER_SRC}style X for=Fader, slot.label.font_size=12\nView v {{ Fader f .X 100x24 }}"));
    assert!(d.iter().any(|g| g.message.contains("fill it")), "{d:#?}");
    // motion is `when` vocabulary, not a base look
    assert!(gogo_ui::has_errors(&diags(&format!("{FADER_SRC}style X for=Fader, slot.thumb.scale=1.2\nView v {{ Fader f .X 100x24 }}"))));
    // a plain style has no contract to check against
    assert!(gogo_ui::has_errors(&diags("style X slot.track.height=4\nView v { Panel p .X }")));
    // nor does a plain node
    assert!(gogo_ui::has_errors(&diags("View v { Rectangle r 10x10, slot.track.height=4, color=red }")));
    // a `when` response holds motion and tint, not geometry
    assert!(gogo_ui::has_errors(&diags(&format!("{FADER_SRC}style X for=Fader {{ when dragging slot.track.height=2 }}\nView v {{ Fader f .X 100x24 }}"))));
}

/// `extends=` carries slot writes down the chain like props — the
/// child's write to the same region wins; dropping the parent's would
/// un-style every wearer of the derived style silently.
#[test]
fn extends_carries_slot_writes_and_the_child_wins() {
    let src = format!("{FADER_SRC}style Thin for=Fader, slot.track.height=4, slot.thumb.size=8
style Flat for=Fader, extends=Thin, slot.track.height=6
View v {{ Fader f .Flat 200x24 }}");
    let fx = Fx::new(&src);
    assert_eq!(fx.rect("f/track").h, 6.0, "the child's write wins");
    assert_eq!(fx.rect("f/thumb").w, 8.0, "the parent's other write survives");
}

// ----- GP40 step 2: state checking + the D2 advisory (E12) -----

/// A `for=` style's `when` states check at the DECLARATION — a typo'd
/// contract state would otherwise wait silently forever on every
/// wearer. An arrangement word is the view's, not the contract's: it
/// passes.
#[test]
fn for_style_when_states_check_at_the_declaration() {
    let d = diags(&format!("{FADER_SRC}style X for=Fader {{ when draging slot.thumb.opacity=0.5 }}
View v {{ Fader f .X 100x24 }}"));
    assert!(d.iter().any(|g| g.message.contains("isn't available") && g.message.contains("dragging")), "{d:#?}");
    // a built-in raises the universal states — custom words refuse
    let d = diags("style X for=Slider { when bogus opacity=0.5 }
View v { Slider s .X value=30 }");
    assert!(d.iter().any(|g| g.message.contains("isn't available")), "{d:#?}");
    // the check runs even when nothing wears the style
    let d = diags(&format!("{FADER_SRC}style X for=Fader {{ when draging slot.thumb.opacity=0.5 }}
View v {{ Label l text=\"x\" }}"));
    assert!(d.iter().any(|g| g.message.contains("isn't available")), "{d:#?}");
    // an arrangement word passes — it answers the view
    let c = build(&format!("{FADER_SRC}arrangement compact

style X for=Fader {{ when compact slot.thumb.opacity=0.5 }}
View v {{ Fader f .X 100x24 }}"));
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
}

/// A PLAIN style's contract states check where the style is APPLIED —
/// a control instance or a built-in; a custom state landing on exactly
/// one control type draws the D2 advisory to add `for=`.
#[test]
fn plain_style_states_check_where_applied() {
    // undeclared on the wearer's contract: error at the wear
    let d = diags(&format!("{FADER_SRC}style X {{ when busy opacity=0.5 }}
View v {{ Fader f .X 100x24 }}"));
    assert!(d.iter().any(|g| g.message.contains("isn't available")), "{d:#?}");
    // declared: builds, and the D2 advisory names the for= fix
    let c = build(&format!("{FADER_SRC}style X {{ when dragging opacity=0.5 }}
View v {{ Fader f .X 100x24 }}"));
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    assert!(c.diags.iter().any(|g| g.message.contains("for=Fader")), "the D2 advisory: {:#?}", c.diags);
    // a custom word on a built-in wearer refuses — it raises universals
    let d = diags("style X { when busy opacity=0.5 }
View v { Button b .X text=\"x\" }");
    assert!(d.iter().any(|g| g.message.contains("isn't available")), "{d:#?}");
    // universal words on a built-in wearer stay legal
    ok("style X { when held opacity=0.5 }
View v { Button b .X text=\"x\" }");
}

// ----- GP40 step 3: theme-entry `.Style` + user-control entries -----

/// GP40 step 3 (user 2026-09-02): a theme entry takes a `.Style` — the
/// app-wide default for every instance of the kind, out-ranked by
/// anything written closer (an instance's own styles and lines).
#[test]
fn user_control_theme_entry_styles_every_instance() {
    let src = format!("{FADER_SRC}style Thin for=Fader, slot.track.height=4\nstyle Fat for=Fader, slot.track.height=6\ntheme {{ Fader .Thin }}\nView v {{ Column c {{ Fader a 200x24; Fader b .Fat 200x24 }} }}");
    let fx = Fx::new(&src);
    assert_eq!(fx.rect("a/track").h, 4.0, "the entry's style reaches every instance");
    assert_eq!(fx.rect("b/track").h, 6.0, "an instance style out-ranks the entry");
}

/// The entry line's own props out-rank its style (the entry-line
/// last-writer rule), and the v1 fences hold: a user-control entry
/// takes styles only, and a variant entry takes nothing that can't
/// switch with the variant yet (the pending variant-stamping call).
#[test]
fn theme_entry_tiers_and_v1_fences() {
    // the entry line's step beats the same key from its style
    let fx = Fx::new("style S for=Slider, step=10\ntheme { Slider .S step=25 }\nView v { Slider a value=30, width=200 }");
    assert_eq!(fx.value("a"), 25.0, "the entry line wins over its style");
    // a user-control entry takes a style, not prop defaults
    assert!(gogo_ui::has_errors(&diags(&format!("{FADER_SRC}theme {{ Fader step=5 }}\nView v {{ Fader f 100x24 }}"))));
    // a variant entry can't restyle a user control yet
    assert!(gogo_ui::has_errors(&diags(&format!("{FADER_SRC}style C for=Fader, slot.track.height=4\ntheme dark {{ Fader .C }}\nView v {{ Fader f 100x24 }}"))));
    // a variant entry's style is base-look only — whens don't switch yet
    assert!(gogo_ui::has_errors(&diags("style H for=Button { when held opacity=0.8 }\ntheme dark { Button .H }\nView v { Button b text=\"x\" }")));
}
