//! GPAR112: the Input control takes input — a tap focuses (the caret
//! shows), keystrokes stream `edit`, Enter and focus-loss commit
//! `change` ONCE per genuine change (gogo_value_events.gd's
//! changed-means-CHANGED law), arrows move the caret. The soft-keyboard
//! raising is the device half and stays open (the tracker row).
use gogo::draw::{Draw, draw};
use gogo::input::{Event, EventKind, Input, Key, MOUSE_ID, Mods, Phase, SEL_HANDLE_EPX, Touch, sel_handle_rect};
use gogo::layout::{DEFAULT_FONT_SIZE, FixedAdvance, Rect, layout};
use gogo::page::{NodeId, PropKey, Color_Straight_Srgb_4xU8_RGBA, Value};
use gogo::runtime::{FOCUSED, Runtime, resolve};
use gogo::{Page, compile};

const SRC: &str = "View v { Column c halign=left, valign=top { Input field width=200, height=36, on:change=FieldChange, on:edit=FieldEdit; Button b text=\"ok\", width=80, height=30 } }";

struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    /// Modifiers the next `key` call carries (reset it by hand).
    mods: Mods,
}

impl Fx {
    fn new() -> Fx {
        let c = compile(SRC, &|_| None);
        assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        Fx { page, rt, input: Input::new(), mods: Mods::NONE }
    }
    fn tap(&mut self, node: &str, t0: f64) -> Vec<Event> {
        let id = self.page.find(node).unwrap();
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        let r = rects[id.index()];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let mut ev = self.input.touch(&self.page, &mut self.rt, &res, &rects, &FixedAdvance, Touch { phase: Phase::Down, x, y, t: t0, id: 1 });
        ev.extend(self.input.touch(&self.page, &mut self.rt, &res, &rects, &FixedAdvance, Touch { phase: Phase::Up, x, y, t: t0 + 0.1, id: 1 }));
        ev
    }
    /// One raw pointer sample — `id` picks the pointer (`MOUSE_ID` = the
    /// mouse; anything else is a finger).
    fn point(&mut self, phase: Phase, x: f32, y: f32, t: f64, id: u64) -> Vec<Event> {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.touch(&self.page, &mut self.rt, &res, &rects, &FixedAdvance, Touch { phase, x, y, t, id })
    }
    /// Advances the input clock (the long-press door matures here).
    fn tick(&mut self, dt: f32) {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.tick(&self.page, &mut self.rt, &res, &rects, dt);
    }
    fn key(&mut self, k: Key) -> Vec<Event> {
        let res = resolve(&self.page, &self.rt);
        let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, &res, &rects, k, self.mods)
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
        let r = rects[field.index()];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        input.touch(&page, &mut rt, &res, &rects, &FixedAdvance, Touch { phase: Phase::Down, x, y, t: 0.0, id: 1 });
        input.touch(&page, &mut rt, &res, &rects, &FixedAdvance, Touch { phase: Phase::Up, x, y, t: 0.1, id: 1 });
    }
    for ch in ['a', 'b', 'c', 'd'] {
        let res = resolve(&page, &rt);
        let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
        input.key(&mut page, &mut rt, &res, &rects, Key::Char(ch), Mods::NONE);
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
    let ev = fx.input.ime_replace(&mut fx.page, &mut fx.rt, "hey", 3, 3);
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Edit(_))), "{ev:?}");
    assert_eq!(fx.text(field), "hey");
    assert_eq!(fx.rt.caret, Some((field, 3)));
    assert!(fx.input.ime_replace(&mut fx.page, &mut fx.rt, "hey", 3, 3).is_empty(), "same state, no event");
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

/// FixedAdvance's per-char width — the pins below compute press x
/// coordinates from it (0.55 em per char, layout.rs's fake).
fn adv() -> f32 {
    DEFAULT_FONT_SIZE * 0.55
}

/// An x that lands INSIDE char `k` of the field's text, so the nearer
/// boundary is k (the drawn text starts 8 epx into the plate).
fn at_char(k: usize) -> f32 {
    8.0 + adv() * (k as f32 + 0.2)
}

#[test]
fn shift_arrows_select_and_a_plain_arrow_collapses() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello".chars() {
        fx.key(Key::Char(c));
    }
    fx.mods = Mods::SHIFT;
    fx.key(Key::Left);
    fx.key(Key::Left);
    assert_eq!(fx.rt.selection, Some((field, 3, 5)), "shift grows from the caret");
    fx.key(Key::Right);
    assert_eq!(fx.rt.selection, Some((field, 4, 5)), "one anchor: shift back SHRINKS");
    fx.mods = Mods::NONE;
    fx.key(Key::Left);
    assert_eq!(fx.rt.selection, None, "a plain arrow collapses");
    assert_eq!(fx.rt.caret, Some((field, 4)), "to the selection's own side, no move");
    assert_eq!(fx.text(field), "hello", "movement never edits");
}

#[test]
fn typing_replaces_the_selection_with_one_edit() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello".chars() {
        fx.key(Key::Char(c));
    }
    fx.mods = Mods::SHIFT;
    fx.key(Key::Home);
    assert_eq!(fx.rt.selection, Some((field, 0, 5)));
    fx.mods = Mods::NONE;
    let ev = fx.key(Key::Char('X'));
    assert_eq!(fx.text(field), "X", "typing REPLACES the selection");
    assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Edit(_))).count(), 1, "one edit: {ev:?}");
    assert_eq!(fx.rt.caret, Some((field, 1)));
    for c in "yz".chars() {
        fx.key(Key::Char(c));
    }
    fx.mods = Mods::SHIFT;
    fx.key(Key::Left);
    fx.key(Key::Left);
    fx.mods = Mods::NONE;
    let ev = fx.key(Key::Backspace);
    assert_eq!(fx.text(field), "X", "backspace erases JUST the selection");
    assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Edit(_))).count(), 1, "{ev:?}");
}

#[test]
fn ctrl_a_selects_all_and_ctrl_letters_never_insert() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "abc".chars() {
        fx.key(Key::Char(c));
    }
    fx.mods = Mods::CTRL;
    fx.key(Key::Char('a'));
    assert_eq!(fx.rt.selection, Some((field, 0, 3)));
    assert_eq!(fx.input.selected_text(&fx.page, &fx.rt).as_deref(), Some("abc"));
    let ev = fx.key(Key::Char('v'));
    assert!(ev.is_empty(), "Ctrl+letter is the shell's shortcut, never an insert: {ev:?}");
    assert_eq!(fx.text(field), "abc");
}

#[test]
fn a_press_places_the_caret_and_a_mouse_drag_selects() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello world".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(3), 18.0, 10.0, MOUSE_ID);
    assert_eq!(fx.rt.caret, Some((field, 3)), "the press places the caret, not the end");
    assert_eq!(fx.rt.selection, None);
    fx.point(Phase::Move, at_char(8), 18.0, 10.1, MOUSE_ID);
    assert_eq!(fx.rt.selection, Some((field, 3, 8)), "the mouse drag selects");
    fx.point(Phase::Up, at_char(8), 18.0, 10.2, MOUSE_ID);
    assert_eq!(fx.input.selected_text(&fx.page, &fx.rt).as_deref(), Some("lo wo"));
    // The highlight draws under the glyphs.
    let res = resolve(&fx.page, &fx.rt);
    let rects = layout(&fx.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let draws = draw(&fx.page, &res, &rects);
    assert!(draws.iter().any(|d| matches!(d, Draw::Selection { lo: 3, hi: 8, .. })), "{draws:?}");
}

#[test]
fn a_double_click_selects_the_word() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello world".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(8), 18.0, 20.0, MOUSE_ID);
    fx.point(Phase::Up, at_char(8), 18.0, 20.05, MOUSE_ID);
    fx.point(Phase::Down, at_char(8), 18.0, 20.2, MOUSE_ID);
    fx.point(Phase::Up, at_char(8), 18.0, 20.25, MOUSE_ID);
    assert_eq!(fx.rt.selection, Some((field, 6, 11)), "the word under the press");
    assert_eq!(fx.input.selected_text(&fx.page, &fx.rt).as_deref(), Some("world"));
}

#[test]
fn a_finger_press_places_the_caret_but_its_drag_never_selects() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello world".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(3), 18.0, 30.0, 7);
    assert_eq!(fx.rt.caret, Some((field, 3)), "a finger press also places the caret");
    fx.point(Phase::Move, at_char(8), 18.0, 30.1, 7);
    assert_eq!(fx.rt.selection, None, "a finger drag pans, never selects (the long-press is its door)");
    fx.point(Phase::Up, at_char(8), 18.0, 30.2, 7);
}

#[test]
fn an_ime_span_becomes_the_selection() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    fx.input.ime_replace(&mut fx.page, &mut fx.rt, "hello", 1, 4);
    assert_eq!(fx.rt.selection, Some((field, 1, 4)), "the keyboard's own select shows");
    assert_eq!(fx.rt.caret, Some((field, 4)));
    assert_eq!(fx.input.ime_seed(&fx.page, &fx.rt), Some(("hello".to_string(), 1, 4)), "the seed answers the span back");
}

#[test]
fn blur_clears_the_selection() {
    let mut fx = Fx::new();
    fx.tap("field", 0.0);
    for c in "abc".chars() {
        fx.key(Key::Char(c));
    }
    fx.mods = Mods::CTRL;
    fx.key(Key::Char('a'));
    fx.mods = Mods::NONE;
    assert!(fx.rt.selection.is_some());
    fx.tap("b", 1.0);
    assert_eq!(fx.rt.selection, None, "focus left, nothing stays highlighted");
    assert_eq!(fx.rt.caret, None);
}

/// Step 2, the shell's clipboard doors: gogo hands text out
/// (`selected_text`) and takes text in (`paste`); the clipboard itself
/// never enters gogo.
#[test]
fn paste_replaces_the_selection_and_max_length_caps_the_result() {
    let src = "View v { Input f width=200, height=36, max_length=5, on:edit=FEdit }";
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let mut page = c.page;
    let mut rt = Runtime::new(&page);
    let mut input = Input::new();
    let field = page.find("f").unwrap();
    let res = resolve(&page, &rt);
    let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let r = rects[field.index()];
    input.touch(&page, &mut rt, &res, &rects, &FixedAdvance, Touch { phase: Phase::Down, x: r.x + 5.0, y: r.y + 5.0, t: 0.0, id: 1 });
    input.touch(&page, &mut rt, &res, &rects, &FixedAdvance, Touch { phase: Phase::Up, x: r.x + 5.0, y: r.y + 5.0, t: 0.1, id: 1 });
    let ev = input.paste(&mut page, &mut rt, "hello world");
    assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Edit(_))).count(), 1, "{ev:?}");
    let text = |page: &Page, rt: &Runtime| match resolve(page, rt).prop(page, field, gogo::page::PropKey::Text) {
        Some(gogo::page::Value::VText { text, .. }) => page.str(text).to_string(),
        _ => String::new(),
    };
    assert_eq!(text(&page, &rt), "hello", "max_length trims the paste to fit");
    // Select "hel" (Home, three shifted Rights) and paste "xy": the
    // selection is replaced and the cap holds against the RESULT.
    input.key(&mut page, &mut rt, &res, &rects, Key::Home, Mods::NONE);
    input.key(&mut page, &mut rt, &res, &rects, Key::Right, Mods::SHIFT);
    input.key(&mut page, &mut rt, &res, &rects, Key::Right, Mods::SHIFT);
    input.key(&mut page, &mut rt, &res, &rects, Key::Right, Mods::SHIFT);
    let ev = input.paste(&mut page, &mut rt, "xy");
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!(text(&page, &rt), "xylo", "the selection is replaced, the tail stays");
}

#[test]
fn the_cut_door_removes_the_selection_with_one_edit() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello".chars() {
        fx.key(Key::Char(c));
    }
    fx.mods = Mods::SHIFT;
    fx.key(Key::Left);
    fx.key(Key::Left);
    fx.mods = Mods::NONE;
    assert_eq!(fx.input.selected_text(&fx.page, &fx.rt).as_deref(), Some("lo"), "the copy half reads the span");
    let ev = fx.input.delete_selection(&mut fx.page, &mut fx.rt);
    assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Edit(_))).count(), 1, "{ev:?}");
    assert_eq!(fx.text(field), "hel");
    assert_eq!(fx.rt.selection, None);
    assert!(fx.input.delete_selection(&mut fx.page, &mut fx.rt).is_empty(), "nothing selected, nothing happens");
}

/// Step 3: the touch door. A phone has no Ctrl — a still long-press in
/// a field selects the word under the finger and raises the edit bar
/// (`Runtime::edit_bar`; the bar itself is the built-in `edit_bar.gogo`
/// page the shell composes and answers). The touch-born selection
/// hangs its two drag circles below the field.
#[test]
fn a_long_press_selects_the_word_and_raises_the_bar() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello world".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(8), 18.0, 40.0, 7);
    fx.tick(0.6);
    assert_eq!(fx.rt.selection, Some((field, 6, 11)), "the word under the finger");
    assert_eq!(fx.rt.edit_bar, Some(field));
    fx.point(Phase::Up, at_char(8), 18.0, 40.7, 7);
    assert!(fx.rt.sel_handles, "a touch selection shows drag handles");
    let res = resolve(&fx.page, &fx.rt);
    let rects = layout(&fx.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let draws = draw(&fx.page, &res, &rects);
    assert!(draws.iter().any(|d| matches!(d, Draw::SelHandles { lo: 6, hi: 11, .. })), "{draws:?}");
}

#[test]
fn a_mouse_hold_never_raises_the_bar() {
    let mut fx = Fx::new();
    fx.tap("field", 0.0);
    for c in "hi".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(1), 18.0, 50.0, MOUSE_ID);
    fx.tick(0.6);
    assert_eq!(fx.rt.edit_bar, None, "the mouse has drag-select and Ctrl");
    fx.point(Phase::Up, at_char(1), 18.0, 50.7, MOUSE_ID);
}

/// The bar's All (the shell answers that button on the bar's own page)
/// selects everything and keeps the bar up — Copy is the natural next
/// tap.
#[test]
fn select_all_keeps_the_bar_up() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello world".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(8), 18.0, 40.0, 7);
    fx.tick(0.6);
    fx.point(Phase::Up, at_char(8), 18.0, 40.7, 7);
    assert!(fx.input.select_all(&fx.page, &mut fx.rt));
    assert_eq!(fx.rt.selection, Some((field, 0, 11)));
    assert_eq!(fx.rt.edit_bar, Some(field), "All keeps the bar up");
    assert!(fx.rt.sel_handles, "All is a touch door — handles show");
}

/// The two drag circles below a touch selection: grabbing one moves
/// ITS end; the selection never collapses past one char; lifting keeps
/// the bar (adjusting is not dismissing). The grab uses the same
/// `sel_handle_rect` the renderer draws.
#[test]
fn a_handle_drag_moves_its_end() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello world".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(2), 18.0, 40.0, 7);
    fx.tick(0.6);
    fx.point(Phase::Up, at_char(2), 18.0, 40.7, 7);
    assert_eq!(fx.rt.selection, Some((field, 0, 5)), "hello selected");
    let res = resolve(&fx.page, &fx.rt);
    let rects = layout(&fx.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let fr = rects[field.index()];
    let hr = sel_handle_rect(fr.y + fr.h, 8.0 + adv() * 5.0, SEL_HANDLE_EPX);
    let (hx, hy) = (hr.x + hr.w / 2.0, hr.y + hr.h / 2.0);
    fx.point(Phase::Down, hx, hy, 41.0, 7);
    fx.point(Phase::Move, at_char(8), hy, 41.1, 7);
    assert_eq!(fx.rt.selection, Some((field, 0, 8)), "the HI end followed the finger");
    // Dragging past the other end stops at one char.
    fx.point(Phase::Move, 0.0, hy, 41.2, 7);
    assert_eq!(fx.rt.selection, Some((field, 0, 1)));
    fx.point(Phase::Up, at_char(0), hy, 41.3, 7);
    assert_eq!(fx.rt.edit_bar, Some(field), "adjusting is not dismissing");
    assert!(fx.rt.sel_handles, "the handles stand after the lift");
}

/// A mouse drag-select shows NO handles (the mouse has drag-select and
/// Ctrl; the circles are the finger's affordance).
#[test]
fn a_mouse_selection_has_no_handles() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(1), 18.0, 40.0, MOUSE_ID);
    fx.point(Phase::Move, at_char(4), 18.0, 40.1, MOUSE_ID);
    fx.point(Phase::Up, at_char(4), 18.0, 40.2, MOUSE_ID);
    assert_eq!(fx.rt.selection, Some((field, 1, 4)));
    assert!(!fx.rt.sel_handles, "mouse selections show no drag circles");
}

#[test]
fn a_press_off_the_bar_dismisses_it() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hello world".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(8), 18.0, 40.0, 7);
    fx.tick(0.6);
    fx.point(Phase::Up, at_char(8), 18.0, 40.7, 7);
    assert_eq!(fx.rt.edit_bar, Some(field));
    // A press on the field's TEXT (clear of the bar's page and of the
    // handle circles) dismisses and runs its normal course (the caret
    // moves there).
    fx.point(Phase::Down, at_char(2), 4.0, 60.0, 7);
    assert_eq!(fx.rt.edit_bar, None);
    assert_eq!(fx.rt.caret, Some((field, 2)));
    fx.point(Phase::Up, at_char(2), 4.0, 60.1, 7);
}

/// Emoji ride gogo as single CHARS (the platform seam converts Java's
/// UTF-16 units before they get here): backspace removes the WHOLE
/// emoji, and an IME push with a char span lands exactly.
#[test]
fn backspace_removes_a_whole_emoji() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.tap("field", 0.0);
    for c in "hi\u{1F60D}".chars() {
        fx.key(Key::Char(c));
    }
    assert_eq!(fx.text(field), "hi\u{1F60D}");
    assert_eq!(fx.rt.caret, Some((field, 3)), "3 chars, not 4 units");
    fx.key(Key::Backspace);
    assert_eq!(fx.text(field), "hi", "the whole emoji goes");
    assert_eq!(fx.rt.caret, Some((field, 2)));
    let ev = fx.input.ime_replace(&mut fx.page, &mut fx.rt, "hi\u{1F60D}", 3, 3);
    assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Edit(_))), "{ev:?}");
    assert_eq!(fx.rt.caret, Some((field, 3)));
}

/// `selection_color=` paints the highlight, and the drag circles wear
/// it made opaque. The word is an Input's own — a Label refuses it
/// (G2002).
#[test]
fn selection_color_paints_the_highlight_and_handles() {
    let mut fx = Fx::new();
    let field = fx.page.find("field").unwrap();
    fx.rt.set_prop(field, PropKey::SelectionColor, Value::VColor(Color_Straight_Srgb_4xU8_RGBA([255, 0, 0, 128])));
    fx.tap("field", 0.0);
    for c in "hello".chars() {
        fx.key(Key::Char(c));
    }
    fx.point(Phase::Down, at_char(1), 18.0, 40.0, 7);
    fx.tick(0.6);
    fx.point(Phase::Up, at_char(1), 18.0, 40.7, 7);
    let res = resolve(&fx.page, &fx.rt);
    let rects = layout(&fx.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
    let draws = draw(&fx.page, &res, &rects);
    assert!(draws.iter().any(|d| matches!(d, Draw::Selection { color, .. } if color.0 == [255, 0, 0, 128])), "{draws:?}");
    assert!(draws.iter().any(|d| matches!(d, Draw::SelHandles { color, .. } if color.0 == [255, 0, 0, 255])), "{draws:?}");
    let c = compile("View v { Label l text=\"x\", selection_color=#FF000000 }", &|_| None);
    assert!(c.diags.iter().any(|d| d.code == "G2002"), "{:#?}", c.diags);
}
