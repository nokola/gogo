//! Port of the Godot gogo suite's Grid and Tiles groups
//! (`FantasiaGodot/addons/gogo/test/run_tests.gd`):
//! `_grid_tests` 11116-11562 (91 checks) and `_tiles_tests` 11562-11841
//! (38 checks). Tracks (`cols=`), rows, `skip`, spans, the `areas=` form,
//! the error battery, and the Tiles cell lattice.
//!
//! ENGINE (not ported):
//! - `grid.epx-scaled` / `grid.epx-reapply` (11311, 11317): epx provider
//!   and binding re-apply — Rust lays out in epx, the host scales.
//!
//! Godot's runtime `set_cell` / `set("cols", …)` writes are
//! `Runtime::set_prop` here; `x.visible = false` is `Runtime::set_visible`
//! (gone). Godot sized a detached container with `g.size = …`; here the
//! grid gets `width=`/`height=` at the top-left of a View.
use gogo::layout::{FixedAdvance, Rect, TextMeasure, layout};
use gogo::page::{PropKey, Value};
use gogo::runtime::{Runtime, resolve};
use gogo::{Compiled, Page, compile};

fn build(src: &str) -> Compiled {
    compile(src, &|_| None)
}

fn ok(src: &str) -> Compiled {
    let c = build(src);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn fails(src: &str) -> bool {
    gogo::has_errors(&build(src).diags)
}

/// Godot built snippets detached (any root); Rust needs a `View`/`Panel`
/// root (G7003), so a snippet goes inside one — the fence under test must
/// be what refuses it, not the root rule.
fn in_view(src: &str) -> String {
    format!("View v {{\n{src}\n}}\n")
}

fn codes(src: &str) -> Vec<String> {
    build(src).diags.iter().map(|d| d.code.clone()).collect()
}

fn warnings(src: &str) -> Vec<String> {
    let c = ok(src);
    c.diags.iter().filter(|d| d.severity == gogo::Severity::Warning).map(|d| d.message.clone()).collect()
}

fn lay(src: &str, w: f32, h: f32) -> (Page, Vec<Rect>) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, w, h), &FixedAdvance);
    (c.page, rects)
}

fn relay(page: &Page, rt: &Runtime, w: f32, h: f32) -> Vec<Rect> {
    let res = resolve(page, rt);
    layout(page, &res, Rect::new(0.0, 0.0, w, h), &FixedAdvance)
}

fn rect(p: &Page, r: &[Rect], name: &str) -> Rect {
    r[p.find(name).unwrap_or_else(|| panic!("no node {name}")) as usize]
}

/// A fake bitmap for `aspect=natural`: every image is 100×50 px.
struct Pic;

impl TextMeasure for Pic {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        FixedAdvance.measure(text, font_size, max_width, font)
    }
    fn image_size(&self, _key: &str) -> Option<(f32, f32)> {
        Some((100.0, 50.0))
    }
}

// ------------------------------------------------------------------ Grid

/// `_grid_tests` 11119-11129: a track list parses with every value kind
/// in it (number, word, call, `$ref`), may break across lines with a
/// trailing comma, and an unclosed list is a build error.
#[test]
fn grid_track_list_parses_every_value_kind() {
    let c = ok("theme { $w = 50 }\nView v { Grid g cols=[80, hug, fill(2), $w] { } }\n");
    let g = c.page.find("g").unwrap();
    let cols = c.page.prop(g, PropKey::Cols).expect("cols prop");
    let items: Vec<Value> = c.page.list(cols).to_vec();
    assert_eq!(items.len(), 4, "{items:?}");
    assert!(matches!(items[0], Value::VNum(n) if n == 80.0), "{items:?}");
    assert!(matches!(items[1], Value::VIdent(w) if c.page.str(w) == "hug"), "{items:?}");
    assert!(matches!(items[2], Value::VCall { name, .. } if c.page.str(name) == "fill"), "{items:?}");
    assert!(matches!(items[3], Value::VRef { .. }), "{items:?}");
    ok("View v { Grid g cols=[80,\n        hug,\n] { } }\n");
    assert!(fails("View v { Grid g cols=[80, hug { } }\n"), "an unclosed list must not build");
}

/// `_grid_tests` 11130: a list on a prop that takes none (`width=[1, 2]`)
/// is refused. Rust refuses it through the placement path (G0056); a list
/// on a NON-placement prop is GPAR17's open row, not this check.
#[test]
fn a_list_on_a_size_prop_is_refused() {
    assert!(fails("View v { Rectangle r width=[1, 2] }\n"), "{:?}", codes("View v { Rectangle r width=[1, 2] }\n"));
}

const ROW_FORM: &str = "View v {\n Grid g cols=[40, hug, fill], width=200, height=100, halign=left, valign=top {\n  row height=30 { Rectangle a min_size=10x10; Rectangle b min_size=20x12; Rectangle c min_size=5x5 }\n  row { Rectangle d min_size=15x25, col_span=3 }\n }\n}\n";

/// `_grid_tests` 11140-11158: the row form — a px track is its number, a
/// hug track its widest cell, a fill track the rest; a row's `height=` is
/// exact; `col_span=3` covers the whole width; the grid's own minimum is
/// the track sum × the row sum (65×55).
#[test]
fn row_form_px_hug_fill_tracks_and_col_span() {
    let (p, r) = lay(ROW_FORM, 400.0, 400.0);
    assert_eq!(rect(&p, &r, "a"), Rect::new(0.0, 0.0, 40.0, 30.0), "{r:?}");
    assert_eq!(rect(&p, &r, "b"), Rect::new(40.0, 0.0, 20.0, 30.0), "{r:?}");
    assert_eq!(rect(&p, &r, "c"), Rect::new(60.0, 0.0, 140.0, 30.0), "{r:?}");
    assert_eq!(rect(&p, &r, "d"), Rect::new(0.0, 30.0, 200.0, 25.0), "{r:?}");
    // The minimum: the same grid hugging both axes.
    let (p, r) = lay(&ROW_FORM.replace("width=200, height=100, ", ""), 400.0, 400.0);
    let g = rect(&p, &r, "g");
    assert_eq!((g.w, g.h), (65.0, 55.0), "{g:?}");
}

/// `_grid_tests` 11169-11192: fill tracks share by weight (1:3 of 400 =
/// 100/300), fractional weights included (`fill(1.618)` next to `fill` in
/// 261.8 is 161.8 — the shipped 0.4*/1.618* stars).
#[test]
fn fill_tracks_share_by_weight_fractions_too() {
    let (p, r) = lay("View v {\n Grid g cols=[fill, fill(3)], width=400, height=50, halign=left, valign=top {\n  row { Rectangle a min_size=10x10; Rectangle b min_size=10x10 }\n }\n}\n", 500.0, 500.0);
    let a = rect(&p, &r, "a");
    let b = rect(&p, &r, "b");
    assert!(a.w == 100.0 && b.x == 100.0 && b.w == 300.0, "{a:?} {b:?}");
    let (p, r) = lay("View v {\n Grid g cols=[fill(1.618), fill], width=261.8, height=20, halign=left, valign=top {\n  row { Rectangle a min_size=1x1; Rectangle b min_size=1x1 }\n }\n}\n", 500.0, 500.0);
    let a = rect(&p, &r, "a");
    assert!((a.w - 161.8).abs() < 0.01, "{a:?}");
}

const ROW_SPAN: &str = "View v {\n Grid g cols=[hug, hug], width=40, height=50, halign=left, valign=top {\n  row { Rectangle a min_size=20x20; Rectangle e min_size=20x20, row_span=2 }\n  row { Rectangle b min_size=20x30; skip }\n }\n}\n";

/// `_grid_tests` 11204, 11214: the `skip` under a `row_span=2` cell keeps
/// the cell beside it in its own track — `b` sits in track 0 of row 2 at
/// the row's height (0,20,20,30).
#[test]
fn a_skip_under_a_row_span_keeps_the_neighbour_in_its_track() {
    let (p, r) = lay(ROW_SPAN, 400.0, 400.0);
    assert_eq!(rect(&p, &r, "b"), Rect::new(0.0, 20.0, 20.0, 30.0), "{r:?}");
}

/// `_grid_tests` 11212: a `row_span=2` cell is as tall as both rows it
/// covers (20 + 30 = 50), the spanned hug rows summing.
#[test]
fn a_row_span_cell_covers_both_rows() {
    let (p, r) = lay(ROW_SPAN, 400.0, 400.0);
    assert_eq!(rect(&p, &r, "e"), Rect::new(20.0, 0.0, 20.0, 50.0), "{r:?}");
}

/// `_grid_tests` 11225-11231: an interior `skip` leaves a hole — the next
/// cell starts after the skipped 20-px track; a row may omit its tail.
#[test]
fn an_interior_skip_leaves_a_hole() {
    let (p, r) = lay("View v {\n Grid g cols=[20, hug, hug], width=60, height=20, halign=left, valign=top {\n  row { skip; Rectangle a min_size=10x10 }\n }\n}\n", 400.0, 400.0);
    assert_eq!(rect(&p, &r, "a").x, 20.0, "{r:?}");
}

const DEFICIT: &str = "View v {\n Grid g cols=[hug, hug], width=60, height=20, halign=left, valign=top {\n  row { Rectangle a min_size=10x10; Rectangle b min_size=10x10 }\n  row { Rectangle s min_size=60x10, col_span=2 }\n }\n}\n";

/// `_grid_tests` 11243: a grid with a wide spanned cell over two hug
/// tracks builds.
#[test]
fn a_spanned_cell_over_hug_tracks_builds() {
    ok(DEFICIT);
}

/// `_grid_tests` 11249-11251: a spanned cell wider than the hug tracks it
/// covers spreads the deficit into them — the grid's minimum width is 60
/// and each 10-px track grows to 30.
#[test]
fn a_spanned_cells_deficit_spreads_into_hug_tracks() {
    let (p, r) = lay(&DEFICIT.replace("width=60, height=20, ", ""), 400.0, 400.0);
    assert_eq!(rect(&p, &r, "g").w, 60.0, "{r:?}");
    let (p, r) = lay(DEFICIT, 400.0, 400.0);
    let a = rect(&p, &r, "a");
    let b = rect(&p, &r, "b");
    assert!(a.w == 30.0 && b.x == 30.0, "{a:?} {b:?}");
}

/// `_grid_tests` 11263-11270: a cell aligns inside its track × row with
/// the one placement vocabulary — `halign=center, valign=bottom` puts a
/// 10×10 box at (45,40) in a 100×50 cell.
#[test]
fn a_cell_aligns_inside_its_cell_with_halign_valign() {
    let (p, r) = lay("View v {\n Grid g cols=[100], width=100, height=50, halign=left, valign=top {\n  row height=50 { Rectangle a min_size=10x10, halign=center, valign=bottom }\n }\n}\n", 400.0, 400.0);
    assert_eq!(rect(&p, &r, "a"), Rect::new(45.0, 40.0, 10.0, 10.0), "{r:?}");
}

/// `_grid_tests` 11281-11291: a hidden cell keeps its cell but stops
/// contributing to its hug track (WPF Collapsed) — `b` moves from 30 to 0
/// once `a` is gone.
#[test]
fn a_gone_cell_keeps_its_cell_and_stops_sizing_its_track() {
    let c = ok("View v {\n Grid g cols=[hug, hug], width=40, height=10, halign=left, valign=top {\n  row { Rectangle a min_size=30x10; Rectangle b min_size=10x10 }\n }\n}\n");
    let mut page = c.page;
    let a = page.find("a").unwrap();
    let mut rt = Runtime::new(&page);
    let r = relay(&page, &rt, 400.0, 400.0);
    assert_eq!(rect(&page, &r, "b").x, 30.0, "{r:?}");
    rt.set_visible(&mut page, a, false);
    let r = relay(&page, &rt, 400.0, 400.0);
    assert_eq!(rect(&page, &r, "b").x, 0.0, "{r:?}");
}

/// `_grid_tests` 11305: a grid with a px track and a fill track builds
/// (the epx-scaling halves of that group are ENGINE, see the module doc).
#[test]
fn a_px_and_fill_track_grid_builds() {
    ok("View v {\n Grid g cols=[40, fill] {\n  row { Rectangle a min_size=1x1; Rectangle b min_size=1x1 }\n }\n}\n");
}

const NESTED: &str = "widget Chip $label=\"x\" {\n Label c text=$label\n}\nView v {\n Grid g cols=[hug, hug], row_height=40, width=100, height=40, halign=left, valign=top {\n  row { Grid inner cols=[hug] { row { Rectangle ia min_size=12x12 } }; Chip cell2 }\n }\n}\n";

/// `_grid_tests` 11332-11335: a nested grid and a widget instance are
/// cells like any other — the instance lands in track 1, right after the
/// inner grid's 12 px.
#[test]
fn nested_grid_and_widget_instance_are_cells() {
    let (p, r) = lay(NESTED, 400.0, 400.0);
    assert_eq!(rect(&p, &r, "cell2").x, 12.0, "{r:?}");
}

/// `_grid_tests` 11341: `row_height=40` on the grid is every row's height
/// when the row does not say its own.
#[test]
fn row_height_on_the_grid_is_the_default_row_height() {
    let (p, r) = lay(NESTED, 400.0, 400.0);
    assert_eq!(rect(&p, &r, "inner").h, 40.0, "{r:?}");
}

const AREAS: &str = "View v {\n Grid g cols=[hug, fill], width=200, height=150, halign=left, valign=top,\n        areas=[\"header header : 30\",\n               \"nav    body   : fill\",\n               \"footer footer : 20\"] {\n  Rectangle header min_size=10x10\n  Rectangle nav min_size=25x10\n  Rectangle body min_size=10x10\n  Rectangle footer min_size=10x10\n }\n}\n";

/// `_grid_tests` 11358-11374: the areas form places cells by name; each
/// row string carries its height after the colon (`: 30`, `: fill`).
#[test]
fn areas_form_places_cells_by_name_with_row_heights() {
    let (p, r) = lay(AREAS, 400.0, 400.0);
    assert_eq!(rect(&p, &r, "header"), Rect::new(0.0, 0.0, 200.0, 30.0), "{r:?}");
    assert_eq!(rect(&p, &r, "nav"), Rect::new(0.0, 30.0, 25.0, 100.0), "{r:?}");
    assert_eq!(rect(&p, &r, "body"), Rect::new(25.0, 30.0, 175.0, 100.0), "{r:?}");
    assert_eq!(rect(&p, &r, "footer"), Rect::new(0.0, 130.0, 200.0, 20.0), "{r:?}");
}

/// `_grid_tests` 11381-11384: a `.` hole in an areas row is fine; an area
/// name no child binds to is a warning naming it, not an error.
#[test]
fn areas_hole_is_fine_and_an_unbound_area_warns() {
    ok(&in_view("Grid g cols=[hug, hug], areas=[\"a . : hug\"] { Rectangle a min_size=10x10 }"));
    let w = warnings(&in_view("Grid g cols=[hug, hug], areas=[\"a b : hug\"] { Rectangle a min_size=10x10 }"));
    assert!(w.iter().any(|m| m.contains("`b`")), "{w:?}");
}

/// `_grid_tests` 11390-11400, 11402-11434, 11440-11448: the grid shape
/// fences — `cols=` required and a list, `auto`/`hug(2)`/`fill(0)`/an
/// unknown word refused as tracks; a row must cover exactly its tracks
/// (spans counted, `skip` written under a `row_span`, no span past the
/// last row); `skip`/`row` only where they belong, `Row {}` hinted to
/// `row {}`, rows unnamed, undrawable, `height=`-only; a cell's own
/// `width=fill(2)` and a `col_span` outside a grid refused; `cols=`
/// never from a style; no `when` between cells.
#[test]
fn grid_shape_fences_refuse_bad_structure() {
    let bad: &[&str] = &[
        "Grid g { row { Rectangle r } }",
        "Grid g cols=3 { }",
        "Grid g cols=[auto] { }",
        "Grid g cols=[hug(2)] { }",
        "Grid g cols=[fill(0)] { }",
        "Grid g cols=[stretch] { }",
        "Grid g cols=[hug, hug] { row { Rectangle a; Rectangle b; Rectangle c } }",
        "Grid g cols=[hug, hug] { row { Rectangle a; Rectangle b col_span=2 } }",
        "Grid g cols=[hug, hug] { row { Rectangle a; Rectangle e row_span=2 } row { Rectangle b; Rectangle c } }",
        "Grid g cols=[hug] { row { Rectangle a row_span=2 } }",
        "Grid g cols=[hug] { row { skip x } }",
        "Grid g cols=[hug] { skip }",
        "skip",
        "Column { row { } }",
        "row { }",
        "Grid g cols=[hug] { Rectangle r }",
        "Grid g cols=[hug] { Row { } }",
        "Grid g cols=[hug] { row { row { } } }",
        "Grid g cols=[hug] { row hdr { } }",
        "Grid g cols=[hug] { row background=red { } }",
        "Grid g cols=[hug] { row width=5 { } }",
        "Grid g cols=[hug] { row { Rectangle r width=fill(2) } }",
        "Rectangle r col_span=2",
        "Grid g cols=[hug] { row { when held } }",
    ];
    for src in bad {
        let src = in_view(src);
        assert!(fails(&src), "must not build: {src:?} → {:?}", codes(&src));
    }
    let styled = "style S cols=[hug]\nView v { Grid g .S cols=[hug] { } }\n";
    assert!(fails(styled), "must not build: {styled:?} → {:?}", codes(styled));
}

/// `_grid_tests` 11436-11438: `columns=` and `rows=` are signposts — an
/// error pointing at `cols=[...]` and at heights travelling with rows.
#[test]
fn columns_and_rows_are_signposted_errors() {
    let columns = in_view("Grid g columns=[hug] { }");
    assert!(fails(&columns), "{:?}", codes(&columns));
    let rows = in_view("Grid g cols=[hug], rows=[40] { }");
    assert!(fails(&rows), "{:?}", codes(&rows));
}

/// `_grid_tests` 11451-11481: the areas-form fences — no mixing with
/// `row {}`, areas must be rectangles, name exactly the tracks, bind to a
/// named child by a known name, one `:` and a real height per row, holes
/// spelled `.` (not `skip`), at least one row, no `row_height=` beside.
#[test]
fn areas_fences_refuse_bad_area_strings() {
    let bad: &[&str] = &[
        "Grid g cols=[hug], areas=[\"a : hug\"] { row { Rectangle a } }",
        "Grid g cols=[hug, hug], areas=[\"a . : hug\", \". a : hug\"] { Rectangle a }",
        "Grid g cols=[hug, hug], areas=[\"a b c : hug\"] { }",
        "Grid g cols=[hug], areas=[\"header : hug\"] { Rectangle hdr }",
        "Grid g cols=[hug], areas=[\"a : hug\"] { Rectangle }",
        "Grid g cols=[hug], areas=[\"a : 1 : 2\"] { Rectangle a }",
        "Grid g cols=[hug], areas=[\"a : wat\"] { Rectangle a }",
        "Grid g cols=[hug], areas=[\"a : hug\"] { skip }",
        "Grid g cols=[hug], areas=[] { }",
        "Grid g cols=[hug], areas=[\"a\"] { Rectangle a }",
        "Grid g cols=[hug], row_height=20, areas=[\"a : 10\"] { Rectangle a }",
    ];
    for src in bad {
        let src = in_view(src);
        assert!(fails(&src), "must not build: {src:?} → {:?}", codes(&src));
    }
}

/// `_grid_tests` 11484: `Canvas` is not a word — Godot redirected it to
/// `Layers`; Rust has no alias table (gogo-design.md §7 "Retired-spelling
/// redirects"), so it is the unknown-node-type error G7011.
#[test]
fn canvas_is_an_unknown_node_type() {
    let src = in_view("Canvas c");
    assert!(codes(&src).contains(&"G7011".to_string()), "{:?}", codes(&src));
}

/// `_grid_tests` 11487: an empty grid is legal.
#[test]
fn an_empty_grid_is_legal() {
    ok("View v { Grid g cols=[hug] { } }\n");
    let (p, r) = lay("View v { Grid g cols=[hug] { } }\n", 100.0, 100.0);
    assert_eq!(rect(&p, &r, "g").w, 100.0, "{r:?}");
}

/// `_grid_tests` 11495-11515: an areas error points at the offending row
/// string's own source line (a bad name on line 3, a bad height on line
/// 1, a non-rectangle on line 2, an escaped `\n` row falling back to the
/// prop's line). Godot also pins the COLUMN; Rust diagnostics carry a
/// line only, so only the line is pinned here.
#[test]
fn areas_errors_point_at_the_row_strings_line() {
    // `in_view` puts the snippet on line 2, so every Godot line is + 1.
    let line_of = |src: &str| -> Vec<u32> { build(src).diags.iter().filter(|d| d.severity == gogo::Severity::Error).map(|d| d.line).collect() };
    let name = in_view("Grid g cols=[hug, hug],\n        areas=[\"a  a : hug\",\n               \"a 9bad : hug\"] { Rectangle a }");
    assert_eq!(line_of(&name), vec![4], "{:?}", build(&name).diags);
    let height = in_view("Grid g cols=[hug], areas=[\"a : wat\"] { Rectangle a }");
    assert_eq!(line_of(&height), vec![2], "{:?}", build(&height).diags);
    let non_rect = in_view("Grid g cols=[hug, hug],\n        areas=[\"a . : hug\",\n               \". a : hug\"] { Rectangle a }");
    assert_eq!(line_of(&non_rect), vec![3], "{:?}", build(&non_rect).diags);
    let escaped = in_view("Grid g cols=[hug], areas=[\"a : hug\\n9bad : hug\"] { Rectangle a }");
    assert_eq!(line_of(&escaped), vec![2], "{:?}", build(&escaped).diags);
}

/// `_grid_tests` 11522-11540 (D89 dead-space lint, areas form): a
/// `height=fill` grid whose rows are all fixed warns "no row takes the
/// leftover", a `width=fill` grid with no fill column warns for columns;
/// a fill row / an unsized grid stays quiet.
#[test]
fn dead_space_lint_on_the_areas_form() {
    let w = warnings(&in_view("Grid g cols=[hug], height=fill, areas=[\"a : 40\"] { Rectangle a min_size=10x10 }"));
    assert!(w.iter().any(|m| m.contains("no row takes the leftover")), "{w:?}");
    let w = warnings(&in_view("Grid g cols=[40], width=fill, areas=[\"a : hug\"] { Rectangle a min_size=10x10 }"));
    assert!(w.iter().any(|m| m.contains("no column takes the leftover")), "{w:?}");
    let w = warnings(&in_view("Grid g cols=[fill], width=fill, height=fill, areas=[\"a : fill\"] { Rectangle a min_size=10x10 }"));
    assert!(!w.iter().any(|m| m.contains("takes the leftover")), "{w:?}");
    let w = warnings(&in_view("Grid g cols=[hug], areas=[\"a : 40\"] { Rectangle a min_size=10x10 }"));
    assert!(!w.iter().any(|m| m.contains("takes the leftover")), "{w:?}");
}

/// `_grid_tests` 11547: a `height=fill` grid with a `row height=fill`
/// builds with no dead-space warning.
#[test]
fn a_fill_row_in_a_fill_grid_is_quiet() {
    let w = warnings("View v { Grid g cols=[hug], height=fill { row height=fill { Rectangle r min_size=10x10 } } }\n");
    assert!(!w.iter().any(|m| m.contains("takes the leftover")), "{w:?}");
}

/// `_grid_tests` 11553: a `height=fill` grid whose only row is a fixed 20
/// warns that no row takes the leftover (the row form gets the D89 lint
/// too).
#[test]
fn a_fill_grid_with_only_fixed_rows_warns() {
    let w = warnings("View v { Grid g cols=[hug], height=fill { row height=20 { Rectangle r min_size=10x10 } } }\n");
    assert!(w.iter().any(|m| m.contains("no row takes the leftover")), "{w:?}");
}

// ----------------------------------------------------------------- Tiles

/// `_tiles_tests` 11564-11568, 11572-11581: the Tiles fences — `cols=`
/// required, a whole number (not a track list, not 2.5); `span=` within
/// the lattice and never beside `full_row=true`; `full_row=` a bool.
#[test]
fn tiles_fences_refuse_bad_cols_and_spans() {
    let bad: &[&str] = &[
        "Tiles t { Rectangle a min_size=10x10 }",
        "Tiles t cols=[fill, fill] { }",
        "Tiles t cols=2.5 { }",
        "Tiles t cols=2 { Rectangle a span=3, min_size=1x1 }",
        "Tiles t cols=3 { Rectangle a full_row=true, span=2, min_size=1x1 }",
        "Tiles t cols=2 { Rectangle a full_row=1, min_size=1x1 }",
    ];
    for src in bad {
        let src = in_view(src);
        assert!(fails(&src), "must not build: {src:?} → {:?}", codes(&src));
    }
}

/// `_tiles_tests` 11570, 11575: `cols=` belongs on a Grid or Tiles and
/// `span=` on a direct Tiles child — anywhere else is an error.
#[test]
fn cols_and_span_are_refused_off_their_kinds() {
    let label = in_view("Label a cols=2");
    assert!(fails(&label), "{:?}", codes(&label));
    let column = in_view("Column c { Rectangle a span=2, min_size=1x1 }");
    assert!(fails(&column), "{:?}", codes(&column));
}

const LATTICE: &str = "View v {\n Tiles t cols=2, width=200, height=hug, halign=left, valign=top {\n  Rectangle hdr full_row=true, min_size=10x16\n  Rectangle a min_size=10x40\n  Rectangle b min_size=10x30\n  Rectangle w span=2, min_size=10x25\n  Rectangle c min_size=10x20\n }\n}\n";

/// `_tiles_tests` 11595-11627: the lattice — unit = width / cols, a
/// `full_row` cell takes the row, cells keep their OWN heights top-aligned
/// in the row, a `span=2` that does not fit wraps whole, the next row
/// starts at the tallest cell; the total height is 101; writing `cols=4`
/// from code re-packs live.
#[test]
fn tiles_lattice_units_full_row_span_wrap_and_cols_flip() {
    let c = ok(LATTICE);
    let page = c.page;
    let t = page.find("t").unwrap();
    let mut rt = Runtime::new(&page);
    let r = relay(&page, &rt, 400.0, 400.0);
    assert_eq!(rect(&page, &r, "hdr"), Rect::new(0.0, 0.0, 200.0, 16.0), "{r:?}");
    assert_eq!(rect(&page, &r, "a"), Rect::new(0.0, 16.0, 100.0, 40.0), "{r:?}");
    assert_eq!(rect(&page, &r, "b"), Rect::new(100.0, 16.0, 100.0, 30.0), "own height, top-aligned: {r:?}");
    assert_eq!(rect(&page, &r, "w"), Rect::new(0.0, 56.0, 200.0, 25.0), "{r:?}");
    assert_eq!(rect(&page, &r, "c"), Rect::new(0.0, 81.0, 100.0, 20.0), "{r:?}");
    assert_eq!(rect(&page, &r, "t").h, 101.0, "{r:?}");
    rt.set_prop(t, PropKey::Cols, Value::VNum(4.0));
    let r = relay(&page, &rt, 400.0, 400.0);
    let a = rect(&page, &r, "a");
    assert!(a == Rect::new(0.0, 16.0, 50.0, 40.0) && rect(&page, &r, "b").x == 50.0 && rect(&page, &r, "w").x == 100.0, "{r:?}");
}

/// `_tiles_tests` 11644-11658: a span that cannot fit wraps whole and the
/// vacated tail stays EMPTY (no backfill); the next unit cell shares the
/// span's row (measured shipped truth: top.effects.3).
#[test]
fn a_span_that_does_not_fit_wraps_whole_without_backfill() {
    let (p, r) = lay("View v {\n Tiles t cols=3, width=300, height=100, halign=left, valign=top {\n  Rectangle a min_size=10x10\n  Rectangle b min_size=10x10\n  Rectangle w span=2, min_size=10x12\n  Rectangle c min_size=10x10\n }\n}\n", 400.0, 400.0);
    let b = rect(&p, &r, "b");
    let w = rect(&p, &r, "w");
    assert!(b.x == 100.0 && b.y == 0.0 && w == Rect::new(0.0, 10.0, 200.0, 12.0), "{b:?} {w:?}");
    let c = rect(&p, &r, "c");
    assert!((c.x, c.y) == (200.0, 10.0), "{c:?}");
}

const FIXED: &str = "View v {\n Tiles t cols=3, width=300, height=200, halign=left, valign=top {\n  Rectangle a full_row=true, min_size=10x20\n  Rectangle b full_row=true, min_size=10x20\n  Rectangle c full_row=true, min_size=10x20\n  Rectangle d full_row=true, min_size=10x20\n }\n}\n";

/// `_tiles_tests` 11673: four full-row cells build.
#[test]
fn full_row_cells_build() {
    ok(FIXED);
}

/// `_tiles_tests` 11686-11692: a cell given a fixed size from code
/// (Godot `set_cell fixed_w/fixed_h` — the shipped icon-size flip) packs
/// at its OWN pitch: four 66×66 squares sit at x 0, 66, 132, 198 in a
/// 3-column lattice; clearing the fixed size returns the cell to its
/// full-row 300×20.
#[test]
#[ignore = "NEW: Godot fixed-size cells pack at their own pitch (b.x 66) vs Rust cells always take unit×span (a width write shrinks the box, b.x stays 100)"]
fn fixed_size_cells_pack_at_their_own_pitch() {
    let c = ok(FIXED);
    let page = c.page;
    let mut rt = Runtime::new(&page);
    for n in ["a", "b", "c", "d"] {
        let id = page.find(n).unwrap();
        rt.set_prop(id, PropKey::FullRow, Value::VBool(false));
        rt.set_prop(id, PropKey::Size, Value::VSize(Some(66.0), Some(66.0)));
    }
    let r = relay(&page, &rt, 400.0, 400.0);
    let a = rect(&page, &r, "a");
    assert!(a == Rect::new(0.0, 0.0, 66.0, 66.0) && rect(&page, &r, "b").x == 66.0 && rect(&page, &r, "d").x == 198.0, "{r:?}");
    let a_id = page.find("a").unwrap();
    rt.set_prop(a_id, PropKey::Size, Value::VSize(None, None));
    rt.set_prop(a_id, PropKey::FullRow, Value::VBool(true));
    let r = relay(&page, &rt, 400.0, 400.0);
    let a = rect(&page, &r, "a");
    assert!((a.w, a.h) == (300.0, 20.0), "{a:?}");
}

/// `_tiles_tests` 11700-11712: `aspect=natural` makes a bitmap-exact
/// tile — a 100×50 image spanning a 200-wide lattice is 200×100, and the
/// Tiles' own height is that 100.
#[test]
fn aspect_natural_image_tile_is_bitmap_exact() {
    let c = ok("View v {\n Tiles t cols=2, width=200, height=hug, halign=left, valign=top {\n  Image pic span=2, aspect=natural, source=\"res://pic.png\"\n }\n}\n");
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let r = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &Pic);
    let pic = rect(&c.page, &r, "pic");
    assert_eq!((pic.w, pic.h), (200.0, 100.0), "{pic:?}");
    assert_eq!(rect(&c.page, &r, "t").h, 100.0, "{r:?}");
}

/// `_tiles_tests` 11735-11747 (two user-caught regressions, 2026-07-12):
/// under a hugging parent the Tiles claims the widest cell's OWN minimum
/// (50 — not a×cols = 40), the parent sizes it 50×22, and the cells pack
/// in that width (b at 25, the full row 50×12 under them).
#[test]
fn tiles_under_a_hugging_parent_claim_the_widest_cells_own_minimum() {
    let (p, r) = lay("View v {\n Panel p halign=left, valign=top {\n  Tiles t cols=2 {\n   Rectangle a min_size=20x10\n   Rectangle b min_size=20x10\n   Rectangle w full_row=true, min_size=50x12\n  }\n }\n}\n", 400.0, 400.0);
    let t = rect(&p, &r, "t");
    assert_eq!(t.w, 50.0, "widest OWN minimum: {t:?}");
    assert_eq!((t.w, t.h), (50.0, 22.0), "{t:?}");
    let b = rect(&p, &r, "b");
    let w = rect(&p, &r, "w");
    assert!((b.x, b.y) == (25.0, 0.0) && w == Rect::new(0.0, 10.0, 50.0, 12.0), "{b:?} {w:?}");
}

/// `_tiles_tests` 11762-11772: a gone cell reflows away entirely (wrap
/// semantics, unlike Grid's kept cell) — `b` moves from 100 to 0.
#[test]
fn a_gone_tile_reflows_away() {
    let c = ok("View v {\n Tiles t cols=2, width=200, height=50, halign=left, valign=top {\n  Rectangle a min_size=10x10\n  Rectangle b min_size=10x10\n }\n}\n");
    let mut page = c.page;
    let a = page.find("a").unwrap();
    let mut rt = Runtime::new(&page);
    let r = relay(&page, &rt, 400.0, 400.0);
    assert_eq!(rect(&page, &r, "b").x, 100.0, "{r:?}");
    rt.set_visible(&mut page, a, false);
    let r = relay(&page, &rt, 400.0, 400.0);
    assert_eq!(rect(&page, &r, "b").x, 0.0, "{r:?}");
}

/// `_tiles_tests` 11787-11790: widget instances are cells; `span=` on the
/// instance line spans (two = 2 of 3 units = 200).
#[test]
fn widget_instances_are_tiles_and_span_on_the_instance_line() {
    let (p, r) = lay("widget Card $label=\"x\" {\n Panel p { Label c text=$label }\n}\nView v {\n Tiles t cols=3, width=300, height=100, halign=left, valign=top {\n  Card one label=\"a\"\n  Card two label=\"b\", span=2\n }\n}\n", 400.0, 400.0);
    assert_eq!(rect(&p, &r, "one").w, 100.0, "{r:?}");
    let two = rect(&p, &r, "two");
    assert!((two.x, two.w) == (100.0, 200.0), "{two:?}");
}

const OVERFLOW: &str = "View v {\n Tiles t cols=2, width=200, height=100, halign=left, valign=top {\n  Rectangle a min_size=150x10\n  Rectangle b min_size=20x10\n }\n}\n";

/// `_tiles_tests` 11806: a Tiles whose cell content is wider than its
/// cell builds (the overflow is a runtime warning, not a build error).
#[test]
fn an_oversized_tile_builds() {
    ok(OVERFLOW);
}

/// `_tiles_tests` 11816-11830: an oversized cell overlaps its neighbour
/// and the Tiles WARNS at layout time naming the offender and the fix
/// (`clip_text`); widening the cell (`span=2`) clears the warning; a
/// `fixed_h` under the content trips it on the y axis too. Rust has no
/// layout-time warning channel, so nothing here can be read back.
#[test]
#[ignore = "GP42: dev diagnostics (Godot Tiles.overflow_warnings) — Rust layout has no warning channel"]
fn oversized_tiles_warn_and_heal() {
    // No API to read overflow warnings from `layout`; the check fails by
    // construction until one exists.
    let c = ok(OVERFLOW);
    let overflow_warnings: Vec<String> = Vec::new();
    assert!(overflow_warnings.len() == 1 && overflow_warnings[0].contains("`a`") && overflow_warnings[0].contains("clip_text"), "{overflow_warnings:?} {:?}", c.diags);
}
