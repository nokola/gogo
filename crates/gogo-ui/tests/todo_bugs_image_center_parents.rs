//! An Image sized on both axes with no alignment written is CENTRED in a
//! container parent and sits at the start edge under a free one
//! (docs/todo-bugs.md B28 / B28-DES, 2026-09-13). The addon's rule (D28,
//! `gogo_resolver.gd` img_center) sets Godot size flags, which only a
//! Container honours; the Rust port had it for a Row or Column parent
//! only, so the clone page's 50x50 thumbnail inside `Panel padding=5` sat
//! at the top of its row. `layout::is_container` names the split;
//! `godot_lang.rs` `resolver_basics_image_semantics` keeps the Column
//! case.
use gogo_ui::layout::{FixedAdvance, Rect, is_container, layout};
use gogo_ui::page::Kind;
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn lay(src: &str) -> (Page, Vec<Rect>) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
    (c.page, rects)
}

fn rect(p: &Page, r: &[Rect], name: &str) -> Rect {
    r[(p.find(name).unwrap_or_else(|| panic!("no node {name}"))).index()]
}

fn center(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// The split mirrors the addon's alias table: the kinds that become a
/// Godot Container place their children; the rest are free parents.
#[test]
fn container_kinds_mirror_the_addons_alias_table() {
    for k in [Kind::Row, Kind::Column, Kind::Choice, Kind::Panel, Kind::ListItem, Kind::Grid, Kind::Scroll, Kind::Flow, Kind::Tiles, Kind::List] {
        assert!(is_container(k), "{k:?} is a container");
    }
    for k in [Kind::Layers, Kind::View, Kind::Button, Kind::Check, Kind::Toggle, Kind::Radio, Kind::Custom, Kind::Slot, Kind::Seat, Kind::Empty] {
        assert!(!is_container(k), "{k:?} is a free parent");
    }
}

/// The B28 shape: the thumbnail in `Panel padding=5` beside a text
/// column — its centre is the Panel's centre, which is the row's.
#[test]
fn a_both_axes_image_in_a_panel_in_a_row_is_centred() {
    let (p, r) = lay("View v {\n Row r width=fill, height=80, spacing=0, valign=top {\n  Panel pnl padding=5 {\n   Image img 50x50\n  }\n  Column col width=fill {\n   Label text=\"Clone from current photo\"\n  }\n }\n}\n");
    let img = rect(&p, &r, "img");
    let pnl = rect(&p, &r, "pnl");
    let row = rect(&p, &r, "r");
    assert_eq!(img, Rect::new(5.0, 15.0, 50.0, 50.0), "{r:?}");
    assert_eq!(center(img), center(pnl), "the Image's centre is the Panel's: {img:?} in {pnl:?}");
    assert_eq!(center(img).1, center(row).1, "and so the row's: {img:?} in {row:?}");
}

/// A Grid cell is a container child (the cell's parent is the Grid; a
/// `row { }` is no box): centred in its cell.
#[test]
fn a_both_axes_image_in_a_grid_cell_is_centred() {
    let (p, r) = lay("View v {\n Grid g cols=[fill], width=200, height=100, halign=left, valign=top {\n  row height=fill { Image img 50x50 }\n }\n}\n");
    assert_eq!(rect(&p, &r, "img"), Rect::new(75.0, 25.0, 50.0, 50.0), "{r:?}");
}

/// A Scroll's content is a container child: centred in the viewport.
#[test]
fn a_both_axes_image_in_a_scroll_is_centred() {
    let (p, r) = lay("View v {\n Scroll s width=200, height=200, halign=left, valign=top {\n  Image img 50x50\n }\n}\n");
    assert_eq!(rect(&p, &r, "img"), Rect::new(75.0, 75.0, 50.0, 50.0), "{r:?}");
}

/// A Flow is a container (FlowContainer): centred on the line's cross
/// axis — the 80-tall neighbour makes the line 80 and the Image sits at
/// y=15, not at the line's top.
#[test]
fn a_both_axes_image_in_a_flow_is_centred_on_its_line() {
    let (p, r) = lay("View v {\n Flow f width=200, halign=left, valign=top, spacing=0 {\n  Rectangle tall width=20, height=80\n  Image img 50x50\n }\n}\n");
    assert_eq!(rect(&p, &r, "img"), Rect::new(20.0, 15.0, 50.0, 50.0), "{r:?}");
}

/// A Button is a free parent (the addon writes no anchor): the Image
/// stays at the start edge.
#[test]
fn a_both_axes_image_in_a_button_sits_at_the_start_edge() {
    let (p, r) = lay("View v {\n Button b width=200, height=200, halign=left, valign=top {\n  Image img 50x50\n }\n}\n");
    assert_eq!(rect(&p, &r, "img"), Rect::new(0.0, 0.0, 50.0, 50.0), "{r:?}");
}

/// A Layers is a free parent: the Image stays at the start edge.
#[test]
fn a_both_axes_image_in_a_layers_sits_at_the_start_edge() {
    let (p, r) = lay("View v {\n Layers l width=200, height=200, halign=left, valign=top {\n  Image img 50x50\n }\n}\n");
    assert_eq!(rect(&p, &r, "img"), Rect::new(0.0, 0.0, 50.0, 50.0), "{r:?}");
}

/// The case that always worked: a Column parent centres across its
/// width (the stack road, `godot_lang.rs` resolver_basics_image_semantics).
#[test]
fn a_both_axes_image_in_a_column_is_still_centred() {
    let (p, r) = lay("View v {\n Column c width=200, height=200, halign=left, valign=top {\n  Image img 50x50\n }\n}\n");
    assert_eq!(rect(&p, &r, "img"), Rect::new(75.0, 0.0, 50.0, 50.0), "{r:?}");
}

/// An alignment the author wrote beats the default, in a Panel as in a
/// Column: `halign=left` keeps the Image at the left edge (its height
/// still centres).
#[test]
fn a_written_alignment_beats_the_centring_default_in_a_panel() {
    let (p, r) = lay("View v {\n Panel pnl width=200, height=200, halign=left, valign=top {\n  Image img 50x50, halign=left\n }\n}\n");
    assert_eq!(rect(&p, &r, "img"), Rect::new(0.0, 75.0, 50.0, 50.0), "{r:?}");
}
