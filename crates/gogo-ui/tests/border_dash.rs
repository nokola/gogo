//! `border_dash=[4, 1]` — the dashed border word (2026-09-05, user): a
//! non-empty list of positive lengths in border_thickness units (WPF
//! StrokeDashArray), drawn as crisp axis-aligned quads walking the
//! perimeter with the pattern continuing across corners (D257: never
//! antialiased line ends). A dashed plate keeps its fill and drops the
//! solid border; an odd list repeats to even out; bad spellings are G2009.
//! A Circle (2026-09-06, user) dashes its ring along the centerline,
//! clockwise from the top — ONE masked arc draw per dash/gap pair, never a
//! draw per dash; an Arc's `dash=` is the same mask along its sweep.
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::runtime::{Runtime, resolve};
use gogo::Color_Straight_Srgb_4xU8_RGBA;

fn draws(src: &str) -> Vec<Draw> {
    let c = gogo::compile(src, &|_: &str| None::<String>);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 100.0, 100.0), &FixedAdvance);
    draw(&page, &res, &rects)
}

fn black() -> Color_Straight_Srgb_4xU8_RGBA {
    Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255])
}

#[test]
fn a_dashed_rectangle_walks_its_perimeter_in_four_to_one_dashes() {
    let d = draws("View v { Rectangle r 24x24, halign=left, valign=top, color=#00000000, border_thickness=1, border_color=#000000FF, border_dash=[4, 1] }");
    let quads: Vec<(Rect, Color_Straight_Srgb_4xU8_RGBA, f32)> = d.iter().filter_map(|x| if let Draw::Quad { rect, color, border, .. } = x { Some((*rect, *color, *border)) } else { None }).collect();
    // The fill quad: transparent, no solid border.
    let fill = quads.iter().find(|(r, _, _)| r.w == 24.0 && r.h == 24.0).expect("the plate");
    assert_eq!(fill.2, 0.0, "a dashed plate has no solid border");
    let dashes: Vec<Rect> = quads.iter().filter(|(_, c, _)| *c == black()).map(|(r, _, _)| *r).collect();
    // Perimeter 96 epx, pattern 5 → 19 full periods + one 1-epx tail dash:
    // 20 dashes, a dash crossing a corner drawn as two quads.
    assert!(dashes.len() >= 20, "{dashes:?}");
    // Top edge: dashes at x=0,5,10,15,20 (the last exactly 4 → ends at 24);
    // the 1x1 tail on the corner is the left edge's.
    let top: Vec<&Rect> = dashes.iter().filter(|r| r.y == 0.0 && r.h == 1.0 && r.w > 1.0).collect();
    assert_eq!(top.len(), 5, "{top:?}");
    assert!((top[1].x - 5.0).abs() < 0.001 && (top[1].w - 4.0).abs() < 0.001);
    // The pattern continues around the corner: 24 = 4·5 + 4 → the top's
    // fifth dash is 4 long ending exactly at the corner, then a 1 gap
    // starts the right edge; its first dash is at y=1, 1 wide (inside).
    let right: Vec<&Rect> = dashes.iter().filter(|r| r.w == 1.0 && (r.x - 23.0).abs() < 0.001).collect();
    assert!(!right.is_empty() && (right[0].y - 1.0).abs() < 0.001, "{right:?}");
    // Every dash sits inside the box.
    assert!(dashes.iter().all(|r| r.x >= -0.001 && r.y >= -0.001 && r.x + r.w <= 24.001 && r.y + r.h <= 24.001));
}

#[test]
fn thickness_scales_the_pattern_and_an_odd_list_repeats() {
    let d = draws("View v { Panel p 40x20, halign=left, valign=top, background=#FFFFFFFF, border_thickness=2, border_color=#000000FF, border_dash=[3] }");
    let dashes: Vec<Rect> = d.iter().filter_map(|x| if let Draw::Quad { rect, color, .. } = x { (*color == black()).then_some(*rect) } else { None }).collect();
    let top: Vec<&Rect> = dashes.iter().filter(|r| r.y == 0.0).collect();
    // [3] at thickness 2 = 6-epx dashes and 6-epx gaps, 2 thick.
    assert!((top[0].w - 6.0).abs() < 0.001 && (top[0].h - 2.0).abs() < 0.001, "{:?}", top[0]);
    assert!((top[1].x - 12.0).abs() < 0.001, "gap = dash: {:?}", top[1]);
    // No dash at all without a border.
    let none = draws("View v { Panel p 40x20, background=#FFFFFFFF, border_dash=[4, 1] }");
    assert!(none.iter().all(|x| !matches!(x, Draw::Quad { color, .. } if *color == black())));
}

#[test]
fn bad_spellings_are_refused() {
    let c = gogo::compile("View v { Rectangle r 24x24, border_thickness=1, border_dash=[] }", &|_: &str| None::<String>);
    assert!(c.diags.iter().any(|d| d.code == "G2009"), "{:#?}", c.diags);
    let c = gogo::compile("View v { Rectangle r 24x24, border_thickness=1, border_dash=[4, -1] }", &|_: &str| None::<String>);
    assert!(c.diags.iter().any(|d| d.code == "G2009"), "{:#?}", c.diags);
    let c = gogo::compile("View v { Rectangle r 24x24, border_thickness=1, border_dash=4 }", &|_: &str| None::<String>);
    assert!(c.diags.iter().any(|d| d.code == "G2009"), "{:#?}", c.diags);
    let c = gogo::compile("View v { Label l text=\"x\", border_thickness=1, border_dash=[4, 1] }", &|_: &str| None::<String>);
    assert!(!gogo::has_errors(&c.diags), "any box has a plate — a dashed one too: {:#?}", c.diags);
}

#[test]
fn a_dashed_circle_is_arcs_along_its_ring() {
    let d = draws("View v { Circle c 24x24, halign=left, valign=top, color=#00000000, border_thickness=1, border_color=#000000FF, border_dash=[4, 1] }");
    // The disc keeps its fill and loses the solid ring.
    let discs: Vec<f32> = d.iter().filter_map(|x| if let Draw::Circle { border, .. } = x { Some(*border) } else { None }).collect();
    assert_eq!(discs, vec![0.0], "{d:?}");
    let arcs: Vec<(Rect, f32, f32, f32, (f32, f32, f32))> = d.iter().filter_map(|x| if let Draw::Arc { rect, from, sweep, thickness, dash, .. } = x { Some((*rect, *from, *sweep, *thickness, *dash)) } else { None }).collect();
    // ONE masked full ring: centerline radius 11.5, the dash 4 epx and the
    // period 5 epx as angles.
    let per_epx: f32 = 360.0 / (std::f32::consts::TAU * 11.5);
    assert_eq!(arcs.len(), 1, "{arcs:?}");
    let (rect, from, sweep, thickness, (offset, dash, period)) = arcs[0];
    assert!(rect.w == 24.0 && from == 0.0 && sweep == 360.0 && thickness == 1.0);
    assert!((offset - 0.0).abs() < 0.001 && (dash - 4.0 * per_epx).abs() < 0.01 && (period - 5.0 * per_epx).abs() < 0.01, "{arcs:?}");
    // Two dash/gap pairs: two masks over the same period, the second
    // offset past the first pair.
    let two = draws("View v { Circle c 24x24, halign=left, valign=top, border_thickness=1, border_color=#000000FF, border_dash=[4, 1, 2, 1] }");
    let masks: Vec<(f32, f32, f32)> = two.iter().filter_map(|x| if let Draw::Arc { dash, .. } = x { Some(*dash) } else { None }).collect();
    assert_eq!(masks.len(), 2, "{masks:?}");
    assert!((masks[0].2 - 8.0 * per_epx).abs() < 0.01 && (masks[1].0 - 5.0 * per_epx).abs() < 0.01 && (masks[1].1 - 2.0 * per_epx).abs() < 0.01, "{masks:?}");
    // Without a ring there is nothing to dash.
    let none = draws("View v { Circle c 24x24, color=#FFFFFFFF, border_dash=[4, 1] }");
    assert!(none.iter().all(|x| !matches!(x, Draw::Arc { .. })));
}

#[test]
fn an_arc_dashes_its_own_stroke() {
    // `dash=` in thickness units along the arc's sweep.
    let d = draws("View v { Arc a 40x40, halign=left, valign=top, color=#000000FF, thickness=2, from=90, sweep=180, dash=[3, 1] }");
    let arcs: Vec<(f32, f32, f32, (f32, f32, f32))> = d.iter().filter_map(|x| if let Draw::Arc { from, sweep, thickness, dash, .. } = x { Some((*from, *sweep, *thickness, *dash)) } else { None }).collect();
    let per_epx: f32 = 360.0 / (std::f32::consts::TAU * 19.0); // centerline radius 20 - 1
    assert_eq!(arcs.len(), 1, "{arcs:?}");
    let (from, sweep, thickness, (offset, dash, period)) = arcs[0];
    assert!(from == 90.0 && sweep == 180.0 && thickness == 2.0 && offset == 0.0);
    assert!((dash - 6.0 * per_epx).abs() < 0.01 && (period - 8.0 * per_epx).abs() < 0.01, "{arcs:?}");
    // A solid Arc carries no mask.
    let solid = draws("View v { Arc a 40x40, color=#000000FF, thickness=2 }");
    assert!(solid.iter().any(|x| matches!(x, Draw::Arc { dash, .. } if *dash == gogo::draw::SOLID)));
    // The word is the Arc's; a plate dashes with `border_dash=`.
    let c = gogo::compile("View v { Rectangle r 24x24, border_thickness=1, dash=[4, 1] }", &|_: &str| None::<String>);
    assert!(c.diags.iter().any(|d| d.code == "G2002"), "{:#?}", c.diags);
    let c = gogo::compile("View v { Arc a 40x40, thickness=2, dash=4 }", &|_: &str| None::<String>);
    assert!(c.diags.iter().any(|d| d.code == "G2009"), "{:#?}", c.diags);
}
