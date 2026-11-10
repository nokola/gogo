//! A repeat's rows are clones the runtime appends at the arena's END
//! (`Page::clone_subtree`), so no ancestor's index range holds them. The
//! draw walk once moved a sliding node's children by `id+1..subtree_end`
//! and the rows stayed put: on the text page the font names sat at their
//! resting spot while the settings sheet slid up under them (user, S24,
//! 2026-09-13; todo-bugs.md B37). The walk now carries the motion down
//! the placed tree as a transform, so whatever hangs under a mover moves
//! — rows, seated nodes, anything cloned after the build.

use gogo::anim::Finish;
use gogo::KeptFrame;
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::NodeId;
use gogo::runtime::{Runtime, resolve, resolve_animated};
use gogo::{Compiled, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

const SHEET: &str = "anim slideUp { sheet.translate.y from=100%, to=0, duration=1 }\nView v {\n    Layers width=fill, height=fill {\n        Panel sheet width=fill, height=100, valign=bottom {\n            Column width=fill {\n                repeat rows capacity=2 { Label r text=\"row\", height=20 }\n            }\n        }\n        Panel later width=fill, height=10, valign=top\n    }\n}\n";

fn view() -> Rect {
    Rect::new(0.0, 0.0, 200.0, 400.0)
}

/// Mid-slide, a row's drawn text sits exactly one sheet-translate below
/// its resting rect — the same displacement the sheet itself shows.
#[test]
fn cloned_rows_move_with_the_sheet_they_hang_under() {
    let c = ok(SHEET);
    let mut page = c.page;
    let sheet: NodeId = page.find("sheet").unwrap();
    let rows: NodeId = page.find("rows").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_count(&mut page, rows, 2);
    let row1: NodeId = page.find("rows/1/r").expect("the second row's label");
    assert!(row1 > page.find("later").unwrap(), "the precondition: the row is cloned past the sheet's later sibling");
    let res = resolve(&page, &rt);
    let rest: Vec<Rect> = rt.glide(layout(&page, &res, view(), &FixedAdvance));
    let rest_row: Rect = rest[row1];
    assert!(rest_row.h > 0.0, "the row laid out: {rest_row:?}");
    let slide = page.intern_lookup("slideUp");
    rt.play_id(&page, slide, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    let mut f = KeptFrame::fresh(&page, resolve_animated(&page, &mut rt), view(), &FixedAdvance);
    let ty: f32 = f.look().motion[sheet.index()].translate[1];
    assert!(ty > 1.0 && ty < 99.0, "mid-slide the sheet is part-way down: {ty}");
    f.glide(&mut rt);
    let text = page.intern_lookup("row");
    let list: Vec<Draw> = draw(&page, f.frame());
    let drawn: Vec<Rect> = list.iter().filter_map(|d| match d { Draw::Text { rect, text: t, .. } if *t == text => Some(*rect), _ => None }).collect();
    assert_eq!(drawn.len(), 2, "both rows drew their text: {drawn:?}");
    let row1_drawn: Rect = drawn[1];
    assert!((row1_drawn.y - (rest_row.y + ty)).abs() < 0.01, "row 1's text rides the sheet: drawn {row1_drawn:?}, rest {rest_row:?}, sheet translate {ty}");
    assert!((row1_drawn.x - rest_row.x).abs() < 0.01);
}
