//! Port of the Godot gogo suite's min-size / hug / cap groups
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`), batch `minsize`:
//!
//! - `_wrap_seed_tests` 373-426 (4)
//! - `_runaway_growth_tests` 426-490 (5) — ENGINE
//! - `_uncapped_hug_natural_tests` 490-827 (30)
//! - `_max_size_tests` 827-921 (12)
//! - `_live_hug_tests` 921-998 (7)
//! - `_rtl_cap_tests` 998-1046 (4) — ENGINE
//! - `_corner_radius_tests` 1046-1084 (5)
//! - `_min_max_band_tests` 1084-1138 (10)
//! - `_max_fill_cap_tests` 1138-1204 (7)
//! - `_capped_fill_tests` 1204-1304 (14)
//! - `_container_cap_tests` 1304-1372 (8)
//! - `_container_seat_tests` 1372-1431 (7)
//!
//! Most Godot checks there guard the engine's min-size ratchet and its
//! live re-measure; what is ported is the GEOMETRY a hug / capped / fill
//! box lands at, read from `layout()` after the same edits (a text write,
//! a hide, a pose, a frame resize).
//!
//! ENGINE skips (no Rust counterpart):
//! - `_runaway_growth_tests` (5): D274 size-growth watchdog over frames.
//! - `nathug.g1005-*` (3): seat width-feedback ratchet warning.
//! - `nathug.seat-pose-moved`, `nathug.seat-pose-fixed-meta` (2): engine
//!   parent / size-flag / meta stamps.
//! - `_rtl_cap_tests` (4): RichTextLabel raw register.
//! - `seat.align-as-flags`, `seat.no-live-writer`, `seat.units-keeps-seat`
//!   (3): size-flag encoding, re-assert hook meta, units re-apply list.
//!
//! Text is measured with `FixedAdvance` (0.55 em per char, 1.25 em line
//! pitch): at the 15-epx default a char is 8.25 wide and a line 18.75
//! tall; at `font_size=16` 8.8 and 20. Godot measured Roboto, so pixel
//! numbers below are the FixedAdvance ones, relations are kept as written.
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{Len, Page};
use gogo::runtime::{Resolved, Runtime, resolve};
use gogo::{Compiled, Orientation, ScreenClass, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn errs(src: &str) -> bool {
    gogo::has_errors(&compile(src, &|_| None).diags)
}

fn lay(page: &Page, rt: &Runtime, w: f32, h: f32) -> (Resolved, Vec<Rect>) {
    let res = resolve(page, rt);
    let rects = layout(page, &res, Rect::new(0.0, 0.0, w, h), &FixedAdvance);
    (res, rects)
}

fn rect(page: &Page, rects: &[Rect], name: &str) -> Rect {
    rects[page.find(name).unwrap_or_else(|| panic!("no node `{name}`")) as usize]
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

/// True when the draw list clips at exactly `r` (the written-cap clip of
/// GPAR37: the box keeps its number and cuts what pokes past).
fn clips_at(draws: &[Draw], r: Rect) -> bool {
    draws.iter().any(|d| matches!(d, Draw::ClipPush(c) if near(c.x, r.x) && near(c.y, r.y) && near(c.w, r.w) && near(c.h, r.h)))
}

// ---------------------------------------------------------------- seed

/// `_wrap_seed_tests` 373-426 (seed.build-ok, seed.label-one-line,
/// seed.width-seeded, seed.plate-min-sane). Godot seeds a hidden wrap
/// label's width so it never reads one-char-per-line; the geometry that
/// seed protects is: a hug plate under `max_width=fill` holding a wrap
/// label that fits is one line tall, the label as wide as its text.
#[test]
fn hug_plate_under_a_fill_cap_holds_one_line_when_the_text_fits() {
    let c = ok("View v {\n Panel plate halign=center, valign=top, width=hug, height=hug, max_width=fill {\n  Column {\n   Label msg text=\"a sentence long enough to stack many many lines when wrapped at nothing\", font_size=16, wrap=true\n  }\n }\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 800.0, 600.0);
    let msg = rect(&page, &rects, "msg");
    let plate = rect(&page, &rects, "plate");
    // 71 chars × 8.8 = 624.8 fits the 800 frame: exactly one 20-epx line.
    assert!(near(msg.h, 20.0), "one line: {msg:?}");
    assert!(near(msg.w, 624.8), "as wide as its unwrapped text: {msg:?}");
    assert!(plate.h < 60.0 && near(plate.h, 20.0), "plate hugs one line: {plate:?}");
}

// ------------------------------------------------- uncapped hug natural

/// `_uncapped_hug_natural_tests` 490-529 (nathug.build-ok,
/// nathug.single-line): a hug plate with a Layers hop over a Column over a
/// wrap label lands at the unwrapped text width + padding, one line.
#[test]
fn hug_chain_with_a_layers_hop_keeps_one_line() {
    let c = ok("View v {\n Layers host {\n  Panel plate halign=center, valign=top, width=hug, height=hug, padding=10 {\n   Layers inner {\n    Column col {\n     Label msg text=\"one finger pans, two fingers rotate\", font_size=16, wrap=true\n    }\n   }\n  }\n }\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 700.0, 600.0);
    let plate = rect(&page, &rects, "plate");
    let msg = rect(&page, &rects, "msg");
    // 35 chars × 8.8 = 308 + 2 × 10 padding.
    assert!(near(plate.w, 328.0), "{plate:?}");
    assert!(near(msg.h, 20.0), "one line: {msg:?}");
}

/// `_uncapped_hug_natural_tests` 517-519 (nathug.g1004-dead) and 813-816
/// (nathug.container-dead-flavor). FLIP — todo-gogo.md GP17 SHIPPED +
/// REGRESSION FIXED: a wrap label with no width word and no alignment
/// binds like `fill` at its parent's frame even under a hug chain, so
/// there is nothing dead to warn about; Rust's G1004 (gogo-design.md §7
/// start states) fires only on a DECLARED `width=hug`. Godot warned
/// "never wraps" for both shapes. Proof the label is live: longer text
/// folds at the host frame and the plate stops at the frame.
#[test]
fn chain_hug_wrap_label_raises_no_g1004_because_it_binds_at_the_frame() {
    let short = "View v {\n Layers host {\n  Panel plate halign=center, valign=top, width=hug, height=hug, padding=10 {\n   Layers inner { Column col { Label msg text=\"one finger pans, two fingers rotate\", font_size=16, wrap=true } }\n  }\n }\n}\n";
    assert!(!codes(short).contains(&"G1004".to_string()), "{:#?}", compile(short, &|_| None).diags);
    let container = "View v {\n Column chost {\n  Panel cplate width=hug, padding=10 {\n   Label cmsg text=\"a hint line that would have slivered\", font_size=16, wrap=true\n  }\n }\n}\n";
    assert!(!codes(container).contains(&"G1004".to_string()), "{:#?}", compile(container, &|_| None).diags);
    // The label WOULD wrap: 100 chars × 8.8 = 880 in a 700 frame folds to two lines.
    let long = "View v {\n Layers host {\n  Panel plate halign=center, valign=top, width=hug, height=hug, padding=10 {\n   Layers inner { Column col { Label msg text=\"one finger pans, two fingers rotate, three fingers undo, four fingers redo, five fingers reset the view\", font_size=16, wrap=true } }\n  }\n }\n}\n";
    let c = ok(long);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 700.0, 600.0);
    let msg = rect(&page, &rects, "msg");
    let plate = rect(&page, &rects, "plate");
    assert!(near(msg.h, 40.0), "two lines: {msg:?}");
    assert!(plate.w <= 700.0 && plate.w > 600.0, "the plate stops inside the frame: {plate:?}");
}

/// `_uncapped_hug_natural_tests` 531-546, 583-597, 564-575
/// (nathug.seat-hop, nathug.seat-row-width, nathug.seat-hugn-unwrapped,
/// nathug.seat-fill-height). D263: a seat measures its occupant by the
/// occupant's DECLARED width intent — a hug occupant (no width word, or
/// `hug(n)` whatever its share weight) keeps its unwrapped one-line
/// extent, so a Row grants the seat that width (a wrapped label would be
/// narrower); a `width=fill` occupant seated where the socket width is
/// known wraps there and the seat's height counts the folded lines.
#[test]
fn seat_measures_its_occupant_by_declared_width_intent() {
    let c = ok("arrangement seated\nView v {\n Column col halign=left, valign=top, width=100 {\n  Row hugRow height=30 { seat hugSeat }\n  Row hugnRow height=30 { seat hugnSeat }\n  seat fillSeat\n }\n Panel hugOcc { Label hugLbl text=\"a reseated line with room to wrap\", wrap=true }\n Panel hugnOcc width=hug(2) { Label hugnLbl text=\"a reseated line with room to wrap\", wrap=true }\n Panel fillOcc width=fill { Label fillLbl text=\"a reseated line with room to wrap\", wrap=true }\n}\nwhen seated {\n hugOcc.seat = hugSeat\n hugnOcc.seat = hugnSeat\n fillOcc.seat = fillSeat\n}\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.go_id(0);
    let (_, rects) = lay(&page, &rt, 400.0, 400.0);
    // 33 chars × 8.25 = 272.25 unwrapped.
    let unwrapped = 272.25;
    let hug_seat = rect(&page, &rects, "hugSeat");
    assert!(near(hug_seat.w, unwrapped), "hug occupant: seat = unwrapped {hug_seat:?}");
    let hugn_seat = rect(&page, &rects, "hugnSeat");
    assert!(near(hugn_seat.w, unwrapped), "hug(2) occupant is still hug: {hugn_seat:?}");
    // The fill occupant wraps at the 100-wide socket: three lines.
    let fill_seat = rect(&page, &rects, "fillSeat");
    let fill_lbl = rect(&page, &rects, "fillLbl");
    assert!(fill_seat.h > 18.75 * 1.8, "folded lines counted: {fill_seat:?}");
    assert!(near(fill_lbl.h, 3.0 * 18.75), "{fill_lbl:?}");
}

/// `_uncapped_hug_natural_tests` 548-562 (nathug.seat-fill-wraps). Godot
/// measured the seat with NO socket width (detached `seat_min`) and a
/// `width=fill` occupant still reported its WRAPPED minimum (well under
/// half the unwrapped line) — D263: fill has no hug to honor. Rust asked
/// the same way (a Row asking its children along its main axis) gives
/// the fill occupant its full unwrapped line: the seat is 272.25 wide in
/// a 100-wide column.
/// SHIPPED round 4 (2026-08-31, GPAR78): the fill occupant probes at
/// width 0 when no socket width is known — the wrapped minimum.
#[test]
fn a_fill_occupant_in_a_row_seat_reports_wrapped() {
    let c = ok("arrangement seated\nView v {\n Column col halign=left, valign=top, width=100 {\n  Row fillRow height=30 { seat fillSeat }\n }\n Panel fillOcc width=fill { Label fillLbl text=\"a reseated line with room to wrap\", wrap=true }\n}\nwhen seated { fillOcc.seat = fillSeat }\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.go_id(0);
    let (_, rects) = lay(&page, &rt, 400.0, 400.0);
    let fill_seat = rect(&page, &rects, "fillSeat");
    assert!(fill_seat.w < 272.25 * 0.5, "fill occupant should wrap: {fill_seat:?}");
}

/// `_uncapped_hug_natural_tests` 599-614 (nathug.stamp-build,
/// nathug.declared-stamp): the declared width word is recorded per node
/// (Godot stamped it as meta; Rust keeps it in `Placement.width`).
#[test]
fn declared_width_words_are_recorded() {
    let c = ok("View v {\n Row host {\n  Panel a width=fill, height=10\n  Panel b width=hug(2), height=10\n  Panel c width=60, height=10\n }\n}\n");
    let page = c.page;
    let width = |n: &str| page.nodes[page.find(n).unwrap() as usize].placement.width;
    assert_eq!(width("a"), Len::Fill(1.0));
    assert_eq!(width("b"), Len::Hug(2.0));
    assert_eq!(width("c"), Len::Exact(60.0));
}

/// `_uncapped_hug_natural_tests` 637-685 (nathug.seat-pose-build,
/// nathug.seat-posed-width, nathug.seat-pose-restores; the fixture
/// `seat_pose_view.gogo`). A screen pose seats a fill box into a 140-wide
/// strip and pins it to 120: the seat is 120 wide and the wrap text folds
/// there (its unwrapped extent is far past 150). Back in portrait the box
/// is home again at the home column's full width.
#[test]
fn a_posed_exact_width_binds_wrap_while_seated_and_fill_returns_after() {
    let c = ok("when screen.landscapeSmall {\n occ.seat = infoSeat\n occ.width = 120\n}\nView seatPoseView {\n Column halign=left, valign=top, spacing=0 {\n  Row strip width=140, height=200, spacing=0 {\n   seat infoSeat\n  }\n  Column home width=140 {\n   Panel occ width=fill {\n    Label blurb text=\"a seated line long enough to wrap several times over\", wrap=true, font_size=16\n   }\n  }\n }\n}\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.set_screen(&page, Orientation::Landscape, ScreenClass::LandscapeSmall);
    let (_, rects) = lay(&page, &rt, 680.0, 320.0);
    let seat = rect(&page, &rects, "infoSeat");
    let occ = rect(&page, &rects, "occ");
    let blurb = rect(&page, &rects, "blurb");
    assert!(near(seat.w, 120.0), "{seat:?}");
    assert_eq!(occ, seat, "the occupant fills its socket");
    // 51 chars × 8.8 = 448.8 unwrapped (> 150); bound at 120 it folds.
    assert!(blurb.h > 20.0 * 2.5, "wraps under the posed width: {blurb:?}");
    rt.set_screen(&page, Orientation::Portrait, ScreenClass::PortraitMedium);
    let (_, rects) = lay(&page, &rt, 360.0, 740.0);
    let occ = rect(&page, &rects, "occ");
    let seat = rect(&page, &rects, "infoSeat");
    assert!(near(occ.w, 140.0) && near(occ.y, 200.0), "home again at fill width: {occ:?}");
    assert!(near(seat.w, 0.0), "the socket is empty: {seat:?}");
}

/// `_uncapped_hug_natural_tests` 687-690 (nathug.g1004-sliver). FLIP —
/// todo-gogo.md GP17 SHIPPED / gogo-design.md §7 (D254): a label on a
/// Row's main axis with no width word hugs, and a hug wrap label measures
/// UNWRAPPED, so the Row grants it its whole one-line width — the
/// one-char sliver Godot warned about cannot happen, and Rust's G1004 is
/// reserved for a declared `width=hug`.
#[test]
fn a_wrap_label_on_a_rows_main_axis_gets_its_whole_line() {
    let src = "View v {\n Row host halign=left, valign=top {\n  Label s text=\"a long sentence indeed\", wrap=true\n }\n}\n";
    assert!(!codes(src).contains(&"G1004".to_string()), "{:#?}", compile(src, &|_| None).diags);
    let c = ok(src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 400.0, 400.0);
    let s = rect(&page, &rects, "s");
    // 22 chars × 8.25 = 181.5, one line.
    assert!(near(s.w, 181.5) && near(s.h, 18.75), "{s:?}");
}

/// `_uncapped_hug_natural_tests` 692-711 (nathug.floor-silent): a
/// `min_width` floor on a Row child holding a fill wrap label is a real
/// width — no G1004, and the label gets the floor.
#[test]
fn a_floored_button_gives_its_fill_wrap_label_the_floor() {
    let src = "View v {\n Row host halign=left, valign=top {\n  Button b min_width=88 {\n   Panel {\n    Column width=fill {\n     Label cap text=\"rotate 90 degrees today\", wrap=true, width=fill\n    }\n   }\n  }\n }\n}\n";
    assert!(!codes(src).contains(&"G1004".to_string()), "{:#?}", compile(src, &|_| None).diags);
    let c = ok(src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 400.0, 400.0);
    let cap = rect(&page, &rects, "cap");
    assert!(near(cap.w, 88.0), "{cap:?}");
    assert!(near(cap.h, 3.0 * 18.75), "wraps at the floor: {cap:?}");
}

/// `_uncapped_hug_natural_tests` 713-729 (nathug.leftover-sliver). Godot
/// warns G1004 "leftover": `width=fill` on a Row's main axis is only the
/// Row's leftover, and a content-sized chain above (a left-aligned Panel)
/// makes that leftover the label's own raw minimum. Rust says nothing
/// and lays the label out ZERO wide (a fill wrap label claims no width;
/// the hug Row over it measures 0).
#[test]
fn a_fill_wrap_label_under_a_hug_chain_is_warned_about() {
    let src = "View v {\n Column host {\n  Panel plate halign=left {\n   Row {\n    Label cap text=\"a long caption that should wrap somewhere\", wrap=true, width=fill\n   }\n  }\n }\n}\n";
    let c = compile(src, &|_| None);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let cap = rect(&page, &rects, "cap");
    assert!(c.diags.iter().any(|d| d.code == "G1004"), "no G1004; label landed at {cap:?}; diags {:#?}", c.diags);
}

/// `_uncapped_hug_natural_tests` 731-747 (nathug.leftover-bound-silent):
/// the same shape with the chain bound (`width=fill` all the way down)
/// warns nothing and the label spans the host.
#[test]
fn a_bound_fill_chain_gives_the_wrap_label_the_host_width() {
    let src = "View v {\n Column host {\n  Panel plate width=fill {\n   Row width=fill {\n    Label cap text=\"a long caption that should wrap somewhere\", wrap=true, width=fill\n   }\n  }\n }\n}\n";
    assert!(!codes(src).contains(&"G1004".to_string()), "{:#?}", compile(src, &|_| None).diags);
    let c = ok(src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let cap = rect(&page, &rects, "cap");
    assert!(near(cap.w, 360.0), "{cap:?}");
}

/// `_uncapped_hug_natural_tests` 749-762 (nathug.cap-silent): a cap gives
/// wrap a width to bind at — no warning, the plate stops at 200 and the
/// text folds into two lines.
#[test]
fn a_cap_gives_wrap_a_width_to_bind_at() {
    let src = "View v {\n Layers host {\n  Panel plate width=hug, max_width=200, halign=center, valign=top {\n   Label msg text=\"a line long enough to be worth wrapping\", wrap=true\n  }\n }\n}\n";
    assert!(!codes(src).contains(&"G1004".to_string()), "{:#?}", compile(src, &|_| None).diags);
    let c = ok(src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 700.0, 600.0);
    let plate = rect(&page, &rects, "plate");
    let msg = rect(&page, &rects, "msg");
    assert!(near(plate.w, 200.0), "{plate:?}");
    assert!(near(msg.h, 2.0 * 18.75), "two lines: {msg:?}");
}

/// `_uncapped_hug_natural_tests` 764-775 (nathug.bare-silent): a bare
/// hug plate (D71) warns nothing; its wrap label stays one unwrapped line.
#[test]
fn a_bare_hug_plate_is_silent_and_one_line() {
    let src = "View v {\n Layers host {\n  Panel plate width=hug, height=hug {\n   Label msg text=\"the host code sizes this rect\", wrap=true\n  }\n }\n}\n";
    assert!(!codes(src).contains(&"G1004".to_string()), "{:#?}", compile(src, &|_| None).diags);
    let c = ok(src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 700.0, 600.0);
    let plate = rect(&page, &rects, "plate");
    // 29 chars × 8.25 = 239.25, one line.
    assert!(near(plate.w, 239.25) && near(plate.h, 18.75), "{plate:?}");
}

/// `_uncapped_hug_natural_tests` 777-827 (nathug.container-build-ok,
/// nathug.container-hug, nathug.container-floor, nathug.container-cap,
/// nathug.container-live). D254: a hug child of a Column gets its full
/// single-line width; a declared `min_width` floor wins over natural; a
/// D144 cap on a Layers binds (natural up to the cap, wrap past it); a
/// longer text re-widens the hug plate.
#[test]
fn hug_under_a_column_takes_natural_floor_and_cap() {
    let c = ok("View v {\n Column chost {\n  Panel cplate width=hug, padding=10 {\n   Label cmsg text=\"a hint line that would have slivered\", font_size=16, wrap=true\n  }\n  Panel cfloor width=hug, min_width=400, padding=10 {\n   Label flmsg text=\"short\", font_size=16, wrap=true\n  }\n  Layers ccap width=hug, max_width=120, halign=left {\n   Label capmsg text=\"a long enough sentence to overflow one hundred twenty pixels\", font_size=16, wrap=true\n  }\n }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 700.0, 600.0);
    let cplate = rect(&page, &rects, "cplate");
    let cmsg = rect(&page, &rects, "cmsg");
    // 36 chars × 8.8 = 316.8 + 20 padding.
    assert!(near(cplate.w, 336.8) && near(cmsg.h, 20.0), "{cplate:?} {cmsg:?}");
    assert!(near(rect(&page, &rects, "cfloor").w, 400.0), "{:?}", rect(&page, &rects, "cfloor"));
    let ccap = rect(&page, &rects, "ccap");
    let capmsg = rect(&page, &rects, "capmsg");
    assert!(near(ccap.w, 120.0), "{ccap:?}");
    assert!(capmsg.h >= 40.0, "wraps past the cap: {capmsg:?}");
    // Live: a longer text re-widens the plate.
    let cmsg_id = page.find("cmsg").unwrap();
    rt.set_text(&mut page, cmsg_id, "a hint line that would have slivered, now grown even longer");
    let (_, rects) = lay(&page, &rt, 700.0, 600.0);
    let grown = rect(&page, &rects, "cplate");
    // 59 chars × 8.8 = 519.2 + 20.
    assert!(near(grown.w, 539.2) && grown.w > cplate.w + 10.0, "{grown:?}");
}

// ------------------------------------------------------------ max size

/// `_max_size_tests` 836-839 (cap.fixed-refused, cap.positive). Godot
/// refuses `max_width` next to an exact `width` ("exact size") and a
/// non-positive cap ("positive"); Rust builds both.
#[test]
#[ignore = "GP41: not ported: load-time lints (max next to exact width; non-positive cap)"]
fn a_cap_next_to_an_exact_width_or_a_negative_cap_is_refused() {
    assert!(errs("View v { Layers { Panel p width=200, max_width=100 } }"), "cap next to an exact width built");
    assert!(errs("View v { Layers { Panel p width=hug, max_width=-5 } }"), "negative cap built");
}

/// `_max_size_tests` 840-841 (cap.container-refused). Godot v1 refuses a
/// hug cap on a non-Layers child of a container ("free parent"); Rust
/// accepts it and binds it.
#[test]
#[ignore = "GP34: not ported: D144 cap only on Layers/Scroll/List under a container"]
fn a_hug_cap_on_a_panel_under_a_column_is_refused() {
    assert!(errs("View v { Column { Panel p width=hug, max_width=100 } }"));
}

/// `_max_size_tests` 843-905 (cap.build-ok, cap.hug-under, cap.centered,
/// cap.bare-binds, cap.binds, cap.wraps, cap.shrinks-back). D132: `width=
/// hug, max_width=N` hugs the unwrapped text under the cap, centers, a
/// bare capped axis sits flush left; a long text write hits the cap and
/// wraps; a short one shrinks the plate back.
#[test]
fn hug_cap_hugs_under_binds_at_and_shrinks_back() {
    let c = ok("View v {\n Layers host {\n  Panel plate width=hug, max_width=200, halign=center, valign=top, padding=12 {\n   Label msg text=\"hi\", font_size=16, wrap=true\n  }\n  Panel bare width=hug, max_width=90, valign=top, padding=4 {\n   Label barelbl text=\"a line with room to wrap\", font_size=16, wrap=true\n  }\n }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let plate = rect(&page, &rects, "plate");
    // "hi" = 2 × 8.8 = 17.6 + 24 padding.
    let expect = 41.6;
    assert!(near(plate.w, expect) && plate.w < 200.0, "{plate:?}");
    assert!(near(plate.x + plate.w / 2.0, 180.0), "centered: {plate:?}");
    let bare = rect(&page, &rects, "bare");
    assert!(near(bare.w, 90.0) && near(bare.x, 0.0), "bare cap binds flush left: {bare:?}");
    let msg = page.find("msg").unwrap();
    rt.set_text(&mut page, msg, "this toast line is long enough to overflow the two hundred epx cap comfortably");
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let plate = rect(&page, &rects, "plate");
    let msg_r = rect(&page, &rects, "msg");
    assert!(near(plate.w, 200.0), "{plate:?}");
    assert!(msg_r.h > 20.0, "wraps: {msg_r:?}");
    rt.set_text(&mut page, msg, "hi");
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let plate = rect(&page, &rects, "plate");
    assert!(near(plate.w, expect), "shrinks back: {plate:?}");
}

/// `_max_size_tests` 883-891 (cap.yields-to-min, cap.height-yields). FLIP
/// — GPAR37 DECIDED (user: "human means max"): a written `max_*` is hard;
/// Godot let rigid content win (208 / 128), Rust keeps the cap (90 / 50)
/// and clips what pokes past.
#[test]
fn a_written_cap_holds_over_rigid_content_and_clips() {
    let c = ok("View v {\n Layers host {\n  Panel rigid width=hug, max_width=90, valign=top, padding=4 {\n   Rectangle wide 200x20, color=#FFFFFFFF\n  }\n  Panel tall height=hug, max_height=50, valign=top, halign=left, padding=4 {\n   Rectangle high 30x120, color=#FFFFFFFF\n  }\n }\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let (res, rects) = lay(&page, &rt, 360.0, 600.0);
    let rigid = rect(&page, &rects, "rigid");
    let tall = rect(&page, &rects, "tall");
    assert!(near(rigid.w, 90.0), "{rigid:?}");
    assert!(near(tall.h, 50.0), "{tall:?}");
    let draws = draw(&page, &res, &rects);
    assert!(clips_at(&draws, rigid), "{draws:#?}");
    assert!(clips_at(&draws, tall), "{draws:#?}");
}

// ------------------------------------------------------------ live hug

/// `_live_hug_tests` 921-998 (hug.build-ok, hug.baseline, hug.shrinks,
/// hug.grows-back, hug.span-intact, hug.text-shrinks, hug.text-grows-back).
/// D141: an uncapped hug box re-hugs after a hide and after a text write,
/// both ways; the spanning axis (both margins) never moves.
#[test]
fn an_uncapped_hug_box_follows_hides_and_text_both_ways() {
    let c = ok("View v {\n Layers lhost {\n  Panel lpanel margin_left=10, margin_right=10, valign=top {\n   Column spacing=0 {\n    Rectangle lrow1 color=#4488CCFF, height=40\n    Rectangle lrow2 color=#CC8844FF, height=40\n    Rectangle lrow3 color=#44CC88FF, height=40\n   }\n  }\n  Panel ltext halign=left, valign=bottom, padding=4 {\n   Label ltlabel text=\"a much longer declared line\", font_size=16\n  }\n }\n}\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let span = |r: Rect| near(r.x, 10.0) && near(r.w, 340.0);
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let base = rect(&page, &rects, "lpanel");
    assert!(base.h >= 120.0 && near(base.h, 120.0), "{base:?}");
    assert!(span(base), "{base:?}");
    let lrow2 = page.find("lrow2").unwrap();
    rt.set_visible(&mut page, lrow2, false);
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let hidden = rect(&page, &rects, "lpanel");
    assert!(near(hidden.h, base.h - 40.0), "{hidden:?} baseline {base:?}");
    assert!(span(hidden), "{hidden:?}");
    rt.set_visible(&mut page, lrow2, true);
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let back = rect(&page, &rects, "lpanel");
    assert!(near(back.h, base.h) && span(back), "{back:?}");
    // Text on an uncapped hug: shorter shrinks, longer grows back.
    let w0 = rect(&page, &rects, "ltext").w;
    // 27 chars × 8.8 = 237.6 + 8 padding.
    assert!(near(w0, 245.6), "{w0}");
    let ltlabel = page.find("ltlabel").unwrap();
    rt.set_text(&mut page, ltlabel, "hi");
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let short = rect(&page, &rects, "ltext");
    assert!(short.w < w0 - 20.0, "{short:?} w0={w0}");
    rt.set_text(&mut page, ltlabel, "a much longer declared line");
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let grown = rect(&page, &rects, "ltext");
    assert!(near(grown.w, w0), "{grown:?} w0={w0}");
}

// -------------------------------------------------------- corner radius

/// `_corner_radius_tests` 1053-1073 (corner.build-ok, corner.per-key,
/// corner.later-wins, corner.slot-style-path). Godot: four named
/// per-corner keys join the box family, the uniform writes all four, later
/// on the line wins, and a `slot.<name>.<key>` style path reaches a
/// corner. Rust knows only `corner_radius` (the per-corner key is an
/// unknown-property warning and `Draw::Quad` carries one radius).
#[test]
#[ignore = "GP40: not ported: per-corner corner_radius keys and slot.<name>.<prop> style paths"]
fn per_corner_radius_keys_are_known() {
    let src = "View v {\n Column {\n  Panel sheet background=#223344FF, corner_radius=8, corner_radius_top_left=2 { Label a text=\"x\" }\n  Panel stomp corner_radius_top_left=2, corner_radius=8 { Label b text=\"y\" }\n }\n}\n";
    let c = compile(src, &|_| None);
    assert!(!c.diags.iter().any(|d| d.code == "G2001"), "{:#?}", c.diags);
    let slot = "widget Fader {\n Layers {\n  slot track width=fill, height=24 { Rectangle color=#4488FFFF }\n }\n}\nView v { Fader f 200x24, slot.track.corner_radius_top_left=3 }\n";
    let c = compile(slot, &|_| None);
    assert!(!gogo::has_errors(&c.diags) && !c.diags.iter().any(|d| d.code == "G2001"), "{:#?}", c.diags);
}

/// `_corner_radius_tests` 1082-1083 (corner.circle-refused): a Circle
/// refuses the whole box family ("box"); Rust has no per-kind property
/// legality yet.
#[test]
fn a_circle_refuses_corner_keys() {
    assert!(errs("View v { Circle c width=10, height=10, corner_radius=4 }"));
    assert!(errs("View v { Circle c width=10, height=10, corner_radius_top_left=4 }"));
}

// --------------------------------------------------------- min/max band

/// `_aspect_fit_tests` 12209-12219 (aspect.both-spelled, aspect.hug-fence,
/// aspect.cap-fence, aspect.container-fence, aspect.natural-non-image,
/// aspect.bad-value): the six `aspect=` fences, all G1017 (GPAR80,
/// round 4, 2026-08-31).
#[test]
fn aspect_contract_fences() {
    let g1017 = |src: &str| codes(src).contains(&"G1017".to_string());
    assert!(g1017("View v { Layers l { Rectangle r width=100, height=50, aspect=1 } }"), "both spelled");
    assert!(g1017("View v { Layers l { Panel p width=hug, aspect=1 } }"), "hug axis");
    assert!(g1017("View v { Layers l { Panel p width=fill, max_width=100, aspect=1 } }"), "cap combine");
    assert!(g1017("View v { Column c { Rectangle r width=100, aspect=1 } }"), "container parent");
    assert!(g1017("View v { Layers l { Rectangle r aspect=natural } }"), "natural off an Image");
    assert!(g1017("View v { Layers l { Rectangle r aspect=-2 } }"), "negative ratio");
    assert!(!gogo::has_errors(&compile("View v { Layers l { Rectangle r width=200, aspect=2, halign=left, valign=top } }", &|_| None).diags), "the legal one-driven-axis form builds");
}

/// `_aspect_fit_tests` 12233-12262 (aspect.derives-fixed,
/// aspect.derives-fill, aspect.fit-solves, aspect.fit-centered): the
/// spelled axis drives (`width=200, aspect=2` → 200×100; a fill width
/// with 20/20 margins in 400 → 360×90), both `fill` = the largest ratio
/// box that fits, centered (400×300 host, aspect=1 → 300×300 at x=50).
#[test]
fn aspect_derives_the_unspelled_axis() {
    let c = ok("View v { Layers ahost {\n Rectangle fixed_drive width=200, aspect=2, halign=left, valign=top\n Rectangle fill_drive width=fill, aspect=4, margin_left=20, margin_right=20, valign=top\n Rectangle fitbox width=fill, height=fill, aspect=1\n} }\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 400.0, 300.0);
    let fd = rect(&page, &rects, "fixed_drive");
    assert_eq!((fd.w, fd.h), (200.0, 100.0), "{fd:?}");
    let fl = rect(&page, &rects, "fill_drive");
    assert_eq!((fl.w, fl.h), (360.0, 90.0), "{fl:?}");
    let fb = rect(&page, &rects, "fitbox");
    assert_eq!((fb.w, fb.h), (300.0, 300.0), "{fb:?}");
    assert_eq!(fb.x, 50.0, "the fit box centers: {fb:?}");
}

/// GPAR41 (round 4, 2026-08-31): `border_thickness` insets content in
/// LAYOUT on every kind, the same value draw uses for its text inset —
/// the two passes used to disagree by the border on ListItem/Slot.
#[test]
fn border_insets_content_in_layout_on_every_kind() {
    // (The fixture wears the full legal List shape — empty seat + one
    // repeat — since the round-6 structure fences; one live row.)
    let c = ok("View v { List l halign=left, valign=top, width=200 {\n empty { }\n repeat t capacity=1 { ListItem li border_thickness=3, padding=5 { Rectangle r min_size=10x10 } }\n} }\n");
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let rep = page.find("t").unwrap();
    rt.set_count(&mut page, rep, 1);
    let (_, rects) = lay(&page, &rt, 400.0, 400.0);
    let li = rect(&page, &rects, "t/0/li");
    let r = rect(&page, &rects, "t/0/r");
    assert_eq!(r.x - li.x, 8.0, "border 3 + padding 5: li={li:?} r={r:?}");
    assert_eq!(r.y - li.y, 8.0, "li={li:?} r={r:?}");
}

/// GPAR38 (round 4, 2026-08-31): both margins on an ALIGNED capped axis
/// inset the measuring frame once — `halign=left, margin_left=12,
/// margin_right=12, max_width=fill` sits at x=12 (it used to sit at 24,
/// the margin counted in the frame AND again by the align arm); the right
/// twin ends 12 in from the edge; the addon's `cap.fill-margin-shift`
/// asymmetric-center case keeps its (near-far)/2 midpoint shift.
#[test]
fn aligned_capped_margins_count_once() {
    let src = "View v { Layers l {\n Panel a width=hug, max_width=fill, halign=left, valign=top, margin_left=12, margin_right=12 { Rectangle ra min_size=40x10 }\n Panel b width=hug, max_width=fill, halign=right, valign=top, margin_left=12, margin_right=12 { Rectangle rb min_size=40x10 }\n Panel c width=hug, max_width=fill, halign=center, valign=top, margin_left=30, margin_right=10 { Rectangle rc min_size=40x10 }\n} }\n";
    let c = ok(src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 400.0, 400.0);
    assert_eq!(rect(&page, &rects, "a").x, 12.0, "{:?}", rect(&page, &rects, "a"));
    assert_eq!(rect(&page, &rects, "b").right(), 388.0, "{:?}", rect(&page, &rects, "b"));
    let c = rect(&page, &rects, "c");
    assert!(near(c.x + c.w / 2.0, 210.0), "midpoint shifts by (30-10)/2: {c:?}");
}

/// `_min_max_band_tests` 1091-1096 (band.err-crossed, band.err-fixed-min,
/// band.err-fixed-min-size): a floor above the cap refuses at build
/// (G1013). FLIP: a floor next to an exact size is LEGAL on any axis and
/// the floor still wins over a smaller written size — it exists to bound
/// POSED sizes, so it is not dead (GPAR79, user 2026-08-31; G1012
/// retired; Godot refuses both fixture lines).
#[test]
fn crossed_band_refuses_and_a_floor_beside_an_exact_size_builds_and_clamps() {
    assert!(codes("View v { Layers { Panel p width=hug, min_width=300, max_width=100 } }").contains(&"G1013".to_string()));
    let c = ok("View v { Layers { Panel p size=100x40, min_height=50, halign=left, valign=top } }");
    let rt = Runtime::new(&c.page);
    let (_res, rects) = lay(&c.page, &rt, 400.0, 800.0);
    let p = c.page.find("p").unwrap() as usize;
    assert_eq!((rects[p].w, rects[p].h), (100.0, 50.0), "the floor beats the smaller written height");
    let c = ok("View v { Column { Panel q width=100, height=40, min_width=120 } }");
    let rt = Runtime::new(&c.page);
    let (_res, rects) = lay(&c.page, &rt, 400.0, 800.0);
    let q = c.page.find("q").unwrap() as usize;
    assert_eq!(rects[q].w, 120.0, "the clamp holds on a stack child too");
}

/// `_min_max_band_tests` 1097-1098 (band.err-aspect-min): a floor on the
/// axis `aspect=` derives "would break the ratio" in Godot; Rust builds it.
#[test]
#[ignore = "GP41: not ported: load-time lint for min_* on an aspect-derived axis"]
fn a_floor_on_an_aspect_derived_axis_refuses() {
    assert!(errs("View v { Layers { Panel p width=fill, aspect=1.5, min_height=10 } }"));
}

/// `_min_max_band_tests` 1100-1137 (band.build-ok, band.floor,
/// band.natural, band.cap, band.cap-wraps). D135: `width=hug, min_width=A,
/// max_width=B` clamps the natural size into [A, B]; wrap text under the
/// cap folds.
#[test]
fn the_min_max_band_clamps_natural_size() {
    let c = ok(BAND_SRC);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 600.0, 600.0);
    assert!(near(rect(&page, &rects, "floorp").w, 100.0), "{:?}", rect(&page, &rects, "floorp"));
    assert!(near(rect(&page, &rects, "midp").w, 108.0), "{:?}", rect(&page, &rects, "midp"));
    let capp = rect(&page, &rects, "capp");
    assert!(near(capp.w, 120.0), "{capp:?}");
    let caplbl = rect(&page, &rects, "caplbl");
    assert!(caplbl.h > 20.0, "wraps under the cap: {caplbl:?}");
}

const BAND_SRC: &str = "View v {\n Layers bhost {\n  Panel floorp width=hug, min_width=100, max_width=300, valign=top, padding=4 {\n   Rectangle small 20x10, color=#FFFFFFFF\n  }\n  Panel midp width=hug, min_width=50, max_width=300, valign=top, padding=4 {\n   Rectangle mid 100x10, color=#FFFFFFFF\n  }\n  Panel capp width=hug, max_width=120, min_width=50, valign=top, padding=4 {\n   Label caplbl text=\"a line with plenty of room to wrap\", font_size=16, wrap=true\n  }\n  Panel rigidp width=hug, min_width=50, max_width=120, valign=top, padding=4 {\n   Rectangle big 400x10, color=#FFFFFFFF\n  }\n  Panel fillf width=fill, min_width=200, valign=bottom, height=20\n }\n}\n";

/// `_min_max_band_tests` 1119-1123 (band.offsets-truthful). FLIP — GPAR37
/// DECIDED: Godot let the 400-wide rigid child beat the 120 cap (408);
/// Rust keeps the written cap (the floor 50 sits under it) and clips.
#[test]
fn in_the_band_the_written_cap_beats_rigid_content() {
    let c = ok(BAND_SRC);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (res, rects) = lay(&page, &rt, 600.0, 600.0);
    let rigidp = rect(&page, &rects, "rigidp");
    assert!(near(rigidp.w, 120.0), "{rigidp:?}");
    assert!(clips_at(&draw(&page, &res, &rects), rigidp));
}

// ------------------------------------------------------- max-*=fill cap

const FILL_CAP_SRC: &str = "View v {\n Layers fhost {\n  Panel fplate width=hug, max_width=fill, halign=center, valign=top, margin_left=12, margin_right=12, padding=12 {\n   Label fmsg text=\"hi\", font_size=16, wrap=true\n  }\n  Panel skew width=hug, max_width=fill, halign=center, valign=top, margin_left=30, margin_right=10, padding=4 {\n   Label skmsg text=\"hi\", font_size=16, wrap=true\n  }\n  Panel vfill height=hug, max_height=fill, halign=left, valign=top, padding=4 {\n   Rectangle high2 30x120, color=#FFFFFFFF\n  }\n }\n}\n";

/// `_max_fill_cap_tests` 1143-1167, 1187-1196 (cap.fill-ok,
/// cap.fill-under, cap.fill-margin-shift, cap.fill-height-inert). D132:
/// `max_width=fill` caps a hug at what a fill axis would get (frame minus
/// margins): short text hugs under it, asymmetric margins shift the
/// centering midpoint, and the height twin stays inert above content
/// that fits.
#[test]
fn a_fill_cap_hugs_under_and_margins_shift_the_midpoint() {
    let c = ok(FILL_CAP_SRC);
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let fplate = rect(&page, &rects, "fplate");
    assert!(near(fplate.w, 41.6) && fplate.w < 336.0, "{fplate:?}");
    let skew = rect(&page, &rects, "skew");
    // The centering frame is 30..350: midpoint 190.
    assert!(near(skew.x + skew.w / 2.0, 190.0), "{skew:?}");
    assert!(near(rect(&page, &rects, "vfill").h, 128.0), "{:?}", rect(&page, &rects, "vfill"));
}

/// `_max_fill_cap_tests` 1169-1180 (cap.fill-binds, cap.fill-resize).
/// Godot: a long text write makes the plate bind at frame − margins (336
/// in a 360 frame, 276 after a resize to 300) — the plate's natural width
/// is the label's UNWRAPPED extent, capped, and the text wraps inside.
/// Rust measures the stretch label WRAPPED at the frame first, so the
/// plate hugs the widest wrapped line + padding (314.4) and lands under
/// the cap; at the 300 frame the unwritten ancestors then grow to hold
/// plate + margins (the GPAR40 content floor counts margins), the plate
/// is re-measured in that wider frame and stays 314.4 — wider than
/// frame − margins — and the root View pokes past the viewport.
#[test]
fn a_fill_cap_binds_at_the_live_frame_when_the_text_is_long() {
    let c = ok(FILL_CAP_SRC);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let fmsg = page.find("fmsg").unwrap();
    rt.set_text(&mut page, fmsg, "this toast line is long enough to overflow the live frame width comfortably and then some");
    let (_, rects) = lay(&page, &rt, 360.0, 600.0);
    let at360 = rect(&page, &rects, "fplate");
    let (_, rects) = lay(&page, &rt, 300.0, 600.0);
    let at300 = rect(&page, &rects, "fplate");
    let root300 = rects[0];
    assert!(near(at360.w, 336.0), "binds at 360 - 24: {at360:?}; at 300: {at300:?} root {root300:?}");
    assert!(near(at300.w, 276.0), "re-caps at 300 - 24: {at300:?} root {root300:?}");
}

/// `_max_fill_cap_tests` 1201-1203 (cap.center-margin-still-fenced): a
/// single margin on a centered axis with no fill cap "pins no edge" in
/// Godot; Rust builds it.
#[test]
#[ignore = "GP41: not ported: load-time lint for a lone margin on a centered axis without a cap"]
fn a_lone_margin_on_a_centered_axis_refuses() {
    assert!(errs("View v { Layers { Panel p halign=center, margin_left=12 } }"));
}

// ---------------------------------------------------------- capped fill

/// `_capped_fill_tests` 1215-1216 (fcap.cap-fill-pointless): `width=fill,
/// max_width=fill, halign=center` changes nothing — refused (Rust G1011).
#[test]
fn a_fill_capped_at_fill_refuses() {
    assert!(codes("View v { Layers { Rectangle p width=fill, max_width=fill, halign=center, color=red } }").contains(&"G1011".to_string()));
}

/// `_capped_fill_tests` 1212-1214, 1221-1223 (fcap.needs-side,
/// fcap.scroll-refused): Godot demands a stated side on a capped fill and
/// refuses the cap on a Scroll; Rust builds both (a side-less capped fill
/// centers).
#[test]
#[ignore = "GP41: not ported: load-time lints (capped fill needs a stated side; no capped fill on Scroll)"]
fn a_capped_fill_needs_a_side_and_no_scroll() {
    assert!(errs("View v { Layers { Rectangle p width=fill, max_width=100, color=red } }"), "side-less capped fill built");
    assert!(errs("View v { Layers { Scroll s width=fill, max_width=100, halign=center, height=100 { Column c { Rectangle r 40x40, color=red } } } }"), "capped Scroll built");
}

/// `_capped_fill_tests` 1217-1220 (fcap.container-refused): a capped fill
/// under a Column is refused in Godot ("Layers"); Rust accepts it.
#[test]
#[ignore = "GP34: not ported: D144 cap only on Layers/Scroll/List under a container"]
fn a_capped_fill_under_a_column_is_refused() {
    assert!(errs("View v { Column { Rectangle p width=fill, max_width=100, halign=center, color=red } }"));
}

/// `_capped_fill_tests` 1225-1278 (fcap.build-ok, fcap.fills-narrow,
/// fcap.margins-narrow, fcap.binds-center, fcap.binds-left,
/// fcap.margins-shift, fcap.height, fcap.fills-again). D245: `width=fill,
/// max_width=N, halign=<side>` fills edge to edge on a frame narrower than
/// N; when the cap binds the stated side places the box; margins inset
/// the frame and shift the centering midpoint; the height axis runs the
/// same math; a narrower frame again fills again.
#[test]
fn a_capped_fill_fills_narrow_and_sides_when_the_cap_binds() {
    let c = ok("View v {\n Layers cfHost {\n  Rectangle band width=fill, max_width=200, halign=center, valign=top, height=40, color=#333333FF\n  Rectangle lband width=fill, max_width=200, halign=left, valign=top, margin_top=60, height=40, color=#444444FF\n  Rectangle mband width=fill, max_width=200, halign=center, valign=top, margin_top=120, margin_left=20, margin_right=40, height=40, color=#555555FF\n  Rectangle vband height=fill, max_height=50, valign=top, halign=left, width=40, color=#666666FF\n }\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 150.0, 600.0);
    let band = rect(&page, &rects, "band");
    assert!(near(band.w, 150.0) && near(band.x, 0.0), "{band:?}");
    let mband = rect(&page, &rects, "mband");
    assert!(near(mband.w, 90.0) && near(mband.x, 20.0), "{mband:?}");
    let (_, rects) = lay(&page, &rt, 600.0, 600.0);
    let band = rect(&page, &rects, "band");
    assert!(near(band.w, 200.0) && near(band.x, 200.0), "{band:?}");
    let lband = rect(&page, &rects, "lband");
    assert!(near(lband.w, 200.0) && near(lband.x, 0.0) && near(lband.y, 60.0), "{lband:?}");
    let mband = rect(&page, &rects, "mband");
    assert!(near(mband.w, 200.0) && near(mband.x, 190.0), "{mband:?}");
    assert!(near(rect(&page, &rects, "vband").h, 50.0), "{:?}", rect(&page, &rects, "vband"));
    let (_, rects) = lay(&page, &rt, 150.0, 600.0);
    assert!(near(rect(&page, &rects, "band").w, 150.0), "{:?}", rect(&page, &rects, "band"));
}

/// `_capped_fill_tests` 1283-1303 (fcap.floor-ok, fcap.yields-to-min).
/// FLIP — GPAR37 DECIDED: Godot let the 180-wide child beat the 100 cap;
/// Rust keeps the written cap and clips.
#[test]
fn a_capped_fill_keeps_its_cap_over_rigid_content() {
    let c = ok("View v {\n Layers f2Host {\n  Panel floored width=fill, max_width=100, halign=center, valign=top {\n   Rectangle wide2 180x20, color=#FFFFFFFF\n  }\n }\n}\n");
    let page = c.page;
    let rt = Runtime::new(&page);
    let (res, rects) = lay(&page, &rt, 600.0, 200.0);
    let floored = rect(&page, &rects, "floored");
    assert!(near(floored.w, 100.0), "{floored:?}");
    assert!(clips_at(&draw(&page, &res, &rects), floored));
}

// -------------------------------------------------------- container cap

const CCAP_SRC: &str = "arrangement mid\narrangement big\nView v {\n Column ccHost spacing=0, halign=left, valign=top {\n  Layers ccRow height=hug, min_height=30, max_height=60 {\n   Rectangle ccContent 40x20\n  }\n  Rectangle ccBelow 40x10\n }\n}\nwhen mid { ccContent.height = 45 }\nwhen big { ccContent.height = 100 }\n";

/// `_container_cap_tests` 1312-1342, 1354-1358 (ccap.ok, ccap.floor,
/// ccap.follows, ccap.cap-binds, ccap.shrinks-back). D144: a `Layers
/// height=hug, min_height=30, max_height=60` under a Column takes the
/// floor under small content, follows content between floor and cap,
/// clamps at the cap, and comes back down — the row below moves with it.
#[test]
fn a_capped_layers_under_a_column_follows_content_between_floor_and_cap() {
    let c = ok(CCAP_SRC);
    let page = c.page;
    let mut rt = Runtime::new(&page);
    let row_and_below = |rt: &Runtime| {
        let (_, rects) = lay(&page, rt, 40.0, 120.0);
        (rect(&page, &rects, "ccRow").h, rect(&page, &rects, "ccBelow").y)
    };
    let (h, y) = row_and_below(&rt);
    assert!(near(h, 30.0) && near(y, 30.0), "floor: row={h} below={y}");
    rt.go_id(0);
    let (h, y) = row_and_below(&rt);
    assert!(near(h, 45.0) && near(y, 45.0), "follows: row={h} below={y}");
    rt.go_id(1);
    let (h, y) = row_and_below(&rt);
    assert!(near(h, 60.0) && near(y, 60.0), "cap binds: row={h} below={y}");
    rt.go_id(u32::MAX);
    let (h, y) = row_and_below(&rt);
    assert!(near(h, 30.0) && near(y, 30.0), "shrinks back: row={h} below={y}");
}

/// `_container_cap_tests` 1344-1352 (ccap.overflow-draws). FLIP — GPAR37
/// DECIDED: the content past the cap still measures its full 100 (the
/// row's rect stays 60), but where Godot let it overdraw past the box
/// (the WPF tuck, no clip) Rust clips at the written cap.
#[test]
fn content_past_a_container_cap_is_clipped_not_overdrawn() {
    let c = ok(CCAP_SRC);
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.go_id(1);
    let (res, rects) = lay(&page, &rt, 40.0, 120.0);
    let row = rect(&page, &rects, "ccRow");
    let content = rect(&page, &rects, "ccContent");
    assert!(content.h > row.h + 20.0, "content={content:?} row={row:?}");
    assert!(clips_at(&draw(&page, &res, &rects), row), "{:#?}", draw(&page, &res, &rects));
}

/// `_container_cap_tests` 1363-1371 (ccap.err-non-layers,
/// ccap.err-fill-cap): under a container only a Layers may carry a hug
/// cap, and `max_*=fill` has no frame there — both refuse in Godot; Rust
/// builds both.
#[test]
#[ignore = "GP34: not ported: D144 cap only on Layers/Scroll/List under a container; no fill cap there"]
fn container_cap_shapes_that_godot_refuses() {
    assert!(errs("View v { Column { Panel p height=hug, max_height=60 { Label text=\"x\", font_size=12 } } }"), "non-Layers cap built");
    assert!(errs("View v { Column { Layers l height=hug, max_height=fill { Rectangle q 10x10 } } }"), "fill cap under a Column built");
}

// ------------------------------------------------------- container seat

/// `_container_seat_tests` 1386-1400, 1408-1412 (seat.ok,
/// seat.column-stacks, seat.min-change-keeps-seat). D164: the dots-button
/// shape — a 50×90 Button, a Column with a 43 gap and a 16-wide centered
/// glyph — puts the glyph at (17, 43), and a later change to the glyph's
/// own minimum (here a posed height) leaves it seated there.
#[test]
fn a_seated_glyph_stays_put_when_its_minimum_changes() {
    let c = ok("arrangement grown\nView v {\n Button seatHost 50x90 {\n  Column spacing=0 {\n   gap 43\n   Rectangle seatGlyph width=16, halign=center\n  }\n }\n}\nwhen grown { seatGlyph.height = 4 }\n");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    let (_, rects) = lay(&page, &rt, 200.0, 200.0);
    let glyph = rect(&page, &rects, "seatGlyph");
    assert!(near(glyph.x, 17.0) && near(glyph.y, 43.0), "{glyph:?}");
    rt.go_id(0);
    let (_, rects) = lay(&page, &rt, 200.0, 200.0);
    let glyph = rect(&page, &rects, "seatGlyph");
    assert!(near(glyph.x, 17.0) && near(glyph.y, 43.0) && near(glyph.h, 4.0), "{glyph:?}");
}

/// `_container_seat_tests` 1422-1430 (seat.flip-refused): a `when` block
/// re-pinning a container child with a margin is refused at load ("seats
/// its children"). Rust refuses both spellings as G1014 — the node line
/// and the posed form (the posed half landed with GPAR94, round 6).
#[test]
fn a_when_block_cannot_re_pin_a_container_child() {
    assert!(errs("arrangement alt\nView v {\n Column {\n  Button b 40x40\n }\n}\nwhen alt {\n b.margin_left = 6\n}\n"));
}
