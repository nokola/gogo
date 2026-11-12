//! Input and the built-in controls: a Toggle's knob glides and its `held`
//! lasts through a drag, Tab follows the file, the wedge of
//! `automatic=arc(…)` counts from the top like an `Arc`, `keyboard_target`
//! written from code, `wrap=` on the press controls, and the plate words
//! of an Input.

use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{PropKey, Value};
use gogo_ui::runtime::{HELD, Runtime, SELECTED};
use gogo_ui::{ArrangementId, Draw, Event, EventKind, Input, KeptFrame, Key, Mods, NodeId, Page, Phase, Touch, compile, hit};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
/// One frame of a 60 Hz screen, in seconds.
const FRAME: f32 = 1.0 / 60.0;
/// What `FixedAdvance` answers at the default font size (15): the height
/// of one line.
const LINE: f32 = 18.75;
/// Eight four-letter words: 39 letters, 321.75 wide on one line.
const LONG: &str = "aaaa bbbb cccc dddd eeee ffff gggg hhhh";

/// One page run the way a host runs it: the clock moves, then the look,
/// the layout and the draw list of the frame are brought up to date, in
/// buffers kept between frames.
struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
    kept: KeptFrame,
    draws: Vec<Draw>,
    /// The touch clock, in seconds.
    now: f64,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = compile(src, &|_| None);
        assert!(!gogo_ui::has_errors(&c.diags), "{src}\n{:#?}", c.diags);
        let rt = Runtime::new(&c.page);
        let mut fx = Fx { page: c.page, rt, input: Input::new(), kept: KeptFrame::default(), draws: Vec::new(), now: 0.0 };
        fx.frame(0.0);
        fx
    }

    fn frame(&mut self, dt: f32) {
        self.rt.tick(&mut self.page, dt);
        self.kept.frame_retained(&mut self.rt, &self.page, VIEW, &FixedAdvance, &mut self.draws, None);
    }

    /// Frames of a 60 Hz screen until `seconds` have passed.
    fn frames(&mut self, seconds: f32) {
        for _ in 0..(seconds / FRAME).ceil() as u32 {
            self.frame(FRAME);
        }
    }

    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node `{name}`"))
    }

    fn name(&self, id: NodeId) -> String {
        self.page.str(self.page.nodes[id].name).to_string()
    }

    fn rect(&self, name: &str) -> Rect {
        self.kept.rects()[self.id(name).index()]
    }

    fn center(&self, name: &str) -> (f32, f32) {
        let r = self.rect(name);
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    fn touch(&mut self, phase: Phase, x: f32, y: f32) -> Vec<Event> {
        self.now += 0.01;
        self.input.touch(&self.page, &mut self.rt, self.kept.frame(), &FixedAdvance, Touch { phase, x, y, t: self.now, id: 1 })
    }

    /// A tap in the middle of `name`, a second after the touch before it
    /// (two taps closer than that are a double tap).
    fn tap(&mut self, name: &str) -> Vec<Event> {
        let (x, y) = self.center(name);
        self.now += 1.0;
        let mut events = self.touch(Phase::Down, x, y);
        events.extend(self.touch(Phase::Up, x, y));
        events
    }

    fn key(&mut self, k: Key) -> Vec<Event> {
        let events = self.input.key(&mut self.page, &mut self.rt, self.kept.frame(), k, Mods::NONE);
        self.frame(0.0);
        events
    }

    fn focused(&self) -> String {
        self.input.focused().map_or(String::from("nothing"), |n| self.name(n))
    }

    /// The names the focus lands on as `k` is pressed `times` times.
    fn walk(&mut self, k: Key, times: usize) -> Vec<String> {
        (0..times)
            .map(|_| {
                self.key(k);
                self.focused()
            })
            .collect()
    }

    fn go(&mut self, arrangement: &str) {
        let at = self.page.arrangements.iter().position(|a| self.page.str(*a) == arrangement).unwrap_or_else(|| panic!("no arrangement `{arrangement}`"));
        self.rt.go_instant_id(ArrangementId::from_index(at));
        self.frame(0.0);
    }

    /// The left edge of the one circle the page draws: the knob of its
    /// built-in Toggle.
    fn knob_x(&self) -> f32 {
        let knobs: Vec<f32> = self.draws.iter().filter_map(|d| if let Draw::Circle { rect, .. } = d { Some(rect.x) } else { None }).collect();
        assert_eq!(knobs.len(), 1, "{:#?}", self.draws);
        knobs[0]
    }

    /// The node under a point, or the name of the box it is in when the
    /// point is on a child of `within`.
    fn is_under(&self, x: f32, y: f32, within: &str) -> bool {
        let root = self.id(within);
        hit(&self.page, self.kept.frame(), x, y, true).is_some_and(|n| self.kept.look().placed_is_under(&self.page, n, root))
    }

    /// The plate quads of the frame, in draw order.
    fn quads(&self) -> Vec<Draw> {
        self.draws.iter().filter(|d| matches!(d, Draw::Quad { .. } | Draw::QuadCorners { .. })).cloned().collect()
    }

    /// The `wrap` of the one text the page draws.
    fn text_wraps(&self) -> bool {
        let wraps: Vec<bool> = self.draws.iter().filter_map(|d| if let Draw::Text { wrap, .. } = d { Some(*wrap) } else { None }).collect();
        assert_eq!(wraps.len(), 1, "{:#?}", self.draws);
        wraps[0]
    }
}

// ----- a built-in Toggle's knob glides ---------------------------------------

const SWITCH: &str = "View v { Toggle t text=\"x\", halign=left, valign=top }";

/// A tap sends the knob to the other end over a few frames. The state
/// flips at the tap; the knob is the picture of it and travels. The
/// frames of the travel lay out nothing.
#[test]
fn a_tapped_toggle_glides_to_the_other_end() {
    let mut fx = Fx::new(SWITCH);
    let t = fx.id("t");
    let off: f32 = fx.knob_x();
    fx.tap("t");
    assert!(fx.rt.has(t, SELECTED), "the tap selects at once");
    fx.frame(FRAME);
    let first: f32 = fx.knob_x();
    fx.frame(FRAME);
    let second: f32 = fx.knob_x();
    assert_eq!(fx.rt.debug_layout_stats().measured, 0, "a knob on its way measures nothing");
    assert_eq!(fx.rt.debug_layout_stats().placed, 0, "a knob on its way places nothing");
    fx.frames(1.0);
    let on: f32 = fx.knob_x();
    assert!(on > off + 10.0, "the knob ends at the other end: {off} -> {on}");
    assert!(first < on, "the frame after the tap the knob has not jumped: {first} (off {off}, on {on})");
    assert!(second > off && second < on, "two frames after the tap the knob is between the ends: {second} (off {off}, on {on})");
    // And back.
    fx.tap("t");
    fx.frame(FRAME);
    fx.frame(FRAME);
    let back: f32 = fx.knob_x();
    assert!(back > off && back < on, "{back} (off {off}, on {on})");
    fx.frames(1.0);
    assert_eq!(fx.knob_x(), off);
}

/// A value or a `selected` written from code, and a key, move the knob
/// the same way a tap does.
#[test]
fn a_toggle_set_from_code_or_by_a_key_glides_too() {
    for flip in ["value", "selected", "key"] {
        let mut fx = Fx::new(SWITCH);
        let t = fx.id("t");
        let off: f32 = fx.knob_x();
        match flip {
            "value" => fx.rt.set_prop(t, PropKey::Value, Value::VNum(1.0)),
            "selected" => fx.rt.set_state(t, SELECTED, true),
            _ => {
                fx.key(Key::Tab);
                fx.key(Key::Enter);
            }
        }
        fx.frame(FRAME);
        fx.frame(FRAME);
        let moving: f32 = fx.knob_x();
        fx.frames(1.0);
        let on: f32 = fx.knob_x();
        assert!(on > off + 10.0, "{flip}: {off} -> {on}");
        assert!(moving > off && moving < on, "{flip}: two frames in the knob is between the ends: {moving} (off {off}, on {on})");
    }
}

/// A Toggle's own `transition` line times its knob: a longer one is
/// slower, and `duration=0` is the jump.
#[test]
fn a_toggles_own_transition_times_its_knob() {
    let mut jump = Fx::new("View v { Toggle t text=\"x\", halign=left, valign=top { transition duration=0 } }");
    let off: f32 = jump.knob_x();
    jump.tap("t");
    jump.frame(FRAME);
    let at_once: f32 = jump.knob_x();
    jump.frames(1.0);
    assert!(at_once > off + 10.0, "duration=0 jumps: {off} -> {at_once}");
    assert_eq!(jump.knob_x(), at_once);

    let mut slow = Fx::new("View v { Toggle t text=\"x\", halign=left, valign=top { transition duration=2 } }");
    slow.tap("t");
    slow.frames(1.0);
    let halfway: f32 = slow.knob_x();
    slow.frames(1.5);
    let on: f32 = slow.knob_x();
    assert!(halfway > off && halfway < on, "a second into a two-second rule the knob is on its way: {halfway} (off {off}, on {on})");
}

/// A Toggle that does not show has no knob to watch travel: when it
/// appears its knob is at its end.
#[test]
fn a_knob_that_is_not_shown_does_not_travel() {
    let mut fx = Fx::new("View v { Panel p visibility=gone { Toggle t text=\"x\", halign=left, valign=top } }");
    let (p, t) = (fx.id("p"), fx.id("t"));
    fx.rt.set_prop(t, PropKey::Value, Value::VNum(1.0));
    fx.frame(FRAME);
    assert!(!fx.rt.animating(), "nothing travels on a page that shows no Toggle");
    fx.rt.set_visible(&mut fx.page, p, true);
    fx.frame(FRAME);
    let shown: f32 = fx.knob_x();
    fx.frames(1.0);
    assert_eq!(fx.knob_x(), shown, "the knob was at its end when the Toggle appeared");
}

/// Under a finger the knob does not glide: it is where the finger is, on
/// the frame the finger moved. And `held` stays on until the finger
/// lifts, also while the finger is far off the Toggle's box.
#[test]
fn a_dragged_toggle_follows_the_finger_and_stays_held() {
    let mut fx = Fx::new(SWITCH);
    let t = fx.id("t");
    let off: f32 = fx.knob_x();
    let r: Rect = fx.rect("t");
    let (x, y) = (r.x + 8.0, r.y + r.h / 2.0);
    fx.touch(Phase::Down, x, y);
    fx.touch(Phase::Move, x + 10.0, y);
    fx.frame(FRAME);
    let under: f32 = fx.knob_x();
    fx.frame(FRAME);
    assert!(under > off, "the knob left its end with the finger: {off} -> {under}");
    assert_eq!(fx.knob_x(), under, "a knob under a finger is there at once, it does not travel");
    assert!(fx.rt.has(t, HELD));
    // Far below the Toggle, and further right.
    fx.touch(Phase::Move, x + 20.0, y + 300.0);
    fx.frame(FRAME);
    let further: f32 = fx.knob_x();
    fx.frame(FRAME);
    assert!(fx.rt.has(t, HELD), "`held` lasts through the drag, off the Toggle's box too");
    assert!(further > under, "{under} -> {further}");
    assert_eq!(fx.knob_x(), further, "off its box the knob still follows at once");
    let events: Vec<Event> = fx.touch(Phase::Up, x + 20.0, y + 300.0);
    assert!(!fx.rt.has(t, HELD), "the lift ends `held`");
    assert!(events.iter().any(|e| matches!(e.kind, EventKind::Change(Value::VBool(true)))), "{events:?}");
}

/// A Button is a tap: sliding off it ends `held`, as before.
#[test]
fn a_button_still_loses_held_when_the_finger_slides_off() {
    let mut fx = Fx::new("View v { Button b text=\"go\", halign=left, valign=top }");
    let b = fx.id("b");
    let (x, y) = fx.center("b");
    fx.touch(Phase::Down, x, y);
    assert!(fx.rt.has(b, HELD));
    fx.touch(Phase::Move, x, y + 300.0);
    assert!(!fx.rt.has(b, HELD));
}

// ----- Tab follows the file --------------------------------------------------

/// Tab steps through the controls top to bottom as the file has them: a
/// Slider inside a Panel comes before the Input written after that Panel.
#[test]
fn tab_visits_the_controls_in_the_order_of_the_file() {
    let mut fx = Fx::new("View v { Column c halign=left, valign=top {\n Button first text=\"1\"\n Panel p { Slider s width=100 }\n Input i width=100\n Button last text=\"2\"\n} }");
    assert_eq!(fx.walk(Key::Tab, 5), ["first", "s", "i", "last", "first"]);
    assert_eq!(fx.walk(Key::TabBack, 4), ["last", "i", "s", "first"]);
    // With nothing focused Shift-Tab enters at the last one.
    let mut fresh = Fx::new("View v { Column c halign=left, valign=top {\n Button first text=\"1\"\n Panel p { Slider s width=100 }\n Input i width=100\n} }");
    assert_eq!(fresh.walk(Key::TabBack, 3), ["i", "s", "first"]);
}

/// The rows a repeat makes are visited where the repeat is written, in
/// the order they show.
#[test]
fn tab_visits_the_rows_of_a_repeat_where_it_is_written() {
    let mut fx = Fx::new("View v { Column c halign=left, valign=top {\n Button before text=\"b\"\n Column list { repeat rows capacity=3 { Button r text=\"row\" } }\n Button after text=\"a\"\n} }");
    let rows = fx.id("rows");
    fx.rt.set_count(&mut fx.page, rows, 2);
    fx.frame(0.0);
    assert_eq!(fx.walk(Key::Tab, 5), ["before", "rows/0/r", "rows/1/r", "after", "before"]);
    assert_eq!(fx.walk(Key::TabBack, 3), ["after", "rows/1/r", "rows/0/r"]);
}

/// A control a `when` moved into a seat is visited where the seat is.
#[test]
fn tab_visits_a_seated_control_where_its_seat_is() {
    let mut fx = Fx::new("arrangement open\nwhen open { late.seat = spot }\nView v { Column c halign=left, valign=top {\n Button a text=\"a\"\n seat spot\n Button z text=\"z\"\n Column home { Button late text=\"late\" }\n} }");
    assert_eq!(fx.walk(Key::Tab, 3), ["a", "z", "late"]);
    let mut seated = Fx::new("arrangement open\nwhen open { late.seat = spot }\nView v { Column c halign=left, valign=top {\n Button a text=\"a\"\n seat spot\n Button z text=\"z\"\n Column home { Button late text=\"late\" }\n} }");
    seated.go("open");
    assert_eq!(seated.walk(Key::Tab, 3), ["a", "late", "z"]);
}

// ----- `keyboard_target` written from code -----------------------------------

const THREE: &str = "View v { Column c halign=left, valign=top {\n Button a text=\"a\"\n Button b text=\"b\"\n Button z text=\"z\"\n Button never text=\"n\", keyboard_target=off\n} }";

/// `keyboard_target=off` written from code keeps Tab and a tap's focus
/// off the control; clearing the write gives the file's word back.
#[test]
fn keyboard_target_written_from_code_takes_a_control_out_of_the_tab_order() {
    let mut fx = Fx::new(THREE);
    let (b, never) = (fx.id("b"), fx.id("never"));
    assert_eq!(fx.walk(Key::Tab, 4), ["a", "b", "z", "a"]);
    let off = Value::VIdent(fx.page.intern("off"));
    fx.rt.set_prop(b, PropKey::KeyboardTarget, off);
    fx.frame(0.0);
    assert_eq!(fx.walk(Key::Tab, 3), ["z", "a", "z"], "Tab skips `b`");
    fx.tap("b");
    assert_eq!(fx.focused(), "nothing", "a tap on `b` gives it no focus");
    fx.rt.clear_prop(b, PropKey::KeyboardTarget);
    fx.frame(0.0);
    fx.tap("b");
    assert_eq!(fx.focused(), "b", "with the write cleared `b` takes the focus again");
    assert_eq!(fx.walk(Key::Tab, 2), ["z", "a"]);
    // The file's `off` is not code's to clear.
    fx.rt.clear_prop(never, PropKey::KeyboardTarget);
    fx.frame(0.0);
    assert_eq!(fx.walk(Key::Tab, 3), ["b", "z", "a"]);
}

/// A control switched off while it has the focus is like one disabled
/// while it has the focus: it answers no key, and Tab goes on from it.
#[test]
fn a_focused_control_switched_off_answers_no_key_and_tab_moves_on() {
    let mut fx = Fx::new(THREE);
    let b = fx.id("b");
    assert_eq!(fx.walk(Key::Tab, 2), ["a", "b"]);
    let off = Value::VIdent(fx.page.intern("off"));
    fx.rt.set_prop(b, PropKey::KeyboardTarget, off);
    fx.frame(0.0);
    let events: Vec<Event> = fx.key(Key::Enter);
    assert!(events.is_empty(), "Enter on a control that is no keyboard target does nothing: {events:?}");
    assert_eq!(fx.walk(Key::Tab, 2), ["z", "a"], "Tab goes on from where `b` is");
}

// ----- the wedge of `automatic=arc(…)` counts from the top -------------------

/// A dial 48 x 48 at the window's top left: a wedge that opens from 0 for
/// a quarter turn, and a dot that goes once around.
const DIAL: &str = r#"
control Dial $value=0, $min=0, $max=1, $step=0, min_size=48x48 {
    state_builtin hovered, focused, disabled
    Circle color=#333333FF {
        slot track width=fill, height=fill {
            slot wedge automatic=arc(from=0, sweep=90) by $value in $min..$max {
                Arc thickness=4, color=#4488FFFF
            }
            slot dot automatic=angle(from=0, sweep=360) by $value in $min..$max {
                Circle 8x8, color=#FFFFFFFF
            }
        }
    }
}
"#;

/// `from=0` is 12 o'clock and the sweep runs clockwise, as on an `Arc`:
/// a full quarter wedge from 0 is the upper right quarter of the dial.
#[test]
fn a_wedge_counts_from_the_top_like_an_arc() {
    let fx = Fx::new(&format!("{DIAL}View v {{ Dial d 48x48, value=1, halign=left, valign=top }}"));
    assert!(fx.is_under(36.0, 12.0, "d/wedge"), "the upper right quarter shows");
    assert!(!fx.is_under(36.0, 36.0, "d/wedge"), "the lower right quarter is cut");
    assert!(!fx.is_under(12.0, 12.0, "d/wedge"), "the upper left quarter is cut");
    assert!(!fx.is_under(12.0, 36.0, "d/wedge"), "the lower left quarter is cut");
}

/// The dot of `automatic=angle(…)` is on the same dial: at 0 it sits at
/// the top, a quarter of the way around it sits at the right.
#[test]
fn an_angle_slot_counts_from_the_top_too() {
    let top = Fx::new(&format!("{DIAL}View v {{ Dial d 48x48, value=0, halign=left, valign=top }}"));
    let (x, y) = top.center("d/dot");
    assert!((x - 24.0).abs() < 0.01 && (y - 4.0).abs() < 0.01, "at 0 the dot is at 12 o'clock: {x}, {y}");
    let right = Fx::new(&format!("{DIAL}View v {{ Dial d 48x48, value=0.25, halign=left, valign=top }}"));
    let (x, y) = right.center("d/dot");
    assert!((x - 44.0).abs() < 0.01 && (y - 24.0).abs() < 0.01, "a quarter around the dot is at 3 o'clock: {x}, {y}");
}

// ----- `wrap=` on the press controls -----------------------------------------

/// `wrap=true` folds the text of a Button, a Check, a Radio and a Toggle
/// where its width binds, and the control grows as tall as its lines.
#[test]
fn the_text_of_a_press_control_wraps() {
    for (kind, open, close) in [("Button", "", ""), ("Check", "", ""), ("Toggle", "", ""), ("Radio", "choice pick { ", "; Radio other text=\"o\", visibility=gone }")] {
        let one_line = Fx::new(&format!("View v {{ Column c halign=left, valign=top {{ {open}{kind} k text=\"{LONG}\", width=150{close} }} }}"));
        let folded = Fx::new(&format!("View v {{ Column c halign=left, valign=top {{ {open}{kind} k text=\"{LONG}\", width=150, wrap=true{close} }} }}"));
        let (flat, tall): (Rect, Rect) = (one_line.rect("k"), folded.rect("k"));
        assert!(flat.h <= 44.0, "{kind} without `wrap=` holds one line: {flat:?}");
        assert!(tall.h >= 3.0 * LINE && tall.h > flat.h + LINE, "{kind} with `wrap=true` is as tall as its folded text: {tall:?} (one line: {flat:?})");
        assert_eq!(tall.w, 150.0, "{kind}: {tall:?}");
        assert!(folded.text_wraps(), "{kind} draws its text folded");
        assert!(!one_line.text_wraps());
    }
}

/// A Button that hugs its text folds it at its cap, as a Label does.
#[test]
fn a_hugging_button_folds_its_text_at_its_cap() {
    let fx = Fx::new(&format!("View v {{ Button b text=\"{LONG}\", wrap=true, max_width=150, halign=left, valign=top }}"));
    let r: Rect = fx.rect("b");
    assert_eq!(r.w, 150.0, "{r:?}");
    assert!(r.h >= 3.0 * LINE, "{r:?}");
    // Text that fits stays one line, and the Button is as wide as it.
    let short = Fx::new("View v { Button b text=\"go\", wrap=true, max_width=150, halign=left, valign=top }");
    let r: Rect = short.rect("b");
    assert!(r.w < 60.0 && r.h < 2.0 * LINE, "{r:?}");
}

// ----- the plate of an Input -------------------------------------------------

/// An Input's plate takes `border_dash=` and the four corner radii: it
/// is drawn as a Panel's plate with the same words is.
#[test]
fn an_inputs_plate_is_drawn_as_a_panels() {
    for words in ["corner_radius_top_left=10", "border_dash=[4, 2]", "border_dash=[4, 2], corner_radius_bottom_right=6", "corner_radius=5"] {
        let look = format!("width=200, height=40, halign=left, valign=top, background=#202020FF, border_thickness=2, border_color=#FFFFFFFF, {words}");
        let field = Fx::new(&format!("View v {{ Input i {look} }}"));
        let panel = Fx::new(&format!("View v {{ Panel i {look} }}"));
        assert!(!panel.quads().is_empty());
        assert_eq!(field.quads(), panel.quads(), "`{words}` on an Input");
    }
}
