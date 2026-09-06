//! Gradient tiling (2026-09-05, user): `repeat=repeat|mirror` with a
//! `from=`/`to=` span makes the span ONE PERIOD and the ramp repeats
//! across the box — the draw carries `period` (start, length in run
//! units) and the bands in 0..1 of the period. Without a span the run is
//! the box as before (`repeat=mirror` folds once, `repeat=none` clamps).
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::runtime::{Runtime, resolve};

fn gradients(src: &str) -> Vec<((f32, f32), (f32, f32), f32)> {
    let c = gogo::compile(src, &|_: &str| None::<String>);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 340.0, 14.0), &FixedAdvance);
    draw(&page, &res, &rects).iter().filter_map(|d| if let Draw::Gradient { band, period, angle, .. } = d { Some((*band, *period, *angle)) } else { None }).collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

#[test]
fn a_mirrored_span_tiles_with_the_span_as_the_period() {
    // The progress shimmer: a 20-epx period at 3 degrees in a 340x14 box.
    let g = gradients("View v { Gradient g width=340, height=14, repeat=mirror, from=0x0, to=0.05874x0.07477, stops=[#252B32/0@0, #252B32/0@0.2, #252B32A0@0.5] }");
    assert!(!g.is_empty());
    for (band, period, angle) in &g {
        assert!(near(period.0, 0.0), "the period starts at the span's start: {period:?}");
        assert!(period.1 > 0.0, "a tiled gradient has a period: {period:?}");
        assert!(band.0 >= 0.0 && band.1 <= 1.0, "bands are period-local: {band:?}");
        assert!((angle - 93.0).abs() < 0.2, "the span's direction is the compass angle: {angle}");
    }
    // The fold: an up run (0..0.5) and its mirror (0.5..1) — four bands
    // for three stops.
    assert_eq!(g.len(), 4, "{g:?}");
    assert!(g.iter().any(|(b, _, _)| near(b.0, 0.2) && near(b.1, 0.5)), "the ramp band 0.2..0.5: {g:?}");
    assert!(g.iter().any(|(b, _, _)| near(b.0, 0.5) && near(b.1, 0.8)), "its mirror 0.5..0.8: {g:?}");
}

#[test]
fn without_a_span_or_without_repeat_nothing_tiles() {
    let folded = gradients("View v { Gradient g width=340, height=14, repeat=mirror, stops=[white, black] }");
    assert!(folded.iter().all(|(_, p, _)| p.1 == 0.0), "no span = the box is the run: {folded:?}");
    assert!(folded.iter().any(|(b, _, _)| near(b.0, 0.0) && near(b.1, 0.5)) && folded.iter().any(|(b, _, _)| near(b.0, 0.5) && near(b.1, 1.0)), "the one fold stays: {folded:?}");
    let clamped = gradients("View v { Gradient g width=340, height=14, from=0x0, to=0.25x0, stops=[white, black] }");
    assert!(clamped.iter().all(|(_, p, _)| p.1 == 0.0), "repeat=none with a span clamps, as before: {clamped:?}");
    assert!(clamped.iter().any(|(b, _, _)| near(b.1, 0.25)), "the span remaps the bands: {clamped:?}");
    let repeated = gradients("View v { Gradient g width=340, height=14, repeat=repeat, from=0x0, to=0.25x0, stops=[white, black] }");
    assert!(repeated.iter().all(|(b, p, _)| near(p.0, 0.0) && near(p.1, 0.25) && near(b.0, 0.0) && near(b.1, 1.0)), "repeat: one ramp per quarter box: {repeated:?}");
}
