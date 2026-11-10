//! `draw_subtree` (todo-rust 2.8.3): one node's draw list is the same run
//! of entries `draw` gives it — so a text callout's bake draws through the
//! screen's walk — starting at full opacity (an ancestor's `opacity=` is
//! the screen's, not the subtree's), and empty for a node with no rect.
use gogo_ui::draw::{Draw, draw, draw_subtree};
use gogo_ui::KeptFrame;
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{PropKey, Value};
use gogo_ui::runtime::{Runtime, resolve_animated};
use gogo_ui::{Compiled, NodeId, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn named(c: &Compiled, name: &str) -> NodeId {
    (0..c.page.nodes.len()).map(|i| NodeId::new(i as u32)).find(|n| c.page.str(c.page.nodes[*n].name) == name).unwrap_or_else(|| panic!("node {name}"))
}

const SRC: &str = r#"
View root width=200, height=200 {
    Panel dim width=200, height=100, opacity=0.5 {
        Panel plate width=80, height=40, margin_left=10, margin_top=10, background=#FF0000FF {
            Label word text="x", font_color=#00FF00FF
        }
    }
    Panel other width=200, height=100, background=#0000FFFF
}
"#;

#[test]
fn a_subtree_is_the_screen_walks_run_for_that_node() {
    let c = ok(SRC);
    let mut rt = Runtime::new(&c.page);
    // Rotation is MOTION — a code write (the callout's `set_geometry` road)
    // or a `when` response — and only the animated resolve carries it.
    rt.set_prop(named(&c, "plate"), PropKey::Rotate, Value::VNum(30.0));
    let f = KeptFrame::fresh(&c.page, resolve_animated(&c.page, &mut rt), Rect::new(0.0, 0.0, 200.0, 200.0), &FixedAdvance);
    let full: Vec<Draw> = draw(&c.page, f.frame());
    let whole: Vec<Draw> = draw_subtree(&c.page, f.frame(), NodeId::new(0));
    assert_eq!(whole, full, "the root's subtree IS the page's list");

    let other: Vec<Draw> = draw_subtree(&c.page, f.frame(), named(&c, "other"));
    assert_eq!(other.len(), 1);
    assert!(full.ends_with(&other), "a sibling at full opacity is the tail run of the page's list");

    // The rotated plate: its RotatePush leads, its pop closes — the run
    // the bake edits for the view's rotation.
    let plate: Vec<Draw> = draw_subtree(&c.page, f.frame(), named(&c, "plate"));
    assert!(matches!(plate.first(), Some(Draw::RotatePush { angle, .. }) if (*angle - 30.0).abs() < 1e-3), "{plate:?}");
    assert!(matches!(plate.last(), Some(Draw::RotatePop)), "{plate:?}");
    // The kept frame's door (a host's texture bake) is the same run.
    let mut kept_run: Vec<Draw> = vec![Draw::RotatePop];
    f.subtree_draws(&c.page, named(&c, "plate"), &mut kept_run);
    assert_eq!(kept_run, plate, "cleared first, then the plate's run");
    // Its plate quad: full red here, half red on screen (the dim ancestor).
    let sub_quad = plate.iter().find_map(|d| if let Draw::Quad { color, .. } = d { Some(color.0) } else { None }).expect("the plate quad");
    assert_eq!(sub_quad, [0xFF, 0, 0, 0xFF]);
    let screen_quad = full.iter().find_map(|d| if let Draw::Quad { color, .. } = d { if color.0[0] == 0xFF && color.0[2] == 0 { Some(color.0) } else { None } } else { None }).expect("the plate quad on screen");
    assert_eq!(screen_quad, [0xFF, 0, 0, 128], "the ancestor's opacity is the screen's business");
    // The word under it draws in both lists.
    assert!(plate.iter().any(|d| matches!(d, Draw::Text { .. })));

    assert!(draw_subtree(&c.page, KeptFrame::default().frame(), NodeId::new(0)).is_empty(), "no rects, no draws");
}
