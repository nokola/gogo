//! A box whose every color is fully transparent adds NO command to the
//! draw list (`Draw::draws_nothing`, docs/todo-perf-smudge.md/PS25).
//!
//! A Button paints a default plate, so `background=#00000000` is how a file
//! removes it — on every page. Each such plate used to reach the GPU as a
//! blended quad over its whole rect that changed no pixel: the effect list's
//! tiles were two of them each, and the list scrolled at 45 frames a second
//! instead of 60 on an Adreno 330. If the skip goes away nothing LOOKS
//! wrong — only old phones get slow again — so these count the commands.
//! The taps must not care: a transparent Button is still a Button.
use gogo::draw::{Draw, draw, hit};
use gogo::layout::{FixedAdvance, Rect};
use gogo::runtime::{Runtime, resolve};
use gogo::{Color_Straight_Srgb_4xU8_RGBA, KeptFrame};

fn draws(src: &str) -> Vec<Draw> {
    let c = gogo::compile(src, &|_: &str| None::<String>);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let rt = Runtime::new(&page);
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    draw(&page, f.frame())
}

/// The commands that reach the GPU as a quad over their rect.
fn painted(list: &[Draw]) -> Vec<&Draw> {
    list.iter().filter(|d| matches!(d, Draw::Quad { .. } | Draw::QuadCorners { .. } | Draw::Circle { .. } | Draw::Arc { .. } | Draw::Image { .. } | Draw::Text { .. } | Draw::TextStretch { .. })).collect()
}

#[test]
fn the_effect_tile_shape_draws_only_its_caption() {
    // The effect list's tile: a plate-less Button over a Rectangle that is
    // transparent unless the tile asks for a color.
    let list = draws("View v { Button b 100x100, halign=left, valign=top, background=transparent { Layers box { Rectangle tileBg color=#00000000 \n Label caption text=\"Blur\", font_color=#FFFFFFFF } } }");
    let shown = painted(&list);
    assert_eq!(shown.len(), 1, "only the caption draws: {shown:#?}");
    assert!(matches!(shown[0], Draw::Text { .. }));
    // The same tile with a color gets its rect back.
    let colored = draws("View v { Button b 100x100, halign=left, valign=top, background=transparent { Layers box { Rectangle tileBg color=#E51400FF \n Label caption text=\"Text\", font_color=#FFFFFFFF } } }");
    assert_eq!(painted(&colored).len(), 2, "{colored:#?}");
}

#[test]
fn every_kind_of_transparent_box_is_left_out() {
    for src in [
        "View v { Panel p 40x40, background=#00000000 }",
        "View v { Panel p 40x40, background=#00000000, corner_radius_top_left=8 }",
        "View v { Panel p 40x40, background=#00000000, border_thickness=2, border_color=#00000000 }",
        "View v { Rectangle r 40x40, color=#00000000 }",
        "View v { Circle c 40x40, color=#00000000 }",
        "View v { Arc a 40x40, color=#00000000, thickness=2 }",
        "View v { Label l text=\"x\", font_color=#00000000 }",
        "View v { Label l text=\"x\", opacity=0 }",
        "View v { Panel p 40x40, background=#FF0000FF, opacity=0 }",
    ] {
        let list = draws(src);
        assert!(painted(&list).is_empty(), "`{src}` draws nothing, its list: {list:#?}");
    }
}

#[test]
fn a_transparent_fill_with_a_border_that_shows_still_draws() {
    let white = Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 255]);
    let list = draws("View v { Panel p 40x40, background=#00000000, border_thickness=2, border_color=#FFFFFFFF }");
    assert!(list.iter().any(|d| matches!(d, Draw::Quad { border, border_color, .. } if *border == 2.0 && *border_color == white)), "{list:#?}");
    let ring = draws("View v { Circle c 40x40, color=#00000000, border_thickness=1, border_color=#FFFFFFFF }");
    assert!(ring.iter().any(|d| matches!(d, Draw::Circle { border, .. } if *border == 1.0)), "{ring:#?}");
    // One level of alpha is not nothing.
    let faint = draws("View v { Rectangle r 40x40, color=#00000001 }");
    assert_eq!(painted(&faint).len(), 1, "{faint:#?}");
}

#[test]
fn a_transparent_button_still_takes_the_tap() {
    let c = gogo::compile("View v { Button b 100x100, halign=left, valign=top, background=#00000000 }", &|_: &str| None::<String>);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let rt = Runtime::new(&page);
    let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    assert!(painted(&draw(&page, f.frame())).is_empty());
    assert_eq!(hit(&page, f.frame(), 50.0, 50.0, false), page.find("b"), "a tap lands on the Button though nothing of it is drawn");
    assert_eq!(hit(&page, f.frame(), 150.0, 150.0, false), None);
}
