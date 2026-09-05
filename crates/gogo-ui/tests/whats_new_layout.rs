//! Lays out a real What's New page (tests/fixtures/whats_new.gogo, a copy
//! of the app's page + theme; images are not read at compile) and
//! pins the numbers the shipped file declares: the ok Button is 130×60
//! centered, its `face` sits 8 in from every edge (114×44).
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::runtime::{Runtime, resolve};
use std::path::Path;

#[test]
fn whats_new_ok_button_and_face() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/whats_new.gogo");
    let c = gogo::compile_file(&path).unwrap();
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let rt = Runtime::new(&page);
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 533.0, 400.0), &FixedAdvance);
    let ok = (page.find("okButton").unwrap()).index();
    let face = (page.find("face").unwrap()).index();
    assert_eq!((rects[ok].w, rects[ok].h), (130.0, 60.0));
    // The column is 493 wide minus the auto scroll bar's 4-epx column (the content scrolls).
    assert!((rects[ok].x - (20.0 + (489.0 - 130.0) / 2.0)).abs() < 0.01, "centered in the padded column: {:?}", rects[ok]);
    assert_eq!((rects[face].w, rects[face].h), (114.0, 44.0));
    assert_eq!((rects[face].x - rects[ok].x, rects[face].y - rects[ok].y), (8.0, 8.0));
    // The Scroll content is taller than the viewport: it scrolls.
    let scroll = (page.find("scroll").unwrap()).index();
    assert!(rects[scroll + 1].h > rects[scroll].h);
}
