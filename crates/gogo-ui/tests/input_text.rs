//! GPAR112: the Input control takes input — a tap focuses (the caret
//! shows), keystrokes stream `edit`, Enter and focus-loss commit
//! `change` ONCE per genuine change (gogo_value_events.gd's
//! changed-means-CHANGED law), arrows move the caret. The soft-keyboard
//! raising is the device half and stays open (the tracker row).
use gogo::draw::{Draw, draw};
use gogo::input::{Event, EventKind, Input, Key, Phase, Touch};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::page::{NodeId, PropKey, Value};
use gogo::runtime::{FOCUSED, Runtime, resolve};
use gogo::{Page, compile};

const SRC: &str = "View v { Column c halign=left, valign=top { Input field width=200, height=36, on:change=FieldChange, on:edit=FieldEdit; Button b text=\"ok\", width=80, height=30 } }";

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
}

impl Fx {
    fn new() -> Fx {
        let c = compile(SRC, &|_| None);
        assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        Fx { page, rt, input: Input::new() }
    }
    fn tap(&mut self, node: &str, t0: f64) -> Vec<Event> {
        let id = self.page.find(node).unwrap();
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let r = rects[id as usize];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let mut ev = self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase: Phase::Down, x, y, t: t0, id: 1 });
        ev.extend(self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase: Phase::Up, x, y, t: t0 + 0.1, id: 1 }));
        ev
    }
    fn key(&mut self, k: Key) -> Vec<Event> {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, &res, &rects, k)
    }
    fn text(&self, node: NodeId) -> String {
        let res = resolve(&self.page, &self.rt);
        match res.prop(&self.page, node, PropKey::Text) {
            Some(Value::VText { text, .. }) => self.page.str(text).to_string(),
            _ => String::new(),
        }
    }
}

#[test]
fn tap_focuses_typing_streams_and_the_doors_commit_once() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    assert!(fx.rt.has(field, FOCUSED), "a tap focuses the field");
    let ev = fx.key(Key::Char('h'));
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Edit(_))), "each keystroke streams edit: {ev:?}");
    fx.key(Key::Char('i'));
    assert_eq!(fx.text(field), "hi");
    let ev = fx.key(Key::Enter);
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Change(_))), "Enter commits: {ev:?}");
    assert!(fx.key(Key::Enter).is_empty(), "a commit landing on the last committed value is a repair, not a change");
    fx.key(Key::Char('!'));
    let ev = fx.tap("b", 1.0);
    assert!(ev.iter().any(|e| e.node == field && matches!(e.kind, EventKind::Change(_))), "focus loss commits the changed text: {ev:?}");
    assert!(!fx.rt.has(field, FOCUSED), "the tap elsewhere blurred");
    assert_eq!(fx.text(field), "hi!");
}

#[test]
fn the_caret_moves_and_rides_the_draw_list() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in ['a', 'b', 'c'] {
        fx.key(Key::Char(c));
    }
    fx.key(Key::Left);
    fx.key(Key::Left);
    fx.key(Key::Char('X'));
    assert_eq!(fx.text(field), "aXbc", "insert at the caret");
    fx.key(Key::Home);
    fx.key(Key::Delete);
    assert_eq!(fx.text(field), "Xbc");
    fx.key(Key::End);
    fx.key(Key::Backspace);
    assert_eq!(fx.text(field), "Xb");
    assert_eq!(fx.rt.caret, Some((field, 2)));
    let res = resolve(&fx.page, &fx.rt);
    let rects = layout(&fx.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let list = draw(&fx.page, &res, &rects);
    assert!(list.iter().any(|d| matches!(d, Draw::Caret { index: 2, .. })), "the caret rides the draw list: {list:#?}");
}

/// GPAR112 round 2: `placeholder_text=` draws dimmed while the field is
/// empty and leaves when text arrives; `max_length=` stops typing at that
/// many characters (Godot's LineEdit words).
#[test]
fn placeholder_shows_while_empty_and_max_length_caps_typing() {
    let c = compile("View v { Input field width=200, height=36, placeholder_text=\"Search\", max_length=3, on:edit=FieldEdit }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let mut input = Input::new();
    let field = page.find("field").unwrap();
    let shows_placeholder = |page: &Page, rt: &Runtime| {
        let res = resolve(page, rt);
        let rects = layout(page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        draw(page, &res, &rects).iter().any(|d| matches!(d, Draw::Text { text, .. } if page.str(*text) == "Search"))
    };
    assert!(shows_placeholder(&page, &rt), "empty field shows the placeholder");
    {
        let res = resolve(&page, &rt);
        let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let r = rects[field as usize];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        input.touch(&page, &mut rt, &res, &rects, Touch { phase: Phase::Down, x, y, t: 0.0, id: 1 });
        input.touch(&page, &mut rt, &res, &rects, Touch { phase: Phase::Up, x, y, t: 0.1, id: 1 });
    }
    for ch in ['a', 'b', 'c', 'd'] {
        let res = resolve(&page, &rt);
        let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        input.key(&mut page, &mut rt, &res, &rects, Key::Char(ch));
    }
    let res = resolve(&page, &rt);
    let text = match res.prop(&page, field, PropKey::Text) {
        Some(Value::VText { text, .. }) => page.str(text).to_string(),
        _ => String::new(),
    };
    assert_eq!(text, "abc", "typing stops at max_length");
    assert!(!shows_placeholder(&page, &rt), "a filled field hides the placeholder");
}

/// GPAR112, the GameActivity door: the soft IME hands WHOLE text states
/// (autocorrect/swipe rewrite spans, not keys). `ime_replace` writes the
/// state into the focused field — max_length applies like typing, `edit`
/// streams once per genuine change, and the focus-loss door still
/// commits once.
#[test]
fn ime_updates_write_the_focused_field() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    let ev = fx.input.ime_replace(&mut fx.page, &mut fx.rt, "hey", 3);
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Edit(_))), "{ev:?}");
    assert_eq!(fx.text(field), "hey");
    assert_eq!(fx.rt.caret, Some((field, 3)));
    assert!(fx.input.ime_replace(&mut fx.page, &mut fx.rt, "hey", 3).is_empty(), "same state, no event");
    let ev = fx.tap("b", 1.0);
    assert!(ev.iter().any(|e| e.node == field && matches!(e.kind, EventKind::Change(_))), "blur commits: {ev:?}");
}

#[test]
fn an_unfocused_page_ignores_keys_and_reset_forgets_without_committing() {
    let mut fx = Fx::new();
    assert!(fx.key(Key::Char('x')).is_empty(), "nothing focused, nothing happens");
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    fx.key(Key::Char('q'));
    fx.input.reset(&fx.page, &mut fx.rt);
    assert!(!fx.rt.has(field, FOCUSED));
    assert!(fx.rt.caret.is_none(), "the page left — no commit, no caret");
}
