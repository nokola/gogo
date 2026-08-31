//! Touch → the spec's input vocabulary. One finger owns a gesture (others
//! are swallowed while it runs): a press lands on the top-most input-taking
//! node (`hit`), firing `press` at the down and `release` exactly once when
//! the gesture ends, however it ends; a Button holds `held` (slide-off
//! clears, slide-back re-arms) and fires `activate` on a completed tap —
//! with `toggle=true` it
//! also flips `selected` and reports `change(bool)`; Check flips `selected`
//! and reports `change`; a Radio picks one of its `choice` group; a Toggle
//! is dragged or tapped and reports `change` only when it settles on the
//! other side; a Slider streams `edit` while dragged and commits `change`
//! on release when the value actually changed; a drag past the deadzone
//! inside a Scroll cancels the child's press and pans, pulling past the
//! edges with a diminishing give, releasing into a fling that `tick`
//! decays and springs back. `disabled` cascades: nothing under a disabled
//! ancestor takes input. Coordinates are epx.

use crate::draw::hit;
use crate::layout::{Rect, SLIDER_KNOB_EPX, TOGGLE_PILL};
use crate::draw::TOGGLE_KNOB_EPX;
use crate::page::*;
use crate::runtime::{FOCUSED, HELD, Resolved, Runtime, SELECTED};

/// Pan starts after this much travel (spec: 16 epx).
pub const SCROLL_DEADZONE: f32 = 16.0;
/// Release-velocity amplifier (spec `fling_boost=2`).
pub const FLING_BOOST: f32 = 2.0;
/// Exponential decay per second (spec `fling_friction=1.5`).
pub const FLING_FRICTION: f32 = 1.5;
/// Below this speed (epx/s) a fling stops.
pub const FLING_MIN_SPEED: f32 = 40.0;
/// A fling never starts faster than this (epx/s).
pub const FLING_MAX_SPEED: f32 = 6000.0;
/// Touching a list gliding faster than this stops it WITHOUT pressing the
/// child under the finger; a slower glide just stops and the tap goes on.
pub const CATCH_SPEED: f32 = 60.0;
/// Seconds of trailing finger travel that set the release velocity
/// (Android's VelocityTracker uses the same ~100 ms horizon).
pub const FLING_WINDOW: f32 = 0.1;
/// A finger resting this long before the lift means an intentional stop.
pub const FLING_STALE: f32 = 0.12;
/// Overscroll pull-back, epx/s² per epx of excess.
pub const SPRING_STIFFNESS: f32 = 200.0;
/// Under critical (2·√stiffness ≈ 28): one soft bounce.
pub const SPRING_DAMPING: f32 = 24.0;
/// Finger epx → shown epx at the edge, softening toward a quarter of the viewport.
pub const OVERSCROLL_GIVE: f32 = 0.5;
/// A Toggle finger wobble below this stays a tap.
pub const TOGGLE_DRAG_THRESHOLD: f32 = 6.0;
const VELOCITY_SAMPLES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub phase: Phase,
    pub x: f32,
    pub y: f32,
    /// Seconds, any monotonic origin.
    pub t: f64,
    /// The finger (the platform's touch id; a mouse is one fixed id).
    pub id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    /// Finger down on a press surface or a Slider (`on:press=`); on a
    /// Slider the press/release pair brackets the drag (a host pauses its
    /// live preview between them).
    Press,
    /// The end of a press-surface or Slider gesture (`on:release=`):
    /// exactly one per press, however it ends — lift inside or out, cancel,
    /// a Scroll taking the gesture, the node disabling/hiding under the
    /// finger.
    Release,
    Activate,
    Change(Value),
    Edit(Value),
    /// The raw pointer on an `on:pointer=` node: every phase, the position
    /// in the node's own epx space (0,0 = its top-left), captured until up.
    /// A mouse is one pointer — the shell feeds it as one fixed finger.
    Pointer { phase: Phase, x: f32, y: f32 },
}

/// What happened, on which node, and the `on:` action the file names for
/// it (`None` = the node's own name is the handler, per auto-wiring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub node: NodeId,
    pub kind: EventKind,
    pub action: Option<Action>,
}

/// Trailing drag deltas for the release velocity (a fixed ring).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Samples {
    dt: [f32; VELOCITY_SAMPLES],
    d: [[f32; 2]; VELOCITY_SAMPLES],
    head: usize,
    count: usize,
}

impl Samples {
    const EMPTY: Samples = Samples { dt: [0.0; VELOCITY_SAMPLES], d: [[0.0; 2]; VELOCITY_SAMPLES], head: 0, count: 0 };

    fn push(&mut self, dt: f32, d: [f32; 2]) {
        self.dt[self.head] = dt.clamp(0.001, FLING_WINDOW);
        self.d[self.head] = d;
        self.head = (self.head + 1) % VELOCITY_SAMPLES;
        self.count = (self.count + 1).min(VELOCITY_SAMPLES);
    }

    /// Finger velocity over the last ~100 ms of travel, newest backward.
    fn velocity(&self) -> [f32; 2] {
        let mut d = [0.0f32; 2];
        let mut t = 0.0f32;
        for k in 0..self.count {
            let i = (self.head + 2 * VELOCITY_SAMPLES - 1 - k) % VELOCITY_SAMPLES;
            d[0] += self.d[i][0];
            d[1] += self.d[i][1];
            t += self.dt[i];
            if t >= FLING_WINDOW {
                break;
            }
        }
        if t <= 0.0 { [0.0, 0.0] } else { [d[0] / t, d[1] / t] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    node: NodeId,
    kind: Kind,
    /// The finger that owns this gesture.
    id: u64,
    start: (f32, f32),
    /// The Scroll ancestor that may take over the gesture.
    scroll: Option<NodeId>,
    panning: bool,
    last: (f32, f32),
    last_t: f64,
    samples: Samples,
    /// A Scroll that was gliding fast when the finger landed: stopped, and
    /// the press must not activate what is under the finger.
    caught_fling: bool,
    /// The unclamped scroll position while panning (`raw` past the edges).
    accum: [f32; 2],
    /// A Slider's or Toggle's value when the finger landed (a cancelled
    /// drag restores it; a release on it reports nothing).
    value_from: f32,
    /// A Toggle press that travelled past the wobble threshold.
    toggle_moved: bool,
    /// The finger went down on a press surface and its `release` has not
    /// fired yet. Cleared when it fires, so no two end paths fire it twice
    /// (a Scroll steal ends the press while the finger keeps moving).
    release_owed: bool,
    /// An `on:pointer=` node owns the finger raw: no held, no scroll steal.
    raw: Option<Action>,
}

/// A glide (and/or a spring back from the edge) on one Scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fling {
    scroll: NodeId,
    /// Content velocity, epx/s (positive = the offset grows).
    v: [f32; 2],
    /// The unclamped position; its excess past the range is the overscroll.
    accum: [f32; 2],
}

/// One keyboard event, in gogo's vocabulary — the shell maps the OS
/// keys here and the focused `Input` control consumes them (GPAR112).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Enter,
}

/// The field's text as shown: the last code write, else the file's word.
fn input_text(page: &Page, rt: &Runtime, node: NodeId) -> String {
    match text_value(page, rt, node) {
        Value::VText { text, .. } => page.str(text).to_string(),
        _ => String::new(),
    }
}

/// The `Value` the field's events carry.
fn text_value(page: &Page, rt: &Runtime, node: NodeId) -> Value {
    rt.writes.iter().rev().find(|(n, p)| *n == node && p.key == PropKey::Text).map(|(_, p)| p.value).or_else(|| page.prop(node, PropKey::Text)).unwrap_or(Value::VText { text: 0, key: NO_KEY })
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    press: Option<Press>,
    fling: Option<Fling>,
    /// The offset this machine last wrote for `fling.scroll`: a different
    /// value means code moved the scroll — adopt it and stand down.
    expected: Option<(NodeId, [f32; 2])>,
    /// The focused `Input` control — keystrokes go to it; a tap anywhere
    /// else is the focus-loss commit door (GPAR112).
    focus: Option<NodeId>,
    /// The caret, in CHARS into the field's text.
    caret: usize,
    /// The text at focus (or the last commit): `change` fires once per
    /// genuine transition — the shipped changed-means-CHANGED law.
    committed: String,
}

fn event_action(page: &Page, node: NodeId, event: &str) -> Option<Action> {
    let n = &page.nodes[node as usize];
    let id = page.intern_lookup(event);
    page.handlers[n.handlers.0 as usize..(n.handlers.0 + n.handlers.1) as usize].iter().find(|h| h.event == id).map(|h| h.action)
}

fn scroll_ancestor(page: &Page, node: NodeId) -> Option<NodeId> {
    let mut n = page.nodes[node as usize].parent;
    while n != NO_NODE {
        if page.nodes[n as usize].kind == Kind::Scroll {
            return Some(n);
        }
        n = page.nodes[n as usize].parent;
    }
    None
}

/// Is `node` inside `root`'s subtree (`root` itself counts)? The
/// fling-catch uses it: only a touch that lands ON the gliding Scroll
/// stops the glide — chrome drawn over it keeps its own press (D130).
fn within(page: &Page, node: NodeId, root: NodeId) -> bool {
    let mut n = node;
    while n != NO_NODE {
        if n == root {
            return true;
        }
        n = page.nodes[n as usize].parent;
    }
    false
}

/// The innermost Scroll around `node` (`node` itself counts) that scrolls
/// the way the finger committed to; `None` = nothing here scrolls that way.
fn scroll_taker(page: &Page, res: &Resolved, node: NodeId, vertical: bool) -> Option<NodeId> {
    let mut n = node;
    while n != NO_NODE {
        if page.nodes[n as usize].kind == Kind::Scroll {
            let axes = scroll_axes(page, res, n);
            if if vertical { axes.1 } else { axes.0 } {
                return Some(n);
            }
        }
        n = page.nodes[n as usize].parent;
    }
    None
}

/// How far a Scroll's content can move: (max_x, max_y) in epx.
fn scroll_range(page: &Page, rects: &[Rect], scroll: NodeId) -> [f32; 2] {
    let viewport = rects[scroll as usize];
    let mut max = [0.0f32; 2];
    for c in page.children(scroll) {
        let r = rects[c];
        max[0] = max[0].max(r.w - viewport.w);
        max[1] = max[1].max(r.h - viewport.h);
    }
    max
}

/// Which axes a Scroll pans: `allow=` when present, else the triad reading
/// (`layout::scroll_axes_of`, the one rule).
fn scroll_axes(page: &Page, res: &Resolved, scroll: NodeId) -> (bool, bool) {
    crate::layout::scroll_axes_of(page, res, scroll)
}

/// Diminishing overscroll: excess epx → shown epx. `OVERSCROLL_GIVE` slope
/// at the edge, flattening toward a quarter of the viewport span.
fn soft(raw: f32, span: f32) -> f32 {
    if raw == 0.0 {
        return 0.0;
    }
    let cap = (span * 0.25).max(1.0);
    cap * (OVERSCROLL_GIVE * raw / cap).tanh()
}

/// Splits an unclamped position into the clamped offset plus the softened
/// overscroll the content shows; writes it; returns the raw excess.
fn apply_accum(rt: &mut Runtime, rects: &[Rect], scroll: NodeId, range: [f32; 2], axes: (bool, bool), accum: &mut [f32; 2]) -> [f32; 2] {
    let viewport = rects[scroll as usize];
    let mut raw = [0.0f32; 2];
    let mut shown = [0.0f32; 2];
    for i in 0..2 {
        let on = if i == 0 { axes.0 } else { axes.1 };
        let clamped = accum[i].clamp(0.0, range[i]);
        if !on {
            // A disabled axis never overscrolls.
            accum[i] = clamped;
        }
        raw[i] = accum[i] - clamped;
        shown[i] = clamped + soft(raw[i], if i == 0 { viewport.w } else { viewport.h });
    }
    if rt.scroll[scroll as usize] != shown {
        rt.scroll[scroll as usize] = shown;
        rt.dirty = true;
    }
    raw
}

fn slider_value(page: &Page, res: &Resolved, rects: &[Rect], node: NodeId, x: f32) -> f32 {
    let min = res.num(page, node, PropKey::Min).unwrap_or(0.0);
    let max = res.num(page, node, PropKey::Max).unwrap_or(100.0);
    let r = rects[node as usize];
    // Fraction 0 and 1 park the knob's EDGES at the track's ends, so the
    // value tracks the knob's center.
    let span = r.w - SLIDER_KNOB_EPX;
    let t = if span > 0.0 { ((x - r.x - SLIDER_KNOB_EPX / 2.0) / span).clamp(0.0, 1.0) } else { 0.0 };
    snap_step(page, res, node, min + (max - min) * t)
}

/// `step=`: quantized from `min`, then clamped (Range's rule).
fn snap_step(page: &Page, res: &Resolved, node: NodeId, v: f32) -> f32 {
    res.snap_slider(page, node, v)
}

/// The value as shown: a write this frame (not yet resolved) or the
/// resolved one, quantized either way.
fn value_of(rt: &Runtime, res: &Resolved, page: &Page, node: NodeId) -> f32 {
    let raw = rt.writes.iter().find(|(n, w)| *n == node && w.key == PropKey::Value).and_then(|(_, w)| if let Value::VNum(v) = w.value { Some(v) } else { None }).or_else(|| res.num(page, node, PropKey::Value)).unwrap_or(0.0);
    res.snap_slider(page, node, raw)
}

/// A Toggle's knob fraction under the finger: the pill sits at the node's
/// padded left, the knob's edges park at the pill's ends.
fn toggle_fraction(res: &Resolved, rects: &[Rect], node: NodeId, x: f32) -> f32 {
    let r = rects[node as usize];
    let pad = res.placement[node as usize].padding[0];
    let pill_x = r.x + pad;
    let span = TOGGLE_PILL.0 - TOGGLE_KNOB_EPX;
    if span <= 0.0 { 0.0 } else { ((x - pill_x - TOGGLE_KNOB_EPX / 2.0) / span).clamp(0.0, 1.0) }
}

impl Input {
    pub fn new() -> Input {
        Input::default()
    }

    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    /// Is a Scroll gliding or springing back?
    pub fn is_gliding(&self) -> bool {
        self.fling.is_some()
    }

    fn push_event(&self, rt: &mut Runtime, events: &mut Vec<Event>, node: NodeId, kind: EventKind, action: Option<Action>) {
        // `go(x)` / `toggle(x)` flip the view's arrangement right here — a
        // declarative wiring, no host handler.
        match action {
            Some(Action::Go(a)) => rt.go(a),
            Some(Action::ToggleArrangement(a)) => rt.toggle(a),
            _ => {}
        }
        events.push(Event { node, kind, action });
    }

    /// Fires the `release` a press still owes. Needed so a hold-to-repeat
    /// host always sees its stop: every end path (lift, cancel, scroll
    /// steal, disable/hide under the finger) comes through here, and the
    /// owed flag keeps it to exactly one per press.
    fn release(&self, page: &Page, rt: &mut Runtime, events: &mut Vec<Event>, p: &mut Press) {
        if !p.release_owed {
            return;
        }
        p.release_owed = false;
        if let Some(action) = event_action(page, p.node, "release") {
            self.push_event(rt, events, p.node, EventKind::Release, Some(action));
        }
    }

    /// A gesture ends without a release: the press and any half-done value
    /// go back to where the finger found them.
    fn cancel_press(&mut self, page: &Page, rt: &mut Runtime, p: &Press) {
        rt.set_state(p.node, HELD, false);
        match p.kind {
            Kind::Slider => rt.set_prop(p.node, PropKey::Value, Value::VNum(p.value_from)),
            Kind::Toggle => {
                rt.set_prop(p.node, PropKey::Value, Value::VNum(p.value_from));
                rt.set_state(p.node, SELECTED, p.value_from >= 0.5);
            }
            _ => {}
        }
        let _ = page;
    }

    /// Forgets the gesture in flight and any glide: for the host to call
    /// when the page leaves the screen, so a list closed mid-fling is
    /// where the user left it on return and no `held` ghost survives a
    /// page swap. Nothing is reported (the finger is not on this page).
    pub fn reset(&mut self, rt: &mut Runtime) {
        if let Some(p) = self.press.take() {
            rt.set_state(p.node, HELD, false);
        }
        self.fling = None;
        self.expected = None;
        // Focus is forgotten WITHOUT a commit: the page left the screen,
        // reporting now would land on nobody.
        if let Some(f) = self.focus.take() {
            rt.set_state(f, FOCUSED, false);
        }
        rt.caret = None;
        self.caret = 0;
        self.committed.clear();
    }

    /// One keyboard event to the focused Input (GPAR112): characters
    /// edit at the caret and stream `edit`, Enter commits `change` once
    /// per genuine change (the shipped changed-means-CHANGED law),
    /// arrows/Home/End move the caret. Nothing focused = nothing happens.
    pub fn key(&mut self, page: &mut Page, rt: &mut Runtime, k: Key) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(f) = self.focus else { return events };
        let mut text: Vec<char> = input_text(page, rt, f).chars().collect();
        self.caret = self.caret.min(text.len());
        match k {
            Key::Char(c) if !c.is_control() => {
                // `max_length=` caps typing at that many characters
                // (Godot's LineEdit word; GPAR112). Absent or 0 = no cap.
                let cap = match page.prop(f, PropKey::MaxLength) {
                    Some(Value::VNum(n)) if n > 0.0 => n as usize,
                    _ => usize::MAX,
                };
                if text.len() < cap {
                    text.insert(self.caret, c);
                    self.caret += 1;
                }
            }
            Key::Backspace if self.caret > 0 => {
                self.caret -= 1;
                text.remove(self.caret);
            }
            Key::Delete if self.caret < text.len() => {
                text.remove(self.caret);
            }
            Key::Left => self.caret = self.caret.saturating_sub(1),
            Key::Right => self.caret = (self.caret + 1).min(text.len()),
            Key::Home => self.caret = 0,
            Key::End => self.caret = text.len(),
            Key::Enter => {
                let now: String = text.iter().collect();
                if now != self.committed {
                    self.committed = now;
                    self.push_event(rt, &mut events, f, EventKind::Change(text_value(page, rt, f)), event_action(page, f, "change"));
                }
                return events;
            }
            _ => return events,
        }
        rt.caret = Some((f, self.caret as u32));
        rt.dirty = true;
        if matches!(k, Key::Left | Key::Right | Key::Home | Key::End) {
            return events;
        }
        let now: String = text.iter().collect();
        rt.set_text(page, f, &now);
        self.push_event(rt, &mut events, f, EventKind::Edit(text_value(page, rt, f)), event_action(page, f, "edit"));
        events
    }

    /// Focus leaves the field: commit once when the text CHANGED since
    /// focus, and the caret goes away.
    fn blur(&mut self, page: &Page, rt: &mut Runtime, events: &mut Vec<Event>) {
        let Some(f) = self.focus.take() else { return };
        rt.set_state(f, FOCUSED, false);
        rt.caret = None;
        let now = input_text(page, rt, f);
        if now != self.committed {
            self.push_event(rt, events, f, EventKind::Change(text_value(page, rt, f)), event_action(page, f, "change"));
        }
        self.committed.clear();
    }

    /// Feeds one touch sample; returns the events it completed.
    pub fn touch(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], t: Touch) -> Vec<Event> {
        let mut events = Vec::new();
        match t.phase {
            Phase::Down => {
                if self.press.is_some() {
                    // A second finger mid-gesture is swallowed.
                    return events;
                }
                // A glide is stopped only by a touch that lands ON the
                // gliding Scroll or its content — chrome drawn over it
                // keeps its own press and the list keeps rolling
                // underneath (D130, GPAR4). A FAST catch is "caught": the
                // child under the finger is not pressed by accident.
                let hit_node = hit(page, res, rects, t.x, t.y, false);
                let mut caught_accum: Option<(NodeId, [f32; 2])> = None;
                let caught = match self.fling.take() {
                    Some(f) if hit_node.is_some_and(|n| within(page, n, f.scroll)) => {
                        let range = scroll_range(page, rects, f.scroll);
                        let raw = [f.accum[0] - f.accum[0].clamp(0.0, range[0]), f.accum[1] - f.accum[1].clamp(0.0, range[1])];
                        // The pull stays under the finger (GPAR5, the
                        // shipped catch): freezing a mid-spring bounce
                        // keeps the softened offset — snapping it to the
                        // edge would jump the content on touch.
                        let mut accum = f.accum;
                        let axes = scroll_axes(page, res, f.scroll);
                        apply_accum(rt, rects, f.scroll, range, axes, &mut accum);
                        caught_accum = Some((f.scroll, accum));
                        self.expected = Some((f.scroll, rt.scroll[f.scroll as usize]));
                        (f.v[0].powi(2) + f.v[1].powi(2)).sqrt() >= CATCH_SPEED || raw != [0.0, 0.0]
                    }
                    fling => {
                        self.fling = fling;
                        false
                    }
                };
                let Some(node) = hit_node else {
                    // A tap on nothing still blurs the field (the
                    // focus-loss commit door).
                    self.blur(page, rt, &mut events);
                    return events;
                };
                let kind = page.nodes[node as usize].kind;
                if rt.disabled(page, node) {
                    return events;
                }
                // A tap on an Input takes the caret; a tap on anything
                // else commits-and-blurs (GPAR112 — the shipped doors:
                // Enter, blur; once per genuine change).
                if kind == Kind::Input {
                    if self.focus != Some(node) {
                        self.blur(page, rt, &mut events);
                        self.focus = Some(node);
                        self.committed = input_text(page, rt, node);
                        rt.set_state(node, FOCUSED, true);
                    }
                    self.caret = input_text(page, rt, node).chars().count();
                    rt.caret = Some((node, self.caret as u32));
                    rt.dirty = true;
                } else {
                    self.blur(page, rt, &mut events);
                }
                let raw = event_action(page, node, "pointer");
                if let Some(action) = raw {
                    let r = rects[node as usize];
                    self.push_event(rt, &mut events, node, EventKind::Pointer { phase: Phase::Down, x: t.x - r.x, y: t.y - r.y }, Some(action));
                    self.press = Some(Press { node, kind, id: t.id, start: (t.x, t.y), scroll: None, panning: false, last: (t.x, t.y), last_t: t.t, samples: Samples::EMPTY, caught_fling: caught, accum: [0.0; 2], value_from: 0.0, toggle_moved: false, release_owed: false, raw });
                    return events;
                }
                let scroll = if kind == Kind::Scroll { Some(node) } else { scroll_ancestor(page, node) };
                let mut value_from = 0.0;
                let pressed = (kind.is_press_surface() || kind == Kind::Slider) && !caught;
                if pressed {
                    rt.set_state(node, HELD, true);
                    if let Some(action) = event_action(page, node, "press") {
                        self.push_event(rt, &mut events, node, EventKind::Press, Some(action));
                    }
                }
                if kind == Kind::Toggle {
                    value_from = if rt.writes.iter().any(|(n, w)| *n == node && w.key == PropKey::Value) { value_of(rt, res, page, node) } else if rt.has(node, SELECTED) { 1.0 } else { 0.0 };
                }
                if kind == Kind::Slider && !caught {
                    value_from = value_of(rt, res, page, node);
                    let v = slider_value(page, res, rects, node, t.x);
                    // `edit` streams CHANGED values only (the shipped
                    // set_param no-change guard, GPAR6): a press on the
                    // current value says nothing.
                    if v != value_from {
                        rt.set_prop(node, PropKey::Value, Value::VNum(v));
                        self.push_event(rt, &mut events, node, EventKind::Edit(Value::VNum(v)), event_action(page, node, "edit"));
                    }
                }
                // A caught overscroll hands its RAW accum to the press, so
                // the pan continues from the pull, not from the edge.
                let accum = match (scroll, caught_accum) {
                    (Some(s), Some((cs, a))) if s == cs => a,
                    (Some(s), _) => rt.scroll[s as usize],
                    (None, _) => [0.0; 2],
                };
                self.press = Some(Press { node, kind, id: t.id, start: (t.x, t.y), scroll, panning: false, last: (t.x, t.y), last_t: t.t, samples: Samples::EMPTY, caught_fling: caught, accum, value_from, toggle_moved: false, release_owed: pressed, raw: None });
            }
            Phase::Move => {
                let Some(mut p) = self.press else { return events };
                if p.id != t.id {
                    return events;
                }
                let dead = |n: NodeId| rt.disabled(page, n) || res.gone(page, n) || res.hidden(page, n);
                if p.panning {
                    // The Scroll owns the gesture now: only ITS death ends it;
                    // the child the finger landed on may hide under the pan.
                    if p.scroll.map_or(false, dead) {
                        self.press = None;
                        return events;
                    }
                } else if dead(p.node) {
                    // Disabled or hidden under the finger: the gesture ends
                    // here, and a raw owner hears the cancel (it would
                    // otherwise keep its grab until the next touch).
                    if let Some(action) = p.raw {
                        let r = rects[p.node as usize];
                        self.push_event(rt, &mut events, p.node, EventKind::Pointer { phase: Phase::Cancel, x: t.x - r.x, y: t.y - r.y }, Some(action));
                    }
                    self.release(page, rt, &mut events, &mut p);
                    self.cancel_press(page, rt, &p);
                    self.press = None;
                    return events;
                }
                if let Some(action) = p.raw {
                    let r = rects[p.node as usize];
                    self.push_event(rt, &mut events, p.node, EventKind::Pointer { phase: Phase::Move, x: t.x - r.x, y: t.y - r.y }, Some(action));
                    p.last = (t.x, t.y);
                    p.last_t = t.t;
                    self.press = Some(p);
                    return events;
                }
                let dt = (t.t - p.last_t).max(1e-4) as f32;
                let delta = (t.x - p.last.0, t.y - p.last.1);
                p.samples.push(dt, [delta.0, delta.1]);
                if !p.panning && p.scroll.is_some() {
                    let dx = (t.x - p.start.0).abs();
                    let dy = (t.y - p.start.1).abs();
                    if (dx * dx + dy * dy).sqrt() > SCROLL_DEADZONE {
                        // Decided once, by the direction the finger committed
                        // to: a Slider keeps a sideways drag (its own axis), a
                        // Toggle keeps a drag it already started, otherwise the
                        // innermost Scroll that scrolls THAT way takes the
                        // gesture and the press is cancelled (a Slider's value
                        // goes back to where the finger found it). Nothing
                        // scrolls that way: the child keeps the finger for good.
                        let vertical = dy > dx;
                        let child_keeps = (p.kind == Kind::Slider && !vertical) || (p.kind == Kind::Toggle && p.toggle_moved);
                        let taker = if child_keeps { None } else { scroll_taker(page, res, p.node, vertical) };
                        match taker {
                            Some(scroll) => {
                                p.scroll = Some(scroll);
                                p.panning = true;
                                self.release(page, rt, &mut events, &mut p);
                                self.cancel_press(page, rt, &p);
                                p.accum = rt.scroll[scroll as usize];
                            }
                            None => p.scroll = None,
                        }
                    }
                }
                if p.kind == Kind::Slider && !p.panning {
                    let v = slider_value(page, res, rects, p.node, t.x);
                    // One `edit` per crossed step, none for a same-value
                    // sample (GPAR6).
                    if v != value_of(rt, res, page, p.node) {
                        rt.set_prop(p.node, PropKey::Value, Value::VNum(v));
                        self.push_event(rt, &mut events, p.node, EventKind::Edit(Value::VNum(v)), event_action(page, p.node, "edit"));
                    }
                } else if p.kind == Kind::Toggle && !p.panning && (p.toggle_moved || (t.x - p.start.0).abs() >= TOGGLE_DRAG_THRESHOLD) {
                    // Past the wobble the knob follows the finger; `selected`
                    // flips live as it crosses the middle.
                    p.toggle_moved = true;
                    let f = toggle_fraction(res, rects, p.node, t.x);
                    rt.set_prop(p.node, PropKey::Value, Value::VNum(f));
                    rt.set_state(p.node, SELECTED, f >= 0.5);
                } else if p.panning {
                    if let Some(scroll) = p.scroll {
                        let axes = scroll_axes(page, res, scroll);
                        let range = scroll_range(page, rects, scroll);
                        if axes.0 {
                            p.accum[0] -= delta.0;
                        }
                        if axes.1 {
                            p.accum[1] -= delta.1;
                        }
                        apply_accum(rt, rects, scroll, range, axes, &mut p.accum);
                        self.expected = Some((scroll, rt.scroll[scroll as usize]));
                    }
                }
                if p.kind.is_press_surface() && !p.panning && !p.caught_fling {
                    // Slide-off clears the press, slide-back re-arms it. The
                    // node's OWN rect decides, not the top-most hit — a
                    // plate opening over it mid-press is not a slide-off.
                    let inside = rects[p.node as usize].contains(t.x, t.y);
                    rt.set_state(p.node, HELD, inside);
                }
                p.last = (t.x, t.y);
                p.last_t = t.t;
                self.press = Some(p);
            }
            Phase::Up | Phase::Cancel => {
                let Some(mut p) = self.press else { return events };
                if p.id != t.id {
                    return events;
                }
                self.press = None;
                if let Some(action) = p.raw {
                    let r = rects[p.node as usize];
                    self.push_event(rt, &mut events, p.node, EventKind::Pointer { phase: t.phase, x: t.x - r.x, y: t.y - r.y }, Some(action));
                    return events;
                }
                rt.set_state(p.node, HELD, false);
                self.release(page, rt, &mut events, &mut p);
                if p.panning {
                    if let Some(scroll) = p.scroll {
                        let axes = scroll_axes(page, res, scroll);
                        let range = scroll_range(page, rects, scroll);
                        // Velocity from the trailing window; a finger that
                        // rested before lifting gives no glide. Masked to
                        // the enabled axes BEFORE anything else, so a
                        // sideways flick never leaves a component the
                        // integrator would drift on.
                        let finger = if (t.t - p.last_t) as f32 > FLING_STALE { [0.0, 0.0] } else { p.samples.velocity() };
                        let mut v = [-finger[0] * FLING_BOOST * if axes.0 { 1.0 } else { 0.0 }, -finger[1] * FLING_BOOST * if axes.1 { 1.0 } else { 0.0 }];
                        let speed = (v[0].powi(2) + v[1].powi(2)).sqrt();
                        if speed > FLING_MAX_SPEED {
                            v = [v[0] / speed * FLING_MAX_SPEED, v[1] / speed * FLING_MAX_SPEED];
                        }
                        let mut accum = p.accum;
                        let raw = apply_accum(rt, rects, scroll, range, axes, &mut accum);
                        if speed >= FLING_MIN_SPEED || raw != [0.0, 0.0] {
                            self.fling = Some(Fling { scroll, v, accum });
                        }
                        self.expected = Some((scroll, rt.scroll[scroll as usize]));
                    }
                    return events;
                }
                if t.phase == Phase::Cancel {
                    self.cancel_press(page, rt, &p);
                    return events;
                }
                if p.caught_fling {
                    return events;
                }
                // Released inside = inside the pressed node's OWN rect (a dialog
                // opening under the finger does not turn the tap into a miss).
                let inside = rects[p.node as usize].contains(t.x, t.y) && !(rt.disabled(page, p.node) || res.gone(page, p.node) || res.hidden(page, p.node));
                match p.kind {
                    Kind::Slider => {
                        // changed means CHANGED: a release on the value the
                        // finger found reports nothing.
                        let v = value_of(rt, res, page, p.node);
                        if v != p.value_from {
                            self.push_event(rt, &mut events, p.node, EventKind::Change(Value::VNum(v)), event_action(page, p.node, "change"));
                        }
                    }
                    // Order on every toggling surface: `change` first, then
                    // `activate` (Godot's BaseButton emits toggled before
                    // pressed) — an activate handler reads the new value.
                    Kind::Button if inside => {
                        if res.bool(page, p.node, PropKey::ToggleMode).unwrap_or(false) {
                            let on = !rt.has(p.node, SELECTED);
                            rt.set_state(p.node, SELECTED, on);
                            self.push_event(rt, &mut events, p.node, EventKind::Change(Value::VBool(on)), event_action(page, p.node, "change"));
                        }
                        self.push_event(rt, &mut events, p.node, EventKind::Activate, event_action(page, p.node, "activate"));
                    }
                    Kind::ListItem if inside => {
                        self.push_event(rt, &mut events, p.node, EventKind::Activate, event_action(page, p.node, "activate"));
                    }
                    Kind::Check if inside => {
                        let on = !rt.has(p.node, SELECTED);
                        rt.set_state(p.node, SELECTED, on);
                        self.push_event(rt, &mut events, p.node, EventKind::Change(Value::VBool(on)), event_action(page, p.node, "change"));
                        self.push_event(rt, &mut events, p.node, EventKind::Activate, event_action(page, p.node, "activate"));
                    }
                    // A release ALWAYS settles a Toggle (the shipped
                    // input_ended takes no rect test, GPAR8): slide off in
                    // any direction and lift — a drag lands by side, an
                    // unmoved press flips like a tap.
                    Kind::Toggle => {
                        let was = p.value_from >= 0.5;
                        let f = if p.toggle_moved { value_of(rt, res, page, p.node) } else { p.value_from };
                        // A drag settles to the side the knob is on; a tap goes
                        // to the other end.
                        let on = if p.toggle_moved { f >= 0.5 } else { !was };
                        rt.set_prop(p.node, PropKey::Value, Value::VNum(if on { 1.0 } else { 0.0 }));
                        rt.set_state(p.node, SELECTED, on);
                        if on != was {
                            self.push_event(rt, &mut events, p.node, EventKind::Change(Value::VBool(on)), event_action(page, p.node, "change"));
                        }
                        self.push_event(rt, &mut events, p.node, EventKind::Activate, event_action(page, p.node, "activate"));
                    }
                    Kind::Radio if inside => {
                        // One of N across the whole `choice` scope (nested
                        // Rows included) — every Radio has one, a lone Radio
                        // is refused at build (G0069). Re-tapping the current
                        // pick stays pressed and reports nothing.
                        let already = rt.has(p.node, SELECTED);
                        let choice = page.nodes[p.node as usize].choice;
                        let mut change_action: Option<Action> = None;
                        if choice != NO_CHOICE {
                            for (i, n) in page.nodes.iter().enumerate() {
                                if n.kind == Kind::Radio && n.choice == choice {
                                    rt.set_state(i as NodeId, SELECTED, i as NodeId == p.node);
                                }
                            }
                            change_action = page.choices[choice as usize].action;
                        }
                        if !already {
                            self.push_event(rt, &mut events, p.node, EventKind::Change(Value::VIdent(page.nodes[p.node as usize].name)), change_action);
                        }
                        self.push_event(rt, &mut events, p.node, EventKind::Activate, event_action(page, p.node, "activate"));
                    }
                    _ => {}
                }
            }
        }
        events
    }

    /// Advances a fling / edge spring by `dt` seconds; true while something
    /// still moves. A scroll offset code moved since the last write is
    /// adopted: the glide stands down rather than fight it.
    pub fn tick(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], dt: f32) -> bool {
        let Some(mut f) = self.fling else { return false };
        if let Some((s, expected)) = self.expected {
            if s == f.scroll && rt.scroll[s as usize] != expected {
                self.fling = None;
                self.expected = None;
                return false;
            }
        }
        let axes = scroll_axes(page, res, f.scroll);
        let range = scroll_range(page, rects, f.scroll);
        let before = rt.scroll[f.scroll as usize];
        for i in 0..2 {
            let on = if i == 0 { axes.0 } else { axes.1 };
            if !on {
                f.v[i] = 0.0;
                continue;
            }
            let raw = f.accum[i] - f.accum[i].clamp(0.0, range[i]);
            if raw != 0.0 {
                // Beyond an edge: a damped spring pulls the excess back.
                f.v[i] += (-SPRING_STIFFNESS * raw - SPRING_DAMPING * f.v[i]) * dt;
            } else {
                f.v[i] *= (-FLING_FRICTION * dt).exp();
            }
            f.accum[i] += f.v[i] * dt;
        }
        let raw = apply_accum(rt, rects, f.scroll, range, axes, &mut f.accum);
        let moved = rt.scroll[f.scroll as usize] != before;
        let speed = (f.v[0].powi(2) + f.v[1].powi(2)).sqrt();
        if speed < FLING_MIN_SPEED && (raw[0].powi(2) + raw[1].powi(2)).sqrt() < 0.75 {
            // Landed: exactly on the clamped position, nothing left to do.
            let mut accum = [f.accum[0].clamp(0.0, range[0]), f.accum[1].clamp(0.0, range[1])];
            apply_accum(rt, rects, f.scroll, range, axes, &mut accum);
            self.expected = Some((f.scroll, rt.scroll[f.scroll as usize]));
            self.fling = None;
            return moved;
        }
        self.expected = Some((f.scroll, rt.scroll[f.scroll as usize]));
        self.fling = Some(f);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;
    use crate::layout::{FixedAdvance, layout};
    use crate::runtime::{DISABLED, resolve};

    struct Fx {
        page: Page,
        rt: Runtime,
        input: Input,
    }

    impl Fx {
        fn new(src: &str) -> Fx {
            let c = compile(src, &|_| None);
            assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
            let rt = Runtime::new(&c.page);
            Fx { page: c.page, rt, input: Input::new() }
        }
        fn lay(&self) -> (Resolved, Vec<Rect>) {
            let res = resolve(&self.page, &self.rt);
            let rects = layout(&self.page, &res, Rect::new(0.0, 0.0, 400.0, 800.0), &FixedAdvance);
            (res, rects)
        }
        fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
            self.touch_id(phase, x, y, t, 1)
        }
        fn touch_id(&mut self, phase: Phase, x: f32, y: f32, t: f64, id: u64) -> Vec<Event> {
            let (res, rects) = self.lay();
            self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase, x, y, t, id })
        }
        fn tick(&mut self, dt: f32) -> bool {
            let (res, rects) = self.lay();
            self.input.tick(&self.page, &mut self.rt, &res, &rects, dt)
        }
    }

    #[test]
    fn tap_slide_off_and_slide_back() {
        let mut fx = Fx::new("View v { Button b width=100, height=40, halign=left, valign=top, on:activate=go_home }");
        let b = fx.page.find("b").unwrap();
        assert!(fx.touch(Phase::Down, 10.0, 10.0, 0.0).is_empty());
        assert!(fx.rt.has(b, HELD));
        fx.touch(Phase::Move, 200.0, 200.0, 0.1);
        assert!(!fx.rt.has(b, HELD), "slide-off clears");
        fx.touch(Phase::Move, 20.0, 20.0, 0.2);
        assert!(fx.rt.has(b, HELD), "slide-back re-arms");
        let ev = fx.touch(Phase::Up, 20.0, 20.0, 0.3);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, EventKind::Activate);
        assert!(matches!(ev[0].action, Some(Action::Named(id)) if fx.page.str(id) == "go_home"));
        assert!(!fx.rt.has(b, HELD));
        // A release outside fires nothing.
        fx.touch(Phase::Down, 10.0, 10.0, 1.0);
        assert!(fx.touch(Phase::Up, 300.0, 300.0, 1.1).is_empty());
    }

    /// D103: everything passive is transparent, always — a wired handler
    /// alone (`on:show=`) never claims input, so a full-screen Panel with
    /// one wired handler must not eat the tap meant for the Button under it.
    #[test]
    fn passive_node_with_handler_stays_transparent() {
        let mut fx = Fx::new("View v { Layers z { Button b width=100, height=40, halign=left, valign=top, on:activate=go_home; Panel cover width=fill, height=fill, on:show=shown } }");
        let b = fx.page.find("b").unwrap();
        fx.touch(Phase::Down, 10.0, 10.0, 0.0);
        assert!(fx.rt.has(b, HELD), "the tap lands on the Button, not the covering Panel");
        let ev = fx.touch(Phase::Up, 10.0, 10.0, 0.1);
        assert!(ev.iter().any(|e| e.kind == EventKind::Activate));
    }

    /// `press` fires at the down; its `release` exactly once when the
    /// gesture ends, HOWEVER it ends — a hold-to-repeat host must always
    /// see its stop.
    #[test]
    fn press_release_pair_however_the_gesture_ends() {
        let src = "View v { Button b width=100, height=40, halign=left, valign=top, on:press=start_repeat, on:release=stop_repeat, on:activate=go_home }";
        // Lift inside: the release comes before the tap's activate.
        let mut fx = Fx::new(src);
        let ev = fx.touch(Phase::Down, 10.0, 10.0, 0.0);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, EventKind::Press);
        let ev = fx.touch(Phase::Up, 10.0, 10.0, 0.1);
        assert_eq!(ev.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![EventKind::Release, EventKind::Activate]);
        // A lift after a slide-off: no activate, the release still fires.
        fx.touch(Phase::Down, 10.0, 10.0, 1.0);
        fx.touch(Phase::Move, 300.0, 300.0, 1.1);
        let ev = fx.touch(Phase::Up, 300.0, 300.0, 1.2);
        assert_eq!(ev.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![EventKind::Release]);
        // A cancelled touch releases too.
        fx.touch(Phase::Down, 10.0, 10.0, 2.0);
        let ev = fx.touch(Phase::Cancel, 10.0, 10.0, 2.1);
        assert_eq!(ev.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![EventKind::Release]);
        // Disabled under the finger mid-gesture: the release fires there.
        fx.touch(Phase::Down, 10.0, 10.0, 3.0);
        let b = fx.page.find("b").unwrap();
        fx.rt.set_state(b, DISABLED, true);
        let ev = fx.touch(Phase::Move, 12.0, 12.0, 3.1);
        assert_eq!(ev.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![EventKind::Release]);
        // A Scroll steal releases at the steal, and never a second time.
        let mut fx = Fx::new("View v { Scroll s height=200 { Column c { Button a height=100, on:press=start_repeat, on:release=stop_repeat; Button b height=100; Button d height=100; Button e height=100 } } }");
        assert_eq!(fx.touch(Phase::Down, 50.0, 50.0, 0.0).len(), 1);
        let ev = fx.touch(Phase::Move, 50.0, 20.0, 0.02);
        assert_eq!(ev.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![EventKind::Release], "the steal releases");
        assert!(fx.touch(Phase::Up, 50.0, 10.0, 0.03).is_empty(), "the pan's lift releases nothing again");
        // A caught fling pressed nothing, so it releases nothing.
        let s = fx.page.find("s").unwrap();
        fx.input.fling = Some(Fling { scroll: s, v: [0.0, -500.0], accum: [0.0, 100.0] });
        assert!(fx.touch(Phase::Down, 50.0, 50.0, 2.0).is_empty());
        assert!(fx.touch(Phase::Up, 50.0, 50.0, 2.1).is_empty());
    }

    #[test]
    fn scroll_pan_cancels_the_press_and_flings() {
        let mut fx = Fx::new("View v { Scroll s height=200 { Column c { Button a height=100; Button b height=100; Button d height=100; Button e height=100 } } }");
        let a = fx.page.find("a").unwrap();
        let s = fx.page.find("s").unwrap();
        fx.touch(Phase::Down, 50.0, 50.0, 0.0);
        assert!(fx.rt.has(a, HELD));
        fx.touch(Phase::Move, 50.0, 45.0, 0.01);
        assert!(fx.rt.has(a, HELD), "inside the deadzone the press stands");
        fx.touch(Phase::Move, 50.0, 20.0, 0.02);
        assert!(!fx.rt.has(a, HELD), "past the deadzone the Scroll owns the gesture");
        // The pan starts from where the deadzone broke, not from the press.
        assert_eq!(fx.rt.scroll[s as usize][1], 25.0);
        let ev = fx.touch(Phase::Up, 50.0, 10.0, 0.03);
        assert!(ev.is_empty(), "a pan never activates");
        assert!(fx.input.fling.is_some());
        let before = fx.rt.scroll[s as usize][1];
        assert!(fx.tick(0.016));
        assert!(fx.rt.scroll[s as usize][1] > before, "the glide keeps going");
        // Content is 400 tall in a 200 viewport: past 200 only by the soft
        // give, and the spring brings it home.
        let mut peak = 0.0f32;
        for _ in 0..400 {
            fx.tick(0.016);
            peak = peak.max(fx.rt.scroll[s as usize][1]);
        }
        assert!(peak <= 250.0, "overscroll is capped at a quarter viewport: {peak}");
        assert!(fx.input.fling.is_none());
        assert_eq!(fx.rt.scroll[s as usize][1], 200.0, "landed exactly on the edge");
        // Touching a FAST-gliding list stops it without pressing what is under the finger.
        fx.input.fling = Some(Fling { scroll: s, v: [0.0, -500.0], accum: [0.0, 100.0] });
        fx.touch(Phase::Down, 50.0, 50.0, 2.0);
        assert!(fx.input.fling.is_none());
        assert!(fx.touch(Phase::Up, 50.0, 50.0, 2.1).is_empty());
        // A slow glide just stops; the tap goes through.
        fx.input.fling = Some(Fling { scroll: s, v: [0.0, -45.0], accum: [0.0, 100.0] });
        fx.touch(Phase::Down, 50.0, 50.0, 3.0);
        assert!(fx.input.fling.is_none());
        assert_eq!(fx.touch(Phase::Up, 50.0, 50.0, 3.1).len(), 1);
    }

    /// The release velocity is the trailing 100 ms window, a resting finger
    /// gives no glide, and a flick is capped at 6000 epx/s.
    #[test]
    fn release_velocity_window_stale_and_cap() {
        let src = "View v { Scroll s height=200 { Column c { Button a height=100; Button b height=100; Button d height=100; Button e height=100 } } }";
        let mut fx = Fx::new(src);
        fx.touch(Phase::Down, 50.0, 190.0, 0.0);
        fx.touch(Phase::Move, 50.0, 170.0, 0.01);
        fx.touch(Phase::Move, 50.0, 150.0, 0.02);
        fx.touch(Phase::Move, 50.0, 150.0, 0.5);
        fx.touch(Phase::Up, 50.0, 150.0, 0.7);
        assert!(fx.input.fling.is_none(), "a finger at rest before the lift means stop");
        let mut fx = Fx::new(src);
        fx.touch(Phase::Down, 50.0, 190.0, 0.0);
        for i in 1..=5 {
            fx.touch(Phase::Move, 50.0, 190.0 - 40.0 * i as f32, 0.001 * i as f64);
        }
        fx.touch(Phase::Up, 50.0, 0.0, 0.006);
        let f = fx.input.fling.expect("a flick glides");
        assert!((f.v[1] - FLING_MAX_SPEED).abs() < 1.0, "capped: {:?}", f.v);
        // A slanted (mostly vertical) flick on a vertical list leaves no x
        // component; a SIDEWAYS one never scrolls it at all (the button
        // keeps the finger — see `drag_direction_decides_who_gets_the_finger`).
        let mut fx = Fx::new(src);
        fx.touch(Phase::Down, 50.0, 100.0, 0.0);
        fx.touch(Phase::Move, 60.0, 60.0, 0.01);
        fx.touch(Phase::Move, 70.0, 20.0, 0.02);
        fx.touch(Phase::Up, 70.0, 20.0, 0.03);
        let f = fx.input.fling.expect("glides");
        assert_eq!(f.v[0], 0.0);
        assert!(f.v[1] > 0.0);
    }

    /// GPAR7: the direction the finger commits to decides who owns the
    /// gesture. On a Slider in a scrolling page a sideways drag is the
    /// slider's (the page never moves); a downward one scrolls the page and
    /// puts the slider back where the finger found it. On a horizontal
    /// strip inside a vertical page, down scrolls the page, sideways the
    /// strip. A sideways drag on a Button in a vertical list scrolls
    /// nothing and the Button keeps its press.
    #[test]
    fn drag_direction_decides_who_gets_the_finger() {
        let src = "View v { Scroll page width=400, height=300, halign=left, valign=top { Column c width=fill { Slider s width=fill, height=40, min=0, max=100, value=50; Button b width=fill, height=40, text=\"x\", on:activate=tap; Rectangle tall width=fill, height=2000 } } }";
        let mut fx = Fx::new(src);
        let page = fx.page.find("page").unwrap();
        let s = fx.page.find("s").unwrap();
        let b = fx.page.find("b").unwrap();
        // Sideways on the slider.
        fx.touch(Phase::Down, 200.0, 20.0, 0.0);
        fx.touch(Phase::Move, 240.0, 22.0, 0.1);
        fx.touch(Phase::Move, 280.0, 24.0, 0.2);
        assert_eq!(fx.rt.scroll[page as usize], [0.0, 0.0], "a sideways drag never scrolls the page");
        let ev = fx.touch(Phase::Up, 280.0, 24.0, 0.3);
        let changed = ev.iter().find_map(|e| if let EventKind::Change(Value::VNum(v)) = e.kind { Some(v) } else { None }).unwrap_or_else(|| panic!("the slider changed: {ev:?}"));
        assert!(changed > 65.0 && changed < 75.0, "{changed}");
        // Down the slider: the page scrolls, the slider goes back to where the finger found it.
        fx.touch(Phase::Down, 100.0, 30.0, 1.0);
        {
            let (res, _) = fx.lay();
            let v = value_of(&fx.rt, &res, &fx.page, s);
            assert!(v > 20.0 && v < 30.0, "the down jumped the value: {v}");
        }
        fx.touch(Phase::Move, 102.0, -10.0, 1.1);
        fx.touch(Phase::Move, 104.0, -50.0, 1.2);
        assert!(fx.rt.scroll[page as usize][1] > 0.0, "the page took the drag");
        {
            let (res, _) = fx.lay();
            assert_eq!(value_of(&fx.rt, &res, &fx.page, s), changed, "the slider is back where the finger found it");
        }
        let ev = fx.touch(Phase::Up, 104.0, -50.0, 1.3);
        assert!(!ev.iter().any(|e| matches!(e.kind, EventKind::Change(_))), "no slider change from a scroll: {ev:?}");
        // Sideways on the button: nothing scrolls, the press survives to the tap.
        let mut fx = Fx::new(src);
        fx.touch(Phase::Down, 100.0, 60.0, 0.0);
        fx.touch(Phase::Move, 130.0, 62.0, 0.1);
        fx.touch(Phase::Move, 160.0, 64.0, 0.2);
        assert!(fx.rt.has(b, HELD), "the button keeps a sideways drag");
        assert_eq!(fx.rt.scroll[page as usize], [0.0, 0.0]);
        let ev = fx.touch(Phase::Up, 160.0, 64.0, 0.3);
        assert!(ev.iter().any(|e| e.kind == EventKind::Activate), "{ev:?}");
        // Nested: a horizontal strip inside the vertical page.
        let nested = "View v { Scroll page width=400, height=300, halign=left, valign=top { Column c width=fill { Scroll strip allow=horizontal, width=fill, height=60 { Row r { Rectangle wide width=1200, height=40 } }; Rectangle tall width=fill, height=2000 } } }";
        let mut fx = Fx::new(nested);
        let page = fx.page.find("page").unwrap();
        let strip = fx.page.find("strip").unwrap();
        fx.touch(Phase::Down, 100.0, 30.0, 0.0);
        fx.touch(Phase::Move, 102.0, -10.0, 0.1);
        fx.touch(Phase::Move, 104.0, -50.0, 0.2);
        assert!(fx.rt.scroll[page as usize][1] > 0.0, "up on the strip scrolls the page");
        assert_eq!(fx.rt.scroll[strip as usize], [0.0, 0.0]);
        let mut fx = Fx::new(nested);
        fx.touch(Phase::Down, 300.0, 30.0, 0.0);
        fx.touch(Phase::Move, 260.0, 32.0, 0.1);
        fx.touch(Phase::Move, 220.0, 34.0, 0.2);
        assert!(fx.rt.scroll[strip as usize][0] > 0.0, "sideways on the strip scrolls the strip");
        assert_eq!(fx.rt.scroll[page as usize], [0.0, 0.0], "and not the page");
    }

    /// GPAR59: on every toggling surface `change` lands before `activate`
    /// (Godot's BaseButton emits toggled before pressed), so an activate
    /// handler reads the committed value.
    #[test]
    fn change_lands_before_activate() {
        let src = "View v { Column c { Check k text=\"k\", on:activate=a, on:change=c; Button tb text=\"tb\", toggle=true, on:activate=a, on:change=c; Toggle t text=\"t\", on:activate=a, on:change=c; choice pick on:change=c { Radio r1 text=\"1\", on:activate=a; Radio r2 text=\"2\", on:activate=a } } }";
        let mut fx = Fx::new(src);
        let (_, rects) = fx.lay();
        for name in ["k", "tb", "t", "r2"] {
            let n = fx.page.find(name).unwrap();
            let r = rects[n as usize];
            fx.touch(Phase::Down, r.x + 5.0, r.y + r.h / 2.0, 0.0);
            let ev = fx.touch(Phase::Up, r.x + 5.0, r.y + r.h / 2.0, 0.1);
            let order: Vec<&str> = ev.iter().filter_map(|e| match e.kind { EventKind::Change(_) => Some("change"), EventKind::Activate => Some("activate"), _ => None }).collect();
            assert_eq!(order, vec!["change", "activate"], "{name}: {ev:?}");
        }
    }

    /// GPAR60: `reset` drops the gesture and the glide with no report;
    /// a Slider's declared and code-written values are quantized like a
    /// touch; a release counts inside by the pressed node's OWN rect.
    #[test]
    fn reset_quantized_writes_and_own_rect_release() {
        let src = "View v { Scroll page width=400, height=300, halign=left, valign=top { Column c width=fill { Button b width=fill, height=40, text=\"x\"; Rectangle tall width=fill, height=2000 } } }";
        let mut fx = Fx::new(src);
        fx.touch(Phase::Down, 100.0, 20.0, 0.0);
        for i in 1..=5 {
            fx.touch(Phase::Move, 100.0, 20.0 - 30.0 * i as f32, 0.01 * i as f64);
        }
        fx.touch(Phase::Up, 100.0, -130.0, 0.06);
        assert!(fx.input.fling.is_some(), "gliding");
        fx.input.reset(&mut fx.rt);
        assert!(fx.input.fling.is_none() && fx.input.press.is_none());
        let mut fx = Fx::new(src);
        let b = fx.page.find("b").unwrap();
        fx.touch(Phase::Down, 100.0, 20.0, 1.0);
        assert!(fx.rt.has(b, HELD));
        fx.input.reset(&mut fx.rt);
        assert!(!fx.rt.has(b, HELD), "no held ghost");
        assert!(fx.touch(Phase::Move, 100.0, 0.0, 1.1).is_empty(), "the gesture is gone");

        let mut fx = Fx::new("View v { Slider s width=200, height=32, halign=left, valign=top, min=0, max=100, step=5, value=33.3 }");
        let s = fx.page.find("s").unwrap();
        {
            let (res, _) = fx.lay();
            assert_eq!(value_of(&fx.rt, &res, &fx.page, s), 35.0, "the declared value sits on a step");
            assert_eq!(res.slider_value(&fx.page, s), 35.0, "and the knob draws there");
        }
        fx.rt.set_prop(s, PropKey::Value, Value::VNum(47.9));
        {
            let (res, _) = fx.lay();
            assert_eq!(value_of(&fx.rt, &res, &fx.page, s), 50.0, "a code write too");
            assert_eq!(res.slider_value(&fx.page, s), 50.0);
        }

        let mut fx = Fx::new("View v { Layers z { Button b width=100, height=40, halign=left, valign=top, on:activate=tap; Panel cover width=fill, height=fill, interactive=true, visibility=gone } }");
        let cover = fx.page.find("cover").unwrap();
        fx.touch(Phase::Down, 10.0, 10.0, 0.0);
        fx.rt.set_visible(&mut fx.page, cover, true);
        let ev = fx.touch(Phase::Up, 10.0, 10.0, 0.1);
        assert!(ev.iter().any(|e| e.kind == EventKind::Activate), "a plate opening under the finger is not a miss: {ev:?}");
    }

    /// GPAR13: a Slider brackets its drag with `press`/`release`, exactly
    /// one each, however the gesture ends.
    #[test]
    fn slider_press_and_release_bracket_the_drag() {
        let mut fx = Fx::new("View v { Slider s width=200, height=32, halign=left, valign=top, min=0, max=100, on:press=hold, on:release=let_go }");
        let ev = fx.touch(Phase::Down, 100.0, 16.0, 0.0);
        assert_eq!(ev[0].kind, EventKind::Press, "{ev:?}");
        assert!(matches!(ev[0].action, Some(Action::Named(id)) if fx.page.str(id) == "hold"));
        fx.touch(Phase::Move, 120.0, 16.0, 0.1);
        let ev = fx.touch(Phase::Up, 120.0, 16.0, 0.2);
        assert_eq!(ev.iter().filter(|e| e.kind == EventKind::Release).count(), 1, "{ev:?}");
        assert!(matches!(ev[0].action, Some(Action::Named(id)) if fx.page.str(id) == "let_go"));
        fx.touch(Phase::Down, 100.0, 16.0, 1.0);
        let ev = fx.touch(Phase::Cancel, 100.0, 16.0, 1.1);
        assert_eq!(ev.iter().filter(|e| e.kind == EventKind::Release).count(), 1, "a cancel releases too: {ev:?}");
    }

    /// GPAR51: a raw `on:pointer` owner hears `Cancel` when its node is
    /// disabled under the finger (else it keeps its grab until the next
    /// touch); a pan outlives the child it started on.
    #[test]
    fn raw_owner_hears_cancel_and_a_pan_outlives_its_child() {
        let mut fx = Fx::new("View v { Panel pad width=200, height=200, halign=left, valign=top, interactive=true, on:pointer=track }");
        let pad = fx.page.find("pad").unwrap();
        fx.touch(Phase::Down, 50.0, 50.0, 0.0);
        fx.rt.set_state(pad, DISABLED, true);
        let ev = fx.touch(Phase::Move, 60.0, 60.0, 0.1);
        assert!(ev.iter().any(|e| matches!(e.kind, EventKind::Pointer { phase: Phase::Cancel, .. })), "{ev:?}");
        assert!(fx.touch(Phase::Move, 70.0, 70.0, 0.2).is_empty(), "the gesture is over");
        let mut fx = Fx::new("View v { Scroll page width=400, height=300, halign=left, valign=top { Column c width=fill { Button b width=fill, height=40, text=\"x\"; Rectangle tall width=fill, height=2000 } } }");
        let page = fx.page.find("page").unwrap();
        let b = fx.page.find("b").unwrap();
        fx.touch(Phase::Down, 100.0, 20.0, 0.0);
        fx.touch(Phase::Move, 102.0, -20.0, 0.1);
        let y0 = fx.rt.scroll[page as usize][1];
        assert!(y0 > 0.0, "panning");
        fx.rt.set_visible(&mut fx.page, b, false);
        fx.touch(Phase::Move, 104.0, -60.0, 0.2);
        assert!(fx.rt.scroll[page as usize][1] > y0, "the pan continues after the child hid");
    }

    #[test]
    fn slider_laws_step_change_and_cancel() {
        let mut fx = Fx::new("View v { Column c { Slider s min=0, max=100, value=50, step=10, width=220, on:change=size_changed; Toggle t text=\"x\" } }");
        let s = fx.page.find("s").unwrap();
        let ev = fx.touch(Phase::Down, 219.0, 16.0, 0.0);
        assert!(matches!(ev[0].kind, EventKind::Edit(Value::VNum(v)) if v == 100.0), "{ev:?}");
        fx.touch(Phase::Move, 100.0, 16.0, 0.1);
        let v = value_of(&fx.rt, &fx.lay().0, &fx.page, s);
        assert_eq!(v % 10.0, 0.0, "quantized to step: {v}");
        fx.touch(Phase::Move, 10.0, 16.0, 0.15);
        let ev = fx.touch(Phase::Up, 10.0, 16.0, 0.2);
        assert!(matches!(ev[0].kind, EventKind::Change(Value::VNum(v)) if v == 0.0), "{ev:?}");
        assert!(matches!(ev[0].action, Some(Action::Named(_))));
        // A tap on the value already there reports nothing (GPAR6): no
        // `edit` on the down, no `change` on the lift.
        let ev = fx.touch(Phase::Down, 8.0, 16.0, 1.0);
        assert!(ev.iter().all(|e| !matches!(e.kind, EventKind::Edit(_))), "{ev:?}");
        assert!(fx.touch(Phase::Up, 8.0, 16.0, 1.1).is_empty(), "changed means changed");
        // A cancelled drag restores the pre-drag value.
        fx.touch(Phase::Down, 8.0, 16.0, 2.0);
        fx.touch(Phase::Move, 150.0, 16.0, 2.1);
        assert!(fx.touch(Phase::Cancel, 150.0, 16.0, 2.2).is_empty());
        assert_eq!(value_of(&fx.rt, &fx.lay().0, &fx.page, s), 0.0);
    }

    /// A Toggle tap goes to the other side and reports; a drag that ends on
    /// the side it started reports nothing (changed means changed).
    #[test]
    fn toggle_tap_drag_and_wiggle() {
        let mut fx = Fx::new("View v { Toggle t text=\"x\", halign=left, valign=top }");
        let t = fx.page.find("t").unwrap();
        let (res, rects) = fx.lay();
        let tr = rects[t as usize];
        let pad = res.placement[t as usize].padding[0];
        fx.touch(Phase::Down, tr.x + pad + 5.0, tr.y + 5.0, 1.0);
        let ev = fx.touch(Phase::Up, tr.x + pad + 5.0, tr.y + 5.0, 1.1);
        assert!(fx.rt.has(t, SELECTED));
        assert!(ev.iter().any(|e| e.kind == EventKind::Change(Value::VBool(true))));
        // Drag from the on side to the off side: reports once.
        fx.touch(Phase::Down, tr.x + pad + 40.0, tr.y + 5.0, 2.0);
        fx.touch(Phase::Move, tr.x + pad + 20.0, tr.y + 5.0, 2.1);
        fx.touch(Phase::Move, tr.x + pad + 2.0, tr.y + 5.0, 2.2);
        assert!(!fx.rt.has(t, SELECTED), "flips live mid-drag");
        let ev = fx.touch(Phase::Up, tr.x + pad + 2.0, tr.y + 5.0, 2.3);
        assert_eq!(ev.iter().filter(|e| matches!(e.kind, EventKind::Change(_))).count(), 1);
        // A wiggle across the middle and back: zero changes.
        fx.touch(Phase::Down, tr.x + pad + 2.0, tr.y + 5.0, 3.0);
        fx.touch(Phase::Move, tr.x + pad + 40.0, tr.y + 5.0, 3.1);
        assert!(fx.rt.has(t, SELECTED));
        fx.touch(Phase::Move, tr.x + pad + 2.0, tr.y + 5.0, 3.2);
        let ev = fx.touch(Phase::Up, tr.x + pad + 2.0, tr.y + 5.0, 3.3);
        assert!(!ev.iter().any(|e| matches!(e.kind, EventKind::Change(_))));
        assert!(!fx.rt.has(t, SELECTED));
    }

    /// `toggle=true` on a Button flips `selected` and reports `change`.
    #[test]
    fn toggle_button_flips_selected() {
        let mut fx = Fx::new("View v { Button b width=100, height=40, halign=left, valign=top, toggle=true, on:change=brush_toggled }");
        let b = fx.page.find("b").unwrap();
        fx.touch(Phase::Down, 10.0, 10.0, 0.0);
        let ev = fx.touch(Phase::Up, 10.0, 10.0, 0.1);
        assert!(fx.rt.has(b, SELECTED));
        assert!(ev.iter().any(|e| e.kind == EventKind::Change(Value::VBool(true))));
        fx.touch(Phase::Down, 10.0, 10.0, 1.0);
        let ev = fx.touch(Phase::Up, 10.0, 10.0, 1.1);
        assert!(!fx.rt.has(b, SELECTED));
        assert!(ev.iter().any(|e| e.kind == EventKind::Change(Value::VBool(false))));
    }

    /// A `choice` is one exclusive set through nested Rows, carries its
    /// `on:change`, and a re-tap of the pick reports nothing.
    #[test]
    fn choice_scope_handler_and_retap() {
        let mut fx = Fx::new("View v { Column c { choice mode value=a, on:change=emit(ModePicked) { Row r { Radio a width=40, height=40; Radio b width=40, height=40 }; Radio d width=40, height=40 } } }");
        let a = fx.page.find("a").unwrap();
        let b = fx.page.find("b").unwrap();
        let d = fx.page.find("d").unwrap();
        assert!(fx.rt.has(a, SELECTED));
        let (_, rects) = fx.lay();
        let rd = rects[d as usize];
        fx.touch(Phase::Down, rd.x + 5.0, rd.y + 5.0, 0.0);
        let ev = fx.touch(Phase::Up, rd.x + 5.0, rd.y + 5.0, 0.1);
        assert!(fx.rt.has(d, SELECTED) && !fx.rt.has(a, SELECTED), "the cousin in the Row cleared");
        let change = ev.iter().find(|e| matches!(e.kind, EventKind::Change(_))).expect("reports");
        assert!(matches!(change.action, Some(Action::Emit(id)) if fx.page.str(id) == "ModePicked"));
        assert!(matches!(change.kind, EventKind::Change(Value::VIdent(id)) if fx.page.str(id) == "d"));
        // Re-tap: still pressed, no report.
        fx.touch(Phase::Down, rd.x + 5.0, rd.y + 5.0, 1.0);
        let ev = fx.touch(Phase::Up, rd.x + 5.0, rd.y + 5.0, 1.1);
        assert!(fx.rt.has(d, SELECTED));
        assert!(!ev.iter().any(|e| matches!(e.kind, EventKind::Change(_))));
        let _ = b;
    }

    /// `disabled` cascades: a child under a disabled Panel takes no press,
    /// and disabling mid-drag ends the gesture.
    #[test]
    fn disabled_cascades_and_cancels_mid_drag() {
        let mut fx = Fx::new("View v { Panel band disabled=true, halign=left, valign=top { Button b width=100, height=40 } Slider s min=0, max=100, value=0, width=200, halign=left, valign=bottom }");
        let b = fx.page.find("b").unwrap();
        let band = fx.page.find("band").unwrap();
        let s = fx.page.find("s").unwrap();
        fx.touch(Phase::Down, 10.0, 10.0, 0.0);
        assert!(!fx.rt.has(b, HELD), "no press under a disabled ancestor");
        assert!(fx.touch(Phase::Up, 10.0, 10.0, 0.1).is_empty());
        fx.rt.set_state(band, DISABLED, false);
        fx.touch(Phase::Down, 10.0, 10.0, 1.0);
        assert!(fx.rt.has(b, HELD));
        fx.touch(Phase::Up, 10.0, 10.0, 1.1);
        let (_, rects) = fx.lay();
        let sr = rects[s as usize];
        fx.touch(Phase::Down, sr.x + 8.0, sr.y + 8.0, 2.0);
        fx.touch(Phase::Move, sr.x + 100.0, sr.y + 8.0, 2.1);
        fx.rt.set_state(s, DISABLED, true);
        fx.touch(Phase::Move, sr.x + 150.0, sr.y + 8.0, 2.2);
        assert!(!fx.input.is_pressed(), "disabling mid-drag ends the gesture");
        assert!(!fx.rt.has(s, HELD));
        assert_eq!(value_of(&fx.rt, &fx.lay().0, &fx.page, s), 0.0, "the pre-drag value is back");
    }

    /// One finger owns the gesture: a second finger's down/up is swallowed.
    #[test]
    fn second_finger_is_swallowed() {
        let mut fx = Fx::new("View v { Button b width=100, height=40, halign=left, valign=top; Button c width=100, height=40, halign=right, valign=top }");
        let b = fx.page.find("b").unwrap();
        fx.touch_id(Phase::Down, 10.0, 10.0, 0.0, 1);
        fx.touch_id(Phase::Down, 390.0, 10.0, 0.1, 2);
        assert!(fx.rt.has(b, HELD));
        assert!(fx.touch_id(Phase::Up, 390.0, 10.0, 0.2, 2).is_empty(), "the other finger's lift ends nothing");
        assert!(fx.rt.has(b, HELD));
        assert_eq!(fx.touch_id(Phase::Up, 10.0, 10.0, 0.3, 1).len(), 1);
    }

    /// `on:activate=toggle(x)` flips the arrangement with no host handler.
    #[test]
    fn go_and_toggle_actions_flip_the_arrangement() {
        let mut fx = Fx::new("arrangement open\nView v { Button b width=100, height=40, halign=left, valign=top, on:activate=toggle(open) }");
        fx.touch(Phase::Down, 10.0, 10.0, 0.0);
        fx.touch(Phase::Up, 10.0, 10.0, 0.1);
        assert_eq!(fx.rt.arrangement, 0);
        fx.touch(Phase::Down, 10.0, 10.0, 1.0);
        fx.touch(Phase::Up, 10.0, 10.0, 1.1);
        assert_eq!(fx.rt.arrangement, u32::MAX);
    }
}
