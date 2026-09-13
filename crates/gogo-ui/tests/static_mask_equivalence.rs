//! B36-DES stages (1) and (2) gate: neither the static-node mask nor the
//! retained resolve changes anything a page shows. Every fixture runs
//! three roads side by side — mask off, mask on with a full rebuild
//! every resolve, and the retained resolve hosts take — through the
//! same script: build, resolve, flip every state, promote a static node
//! with a code write, play anims, tick, seat a subtree and press inside
//! it, hide a parent over a pressed child, twenty random flips, grow
//! repeats, switch the theme variant. After every step the three
//! `Resolved` snapshots must be identical. A mismatch names the first
//! field and node that differ so the failure reads. The mask is also
//! checked to be non-trivial on the big fixtures (some nodes static,
//! some dynamic), and the retained road to be really local on the
//! effect page (one Button flip re-ladders a handful of nodes), so the
//! pin cannot pass vacuously.
//!
//! Stage (3) rides the same script: after every step each road also
//! lays out and draws — the full road (`layout`, `draw`) beside the
//! retained one (`Runtime::layout_retained`, `Runtime::draw_retained`)
//! — and the rects (bit for bit) and the draw lists must be equal. The
//! script grows the steps layout cares about: a scroll on every Scroll
//! and List, a label rewritten in place, a viewport change (the full
//! road, asserted). The effect page's Button flip must lay out nothing
//! and re-emit under a hundred commands.
use gogo::draw::DrawStats;
use gogo::layout::LayoutStats;
use gogo::page::State;
use gogo::runtime::{DISABLED, FOCUSED, FOCUS_WITHIN, HELD, HOVERED, Item, ItemValue, Resolved, Runtime, SELECTED, resolve_animated};
use gogo::{ArrangementId, BASE, CompileOptions, Draw, Finish, FixedAdvance, Kind, NO_NODE, NO_STR, NodeId, Page, PropKey, Rect, Value, compile, compile_file_with, draw, has_errors, layout};
use std::path::{Path, PathBuf};

/// The box every fixture lays out in; `WIDER` is the viewport-change step.
const VIEWPORT: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
const WIDER: Rect = Rect { x: 0.0, y: 0.0, w: 520.0, h: 900.0 };

// ---------------------------------------------------------------- fixtures

/// godot_seat_pose.rs `SEAT_VIEW`: seat words under arrangements and
/// screen classes.
const SEAT_VIEW: &str = "arrangement open
arrangement parked
arrangement lifted

when screen.landscapeSmall { mover.seat = sideSeat }
when open screen.landscapeSmall { mover.seat = pairSeat }
when parked screen.landscapeSmall { mover.seat = home }
when lifted { mover.seat = liftSeat }
when lifted screen.portraitSmall { mover.seat = pairSeat }

View seatView {
    Row dock halign=left, valign=top, height=60, spacing=0 {
        Column homeCol {
            Rectangle mover color=#FFFFFFFF, 20x20
            Rectangle trail color=#FFFFFFFF, 10x10
        }
        seat sideSeat
        seat pairSeat
        seat liftSeat
    }
}
";

/// godot_arrangement.rs `STATE_VIEW`: arrangements with a view-level
/// timing rule, a style `when`, action buttons.
const STATE_VIEW: &str = "\
arrangement collapsed
arrangement wide

transition duration=0.2, easing=cubic_out

when collapsed {
    bar.margin_bottom = -40
    bar.height = 90
    scrim.opacity = 0.5
}
when wide {
    bar.height = 120
}

style Dot min_width=6, min_height=6 {
    when collapsed opacity=0.25
}

View screen {
    Rectangle scrim color=#000000FF
    Rectangle bar color=#FFFFFFFF, width=fill, valign=bottom, height=60
    Rectangle dot .Dot color=#FFFFFFFF, halign=left, valign=top
    Button goBtn text=\"go\", 50x50, halign=right, valign=top, on:activate=go(collapsed)
    Button toggleBtn text=\"tg\", 50x50, halign=right, valign=bottom, on:activate=toggle(collapsed)
    Label plain text=\"static\"
}
";

/// godot_arrangement.rs `POSE_TIMING`: per-node pose-pair rules.
const POSE_TIMING: &str = "\
arrangement open

transition base -> open duration=0.5, easing=cubic_out
transition open -> base duration=0.5, easing=cubic_out

when open {
    slab.margin_top = 100
    plate.margin_top = 100
    tag.margin_top = 100
}

View screen {
    Rectangle slab 40x40, color=#FFFFFFFF, halign=left, valign=top
    Rectangle plate 40x40, color=#FFFFFFFF, halign=left, valign=top {
        transition base -> open duration=1.0, easing=cubic_out
        transition open -> base duration=1.0, easing=cubic_out
    }
    Rectangle tag 40x40, color=#FFFFFFFF, halign=left, valign=top {
        transition base -> open duration=0
        transition open -> base duration=0
    }
    Rectangle still 40x40, color=#FFFFFF80, opacity=0.5, halign=right, valign=top
}
";

/// godot_scroll_input.rs `DRIFT`: `on:show`/`on:hide` anims on the root,
/// a lane-targeted box, plus a sibling nothing touches.
const DRIFT: &str = "anim fadein { box.opacity from=0.0, to=1.0, duration=0.5 }\nanim vanish duration_each=0.5 {\n box.translate.x to=-60\n and box.opacity to=0.0\n}\nPanel driftRoot on:show=play(fadein), on:hide=play(vanish) {\n Rectangle box color=#FFFFFFFF, min_width=10, min_height=10\n Rectangle bystander color=#FF000080, min_width=10, min_height=10\n}\n";

/// An anim with a held end pose beside a `when` and a declared motion
/// word: three flavours of dynamic node next to two static ones.
const ANIM_MIX: &str = "anim pop { bar.translate.x from=-40, to=0, duration=0.5\n and bar.opacity to=1, duration=0.5 }\nanim tint { bar.background to=#FF0000FF, duration=0.4 }\nView v {\n Column c {\n  Panel bar width=100, height=20, background=#00FF00FF {\n   transition any -> held duration=0.3\n   when held background=#0000FFFF\n  }\n  Image tilted rotate=90, width=20, height=20\n  Label caption text=\"caption\"\n  Button plainBtn text=\"plain\", tap_pulse=true\n }\n}\n";

/// godot_widgets.rs `FADER_SRC` with an instance: a control with a
/// behavior, custom state, drivers and a lane anim.
const FADER: &str = "control Fader $value=0, $min=0, $max=100, $step=0 {\n    behavior slider by $value in $min..$max, step=$step\n    state_builtin hovered, focused, disabled\n    state dragging\n    signal drag_started, drag_ended\n    transition duration=0.12, easing=cubic_out\n    anim releasePop {\n        slot.thumb.translate.y from=-3, to=0, duration=0.2\n    }\n    Layers {\n        slot track width=fill, height=24 {\n            slot progress automatic=length by $value in $min..$max { Rectangle color=#4488FFFF }\n            slot thumb automatic=position by $value in $min..$max { Circle 12x12, color=#FFFFFFFF }\n        }\n        slot label { Label text=live($value) }\n    }\n}\nView v {\n Column col {\n  Fader gain 200x24, value=25 {\n   when dragging opacity=0.4\n  }\n  Label note text=\"volume\"\n  Check tick text=\"on\"\n  Toggle sw text=\"sw\"\n }\n}\n";

/// godot_scroll_input.rs `TALL_LIST` and `FOCUS_WITHIN`: a Scroll, a
/// `focus_within` response.
const TALL_LIST: &str = "View v { Scroll sv width=100, height=100, halign=left, valign=top { Column list { Button b1 text=\"one\"; Rectangle tall width=80, height=1000 } } }";
const FOCUS_WITHIN_VIEW: &str = "View v { Column top {\n Panel rowA background=#00000000 {\n  when focus_within background=#FFFFFF22\n  Row r { Label rowLabel text=\"Music\"; Button inner text=\"s\" }\n }\n Button outside text=\"o\"\n} }";

/// godot_repeat_list.rs `KEYED`: the effect page's preset-strip shape.
const KEYED: &str = r#"View v {
    Column list spacing=4, halign=left, valign=top {
        Label header text="Presets"
        repeat presets by $item.key {
            empty { Label text="none" }
            Button tile width=120, height=20, on:activate=OnTile { Label title text=$item.title, font_size=10 }
        }
        Label footer text="end"
    }
}"#;

/// repeat_rows_keep_their_drivers.rs `BARS`: a counted repeat of a
/// control with drivers.
const BARS: &str = "control Bar $value=0, $min=0, $max=10 {\n    behavior slider by $value in $min..$max\n    state_builtin focused, held\n    transition duration=0.2\n    Layers width=fill, height=20 {\n        slot track width=fill, height=fill {\n            slot progress automatic=length by $value in $min..$max { Rectangle width=fill, height=fill, color=#FF0000FF }\n        }\n        Label lab text=live($value)\n    }\n}\nView v {\n    Column width=fill {\n        repeat rows capacity=2 { Bar b width=fill }\n        Label after text=\"after\"\n    }\n}\n";

/// A Panel seated elsewhere by an arrangement, with a Button deep inside
/// it that has its own `when held`: a flip on a node inside a seated
/// subtree (the retained road must re-ladder it where it sits).
const SEATED_DEEP: &str = "arrangement moved
transition duration=0.2

when moved { box.seat = farSeat }

View v {
    Row r {
        Column home {
            Panel box width=50, height=50 {
                Column inner {
                    Button deep text=\"d\" {
                        transition any -> held duration=0.2
                        when held background=#FF0000FF
                    }
                }
            }
        }
        seat farSeat
    }
}
";

/// A parent whose visibility a `when` flips over a child with its own
/// `when held` and an `on:show` anim: the child's SEEN visibility
/// (GP19) follows the parent, so the fold over ancestors must run for
/// the subtree. Two parents: one hidden by an arrangement, one by its
/// own `selected`.
const HIDE_PARENT: &str = "arrangement hid
anim fadein { child.opacity from=0, to=1, duration=0.3 }

when hid { p.visibility = gone }

View v {
    Column c {
        Panel p width=100, height=100 {
            Button child text=\"c\", on:show=play(fadein) {
                transition any -> held duration=0.2
                when held opacity=0.5
            }
            Label under text=\"u\"
        }
        Button box text=\"b\", toggle=true {
            when selected visibility=gone
            Label inner text=\"i\"
        }
    }
}
";

// ---------------------------------------------------------------- the three roads

/// One fixture's three runtimes on the same page (compiled once and
/// cloned): the mask off (the road before stage 1), the mask on with a
/// full rebuild every resolve (stage 1), and the retained resolve
/// (stage 2, the road hosts take).
struct Roads {
    name: String,
    page_off: Page,
    rt_off: Runtime,
    page_full: Page,
    rt_full: Runtime,
    page_ret: Page,
    rt_ret: Runtime,
    steps: usize,
    /// The box the page lays out in this step (`set_viewport`).
    viewport: Rect,
    /// The draw lists the two retained-road runtimes hand out and take
    /// back: the retained one's, and the switched-off one's (which must
    /// walk whole every time).
    draws_ret: Vec<Draw>,
    draws_off: Vec<Draw>,
}

impl Roads {
    fn new(name: &str, page: Page) -> Roads {
        let page_off = page.clone();
        let page_full = page.clone();
        let page_ret = page;
        let mut rt_off = Runtime::new(&page_off);
        rt_off.debug_set_static_mask(false);
        rt_off.debug_set_retained(false);
        let mut rt_full = Runtime::new(&page_full);
        rt_full.debug_set_retained(false);
        // The stage-3 switch off: this runtime's `layout_retained` and
        // `draw_retained` must take the full road every step.
        rt_full.debug_set_retained_layout(false);
        let rt_ret = Runtime::new(&page_ret);
        Roads { name: name.to_string(), page_off, rt_off, page_full, rt_full, page_ret, rt_ret, steps: 0, viewport: VIEWPORT, draws_ret: Vec::new(), draws_off: Vec::new() }
    }

    fn set_viewport(&mut self, viewport: Rect) {
        self.viewport = viewport;
    }

    /// Applies `f` to all three roads, resolves each, and asserts the
    /// three looks and shown tables equal — and that the look the
    /// retained runtime keeps is the one it handed back. Then lays out
    /// and draws each road and asserts the retained rects and list equal
    /// the full road's (`layout_and_draw`).
    fn step(&mut self, what: &str, mut f: impl FnMut(&mut Page, &mut Runtime)) {
        f(&mut self.page_off, &mut self.rt_off);
        f(&mut self.page_full, &mut self.rt_full);
        f(&mut self.page_ret, &mut self.rt_ret);
        let off: Resolved = resolve_animated(&self.page_off, &mut self.rt_off);
        let full: Resolved = resolve_animated(&self.page_full, &mut self.rt_full);
        let ret: Resolved = resolve_animated(&self.page_ret, &mut self.rt_ret);
        self.steps += 1;
        if let Some(diff) = first_diff(&self.page_full, "mask on", &full, "mask off", &off) {
            panic!("{}: after step {} `{what}` the mask-on resolve differs from mask-off: {diff}", self.name, self.steps);
        }
        if let Some(diff) = first_diff(&self.page_ret, "retained", &ret, "full rebuild", &full) {
            panic!("{}: after step {} `{what}` the retained resolve differs from a full rebuild: {diff}", self.name, self.steps);
        }
        assert_eq!(self.rt_full.shown, self.rt_off.shown, "{}: after step {} `{what}` the shown table differs between mask on and off", self.name, self.steps);
        assert_eq!(self.rt_ret.shown, self.rt_full.shown, "{}: after step {} `{what}` the shown table differs between retained and full", self.name, self.steps);
        assert!(*self.rt_ret.resolved() == ret, "{}: after step {} `{what}` the runtime's kept look is not the one it returned", self.name, self.steps);
        self.layout_and_draw(what, &off, &full, &ret);
    }

    /// Stage (3): every road lays out and glides (so the three runtimes
    /// stay in step — `glide` feeds `%` anim endpoints back), then the
    /// rects and the draw list of the retained road are checked against
    /// the full road bit for bit, and the switched-off road is checked
    /// to have walked everything.
    fn layout_and_draw(&mut self, what: &str, off: &Resolved, full: &Resolved, ret: &Resolved) {
        let vp: Rect = self.viewport;
        // Mask-off road: the plain layout, glided so this runtime stays
        // in step with the other two (its draw list is nobody's reference).
        let rects_off: Vec<Rect> = layout(&self.page_off, off, vp, &FixedAdvance);
        let _shown_off: Vec<Rect> = self.rt_off.glide(rects_off);
        // Retained-off road: the caches switched off must equal the plain functions.
        let rects_full: Vec<Rect> = layout(&self.page_full, full, vp, &FixedAdvance);
        let rects_full_cache: Vec<Rect> = self.rt_full.layout_retained(&self.page_full, vp, &FixedAdvance).to_vec();
        self.same_rects(what, "the switched-off retained layout", &rects_full_cache, "`layout`", &rects_full);
        assert!(self.rt_full.debug_layout_stats().full, "{}: after step {} `{what}` the switched-off retained layout did not take the full road", self.name, self.steps);
        let shown_full: Vec<Rect> = self.rt_full.glide(rects_full);
        let draws_full: Vec<Draw> = draw(&self.page_full, full, &shown_full);
        self.rt_full.draw_retained(&self.page_full, &shown_full, &mut self.draws_off);
        let draws_off: Vec<Draw> = std::mem::take(&mut self.draws_off);
        self.same_draws(what, "the switched-off retained draw", &draws_off, "`draw`", &draws_full);
        self.draws_off = draws_off;
        assert!(self.rt_full.debug_draw_stats().full, "{}: after step {} `{what}` the switched-off retained draw did not walk everything", self.name, self.steps);
        // Retained road: the caches on.
        let rects_ref: Vec<Rect> = layout(&self.page_ret, ret, vp, &FixedAdvance);
        let rects_ret: Vec<Rect> = self.rt_ret.layout_retained(&self.page_ret, vp, &FixedAdvance).to_vec();
        self.same_rects(what, "the retained layout", &rects_ret, "`layout`", &rects_ref);
        let shown_ret: Vec<Rect> = self.rt_ret.glide(rects_ret);
        let draws_ref: Vec<Draw> = draw(&self.page_ret, ret, &shown_ret);
        self.rt_ret.draw_retained(&self.page_ret, &shown_ret, &mut self.draws_ret);
        let draws_ret: Vec<Draw> = std::mem::take(&mut self.draws_ret);
        self.same_draws(what, "the retained draw", &draws_ret, "`draw`", &draws_ref);
        self.draws_ret = draws_ret;
    }

    fn same_rects(&self, what: &str, a_name: &str, a: &[Rect], b_name: &str, b: &[Rect]) {
        if a.len() != b.len() {
            panic!("{}: after step {} `{what}` {a_name} has {} rects, {b_name} {}", self.name, self.steps, a.len(), b.len());
        }
        if let Some(i) = (0..a.len()).find(|i| a[*i] != b[*i]) {
            let node = self.page_ret.nodes.get(i).map(|n| self.page_ret.str(n.name).to_string()).unwrap_or_default();
            panic!("{}: after step {} `{what}` {a_name} differs from {b_name} at node {i} (`{node}`): {:?} vs {:?}; layout stats {:?}", self.name, self.steps, a[i], b[i], self.rt_ret.debug_layout_stats());
        }
    }

    fn same_draws(&self, what: &str, a_name: &str, a: &[Draw], b_name: &str, b: &[Draw]) {
        let n = a.len().max(b.len());
        if let Some(i) = (0..n).find(|i| a.get(*i) != b.get(*i)) {
            panic!("{}: after step {} `{what}` {a_name} differs from {b_name} at command {i} of {}/{}: {:?} vs {:?}; draw stats {:?}", self.name, self.steps, a.len(), b.len(), a.get(i), b.get(i), self.rt_ret.debug_draw_stats());
        }
    }

    fn resolve(&mut self, what: &str) {
        self.step(what, |_, _| {});
    }

    fn tick(&mut self, what: &str, dt: f32) {
        self.step(what, |page, rt| {
            rt.tick(page, dt);
        });
    }

    fn page(&self) -> &Page {
        &self.page_ret
    }

    /// Static and dynamic counts under the mask.
    fn counts(&self) -> (usize, usize) {
        let n = self.page_ret.nodes.len();
        let stat = (0..n).filter(|i| self.rt_ret.is_static(NodeId::from_index(*i))).count();
        (stat, n - stat)
    }
}

/// The first field (and, for a per-node table, the first node) where the
/// two snapshots disagree, spelled for a failure message.
fn first_diff(page: &Page, a_name: &str, a: &Resolved, b_name: &str, b: &Resolved) -> Option<String> {
    fn table<T: PartialEq + std::fmt::Debug>(page: &Page, names: (&str, &str), field: &str, a: &[T], b: &[T]) -> Option<String> {
        if a.len() != b.len() {
            return Some(format!("{field}: {} entries {}, {} {}", a.len(), names.0, b.len(), names.1));
        }
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            if x != y {
                let name = page.nodes.get(i).map(|n| page.str(n.name).to_string()).unwrap_or_default();
                return Some(format!("{field}[{i}] (node `{name}`): {}={x:?} {}={y:?}", names.0, names.1));
            }
        }
        None
    }
    fn list<T: PartialEq + std::fmt::Debug>(names: (&str, &str), field: &str, a: &[T], b: &[T]) -> Option<String> {
        if a.len() != b.len() {
            return Some(format!("{field}: {} entries {}, {} {}", a.len(), names.0, b.len(), names.1));
        }
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            if x != y {
                return Some(format!("{field}[{i}]: {}={x:?} {}={y:?}", names.0, names.1));
            }
        }
        None
    }
    fn scalar<T: PartialEq + std::fmt::Debug>(names: (&str, &str), field: &str, a: &T, b: &T) -> Option<String> {
        (a != b).then(|| format!("{field}: {}={a:?} {}={b:?}", names.0, names.1))
    }
    if a == b {
        return None;
    }
    let names = (a_name, b_name);
    table(page, names, "anim_owned", &a.anim_owned, &b.anim_owned)
        .or_else(|| list(names, "anim_owned_props", &a.anim_owned_props, &b.anim_owned_props))
        .or_else(|| table(page, names, "placement", &a.placement, &b.placement))
        .or_else(|| list(names, "overrides", &a.overrides, &b.overrides))
        .or_else(|| table(page, names, "override_span", &a.override_span, &b.override_span))
        .or_else(|| scalar(names, "override_tail", &a.override_tail, &b.override_tail))
        .or_else(|| scalar(names, "variant", &a.variant, &b.variant))
        .or_else(|| table(page, names, "scroll", &a.scroll, &b.scroll))
        .or_else(|| scalar(names, "safe", &a.safe, &b.safe))
        .or_else(|| table(page, names, "motion", &a.motion, &b.motion))
        .or_else(|| table(page, names, "seat", &a.seat, &b.seat))
        .or_else(|| table(page, names, "states", &a.states, &b.states))
        .or_else(|| list(names, "raised", &a.raised, &b.raised))
        .or_else(|| list(names, "repeat_rows", &a.repeat_rows, &b.repeat_rows))
        .or_else(|| list(names, "hiding", &a.hiding, &b.hiding))
        .or_else(|| list(names, "auto", &a.auto, &b.auto))
        .or_else(|| list(names, "presence", &a.presence, &b.presence))
        .or_else(|| scalar(names, "caret", &a.caret, &b.caret))
        .or_else(|| scalar(names, "selection", &a.selection, &b.selection))
        .or_else(|| scalar(names, "sel_handles", &a.sel_handles, &b.sel_handles))
        .or_else(|| (a.locale != b.locale).then(|| "locale".to_string()))
        .or_else(|| Some("some field differs that first_diff does not spell — extend it".to_string()))
}

/// Every `when` that writes `key`: (the node it restyles, its state, the
/// node whose state it watches — the node itself for a plain `when`).
fn whens_writing(page: &Page, key: PropKey) -> Vec<(NodeId, State, NodeId)> {
    let mut out: Vec<(NodeId, State, NodeId)> = Vec::new();
    for (i, node) in page.nodes.iter().enumerate() {
        let id = NodeId::from_index(i);
        for w in &page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize] {
            if page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize].iter().any(|p| p.key == key) {
                out.push((id, w.state, if w.of == NO_NODE { id } else { w.of }));
            }
        }
    }
    out
}

/// Can `flip` turn this state on and off from code? Screen words need
/// the host's platform enums; the script leaves them to `set_screen`
/// pins elsewhere.
fn flippable(state: State) -> bool {
    !matches!(state, State::ScreenOrientation(_) | State::ScreenClass(_) | State::ArrangementScreen(..) | State::ScreenPair(..))
}

/// Turns `state` on `owner` on or off the way code does: an arrangement
/// through `go_id` (off = back to base), a built-in state through its
/// bit, a custom state on the declared ancestor that declares it.
fn flip(page: &Page, rt: &mut Runtime, owner: NodeId, state: State, on: bool) {
    match state {
        State::Arrangement(i) => rt.go_id(if on { i } else { BASE }),
        State::Held => rt.set_state(owner, HELD, on),
        State::Hovered => rt.set_state(owner, HOVERED, on),
        State::Focused => rt.set_state(owner, FOCUSED, on),
        State::FocusWithin => rt.set_state(owner, FOCUS_WITHIN, on),
        State::Selected => rt.set_state(owner, SELECTED, on),
        State::StateDisabled => rt.set_state(owner, DISABLED, on),
        State::Custom(s) => {
            let mut a = owner;
            while a != NO_NODE {
                if page.custom_states.iter().any(|(root, st)| *root == a && *st == s) {
                    rt.set_custom(page, a, page.str(s), on).unwrap();
                    return;
                }
                a = page.declared_parent(a);
            }
        }
        State::ScreenOrientation(_) | State::ScreenClass(_) | State::ArrangementScreen(..) | State::ScreenPair(..) => {}
    }
}

// ---------------------------------------------------------------- the script

/// How many nodes of one kind get their own flip-and-resolve; the rest
/// flip together in one step (a 1175-node page has hundreds of buttons
/// and a debug resolve is slow).
const SOLO_FLIPS: usize = 16;

/// Runs the whole script on one fixture and returns the static/dynamic
/// counts under the mask.
fn run_script(name: &str, page: Page) -> (usize, usize) {
    let mut roads = Roads::new(name, page);
    let n = roads.page().nodes.len();
    roads.resolve("first resolve");
    // Built-in state flips: HELD on every Button (press, release), SELECTED
    // on every Check/Toggle/Radio, a tick between so `transition` tweens run.
    let of_kind = |page: &Page, kinds: &[Kind]| -> Vec<NodeId> { (0..n).map(NodeId::from_index).filter(|id| kinds.contains(&page.nodes[*id].kind)).collect() };
    let buttons = of_kind(roads.page(), &[Kind::Button]);
    for (k, b) in buttons.iter().enumerate() {
        if k < SOLO_FLIPS {
            roads.step("press a Button", |_, rt| rt.set_state(*b, HELD, true));
            roads.tick("tick mid-press", 0.05);
            roads.step("release the Button", |_, rt| rt.set_state(*b, HELD, false));
        }
    }
    if buttons.len() > SOLO_FLIPS {
        let rest: Vec<NodeId> = buttons[SOLO_FLIPS..].to_vec();
        roads.step("press the remaining Buttons", |_, rt| rest.iter().for_each(|b| rt.set_state(*b, HELD, true)));
        roads.tick("tick mid-press", 0.05);
        roads.step("release the remaining Buttons", |_, rt| rest.iter().for_each(|b| rt.set_state(*b, HELD, false)));
    }
    roads.tick("tick until the press tweens land", 0.5);
    let marks = of_kind(roads.page(), &[Kind::Check, Kind::Toggle, Kind::Radio]);
    for (k, m) in marks.iter().enumerate() {
        if k < SOLO_FLIPS {
            roads.step("select a mark", |_, rt| rt.set_state(*m, SELECTED, true));
            roads.step("deselect the mark", |_, rt| rt.set_state(*m, SELECTED, false));
        }
    }
    if marks.len() > SOLO_FLIPS {
        let rest: Vec<NodeId> = marks[SOLO_FLIPS..].to_vec();
        roads.step("select the remaining marks", |_, rt| rest.iter().for_each(|m| rt.set_state(*m, SELECTED, true)));
        roads.step("deselect the remaining marks", |_, rt| rest.iter().for_each(|m| rt.set_state(*m, SELECTED, false)));
    }
    // Every declared custom state on every control root.
    let customs: Vec<(NodeId, String)> = roads.page().custom_states.iter().map(|(root, s)| (*root, roads.page().str(*s).to_string())).collect();
    for (root, state) in &customs {
        roads.step("raise a custom state", |page, rt| rt.set_custom(page, *root, state, true).unwrap());
        roads.tick("tick with the state up", 0.05);
        roads.step("clear the custom state", |page, rt| rt.set_custom(page, *root, state, false).unwrap());
    }
    // Every arrangement: in, tick through the glide, back to base.
    let arrangements = roads.page().arrangements.len();
    for i in 0..arrangements {
        roads.step("go to an arrangement", |_, rt| rt.go_id(ArrangementId::from_index(i)));
        roads.tick("tick mid-flip", 0.1);
        roads.tick("tick mid-flip", 0.1);
    }
    if arrangements > 0 {
        roads.step("back to the base pose", |_, rt| rt.go_id(BASE));
        roads.tick("tick after the return", 1.0);
    }
    // A flip on a node deep in a SEATED subtree: every `when` that seats
    // a node (`x.seat = name`) is turned on — by its arrangement, or the
    // state on its owner — and a Button inside the seated node is
    // pressed and released while it sits there.
    let seaters: Vec<(NodeId, State, NodeId)> = whens_writing(roads.page(), PropKey::SeatOf).into_iter().filter(|(_, s, _)| flippable(*s)).collect();
    for (node, state, owner) in seaters.iter().take(3) {
        let inside: Option<NodeId> = (0..n).map(NodeId::from_index).find(|b| roads.page().nodes[*b].kind == Kind::Button && roads.page().is_under(*b, *node));
        roads.step("turn a seating `when` on", |page, rt| flip(page, rt, *owner, *state, true));
        roads.tick("tick after the seat move", 0.05);
        if let Some(b) = inside {
            roads.step("press a Button inside the seated subtree", |_, rt| rt.set_state(b, HELD, true));
            roads.tick("tick mid-press in the seat", 0.05);
            roads.step("release it", |_, rt| rt.set_state(b, HELD, false));
        }
        roads.step("turn the seating `when` off", |page, rt| flip(page, rt, *owner, *state, false));
        roads.tick("tick after the seat move back", 0.5);
    }
    // A flip that changes a PARENT's visibility over a pressed child:
    // the child's seen visibility (GP19) follows its ancestors, so the
    // fold must run for the subtree, not the parent alone.
    let hiders: Vec<(NodeId, State, NodeId)> = whens_writing(roads.page(), PropKey::Visibility).into_iter().filter(|(node, s, _)| flippable(*s) && roads.page().nodes[*node].children.1 > 0).collect();
    for (node, state, owner) in hiders.iter().take(3) {
        let inside: Option<NodeId> = (0..n).map(NodeId::from_index).find(|b| *b != *node && roads.page().nodes[*b].kind == Kind::Button && roads.page().is_under(*b, *node));
        if let Some(b) = inside {
            roads.step("press a Button under a parent about to hide", |_, rt| rt.set_state(b, HELD, true));
        }
        roads.step("hide the parent through its `when`", |page, rt| flip(page, rt, *owner, *state, true));
        roads.tick("tick with the parent hidden", 0.05);
        if let Some(b) = inside {
            roads.step("release the Button while hidden", |_, rt| rt.set_state(b, HELD, false));
        }
        roads.step("show the parent again", |page, rt| flip(page, rt, *owner, *state, false));
        roads.tick("tick after the show", 0.4);
    }
    // Twenty random flips in a row (a fixed-seed xorshift; the seed is
    // in every step's name, so a failure names it).
    let seed0: u64 = 0x9E37_79B9_7F4A_7C15 ^ (n as u64);
    let mut seed: u64 = seed0;
    let mut roll = move || -> u64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for k in 0..20 {
        let r: u64 = roll();
        let pick = (r >> 8) as usize;
        let what = format!("random flip {k} (seed {seed0:#x})");
        match r % 4 {
            0 if !buttons.is_empty() => {
                let b = buttons[pick % buttons.len()];
                let on = !roads.rt_ret.has(b, HELD);
                roads.step(&what, |_, rt| rt.set_state(b, HELD, on));
            }
            1 if !marks.is_empty() => {
                let m = marks[pick % marks.len()];
                let on = !roads.rt_ret.has(m, SELECTED);
                roads.step(&what, |_, rt| rt.set_state(m, SELECTED, on));
            }
            2 if arrangements > 0 => {
                let i = pick % (arrangements + 1);
                let target = if i == arrangements { BASE } else { ArrangementId::from_index(i) };
                roads.step(&what, |_, rt| rt.go_id(target));
            }
            _ => roads.tick(&what, 0.03),
        }
    }
    roads.tick("tick the random flips' motion out", 1.5);
    // Promotion: a code write on a static node, then taken back; a static
    // node with children hidden (the ancestor fold over static children).
    let first_static: Option<NodeId> = (0..n).map(NodeId::from_index).find(|id| roads.rt_ret.is_static(*id));
    if let Some(s) = first_static {
        roads.step("write opacity on a static node", |_, rt| rt.set_prop(s, PropKey::Opacity, Value::VNum(0.5)));
        roads.step("write a color on the same node", |_, rt| rt.set_prop(s, PropKey::Color, Value::VColor(gogo::Color_Straight_Srgb_4xU8_RGBA([1, 2, 3, 128]))));
        roads.step("take the opacity write back", |_, rt| rt.clear_prop(s, PropKey::Opacity));
        roads.step("take the color write back", |_, rt| rt.clear_prop(s, PropKey::Color));
        assert!(roads.rt_ret.is_static(s), "{name}: the node is static again once its writes are gone");
    }
    let static_parent: Option<NodeId> = (0..n).map(NodeId::from_index).find(|id| roads.rt_ret.is_static(*id) && roads.page().nodes[*id].children.1 > 0);
    if let Some(p) = static_parent {
        roads.step("hide a static parent", |page, rt| rt.set_visible(page, p, false));
        roads.step("show it again", |page, rt| rt.set_visible(page, p, true));
        roads.step("take the visibility write back", |_, rt| rt.clear_prop(p, PropKey::Visibility));
    }
    // Stage (3), the steps layout cares about. A scroll offset on every
    // Scroll and List (the Scroll re-places its children, nothing else),
    // moved and back.
    let scrolls: Vec<NodeId> = of_kind(roads.page(), &[Kind::Scroll, Kind::List]);
    for s in &scrolls {
        roads.step("scroll a Scroll", |_, rt| rt.set_scroll(*s, [7.0, 13.0]));
        roads.tick("tick after the scroll", 0.05);
        roads.step("scroll it back", |_, rt| rt.set_scroll(*s, [0.0, 0.0]));
    }
    // A label's text from code: the first write interns a slot, the next
    // two rewrite it IN PLACE (the look shows the same string id — only
    // the runtime's report tells the retained layout), then the write
    // goes and the file's text is back.
    if let Some(label) = of_kind(roads.page(), &[Kind::Label]).first().copied() {
        roads.step("write a label's text", |page, rt| rt.set_text(page, label, "a longer line of words for the row"));
        roads.step("rewrite it in place, shorter", |page, rt| rt.set_text(page, label, "short"));
        roads.step("rewrite it in place, longer", |page, rt| rt.set_text(page, label, "and a much longer line of words that wraps somewhere in a narrow box"));
        roads.step("take the text write back", |_, rt| rt.clear_prop(label, PropKey::Text));
    }
    // A viewport change is the full road, and back.
    roads.set_viewport(WIDER);
    roads.resolve("a wider viewport");
    assert!(roads.rt_ret.debug_layout_stats().full, "{name}: a viewport change must take the full layout road");
    roads.set_viewport(VIEWPORT);
    roads.resolve("the viewport back");
    // Anims: play the first few, tick three frames each.
    let anims: Vec<gogo::StrId> = roads.page().anim_heads.iter().take(3).map(|h| h.anim).collect();
    for a in &anims {
        roads.step("play an anim", |page, rt| rt.play_id(page, *a, Finish::Nothing));
        for _ in 0..3 {
            roads.tick("tick a frame of the anim", 1.0 / 60.0);
        }
        roads.tick("tick the anim to its end", 2.0);
    }
    // Repeats: keyed rows in, reordered, out; counted rows up and down.
    let repeats: Vec<gogo::page::RepeatInfo> = roads.page().repeats.clone();
    for info in &repeats {
        let repeat = info.node;
        if info.keyed {
            let by: String = if info.by == NO_STR { String::new() } else { roads.page().str(info.by).to_string() };
            let (s0, sc) = info.sites;
            let mut fields: Vec<String> = roads.page().bind_sites[s0 as usize..(s0 + sc) as usize].iter().map(|s| roads.page().str(s.field).to_string()).collect();
            fields.sort();
            fields.dedup();
            // `by $item` takes plain values (every site binds the whole
            // item); `by $item.<field>` takes records (every site binds a
            // field). Any other shape would be refused, so skip it.
            let shape_ok = if by.is_empty() { fields.iter().all(String::is_empty) } else { fields.iter().all(|f| !f.is_empty()) };
            if !shape_ok {
                continue;
            }
            let item = |i: i64| -> Item {
                if by.is_empty() {
                    return Item::Value(ItemValue::Int(i));
                }
                let mut rec: Vec<(String, ItemValue)> = vec![(by.clone(), ItemValue::Int(i))];
                for f in &fields {
                    if *f != by && !f.is_empty() {
                        rec.push((f.clone(), ItemValue::Str(format!("row {i}"))));
                    }
                }
                Item::Record(rec)
            };
            let three: Vec<Item> = (1..=3).map(item).collect();
            let two: Vec<Item> = [3, 1].iter().map(|i| item(*i)).collect();
            roads.step("set three keyed rows", |page, rt| rt.set_items(page, repeat, &three).unwrap());
            roads.step("reorder to two rows", |page, rt| rt.set_items(page, repeat, &two).unwrap());
            roads.step("clear the rows", |page, rt| rt.set_items(page, repeat, &[]).unwrap());
            roads.step("three rows again", |page, rt| rt.set_items(page, repeat, &three).unwrap());
            // Growth past the pool: new rows clone in (a node-count
            // change — the retained road's full-rebuild case), then a
            // press on the newest row.
            let six: Vec<Item> = (1..=6).map(item).collect();
            roads.step("grow to six rows", |page, rt| rt.set_items(page, repeat, &six).unwrap());
            let newest: Option<NodeId> = roads.rt_ret.items(roads.page(), repeat).last().copied().and_then(|row| (row.index()..roads.page().nodes.len()).map(NodeId::from_index).find(|b| roads.page().nodes[*b].kind == Kind::Button && roads.page().is_under(*b, row)));
            if let Some(b) = newest {
                roads.step("press a Button on the newest row", |_, rt| rt.set_state(b, HELD, true));
                roads.tick("tick mid-press on the new row", 0.05);
                roads.step("release it", |_, rt| rt.set_state(b, HELD, false));
            }
            roads.step("back to three rows", |page, rt| rt.set_items(page, repeat, &three).unwrap());
        } else {
            roads.step("count three rows", |page, rt| rt.set_count(page, repeat, 3));
            roads.step("count one row", |page, rt| rt.set_count(page, repeat, 1));
        }
    }
    // A theme variant switch, alone and in the same frame as a pose flip
    // (the road where a static node's declared colors change).
    roads.step("switch to the dark variant", |page, rt| rt.variant = page.intern("dark"));
    if arrangements > 0 {
        roads.step("switch back to light and flip a pose in one frame", |_, rt| {
            rt.variant = NO_STR;
            rt.go_id(ArrangementId::from_index(0));
        });
        roads.tick("tick mid-flip", 0.1);
        roads.step("back to base", |_, rt| rt.go_id(BASE));
        roads.tick("tick after the return", 1.0);
    } else {
        roads.step("switch back to light", |_, rt| rt.variant = NO_STR);
    }
    roads.counts()
}

fn compiled(name: &str, src: &str) -> Page {
    let c = compile(src, &|_| None);
    assert!(!has_errors(&c.diags), "{name}: {:#?}", c.diags);
    c.page
}

#[test]
fn inline_fixtures_resolve_the_same_on_all_three_roads() {
    let fixtures: &[(&str, &str)] = &[("SEAT_VIEW", SEAT_VIEW), ("STATE_VIEW", STATE_VIEW), ("POSE_TIMING", POSE_TIMING), ("DRIFT", DRIFT), ("ANIM_MIX", ANIM_MIX), ("FADER", FADER), ("TALL_LIST", TALL_LIST), ("FOCUS_WITHIN", FOCUS_WITHIN_VIEW), ("KEYED", KEYED), ("BARS", BARS), ("SEATED_DEEP", SEATED_DEEP), ("HIDE_PARENT", HIDE_PARENT)];
    let mut any_static = 0;
    let mut any_dynamic = 0;
    for (name, src) in fixtures {
        let (s, d) = run_script(name, compiled(name, src));
        any_static += s;
        any_dynamic += d;
    }
    assert!(any_static > 0 && any_dynamic > 0, "the mask is trivial over the inline fixtures: {any_static} static, {any_dynamic} dynamic");
}

/// The mask must sort ANIM_MIX exactly: the anim target, the `when`
/// node, the declared `rotate=`, the `tap_pulse` button (static exactly
/// when the compiler attached it no `when` and no handler — read off the
/// compiled node below) and the plain caption.
#[test]
fn the_rule_sorts_the_mixed_fixture_as_documented() {
    let page = compiled("ANIM_MIX", ANIM_MIX);
    let rt = Runtime::new(&page);
    let at = |name: &str| page.find(name).unwrap_or_else(|| panic!("no `{name}`"));
    assert!(!rt.is_static(at("bar")), "an anim target with a `when` is dynamic");
    assert!(!rt.is_static(at("tilted")), "a declared motion word is dynamic");
    assert!(rt.is_static(at("caption")), "a plain label is static");
    // A press surface is static only when the compiler attached no `when`
    // and no handler of its own; the assertion reads the compiled node so
    // the rule, not a guess, decides (the built-in press feedback rides
    // `compose_anim`, which needs no `when`).
    let btn = at("plainBtn");
    let n = &page.nodes[btn.index()];
    let expect_static: bool = n.whens.1 == 0 && n.handlers.1 == 0;
    assert_eq!(rt.is_static(btn), expect_static, "the tap_pulse button: whens={} handlers={}", n.whens.1, n.handlers.1);
    assert!(rt.is_static(at("c")), "a plain column is static");
}

#[test]
fn whats_new_fixture_resolves_the_same_and_the_mask_is_not_trivial() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/whats_new.gogo");
    let c = gogo::compile_file(&path).unwrap();
    assert!(!has_errors(&c.diags), "{:#?}", c.diags);
    let (s, d) = run_script("whats_new.gogo", c.page);
    assert!(s > 0 && d > 0, "whats_new.gogo: the mask is trivial ({s} static, {d} dynamic)");
}

/// Every page under crates/app/ui, compiled the way the app's build does
/// (its one code control named; keys and images per page instead of the
/// build-wide tables, which resolve does not read).
#[test]
fn app_pages_resolve_the_same_with_the_mask_on_and_off() {
    let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/ui");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        panic!("{} is not there — the app's pages are the big fixtures this pin needs", dir.display());
    };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "gogo")).collect();
    paths.sort();
    let is_page = |source: &str| source.lines().any(|l| l.starts_with("View ") || l.starts_with("View{") || l == "View" || l.starts_with("Panel ") || l.starts_with("Panel{") || l == "Panel");
    let options = CompileOptions { code_controls: &["GradientEdit"], ..CompileOptions::default() };
    let mut ran = 0;
    let mut big: Vec<(String, usize, usize)> = Vec::new();
    for path in &paths {
        let text = std::fs::read_to_string(path).unwrap();
        if !is_page(&text) {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let c = compile_file_with(path, options).unwrap();
        assert!(!has_errors(&c.diags), "{name}: {:#?}", c.diags);
        let n = c.page.nodes.len();
        let (s, d) = run_script(&name, c.page);
        if n >= 200 {
            big.push((name.clone(), s, d));
        }
        ran += 1;
    }
    assert!(ran >= 20, "only {ran} app pages ran — the ui folder moved?");
    assert!(!big.is_empty(), "no app page with 200+ nodes — the big fixtures this pin exists for are missing");
    for (name, s, d) in &big {
        assert!(*s > 0 && *d > 0, "{name}: the mask is trivial ({s} static, {d} dynamic)");
    }
}

/// One frame of a host: resolve, the retained layout, the glide, the
/// retained draw. Returns how many nodes the resolve re-laddered and
/// what the two retained roads did.
fn frame(page: &Page, rt: &mut Runtime, draws: &mut Vec<Draw>) -> (u32, LayoutStats, DrawStats) {
    resolve_animated(page, rt);
    let rects: Vec<Rect> = rt.layout_retained(page, VIEWPORT, &FixedAdvance).to_vec();
    let shown: Vec<Rect> = rt.glide(rects);
    rt.draw_retained(page, &shown, draws);
    (rt.debug_laddered(), rt.debug_layout_stats(), rt.debug_draw_stats())
}

/// The retained roads are really local on the big page: one Button HELD
/// flip on the effect page re-ladders the Button and its watchers, not
/// the page; the tick after it re-ladders nothing (no state moved); a
/// full rebuild is the first resolve only. Stage (3): the press writes
/// a background, no layout word — the retained layout measures nothing
/// and re-places no box, and the retained draw re-emits the Button's
/// commands and its ancestors' own, under a hundred, the rest copied.
/// The counts are printed so a run shows them (`--nocapture`).
#[test]
fn a_button_flip_on_the_effect_page_stays_local() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/ui/effect_page.gogo");
    let options = CompileOptions { code_controls: &["GradientEdit"], ..CompileOptions::default() };
    let c = compile_file_with(&path, options).unwrap();
    assert!(!has_errors(&c.diags), "{:#?}", c.diags);
    let mut page = c.page;
    let n = page.nodes.len();
    // 299 nodes as compiled; the 1175 the device shows are the preset rows
    // and other repeat clones `set_items` adds at runtime.
    assert!(n >= 250, "effect_page.gogo has {n} nodes — the big fixture this pin exists for shrank?");
    let mut rt = Runtime::new(&page);
    let mut draws: Vec<Draw> = Vec::new();
    let (first, first_lay, first_draw) = frame(&page, &mut rt, &mut draws);
    assert_eq!(first as usize, n, "the first resolve is a full rebuild");
    assert!(first_lay.full && first_draw.full, "the first layout and draw are the full roads");
    // One settled frame: the rebuild's seat table may send the `when
    // disabled` watchers through once (`disabled` reads placed parents a
    // resolve late); a press after that is the steady-state case.
    let (settled, _, _) = frame(&page, &mut rt, &mut draws);
    let button: NodeId = (0..n).map(NodeId::from_index).find(|id| page.nodes[*id].kind == Kind::Button && page.nodes[*id].whens.1 > 0).expect("a Button with a `when` of its own");
    rt.set_state(button, HELD, true);
    let (pressed, lay, dr) = frame(&page, &mut rt, &mut draws);
    rt.tick(&mut page, 0.05);
    let (ticked, lay_tick, dr_tick) = frame(&page, &mut rt, &mut draws);
    rt.set_state(button, HELD, false);
    let (released, lay_rel, dr_rel) = frame(&page, &mut rt, &mut draws);
    println!("effect page: {n} nodes, {} draw commands; the settled frame re-laddered {settled}, a Button HELD flip {pressed}, the tick after it {ticked}, the release {released}", draws.len());
    println!("effect page: the press measured {} nodes over {} boxes (full: {}) and re-emitted {} commands (full: {}); the tick {} / {} / {}; the release {} / {} / {}", lay.measured, lay.roots, lay.full, dr.emitted, dr.full, lay_tick.measured, dr_tick.emitted, dr_tick.full, lay_rel.measured, dr_rel.emitted, dr_rel.full);
    assert!(pressed < 50, "the press re-laddered {pressed} of {n} nodes — the reach is not local");
    assert!(ticked < 50, "a tick with no state change re-laddered {ticked} nodes");
    assert!(released < 50, "the release re-laddered {released} of {n} nodes — the reach is not local");
    for (when, l, d) in [("press", lay, dr), ("tick", lay_tick, dr_tick), ("release", lay_rel, dr_rel)] {
        assert!(!l.full && l.measured < 100, "the {when} took the layout road for {} measures (full: {}) — the change is not local", l.measured, l.full);
        assert!(!d.full && d.emitted < 100, "the {when} re-emitted {} draw commands (full: {}) — the change is not local", d.emitted, d.full);
    }
}
