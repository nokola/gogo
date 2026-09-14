//! Port of the Godot gogo suite's seat / pose / reveal groups
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//!
//! - `_seat_tests` 7282-7441 (38 checks): seat load rules, the move /
//!   restore lifecycle through screen classes and arrangements.
//! - `_arrangement_anchor_tests` 7441-7478 (4): an untimed arrangement
//!   re-anchors a free child.
//! - `_flow_pose_tests` 7478-7558 (13): a `when` resizes stack children.
//! - `_pose_size_tests` 7558-7659 (15): posed sizes move the real box, a
//!   declared floor stands under a posed width, timed grow.
//! - `_reveal_tests` 7659-7786 (23): `reveal_x` / `reveal_y`.
//!
//! Fixtures `seat_view.gogo`, `arrangement_anchor.gogo`, `flow_view.gogo`,
//! `pose_size_view.gogo`, `reveal_view.gogo` are inlined below, translated
//! to the Rust dialect (`Control` → `Layers`, `autowrap_mode` → `wrap`).
//!
//! ENGINE (not ported):
//! - `reveal.proxy` (7714): clipping proxy node structure, script check.
//! - `reveal.props-head` (7722): GogoProps script value routing.
//! - `reveal.flags-mirror` (7726): Godot size-flag mirroring on the proxy.
//!
//! Godot's `custom_minimum_size` checks in `_pose_size_tests` are ported
//! as the box the user sees (the stamp IS the rendered size there), and
//! the stamp-timing trio as the glide the stamp exists to allow.
use gogo::input::{Input, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{NodeId, PropKey};
use gogo::runtime::{Resolved, Runtime, resolve, resolve_animated};
use gogo::{Compiled, EventKind, Orientation, Page, ScreenClass, compile};

fn build(src: &str) -> Compiled {
    compile(src, &|_| None)
}

fn ok(src: &str) -> Compiled {
    let c = build(src);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn refuses(src: &str) -> bool {
    gogo::has_errors(&build(src).diags)
}

/// The labels of the snippets Rust builds WITHOUT an error. Godot refuses
/// every one, so an empty list is parity; the panic names the accepted
/// ones so one run reports the whole family.
fn accepted(cases: &[(&str, &str)]) -> Vec<String> {
    cases.iter().filter(|(_, src)| !refuses(src)).map(|(label, _)| label.to_string()).collect()
}

/// One page with its runtime: resolve → layout in a 400×800 viewport.
struct Fx {
    page: Page,
    rt: Runtime,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = ok(src);
        let rt = Runtime::new(&c.page);
        Fx { page: c.page, rt }
    }

    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }

    fn lay(&self) -> (Resolved, Vec<Rect>) {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        (res, rects)
    }

    fn rect(&self, name: &str) -> Rect {
        self.lay().1[(self.id(name)).index()]
    }

    /// The names of the nodes laid out under `parent`, in layout order
    /// (Godot: `get_parent()` + `get_index()`).
    fn children_of(&self, parent: &str) -> Vec<String> {
        let (res, _) = self.lay();
        res.children(&self.page, self.id(parent)).map(|c| self.page.str(self.page.nodes[c.index()].name).to_string()).collect()
    }

    /// `set_screen_class("landscapeSmall")` etc. `None` = Godot's `""`:
    /// a class no fixture here mentions, so no screen word is active.
    fn class(&mut self, class: Option<ScreenClass>) {
        match class {
            Some(ScreenClass::LandscapeSmall) => self.rt.set_screen(&self.page, Orientation::Landscape, ScreenClass::LandscapeSmall),
            Some(c) => self.rt.set_screen(&self.page, Orientation::Portrait, c),
            None => self.rt.set_screen(&self.page, Orientation::Portrait, ScreenClass::PortraitLarge),
        }
    }
}

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() < tol
}

// ------------------------------------------------------------ _seat_tests

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

/// `_seat_tests` 7284-7285 `seat.err-children`: a seat is an empty socket —
/// a child inside one is a load error in Godot.
#[test]
fn seat_refuses_children() {
    assert!(refuses("View v { Row r { seat s { Rectangle x 10x10 } } }"));
}

/// `_seat_tests` 7286-7287 `seat.err-unnamed`: a seat needs a name (a
/// `when` has to address it).
#[test]
fn seat_needs_a_name() {
    assert!(refuses("View v { Row r { seat } }"));
}

/// `_seat_tests` 7288-7289 `seat.err-props`, FLIPPED: in the Rust
/// dialect placement words on a seat size the SOCKET itself (GP18's
/// fixed socket — `seat socket width=200` under a Layers); a non-box
/// word is still refused by the kind table.
#[test]
fn seat_takes_nothing_but_a_name() {
    assert!(!refuses("View v { Row r { seat s width=20 } }"), "a sized socket is GP18's fixed-socket pattern");
    assert!(refuses("View v { Row r { seat s background=#FF0000FF } }"), "looks belong to the occupant");
}

/// `_seat_tests` 7292-7293 `seat.err-top-level`: a seat at the top of the
/// file is refused (Godot: "inside a container"; Rust: G7003 — a file's
/// root is a View or Panel).
#[test]
fn seat_at_top_level_is_refused() {
    let d = build("seat s").diags;
    assert!(d.iter().any(|d| d.code == "G7003"), "{d:#?}");
}

/// `_seat_tests` 7290-7291 `seat.err-free-parent`, FLIPPED: the Rust
/// dialect hosts seats anywhere — a free parent gives the socket its
/// declared (or occupant's) box; GP18's fixed socket sits under a
/// Layers, caption_page's seats under Panels.
#[test]
fn seat_needs_a_container_parent() {
    assert!(!refuses("View v { Layers l { seat s } }"), "free hosts are legal Rust sockets");
}

/// `_seat_tests` 7294-7297 `seat.err-unknown-seat` + `seat.err-not-a-seat`:
/// `x.seat = name` must name a declared seat — an unknown name and a
/// Column are both refused.
#[test]
fn seat_move_targets_a_declared_seat() {
    let not_a_seat = "when screen.landscapeSmall { m.seat = h }\nView v { Row r { Column h { Rectangle m 10x10 }; seat s } }";
    let a = accepted(&[("unknown seat", "when screen.landscapeSmall { m.seat = s }\nView v { Row r { Column h { Rectangle m 10x10 } } }"), ("not a seat", not_a_seat)]);
    assert!(a.is_empty(), "Rust builds: {a:?}");
}

/// `_seat_tests` 7302-7306 `seat.arrangement-untimed-builds`: an
/// arrangement no `transition` rule times may move seats. Same in both.
#[test]
fn untimed_arrangement_may_move_a_seat() {
    let mut fx = Fx::new("arrangement o\nwhen o { m.seat = s }\nView v { Row r halign=left, valign=top, height=20 { Column h { Rectangle m 10x10 }; seat s } }");
    assert_eq!(fx.children_of("h"), vec!["m"]);
    fx.rt.go_id(gogo::ArrangementId::new(0));
    assert_eq!(fx.children_of("s"), vec!["m"], "{:?}", fx.lay().1);
}

/// `_seat_tests` 7307-7317 `seat.err-arrangement-timed`,
/// `seat.err-arrangement-timed-points-both`, `seat.err-arrangement-bare-timed`,
/// `seat.err-arrangement-any-timed`: Godot's G3003 refuses a seat move
/// under a timed arrangement (named, bare and `any` rules alike). FLIP:
/// todo-gogo.md GP25 SHIPPED/DECIDED 2026-08-30 — Rust glides what Godot
/// refused; the G3003/G3004 fences are moot. So every shape builds and
/// the move applies.
#[test]
fn timed_arrangement_seat_move_builds_and_applies() {
    let tail = "when o { m.seat = s }\nView v { Row r halign=left, valign=top, height=20 { Column h { Rectangle m 10x10 }; seat s } }";
    for rule in ["transition base -> o duration=0.1", "transition duration=0.1", "transition any -> base duration=0.1"] {
        let mut fx = Fx::new(&format!("arrangement o\n{rule}\n{tail}"));
        fx.rt.go_id(gogo::ArrangementId::new(0));
        assert_eq!(fx.children_of("s"), vec!["m"], "rule `{rule}`: {:?}", fx.lay().1);
    }
}

/// `_seat_tests` 7343-7346 `seat.err-two-occupants` + `seat.err-dup`:
/// one occupant per seat and one writer per occupant, PER CONDITION.
/// FLIP on `seat.err-cross-axis-occupants` (7320-7322): an arrangement
/// claim and a class claim on one seat are LEGAL here — the flattened
/// placement rule (E2: equal rank → later line in the file wins)
/// settles them, and brush_options.gogo's `previewAbove` seat depends
/// on exactly that ranking over its screen-class seats.
#[test]
fn one_occupant_per_seat() {
    let two = "when screen.landscapeSmall { m.seat = s }\nwhen screen.landscapeSmall { m2.seat = s }\nView v { Row r halign=left, valign=top, height=20 { Column h { Rectangle m 10x10; Rectangle m2 10x10 }; seat s } }";
    let a = accepted(&[
        ("two occupants", two),
        ("dup", "when screen.landscapeSmall { m.seat = s }\nwhen screen.landscapeSmall { m.seat = s }\nView v { Row r { Column h { Rectangle m 10x10 }; seat s } }"),
    ]);
    assert!(a.is_empty(), "Rust builds: {a:?}");
    let cross = "arrangement o\nwhen o { m.seat = s }\nwhen screen.landscapeSmall { m2.seat = s }\nView v { Row r { Column h { Rectangle m 10x10; Rectangle m2 10x10 }; seat s } }";
    assert!(!refuses(cross), "cross-vocabulary claims rank by the E2 later-line rule instead of refusing");
}

/// `_seat_tests` 7326-7331 `seat.two-arrangements-share`: two different
/// arrangements are exclusive, so their claims on one seat are legal.
#[test]
fn two_arrangements_may_share_a_seat() {
    let mut fx = Fx::new("arrangement o\narrangement p\nwhen o { m.seat = s }\nwhen p { m2.seat = s }\nView v { Row r halign=left, valign=top, height=20 { Column h { Rectangle m 10x10; Rectangle m2 10x10 }; seat s } }");
    fx.rt.go_id(gogo::ArrangementId::new(0));
    assert_eq!(fx.children_of("s"), vec!["m"]);
    fx.rt.go_id(gogo::ArrangementId::new(1));
    assert_eq!(fx.children_of("s"), vec!["m2"]);
}

/// `_seat_tests` 7332-7333 `seat.err-node-scope`: a seat move is a
/// view-level response — `when hovered seat=s` inside a node is refused.
#[test]
fn seat_move_is_view_level_only() {
    assert!(refuses("View v { Row r { Column h { Rectangle m 10x10 { when hovered seat=s } }; seat s } }"));
}

/// `_seat_tests` 7337-7338 `seat.err-cycle`: no move into the node's own
/// subtree.
#[test]
fn seat_move_refuses_a_cycle() {
    let src = "when screen.landscapeSmall { m.seat = inner }\nView v { Row r halign=left, valign=top, height=20 { Column m { seat inner } } }";
    assert!(refuses(src));
}

/// `_seat_tests` 7339-7346 `seat.err-home-reserved`, `seat.err-home-dup`
/// (D238): `home` is reserved — no seat may take the name; two writers
/// of one node's seat refuse. FLIP on `seat.err-home-free-home`: the
/// Rust dialect PARKS occupants under free parents (D263,
/// caption_page's panelMain under the View), so a free home is legal.
#[test]
fn home_is_reserved_and_validated() {
    assert!(!refuses("when screen.landscapeSmall { m.seat = home }\nView v { Layers l { Rectangle m 10x10 } }"), "a free home is the Rust parking pattern (D263)");
    let a = accepted(&[
        ("reserved", "View v { Row r { seat home } }"),
        ("dup", "when screen.landscapeSmall { m.seat = home }\nwhen screen.landscapeSmall { m.seat = home }\nView v { Row r { Column h { Rectangle m 10x10 } } }"),
    ]);
    assert!(a.is_empty(), "Rust builds: {a:?}");
}

/// `_seat_tests` 7349-7423 (`seat.build` … `seat.restores-home`): the
/// move/restore lifecycle. Mover rests FIRST in homeCol (index 0, ahead of
/// trail); an empty seat is 0 wide, an occupied one the occupant's 20;
/// the ranking is composed pair > arrangement > class; `= home` sends the
/// mover to its declared spot while the class move still holds; dropping
/// every condition restores parent AND position.
#[test]
fn seat_move_and_restore_lifecycle() {
    let mut fx = Fx::new(SEAT_VIEW);
    assert_eq!(fx.children_of("homeCol"), vec!["mover", "trail"], "seat.home");
    assert_eq!(fx.rect("sideSeat").w, 0.0, "seat.empty-zero: {:?}", fx.rect("sideSeat"));
    fx.class(Some(ScreenClass::LandscapeSmall));
    assert_eq!(fx.children_of("sideSeat"), vec!["mover"], "seat.moves");
    assert_eq!(fx.children_of("homeCol"), vec!["trail"]);
    // Godot pins the seat's combined minimum (20, 20); here the Row's main
    // axis shows it — the cross axis stretches to the Row's 60.
    assert_eq!(fx.rect("sideSeat").w, 20.0, "seat.occupied-min: {:?}", fx.rect("sideSeat"));
    assert_eq!(fx.rect("mover").w, 20.0, "{:?}", fx.rect("mover"));
    // The composed pair outranks the class alone and drops with the arrangement.
    fx.rt.go_id(gogo::ArrangementId::new(0));
    assert_eq!(fx.children_of("pairSeat"), vec!["mover"], "seat.pair-wins");
    fx.rt.go_id(gogo::BASE);
    assert_eq!(fx.children_of("sideSeat"), vec!["mover"], "seat.pair-drops");
    // `= home` (D238): parent AND index while the class move still holds.
    fx.rt.go_id(gogo::ArrangementId::new(1));
    assert_eq!(fx.children_of("homeCol"), vec!["mover", "trail"], "seat.home-target");
    fx.rt.go_id(gogo::BASE);
    assert_eq!(fx.children_of("sideSeat"), vec!["mover"], "seat.home-target-drops");
    // A plain arrangement claim vs the class's sideSeat: equal rank (one
    // condition each), the LATER line wins — `when lifted` sits below
    // `when screen.landscapeSmall` in the fixture...
    fx.rt.go_id(gogo::ArrangementId::new(2));
    assert_eq!(fx.children_of("liftSeat"), vec!["mover"], "seat.later-single-wins");
    // ...its composed claim (two conditions) outranks the plain one...
    fx.class(Some(ScreenClass::PortraitSmall));
    assert_eq!(fx.children_of("pairSeat"), vec!["mover"], "seat.pair-outranks-arrangement");
    // ...with no class at all the plain claim still holds...
    fx.class(None);
    assert_eq!(fx.children_of("liftSeat"), vec!["mover"], "seat.arrangement-no-class");
    // ...and dropping the arrangement sends mover home (parent AND index).
    fx.rt.go_id(gogo::BASE);
    assert_eq!(fx.children_of("homeCol"), vec!["mover", "trail"], "seat.arrangement-drops-home");
    fx.class(Some(ScreenClass::LandscapeSmall));
    assert_eq!(fx.children_of("sideSeat"), vec!["mover"], "seat.class-reapplies-after-arrangement");
    fx.class(None);
    assert_eq!(fx.children_of("homeCol"), vec!["mover", "trail"], "seat.restores-home");
}

/// `_seat_tests` 7424-7434 `seat.late-build-ok` + `seat.late-build-seated`:
/// a view built AFTER the class was named starts seated.
#[test]
fn late_build_starts_seated() {
    let mut fx = Fx::new(SEAT_VIEW);
    fx.class(Some(ScreenClass::LandscapeSmall));
    assert_eq!(fx.children_of("sideSeat"), vec!["mover"], "{:?}", fx.lay().1);
}

// ---------------------------------------------- _arrangement_anchor_tests

const ARRANGEMENT_ANCHOR: &str = "arrangement docked

when docked {
    card.halign = left
    card.width = 40
    card.margin_left = 10
}

View anchorView {
    Layers pane width=200, height=60 {
        Rectangle card margin_left=10, margin_right=10, height=20, color=#FFFFFFFF
    }
}
";

/// `_arrangement_anchor_tests` 7448-7473 `aanchor.build`, `aanchor.base-span`,
/// `aanchor.restores`: the card spans its 10-inset frame (x 10, width 180)
/// in `base`, and gets that span back after an untimed arrangement
/// re-anchored it (the docked pose itself: see `docked_pose_drops_the_base_pins`).
#[test]
fn untimed_arrangement_reanchor_restores_the_span() {
    let mut fx = Fx::new(ARRANGEMENT_ANCHOR);
    let span = fx.rect("card");
    assert!(span.x == 10.0 && span.w == 180.0, "aanchor.base-span: {span:?}");
    fx.rt.go_id(gogo::ArrangementId::new(0));
    fx.rt.go_id(gogo::BASE);
    let back = fx.rect("card");
    assert!(back.x == 10.0 && back.w == 180.0, "aanchor.restores: {back:?}");
}

/// `_arrangement_anchor_tests` 7458-7462 `aanchor.docked`: an untimed
/// arrangement re-anchors the free card to a left-docked 40-wide box at
/// x 10 — in Godot a posed alignment key RE-DECLARES the axis (the
/// declared `margin_right=10` drops; the pose's own margin stands alone).
#[test]
fn docked_pose_drops_the_base_pins() {
    let mut fx = Fx::new(ARRANGEMENT_ANCHOR);
    fx.rt.go_id(gogo::ArrangementId::new(0));
    let docked = fx.rect("card");
    assert!(docked.x == 10.0 && docked.w == 40.0, "aanchor.docked: {docked:?}");
}

// --------------------------------------------------------- _flow_pose_tests

const FLOW_VIEW: &str = "arrangement open

when screen.landscapeSmall {
    lead.width = 60
    mid.width = 80
    btn.width = 70
}
when open screen.landscapeSmall {
    lead.width = 90
    card.halign = right
    card.width = 100
}

View flowView {
    Column halign=left, valign=top, spacing=0 {
        Row track width=300, height=40, spacing=0 {
            Panel lead width=fill, height=fill
            Panel mid width=50, height=fill
            Panel tail width=fill, height=fill
            Button btn width=fill, height=fill {
                Label text=\"x\"
            }
        }
    }
    Layers box width=300, height=100, halign=left, valign=top, margin_top=60 {
        Rectangle card color=#FFFFFFFF, width=fill, height=20
    }
}
";

/// `_flow_pose_tests` 7486-7487 `flow.err-align`: Godot refuses a posed
/// halign on a Column child ("seats its children"). FLIP (GPAR94, round
/// 6, recorded): a stack child's `halign` binds on the CROSS axis in
/// BOTH engines' static form, so the posed form stays meaningful here —
/// it realigns the cross axis. Posed MARGINS on a stack child are
/// refused (nothing for them to bind), matching the node-line G1014.
#[test]
fn posed_alignment_on_a_stack_child_realigns_the_cross_axis() {
    assert!(!refuses("when screen.landscapeSmall { b.halign=right }\nView v { Column c { Rectangle b 10x10 } }"));
}

/// `_flow_pose_tests` 7488-7489 `flow.err-margin`: a `when` may not pose a
/// margin on a stack child — the parent seats its children (D164).
#[test]
fn posed_margin_on_a_stack_child_is_refused() {
    assert!(refuses("arrangement o\nwhen o { b.margin_left=4 }\nView v { Column c { Rectangle b 10x10 } }"));
}

/// `_flow_pose_tests` 7490-7493 `flow.size-builds`: a `when` MAY resize a
/// stack child (D238).
#[test]
fn posed_size_on_a_stack_child_builds() {
    ok("when screen.landscapeSmall { b.width=20 }\nView v { Column c { Rectangle b 10x10 } }");
}

/// `_flow_pose_tests` 7500-7553 (`flow.build` … `flow.restores`): in the
/// 300-wide track mid holds its 50 and the three fills split the rest;
/// the class pose pins lead/mid/btn to 60/80/70 and tail takes the
/// leftover 90 (a posed fill child stops expanding — Godot's
/// `flow.posed-no-expand` is the same fact: lead stays 60); the composed
/// pair re-pins lead to 90 and re-anchors the free card right at 100;
/// dropping the pair then the class restores every number and lead's
/// share.
#[test]
fn poses_resize_stack_children_and_restore() {
    let mut fx = Fx::new(FLOW_VIEW);
    let w = |fx: &Fx, n: &str| fx.rect(n).w;
    let third = 250.0 / 3.0;
    assert!(near(w(&fx, "mid"), 50.0, 0.5) && near(w(&fx, "lead"), third, 1.0) && near(w(&fx, "tail"), third, 1.0), "flow.base: lead={} mid={} tail={}", w(&fx, "lead"), w(&fx, "mid"), w(&fx, "tail"));
    fx.class(Some(ScreenClass::LandscapeSmall));
    assert!(near(w(&fx, "lead"), 60.0, 0.5) && near(w(&fx, "mid"), 80.0, 0.5) && near(w(&fx, "btn"), 70.0, 0.5) && near(w(&fx, "tail"), 90.0, 0.5), "flow.posed: lead={} mid={} btn={} tail={}", w(&fx, "lead"), w(&fx, "mid"), w(&fx, "btn"), w(&fx, "tail"));
    assert!(near(w(&fx, "card"), 300.0, 0.5), "flow.card-unposed: {}", w(&fx, "card"));
    fx.rt.go_id(gogo::ArrangementId::new(0));
    assert!(near(w(&fx, "lead"), 90.0, 0.5) && near(w(&fx, "tail"), 60.0, 0.5), "flow.pair-wins: lead={} tail={}", w(&fx, "lead"), w(&fx, "tail"));
    let card = fx.rect("card");
    assert!(near(card.x, 200.0, 0.5) && near(card.w, 100.0, 0.5), "flow.pair-align: {card:?}");
    fx.rt.go_id(gogo::BASE);
    assert!(near(w(&fx, "lead"), 60.0, 0.5), "flow.pair-drops: {}", w(&fx, "lead"));
    assert!(near(w(&fx, "card"), 300.0, 0.5), "flow.card-restores: {}", w(&fx, "card"));
    fx.class(None);
    assert!(near(w(&fx, "mid"), 50.0, 0.5) && near(w(&fx, "lead"), third, 1.0), "flow.restores: lead={} mid={}", w(&fx, "lead"), w(&fx, "mid"));
}

// --------------------------------------------------------- _pose_size_tests

/// `pose_size_view.gogo` without `floored` (see `declared_floor_stands_under_a_posed_width`
/// for why that node cannot be in a file Rust builds).
const POSE_SIZE_VIEW: &str = "arrangement abc
arrangement xyz
arrangement grow
transition base -> grow duration=0.2
transition grow -> base duration=0.2

when screen.portraitSmall {
    shrinkBtn.width = 50
    railBox.min_height = 50
    railTall.min_height = 50
}

when grow { grower.width = 120 }

View poseSize {
    Button shrinkBtn width=60, halign=left, valign=top { Rectangle i 10x10 }
    Button railBox min_height=60, halign=left, valign=bottom { Rectangle rb 10x20 }
    Button railTall min_height=60, halign=center, valign=bottom { Rectangle rt 10x100 }
    Rectangle grower color=#FFFFFFFF, width=80, height=20, halign=right, valign=top
}
";

/// `_pose_size_tests` 7570-7573 `psize.flow-min-builds`: a stack child may
/// pose `min_*` too.
#[test]
fn posed_min_width_on_a_stack_child_builds() {
    ok("when screen.landscapeSmall { b.min_width=20 }\nView v { Column c { Rectangle b 10x10 } }");
}

/// `_pose_size_tests` 7581-7616 (`psize.build`, `psize.base` minus its
/// `floored` clause, `psize.base-tall-content`, `psize.shrink-stamp`,
/// `psize.shrink-rendered`, `psize.floor-replaced`, `psize.floor-content-wins`,
/// `psize.shrink-restores`): the box the user sees follows a posed size —
/// the 60-wide button renders 50 under the pose; a posed `min_height=50`
/// replaces the declared 60 floor on the small-content button; content
/// (100) still wins over any floor on the tall one; dropping the class
/// restores 60/60.
#[test]
fn posed_sizes_move_the_real_box() {
    let mut fx = Fx::new(POSE_SIZE_VIEW);
    assert!(fx.rect("shrinkBtn").w == 60.0 && fx.rect("railBox").h == 60.0 && fx.rect("grower").w == 80.0, "psize.base: btn={:?} rail={:?} gr={:?}", fx.rect("shrinkBtn"), fx.rect("railBox"), fx.rect("grower"));
    let tall_base = fx.rect("railTall").h;
    assert!(tall_base >= 100.0, "psize.base-tall-content: {tall_base}");
    fx.class(Some(ScreenClass::PortraitSmall));
    assert!(near(fx.rect("shrinkBtn").w, 50.0, 0.5), "psize.shrink-stamp/rendered: {:?}", fx.rect("shrinkBtn"));
    assert_eq!(fx.rect("railBox").h, 50.0, "psize.floor-replaced: {:?}", fx.rect("railBox"));
    assert_eq!(fx.rect("railTall").h, tall_base, "psize.floor-content-wins: {:?}", fx.rect("railTall"));
    fx.class(None);
    assert!(fx.rect("shrinkBtn").w == 60.0 && fx.rect("railBox").h == 60.0, "psize.shrink-restores: btn={:?} rail={:?}", fx.rect("shrinkBtn"), fx.rect("railBox"));
}

/// `_pose_size_tests` 7581 `psize.build` + 7584 `psize.base` (floored
/// clause) + 7620-7635 `psize.floor-under`, `psize.floor-wins`,
/// `psize.floor-restores` (user case 2026-08-10): a DECLARED `min_width=30`
/// stands under any posed width — 40 clears it, 20 loses to it, `base`
/// restores the declared 50 (G1012 retired — GPAR79, user 2026-08-31).
#[test]
fn declared_floor_stands_under_a_posed_width() {
    let mut fx = Fx::new("arrangement abc\narrangement xyz\nwhen abc { floored.width = 40 }\nwhen xyz { floored.width = 20 }\nView v { Layers floored min_width=30, width=50, height=20, halign=left, valign=center }");
    assert_eq!(fx.rect("floored").w, 50.0, "psize.base");
    fx.rt.go_id(gogo::ArrangementId::new(0));
    assert!(near(fx.rect("floored").w, 40.0, 0.5), "psize.floor-under: {:?}", fx.rect("floored"));
    fx.rt.go_id(gogo::ArrangementId::new(1));
    assert!(near(fx.rect("floored").w, 30.0, 0.5), "psize.floor-wins: {:?}", fx.rect("floored"));
    fx.rt.go_id(gogo::BASE);
    assert_eq!(fx.rect("floored").w, 50.0, "psize.floor-restores");
}

/// `_pose_size_tests` 7639-7650 `psize.raise-deferred`, `psize.raise-lands`,
/// `psize.lower-immediate`: Godot pins WHEN the minimum-size stamp moves
/// around the 0.2 s grow tween (raise after it settles, lower up front so
/// the shrink tween has room). The stamp is engine; what the user sees is
/// pinned here: the box starts the grow at 80, lands at 120, and on the
/// way back is mid-flight between the two before landing at 80.
#[test]
fn timed_grow_glides_the_box_both_ways() {
    let c = ok(POSE_SIZE_VIEW);
    let mut page = c.page;
    let g = (page.find("grower").unwrap()).index();
    let mut rt = Runtime::new(&page);
    let frame = |rt: &mut Runtime, page: &mut Page, dt: f32| -> Rect {
        rt.tick(page, dt);
        let res = resolve_animated(page, rt);
        let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        rt.glide(rects)[g]
    };
    assert_eq!(frame(&mut rt, &mut page, 0.0).w, 80.0);
    rt.go_id(gogo::ArrangementId::new(2));
    assert_eq!(frame(&mut rt, &mut page, 0.0).w, 80.0, "psize.raise-deferred: no snap to 120 up front");
    assert_eq!(frame(&mut rt, &mut page, 0.6).w, 120.0, "psize.raise-lands");
    assert!(!rt.animating());
    rt.go_id(gogo::BASE);
    // The shrink starts from the full 120 (the room Godot lowers the stamp
    // for) and is under way a tenth of a second later.
    assert_eq!(frame(&mut rt, &mut page, 0.0).w, 120.0, "psize.lower-immediate: the shrink starts from the full box");
    let mid = frame(&mut rt, &mut page, 0.1);
    assert!(mid.w > 80.0 && mid.w < 120.0, "psize.lower-immediate: the shrink tween is under way: {mid:?}");
    assert_eq!(frame(&mut rt, &mut page, 0.6).w, 80.0);
}

// ------------------------------------------------------------ _reveal_tests

const REVEAL_VIEW: &str = "arrangement open

transition duration=0.2, easing=cubic_out

when open {
    hello.reveal_y = 1
    hello.opacity = 1
}

View screen {
    Panel box padding=5 {
        Column stack {
            Button more text=\"v\", 50x50, on:activate=toggle(open)
            Label hello text=\"Hello, Gogo\", width=fill, reveal_y=0, opacity=0
        }
    }
}
";

/// The height a plain `Hello, Gogo` label takes in a Column (Godot's
/// `nat.y`), measured the same way the reveal pins measure.
fn natural_label_height() -> f32 {
    Fx::new("View v { Column c halign=left, valign=top { Label hello text=\"Hello, Gogo\" } }").rect("hello").h
}

/// `_reveal_tests` 7666-7685 (`reveal.err-range`, `reveal.err-type`,
/// `reveal.err-when-undeclared`, `reveal.err-anim-undeclared`,
/// `reveal.own-scope-builds`, `reveal.anim-validates`): `reveal_x`/
/// `reveal_y` take a fraction 0..1, and a `when`/`anim` may only drive a
/// declared channel. DIVERGENCE from the addon (`reveal.err-root`,
/// `reveal.err-free-parent`; user call 2026-09-05): a reveal needs no
/// container parent — under a free parent (Layers, Panel, the root) the
/// node's OWN box shrinks to the fraction from its alignment edge (a
/// Rectangle fill bar under a Layers needs no wrapper Row), see
/// `reveal_under_a_free_parent_shrinks_the_own_box`.
#[test]
fn reveal_load_rules() {
    let a = accepted(&[
        ("range", "View v { Column { Label a text=\"x\", reveal_y=2 } }"),
        ("type", "View v { Column { Label a text=\"x\", reveal_y=maybe } }"),
        ("when undeclared", "arrangement o\nwhen o { a.reveal_y = 1 }\nView v { Column { Label a text=\"x\" } }"),
        ("anim undeclared", "anim p { a.reveal_y to=1, duration=0.1 }\nView v { Column { Label a text=\"x\" } }"),
    ]);
    let own = build("View v { Column { Button b text=\"x\", reveal_y=1 { when held reveal_y=0.5 } } }").diags;
    let animed = build("anim p { a.reveal_y from=0, to=1, duration=0.1 }\nView v { Column { Label a text=\"x\", reveal_y=0 } }").diags;
    assert!(a.is_empty() && !gogo::has_errors(&own) && !gogo::has_errors(&animed), "Rust builds: {a:?}; own-scope diags {own:#?}; anim diags {animed:#?}");
    let free = build("View v { Layers l width=100, height=100 { Label a text=\"x\", reveal_y=0 } }").diags;
    let root = build("Panel root reveal_y=0 { Label a text=\"x\" }").diags;
    assert!(!gogo::has_errors(&free) && !gogo::has_errors(&root), "a free parent and the root take a reveal: {free:#?} {root:#?}");
}

/// A reveal under a free parent (the user call 2026-09-05, the brush
/// page's slider fill): the node's own box takes the fraction of its
/// stretched width from its left edge — a Rectangle `reveal_x=0.3` in a
/// 100-wide Layers is 30 wide at the Layers' x, siblings untouched.
#[test]
fn reveal_under_a_free_parent_shrinks_the_own_box() {
    let fx = Fx::new("View v { Layers l width=100, height=20 { Rectangle back width=fill, height=fill, color=black; Rectangle fill width=fill, height=fill, color=white, reveal_x=0.3 } }");
    let (_, rects) = fx.lay();
    let l = rects[fx.id("l")];
    let fill = rects[fx.id("fill")];
    let back = rects[fx.id("back")];
    assert!(fill.x == l.x && near(fill.w, 30.0, 0.01), "the fill is 30 of 100 from the left: {fill:?} in {l:?}");
    assert_eq!(back.w, 100.0, "a sibling under the free parent keeps its own grant");
}

/// `_reveal_tests` 7693-7762 (`reveal.build`, `reveal.concealed-min`,
/// `reveal.tween-midway`, `reveal.open`, `reveal.open-min`, `reveal.home`,
/// `reveal.toggle-opens`, `reveal.hidden-zero`): a concealed label earns
/// no height (natural width passes through); the `open` pose tweens the
/// fraction under the 0.2 s rule (midway strictly between, then the full
/// natural height with opacity 1); `base` conceals again; the disclosure
/// button's `toggle(open)` drives the pose; a gone child earns nothing.
#[test]
fn reveal_pose_tweens_the_layout_space() {
    let nat = natural_label_height();
    let c = ok(REVEAL_VIEW);
    let mut page = c.page;
    let hello = page.find("hello").unwrap();
    let more = page.find("more").unwrap();
    let mut rt = Runtime::new(&page);
    let frame = |rt: &mut Runtime, page: &mut Page, dt: f32| -> (Resolved, Vec<Rect>) {
        rt.tick(page, dt);
        let res = resolve_animated(page, rt);
        let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let shown = rt.glide(rects);
        (res, shown)
    };
    let (_, r) = frame(&mut rt, &mut page, 0.0);
    assert!(r[hello.index()].h == 0.0 && r[hello.index()].w > 0.0, "reveal.concealed-min: {:?}", r[hello.index()]);
    rt.go_id(gogo::ArrangementId::new(0));
    let (_, r) = frame(&mut rt, &mut page, 0.05);
    assert!(r[hello.index()].h > 0.0 && r[hello.index()].h < nat, "reveal.tween-midway: {:?}", r[hello.index()]);
    let (res, r) = frame(&mut rt, &mut page, 0.3);
    assert!(near(r[hello.index()].h, nat, 0.01) && res.num(&page, hello, PropKey::Opacity) == Some(1.0), "reveal.open/open-min: {:?} opacity {:?}", r[hello.index()], res.num(&page, hello, PropKey::Opacity));
    rt.go_id(gogo::BASE);
    let (_, r) = frame(&mut rt, &mut page, 0.3);
    assert_eq!(r[hello.index()].h, 0.0, "reveal.home");
    // The disclosure button composes with toggle(): a tap opens.
    let (res, r) = frame(&mut rt, &mut page, 0.0);
    let b = r[more.index()];
    let mut input = Input::new();
    input.touch(&page, &mut rt, &res, &r, &FixedAdvance, Touch { phase: Phase::Down, x: b.x + 5.0, y: b.y + 5.0, t: 1.0, id: 1 });
    let ev = input.touch(&page, &mut rt, &res, &r, &FixedAdvance, Touch { phase: Phase::Up, x: b.x + 5.0, y: b.y + 5.0, t: 1.1, id: 1 });
    assert!(ev.iter().any(|e| e.kind == EventKind::Activate), "{ev:?}");
    assert_eq!(rt.arrangement, gogo::ArrangementId::new(0), "reveal.toggle-opens");
    // A gone child earns nothing.
    rt.set_visible(&mut page, hello, false);
    let (res, _) = frame(&mut rt, &mut page, 0.0);
    assert!(res.gone(&page, hello), "reveal.hidden-zero");
}

/// `_reveal_tests` 7768-7786 (`reveal.stretch-build`, `reveal.stretch-extent`,
/// `reveal.stretch-conceals-x`, `reveal.stretch-half-y`; user-caught D93
/// postmortem): reveal on a STRETCHED axis clips to factor × the node's
/// own extent, not the parent's grant — `reveal_x=0, reveal_y=0.5` shows
/// no width and half the natural height even though the Column stretched
/// the label to its 100. (Godot pins the label's extent at ≥ 99 wide
/// behind a 0-wide window; Rust has no window, so the shown rect is pinned.)
#[test]
fn reveal_on_a_stretched_axis_clips_to_the_extent() {
    let nat = natural_label_height();
    let fx = Fx::new("Panel border_color=red, border_thickness=1, padding=5, corner_radius=9 {
    Column width=100 {
        Label hello text=\"Hello, Gogo\", reveal_x=0, reveal_y=0.5
    }
}");
    let r = fx.rect("hello");
    assert_eq!(r.w, 0.0, "reveal.stretch-conceals-x: {r:?}");
    assert!(near(r.h, nat * 0.5, 0.01), "reveal.stretch-half-y: {r:?} nat={nat}");
}
