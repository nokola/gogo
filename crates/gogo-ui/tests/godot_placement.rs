//! Port of the Godot gogo suite's core layout pins
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//! - `_placement_tests` lines 1431-1791 (78 checks): the sizing triad,
//!   halign/valign, margins, the free-fill default (D71), styles and
//!   widget roots carrying placement, theme font size, Layers, the guard
//!   errors, root margins and the root plate.
//! - `_free_padding_tests` lines 1791-2045 (44 checks): padding on free
//!   parents (Layers, the View root), `padding=safe`, padding on Label /
//!   Input / Button, and the refusals.
//!
//! Godot snippets are translated to the Rust dialect (a `View` root where
//! Godot built a bare `Column`/`Layers`/`Control`, `wrap=true`,
//! `Control` → `Layers`, `Layers` root → `View`). Where a Godot check read
//! an engine MINIMUM (`custom_minimum_size`, `get_minimum_size`), the Rust
//! pin hugs the parent (`halign=left, valign=top`) so the minimum shows up
//! as a laid-out size.
//!
//! ENGINE (not ported, no Rust counterpart):
//! - `layers.script` (1701): script-backed GogoLayers host.
//! - `place.root-plate-class`, `place.root-plate-wired` (1773-1775): meta
//!   flags and the plate hook wiring.
//! - `fpad.label-reads-normal` (1935): Label stylebox slot read.
//! - `fpad.input-slots`, `fpad.input-readonly-pigment-kept`,
//!   `fpad.input-bg-still-themed` (1960-1969): per-state LineEdit
//!   stylebox copies.
//! - `fpad.button-bg-still-themed`, `fpad.button-slot-parity`
//!   (1996-2001): theme stylebox ownership, `hover_pressed` slot.
use gogo::draw::{Draw, draw, hit};
use gogo::layout::{DEFAULT_FONT_SIZE, FixedAdvance, Rect, layout};
use gogo::page::{Align, Kind, Len, Rgba};
use gogo::runtime::{HELD, Resolved, Runtime, resolve};
use gogo::{Compiled, NodeId, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn refused(src: &str) -> bool {
    gogo::has_errors(&compile(src, &|_| None).diags)
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{} {}\n", d.code, d.message)).collect()
}

/// A compiled page laid out once in a `w`×`h` viewport.
struct Built {
    page: Page,
    rt: Runtime,
    res: Resolved,
    rects: Vec<Rect>,
    w: f32,
    h: f32,
}

fn build(src: &str, w: f32, h: f32) -> Built {
    let c = ok(src);
    let page = c.page;
    let rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, w, h), &FixedAdvance);
    Built { page, rt, res, rects, w, h }
}

impl Built {
    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }
    fn rect(&self, name: &str) -> Rect {
        self.rects[self.id(name) as usize]
    }
    fn placement(&self, name: &str) -> gogo::page::Placement {
        self.res.placement[self.id(name) as usize]
    }
    /// Lays the page out again after a runtime change (a state, a pose).
    fn relayout(&mut self) {
        self.res = resolve(&self.page, &self.rt);
        self.rects = layout(&self.page, &self.res, Rect::new(0.0, 0.0, self.w, self.h), &FixedAdvance);
    }
    fn draws(&self) -> Vec<Draw> {
        draw(&self.page, &self.res, &self.rects)
    }
    /// The rect a node's text is drawn into (`None` = no text drawn).
    fn text_rect(&self, word: &str) -> Option<Rect> {
        self.draws().iter().find_map(|d| match d {
            Draw::Text { rect, text, .. } if self.page.str(*text) == word => Some(*rect),
            _ => None,
        })
    }
    /// The plate a node draws: the first Quad exactly over its rect.
    fn plate(&self, name: &str) -> Option<(Rgba, f32)> {
        let own = self.rect(name);
        self.draws().iter().find_map(|d| match d {
            Draw::Quad { rect, color, corner, .. } if *rect == own => Some((*color, *corner)),
            _ => None,
        })
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

// ------------------------------------------------------------ _placement_tests

/// `_placement_tests` 1433-1470 (`place.box-ok` … `place.hug-weight`
/// minus the weight): the placement words a Column child writes land as
/// the triad and alignment — `halign=left/center/right`, `valign=…`,
/// `width=hug`, `width=120`, `width=fill`, `width=fill(2.5)`,
/// `halign=center, width=hug(2)`; then the same words seen as rects.
#[test]
fn column_children_encode_alignment_and_the_triad() {
    let b = build(
        "View v {\n Column box {\n  Button l halign=left\n  Button c halign=center\n  Button r halign=right\n  Button t valign=top\n  Button m valign=center\n  Button b valign=bottom\n  Button hg width=hug\n  Button fx width=120\n  Button fl width=fill\n  Button f2 width=fill(2.5)\n  Button h2 halign=center, width=hug(2)\n }\n}\n",
        300.0,
        600.0,
    );
    assert_eq!(b.placement("l").halign, Align::Start);
    assert_eq!(b.placement("c").halign, Align::Center);
    assert_eq!(b.placement("r").halign, Align::End);
    assert_eq!(b.placement("t").valign, Align::Start);
    assert_eq!(b.placement("m").valign, Align::Center);
    assert_eq!(b.placement("b").valign, Align::End);
    assert_eq!(b.placement("hg").width, Len::Hug(1.0));
    assert_eq!(b.placement("fx").width, Len::Exact(120.0));
    assert_eq!(b.placement("fl").width, Len::Fill(1.0));
    assert_eq!(b.placement("f2").width, Len::Fill(2.5));
    let h2 = b.placement("h2");
    assert_eq!((h2.halign, h2.width), (Align::Center, Len::Hug(2.0)));
    // The same words as geometry: a 40-wide button at left / center / right
    // of a 300 column, an exact 120, a fill spanning the column.
    let b = build(
        "View v {\n Column box {\n  Button l halign=left, width=40\n  Button c halign=center, width=40\n  Button r halign=right, width=40\n  Button fx width=120\n  Button fl width=fill\n }\n}\n",
        300.0,
        600.0,
    );
    assert_eq!((b.rect("l").x, b.rect("l").w), (0.0, 40.0), "{:?}", b.rect("l"));
    assert_eq!((b.rect("c").x, b.rect("c").w), (130.0, 40.0), "{:?}", b.rect("c"));
    assert_eq!((b.rect("r").x, b.rect("r").w), (260.0, 40.0), "{:?}", b.rect("r"));
    assert_eq!((b.rect("fx").x, b.rect("fx").w), (0.0, 120.0), "{:?}", b.rect("fx"));
    assert_eq!((b.rect("fl").x, b.rect("fl").w), (0.0, 300.0), "{:?}", b.rect("fl"));
}

/// `_placement_tests` 1467-1469 (`place.hug-weight`): `width=hug(2)` in a
/// Row is a weighted SLOT (it shares the leftover like `fill(2)`) holding
/// a box that hugs its content, centered in that slot. Godot: 300-wide
/// Row, `fill` + `hug(2)` → the hug slot is 200 wide, its 40-wide box
/// sits at x=180.
#[test]
#[ignore = "GP34: not ported: `hug(n)` weighted natural slot — Rust treats hug(2) as a fixed content-sized slot"]
fn hug_weight_shares_the_leftover_and_centers_its_box() {
    let b = build("View v {\n Row row height=20 {\n  Rectangle a width=fill\n  Panel b width=hug(2), halign=center { Rectangle inner 40x10 }\n }\n}\n", 300.0, 100.0);
    assert_eq!(b.rect("a").w, 100.0, "{:?}", b.rect("a"));
    assert_eq!((b.rect("b").x, b.rect("b").w), (180.0, 40.0), "{:?}", b.rect("b"));
}

/// `_placement_tests` 1474-1506 (`place.free-ok` … `place.free-fixed-
/// stays-put`): under a free parent the SAME words mean position and
/// size — center/center centers the box, `width=fill` with no vertical
/// word takes the whole parent (D71), right/bottom margins pin from those
/// edges, `fill` + `margin=16` insets both edges, and an exact width
/// stays at the left edge while the unkeyed height fills.
#[test]
fn free_parent_words_place_and_size() {
    let b = build(
        "View page {\n Button cc halign=center, valign=center\n Button tw width=fill\n Button br halign=right, valign=bottom, margin_right=12, margin_bottom=8\n Panel fr width=fill, height=fill, margin=16 { Button inner }\n Button fx width=120\n}\n",
        400.0,
        400.0,
    );
    let cc = b.rect("cc");
    assert!(close(cc.x + cc.w / 2.0, 200.0) && close(cc.y + cc.h / 2.0, 200.0), "{cc:?}");
    assert!(cc.w < 400.0 && cc.h < 400.0, "center/center hugs: {cc:?}");
    assert_eq!(b.rect("tw"), Rect::new(0.0, 0.0, 400.0, 400.0), "D71: the unkeyed axis takes the parent's extent");
    let br = b.rect("br");
    assert!(close(br.right(), 388.0) && close(br.bottom(), 392.0), "{br:?}");
    assert_eq!(b.rect("fr"), Rect::new(16.0, 16.0, 368.0, 368.0));
    let fx = b.rect("fx");
    assert_eq!((fx.x, fx.w, fx.y, fx.h), (0.0, 120.0, 0.0, 400.0), "{fx:?}");
}

/// `_placement_tests` 1514-1531 (`place.dfl-ok` … `place.dfl-stylebox-
/// only-fills`): the free-fill default — a bare Rectangle fills its
/// Layers, `width=hug, height=hug` opts out (a content-less box is 0×0),
/// and a Panel with only padding fills too.
#[test]
fn free_fill_default_and_the_hug_opt_out() {
    let b = build("View host {\n Rectangle bare color=#FFFFFFFF\n Rectangle optout color=#FFFFFFFF, width=hug, height=hug\n Panel pad padding=6 { Label t text=\"x\", font_size=12 }\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("bare"), Rect::new(0.0, 0.0, 400.0, 400.0));
    assert_eq!(b.rect("optout"), Rect::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(b.rect("pad"), Rect::new(0.0, 0.0, 400.0, 400.0));
}

/// `_placement_tests` 1535-1547 (`place.dfl-button-ok`, `place.dfl-
/// button-content-fills`): Button content follows the same rule — the
/// unkeyed wrapper inside a 60×90 Button is 60×90 (the BrushRow trap: a
/// hugged wrapper wrapped its label at half the button's width).
#[test]
fn button_content_fills_the_button() {
    let b = build("View v {\n Button padRow2 60x90 {\n  Panel content2 padding_top=8 {\n   Column col { Label lbl text=\"two words\", font_size=12, wrap=true }\n  }\n }\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("content2"), Rect::new(0.0, 0.0, 60.0, 90.0));
}

/// `_placement_tests` 1550-1555 (`place.dfl-root-ok`, `place.dfl-root-
/// exempt`). FLIP — gogo-design.md §7 (the host-class rules are gone; the
/// shell hands the root its viewport, `layout::place`): a bare `Panel`
/// root takes the viewport it is offered. Godot left the root's anchors
/// at 0 for a host script to place.
#[test]
fn bare_root_takes_the_viewport() {
    let b = build("Panel bareRoot", 400.0, 300.0);
    assert_eq!(b.rect("bareRoot"), Rect::new(0.0, 0.0, 400.0, 300.0));
}

/// `_placement_tests` 1559-1567 (`place.dfl-container-ok`, `place.dfl-
/// container-untouched`): a container child keeps the slot defaults — a
/// content-less Rectangle in a Column spans the column's width and is 0
/// tall (no free-fill default sneaks in on the main axis).
#[test]
fn container_children_keep_the_slot_defaults() {
    let b = build("View v { Column c { Rectangle rr color=#FFFFFFFF } }", 400.0, 400.0);
    assert_eq!(b.rect("rr"), Rect::new(0.0, 0.0, 400.0, 0.0));
}

/// `_placement_tests` 1571-1581 (`place.dfl-slot-ok`): a widget with a
/// bare `slot` marker takes instance content and builds.
#[test]
fn bare_slot_content_builds() {
    let b = build("widget Holder {\n Layers body {\n  slot\n }\n}\nView v {\n Holder h {\n  Rectangle sc color=#FFFFFFFF\n }\n}\n", 400.0, 400.0);
    assert_eq!(b.page.nodes[b.id("sc") as usize].kind, Kind::Rectangle);
}

/// `_placement_tests` 1582-1584 (`place.dfl-slot-exempt`): content
/// handed to a widget's bare `slot` marker gets NO
/// free-fill default (gogo-spec "Default: a child takes the space its
/// parent gives it … Exempt: … content of the bare default `slot`") — a
/// sizeless Rectangle there stays 0 wide.
#[test]
fn bare_slot_content_gets_no_fill_default() {
    let b = build("widget Holder {\n Layers body {\n  slot\n }\n}\nView v {\n Holder h {\n  Rectangle sc color=#FFFFFFFF\n }\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("sc").w, 0.0, "{:?}", b.rect("sc"));
}

/// `_placement_tests` 1591-1606 (`place.dfl-variant-ok`, `place.dfl-
/// variant-base`, `place.dfl-variant-inherits`): the free-fill default
/// survives an arrangement — a pose that moves ONE axis (`margin_bottom`)
/// keeps the other, defaulted axis at the parent's full width (the appBar
/// once collapsed to zero width when `expanded` flipped).
#[test]
fn default_fill_survives_an_arrangement_pose() {
    let mut b = build("arrangement lifted\nwhen lifted { bar.margin_bottom = 0 }\nView screen2 {\n Layers bar valign=bottom, height=80, margin_bottom=-30\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("bar"), Rect::new(0.0, 350.0, 400.0, 80.0));
    b.rt.go_id(0);
    b.relayout();
    assert_eq!(b.rect("bar"), Rect::new(0.0, 320.0, 400.0, 80.0), "the pose moved the pin, the width stayed full");
}

/// `_placement_tests` 1612-1624 (`place.dfl-widget-root-ok`, `place.dfl-
/// widget-root-centered`): instance-site alignment over a widget body's
/// defaulted axis merges cleanly — `Chip c1 halign=center, valign=center`
/// is centered and hugging (the tutorial Card's "halign has no effect").
#[test]
fn widget_instance_alignment_merges_over_the_body() {
    let b = build("widget Chip {\n Panel padding=4 { Label t2 text=\"chip\", font_size=12 }\n}\nView host2 {\n Chip c1 halign=center, valign=center\n}\n", 400.0, 400.0);
    let c1 = b.rect("c1");
    assert!(close(c1.x + c1.w / 2.0, 200.0) && close(c1.y + c1.h / 2.0, 200.0), "{c1:?}");
    assert!(c1.w < 400.0 && c1.h < 400.0, "centered means hugging: {c1:?}");
}

/// `_placement_tests` 1628-1639 (`place.style-ok`): the Godot fixture's
/// widget body line `Button signals text=$label` builds — the `signals`
/// flag needs no comma before the next `key=`.
#[test]
#[ignore = "NEW: Godot parses `Button signals text=$label` vs Rust G0017 missing `,` before `text` (a flag followed by key= needs a comma)"]
fn signals_flag_needs_no_comma_before_a_key() {
    let src = "widget RowBtn $label=\"\" {\n Button signals text=$label\n}\nView v { RowBtn a label=\"x\" }";
    assert!(!refused(src), "{}", messages(src));
}

/// `_placement_tests` 1640-1644 (`place.style-fill`, `place.node-
/// overrides-style`, `place.fill-n-overrides`): a style carries placement
/// onto a widget instance (`.Grow` → `width=fill`), the node's own word
/// overrides the style (`width=hug`), and `width=fill(3)` on a styled
/// Button keeps its weight. The body line is written `signals, text=`
/// (the comma Rust's parser wants, see the test above).
#[test]
fn styles_carry_placement_and_the_node_overrides() {
    let b = build("style Grow width=fill\nwidget RowBtn $label=\"\" {\n Button signals, text=$label\n}\nView v {\n Column col {\n  RowBtn a .Grow\n  RowBtn b .Grow width=hug\n  Button c .Grow width=fill(3)\n }\n}\n", 400.0, 400.0);
    assert_eq!(b.placement("a").width, Len::Fill(1.0));
    assert_eq!(b.placement("b").width, Len::Hug(1.0));
    assert_eq!(b.placement("c").width, Len::Fill(3.0));
    assert_eq!(b.rect("a").w, 400.0, "{:?}", b.rect("a"));
    assert!(b.rect("b").w < 400.0, "hug: {:?}", b.rect("b"));
    assert_eq!(b.rect("c").w, 400.0, "{:?}", b.rect("c"));
}

/// `_placement_tests` 1648-1675 (`place.theme-fontsize-ok` … `place.
/// default-fontsize`): `theme { font_size = 22 }` is the inherited text
/// size (a Label with no word measures 22), a node's own `font_size=15`
/// wins, and with no theme the built-in default is 15 epx. Heights follow
/// FixedAdvance (1.25 em per line); Godot read the Theme's
/// `default_font_size`.
#[test]
fn theme_font_size_inherits_and_local_wins() {
    let b = build("theme {\n font_size = 22\n}\nView v {\n Column root {\n  Label a text=\"inherits\"\n  Label b text=\"own\", font_size=15\n }\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("a").h, 22.0 * 1.25, "{:?}", b.rect("a"));
    assert_eq!(b.rect("b").h, 15.0 * 1.25, "{:?}", b.rect("b"));
    assert_eq!(DEFAULT_FONT_SIZE, 15.0);
    let b = build("View v { Column root { Label a text=\"defaulted\" } }", 400.0, 400.0);
    assert_eq!(b.rect("a").h, 15.0 * 1.25, "{:?}", b.rect("a"));
}

/// `_placement_tests` 1678-1683 (`place.aliases-ok`, `place.aliases`):
/// `Layers`, `Row`, `Column` are the three container kinds.
#[test]
fn layers_row_column_are_kinds() {
    let b = build("View v { Layers l { Row r { Column c } } }", 100.0, 100.0);
    assert_eq!(b.page.nodes[b.id("l") as usize].kind, Kind::Layers);
    assert_eq!(b.page.nodes[b.id("r") as usize].kind, Kind::Row);
    assert_eq!(b.page.nodes[b.id("c") as usize].kind, Kind::Column);
}

/// `_placement_tests` 1688-1707 (`layers.ok`, `layers.min-from-children`,
/// `layers.input-transparent`, `layers.interactive-claims`): a Layers is
/// as big as its biggest VISIBLE child (a `gone` 300×90 does not count, a
/// fill Rectangle claims nothing), a plain Layers is transparent to input
/// (D103), and `interactive=true` makes it a surface.
#[test]
fn layers_minimum_and_input_claim() {
    let b = build(
        "View v {\n Column col halign=left, valign=top {\n  Layers row {\n   Button big 100x40\n   Button hidden 300x90, visibility=gone\n   Rectangle glow color=#FFFFFF22, width=fill, height=fill\n  }\n  Layers blocker interactive=true, touch_target=off, height=50\n  Layers plain height=50\n }\n}\n",
        400.0,
        400.0,
    );
    assert_eq!(b.rect("row"), Rect::new(0.0, 0.0, 100.0, 40.0));
    let under = |x: f32, y: f32| hit(&b.page, &b.res, &b.rects, x, y, false);
    assert_eq!(under(50.0, 20.0), Some(b.id("big")), "the Layers never stands between a finger and its child");
    assert_eq!(under(50.0, 60.0), Some(b.id("blocker")), "interactive=true claims");
    assert_eq!(under(50.0, 110.0), None, "a plain Layers is transparent: {:?}", b.rect("plain"));
}

/// `_placement_tests` 1710-1712 (`place.err-retired-vert/horiz/stack`).
/// FLIP — gogo-design.md §7 "Retired-spelling redirects … Unknown word =
/// build error, no alias table": `VertPanel`, `HorizPanel`, `StackPanel`
/// are unknown node types (G7011), not redirects to Column/Row.
#[test]
fn retired_container_spellings_are_unknown_types() {
    for word in ["VertPanel", "HorizPanel", "StackPanel"] {
        let list = codes(&format!("View v {{ {word} x }}"));
        assert!(list.contains(&"G7011".to_string()), "{word}: {list:?}");
    }
}

/// `_placement_tests` 1715-1740, the guard errors Rust refuses the same
/// way: `halign=stretch` (G0057), `width=auto` (G0056, names `hug`),
/// `width=center` (G0056), `halign=center, width=fill` (G1011),
/// `width=grow(2)` (G0056), `anchors=` / `offset_left=` /
/// `size_flags_horizontal=` (G0041, gone). Godot's hint text names the
/// door (`width=fill`, `halign`, "signed pins"); Rust's messages differ.
#[test]
fn placement_guard_errors_refuse() {
    assert!(codes("View v { Button b halign=stretch }").contains(&"G0057".to_string()));
    let auto = messages("View v { Button b width=auto }");
    assert!(auto.contains("G0056") && auto.contains("hug"), "{auto}");
    assert!(codes("View v { Button b width=center }").contains(&"G0056".to_string()));
    assert!(codes("View v { Button b halign=center, width=fill }").contains(&"G1011".to_string()));
    assert!(codes("View v { Button b width=grow(2) }").contains(&"G0056".to_string()));
    assert!(codes("View v { Layers c { Button b anchors=center, halign=center } }").contains(&"G0041".to_string()));
    assert!(codes("View v { Button b halign=center, offset_left=4 }").contains(&"G0041".to_string()));
    assert!(codes("View v { Button b halign=center, size_flags_horizontal=3 }").contains(&"G0041".to_string()));
}

/// `_placement_tests` 1719-1720 (`place.err-cross-axis`): `halign=top` is
/// a cross-axis word and a build error pointing at `valign`.
#[test]
fn cross_axis_alignment_word_is_refused() {
    assert!(refused("View v { Button b halign=top }"), "{}", messages("View v { Button b halign=top }"));
    assert!(refused("View v { Button b valign=right }"));
    // In a `when` response too — the runtime pose used to read it silently.
    assert!(refused("View v { Button b { when held halign=top } }"));
    // The text pair splits the same way.
    assert!(refused("View v { Label a text=\"x\", horizontal_alignment=top }"));
}

/// GPAR16, E9 (2026-08-31): `start`/`end` were Rust-only inventions — two
/// more spellings of left/right that only earn their keep with RTL
/// mirroring, and that is a whole milestone (G14: alignment, margins and
/// direction flip together). Until then they are unknown words.
#[test]
fn start_and_end_wait_for_the_rtl_milestone() {
    assert!(refused("View v { Button b halign=start }"), "{}", messages("View v { Button b halign=start }"));
    assert!(refused("View v { Button b valign=end }"));
}

/// `_placement_tests` 1725-1728 (`place.err-weights-differ`, `place.err-
/// weight-zero`): `width=fill(2), height=fill(3)` is refused (one stretch
/// ratio per control), and `fill(0)` is refused (a weight is > 0).
#[test]
#[ignore = "GP41: not ported: load-time lints — differing fill weights and fill(0) compile clean in Rust"]
fn fill_weight_lints() {
    assert!(refused("View v { Button b width=fill(2), height=fill(3) }"), "{}", messages("View v { Button b width=fill(2), height=fill(3) }"));
    assert!(refused("View v { Button b width=fill(0) }"), "{}", messages("View v { Button b width=fill(0) }"));
}

/// `_placement_tests` 1735-1738 (`place.err-anchors-margin-removed`,
/// `place.err-inset-retired`): the removed `anchors_margin=` and the
/// retired `inset=` are build ERRORS naming the replacement.
#[test]
fn removed_props_are_errors() {
    assert!(refused("View v { Button b anchors_margin=4 }"), "{}", messages("View v { Button b anchors_margin=4 }"));
    assert!(refused("View v { Layers c { Rectangle r inset=8 } }"), "{}", messages("View v { Layers c { Rectangle r inset=8 } }"));
}

/// `_placement_tests` 1743-1746 (`place.err-margin-in-container`, `place.
/// err-margin-help-spacing`, `place.err-margin-help-gap`): a margin on a
/// Column child is refused, and the message names both doors —
/// `spacing=` and `gap`.
#[test]
fn margin_in_a_stack_is_refused_and_names_the_doors() {
    let text = messages("View v { Column c { Button b margin=4 } }");
    assert!(text.contains("G1014"), "{text}");
    assert!(text.contains("spacing="), "{text}");
    assert!(text.contains("gap"), "{text}");
}

/// `_placement_tests` 1751-1763 (`place.root-margin-builds`, `place.root-
/// margin-pins`, `place.root-min-from-children`; fixture
/// root_margin_ok.gogo): the root is free placement — a margined root
/// child builds and pins at x=40 spanning to the far edge, and a
/// hug-placed root is as wide as its biggest child plus that margin.
#[test]
fn root_is_free_placement_and_hugs_its_children() {
    let b = build("View root visibility=gone, halign=left, valign=top {\n Panel card margin_left=40 {\n  Label text=\"x\", min_width=120\n }\n}\n", 400.0, 400.0);
    let root = b.rect("root");
    let card = b.rect("card");
    assert_eq!(card.x, 40.0, "{card:?}");
    assert!(close(card.right(), root.right()), "spans to the root's far edge: {card:?} in {root:?}");
    assert!(root.w >= 120.0 + 40.0, "{root:?}");
}

/// `_placement_tests` 1769-1779 (`place.root-plate-ok`, `place.root-
/// plate-box`): the root draws its OWN plate — `View v
/// background=#112233FF, corner_radius=3` puts that color and radius in
/// the draw list over the root's rect.
#[test]
fn root_draws_its_own_plate() {
    let b = build("View v background=#112233FF, corner_radius=3", 200.0, 100.0);
    assert_eq!(b.plate("v"), Some((Rgba([0x11, 0x22, 0x33, 0xFF]), 3.0)), "{:#?}", b.draws());
}

/// `_placement_tests` 1780-1781 (`place.err-placement-in-when`): a
/// placement word in a CONTROL-scope `when` (`when held halign=center` in
/// a style) is refused — placement responds at view scope
/// (arrangements), not to held/hovered.
#[test]
fn placement_in_a_control_scope_when_is_refused() {
    let src = "style S { when held halign=center }\nView v { Button b .S width=40 }";
    assert!(refused(src), "{}", messages(src));
}

// ---------------------------------------------------------- _free_padding_tests

/// `_free_padding_tests` 1793-1833 (`fpad.begin-ok` … `fpad.center-
/// shift`): padding on a Layers is air inside it — a left-pinned child
/// starts at the padding, a fill child insets on both edges, and
/// asymmetric padding moves a centered child's midpoint to the padded
/// box's true center.
#[test]
fn layers_padding_insets_pinned_fill_and_centered_children() {
    let b = build("View v {\n Layers l padding_left=10, padding_right=6 {\n  Rectangle r 20x20, halign=left, color=#FFFFFFFF\n }\n}\n", 400.0, 400.0);
    assert_eq!((b.rect("r").x, b.rect("r").w), (10.0, 20.0), "{:?}", b.rect("r"));
    let b = build("View v {\n Layers l padding_left=10, padding_right=6 {\n  Rectangle r width=fill, height=20, color=#FFFFFFFF\n }\n}\n", 400.0, 400.0);
    assert_eq!((b.rect("r").x, b.rect("r").right()), (10.0, 394.0), "{:?}", b.rect("r"));
    let b = build("View v {\n Layers l padding_left=10 {\n  Rectangle r 20x20, halign=center, color=#FFFFFFFF\n }\n}\n", 400.0, 400.0);
    assert!(close(b.rect("r").x, 195.0), "the padded box's center is 205: {:?}", b.rect("r"));
}

/// `_free_padding_tests` 1837-1847 (`fpad.min-ok`, `fpad.min-derives`):
/// the pads count into the Layers minimum — `padding=8` around a 30×20
/// child makes a 46×36 Layers (seen through a hugging Column).
#[test]
fn layers_padding_counts_into_its_minimum() {
    let b = build("View v {\n Column col halign=left, valign=top {\n  Layers l padding=8 { Rectangle 30x20, color=#FFFFFFFF }\n }\n}\n", 400.0, 400.0);
    assert_eq!((b.rect("l").w, b.rect("l").h), (46.0, 36.0), "{:?}", b.rect("l"));
}

/// `_free_padding_tests` 1850-1864 (`fpad.safe-ok`, `fpad.safe-edges`):
/// `padding=safe` reads each edge's OWN host inset — with insets
/// (3, 5, 7, 11) a top-left child sits at (3, 5).
#[test]
fn padding_safe_reads_each_edge() {
    let mut b = build("View v {\n Layers l padding=safe {\n  Rectangle r 20x20, halign=left, valign=top, color=#FFFFFFFF\n }\n}\n", 400.0, 400.0);
    b.rt.safe = [3.0, 5.0, 7.0, 11.0];
    b.relayout();
    assert_eq!((b.rect("r").x, b.rect("r").y), (3.0, 5.0), "{:?}", b.rect("r"));
}

/// `_free_padding_tests` 1868-1875 (`fpad.idle-ok`, `fpad.idle-
/// untouched`): no padding declared = nothing moves — an exact 20×20 in a
/// Layers sits at (0, 0) at its own size.
#[test]
fn no_padding_leaves_an_exact_child_at_the_origin() {
    let b = build("View v { Layers l { Rectangle r 20x20, color=#FFFFFFFF } }", 400.0, 400.0);
    assert_eq!(b.rect("r"), Rect::new(0.0, 0.0, 20.0, 20.0));
}

/// `_free_padding_tests` 1879-1889 (`fpad.style-ok`, `fpad.style-inset`):
/// a style row carries padding onto a Layers like an inline prop.
#[test]
fn a_style_carries_padding_onto_a_layers() {
    let b = build("style Air padding_left=10\nView v {\n Layers l .Air {\n  Rectangle r 20x20, halign=left, color=#FFFFFFFF\n }\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("r").x, 10.0, "{:?}", b.rect("r"));
}

/// `_free_padding_tests` 1893-1905 (`fpad.root-ok`, `fpad.root-inset`,
/// `fpad.root-min`; fixture root_padding.gogo): the plain View root takes
/// padding — its top-left child sits at (12, 12), and the pads count into
/// the root's derived minimum: a hugging `View padding=12` around a 30×30
/// child is 54×54.
#[test]
fn root_padding_insets_children_and_counts_into_its_minimum() {
    let b = build("View root padding=12 {\n Rectangle box 30x30, halign=left, valign=top, color=#FFFFFFFF\n}\n", 400.0, 400.0);
    assert_eq!((b.rect("box").x, b.rect("box").y), (12.0, 12.0), "{:?}", b.rect("box"));
    let b = build("View root padding=12, halign=left, valign=top {\n Rectangle box 30x30, halign=left, valign=top, color=#FFFFFFFF\n}\n", 400.0, 400.0);
    assert_eq!((b.rect("root").w, b.rect("root").h), (54.0, 54.0), "{:?}", b.rect("root"));
}

/// `_free_padding_tests` 1910-1914 (`fpad.err-column`, `fpad.err-column-
/// gap`, `fpad.err-column-doors`, `fpad.err-grid`). FLIP — GPAR19
/// DECIDED 2026-08-30: `padding` on a Column/Grid (any container) is
/// legal in Rust and is air inside it; Godot refused it and pointed at
/// `spacing=`/`gap` and a Layers/Panel wrapper.
#[test]
fn padding_on_containers_is_legal_and_insets() {
    let b = build("View v {\n Column c padding=8 { Rectangle r height=10, color=#FFFFFFFF }\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("r"), Rect::new(8.0, 8.0, 384.0, 10.0));
    let b = build("View v {\n Grid g cols=[fill, fill], padding_top=4 {\n  row { Rectangle a height=10, color=#FFFFFFFF }\n }\n}\n", 400.0, 400.0);
    assert_eq!(b.rect("a").y, 4.0, "{:?}", b.rect("a"));
}

/// `_free_padding_tests` 1922-1949 (`fpad.label-ok`, `fpad.label-
/// margins`, `fpad.label-invisible`, `fpad.label-min-grows`, `fpad.label-
/// side`): a Label owns its inside — `padding=8` insets the text 8 from
/// the box and grows the box by 16×16 over a plain label, `padding_left=4`
/// grows it 4 wide and not at all tall, and the padding paints no plate.
/// The labels hug (`halign=left`) so their widths show.
#[test]
fn label_padding_insets_text_and_grows_the_box() {
    let b = build("View v {\n Column col {\n  Label padded text=\"x\", font_size=15, padding=8, halign=left\n  Label plain text=\"x\", font_size=15, halign=left\n  Label lopsided text=\"x\", font_size=15, padding_left=4, halign=left\n }\n}\n", 400.0, 400.0);
    let padded = b.rect("padded");
    let plain = b.rect("plain");
    let lopsided = b.rect("lopsided");
    assert!(close(padded.w - plain.w, 16.0) && close(padded.h - plain.h, 16.0), "{padded:?} vs {plain:?}");
    assert!(close(lopsided.w - plain.w, 4.0) && close(lopsided.h - plain.h, 0.0), "{lopsided:?} vs {plain:?}");
    let texts: Vec<Rect> = b.draws().iter().filter_map(|d| if let Draw::Text { rect, .. } = d { Some(*rect) } else { None }).collect();
    assert!(texts.iter().any(|t| close(t.x, padded.x + 8.0) && close(t.y, padded.y + 8.0) && close(t.right(), padded.right() - 8.0)), "text inset 8: {texts:?} in {padded:?}");
    assert!(!b.draws().iter().any(|d| matches!(d, Draw::Quad { rect, .. } if *rect == padded)), "padding paints no plate: {:#?}", b.draws());
}

/// `_free_padding_tests` 1950-1959 (`fpad.input-ok`, `fpad.input-
/// margins`): `Input field padding=6` carries the air on every edge — the
/// box is 12 taller than a plain Input.
#[test]
fn input_padding_grows_the_box() {
    let b = build("View v {\n Column col halign=left, valign=top {\n  Input field padding=6, halign=left\n  Input plain halign=left\n }\n}\n", 400.0, 400.0);
    let field = b.rect("field");
    let plain = b.rect("plain");
    assert!(close(field.h - plain.h, 12.0), "{field:?} vs {plain:?}");
    assert!(close(field.w - plain.w, 12.0), "{field:?} vs {plain:?}");
}

/// `_free_padding_tests` 1975-2009 (`fpad.button-ok`, `fpad.button-
/// margins`, `fpad.button-press-feedback`, `fpad.button-radius-all-
/// states`): `Button pad padding=10` insets its text 10 in the normal
/// AND the held state, and the held look still changes the plate (the
/// D128 bug: padding killed press feedback); `corner_radius=8` keeps its
/// radius in both states with the held pigment applied. Godot's held
/// pigment came from the theme; here it is an authored `when held`.
#[test]
fn button_padding_and_radius_keep_press_feedback() {
    let src = "View v {\n Column col halign=left, valign=top {\n  Button ok text=\"OK\"\n  Button pad text=\"fails\", padding=10 { when held background=#FF0000FF }\n  Button rad text=\"r\", corner_radius=8 { when held background=#FF0000FF }\n }\n}\n";
    let mut b = build(src, 400.0, 400.0);
    let pad = b.rect("pad");
    let text = b.text_rect("fails").unwrap();
    assert!(close(text.x, pad.x + 10.0) && close(text.y, pad.y + 10.0) && close(text.right(), pad.right() - 10.0), "{text:?} in {pad:?}");
    assert_eq!(b.plate("pad").map(|p| p.0), Some(gogo::draw::DEFAULT_BUTTON_BACKGROUND));
    assert_eq!(b.plate("rad").map(|p| p.1), Some(8.0));
    b.rt.set_state(b.id("pad"), HELD, true);
    b.rt.set_state(b.id("rad"), HELD, true);
    b.relayout();
    assert_eq!(b.plate("pad").map(|p| p.0), Some(Rgba([0xFF, 0, 0, 0xFF])), "held changes the plate: {:#?}", b.draws());
    let held_text = b.text_rect("fails").unwrap();
    assert_eq!(held_text, text, "the padding holds while held");
    assert_eq!(b.plate("rad"), Some((Rgba([0xFF, 0, 0, 0xFF]), 8.0)), "radius in every state: {:#?}", b.draws());
}

/// `_free_padding_tests` 2014-2025 (`fpad.button-bg-ok`, `fpad.button-
/// bg-takeover`): a static `background=` takes over the plate WHOLE — the
/// same color in the normal and held state — and the padding stays.
#[test]
fn static_background_takes_over_every_state_and_keeps_padding() {
    let mut b = build("View v {\n Button b text=\"x\", padding=10, background=#4488FFFF, halign=left, valign=top\n}\n", 400.0, 400.0);
    let blue = Rgba([0x44, 0x88, 0xFF, 0xFF]);
    assert_eq!(b.plate("b").map(|p| p.0), Some(blue), "{:#?}", b.draws());
    let text = b.text_rect("x").unwrap();
    assert!(close(text.x, b.rect("b").x + 10.0), "{text:?} in {:?}", b.rect("b"));
    b.rt.set_state(b.id("b"), HELD, true);
    b.relayout();
    assert_eq!(b.plate("b").map(|p| p.0), Some(blue), "held: {:#?}", b.draws());
    assert_eq!(b.text_rect("x").unwrap(), text);
}

/// `_free_padding_tests` 2029-2035 (`fpad.err-rect`, `fpad.err-rect-
/// margin`, `fpad.label-paint-refused`, `fpad.err-paint`): a drawing leaf
/// has nothing inside, so `Rectangle padding=4` is refused naming
/// "children or text" and the `margin` door. Godot's second half —
/// `Label background=` and `Layers background=` refused pointing at
/// `Panel` — is REVERSED here (E8, 2026-08-31): any box has a plate.
#[test]
fn leaf_padding_is_refused_and_any_box_takes_a_plate() {
    let rect = messages("View v { Rectangle r 10x10, padding=4 }");
    assert!(refused("View v { Rectangle r 10x10, padding=4 }"), "{rect}");
    assert!(rect.contains("children or text") && rect.contains("margin"), "{rect}");
    // E8 (2026-08-31) reverses Godot's second half: any box has a plate —
    // a Label's or a Layers' `background=` builds and draws (pins in
    // tests/kind_words.rs).
    assert!(!refused("View v { Label t text=\"x\", background=red }"), "{}", messages("View v { Label t text=\"x\", background=red }"));
    assert!(!refused("View v { Layers l background=red }"), "{}", messages("View v { Layers l background=red }"));
}
