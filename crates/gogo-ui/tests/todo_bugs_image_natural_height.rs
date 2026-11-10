//! `Image height=fill, aspect=natural` derives its WIDTH from the height
//! it gets (docs/todo-bugs.md B26 / B26-DES (b), 2026-09-13). The mask
//! panel's tiles are spelled exactly so and laid out 0 wide — `image_min`
//! is asked per width and never sees the row's height, and the place
//! step worked the other axis out for a numeric `aspect=` only. The
//! addon makes this `TextureRect.EXPAND_FIT_WIDTH_PROPORTIONAL`
//! (`gogo_resolver.gd` aspect_drive): the width follows the bitmap's
//! ratio from the height.
//!
//! The picture is 80x40 everywhere here (ratio 2), so a 50-high box is
//! 100 wide.
use gogo_ui::layout::{FixedAdvance, Rect, TextMeasure, layout};
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

/// A text ruler that knows one picture: every image is 80x40 px.
struct Wide;

impl TextMeasure for Wide {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        FixedAdvance.measure(text, font_size, max_width, font)
    }
    fn image_size(&self, _key: gogo_ui::ImageKey) -> Option<(f32, f32)> {
        Some((80.0, 40.0))
    }
}

fn lay(src: &str) -> (Page, Vec<Rect>) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &Wide);
    (c.page, rects)
}

fn rect(p: &Page, r: &[Rect], name: &str) -> Rect {
    r[(p.find(name).unwrap_or_else(|| panic!("no node {name}"))).index()]
}

/// The B26 shape in a stack: a fill-height `natural` Image in a 50-high
/// Row is 100 wide at the start edge (the row grants the derived width;
/// `halign=left` seats the box, it no longer shrinks it to 0).
#[test]
fn fill_height_natural_image_in_a_row_derives_its_width() {
    let (p, r) = lay("View v {\n Row r width=fill, height=50, spacing=0, valign=top {\n  Image i source=\"res://pic.png\", height=fill, aspect=natural, halign=left\n }\n}\n");
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 100.0, 50.0), "{r:?}");
}

/// The B26 shape as the mask panel spells it: the tile Image sits in a
/// 50-high Button (a free parent, the general place road). Same answer:
/// 100x50 at the Button's start edge.
#[test]
fn fill_height_natural_image_under_a_button_derives_its_width() {
    let (p, r) = lay("View v {\n Button b width=fill, height=50, valign=top {\n  Image i source=\"res://pic.png\", height=fill, aspect=natural, halign=left\n }\n}\n");
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 100.0, 50.0), "{r:?}");
}

/// A fill-height `natural` Image in a Column takes its slot's height and
/// the derived width when aligned (the transposed case: the column's
/// leftover is the height, the width follows it).
#[test]
fn fill_height_natural_image_in_a_column_follows_its_slot() {
    let (p, r) = lay("View v {\n Column c width=200, height=100, halign=left, valign=top, spacing=0 {\n  Image i source=\"res://pic.png\", height=fill, aspect=natural, halign=left\n }\n}\n");
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 200.0, 100.0), "{r:?}");
}

/// The road that already worked and must not move: a fill-WIDTH
/// `natural` Image in a 100-wide Column is 100x50 (the width drives, the
/// height follows).
#[test]
fn fill_width_natural_image_in_a_column_still_derives_its_height() {
    let (p, r) = lay("View v {\n Column c width=100, halign=left, valign=top {\n  Image i source=\"res://pic.png\", width=fill, aspect=natural\n }\n}\n");
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 100.0, 50.0), "{r:?}");
}

/// No size at all (`effect_select.gogo`'s catalog thumbnails): today's
/// road stays — the width the parent offers drives and the height
/// follows the ratio, so a 200-wide hugging Layers is 100 tall and the
/// Image fills it.
#[test]
fn natural_image_with_no_size_still_fills_the_width_and_follows_the_ratio() {
    let (p, r) = lay("View v {\n Layers l width=200, height=hug, halign=left, valign=top {\n  Image i source=\"res://pic.png\", aspect=natural\n }\n}\n");
    assert_eq!(rect(&p, &r, "l"), Rect::new(0.0, 0.0, 200.0, 100.0), "{r:?}");
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 200.0, 100.0), "{r:?}");
}
