//! Port of the Godot gogo suite's input-model batch
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//!
//! - `_hit_through_tests` 8490-8591 (22 checks)
//! - `_mouse_filter_retirement_tests` 8591-8619 (10)
//! - `_interactive_when_tests` 8619-8667 (7)
//! - `_interactive_lint_tests` 8667-8728 (8)
//! - `_input_hint_and_overlay_tests` 8728-8794 (13)
//! - `_safe_inset_tests` 8794-8969 (18)
//!
//! Godot reads `mouse_filter` off the built Control; Rust has no filter
//! word (D103 ONE model, todo-gogo.md Decisions `on:pointer`), so every
//! "is transparent / claims" check here is a `hit()` probe: a touch at the
//! node's center lands on it, on the control around it, or on nothing.
//! Godot's host-fed safe insets are `Runtime::safe` here.
//!
//! ENGINE (not ported) — Godot machinery with no Rust counterpart:
//! - `ihint.once-per-node` (8757): press-miss hint session bookkeeping (GP42 LATER).
//! - `ioverlay.added/active/banner-gated/removed` (8767-8774): input overlay child node (GP43 CUT dev overlays).
//! - `ioverlay.empty-banner/-top/-inside` (8791-8796): overlay banner plate rect (GP43 CUT dev overlays).
use gogo::draw::hit;
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Page, PropKey};
use gogo::runtime::{Resolved, Runtime, resolve};
use gogo::{Compiled, Input, Phase, Severity, Touch, compile};

fn build(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn warnings(src: &str) -> Vec<String> {
    let c = build(src);
    c.diags.iter().filter(|d| d.severity == Severity::Warning).map(|d| format!("{} {}", d.code, d.message)).collect()
}

/// compile → Runtime → resolve → layout → hit / touch, the input.rs `Fx`
/// idiom with a hit-by-name probe on top.
struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    frame: Rect,
}

impl Fx {
    fn new(src: &str, w: f32, h: f32) -> Fx {
        let c = build(src);
        let rt = Runtime::new(&c.page);
        Fx { page: c.page, rt, input: Input::new(), frame: Rect::new(0.0, 0.0, w, h) }
    }

    fn lay(&self) -> (Resolved, Vec<Rect>) {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, self.frame, &FixedAdvance);
        (res, rects)
    }

    fn id(&self, name: &str) -> gogo::NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node named `{name}`"))
    }

    fn rect(&self, name: &str) -> Rect {
        self.lay().1[self.id(name)]
    }

    fn name_of(&self, id: gogo::NodeId) -> String {
        self.page.str(self.page.nodes[id].name).to_string()
    }

    /// The name of the node a touch at (x, y) lands on (`any` = the
    /// top-most painted node of any kind, input-taking or not).
    fn hit_at(&self, x: f32, y: f32, any: bool) -> Option<String> {
        let (res, rects) = self.lay();
        hit(&self.page, &res, &rects, x, y, any).map(|n| self.name_of(n))
    }

    /// A touch at the named node's center.
    fn hit_center(&self, name: &str) -> Option<String> {
        let r = self.rect(name);
        assert!(r.w > 0.0 && r.h > 0.0, "`{name}` has no area to touch: {r:?}");
        self.hit_at(r.x + r.w / 2.0, r.y + r.h / 2.0, false)
    }

    fn interactive(&self, name: &str) -> Option<bool> {
        let (res, _) = self.lay();
        res.bool(&self.page, self.id(name), PropKey::Interactive)
    }

    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) {
        let (res, rects) = self.lay();
        self.input.touch(&self.page, &mut self.rt, &res, &rects, &FixedAdvance, Touch { phase, x, y, t, id: 1 });
    }

    /// Every distinct node a touch lands on across a 10-epx grid over the
    /// frame — the Godot overlay's "capturing regions", by sampling.
    fn capturing(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut y = 5.0;
        while y < self.frame.h {
            let mut x = 5.0;
            while x < self.frame.w {
                if let Some(n) = self.hit_at(x, y, false) {
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
                x += 10.0;
            }
            y += 10.0;
        }
        out
    }
}

// ----- _hit_through_tests 8490-8591 ------------------------------------------

/// `_hit_through_tests` 8490-8550 (hit.ok, hit.spacer-transparent …
/// hit.layers-surface-claims; 17 checks): the ONE input model. Passive
/// kinds are transparent everywhere — inside a Button its content cannot
/// steal the press (the touch lands on the Button), outside it a touch on
/// a Layers/Row/Panel/Gradient/Circle/Grid/Tiles lands on nothing;
/// Buttons claim; `interactive=true` makes a Rectangle or Layers claim,
/// `interactive=false` makes a Button hit-invisible, and the resolved
/// `interactive` value carries the override (Godot's meta provenance).
/// Dialect: `Control` → `Layers`; the bodiless Row/Panel get a height so
/// their center is a real point to touch.
#[test]
fn hit_through_passive_content_and_interactive_overrides() {
    let fx = Fx::new(
        "View v {\n Column col {\n\
Button b 50x90 {\n    Column pad {\n        Layers spacer min_height=43\n        Image pic\n        Rectangle swatch 8x8\n    }\n}\n\
Layers outside 20x20\n\
Row outsideRow height=20\n\
Panel plainPanel height=20\n\
Gradient grad stops=[white, black], 8x8\n\
Circle disc color=red, 8x8\n\
Grid grid cols=[hug] { row { Label g text=\"g\" } }\n\
Tiles tiles cols=1 { Rectangle t 8x8 }\n\
Rectangle catcher interactive=true, touch_target=off, 20x20\n\
Button ghost text=\"x\", interactive=false\n\
Button live text=\"y\", min_width=44, min_height=44\n\
Layers surface interactive=true, touch_target=off, height=40\n\
 }\n}\n",
        400.0,
        800.0,
    );
    // passive everywhere — inside the button the Button takes the touch…
    for inner in ["spacer", "swatch", "pad"] {
        assert_eq!(fx.hit_center(inner).as_deref(), Some("b"), "`{inner}` must not steal the press from `b`");
    }
    // (an Image with no source has no height; touch where it sits)
    let pic = fx.rect("pic");
    assert_eq!(fx.hit_at(pic.x + 4.0, pic.y, false).as_deref(), Some("b"), "the Image inside the Button");
    // …and outside (the D103 flip — these were engine STOP/PASS before).
    for passive in ["outside", "outsideRow", "plainPanel", "grad", "disc", "grid", "tiles"] {
        assert_eq!(fx.hit_center(passive), None, "`{passive}` is transparent");
    }
    // interactive controls keep their behavior
    assert_eq!(fx.hit_center("b").as_deref(), Some("b"));
    assert_eq!(fx.hit_center("live").as_deref(), Some("live"));
    // interactive= overrides per control, with the value readable back
    assert_eq!(fx.hit_center("catcher").as_deref(), Some("catcher"));
    assert_eq!(fx.interactive("catcher"), Some(true));
    assert_eq!(fx.hit_center("ghost"), None, "interactive=false: hit-invisible");
    assert_eq!(fx.interactive("ghost"), Some(false));
    assert_eq!(fx.hit_center("surface").as_deref(), Some("surface"));
}

/// `_hit_through_tests` 8551-8586 (hit.widget-ok, hit.widget-spacer-
/// transparent, hit.widget-slot-transparent, hit.widget-surface-root,
/// hit.instance-override-wins; 5 checks): widget body content AND slotted
/// instance children are transparent (the touch lands on the widget's
/// Button), a Layers root declared `interactive=true` is the surface, and
/// an instance-line `interactive=false` passes through to that root.
/// Dialect: `Button signals 50x90` needs the comma Rust's parser wants
/// after a flag (`Button signals, 50x90`); `Control` → `Layers`.
#[test]
fn hit_through_widget_bodies_and_instance_override() {
    let fx = Fx::new(
        "widget IconBtn {\n    Button signals, 50x90 {\n        Layers spacer min_height=19\n        slot\n    }\n}\n\
widget DragRow {\n    Layers root interactive=true, touch_target=off, height=40 {\n        Label lbl text=\"x\"\n    }\n}\n\
View v { Column c {\nIconBtn one {\n    Layers slotPad 10x10\n}\nDragRow rowOn\nDragRow rowOff interactive=false\n} }\n",
        400.0,
        800.0,
    );
    assert_eq!(fx.hit_center("one/spacer").as_deref(), Some("one"), "body content is transparent");
    assert_eq!(fx.hit_center("slotPad").as_deref(), Some("one"), "slotted content is transparent");
    assert_eq!(fx.hit_center("rowOn").as_deref(), Some("rowOn"), "the declared root surface claims");
    assert_eq!(fx.hit_center("rowOff"), None, "the instance line's interactive=false wins");
    assert_eq!(fx.hit_center("rowOff/lbl"), None);
}

// ----- _mouse_filter_retirement_tests 8591-8619 -------------------------------

/// `_mouse_filter_retirement_tests` 8593-8606 (retire.stop, retire.ignore,
/// retire.pass, retire.style-spelling, retire.instance-spelling,
/// retire.when-self; 6 checks): every legacy `mouse_filter=` spelling
/// FAILS the build — on a node line, in a style, on a widget instance
/// line, in a `when`. Rust only warns (G2001 "not a known property") and
/// builds the page.
#[test]
fn mouse_filter_spellings_are_refused() {
    for src in [
        "View v { Panel p mouse_filter=stop }",
        "View v { Rectangle r mouse_filter=ignore }",
        "View v { Row r mouse_filter=pass }",
        "style S mouse_filter=stop\nView v { Rectangle r .S }",
        "widget W { Panel p }\nView v { W inst mouse_filter=ignore }",
        "View v { Button b text=\"x\" {\n    when held mouse_filter=ignore\n} }",
    ] {
        let c = compile(src, &|_| None);
        assert!(gogo::has_errors(&c.diags), "must not build: {src}\n{:#?}", c.diags);
    }
}

/// `_mouse_filter_retirement_tests` 8607-8608 (retire.when-child): the
/// child-target spelling `when held flash.mouse_filter=stop` fails the
/// build naming `interactive=true`. Rust says nothing at all on this
/// path — not even the G2001 warning the node-line spelling gets.
#[test]
fn mouse_filter_in_a_child_target_when_is_refused() {
    let c = compile("View v { Button b text=\"x\" {\n    when held flash.mouse_filter=stop\n}\n Rectangle flash 8x8 }", &|_| None);
    assert!(!c.diags.is_empty(), "a retired word inside a when must be diagnosed");
    assert!(gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

/// `_mouse_filter_retirement_tests` 8610-8611 (retire.bare-word): a bare
/// `interactive` after the name (no `=`) never becomes a value — the
/// build fails. Rust refuses it as an unexpected value (G0046); Godot's
/// message additionally names the `interactive=true` fix.
#[test]
fn bare_interactive_word_is_refused() {
    let c = compile("View v { Button b interactive }", &|_| None);
    assert!(gogo::has_errors(&c.diags), "{:#?}", c.diags);
    assert!(codes("View v { Button b interactive }").contains(&"G0046".to_string()));
}

/// `_mouse_filter_retirement_tests` 8612-8613 (retire.bare-as-name): a
/// bare `interactive` in the NAME position is the same typo and fails the
/// build naming `interactive=true` — Rust's parser nets it too (E4).
#[test]
fn bare_interactive_as_name_is_refused() {
    let c = compile("View v { Button interactive }", &|_| None);
    assert!(gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

/// `_mouse_filter_retirement_tests` 8614-8615 (retire.type-check):
/// `interactive=maybe` is a type error (the message names `true`). Rust
/// stores the word and builds.
#[test]
fn interactive_wants_a_bool() {
    let c = compile("View v { Button b interactive=maybe }", &|_| None);
    assert!(gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

// ----- _interactive_when_tests 8619-8667 -------------------------------------

/// `_interactive_when_tests` 8621-8640 (iwhen.child-ok, iwhen.child-base,
/// iwhen.child-held, iwhen.child-restored; 4 checks): `interactive` is a
/// plain bool in a when's child-target form — a press on `b` turns the
/// passive `shield` (a SIBLING of `b`, as the Godot fixture writes it)
/// into a catcher, and the release hands the base value back. Rust
/// compiles the line clean but attaches the response to the View as its
/// owner (`When.of` = 0), so `held` on `b` never reaches it.
#[test]
fn when_held_turns_a_sibling_shield_into_a_catcher() {
    let mut fx = Fx::new(
        "View v {\nButton b text=\"x\", touch_target=off, width=100, height=40, halign=left, valign=top {\n    when held shield.interactive=true\n}\nRectangle shield 50x50, halign=right, valign=bottom\n}",
        400.0,
        800.0,
    );
    assert_eq!(fx.hit_center("shield"), None, "base: transparent");
    fx.touch(Phase::Down, 10.0, 10.0, 0.0);
    assert_eq!(fx.hit_center("shield").as_deref(), Some("shield"), "held: the shield catches");
    fx.touch(Phase::Up, 10.0, 10.0, 0.1);
    assert_eq!(fx.hit_center("shield"), None, "restored");
}

/// `_interactive_when_tests` 8642-8652 (iwhen.self-ok, iwhen.self-held,
/// iwhen.self-restored; 3 checks): the self form — a Button that writes
/// `when held interactive=false` parks itself hit-invisible for as long
/// as the finger holds it and takes input again on release. Also pins
/// (not a Godot check) the child-target form on a DESCENDANT, so the
/// sibling failure above is isolated to the target's scope.
#[test]
fn when_held_interactive_false_parks_the_control_hit_invisible() {
    let mut fx = Fx::new("View v { Button b text=\"x\", touch_target=off, width=100, height=40, halign=left, valign=top {\n    when held interactive=false\n} }", 400.0, 800.0);
    assert_eq!(fx.hit_center("b").as_deref(), Some("b"));
    fx.touch(Phase::Down, 10.0, 10.0, 0.0);
    assert_eq!(fx.hit_center("b"), None, "held: hit-invisible");
    fx.touch(Phase::Up, 10.0, 10.0, 0.1);
    assert_eq!(fx.hit_center("b").as_deref(), Some("b"), "released: the base value is back");

    let mut fx = Fx::new("View v {\nButton b touch_target=off, width=100, height=100, halign=left, valign=top {\n    when held shield.interactive=true\n    Rectangle shield 50x50, halign=right, valign=bottom\n}\n}", 400.0, 800.0);
    assert_eq!(fx.hit_center("shield").as_deref(), Some("b"), "base: the passive child hands the touch to the Button");
    fx.touch(Phase::Down, 10.0, 10.0, 0.0);
    assert_eq!(fx.hit_center("shield").as_deref(), Some("shield"), "held: the descendant catches");
    fx.touch(Phase::Up, 10.0, 10.0, 0.1);
    assert_eq!(fx.hit_center("shield").as_deref(), Some("b"), "restored");
}

// ----- _interactive_lint_tests 8667-8728 -------------------------------------

/// `_interactive_lint_tests` 8676-8687 (ilint.noop-warns,
/// ilint.instance-noop-warns; 2 checks): `interactive=false` on a node
/// that is already transparent (a Rectangle; a widget instance whose root
/// is a passive Panel) is a no-op and gets exactly one advisory warning
/// naming the catcher misread. Rust has no such lint.
#[test]
#[ignore = "GP41: not ported: the `interactive=false` no-op warning"]
fn interactive_false_on_a_passive_node_warns() {
    let w = warnings("View v { Rectangle preview interactive=false, 8x8 }");
    assert_eq!(w.len(), 1, "{w:#?}");
    let w = warnings("widget W { Panel p }\nView v { W card interactive=false }");
    assert_eq!(w.len(), 1, "{w:#?}");
}

/// `_interactive_lint_tests` 8689-8692 (ilint.redundant-true-silent):
/// `interactive=true` on a Button is self-documentation, not a mistake —
/// no warning.
#[test]
fn redundant_interactive_true_stays_silent() {
    let w = warnings("View v { Button b text=\"x\", interactive=true, min_width=44, min_height=44 }");
    assert!(w.is_empty(), "{w:#?}");
}

/// `_interactive_lint_tests` 8696-8700 (ilint.touch-warns): a declared
/// surface (`Rectangle interactive=true, 30x30`) joins the 44-epx touch
/// target lint — one warning. Rust has no touch-target lint.
#[test]
#[ignore = "GP41: not ported: the 44-epx touch-target lint on declared surfaces"]
fn small_declared_surface_gets_the_touch_target_lint() {
    let w = warnings("View v { Rectangle grab interactive=true, 30x30 }");
    assert_eq!(w.len(), 1, "{w:#?}");
    assert!(w[0].contains("touch target"), "{w:#?}");
}

/// `_interactive_lint_tests` 8701-8708 (ilint.touch-box-passes,
/// ilint.touch-off-silences; 2 checks): a 60x60 declared surface passes
/// the touch lint, and `touch_target=off` opts a 30x30 one out — both
/// build with no warning.
#[test]
fn sized_or_opted_out_surfaces_pass_the_touch_lint() {
    let w = warnings("View v { Rectangle plate interactive=true, 60x60 }");
    assert!(w.is_empty(), "{w:#?}");
    let w = warnings("View v { Rectangle grab interactive=true, touch_target=off, 30x30 }");
    assert!(w.is_empty(), "{w:#?}");
}

/// `_interactive_lint_tests` 8712-8727 (ilint.dead-wire-warns,
/// ilint.dead-wire-explicit; 2 checks): a raw input handler wired to a
/// node with no input surface is provably dead and gets a diagnostic
/// naming `interactive=true`. FLIP: Godot WARNS (advisory); Rust makes it
/// the G0073 ERROR by decision (todo-gogo.md Decisions, `on:pointer`
/// 2026-08-30: "G0073 refuses `on:pointer` without `interactive=true`").
/// Dialect: Godot's auto-wire by host method name and `on:gui_input=`
/// are both the declared `on:pointer=` here; a wired Layers surface and a
/// Button beside the dead Label draw no diagnostic of their own.
#[test]
fn pointer_wire_on_a_passive_node_is_refused() {
    let src = "View v { Column c {\nLabel deadLabel text=\"x\", on:pointer=OnDeadLabelGuiInput\nLayers liveRow interactive=true, touch_target=off, height=40, on:pointer=OnLiveRowGuiInput\nButton ok text=\"ok\", min_width=44, min_height=44, on:activate=OnOkActivate\n} }";
    let c = compile(src, &|_| None);
    let errors: Vec<&gogo::Diagnostic> = c.diags.iter().filter(|d| d.severity == Severity::Error).collect();
    assert_eq!(errors.len(), 1, "{:#?}", c.diags);
    assert_eq!(errors[0].code, "G0073");
    assert_eq!(errors[0].line, 2, "the dead Label's line");
    assert!(errors[0].message.contains("interactive=true"), "{}", errors[0].message);
    let c = compile("View v { Rectangle zone 60x60, on:pointer=OnDeadLabelGuiInput }", &|_| None);
    assert_eq!(c.diags.iter().filter(|d| d.code == "G0073").count(), 1, "{:#?}", c.diags);
}

// ----- _input_hint_and_overlay_tests 8728-8794 --------------------------------

/// `_input_hint_and_overlay_tests` 8735-8760 (ihint.ok,
/// ihint.names-topmost, ihint.structure-silent; 3 checks): the press-miss
/// hint's two facts, as `hit()` answers them. A touch on the passive plate
/// lands on nothing, and the top-most PAINT under it is the plate (what
/// the hint names, with the `interactive=true` fix); a touch on bare
/// structure (the host Layers) has no paint on top, so the hint stays
/// silent. Dialect: `Control` → `Layers`.
#[test]
fn press_miss_names_the_topmost_paint_and_stays_silent_on_structure() {
    let fx = Fx::new(
        "View v { Layers host 200x100, halign=left, valign=top {\n    Rectangle plate color=#112233FF, 100x50\n    Button ok text=\"ok\", min_width=44, min_height=44, halign=right, valign=bottom\n} }",
        400.0,
        800.0,
    );
    assert_eq!(fx.hit_at(90.0, 45.0, false), None, "the plate is passive");
    assert_eq!(fx.hit_at(90.0, 45.0, true).as_deref(), Some("plate"), "the hint names the top-most paint");
    assert_eq!(fx.hit_at(150.0, 20.0, false), None);
    assert_eq!(fx.hit_at(150.0, 20.0, true).as_deref(), Some("host"), "bare structure: nothing to name");
}

/// `_input_hint_and_overlay_tests` 8763-8764 + 8781-8782
/// (ioverlay.regions, ioverlay.empty-ok; 2 checks): the interactive-region
/// overlay tints capturing regions and only those — sampled over the
/// host, the Button is the one node any touch lands on (never the passive
/// plate); a view with only passive paint has zero capturing regions.
#[test]
fn capturing_regions_are_the_button_alone() {
    let fx = Fx::new(
        "View v { Layers host 200x100, halign=left, valign=top {\n    Rectangle plate color=#112233FF, 100x50\n    Button ok text=\"ok\", min_width=44, min_height=44, halign=right, valign=bottom\n} }",
        200.0,
        100.0,
    );
    assert_eq!(fx.capturing(), vec!["ok".to_string()]);
    let quiet = Fx::new("View v { Layers quiet {\n    Rectangle art color=#112233FF, 100x50\n} }", 400.0, 800.0);
    assert!(quiet.capturing().is_empty(), "{:?}", quiet.capturing());
}

// ----- _safe_inset_tests 8794-8969 -------------------------------------------

/// `_safe_inset_tests` 8806-8827 (safe.padding-ok, safe.padding-edges,
/// safe.padding-design-edge, safe.padding-all-ok, safe.padding-all-edges;
/// 5 checks): `padding_<edge>=safe` reads that edge's own host inset
/// (3/5/7/11 for left/top/right/bottom) and coexists with a design value
/// on another edge; `padding=safe` is sugar for the four per-edge keys,
/// each its own inset, never one value on all four. Read off the layout:
/// the child sits at (left, top) inside the panel and the panel hugs to
/// child + insets.
#[test]
fn safe_padding_reads_each_edge_own_inset() {
    let mut fx = Fx::new("View v { Panel b padding_left=safe, padding_top=safe, padding_bottom=20, halign=left, valign=top { Layers c min_width=40, min_height=30 } }", 400.0, 800.0);
    fx.rt.safe = [3.0, 5.0, 7.0, 11.0];
    let c = fx.rect("c");
    let b = fx.rect("b");
    assert_eq!((c.x, c.y), (3.0, 5.0), "left/top read their insets: {c:?}");
    assert_eq!((b.w, b.h), (43.0, 55.0), "right stays 0, bottom keeps the design 20: {b:?}");

    let mut fx = Fx::new("View v { Panel p padding=safe, halign=left, valign=top { Layers c min_width=40, min_height=30 } }", 400.0, 800.0);
    fx.rt.safe = [3.0, 5.0, 7.0, 11.0];
    let c = fx.rect("c");
    let p = fx.rect("p");
    assert_eq!((c.x, c.y), (3.0, 5.0), "{c:?}");
    assert_eq!((p.w, p.h), (40.0 + 3.0 + 7.0, 30.0 + 5.0 + 11.0), "each edge its OWN inset: {p:?}");
}

/// `_safe_inset_tests` 8831-8840 (safe.hug-ok, safe.hug-min; 2 checks):
/// safe padding counts into the hug minimum like any padding — a
/// `padding=safe` Panel around a 40x30 child is 50x46 under 3/5/7/11.
#[test]
fn safe_padding_counts_into_the_hug_minimum() {
    let mut fx = Fx::new("View v { Panel h padding=safe, halign=left, valign=top { Layers c min_width=40, min_height=30 } }", 400.0, 800.0);
    fx.rt.safe = [3.0, 5.0, 7.0, 11.0];
    let h = fx.rect("h");
    assert_eq!((h.w, h.h), (40.0 + 10.0, 30.0 + 16.0), "{h:?}");
}

/// `_safe_inset_tests` 8844-8874 (safe.margin-ok, safe.margin-carves,
/// safe.margin-live; 3 checks): `margin=safe` is the old SafeArea spelled
/// with margins — a free child fills and every edge carves by its own
/// inset (pad at (3,5), 190x84 in a 200x100 host); pushing new insets
/// re-lays it out ((4,6), 188x82).
#[test]
fn safe_margin_carves_each_edge_and_follows_a_live_push() {
    let mut fx = Fx::new("View host min_width=200, min_height=100 {\n    Layers pad margin=safe {\n        Layers c min_width=10, min_height=10\n    }\n}\n", 200.0, 100.0);
    fx.rt.safe = [3.0, 5.0, 7.0, 11.0];
    let pad = fx.rect("pad");
    assert_eq!((pad.x, pad.y, pad.w, pad.h), (3.0, 5.0, 200.0 - 10.0, 100.0 - 16.0), "{pad:?}");
    fx.rt.safe = [4.0, 6.0, 8.0, 12.0];
    let pad = fx.rect("pad");
    assert_eq!((pad.x, pad.y, pad.w, pad.h), (4.0, 6.0, 200.0 - 12.0, 100.0 - 18.0), "live push: {pad:?}");
}

/// `_safe_inset_tests` 8882-8907 (safe.margin-fill-ok,
/// safe.margin-fill-left-px, safe.margin-fill-right-px; 3 checks):
/// `margin_left=safe` / `margin_right=safe` on an explicit fill axis move
/// the box by exactly the fed inset (60 / 80) and a design margin inside
/// adds on top (the inner box 10 further in). Godot ran this under a ×2
/// epx provider to catch a double conversion of its host-fed PIXELS;
/// Rust has no provider — insets are epx by contract (`Runtime::safe`),
/// so the number that goes in is the number that comes out.
#[test]
fn safe_margin_on_a_fill_axis_is_the_fed_inset_unconverted() {
    let mut fx = Fx::new(
        "View host2 min_width=400, min_height=100 {\n    Layers inset width=fill, height=fill, margin_left=safe {\n        Rectangle m color=#FFFFFFFF, margin_left=10, margin_right=10, height=30, valign=top\n    }\n    Layers rinset width=fill, height=fill, margin_right=safe {\n        Rectangle rm color=#FFFFFFFF, width=20, halign=right, height=30, valign=top\n    }\n}\n",
        400.0,
        100.0,
    );
    fx.rt.safe = [60.0, 0.0, 80.0, 0.0];
    let inset = fx.rect("inset");
    let m = fx.rect("m");
    assert_eq!(inset.x, 60.0, "{inset:?}");
    // Godot read m.offset_left == 20 px: its design margin of 10 epx under
    // the ×2 provider. In epx that is 10, inside the safe 60.
    assert_eq!(m.x - inset.x, 10.0, "the inner design margin sits inside the safe one: {m:?}");
    assert_eq!(m.x, 70.0, "{m:?}");
    let rinset = fx.rect("rinset");
    assert_eq!(rinset.right(), 400.0 - 80.0, "{rinset:?}");
    assert_eq!(fx.rect("rm").right(), 320.0);
}

/// `_safe_inset_tests` 8911-8912 (safe.edgeless-errors): `safe` is a
/// per-edge inset, so `spacing=safe` fails the build ("names no screen
/// edge"). Rust stores the word and lays out with spacing 0.
#[test]
fn safe_on_a_key_that_names_no_edge_is_refused() {
    let c = compile("View v { Column c spacing=safe }", &|_| None);
    assert!(gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

/// `_safe_inset_tests` 8913-8914 (safe.retired-wrapper): the `SafeArea`
/// wrapper fails the build. FLIP: Godot routes the retired word to
/// `margin=safe`; Rust has no redirect table (gogo-design.md §7 "unknown
/// word = build error, no alias table") — G7011 unknown node type.
#[test]
fn safe_area_wrapper_is_not_a_node_type() {
    assert!(codes("View v { SafeArea s { Layers c } }").contains(&"G7011".to_string()));
}

/// `_safe_inset_tests` 8915-8916 (safe.container-margin-still-fenced):
/// margins under a Column stay an error, safe or not (Rust G1014).
/// Dialect: `Control` → `Layers`.
#[test]
fn safe_margin_under_a_column_stays_fenced() {
    assert!(codes("View v { Column c { Layers k margin_left=safe } }").contains(&"G1014".to_string()));
}

/// `_safe_inset_tests` 8923-8953 (safe.scroll-left-inset-ok,
/// safe.scroll-inset-left-only; 2 checks): the user-caught D73 regression
/// — a LEFT-only inset (20) pads the scroll content's left edge only in
/// the brush-select skeleton (hugging panel, vertical Scroll, safe padding
/// on the content Panel, exact-width rows): the panel hugs to inset + row
/// width (80), rows sit AT the inset and flush right (right gap 0).
/// Dialect: `Control` → `View` root.
#[test]
fn left_only_safe_padding_pads_scroll_content_left_edge_only() {
    let mut fx = Fx::new(
        "View screen min_width=400, min_height=300 {\n    Panel panel halign=left, height=fill {\n        Rectangle bg color=#333333FF\n        Scroll sv allow=vertical, bars=never {\n            Panel wrap padding_left=safe, padding_top=safe, padding_bottom=20 {\n                Column list spacing=0 {\n                    Button row1 width=60, height=50\n                    Button row2 width=60, height=50\n                }\n            }\n        }\n    }\n}\n",
        400.0,
        300.0,
    );
    fx.rt.safe = [20.0, 0.0, 0.0, 0.0];
    let panel = fx.rect("panel");
    let row = fx.rect("row1");
    let row_left = row.x - panel.x;
    let right_gap = panel.right() - row.right();
    assert_eq!(row_left, 20.0, "row_left={row_left} panel={panel:?} row={row:?}");
    assert!(right_gap.abs() < 0.5, "right_gap={right_gap} panel={panel:?} row={row:?}");
    assert_eq!(panel.w, 80.0, "panel={panel:?}");
}
