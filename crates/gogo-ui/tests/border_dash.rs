//! `border_dash=[4, 1]` — the dashed border word (2026-09-05, user): a
//! non-empty list of positive lengths in border_thickness units (WPF
//! StrokeDashArray), drawn as crisp axis-aligned quads walking the
//! perimeter with the pattern continuing across corners (D257: never
//! antialiased line ends). A dashed plate keeps its fill and drops the
//! solid border; an odd list repeats to even out; bad spellings are G2009.
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
    let c = gogo::compile("View v { Circle c 24x24, border_thickness=1, border_dash=[4, 1] }", &|_: &str| None::<String>);
    assert!(c.diags.iter().any(|d| d.code == "G2002"), "a Circle has no dashed border: {:#?}", c.diags);
}
