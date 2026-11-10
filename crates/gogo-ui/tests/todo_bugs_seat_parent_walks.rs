//! Pins for two runtime design fixes from docs/todo-bugs.md:
//!
//! - B25-DES: a running page has a DECLARED tree (the file's nesting)
//!   and a PLACED tree (after `when … x.seat = name` moves). Layout and
//!   paint always followed the placed one; the hit test and every input
//!   walk read the parent field bare and followed the declared one, so a
//!   seat move was half a move (the expanded gradient bar took no
//!   touches). Now every walk names its tree through
//!   `Page::declared_parent` / `Resolved::placed_parent`, and a source
//!   pin keeps the next walk from picking one by accident.
//! - B31-DES: a scroll offset written by indexing `rt.scroll[..]` never
//!   dirtied or clamped. `Runtime::set_scroll` dirties and asks the input
//!   tick to clamp once the node is idle; `reset_view_state` forgets the
//!   engine-held view state on a page reuse.
use gogo_ui::draw::hit;
use gogo_ui::KeptFrame;
use gogo_ui::input::{Input, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::NodeId;
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{ArrangementId, Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

/// One page, one runtime, one input machine: compile → resolve → layout →
/// touch / tick, the way the shell drives a page.
struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    frame: Rect,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = ok(src);
        let rt = Runtime::new(&c.page);
        Fx { page: c.page, rt, input: Input::new(), frame: Rect::new(0.0, 0.0, 400.0, 800.0) }
    }

    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node named {name}"))
    }

    fn lay(&self) -> KeptFrame {
        KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), self.frame, &FixedAdvance)
    }

    fn rect(&self, name: &str) -> Rect {
        self.lay().rects()[(self.id(name)).index()]
    }

    /// The node a finger at (x, y) lands on, through the real hit road.
    fn hit_at(&self, x: f32, y: f32) -> Option<NodeId> {
        let f = self.lay();
        hit(&self.page, f.frame(), x, y, false)
    }

    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) {
        let f = self.lay();
        self.input.touch(&self.page, &mut self.rt, f.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 });
    }

    fn tick(&mut self, dt: f32) -> bool {
        let f = self.lay();
        self.input.tick(&self.page, &mut self.rt, f.frame(), dt)
    }
}

fn center(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

// ------------------------------------------------------------ B25-DES

/// The source pin: only `page.rs` (the field's owner) reads the declared
/// parent bare. In runtime.rs, draw.rs and input.rs every line that
/// reads `nodes[..].parent` or walks `page.is_under(..)` must carry a
/// `declared on purpose` note on that same line; everything else goes
/// through `Page::declared_parent` or `Resolved::placed_parent` and so
/// says which tree it wants. The rule is one string test per line so
/// that a new walk fails here before it fails on a device.
#[test]
fn every_parent_walk_names_its_tree() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bare: Vec<String> = Vec::new();
    for file in ["runtime.rs", "draw.rs", "input.rs"] {
        let text: String = std::fs::read_to_string(src.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        for (i, line) in text.lines().enumerate() {
            let reads_field = line.contains("].parent") || line.contains("page.is_under(");
            if reads_field && !line.contains("declared on purpose") {
                bare.push(format!("{file}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(bare.is_empty(), "bare declared-parent reads (name the tree: Page::declared_parent with a `declared on purpose` note, or Resolved::placed_parent):\n{}", bare.join("\n"));
}

/// A button written inside `home`, seated into `dock` by the arrangement
/// while `home` goes gone: the user sees the button in the dock, so a
/// tap there lands on it. Before B25 the hit walk climbed the declared
/// tree, met the gone `home`, and the docked button took no touches.
const DOCKED_HOME_GONE: &str = "arrangement docked\nView v {\n Column home halign=left, valign=top { Button b text=\"go\" }\n Row dock halign=left, valign=bottom, height=60 { seat s }\n}\nwhen docked { b.seat = s }\nwhen docked { home.visibility = gone }\n";

#[test]
fn a_seated_button_takes_taps_when_its_declared_parent_is_gone() {
    let mut fx = Fx::new(DOCKED_HOME_GONE);
    let b = fx.id("b");
    fx.rt.go_id(ArrangementId::new(0));
    let f = fx.lay();
    let (res, _) = (f.look(), f.rects());
    assert!(res.gone(&fx.page, fx.id("home")), "the precondition: home is gone");
    let dock = fx.rect("dock");
    let rb = fx.rect("b");
    assert!(rb.w > 0.0 && rb.y >= dock.y && rb.bottom() <= dock.bottom() + 0.01, "the precondition: the button is laid out in the dock: {rb:?} in {dock:?}");
    let (x, y) = center(rb);
    assert_eq!(fx.hit_at(x, y), Some(b), "a tap on the docked button lands on it");
}

/// The mirror: the seat host is `space_only` (laid out, not drawn, no
/// input) while the button's declared parents stay visible. The user
/// sees no button there, so a tap there finds nothing — a walk up the
/// declared tree would have passed and taken an invisible tap.
const DOCKED_HOST_HIDDEN: &str = "arrangement docked\nView v {\n Column home halign=left, valign=top { Button b text=\"go\" }\n Row dock halign=left, valign=bottom, height=60 { seat s }\n}\nwhen docked { b.seat = s }\nwhen docked { dock.visibility = space_only }\n";

#[test]
fn a_button_seated_under_a_hidden_host_takes_no_taps() {
    let mut fx = Fx::new(DOCKED_HOST_HIDDEN);
    let b = fx.id("b");
    fx.rt.go_id(ArrangementId::new(0));
    let f = fx.lay();
    let (res, _) = (f.look(), f.rects());
    assert!(res.hidden(&fx.page, fx.id("dock")), "the precondition: the dock is space_only");
    assert!(!res.gone(&fx.page, fx.id("home")), "the precondition: the declared parent shows");
    let rb = fx.rect("b");
    assert!(rb.w > 0.0, "the precondition: the seated button is laid out (space_only keeps the slot): {rb:?}");
    let (x, y) = center(rb);
    assert_ne!(fx.hit_at(x, y), Some(b), "a tap over the hidden dock does not reach the seated button");
}

/// Inherited text words follow the placed tree too: a Label seated into
/// a block whose `theme font_size=30` differs from its declared block's
/// 12 draws at 30, the way the addon's theme propagation follows the
/// live tree.
const DOCKED_FONT: &str = "arrangement docked\nView v {\n Column home halign=left, valign=top {\n  theme font_size=12\n  Label lbl text=\"x\"\n }\n Row dock halign=left, valign=bottom, height=60 {\n  theme font_size=30\n  seat s\n }\n}\nwhen docked { lbl.seat = s }\n";

#[test]
fn a_seated_label_inherits_its_seat_hosts_font_size() {
    let mut fx = Fx::new(DOCKED_FONT);
    let lbl = fx.id("lbl");
    let f = fx.lay();
    let (res, _) = (f.look(), f.rects());
    assert_eq!(res.font_size(&fx.page, lbl), Some(12.0), "at home: the declared block's size");
    fx.rt.go_id(ArrangementId::new(0));
    let f = fx.lay();
    let (res, _) = (f.look(), f.rects());
    assert_eq!(res.placed_parent(&fx.page, lbl), fx.id("s"), "the precondition: seated");
    assert_eq!(res.font_size(&fx.page, lbl), Some(30.0), "seated: the host block's size");
}

// ------------------------------------------------------------ B31-DES

/// A Scroll whose content is ten times wider than its viewport.
const WIDE: &str = "View v { Scroll sv width=100, height=100, halign=left, valign=top { Row list { Rectangle wide width=1000, height=50 } } }";

/// A code-set offset past the range is clamped by the next input tick:
/// the write dirties the page at once (a raw write never did), and the
/// clamp lands on the content range, not at layout.
#[test]
fn a_code_set_offset_past_the_range_is_clamped_on_the_next_tick() {
    let mut fx = Fx::new(WIDE);
    let sv = fx.id("sv");
    fx.rt.dirty = false;
    fx.rt.set_scroll(sv, [10_000.0, 0.0]);
    assert!(fx.rt.dirty, "the write dirties the page");
    assert_eq!(fx.rt.scroll_of(sv), [10_000.0, 0.0], "stored as written until a tick can measure the range");
    fx.tick(0.016);
    let f = fx.lay();
    let (_, rects) = (f.look(), f.rects());
    let max_x = rects[fx.id("wide").index()].w - rects[sv.index()].w;
    assert!(max_x > 0.0, "the precondition: the content overflows: {max_x}");
    let got = fx.rt.scroll_of(sv);
    assert!((got[0] - max_x).abs() < 0.5 && got[1] == 0.0, "clamped to the range: {got:?} vs [{max_x}, 0]");
}

/// While a finger pans the Scroll the offset is the finger's: a code
/// write during the pan is not clamped until the finger lifts — the
/// clamp waits for an idle node (the rubber band stores an out-of-range
/// offset on purpose, and the pan's own writes keep flowing).
#[test]
fn the_clamp_waits_while_a_finger_pans_the_scroll() {
    let mut fx = Fx::new(WIDE);
    let sv = fx.id("sv");
    fx.touch(Phase::Down, 50.0, 50.0, 0.0);
    fx.touch(Phase::Move, 20.0, 50.0, 0.5);
    fx.touch(Phase::Move, 10.0, 50.0, 1.0);
    assert!(fx.input.is_pressed(), "the precondition: a press is in flight");
    fx.rt.set_scroll(sv, [10_000.0, 0.0]);
    fx.tick(0.016);
    assert_eq!(fx.rt.scroll_of(sv), [10_000.0, 0.0], "not clamped under the finger");
    fx.touch(Phase::Up, 10.0, 50.0, 3.0);
    fx.tick(0.016);
    let f = fx.lay();
    let (_, rects) = (f.look(), f.rects());
    let max_x = rects[fx.id("wide").index()].w - rects[sv.index()].w;
    let got = fx.rt.scroll_of(sv);
    assert!(got[0] <= max_x + 0.5 && got[0] >= 0.0, "in range once the finger lifted: {got:?} vs max {max_x}");
}

/// A page reuse forgets the engine-held view state: every scroll offset
/// is back at 0, the caret and selection are gone, and the input
/// machine's focus owner is dropped on its next tick.
#[test]
fn reset_view_state_forgets_scroll_focus_and_caret() {
    let mut fx = Fx::new("View v { Column c halign=left, valign=top { Scroll sv width=100, height=100 { Row list { Rectangle wide width=1000, height=50 } }; Button b text=\"go\" } }");
    let sv = fx.id("sv");
    let b = fx.id("b");
    fx.rt.set_scroll(sv, [50.0, 0.0]);
    fx.tick(0.016);
    assert_eq!(fx.rt.scroll_of(sv), [50.0, 0.0], "the precondition: the strip is scrolled");
    let f = fx.lay();
    fx.input.focus_on(&fx.page, &mut fx.rt, f.frame(), Some(b));
    assert_eq!(fx.input.focused(), Some(b), "the precondition: the button holds focus");
    fx.rt.caret = Some((b, 1));
    fx.rt.selection = Some((b, 0, 1));
    fx.rt.dirty = false;
    fx.rt.reset_view_state();
    assert!(fx.rt.dirty, "the reset dirties the page");
    assert_eq!(fx.rt.scroll_of(sv), [0.0, 0.0], "scroll back at 0");
    assert_eq!(fx.rt.caret, None);
    assert_eq!(fx.rt.selection, None);
    assert!(!fx.rt.has(b, gogo_ui::runtime::FOCUSED), "the focus bit is cleared");
    fx.tick(0.016);
    assert_eq!(fx.input.focused(), None, "the input machine forgot its focus owner on the tick");
    assert_eq!(fx.rt.scroll_of(sv), [0.0, 0.0], "still 0 after the clamp tick");
}
