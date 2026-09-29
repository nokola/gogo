//! A bare Image (no size word, no `aspect=`) in a Column is as wide as
//! the Column and as tall as the picture's shape makes it at that width
//! — WPF's `Stretch=Uniform` in a vertical StackPanel. Before, the Column
//! asked it how tall it was, got 0, and the picture vanished: the What's
//! New page's `Image source="…/17_butterflies.jpg"` showed nothing once
//! its `width=`/`height=` were removed (2026-09-29).
//!
//! Under a Panel or a Button the bare Image is still told its size and
//! the picture fits inside: a big picture must never make the welcome
//! screen's panel taller than the window.
//!
//! The picture's own size is spelled `width=hug, height=hug`: an Image's
//! content is its picture.
//!
//! The picture is 80x40 everywhere here (ratio 2).
use gogo::layout::{FixedAdvance, Rect, TextMeasure, layout};
use gogo::runtime::{Runtime, resolve};
use gogo::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

/// A text ruler that knows one picture: every image is 80x40 px.
struct Wide;

impl TextMeasure for Wide {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        FixedAdvance.measure(text, font_size, max_width, font)
    }
    fn image_size(&self, _key: gogo::ImageKey) -> Option<(f32, f32)> {
        Some((80.0, 40.0))
    }
}

fn lay(src: &str, window: Rect) -> (Page, Vec<Rect>) {
    let c = ok(src);
    let rt = Runtime::new(&c.page);
    let res = resolve(&c.page, &rt);
    let rects = layout(&c.page, &res, window, &Wide);
    (c.page, rects)
}

fn rect(p: &Page, r: &[Rect], name: &str) -> Rect {
    r[(p.find(name).unwrap_or_else(|| panic!("no node {name}"))).index()]
}

const WINDOW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 400.0 };

/// The What's New shape: the picture takes the Column's width and its
/// own shape's height, and the next line starts under it.
#[test]
fn a_bare_image_in_a_column_is_the_column_wide_and_follows_the_picture() {
    let (p, r) = lay("View v {\n Column c width=100, halign=left, valign=top, spacing=0 {\n  Image i source=\"res://pic.png\"\n  Rectangle below height=10\n }\n}\n", WINDOW);
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 100.0, 50.0), "{r:?}");
    assert_eq!(rect(&p, &r, "below").y, 50.0, "{r:?}");
}

/// A box that asks its Column how tall to be counts the picture too (the
/// What's New page: Scroll → Panel → Column).
#[test]
fn a_hugging_panel_around_the_column_counts_the_picture() {
    let (p, r) = lay("View v {\n Panel outer width=200, halign=left, valign=top {\n  Column c spacing=0 {\n   Image i source=\"res://pic.png\"\n  }\n }\n}\n", WINDOW);
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 200.0, 100.0), "{r:?}");
    assert_eq!(rect(&p, &r, "outer").h, 100.0, "{r:?}");
}

/// An alignment is not a size: a centered bare Image still takes the
/// Column's width (the picture fills it, so there is nothing to center).
#[test]
fn a_centered_bare_image_in_a_column_still_takes_the_width() {
    let (p, r) = lay("View v {\n Column c width=100, halign=left, valign=top {\n  Image i source=\"res://pic.png\", halign=center\n }\n}\n", WINDOW);
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 100.0, 50.0), "{r:?}");
}

/// Any size word keeps the old road: `min_height=32` (the toolbar's
/// icons) is 32 tall, not the picture's height.
#[test]
fn an_image_with_a_size_word_keeps_its_own_size() {
    let (p, r) = lay("View v {\n Column c width=100, halign=left, valign=top {\n  Image i source=\"res://pic.png\", min_height=32\n }\n}\n", WINDOW);
    assert_eq!(rect(&p, &r, "i").h, 32.0, "{r:?}");
}

/// The welcome screen in landscape: a bare Image filling a Button in a
/// 400x100 window. At 400 wide the picture's shape would ask for 200
/// tall; the Button is told the window, so both stay 100 tall and the
/// picture fits inside when drawn.
#[test]
fn a_bare_image_under_a_button_never_grows_it_past_the_window() {
    let (p, r) = lay("View v {\n Button b {\n  Image i source=\"res://pic.png\"\n }\n}\n", Rect::new(0.0, 0.0, 400.0, 100.0));
    assert_eq!(rect(&p, &r, "b"), Rect::new(0.0, 0.0, 400.0, 100.0), "{r:?}");
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 400.0, 100.0), "{r:?}");
}

/// `width=hug, height=hug` is the picture's own size, one picture pixel
/// per layout unit — the way to write "natural size" (2026-09-29). In a
/// Column the next line starts under it.
#[test]
fn a_hug_image_is_its_pictures_own_size() {
    let (p, r) = lay("View v {\n Column c width=300, halign=left, valign=top, spacing=0 {\n  Image i source=\"res://pic.png\", width=hug, height=hug, halign=left\n  Rectangle below height=10\n }\n}\n", WINDOW);
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 80.0, 40.0), "{r:?}");
    assert_eq!(rect(&p, &r, "below").y, 40.0, "{r:?}");
}

/// One side `hug`: that side is the picture's, the other takes what the
/// parent gives (a 400x100 Button: 80 wide, 100 tall).
#[test]
fn a_hug_width_alone_takes_the_pictures_width_only() {
    let (p, r) = lay("View v {\n Button b {\n  Image i source=\"res://pic.png\", width=hug, halign=left\n }\n}\n", Rect::new(0.0, 0.0, 400.0, 100.0));
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 80.0, 100.0), "{r:?}");
}

/// Only the written word reads the picture: an alignment alone (which
/// also hugs) keeps the old 0, so the caption preview's centering and
/// the mask tiles' `halign=left` never pick up a picture's size.
#[test]
fn an_alignment_alone_does_not_read_the_pictures_size() {
    let (p, r) = lay("View v {\n Layers l width=200, height=200, halign=left, valign=top {\n  Image i source=\"res://pic.png\", halign=center, valign=center\n }\n}\n", WINDOW);
    assert_eq!((rect(&p, &r, "i").w, rect(&p, &r, "i").h), (0.0, 0.0), "{r:?}");
}

/// `size=hug` is `width=hug, height=hug` — the user's own spelling on
/// the What's New page. Before, `size=` took only `WxH` numbers and a
/// word compiled with no error and did nothing.
#[test]
fn size_hug_is_both_sides_hug() {
    let (p, r) = lay("View v {\n Column c width=300, halign=left, valign=top, spacing=0 {\n  Image i source=\"res://pic.png\", size=hug, halign=left\n }\n}\n", WINDOW);
    assert_eq!(rect(&p, &r, "i"), Rect::new(0.0, 0.0, 80.0, 40.0), "{r:?}");
}

/// A word `size=` does not know is an error, not a silent no-op.
#[test]
fn an_unknown_size_word_is_an_error() {
    let c = compile("View v {\n Image i source=\"res://pic.png\", size=natural\n}\n", &|_| None);
    assert!(c.diags.iter().any(|d| d.code == "G0056"), "{:#?}", c.diags);
}
