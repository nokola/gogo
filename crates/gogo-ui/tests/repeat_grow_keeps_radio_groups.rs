//! A repeat grown past its capacity (`Runtime::set_count`) adds nodes past
//! the end every per-node table had at build time; a Radio picked from
//! code walks every node to clear its group. Found on the S24 2026-09-28:
//! the stickers page (a paint-mode `choice` + the sheet's cell repeat, 67
//! tattoo cells past a capacity of 54) panicked with "index out of bounds"
//! the moment the first sticker's panel picked its mode — the radio-group
//! table had stayed at the build-time length.

use gogo::compile;
use gogo::runtime::{Runtime, SELECTED};

const SRC: &str = "View v {\n    choice mode value=a {\n        Radio a\n        Radio b\n    }\n    Column {\n        repeat rows capacity=1 { Label cell text=\"x\" }\n    }\n}\n";

#[test]
fn a_code_pick_after_a_repeat_grew_clears_its_group() {
    let c = compile(SRC, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let rows = page.find("rows").unwrap();
    let before: usize = page.nodes.len();
    rt.set_count(&mut page, rows, 5);
    assert!(page.nodes.len() > before, "the fixture: the repeat grew the arena");
    let (a, b) = (page.find("a").unwrap(), page.find("b").unwrap());
    rt.set_state(a, SELECTED, true);
    rt.set_state(b, SELECTED, true);
    assert!(rt.has(b, SELECTED) && !rt.has(a, SELECTED), "the pick of b cleared a");
}
