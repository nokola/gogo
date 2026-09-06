//! Gradient tiling (2026-09-05, user): `repeat=repeat|mirror` with a
//! `from=`/`to=` span makes the span ONE PERIOD and the ramp repeats
//! across the box — the draw carries `period` (start, length in run
//! units) and the stops in 0..1 of the period. Without a span the run is
//! the box as before (`repeat=mirror` folds once, `repeat=none` clamps).
//! Since D21 a gradient is ONE entry followed by its stop records.
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::runtime::{Runtime, resolve};

/// Each gradient's (stop positions, period, angle).
fn gradients(src: &str) -> Vec<(Vec<f32>, (f32, f32), f32)> {
    let c = gogo::compile(src, &|_: &str| None::<String>);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 340.0, 14.0), &FixedAdvance);
    let draws = draw(&page, &res, &rects);
    let mut out: Vec<(Vec<f32>, (f32, f32), f32)> = Vec::new();
    let mut i: usize = 0;
    while i < draws.len() {
        if let Draw::Gradient { period, angle, stops, .. } = draws[i] {
            let positions: Vec<f32> = draws[i + 1..i + 1 + stops as usize].iter().filter_map(|d| if let Draw::GradientStop { pos, .. } = d { Some(*pos) } else { None }).collect();
            out.push((positions, period, angle));
            i += stops as usize;
        }
        i += 1;
    }
    out
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

fn same(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| near(*x, *y))
}

#[test]
fn a_mirrored_span_tiles_with_the_span_as_the_period() {
    // The progress shimmer: a 20-epx period at 3 degrees in a 340x14 box.
    let g = gradients("View v { Gradient g width=340, height=14, repeat=mirror, from=0x0, to=0.05874x0.07477, stops=[#252B32/0@0, #252B32/0@0.2, #252B32A0@0.5] }");
    assert_eq!(g.len(), 1, "one draw: {g:?}");
    let (stops, period, angle) = &g[0];
    assert!(near(period.0, 0.0), "the period starts at the span's start: {period:?}");
    assert!(period.1 > 0.0, "a tiled gradient has a period: {period:?}");
    assert!((angle - 93.0).abs() < 0.2, "the span's direction is the compass angle: {angle}");
    // The fold: the stops up to 0.5, then their mirror back to 1 —
    // period-local positions.
    assert!(same(stops, &[0.0, 0.2, 0.5, 0.5, 0.8, 1.0]), "{stops:?}");
}

#[test]
fn without_a_span_or_without_repeat_nothing_tiles() {
    let folded = gradients("View v { Gradient g width=340, height=14, repeat=mirror, stops=[white, black] }");
    assert!(folded[0].1.1 == 0.0, "no span = the box is the run: {folded:?}");
    assert!(same(&folded[0].0, &[0.0, 0.5, 0.5, 1.0]), "the one fold stays: {folded:?}");
    let clamped = gradients("View v { Gradient g width=340, height=14, from=0x0, to=0.25x0, stops=[white, black] }");
    assert!(clamped[0].1.1 == 0.0, "repeat=none with a span clamps, as before: {clamped:?}");
    assert!(same(&clamped[0].0, &[0.0, 0.25]), "the span remaps the stops: {clamped:?}");
    let repeated = gradients("View v { Gradient g width=340, height=14, repeat=repeat, from=0x0, to=0.25x0, stops=[white, black] }");
    assert!(near(repeated[0].1.0, 0.0) && near(repeated[0].1.1, 0.25) && same(&repeated[0].0, &[0.0, 1.0]), "repeat: one ramp per quarter box, period-local stops: {repeated:?}");
}
