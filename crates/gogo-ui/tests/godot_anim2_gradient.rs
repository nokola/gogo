//! Godot gogo suite → Rust, batch `anim2_gradient`. Ports these groups of
//! `FantasiaGodot/addons/gogo/test/run_tests.gd`:
//! - `_structured_anim_tests` 7786-7987 (35 checks): `each` / `group` /
//!   `after=` choreography, the header envelope, per-line autoreverse,
//!   the anim error battery.
//! - `_step_anim_tests` 7987-8190 (32): `and` / `then` chains, `delay=`
//!   pauses, `offset=` shifts, `play` inlining, scope lookup, refusals.
//! - `_snake_case_tests` 8190-8210 (5): kebab spellings die at the lexer.
//! - `_lazy_image_tests` 8210-8267 (11): image presence at build time.
//! - `_gradient_tests` 8267-8455 (58): stops, angle, fill, the rules.
//! - `_circle_tests` 8455-8490 (11): disc, ring, when, box-word refusals.
//!
//! Dialect: Godot lets a fixture start at any node (`Rectangle box`,
//! `Control host { … }`); Rust wants a `View`/`Panel` root (G7003), so
//! every fixture here is wrapped in `View v { … }` / `View host { … }`.
//! Godot's `close(name, expected, actual)` calls (four in the anim group)
//! are ported next to the `check` they sit with; they are not counted.
//!
//! ENGINE (not ported, Godot machinery with no Rust counterpart):
//! - lazy.fixture-saved 8215: ResourceSaver writes a .tres fixture.
//! - lazy.delivered / lazy.pending-drained 8238-8239 and
//!   lazy.dup-delivered / lazy.dup-pending-drained 8255-8256: worker-thread
//!   texture decode and its pending counter.
//! - gradient.radial-line 8276: radial center/edge points are the wgpu
//!   shader's, not the draw list's.
//! - gradient.instances-independent 8300: GradientTexture2D object identity.
//! - gradient.conic-cpu 8361: ImageTexture fallback for engines without
//!   FILL_CONIC.
use std::path::PathBuf;

use gogo::anim::Finish;
use gogo::draw::{Draw, GradientKind, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{PropKey, Color_Straight_Srgb_4xU8_RGBA, Value};
use gogo::runtime::{HOVERED, Runtime, resolve};
use gogo::{BuildOptions, Compiled, Diagnostic, Page, build_pages, compile, has_errors};

// ------------------------------------------------------------------ helpers

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!has_errors(&c.diags), "{src}\n{:#?}", c.diags);
    c
}

fn diags(src: &str) -> Vec<Diagnostic> {
    compile(src, &|_| None).diags
}

fn codes(src: &str) -> Vec<String> {
    diags(src).iter().map(|d| d.code.clone()).collect()
}

fn fails(src: &str) -> bool {
    has_errors(&diags(src))
}

/// Any diagnostic (error or warning) whose message contains `needle`.
fn mentions(src: &str, needle: &str) -> bool {
    diags(src).iter().any(|d| d.message.contains(needle))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// One compiled anim track, readable: (node, channel, from, to, start,
/// duration, autoreverse).
#[derive(Clone, Debug, PartialEq)]
struct Lane {
    node: String,
    channel: String,
    from: Option<f32>,
    to: f32,
    start: f32,
    duration: f32,
    autoreverse: bool,
}

fn lanes(page: &Page, anim: &str) -> Vec<Lane> {
    let id = page.intern_lookup(anim);
    page.anims
        .iter()
        .filter(|t| t.anim == id)
        .map(|t| Lane {
            node: page.str(page.nodes[t.node.index()].name).to_string(),
            channel: page.str(t.channel).to_string(),
            from: t.from.and_then(gogo::page::Value::as_num),
            to: t.to.as_num().unwrap_or(f32::NAN),
            start: t.start,
            duration: t.duration,
            autoreverse: t.autoreverse,
        })
        .collect()
}

fn lane<'a>(all: &'a [Lane], node: &str, channel: &str) -> &'a Lane {
    all.iter().find(|l| l.node == node && l.channel == channel).unwrap_or_else(|| panic!("no lane {node}.{channel} in {all:#?}"))
}

/// Compile, resolve at the start state, lay out in a 200×200 frame, draw.
fn scene(src: &str) -> (Page, Vec<Rect>, Vec<Draw>) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let draws = draw(&c.page, &res, &rects);
    (c.page, rects, draws)
}

/// Every gradient segment in the list: (kind, from, to, angle, band).
fn gradients(draws: &[Draw]) -> Vec<(GradientKind, Color_Straight_Srgb_4xU8_RGBA, Color_Straight_Srgb_4xU8_RGBA, f32, (f32, f32))> {
    draws
        .iter()
        .filter_map(|d| match d {
            Draw::Gradient { kind, from, to, angle, band, .. } => Some((*kind, *from, *to, *angle, *band)),
            _ => None,
        })
        .collect()
}

fn circles(draws: &[Draw]) -> Vec<(Rect, Color_Straight_Srgb_4xU8_RGBA, f32, Color_Straight_Srgb_4xU8_RGBA)> {
    draws
        .iter()
        .filter_map(|d| match d {
            Draw::Circle { rect, color, border, border_color } => Some((*rect, *color, *border, *border_color)),
            _ => None,
        })
        .collect()
}

fn rect_of(page: &Page, rects: &[Rect], name: &str) -> Rect {
    rects[(page.find(name).unwrap()).index()]
}

// ------------------------------------------------- _structured_anim_tests

/// `_structured_anim_tests` 7802-7842 (each.build, each.def-shape,
/// each.after-resolved, each.pinned-from, each.spawned, each.b-held-from-y,
/// each.b-held-from-a, each.a-sliding, each.a-pop-up, each.done,
/// each.identity-disarmed): an `each .Cell stagger_by=0.1` drives every
/// wearer of the style with the same lanes, offset by the stagger; a
/// wearer still waiting holds its pinned `from` (no flash); `group pop
/// after=slide` starts when slide ends; when the chain is over every
/// channel is back to identity. Godot measured with epx×2, so its 80 is
/// this file's 40.
#[test]
fn each_staggers_its_wearers_and_disarms_at_the_end() {
    let src = "style Cell min_width=6, min_height=6\nanim fancy {\n    each .Cell stagger_by=0.1 {\n        group slide duration_each=0.2 {\n            translate.y from=40, to=0\n            and opacity from=0, to=1\n        }\n        group pop after=slide { scale.x to=1.1, duration=0.1, autoreverse=true }\n    }\n}\nView host { Rectangle a .Cell; Rectangle b .Cell }\n";
    let c = ok(src);
    let mut page = c.page;
    let all = lanes(&page, "fancy");
    // def-shape: three lanes per wearer, two wearers.
    assert_eq!(all.len(), 6, "{all:#?}");
    // after-resolved: pop starts when slide ends, with autoreverse.
    let pop = lane(&all, "a", "scale.x");
    assert!(near(pop.start, 0.2) && pop.autoreverse, "{pop:?}");
    // pinned-from: slide's translate has an authored from, pop has none.
    assert_eq!(lane(&all, "a", "translate.y").from, Some(40.0));
    assert_eq!(pop.from, None);
    let a = (page.find("a").unwrap()).index();
    let b = (page.find("b").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let fancy = page.intern_lookup("fancy");
    rt.play_id(&page, fancy, Finish::Nothing);
    rt.tick(&mut page, 0.05);
    // b-held-from-y / b-held-from-a: b's pinned from-values hold before its stagger.
    assert!(near(rt.motion[b].translate[1], 40.0), "{:?}", rt.motion[b]);
    assert!(near(rt.motion[b].opacity, 0.0), "{:?}", rt.motion[b]);
    // a-sliding.
    assert!(rt.motion[a].translate[1] < 40.0 && rt.motion[a].opacity > 0.0, "{:?}", rt.motion[a]);
    rt.tick(&mut page, 0.2);
    // a-pop-up: t=0.25, a's pop is on the way out.
    assert!(rt.motion[a].scale[0] > 1.0, "{:?}", rt.motion[a]);
    rt.tick(&mut page, 0.3);
    // done + identity-disarmed.
    assert!(!rt.is_playing_id(fancy));
    assert!(near(rt.motion[a].scale[0], 1.0) && near(rt.motion[b].translate[1], 0.0) && near(rt.motion[a].opacity, 1.0) && near(rt.motion[b].opacity, 1.0), "a {:?} b {:?}", rt.motion[a], rt.motion[b]);
}

/// `_structured_anim_tests` 7852-7861 (each.flat-autoreverse-build,
/// each.flat-autoreverse-mid, each.flat-autoreverse-back,
/// each.flat-autoreverse-done): D136 per-line `autoreverse=true` goes out
/// and back on one property line — half way at the end of `duration`,
/// home again after twice that, then the player is done.
#[test]
fn per_line_autoreverse_goes_out_and_back() {
    let c = ok("anim pulse { box.opacity to=0.5, duration=0.1, autoreverse=true }\nView host { Rectangle box }\n");
    let mut page = c.page;
    let b = (page.find("box").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let pulse = page.intern_lookup("pulse");
    rt.play_id(&page, pulse, Finish::Nothing);
    rt.tick(&mut page, 0.1);
    assert!(near(rt.motion[b].opacity, 0.5), "mid: {}", rt.motion[b].opacity);
    rt.tick(&mut page, 0.11);
    assert!(near(rt.motion[b].opacity, 1.0), "back: {}", rt.motion[b].opacity);
    assert!(!rt.is_playing_id(pulse));
}

/// `_structured_anim_tests` 7867-7874 + 7954 (each.err-bare-in-anim,
/// each.err-stagger-on-anim, group.err-missing-target): a bare property
/// path outside an `each` has no target (G0063), and `stagger_by=` on the
/// anim header is refused with the header's real vocabulary in hand.
#[test]
fn bare_paths_and_stagger_on_the_header_are_refused() {
    assert!(codes("anim a { group { opacity to=1, duration=0.1 } }\nView v { Rectangle box }\n").contains(&"G0063".to_string()));
    assert!(codes("anim a { translate.y to=0, duration=0.1 }\nView v { Rectangle box }\n").contains(&"G0063".to_string()));
    let stagger = "anim a stagger_by=0.1 { each box { opacity to=1, duration=0.1 } }\nView v { Rectangle box }\n";
    assert!(codes(stagger).contains(&"G0066".to_string()), "{:#?}", diags(stagger));
    assert!(mentions(stagger, "repeat=forever"), "{:#?}", diags(stagger));
}

/// `_structured_anim_tests` 7869-7872, 7894 (each.err-after-unknown,
/// each.err-after-cycle, each.err-unknown-style): `after=` naming no
/// group, two groups waiting on each other, and `each .Nope` are build
/// errors that say why.
#[test]
fn after_and_each_references_are_checked() {
    assert!(fails("anim a { each box {\n    group t { opacity to=1, duration=0.1 }\n    group after=nope { rotate to=1, duration=0.1 }\n} }\nView v { Rectangle box }\n"), "after=nope names no group");
    assert!(mentions("anim a {\n    group t after=u { box.opacity to=1, duration=0.1 }\n    group u after=t { box.rotate to=1, duration=0.1 }\n}\nView v { Rectangle box }\n", "cycle"));
    assert!(fails("anim a { each .Nope { opacity to=1, duration=0.1 } }\nView v { Rectangle box }\n"), "unknown style");
}

/// `_structured_anim_tests` 7877-7890 + 7952 (each.header-envelope-ok,
/// each.header-envelope-distributes, group.err-plain-duration-anim): the
/// header envelope — `duration_each=` hands every line its length, a
/// line's own `duration=` wins, `autoreverse=` distributes; a plain
/// `duration=` on the header is refused and names `duration_each`.
#[test]
fn header_envelope_distributes_per_line() {
    let c = ok("anim h duration_each=0.2, easing=cubic_out, autoreverse=true {\n    box.opacity to=1\n    and box.rotate to=10, duration=0.5\n}\nView v { Rectangle box }\n");
    let all = lanes(&c.page, "h");
    let op = lane(&all, "box", "opacity");
    let rot = lane(&all, "box", "rotate");
    assert!(near(op.duration, 0.2) && op.autoreverse, "{op:?}");
    assert!(near(rot.duration, 0.5) && rot.autoreverse, "{rot:?}");
    let plain = "anim a duration=1 { box.opacity to=1 }\nView v { Rectangle box }\n";
    assert!(codes(plain).contains(&"G0066".to_string()) && mentions(plain, "duration_each"), "{:#?}", diags(plain));
}

/// `_structured_anim_tests` 7892 (each.err-double-pin) + 7948-7951
/// (group.err-delay-attr, group.err-plain-duration): two lines that both
/// pin `from=` on one channel would fight from frame one; `delay=` on a
/// group is a retired word; a group's `duration=` reads as a total and
/// must be `duration_each=`. Godot refuses all three at load.
#[test]
fn double_pin_and_group_line_words_are_refused() {
    assert!(fails("anim a {\n    box.opacity from=0, to=1, duration=0.1\n    and box.opacity from=0.5, to=1, duration=0.1\n}\nView v { Rectangle box }\n"), "two pinned steps on box.opacity");
    assert!(fails("anim a { group delay=0.1 { box.opacity to=1, duration=0.1 } }\nView v { Rectangle box }\n"), "delay= on a group is retired");
    assert!(fails("anim a { group duration=1 { box.opacity to=1 } }\nView v { Rectangle box }\n"), "duration= on a group must be duration_each=");
}

/// `_structured_anim_tests` 7899-7926 (group.anim-level-ok,
/// group.anim-level-buckets, group.anim-level-starts,
/// group.anim-level-line-duration): groups directly under the anim carry
/// dotted targets, one group drives several nodes, `after=dim` starts the
/// drop group when dim ends, and a line's own `duration=` beats the
/// group's `duration_each=`.
#[test]
fn anim_level_groups_with_after_order_cross_node_choreography() {
    let c = ok("anim open {\n    group dim duration_each=0.2 { backA.opacity to=0.6 }\n    group drop duration_each=0.3, after=dim {\n        menuA.translate.y from=-40, to=0\n        and menuA.opacity from=0, to=1, duration=0.5\n    }\n}\nView v { Column c {\n    Rectangle backA\n    Rectangle menuA\n} }\n");
    let all = lanes(&c.page, "open");
    assert_eq!(all.len(), 3, "{all:#?}");
    assert!(near(lane(&all, "backA", "opacity").start, 0.0));
    assert!(near(lane(&all, "menuA", "translate.y").start, 0.2), "{all:#?}");
    assert!(near(lane(&all, "menuA", "opacity").start, 0.2), "{all:#?}");
    assert!(near(lane(&all, "menuA", "opacity").duration, 0.5), "{all:#?}");
}

/// `_structured_anim_tests` 7928-7930 (group.anim-level-self): at page
/// level `self.` names the file's own root, so `anim pop { group squash
/// duration_each=0.8 { self.scale to=5 } }` next to `Button b` builds
/// (GPAR86, round 6; inside a widget `self` stays the widget's root).
#[test]
fn page_level_self_names_the_root() {
    let src = "anim pop { group squash duration_each=0.8 { self.scale to=5 } }\nView v { Button b }\n";
    assert!(!fails(src), "{:#?}", diags(src));
    let all = lanes(&compile(src, &|_| None).page, "pop");
    assert!(!all.is_empty() && all.iter().all(|l| l.node == "v" && l.channel == "scale"), "self drives the ROOT's scale: {all:#?}");
}

/// `_structured_anim_tests` 7933-7942 (group.mixed-forms-ok): D136's one
/// grammar — a lane, an `each`, and another lane mix freely in one chain,
/// and the `each` really drives its wearer (a track for `c` exists).
#[test]
fn lanes_eaches_and_groups_mix_in_one_chain() {
    let c = ok("anim a {\n    box.opacity to=1, duration=0.1\n    and each c { opacity to=1, duration=0.1 }\n    then box.rotate to=90, duration=0.1\n}\nView host { Rectangle box; Rectangle c }\n");
    let all = lanes(&c.page, "a");
    assert!(all.iter().any(|l| l.node == "c" && l.channel == "opacity"), "{all:#?}");
    assert!(near(lane(&all, "box", "rotate").start, 0.1), "{all:#?}");
}

/// `_structured_anim_tests` 7944 (group.err-duplicate) + 7956-7959
/// (group.err-each-prefixed, group.err-each-self-prefixed): group names
/// are unique within an anim (they are `after=` anchors); a line inside
/// `each box` / `each self` must not repeat the target.
#[test]
fn group_names_are_unique_and_each_lines_stay_bare() {
    assert!(fails("anim a {\n    group t duration_each=0.1 { box.opacity to=1 }\n    and group t duration_each=0.1 { box.rotate to=1 }\n}\nView v { Rectangle box }\n"), "duplicate group name");
    assert!(fails("anim a { each box { group t duration_each=0.1 { box.opacity to=1 } } }\nView v { Rectangle box }\n"), "each already names the target");
    assert!(fails("anim a { each self { group t duration_each=0.1 { self.scale to=1 } } }\nView v { Rectangle box }\n"), "each self already names the target");
}

/// `_structured_anim_tests` 7946 (group.err-assign): an anim body holds
/// journeys (`to=`), never assignments — `box.opacity = 0` inside a group
/// is a load error in Godot — and here too (G0063, GPAR86 round 6; it
/// used to be dropped without a word).
#[test]
fn assignments_inside_an_anim_are_refused() {
    let src = "anim a { group duration_each=0.1 { box.opacity = 0 } }\nView v { Rectangle box }\n";
    assert!(fails(src), "{:#?}", diags(src));
}

/// `_structured_anim_tests` 7960-7980 (group.err-two-writers,
/// group.err-two-writers-names-holder): a property held by a `when held`
/// and driven by the transition's `anim=` has two writers — refused, and
/// the error names whose `when` holds it even when the anim sits on the
/// style and the `when` on the node.
#[test]
fn two_writers_on_one_property_are_refused_naming_the_holder() {
    assert!(mentions("style S {\n    when held scale=0.9\n    transition any -> held anim=pop\n}\nanim pop {\n    group t duration_each=0.1 { self.scale to=1.1 }\n}\nView v { Button b .S }\n", "two writers"));
    assert!(mentions("style S {\n    transition any -> held anim=pop\n}\nanim pop { self.scale to=1.1, duration=0.1 }\nView v { Button menuBtn .S {\n    when held scale=0.9\n} }\n", "'scale' on menuBtn is held by its `when held`"));
}

// ------------------------------------------------------- _step_anim_tests

/// `_step_anim_tests` 8005-8007 (step.first-at-zero, step.and-shares-start,
/// step.then-waits-cluster): the first step starts at 0, `and` shares its
/// start, `then` waits for the whole and-cluster (0.6, the longer of the
/// two). The Godot fixture's `group d after=b` line is asserted in
/// `crossing_edge_with_after`.
#[test]
fn and_shares_a_start_and_then_waits_the_cluster() {
    let c = ok("anim dash {\n    group a { tiles.opacity to=1, duration=0.6 }\n    and group b { hdr.translate.y from=-20, to=0, duration=0.2 }\n    then chart.scale to=1.1, duration=0.2\n}\nView host { Rectangle tiles; Rectangle hdr; Rectangle chart }\n");
    let all = lanes(&c.page, "dash");
    assert!(near(lane(&all, "tiles", "opacity").start, 0.0), "{all:#?}");
    assert!(near(lane(&all, "hdr", "translate.y").start, 0.0), "{all:#?}");
    assert!(near(lane(&all, "chart", "scale").start, 0.6), "{all:#?}");
}

/// `_step_anim_tests` 7990-8008 (step.crossing-ok, step.after-crosses):
/// the crossing-edge shape — `group d after=b` anchors on ONE member of
/// the and-cluster (b ends at 0.2) and overlaps the join.
#[test]
fn crossing_edge_with_after() {
    let c = ok("anim dash {\n    group a { tiles.opacity to=1, duration=0.6 }\n    and group b { hdr.translate.y from=-20, to=0, duration=0.2 }\n    then chart.scale to=1.1, duration=0.2\n    group d after=b { hdr.opacity to=0.5, duration=0.3 }\n}\nView host { Rectangle tiles; Rectangle hdr; Rectangle chart }\n");
    let all = lanes(&c.page, "dash");
    assert!(near(lane(&all, "hdr", "opacity").start, 0.2), "{all:#?}");
}

/// `_step_anim_tests` 8012-8030 (step.pause-ok, step.pause-shifts): the
/// toast shape — a `then delay=3` pause is a step of its own; the way out
/// starts 3 s after the longer of the two entrance lines (0.3 + 3 = 3.3).
#[test]
fn a_pause_is_a_step_and_shifts_what_follows() {
    let c = ok("anim toast {\n    group easing=cubic_out {\n        plate.translate.y from=60, to=0, duration=0.25\n        and plate.opacity from=0, to=1, duration=0.3\n        then delay=3\n        then plate.translate.y to=60, duration=0.2\n        and plate.opacity to=0, duration=0.2\n    }\n}\nView host { Rectangle plate }\n");
    let all = lanes(&c.page, "toast");
    assert_eq!(all.len(), 4, "{all:#?}");
    assert!(near(all[2].start, 3.3) && near(all[3].start, 3.3), "{all:#?}");
}

/// `_step_anim_tests` 8033-8053 (step.offset-ok, step.offset-overlaps,
/// step.offset-clamps): `offset=` shifts a placed start (negative =
/// overlap: 0.3 − 0.05 = 0.25); the front wall clamps at 0.
#[test]
fn offset_shifts_a_start_and_clamps_at_zero() {
    let c = ok("anim ov {\n    box.opacity to=1, duration=0.3\n    then box.rotate to=90, duration=0.1, offset=-0.05\n}\nView v { Rectangle box }\n");
    let all = lanes(&c.page, "ov");
    assert!(near(lane(&all, "box", "rotate").start, 0.25), "{all:#?}");
    let c = ok("anim c { box.opacity to=1, duration=0.1, offset=-1 }\nView v { Rectangle box }\n");
    let all = lanes(&c.page, "c");
    assert!(near(all[0].start, 0.0), "{all:#?}");
}

/// `_step_anim_tests` 8057-8079 (step.play-ok, step.play-inlined,
/// step.then-waits-play): `and play badge` inlines the played anim at the
/// step's start; `then` waits its full length — autoreverse's way back
/// included (0.15 + 2×0.3 = 0.75).
#[test]
fn play_inlines_and_then_waits_its_full_length() {
    let c = ok("anim badge {\n    badge.translate.y from=-10, to=0, duration=0.15\n    then badge.scale to=1.2, duration=0.3, autoreverse=true\n}\nanim open {\n    panel.opacity from=0, to=1, duration=0.3\n    and play badge\n    then panel.scale to=1.0, duration=0.1\n}\nView host { Rectangle panel; Rectangle badge }\n");
    let all = lanes(&c.page, "open");
    assert!(near(lane(&all, "badge", "scale").start, 0.15), "{all:#?}");
    assert!(near(lane(&all, "panel", "scale").start, 0.75), "{all:#?}");
}

/// `_step_anim_tests` 8083-8107 (step.scope-nested,
/// step.autoreverse-doubles): a step word comes from the NEAREST enclosing
/// scope that sets it (a `duration_each=1` two groups up still reaches the
/// line); a group's `autoreverse=` distributes per line and `then` waits
/// the way back (2 × 0.1).
#[test]
fn step_words_resolve_from_the_nearest_scope_and_autoreverse_doubles() {
    let c = ok("anim s {\n    group duration_each=1 {\n        group easing=cubic_out { box.opacity to=1 }\n    }\n}\nView v { Rectangle box }\n");
    let all = lanes(&c.page, "s");
    assert!(near(all[0].duration, 1.0), "{all:#?}");
    let c = ok("anim v {\n    group duration_each=0.1, autoreverse=true { box.opacity to=1 }\n    then box.rotate to=10, duration=0.1\n}\nView v { Rectangle box }\n");
    let all = lanes(&c.page, "v");
    assert!(near(lane(&all, "box", "rotate").start, 0.2), "{all:#?}");
}

/// `_step_anim_tests` 8112-8129 (step.forward-ref-ok,
/// step.forward-ref-start): `after=` may name a group later in source and
/// reach out of its wrapper (names are anim-flat) — c starts when `late`
/// ends (0.3).
#[test]
fn after_may_reference_a_later_group() {
    let c = ok("anim f {\n    group a {\n        box.opacity to=1, duration=0.2\n        group c after=late { box.scale to=1.1, duration=0.1 }\n    }\n    and group late { box.rotate to=90, duration=0.3 }\n}\nView v { Rectangle box }\n");
    let all = lanes(&c.page, "f");
    assert!(near(lane(&all, "box", "scale").start, 0.3), "{all:#?}");
}

/// `_step_anim_tests` 8132-8146 (step.each-static-then-ok,
/// step.each-static-then-start): an `each` with no stagger has a static
/// length, so a `then` after it is fine and starts when it ends (0.1).
#[test]
fn a_static_each_can_be_waited_on() {
    let c = ok("anim a {\n    each c { opacity to=1, duration=0.1 }\n    then box.opacity to=1, duration=0.2\n}\nView host { Rectangle box; Rectangle c }\n");
    let all = lanes(&c.page, "a");
    assert!(all.iter().any(|l| l.node == "c"), "the each drives c: {all:#?}");
    assert!(near(lane(&all, "box", "opacity").start, 0.1), "{all:#?}");
}

/// `_step_anim_tests` 8149-8156 (step.err-no-connective,
/// step.err-first-connective, step.err-connective-outside): the connective
/// law — every step after the first says `and`/`then` (G0061), the first
/// stands alone (G0061), and a `then` outside an anim is no node (Rust:
/// G7011 unknown node type; Godot says it "sequences anim steps").
#[test]
fn the_connective_law_and_its_edges() {
    assert!(codes("anim a {\n    box.opacity to=1, duration=0.1\n    box.rotate to=1, duration=0.1\n}\nView v { Rectangle box }\n").contains(&"G0061".to_string()));
    let first = "anim a { then box.opacity to=1, duration=0.1 }\nView v { Rectangle box }\n";
    assert!(codes(first).contains(&"G0061".to_string()) && mentions(first, "stands alone"), "{:#?}", diags(first));
    assert!(codes("View { then Button b text=\"x\" }").contains(&"G7011".to_string()));
}

/// `_step_anim_tests` 8153-8158 (step.err-both-placements,
/// step.err-after-on-line): a step is positional (`then`) or referenced
/// (`after=`), never both; `after=` goes on `group` lines only.
#[test]
fn after_is_exclusive_with_then_and_lives_on_groups() {
    assert!(mentions("anim a {\n    group b { box.opacity to=1, duration=0.1 }\n    then group c after=b { box.rotate to=1, duration=0.1 }\n}\nView v { Rectangle box }\n", "positional"));
    assert!(mentions("anim a {\n    group b { box.opacity to=1, duration=0.1 }\n    box.rotate to=1, duration=0.1, after=b\n}\nView v { Rectangle box }\n", "group"));
}

/// `_step_anim_tests` 8160-8167 + 8182 (step.err-wait-on-stagger,
/// step.err-each-nested): nothing waits on a play-time length — a `then`
/// after a staggered each is refused naming `stagger_by`; eaches don't
/// nest.
#[test]
fn staggered_each_cannot_be_waited_on_and_eaches_do_not_nest() {
    assert!(mentions("style M min_width=4\nanim a {\n    each .M stagger_by=0.1 { opacity to=1, duration=0.1 }\n    then box.opacity to=1, duration=0.1\n}\nView v { Rectangle box .M }\n", "stagger_by"));
    assert!(fails("anim a { each box { each box { opacity to=1, duration=0.1 } } }\nView v { Rectangle box }\n"));
}

/// `_step_anim_tests` 8169-8176 (step.err-play-unknown,
/// step.err-play-forever, step.err-play-cycle, step.err-play-in-each): the
/// play edges — an unknown anim, a `repeat=forever` anim (never completes,
/// so it can't be a step), a play cycle, and `play` inside an `each`.
#[test]
fn play_edges_are_refused_by_name() {
    assert!(mentions("anim a { play nope }\nView v { Rectangle box }\n", "unknown anim"));
    assert!(mentions("anim spin repeat=forever { box.opacity { key t=0, v=0; key t=1, v=1 } }\nanim a { play spin }\nView v { Rectangle box }\n", "never completes"));
    assert!(mentions("anim a { play b }\nanim b { play a }\nView v { Rectangle box }\n", "cycle"));
    assert!(mentions("anim b { box.opacity to=1, duration=0.1 }\nanim a { each box { play b } }\nView v { Rectangle box }\n", "doesn't go inside an each"));
}

/// `_step_anim_tests` 8178-8181 (step.err-delay-negative,
/// step.err-group-empty): a pause can't be negative; a group with no
/// steps is refused.
#[test]
fn negative_delay_and_empty_group_are_refused() {
    assert!(fails("anim a {\n    box.opacity to=1, duration=0.1\n    then delay=-1\n}\nView v { Rectangle box }\n"), "a pause can't be negative");
    assert!(fails("anim a { group { } }\nView v { Rectangle box }\n"), "group has no steps");
}

// ------------------------------------------------------ _snake_case_tests

/// `_snake_case_tests` 8191-8203 (snake.props-resolve,
/// snake.kebab-refuses-at-lexer, snake.kebab-value-refuses,
/// snake.arrow-still-lexes, snake.negative-number-still-lexes): D137 —
/// identifiers are snake_case; `-` left the identifier alphabet, so a
/// kebab key or value dies at the lexer with the plain
/// unexpected-character error (G0001), while `->` and negative number
/// literals still lex.
#[test]
fn kebab_dies_at_the_lexer_but_arrows_and_negatives_lex() {
    ok("Panel p min_width=44, corner_radius=6, padding_left=8\n");
    let kebab = "View v { Button b min-width=44 }";
    assert!(codes(kebab).contains(&"G0001".to_string()) && mentions(kebab, "unexpected character `-`"), "{:#?}", diags(kebab));
    let value = "View v { Label a font_color=text-muted }";
    assert!(codes(value).contains(&"G0001".to_string()) && mentions(value, "unexpected character `-`"), "{:#?}", diags(value));
    ok("style S {\n    transition any -> held duration=0.1\n    when held background=white/0.13\n}\nView v { Button b .S text=\"x\" }\n");
    ok("View v { Label a margin_top=-4 }\n");
}

// ------------------------------------------------------ _lazy_image_tests

/// A scratch folder under the OS temp dir for one build_pages run.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gogo_godot_anim2_gradient_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    dir
}

/// `_lazy_image_tests` 8218-8219 (lazy.eager-default): the image is there
/// the moment the build returns — in Rust the build embeds the named file's
/// bytes (the IMAGES table names it) and the page draws it. 8263
/// (lazy.missing-file): a path that names no file fails the build.
#[test]
fn images_are_embedded_at_build_and_a_missing_file_fails_it() {
    let dir = scratch("eager");
    std::fs::write(dir.join("gogo_lazy_test.png"), b"png bytes stand-in").unwrap();
    std::fs::write(dir.join("ui").join("page.gogo"), "View v {\n Image a source=\"res://gogo_lazy_test.png\"\n}\n").unwrap();
    let out = dir.join("pages.rs");
    build_pages(&dir.join("ui"), &out, BuildOptions::new(&dir)).unwrap();
    let generated = std::fs::read_to_string(&out).unwrap();
    assert!(generated.contains("(\"res://gogo_lazy_test.png\", Some(include_bytes!"), "{generated}");
    let c = ok("View v { Image a source=\"res://gogo_lazy_test.png\" }");
    let key = c.images.get("res://gogo_lazy_test.png").expect("a source= is interned");
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let draws = draw(&c.page, &res, &rects);
    assert!(draws.iter().any(|d| matches!(d, Draw::Image { image, .. } if *image == key)), "the draw carries the build-wide image id: {draws:#?}");
    let _ = std::fs::remove_dir_all(&dir);

    let dir = scratch("missing");
    std::fs::write(dir.join("ui").join("page.gogo"), "View v {\n Image a source=\"res://gogo_lazy_missing.png\", loading=lazy\n}\n").unwrap();
    let err = build_pages(&dir.join("ui"), &dir.join("pages.rs"), BuildOptions::new(&dir)).unwrap_err();
    assert!(err.contains("not found"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `_lazy_image_tests` 8226-8231 (lazy.build-ok, both key orders) and
/// 8244-8246 (lazy.dup-build-ok): `loading=lazy` builds whichever side of
/// `source=` it sits on, and two Images naming one path share it (one
/// `ImageKey` in the image table — the cache-hit half of Godot's dup
/// check); the path is an image id, never a page string.
#[test]
fn loading_lazy_builds_in_any_key_order_and_shared_paths_intern_once() {
    ok("View v { Image a source=\"res://gogo_lazy_test.png\", loading=lazy }");
    ok("View v { Image a loading=lazy, source=\"res://gogo_lazy_test.png\" }");
    let c = ok("View v { Column c { Image a source=\"res://gogo_lazy_test.png\", loading=lazy; Image b source=\"res://gogo_lazy_test.png\", loading=lazy } }");
    let hits = c.images.names.iter().filter(|s| s.as_str() == "res://gogo_lazy_test.png").count();
    assert_eq!(hits, 1, "{:?}", c.images.names);
    assert!(!c.page.strings.iter().any(|s| s == "res://gogo_lazy_test.png"), "an image path is an ImageKey, not a page string: {:?}", c.page.strings);
}

/// `_lazy_image_tests` 8260 (lazy.bad-value): `loading=` takes `lazy` or
/// `eager` — `fast` is refused.
#[test]
fn loading_takes_lazy_or_eager_only() {
    let src = "View v { Image a source=\"res://gogo_lazy_test.png\", loading=fast }";
    assert!(fails(src), "{:#?}", diags(src));
}

/// `_lazy_image_tests` 8262 (lazy.wrong-node): `loading=` belongs on an
/// Image — on a Label it is refused.
#[test]
fn loading_belongs_on_an_image() {
    let src = "View v { Label a loading=lazy }";
    assert!(fails(src), "{:#?}", diags(src));
}

// -------------------------------------------------------- _gradient_tests

/// `_gradient_tests` 8270-8285 (gradient.ok, gradient.texture,
/// gradient.stops, gradient.stretch, gradient.size,
/// gradient.modulate-untouched): the D83 Gradient — `fill=radial` reaches
/// the draw list as a radial kind; the three stops become two segments
/// split at 0.94 carrying the written colors (first #FFFFFF6B, last alpha
/// 0); the gradient covers the node's whole rect; the box is its written
/// 35×35; nothing tints the colors on the way.
#[test]
fn radial_gradient_with_a_plateau_reaches_the_draw_list() {
    let (page, rects, draws) = scene("View v { Gradient glow 35x35, fill=radial, stops=[#FFFFFF6B@0, #FFFFFF6B@0.94, #FFFFFF00@1] }");
    let glow = rect_of(&page, &rects, "glow");
    assert_eq!((glow.w, glow.h), (35.0, 35.0), "{glow:?}");
    let g = gradients(&draws);
    assert_eq!(g.len(), 2, "{g:#?}");
    assert!(g.iter().all(|s| s.0 == GradientKind::Radial), "{g:#?}");
    assert!(near(g[0].4.1, 0.94) && near(g[1].4.0, 0.94), "{g:#?}");
    assert_eq!(g[0].1, Color_Straight_Srgb_4xU8_RGBA([0xFF, 0xFF, 0xFF, 0x6B]), "{g:#?}");
    assert_eq!(g[1].2.0[3], 0, "{g:#?}");
    let covers = draws.iter().filter(|d| matches!(d, Draw::Gradient { rect, .. } if *rect == glow)).count();
    assert_eq!(covers, 2, "every segment spans the node's rect: {draws:#?}");
}

/// `_gradient_tests` 8289-8299 (gradient.even-ok, gradient.even-two,
/// gradient.even-three, gradient.default-linear): no `@` on any entry =
/// even spacing (two stops: one band 0..1; three: split at 0.5); the
/// default fill is linear, left → right (compass 90, Godot's fill_from
/// (0,0) → fill_to (1,0)).
#[test]
fn unpinned_stops_spread_evenly_and_default_to_linear_left_to_right() {
    let (_, _, draws) = scene("View v { Column c {\n    Gradient a width=100, height=10, stops=[white, black]\n    Gradient b width=100, height=10, stops=[red, yellow, green]\n} }");
    let g = gradients(&draws);
    let bands: Vec<(f32, f32)> = g.iter().map(|s| s.4).collect();
    assert_eq!(bands, vec![(0.0, 1.0), (0.0, 0.5), (0.5, 1.0)], "{g:#?}");
    assert!(g.iter().all(|s| s.0 == GradientKind::Linear && s.3 == 90.0), "{g:#?}");
}

/// `_gradient_tests` 8305-8312 (gradient.entry-kinds, gradient.ref-entry):
/// a `$ref` may carry a stop position (`$edge@0`) and resolves like any
/// color position — the first stop is #10203040 (GPAR86, round 6: the
/// lexer reads `@pos` after a ref and the stop bakes at compile).
#[test]
fn a_ref_stop_carries_its_position() {
    let src = "$edge = #10203040\nView v { Gradient g width=100, height=10, stops=[$edge@0, accent/0.5@0.3, white@1] }";
    assert!(!fails(src), "{:#?}", diags(src));
    let (_, _, draws) = scene(src);
    let g = gradients(&draws);
    assert_eq!(g[0].1, Color_Straight_Srgb_4xU8_RGBA([0x10, 0x20, 0x30, 0x40]), "{g:#?}");
}

/// `_gradient_tests` 8311 (gradient.opacity-entry): `/opacity` goes on a
/// stops entry — `accent/0.5@0.3` lands at alpha 0.5. (The Godot fixture's
/// first entry is `$edge@0`; here it is the same hex written directly,
/// because the `$ref@pos` form is the NEW deviation above.)
#[test]
fn a_stop_entry_takes_an_opacity_suffix() {
    let (_, _, draws) = scene("View v { Gradient g width=100, height=10, stops=[#10203040@0, accent/0.5@0.3, white@1] }");
    let g = gradients(&draws);
    assert_eq!(g.len(), 2, "{g:#?}");
    assert_eq!(g[0].1, Color_Straight_Srgb_4xU8_RGBA([0x10, 0x20, 0x30, 0x40]), "{g:#?}");
    assert!((g[0].2.0[3] as i32 - 128).abs() <= 1, "accent/0.5 alpha: {g:#?}");
    assert!(near(g[0].4.1, 0.3), "{g:#?}");
}

/// `_gradient_tests` 8316-8320 (gradient.mirror-ok, gradient.mirror-enum)
/// + 8392-8394 (gradient.err-mirror-past-fold, gradient.err-bad-repeat):
/// `repeat=mirror` is a known Gradient word (no unknown-property warning)
/// that mirrors the ramp; with mirror the positions stop at 0.5; an
/// unknown repeat word is refused naming `mirror`.
#[test]
fn repeat_mirror_is_a_gradient_word_with_its_fences() {
    let src = "View v { Gradient g repeat=mirror, stops=[white@0, black@0.5] }";
    assert!(diags(src).is_empty(), "{:#?}", diags(src));
    assert!(mentions("View v { Gradient g repeat=mirror, stops=[white@0, black@0.8] }", "0.5"));
    assert!(mentions("View v { Gradient g repeat=bounce, stops=[white, black] }", "mirror"));
}

/// `_gradient_tests` 8324-8338 (gradient.angle-ok, gradient.angle-line,
/// gradient.angle-up): `angle=` is the CSS compass — 90 = to the right
/// (fill_from (0,0.5) → (1,0.5)), 0 = to the top ((0.5,1) → (0.5,0)) — and
/// travels as written into the draw list.
#[test]
fn angle_is_the_compass() {
    let (_, _, draws) = scene("View v { Gradient g width=100, height=10, angle=90, stops=[white, black] }");
    assert_eq!(gradients(&draws)[0].3, 90.0, "{draws:#?}");
    let (_, _, draws) = scene("View v { Gradient g width=100, height=10, angle=0, stops=[white, black] }");
    assert_eq!(gradients(&draws)[0].3, 0.0, "{draws:#?}");
}

/// `_gradient_tests` 8341-8347 (gradient.fromto-ok, gradient.fromto-explicit)
/// + 8396-8397 (gradient.err-angle-vs-fromto, gradient.err-from-range):
/// `from=`/`to=` are 0..1 box points that survive a later `fill=`; `angle=`
/// next to them is refused ("spell one"); a part outside 0..1 is refused.
#[test]
fn from_and_to_are_box_points_with_their_fences() {
    let src = "View v { Gradient g from=0.25x0.25, fill=radial, to=0.75x0.75, stops=[white, black] }";
    assert!(diags(src).is_empty(), "{:#?}", diags(src));
    assert!(mentions("View v { Gradient g angle=90, from=0x0, stops=[white, black] }", "spell one"));
    assert!(mentions("View v { Gradient g from=2x0, stops=[white, black] }", "0..1"));
}

/// `_gradient_tests` 8352-8359 (gradient.conic-ok, gradient.conic-native):
/// `fill=conic` builds and draws a conic sweep, not a linear ramp.
#[test]
fn conic_fill_is_its_own_kind() {
    let (_, _, draws) = scene("View v { Gradient g width=100, height=100, fill=conic, stops=[red, yellow, red] }");
    let g = gradients(&draws);
    assert!(!g.is_empty() && g.iter().all(|s| s.0 != GradientKind::Linear && s.0 != GradientKind::Radial), "{g:#?}");
}

/// `_gradient_tests` 8365-8380 (gradient.stop-prop-get,
/// gradient.stop-prop-set): stops stay live as per-stop properties
/// (GP40 step 4, user 2026-09-02: parametric keys, the `stop_1_color`
/// spelling, 1-based declaration order) — a code write re-colors ONE
/// stop, the rest hold.
#[test]
fn stops_are_live_per_stop_properties() {
    let c = ok("View v { Gradient g width=100, height=100, stops=[white, black] }");
    let mut rt = Runtime::new(&c.page);
    let g = c.page.find("g").unwrap();
    rt.set_prop(g, PropKey::StopColor(1), Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0xE5, 0x14, 0x00, 0xFF])));
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let list = gradients(&draw(&c.page, &res, &rects));
    assert_eq!(list.first().unwrap().1, Color_Straight_Srgb_4xU8_RGBA([0xE5, 0x14, 0x00, 0xFF]), "{list:#?}");
    assert_eq!(list.last().unwrap().2, Color_Straight_Srgb_4xU8_RGBA([0x00, 0x00, 0x00, 0xFF]), "the other stop holds: {list:#?}");
}

/// The stop-write fences: the declared look has one spelling (`stops=`),
/// the words are a Gradient's, and a live write names a DECLARED stop.
#[test]
fn stop_write_fences() {
    // a static node-line write points at stops=
    assert!(fails("View v { Gradient g stops=[white, black], stop_1_color=red }"));
    // the words are a Gradient's
    assert!(fails("View v { Panel p stop_1_color=red }"));
    // a write past the declared count names the count
    let d = diags("View v { Gradient g width=100, height=100, stops=[white, black] { when screen.landscape stop_3_color=red } }");
    assert!(d.iter().any(|g| g.message.contains("declares 2 stops")), "{d:#?}");
}

/// `_gradient_tests` 8370-8380 (gradient.stop-prop-listed,
/// gradient.stop-animatable): `g.stop_1_color.a to=0.42` as an anim
/// lane — the per-stop property surface landed (GP40 step 4); the LANE
/// rides the non-motion anim engine, its own unported GP40 item.
#[test]
fn stops_are_animatable() {
    let src = "anim glow { g.stop_1_color.a to=0.42, duration=0.1 }\nView v { Gradient g stops=[white, black], on:show=play(glow) }";
    assert!(!fails(src), "{:#?}", diags(src));
    assert_eq!(lanes(&compile(src, &|_| None).page, "glow").len(), 1);
}

/// `_gradient_tests` 8383-8388 (gradient.err-needs-stops,
/// gradient.err-empty, gradient.err-not-list, gradient.err-non-color,
/// gradient.err-some-pinned, gradient.err-decreasing) + 8393, 8395
/// (gradient.err-bad-fill, gradient.err-angle-round): the stops rules —
/// `stops=` is required, never empty, a list of colors, `@` positions
/// all-or-none and never decreasing; `fill=` takes linear|radial|conic;
/// `angle=` aims LINEAR gradients only.
#[test]
fn the_stops_rules_and_the_fill_fences() {
    assert!(mentions("View v { Gradient g }", "stops="));
    assert!(mentions("View v { Gradient g stops=[] }", "empty"));
    assert!(mentions("View v { Gradient g stops=white }", "list"));
    assert!(mentions("View v { Gradient g stops=[white, 12] }", "colors"));
    assert!(mentions("View v { Gradient g stops=[white@0, black] }", "all-or-none"));
    assert!(mentions("View v { Gradient g stops=[white@0.8, black@0.2] }", "never decrease"));
    assert!(mentions("View v { Gradient g fill=diamond, stops=[white, black] }", "conic"));
    assert!(mentions("View v { Gradient g fill=radial, angle=90, stops=[white, black] }", "LINEAR"));
}

/// `_gradient_tests` 8389-8390 (gradient.equal-positions-ok) + 8409-8413
/// (gradient.transparent-hex-ok, gradient.transparent-color0-ok): two
/// stops at one position (a hard edge) build; the spelled transparent
/// forms `#00000000` and `black/0` are legal stop entries.
#[test]
fn equal_positions_and_spelled_transparent_stops_build() {
    let (_, _, draws) = scene("View v { Gradient g width=100, height=10, stops=[white@0, gray@0.3, gray@0.3, black@1] }");
    let bands: Vec<(f32, f32)> = gradients(&draws).iter().map(|s| s.4).collect();
    assert_eq!(bands, vec![(0.0, 0.3), (0.3, 1.0)], "a zero-width band paints nothing: {bands:?}");
    ok("View v { Gradient g stops=[white, #00000000] }");
    ok("View v { Gradient g stops=[white, black/0] }");
}

/// `_gradient_tests` 8398-8399 (gradient.err-at-elsewhere,
/// gradient.err-at-on-number): `@pos` only goes on a Gradient's stops
/// entries — `Rectangle r color=white@0.5` is refused naming `stops=`; a
/// position past 1 (`white@2`) does not even parse.
#[test]
fn a_stop_position_only_goes_on_stops_entries_and_stays_in_range() {
    assert!(mentions("View v { Rectangle r color=white@0.5 }", "stops="));
    assert!(fails("View v { Gradient g stops=[white@2] }"));
}

/// `_gradient_tests` 8400-8401 (gradient.err-solid-until,
/// gradient.err-color-prop): the retired `solid_until=` is refused with
/// the two-equal-stops migration; `color=` is not a Gradient word (it
/// paints its `stops=`).
#[test]
fn solid_until_and_color_are_not_gradient_words() {
    let retired = "View v { Gradient g solid_until=0.94, stops=[white, black] }";
    assert!(fails(retired) && mentions(retired, "stops"), "{:#?}", diags(retired));
    assert!(mentions("View v { Gradient g color=white, stops=[white, black] }", "stops="));
    assert!(fails("View v { Gradient g color=white, stops=[white, black] }"));
}

/// `_gradient_tests` 8407-8408 (gradient.err-transparent-stop,
/// gradient.err-transparent-stop-pinned): a bare `transparent` in stops
/// hides which RGB the fade passes through (straight-alpha blending) — it
/// is refused with the honest spellings (`white/0`) in hand, pinned or
/// not.
#[test]
fn bare_transparent_in_stops_is_refused() {
    assert!(mentions("View v { Gradient g stops=[white, transparent] }", "white/0"));
    assert!(mentions("View v { Gradient g stops=[white@0, transparent@1] }", "white/0"));
}

/// `_gradient_tests` 8417-8447 (gradient.one-stop-code,
/// gradient.one-stop-safe, gradient.one-stop-edits,
/// gradient.one-stop-drops-gradient-only, gradient.one-stop-drop-help,
/// gradient.one-stop-unexcisable-no-fix): a one-stop gradient is a solid —
/// G2003, with the Rectangle rewrite offered (`Rectangle veil color=blue`),
/// dropping gradient-only words like `angle=` (D87); when the extras
/// spread across lines the code still fires, help only. Godot's
/// `applicability`/`edits` fields have no Rust counterpart, so the fix is
/// asserted through the message text.
#[test]
fn a_one_stop_gradient_is_g2003_with_the_rectangle_rewrite() {
    let one = "View v { Gradient veil stops=[blue] }";
    assert!(codes(one).contains(&"G2003".to_string()), "{:#?}", diags(one));
    assert!(mentions(one, "Rectangle") && mentions(one, "color=blue"), "{:#?}", diags(one));
    let with_angle = "View v { Gradient fade fill=linear, angle=90, stops=[white] }";
    assert!(mentions(with_angle, "Rectangle fade color=white") && mentions(with_angle, "dropping gradient-only"), "{:#?}", diags(with_angle));
    let multi = "View v { Gradient fade stops=[white],\n        angle=90 }";
    assert!(codes(multi).contains(&"G2003".to_string()), "{:#?}", diags(multi));
}

/// `_gradient_tests` 8450-8452 (gradient.retired-radial,
/// gradient.retired-ellipse): Godot redirects `Radial` and `Ellipse` with
/// the migration in hand; Rust has no alias table by decision
/// (gogo-design.md §7 "Retired-spelling redirects … Unknown word = build
/// error"), so both are G7011 unknown node type.
#[test]
fn retired_radial_and_ellipse_are_unknown_kinds() {
    assert!(codes("View v { Radial r color=#FFFFFF6B, solid_until=0.94 }").contains(&"G7011".to_string()));
    assert!(codes("View v { Ellipse e 50x50 }").contains(&"G7011".to_string()));
}

// ---------------------------------------------------------- _circle_tests

/// `_circle_tests` 8458-8482 (circle.ok, circle.class, circle.color,
/// circle.size, circle.ring-ok, circle.ring-props, circle.ring-fill-clear,
/// circle.when-color-ok): a Circle is its own drawn disc — `color=$accent`
/// fills it, the box is its written 24×24; a transparent fill plus
/// `border_color=`/`border_thickness=1.5` is a ring; a `when hovered
/// color=` drives the fill through the normal machinery.
#[test]
fn circle_disc_ring_and_when_color() {
    let (page, rects, draws) = scene("View v { Circle dot 24x24, color=$accent }\n$accent = #E51400FF\n");
    let dot = rect_of(&page, &rects, "dot");
    assert_eq!((dot.w, dot.h), (24.0, 24.0), "{dot:?}");
    let c = circles(&draws);
    assert_eq!(c.len(), 1, "{draws:#?}");
    assert_eq!(c[0], (dot, Color_Straight_Srgb_4xU8_RGBA([0xE5, 0x14, 0x00, 0xFF]), 0.0, Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT), "{c:?}");

    let (_, _, draws) = scene("View v { Circle ring width=50, height=50, border_color=#E51400FF, border_thickness=1.5 }");
    let c = circles(&draws);
    assert_eq!(c.len(), 1, "{draws:#?}");
    assert_eq!(c[0].1.0[3], 0, "the fill stays clear: {c:?}");
    assert!(near(c[0].2, 1.5) && c[0].3 == Color_Straight_Srgb_4xU8_RGBA([0xE5, 0x14, 0x00, 0xFF]), "{c:?}");

    let c = ok("View v { Circle c 24x24, color=#FFFFFFFF {\n    when hovered color=#FF0000FF\n} }");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    let id = page.find("c").unwrap();
    let fill = |rt: &Runtime| {
        let res = resolve(&page, rt);
        let rects = layout(&page, &res, Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
        circles(&draw(&page, &res, &rects))[0].1
    };
    assert_eq!(fill(&rt), Color_Straight_Srgb_4xU8_RGBA([0xFF, 0xFF, 0xFF, 0xFF]));
    rt.set_state(id, HOVERED, true);
    assert_eq!(fill(&rt), Color_Straight_Srgb_4xU8_RGBA([0xFF, 0x00, 0x00, 0xFF]));
}

/// `_circle_tests` 8485-8487 (circle.err-corner, circle.err-padding,
/// circle.err-background): box words route away — a circle has no box to
/// pad or round, and its fill is `color=`, not `background=`.
#[test]
fn box_words_are_refused_on_a_circle() {
    assert!(mentions("View v { Circle c corner_radius=4 }", "box"));
    assert!(mentions("View v { Circle c padding=4 }", "box"));
    assert!(mentions("View v { Circle c background=red }", "color="));
}
