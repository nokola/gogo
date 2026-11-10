//! GP26: a `control` claims key GROUPS by contract — `keys arrows,
//! home_end, enter, space` — the built-ins' static-claim law (a Slider
//! owns its arrows, §5.5) extended to user controls. A claimed key
//! reaches the code-behind as a `Key` event on the focused instance root
//! and never traverses; an unclaimed key traverses as always; Tab is
//! never claimable — the way off a control. (Godot answers this with a
//! runtime `_gui_input` claim; gogo declares it, so the file says which
//! keys a control eats.)

use gogo_ui::compile;
use gogo_ui::KeptFrame;
use gogo_ui::input::{Event, EventKind, Input, Key, Mods};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{NodeId, Page};
use gogo_ui::runtime::{FOCUSED, Runtime, resolve};

const STEPPER: &str = "control Stepper $value=0 {\n    state_builtin focused, disabled\n    keys arrows, home_end\n    Layers { Rectangle r 10x10, color=#FF0000FF }\n}\n";

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    /// The frame the last touch or key saw (laid out fresh for each).
    kept: KeptFrame,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = compile(src, &|_| None);
        assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        let f = KeptFrame::fresh(&page, resolve(&page, &rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        Fx { page, rt, input: Input::default(), kept: f }
    }

    fn key(&mut self, k: Key) -> Vec<Event> {
        self.kept = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, self.kept.frame(), k, Mods::NONE)
    }

    fn focused(&self) -> Option<NodeId> {
        self.page.node_ids().find(|n| self.rt.has(*n, FOCUSED))
    }
}

/// A claimed arrow reaches the code-behind and never moves focus; an
/// unclaimed group (enter/space here) stays the router's.
#[test]
fn a_claimed_key_is_an_event_not_a_traversal() {
    let mut fx = Fx::new(&format!("{STEPPER}View v {{ Stepper a\n Stepper b }}"));
    let a = fx.page.find("a").unwrap();
    fx.key(Key::Tab);
    assert_eq!(fx.focused(), Some(a));
    let ev = fx.key(Key::Right);
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!(ev[0].node, a);
    assert!(matches!(ev[0].kind, EventKind::Key { key: Key::Right, .. }), "{ev:?}");
    assert_eq!(fx.focused(), Some(a), "a claimed arrow never traverses");
    let ev = fx.key(Key::Home);
    assert!(matches!(ev[0].kind, EventKind::Key { key: Key::Home, .. }), "{ev:?}");
    // Enter and Space are NOT in this control's claim: no event, and a
    // Custom root is no press surface — nothing activates.
    assert!(fx.key(Key::Enter).is_empty());
    assert!(fx.key(Key::Char(' ')).is_empty());
}

/// Without a `keys` line the old law stands: arrows traverse between
/// instances. And Tab leaves a claiming control regardless.
#[test]
fn unclaimed_arrows_traverse_and_tab_always_leaves() {
    let plain = "control Plain $value=0 {\n    state_builtin focused\n    Layers { Rectangle r 10x10, color=#FF0000FF }\n}\n";
    let mut fx = Fx::new(&format!("{plain}View v {{ Row bar spacing=8 {{ Plain a\n Plain b }} }}"));
    let (a, b) = (fx.page.find("a").unwrap(), fx.page.find("b").unwrap());
    fx.key(Key::Tab);
    assert_eq!(fx.focused(), Some(a));
    fx.key(Key::Right);
    assert_eq!(fx.focused(), Some(b), "no claim, the arrow walks");

    let mut fx = Fx::new(&format!("{STEPPER}View v {{ Row bar spacing=8 {{ Stepper a\n Stepper b }} }}"));
    let (a, b) = (fx.page.find("a").unwrap(), fx.page.find("b").unwrap());
    fx.key(Key::Tab);
    assert_eq!(fx.focused(), Some(a));
    fx.key(Key::Tab);
    assert_eq!(fx.focused(), Some(b), "Tab is the way off, claims or not");
}

/// A disabled instance claims nothing — its keys fall back to the
/// router (which skips it for traversal too).
#[test]
fn a_disabled_instance_claims_nothing() {
    let mut fx = Fx::new(&format!("{STEPPER}View v {{ Stepper a }}"));
    let a = fx.page.find("a").unwrap();
    fx.key(Key::Tab);
    assert_eq!(fx.focused(), Some(a));
    fx.rt.set_state(a, gogo_ui::runtime::DISABLED, true);
    let ev = fx.key(Key::Right);
    assert!(!ev.iter().any(|e| matches!(e.kind, EventKind::Key { .. })), "{ev:?}");
}

/// The fences: unknown group words, `keys tab`, an empty line, and
/// `keys` on a `widget` (no script to answer) all refuse.
#[test]
fn keys_fences() {
    let fails = |src: &str| gogo_ui::has_errors(&compile(src, &|_| None).diags);
    assert!(fails("control C $v=0 { keys frobnicate\n Layers { Rectangle r 10x10 } }\nView v { C c }"), "unknown group");
    assert!(fails("control C $v=0 { keys tab\n Layers { Rectangle r 10x10 } }\nView v { C c }"), "tab is never claimable");
    assert!(fails("control C $v=0 { keys\n Layers { Rectangle r 10x10 } }\nView v { C c }"), "an empty claim");
    assert!(fails("widget W { keys arrows\n Layers { Rectangle r 10x10 } }\nView v { W w }"), "a widget has no script to answer keys");
}
