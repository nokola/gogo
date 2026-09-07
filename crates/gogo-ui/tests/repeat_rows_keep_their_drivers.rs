//! A pooled repeat row is a CLONE of the template (`Runtime::set_count`
//! → `Page::clone_subtree`), and a clone must carry everything a
//! control's contract baked per instance: its driven slots, its
//! `live($value)` labels, its own `transition` feel. Found 2026-09-06
//! (todo-bugs.md #6): the effect page's slider rows are such clones, and
//! their fill stayed at the template's baked fraction while the value
//! moved — the read-out changed, the bar stood still, on every device
//! and on the desktop. The same round adds `Runtime::is_moving`: the
//! frame loop's idle gate asks it AFTER the layout pass, where glides
//! are armed.

use gogo::compile;
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{NodeId, Page, PropKey, Value};
use gogo::runtime::{Runtime, resolve_animated};

const BARS: &str = "control Bar $value=0, $min=0, $max=10 {\n    behavior slider by $value in $min..$max\n    state_builtin focused, held\n    transition duration=0.2\n    Layers width=fill, height=20 {\n        slot track width=fill, height=fill {\n            slot progress automatic=length by $value in $min..$max { Rectangle width=fill, height=fill, color=#FF0000FF }\n        }\n        Label lab text=live($value)\n    }\n}\nView v {\n    Column width=fill {\n        repeat rows capacity=2 { Bar b width=fill }\n    }\n}\n";

struct Fx {
    page: Page,
    rt: Runtime,
}

impl Fx {
    fn new() -> Fx {
        let c = compile(BARS, &|_| None);
        assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let mut rt = Runtime::new(&page);
        let mut page = page;
        let rows = page.find("rows").unwrap();
        rt.set_count(&mut page, rows, 2);
        Fx { page, rt }
    }

    /// The shown fraction of `node` after the frame's arming pass, and a
    /// layout to keep the machinery honest.
    fn shown(&mut self, node: NodeId) -> f32 {
        let res = resolve_animated(&self.page, &mut self.rt);
        let _ = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        res.auto.iter().find(|(n, _)| *n == node).map(|(_, f)| *f).expect("the slot is driven")
    }
}

/// Row 1's fill follows a write on ROW 1's root (not the template's),
/// glides by the control's own feel, lands, and stays landed.
#[test]
fn a_cloned_rows_fill_follows_its_own_value_and_lands_once() {
    let mut fx = Fx::new();
    let root1 = fx.page.find("rows/1/b").expect("the second row's control");
    let progress1 = fx.page.find("rows/1/b/progress").expect("its fill");
    let progress0 = fx.page.find("rows/0/b/progress").expect("row 0's fill");
    assert_eq!(fx.shown(progress1), 0.0);
    fx.rt.set_value(&mut fx.page, root1, 5.0);
    assert_eq!(fx.shown(progress1), 0.0, "the frame the write lands, the fill still shows its rest — the feel owns the travel");
    assert!(fx.rt.is_moving(), "the glide armed in the layout pass keeps frames wanted");
    assert_eq!(fx.shown(progress0), 0.0, "row 0 is untouched");
    fx.rt.tick(&mut fx.page, 0.1);
    let mid = fx.shown(progress1);
    assert!(mid > 0.0 && mid < 0.5, "half-way: {mid}");
    fx.rt.tick(&mut fx.page, 0.2);
    assert_eq!(fx.shown(progress1), 0.5, "landed");
    assert!(!fx.rt.is_moving(), "nothing left to pump");
    // Landed means LANDED: the next frames do not re-arm from the baked rest.
    fx.rt.tick(&mut fx.page, 0.016);
    assert_eq!(fx.shown(progress1), 0.5);
    assert!(!fx.rt.is_moving(), "a landed glide must not re-arm (the clone's rest was missing once)");
}

/// The clone's `live($value)` label rewrites on its own root's write.
#[test]
fn a_cloned_rows_live_label_follows() {
    let mut fx = Fx::new();
    let root1 = fx.page.find("rows/1/b").unwrap();
    let lab1 = fx.page.find("rows/1/b/lab").unwrap();
    let lab0 = fx.page.find("rows/0/b/lab").unwrap();
    fx.rt.set_value(&mut fx.page, root1, 7.0);
    let shown = fx.rt.owned_text[lab1.index()];
    assert_ne!(shown, gogo::page::NO_STR, "the clone's label rewrote");
    assert_eq!(fx.page.str(shown), "7");
    assert_eq!(fx.rt.owned_text[lab0.index()], gogo::page::NO_STR, "row 0's label is untouched");
}

/// A code write of `min=`/`max=` on a row's root re-scales its fill: the
/// driver reads the LIVE range like the behavior does (the effect page
/// writes each filter's range at open — Saturation runs 0..1 over a
/// contract baked 0..10).
#[test]
fn a_rows_fill_reads_the_live_range() {
    let mut fx = Fx::new();
    let root1 = fx.page.find("rows/1/b").unwrap();
    let progress1 = fx.page.find("rows/1/b/progress").unwrap();
    fx.rt.set_prop(root1, PropKey::Min, Value::VNum(0.0));
    fx.rt.set_prop(root1, PropKey::Max, Value::VNum(1.0));
    fx.rt.set_value(&mut fx.page, root1, 0.7);
    fx.shown(progress1);
    fx.rt.tick(&mut fx.page, 0.3);
    assert_eq!(fx.shown(progress1), 0.7, "70% of the LIVE 0..1, not 7% of the baked 0..10");
}
