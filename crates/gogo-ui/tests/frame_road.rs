//! The frame road (`Runtime::frame_retained`): one call per frame that
//! resolves, lays out, glides, composes the anim motion and draws, in the
//! runtime's order, into the caller's buffers. Pinned here because a host
//! once spelled the order itself and drew from the resolve taken before
//! the glide: a `%` slide on a node hidden at play resolved in the glide,
//! and the sheet drew at its seat for one frame, then jumped below the
//! screen and slid (B38, user: "the animation looks like it runs twice").

use gogo::anim::Finish;
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect};
use gogo::page::NodeId;
use gogo::runtime::{Runtime, resolve};
use gogo::{Compiled, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

const SHEET: &str = "anim slideUp { sheet.translate.y from=100%, to=0, duration=1 }\nView v {\n    Layers width=fill, height=fill {\n        Panel sheet width=fill, height=100, valign=bottom, background=#FF0000FF, visibility=gone\n        Panel top width=fill, height=20, valign=top, background=#00FF00FF\n    }\n}\n";

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

fn quad(draws: &[Draw], rgba: [u8; 4]) -> Option<Rect> {
    draws.iter().find_map(|d| match d {
        Draw::Quad { rect, color, .. } if color.0 == rgba => Some(*rect),
        _ => None,
    })
}

fn view() -> Rect {
    Rect::new(0.0, 0.0, 200.0, 400.0)
}

/// The road's list is the plain `draw` of a fresh resolve taken after the
/// glide — the one true answer, however the steps are ordered inside.
fn same_as_fresh(page: &gogo::Page, rt: &Runtime, rects: &[Rect], draws: &[Draw]) {
    let fresh = resolve(page, rt);
    let reference: Vec<Draw> = draw(page, &fresh, rects);
    assert_eq!(format!("{draws:?}"), format!("{reference:?}"), "the road's list is a fresh resolve's after the glide");
}

#[test]
fn a_sheet_hidden_at_play_first_draws_one_height_below_its_seat() {
    let c = ok(SHEET);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let (mut rects, mut draws): (Vec<Rect>, Vec<Draw>) = (Vec::new(), Vec::new());
    rt.frame_retained(&page, view(), &FixedAdvance, &mut rects, &mut draws, None);
    assert!(quad(&draws, RED).is_none(), "hidden at rest");
    let sheet: NodeId = page.find("sheet").unwrap();
    rt.set_visible(&mut page, sheet, true);
    let slide = page.intern_lookup("slideUp");
    rt.play_id(&page, slide, Finish::Nothing);
    // The frame the sheet appears: its first layout, the `%` resolved in
    // the glide — and this same call's list shows it below the screen.
    rt.frame_retained(&page, view(), &FixedAdvance, &mut rects, &mut draws, None);
    let first: Rect = quad(&draws, RED).expect("the sheet draws once shown");
    assert!((first.y - 400.0).abs() < 0.5, "first drawn frame one sheet-height below the seat (y 400): {first:?}");
    same_as_fresh(&page, &rt, &rects, &draws);
    rt.tick(&mut page, 0.5);
    rt.frame_retained(&page, view(), &FixedAdvance, &mut rects, &mut draws, None);
    let mid: Rect = quad(&draws, RED).unwrap();
    assert!(mid.y > 300.5 && mid.y < 399.5, "mid-slide: {mid:?}");
    same_as_fresh(&page, &rt, &rects, &draws);
    for _ in 0..12 {
        rt.tick(&mut page, 0.1);
    }
    rt.frame_retained(&page, view(), &FixedAdvance, &mut rects, &mut draws, None);
    let end: Rect = quad(&draws, RED).unwrap();
    assert!((end.y - 300.0).abs() < 0.5, "at rest at its seat: {end:?}");
    same_as_fresh(&page, &rt, &rects, &draws);
}

/// The `above_children` split: the page without the named subtree into
/// one buffer, that subtree into the other — both kept buffers, both the
/// same draws the whole-page road gives.
#[test]
fn the_split_road_draws_the_page_and_the_top_subtree_apart() {
    let c = ok(SHEET);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let sheet: NodeId = page.find("sheet").unwrap();
    let top: NodeId = page.find("top").unwrap();
    rt.set_visible(&mut page, sheet, true);
    let (mut rects, mut draws, mut above): (Vec<Rect>, Vec<Draw>, Vec<Draw>) = (Vec::new(), Vec::new(), Vec::new());
    rt.frame_retained(&page, view(), &FixedAdvance, &mut rects, &mut draws, Some((top, &mut above)));
    assert!(quad(&draws, RED).is_some() && quad(&draws, GREEN).is_none(), "the page without `top`: {draws:?}");
    assert!(quad(&above, GREEN).is_some() && quad(&above, RED).is_none(), "`top` alone: {above:?}");
    let whole: Vec<Draw> = draw(&page, &resolve(&page, &rt), &rects);
    assert_eq!(draws.len() + above.len(), whole.len(), "the two halves are the whole page");
}
