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
//! ancestor takes input. Tab and the arrows move keyboard focus between
//! reachable boxes (GP26): file order for Tab, nearest-in-direction for
//! arrows, unless the focused control claims the key — Enter/Space
//! activate, a Slider's arrows step by `step=`, a Toggle's pick its
//! sides — and a wheel tick steps the steppable control under the
//! pointer. A key grant scrolls its box into view. `hovered` answers a
//! REAL mouse only (D75 — the shell decides the source; touch never
//! hovers): the box under the cursor and EVERY ancestor light, and a
//! press holds the hovered look until release. Coordinates are epx.

use crate::draw::hit;
use crate::layout::{DEFAULT_FONT_SIZE, Rect, SLIDER_KNOB_EPX, TOGGLE_PILL, TextMeasure};
use crate::draw::TOGGLE_KNOB_EPX;
use crate::page::*;
// The baked row shares the trait's name: inside this file the ROW is
// `Behavior_` (the trait is the one a host implements and imports).
use crate::page::Behavior as Behavior_;
use crate::runtime::{FOCUS_WITHIN, FOCUSED, HELD, HOVERED, Resolved, Runtime, SELECTED};

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
    /// The finger (the platform's touch id; a mouse is one fixed id —
    /// `MOUSE_ID`).
    pub id: u64,
}

/// The mouse's `Touch.id`: touch ids are the platform's, the one mouse
/// rides this fixed id. Mouse-only behaviors key off it — a mouse drag
/// in a field SELECTS, a finger's drag pans the Scroll instead (D75
/// spirit: pointer kinds never blur into each other).
pub const MOUSE_ID: u64 = u64::MAX;

/// Modifier keys as they stood at the keystroke — the shell reads them
/// from the platform and passes them with every key.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
}

impl Mods {
    pub const NONE: Mods = Mods { shift: false, ctrl: false };
    pub const SHIFT: Mods = Mods { shift: true, ctrl: false };
    pub const CTRL: Mods = Mods { shift: false, ctrl: true };
}

/// Seconds two presses may sit apart and still read as a double click
/// (word select in a field).
const DOUBLE_CLICK_S: f64 = 0.35;

/// Seconds a still finger press in a field takes to mature into
/// word-select + the edit bar (the touch door to copy/paste — a phone
/// has no Ctrl).
const LONG_PRESS_S: f32 = 0.5;

/// Air between the edit bar and its field, epx.
pub const EDIT_BAR_GAP_EPX: f32 = 4.0;

/// Where the edit bar (the built-in `edit_bar.gogo` page) sits for a
/// field's rectangle: docked just above the field, below it when the
/// top edge would clip it, left edges aligned and clamped into the
/// view. ONE function so every host docks the bar the same way.
/// `bar_w`/`bar_h` = the laid-out bar's size.
///
/// Returns the bar's top-left in the same epx space as `field`.
pub fn edit_bar_anchor(field: Rect, bar_w: f32, bar_h: f32, view_w: f32) -> (f32, f32) {
    let y = if field.y - bar_h - EDIT_BAR_GAP_EPX >= 0.0 { field.y - bar_h - EDIT_BAR_GAP_EPX } else { field.y + field.h + EDIT_BAR_GAP_EPX };
    let x = field.x.min((view_w - bar_w).max(0.0)).max(0.0);
    (x, y)
}

/// A selection drag handle's drawn size (the circle's diameter), epx.
pub const SEL_HANDLE_EPX: f32 = 16.0;
/// Extra grab air around a handle's circle, epx each side.
const SEL_HANDLE_GRAB_EPX: f32 = 6.0;

/// One selection handle's rectangle: a circle hanging just below the
/// field, centered on the selection edge at `x`. `diam` is in the
/// caller's units (epx in the input machine, px in the renderer), so
/// BOTH consumers — the drawer and the grab hit-check — get the same
/// picture.
pub fn sel_handle_rect(field_bottom: f32, x: f32, diam: f32) -> Rect {
    Rect::new(x - diam / 2.0, field_bottom + diam / 8.0, diam, diam)
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
    /// A key a focused `control` instance claimed (GP26 `keys` line):
    /// the code-behind steps its model — traversal never saw the key.
    Key { key: Key, mods: Mods },
    /// A signal a control's behavior raised (GP52: `behavior slider`'s
    /// declared `drag_started`/`drag_ended`): the name rides the
    /// event's `Action::Emit`, like every `emit()`.
    Signal,
}

/// What happened, on which node, and the `on:` action the file names for
/// it (`None` = the node's own name is the handler, per auto-wiring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub node: NodeId,
    pub kind: EventKind,
    pub action: Option<Action>,
}

impl Event {
    /// A `Pointer` event's phase and position in `node`'s own epx space
    /// (`None` for any other event). Needed because a code-drawn control
    /// hears the pointer on its touch plate but thinks in a sibling's
    /// space — the gradient bar under its marks' plates, the color wheel
    /// under its slop target — and every one of them re-did the same
    /// subtraction over `rects` by hand.
    pub fn pointer_in(&self, rects: &[Rect], node: NodeId) -> Option<(Phase, f32, f32)> {
        let EventKind::Pointer { phase, x, y } = self.kind else { return None };
        let from = rects[self.node];
        let to = rects[node];
        Some((phase, from.x - to.x + x, from.y - to.y + y))
    }
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
    /// The `behavior` row this press belongs to (GP52/GP53) — an index
    /// into `Page::behaviors`; the press node is then the instance root.
    behavior: Option<u32>,
    /// The registered behavior answering it (`Registry::get`), found by
    /// name once, at the press.
    registered: u16,
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
    /// Focus traversal (GP26): file order, wrapping; `TabBack` is
    /// Shift-Tab.
    Tab,
    TabBack,
    /// Up/Down traverse by direction (Left/Right too, off a text
    /// field — an Input's caret claims those).
    Up,
    Down,
}

/// The field's text as shown: the last code write, else the file's word.
/// The caret index a press at `x` means: the nearer character boundary
/// of the field's text, measured through the same `TextMeasure` the
/// layout used (the drawn text starts 8 epx into the plate — draw.rs's
/// inset).
fn index_from_x(page: &Page, rt: &Runtime, res: &Resolved, rects: &[Rect], measure: &dyn TextMeasure, node: NodeId, x: f32) -> usize {
    let local = x - (rects[node].x + 8.0);
    let font_size = res.font_size(page, node).unwrap_or(DEFAULT_FONT_SIZE);
    let font_id = res.font(page, node).unwrap_or(crate::page::NO_STR);
    let font = if font_id == crate::page::NO_STR { None } else { Some(page.str(font_id)) };
    let chars: Vec<char> = input_text(page, rt, node).chars().collect();
    let mut prev = 0.0;
    for i in 0..chars.len() {
        let s: String = chars[..i + 1].iter().collect();
        let (w, _) = measure.measure(&s, font_size, None, font);
        if local < (prev + w) / 2.0 {
            return i;
        }
        prev = w;
    }
    chars.len()
}

/// The caret x for a char index — the inverse of `index_from_x`,
/// through the same shaping, so a drag handle sits exactly where the
/// selection edge draws.
fn x_from_index(page: &Page, rt: &Runtime, res: &Resolved, rects: &[Rect], measure: &dyn TextMeasure, node: NodeId, index: usize) -> f32 {
    let font_size = res.font_size(page, node).unwrap_or(DEFAULT_FONT_SIZE);
    let font_id = res.font(page, node).unwrap_or(crate::page::NO_STR);
    let font = if font_id == crate::page::NO_STR { None } else { Some(page.str(font_id)) };
    let chars: Vec<char> = input_text(page, rt, node).chars().collect();
    let s: String = chars[..index.min(chars.len())].iter().collect();
    let (w, _) = measure.measure(&s, font_size, None, font);
    rects[node].x + 8.0 + w
}

/// The word span at `i`: an alphanumeric/underscore run, else the run of
/// whatever else sits under the press (a double click on a gap selects
/// the gap — the everywhere behavior).
fn word_at(text: &[char], i: usize) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let i = i.min(text.len() - 1);
    let wordy = |c: char| c.is_alphanumeric() || c == '_';
    let class = wordy(text[i]);
    let mut lo = i;
    while lo > 0 && wordy(text[lo - 1]) == class {
        lo -= 1;
    }
    let mut hi = i + 1;
    while hi < text.len() && wordy(text[hi]) == class {
        hi += 1;
    }
    (lo, hi)
}

fn input_text(page: &Page, rt: &Runtime, node: NodeId) -> String {
    match text_value(page, rt, node) {
        Value::VText { text, .. } => page.str(text).to_string(),
        _ => String::new(),
    }
}

/// The `Value` the field's events carry.
fn text_value(page: &Page, rt: &Runtime, node: NodeId) -> Value {
    rt.writes.iter().rev().find(|(n, p)| *n == node && p.key == PropKey::Text).map(|(_, p)| p.value).or_else(|| page.prop(node, PropKey::Text)).unwrap_or(Value::VText { text: NO_STR, key: NO_KEY })
}

#[derive(Debug, Default)]
pub struct Input {
    /// The input behaviors (GP53): gogo's own, then the host's.
    registry: Registry,
    press: Option<Press>,
    fling: Option<Fling>,
    /// The offset this machine last wrote for `fling.scroll`: a different
    /// value means code moved the scroll — adopt it and stand down.
    expected: Option<(NodeId, [f32; 2])>,
    /// The keyboard focus owner — any reachable box (GP26); keystrokes
    /// edit it when it's an Input, and a tap off every reachable box is
    /// the focus-loss commit door (GPAR112).
    focus: Option<NodeId>,
    /// D129 visible focus: the ring (the FOCUSED bit) shows when focus
    /// arrived BY KEY — a tap parks focus without ring residue; caret
    /// controls (Input) always warrant one.
    focus_visible: bool,
    /// The box under the mouse (the hit-test top; its whole ancestor
    /// chain holds the HOVERED bit). None on touch devices — only the
    /// shell's mouse motion sets it.
    hover: Option<NodeId>,
    /// The caret, in CHARS into the field's text.
    caret: usize,
    /// The selection's FIXED end, in chars — `None` = nothing selected.
    /// The caret is always the moving end (Shift+arrows, a mouse drag),
    /// so extending and shrinking need no cases: one anchor, one mover.
    anchor: Option<usize>,
    /// The last press that landed in a field (time, node) — a second
    /// press there within `DOUBLE_CLICK_S` selects the word under it.
    last_field_press: Option<(f64, NodeId)>,
    /// Seconds the current field press has held still — at LONG_PRESS_S
    /// it matures (word-select + the edit bar). Reset on Down.
    hold: f32,
    /// Where the current field press landed (chars) — the long-press
    /// selects the word THERE, not wherever the finger drifted.
    press_index: usize,
    /// The grabbed selection handle mid-drag (`true` = the HI end);
    /// `None` between grabs.
    drag_handle: Option<bool>,
    /// The live selection was born of touch (long-press, double-tap,
    /// the bar's All) — published as `Runtime::sel_handles`.
    touch_sel: bool,
    /// The text at focus (or the last commit): `change` fires once per
    /// genuine transition — the shipped changed-means-CHANGED law.
    committed: String,
}

fn event_action(page: &Page, node: NodeId, event: &str) -> Option<Action> {
    let n = &page.nodes[node];
    let id = page.intern_lookup(event);
    page.handlers[n.handlers.0 as usize..(n.handlers.0 + n.handlers.1) as usize].iter().find(|h| h.event == id).map(|h| h.action)
}

/// Keyboard-reachable RIGHT NOW: the compile-baked `focusable` flag
/// minus the runtime facts — disabled (an ancestor's counts) and hidden
/// (own or an ancestor's — the tap path's `gone`/`hidden` pair, so a
/// repeat's template and unshown rows and a presence mark are skipped
/// exactly like a `visibility=` word).
fn reachable(page: &Page, rt: &Runtime, res: &Resolved, node: NodeId) -> bool {
    if !page.nodes[node].focusable || rt.disabled(page, node) {
        return false;
    }
    let mut n = node;
    loop {
        if res.gone(page, n) || res.hidden(page, n) {
            return false;
        }
        let p = page.nodes[n].parent;
        if p == NO_NODE {
            return true;
        }
        n = p;
    }
}

/// Writes a state bit on `node` and every ancestor — the looking-up
/// states: `focus_within` (D129) and `hovered` (the chain reading, user
/// call 2026-09-02 — the game staple: a settings row lights while its
/// inner slider is key-focused or under the mouse).
fn set_chain(page: &Page, rt: &mut Runtime, node: NodeId, bit: u8, on: bool) {
    let mut n = node;
    loop {
        rt.set_state(n, bit, on);
        let p = page.nodes[n].parent;
        if p == NO_NODE {
            return;
        }
        n = p;
    }
}

fn scroll_ancestor(page: &Page, node: NodeId) -> Option<NodeId> {
    let mut n = page.nodes[node].parent;
    while n != NO_NODE {
        if matches!(page.nodes[n].kind, Kind::Scroll | Kind::List) {
            return Some(n);
        }
        n = page.nodes[n].parent;
    }
    None
}

/// The List a row belongs to — the selection owner a tap reports to.
fn list_ancestor(page: &Page, node: NodeId) -> Option<NodeId> {
    let mut n = page.nodes[node].parent;
    while n != NO_NODE {
        if page.nodes[n].kind == Kind::List {
            return Some(n);
        }
        n = page.nodes[n].parent;
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
        n = page.nodes[n].parent;
    }
    false
}

/// The innermost Scroll around `node` (`node` itself counts) that scrolls
/// the way the finger committed to AND has somewhere to go on that axis
/// (an enabled axis with nothing overflowing can't take a drag — GP36:
/// axis inference enables every non-hug axis, so a fitting axis must not
/// steal the press); `None` = nothing here scrolls that way.
fn scroll_taker(page: &Page, res: &Resolved, rects: &[Rect], node: NodeId, vertical: bool) -> Option<NodeId> {
    let mut n = node;
    while n != NO_NODE {
        if matches!(page.nodes[n].kind, Kind::Scroll | Kind::List) {
            let axes = scroll_axes(page, res, n);
            let range = scroll_range(page, res, rects, n);
            if (if vertical { axes.1 } else { axes.0 }) && (if vertical { range[1] } else { range[0] }) > 0.0 {
                return Some(n);
            }
        }
        n = page.nodes[n].parent;
    }
    None
}

/// How far a Scroll's content can move: (max_x, max_y) in epx. The
/// content is the UNION of the children's rects — a Scroll's one content
/// box, or a List's whole stacked body (a `repeat` stands for its rows;
/// statics move with them, one doctrine both ways).
fn scroll_range(page: &Page, res: &Resolved, rects: &[Rect], scroll: NodeId) -> [f32; 2] {
    let viewport = rects[scroll];
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    let mut any = false;
    let mut kids = res.children(page, scroll);
    let mut i = 0;
    while i < kids.len() {
        let c = kids[i];
        i += 1;
        if page.nodes[c].kind == Kind::Repeat {
            kids.extend(res.children(page, c));
            continue;
        }
        let r = rects[c];
        if r.w <= 0.0 && r.h <= 0.0 {
            continue;
        }
        any = true;
        lo[0] = lo[0].min(r.x);
        lo[1] = lo[1].min(r.y);
        hi[0] = hi[0].max(r.right());
        hi[1] = hi[1].max(r.bottom());
    }
    if !any {
        return [0.0, 0.0];
    }
    [(hi[0] - lo[0] - viewport.w).max(0.0), (hi[1] - lo[1] - viewport.h).max(0.0)]
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
    let viewport = rects[scroll];
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
    if rt.scroll[scroll] != shown {
        rt.scroll[scroll] = shown;
        rt.dirty = true;
    }
    raw
}

fn slider_value(page: &Page, res: &Resolved, rects: &[Rect], node: NodeId, x: f32) -> f32 {
    let min = res.num(page, node, PropKey::Min).unwrap_or(0.0);
    let max = res.num(page, node, PropKey::Max).unwrap_or(100.0);
    let r = rects[node];
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

// ------------------------------------------------------------ input behaviors

/// A control's input behavior — the Godot addon's behavior script
/// (`GogoControlScript`) as a Rust trait (GP53). A `.gogo` control names
/// one (`behavior toggle by $value in $min..$max`); the runtime routes
/// the instance's finger and keys to it and takes back one value write
/// per call through `Cx`.
///
/// Needed so input stays the gogo USER's job: a knob, a range slider or
/// a switch is a few lines of pointer math in the host's crate, never
/// an edit to gogo. The runtime owns the law AROUND the behavior, so a
/// behavior stays small: the gesture bracket (`held`, focus, a second
/// finger swallowed), `edit` per changed write and `change` on a changed
/// release, the restore on a cancelled or reclaimed gesture, the
/// declared `drag_started`/`drag_ended` edges, `selected` derived from
/// the value when the contract declares it, and the `live($value)`
/// label follow. A behavior never sees nodes: geometry arrives as
/// control-local rects (`Cx::slot`), and its only output is
/// `Cx::set_value`/`Cx::set_fraction`.
///
/// One gesture is live at a time (a second finger is swallowed), so a
/// behavior keeps its gesture scratch in `self` — a toggle's press x, a
/// wobble flag. gogo's own `slider` and `toggle` implement this trait;
/// a host adds its own with `Input::register`, and names it to the
/// compiler through `CompileOptions::behaviors` (the spec is what the
/// `.gogo` file is checked against: the slots read, the keys owned).
pub trait Behavior {
    /// The spec the compiler checked the `behavior` line against: the
    /// slots `Cx::slot` hands over, in order, and the key groups `key`
    /// answers.
    fn spec(&self) -> &'static BehaviorSpec;

    /// Finger down at `(x, y)`, control-local epx. `Claim::Take` owns
    /// the gesture (moves and the release come here); `Claim::Pass`
    /// lets the press go on as if there were no behavior (a Scroll above
    /// may take it). The default passes.
    fn began(&mut self, cx: &mut Cx, x: f32, y: f32) -> Claim {
        let _ = (cx, x, y);
        Claim::Pass
    }

    /// The finger moved to `(x, y)`; a write here streams `edit`.
    fn moved(&mut self, cx: &mut Cx, x: f32, y: f32) {
        let _ = (cx, x, y);
    }

    /// The finger lifted at `(x, y)` — the settle write goes here; the
    /// runtime then commits `change` when the value differs from the
    /// one the finger found.
    fn ended(&mut self, cx: &mut Cx, x: f32, y: f32) {
        let _ = (cx, x, y);
    }

    /// The gesture was cancelled or reclaimed. The runtime restores the
    /// grabbed value itself; this is the behavior's cue to drop its
    /// scratch.
    fn canceled(&mut self) {}

    /// A key or wheel action in a group the spec owns. `true` = handled
    /// (a changed write commits `change` directly, no `edit` — a step
    /// and a programmatic write are one thing at the seam); `false`
    /// lets the key go on to traversal. The default refuses.
    fn key(&mut self, cx: &mut Cx, action: KeyAction) -> bool {
        let _ = (cx, action);
        false
    }
}

/// `Behavior::began`'s answer: take the gesture, or let the press go on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    Take,
    Pass,
}

/// The logical key actions (the addon's `_key_to_action`) — platform
/// keys stay out of behaviors. Right/Up = `Increment`, Left/Down =
/// `Decrement`, Home/End = `ToMin`/`ToMax`, Enter/Space = `Activate`; a
/// wheel tick is `Increment`/`Decrement` on the instance under the
/// pointer, focus or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Activate,
    Increment,
    Decrement,
    ToMin,
    ToMax,
}

/// What a behavior sees and may write during one call (GP53): the value
/// triad as numbers, the control's box and slots as control-local rects
/// (the control's own top-left is (0, 0)), and the one write back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cx {
    value: f32,
    pub min: f32,
    pub max: f32,
    /// `step=`: 0 = continuous.
    pub step: f32,
    /// `key_unit=`: the key/wheel unit of a CONTINUOUS slider, in value
    /// units; 0 = unset.
    pub key_unit: f32,
    /// The control's own box.
    pub width: f32,
    pub height: f32,
    slots: [Rect; BEHAVIOR_SLOTS],
    out: Option<f32>,
}

impl Cx {
    fn new(rects: &[Rect], b: &Behavior_, value: f32) -> Cx {
        let r = rects[b.root];
        let mut slots = [Rect::ZERO; BEHAVIOR_SLOTS];
        for (i, s) in b.slots.iter().enumerate() {
            if *s != NO_NODE {
                let sr = rects[*s];
                slots[i] = Rect::new(sr.x - r.x, sr.y - r.y, sr.w, sr.h);
            }
        }
        Cx { value, min: b.min, max: b.max, step: b.step, key_unit: b.key_unit, width: r.w, height: r.h, slots, out: None }
    }

    /// The value now — a write earlier in the same call shows.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// `(value - min) / (max - min)`, 0..1.
    pub fn fraction(&self) -> f32 {
        if self.max > self.min { ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// The `i`th slot of the behavior's spec, control-local (`slot(0)`
    /// is `track` for both stock behaviors).
    pub fn slot(&self, i: usize) -> Rect {
        self.slots[i]
    }

    /// `v` quantized to `step` from `min` (when stepped) and clamped
    /// to the range — Range's rule, the one law every write follows.
    pub fn snap(&self, v: f32) -> f32 {
        let v = if self.step > 0.0 { self.min + ((v - self.min) / self.step).round() * self.step } else { v };
        v.clamp(self.min.min(self.max), self.min.max(self.max))
    }

    /// The one write: `v` lands snapped; the runtime turns it into the
    /// prop write and the `edit`/`change` it owes.
    pub fn set_value(&mut self, v: f32) {
        let v = self.snap(v);
        self.value = v;
        self.out = Some(v);
    }

    /// `set_value` from a 0..1 fraction — the usual last line of
    /// pointer math (`(x - track.x) / track.w`).
    pub fn set_fraction(&mut self, f: f32) {
        self.set_value(self.min + f.clamp(0.0, 1.0) * (self.max - self.min));
    }

    /// The stock key unit: `step=` when stepped, else `key_unit=`,
    /// else 1.0 (Range's default).
    pub fn key_step(&self) -> f32 {
        if self.step > 0.0 {
            self.step
        } else if self.key_unit > 0.0 {
            self.key_unit
        } else {
            1.0
        }
    }
}

/// The slot indexes the stock behaviors read (their specs' order).
const TRACK: usize = 0;
const THUMB: usize = 1;

/// `behavior slider` (GP52, the addon's slider_behavior.gd +
/// ui/slider_phone_behavior.gd): a press jumps the value under the
/// finger and drags follow it over bare x / track width — shipped
/// SliderPhone drew no thumb, so no knob compensation (its UpdatePercent
/// was `x / width` verbatim); arrows/Home/End/wheel step by the key
/// unit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SliderBehavior;

impl SliderBehavior {
    fn apply(cx: &mut Cx, x: f32) {
        let track = cx.slot(TRACK);
        if track.w > 0.0 {
            cx.set_fraction((x - track.x) / track.w);
        }
    }
}

impl Behavior for SliderBehavior {
    fn spec(&self) -> &'static BehaviorSpec {
        &SLIDER_SPEC
    }

    fn began(&mut self, cx: &mut Cx, x: f32, _y: f32) -> Claim {
        Self::apply(cx, x);
        Claim::Take
    }

    fn moved(&mut self, cx: &mut Cx, x: f32, _y: f32) {
        Self::apply(cx, x);
    }

    fn key(&mut self, cx: &mut Cx, action: KeyAction) -> bool {
        match action {
            KeyAction::Increment => cx.set_value(cx.value() + cx.key_step()),
            KeyAction::Decrement => cx.set_value(cx.value() - cx.key_step()),
            KeyAction::ToMin => cx.set_value(cx.min),
            KeyAction::ToMax => cx.set_value(cx.max),
            KeyAction::Activate => return false,
        }
        true
    }
}

/// `behavior toggle` (the addon's toggle_behavior.gd, the effect pages'
/// paint-target switch): a TAP glides the value to the other end; a
/// DRAG past the wobble puts the thumb under the finger (thumb-width
/// compensated over the track) and settles to the nearer end on
/// release. Enter/Space flip, Right/Up/End = on, Left/Down/Home = off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ToggleBehavior {
    press_x: f32,
    moved: bool,
}

impl ToggleBehavior {
    fn apply(cx: &mut Cx, x: f32) {
        let track = cx.slot(TRACK);
        let thumb_w = cx.slot(THUMB).w;
        let span = track.w - thumb_w;
        if span > 0.0 {
            cx.set_fraction((x - track.x - thumb_w / 2.0) / span);
        }
    }

    fn flip(cx: &mut Cx) {
        cx.set_value(if cx.fraction() >= 0.5 { cx.min } else { cx.max });
    }
}

impl Behavior for ToggleBehavior {
    fn spec(&self) -> &'static BehaviorSpec {
        &TOGGLE_SPEC
    }

    fn began(&mut self, _cx: &mut Cx, x: f32, _y: f32) -> Claim {
        self.press_x = x;
        self.moved = false;
        Claim::Take
    }

    fn moved(&mut self, cx: &mut Cx, x: f32, _y: f32) {
        if !self.moved && (x - self.press_x).abs() < TOGGLE_DRAG_THRESHOLD {
            return;
        }
        self.moved = true;
        Self::apply(cx, x);
    }

    fn ended(&mut self, cx: &mut Cx, _x: f32, _y: f32) {
        if self.moved {
            // A drag settles to the side the thumb is on.
            cx.set_value(if cx.fraction() >= 0.5 { cx.max } else { cx.min });
        } else {
            Self::flip(cx);
        }
        self.moved = false;
    }

    fn canceled(&mut self) {
        self.moved = false;
    }

    fn key(&mut self, cx: &mut Cx, action: KeyAction) -> bool {
        match action {
            KeyAction::Activate => Self::flip(cx),
            KeyAction::Increment | KeyAction::ToMax => cx.set_value(cx.max),
            KeyAction::Decrement | KeyAction::ToMin => cx.set_value(cx.min),
        }
        true
    }
}

/// The behaviors an `Input` routes to: gogo's own two inline (no
/// allocation for the stock pages), then the host's in `Input::register`
/// order. Found by name once per gesture (`Behavior::behavior`).
#[derive(Default)]
struct Registry {
    slider: SliderBehavior,
    toggle: ToggleBehavior,
    host: Vec<Box<dyn Behavior>>,
}

impl Registry {
    fn find(&self, name: &str) -> Option<u16> {
        match name {
            "slider" => Some(0),
            "toggle" => Some(1),
            _ => self.host.iter().position(|m| m.spec().name == name).map(|i| i as u16 + 2),
        }
    }

    fn get(&mut self, i: u16) -> &mut dyn Behavior {
        match i {
            0 => &mut self.slider,
            1 => &mut self.toggle,
            _ => self.host[i as usize - 2].as_mut(),
        }
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = BUILTIN_BEHAVIORS.iter().map(|s| s.name).chain(self.host.iter().map(|m| m.spec().name)).collect();
        f.debug_list().entries(names).finish()
    }
}

/// The key action `k` means to a behavior owning the `KEYS_*` groups in
/// `owned` — `None` when the group is not the behavior's (the key goes
/// on to a `keys` claim or traversal).
fn key_action(k: Key, owned: u8) -> Option<KeyAction> {
    match k {
        Key::Right | Key::Up if owned & KEYS_ARROWS != 0 => Some(KeyAction::Increment),
        Key::Left | Key::Down if owned & KEYS_ARROWS != 0 => Some(KeyAction::Decrement),
        Key::Home if owned & KEYS_HOME_END != 0 => Some(KeyAction::ToMin),
        Key::End if owned & KEYS_HOME_END != 0 => Some(KeyAction::ToMax),
        Key::Enter if owned & KEYS_ENTER != 0 => Some(KeyAction::Activate),
        Key::Char(' ') if owned & KEYS_SPACE != 0 => Some(KeyAction::Activate),
        _ => None,
    }
}

/// What a behavior's `change` carries (`Behavior::payload`).
fn change_value(b: &Behavior_, v: f32) -> Value {
    match b.payload {
        Payload::Number => Value::VNum(v),
        Payload::Bool => Value::VBool(v >= (b.min + b.max) / 2.0),
    }
}

/// "changed means CHANGED" for a behavior's commit: the number moved, or
/// (a bool payload) the side did.
fn changed(b: &Behavior_, from: f32, now: f32) -> bool {
    match b.payload {
        Payload::Number => now != from,
        Payload::Bool => (now >= (b.min + b.max) / 2.0) != (from >= (b.min + b.max) / 2.0),
    }
}

/// The `behavior` row whose instance holds `node` (GP52) — an index
/// into `Page::behaviors`, by subtree range (the claim rides the
/// instance root; every descendant is the behavior's surface).
fn behavior_at(page: &Page, node: NodeId) -> Option<u32> {
    // By ancestry, not by index range: a later sibling's subtree shares
    // the control root's `id+1..subtree_end` range (Node::subtree_end).
    page.behaviors.iter().position(|b| page.is_under(node, b.root)).map(|i| i as u32)
}

/// The behavior with the control's LIVE range: a code write of `min=` /
/// `max=` / `step=` on the control root (SliderPhone's Minimum/Maximum
/// setters — an effect row takes its filter's range at every open)
/// overrides the contract's baked numbers, as the addon's slider_behavior
/// read the live params. Needed because the rows are pooled slots: one
/// declared control serves every filter's range.
fn live_behavior(page: &Page, res: &Resolved, bi: u32) -> Behavior_ {
    let mut b = page.behaviors[bi as usize];
    if let Some(v) = res.num(page, b.root, PropKey::Min) {
        b.min = v;
    }
    if let Some(v) = res.num(page, b.root, PropKey::Max) {
        b.max = v;
    }
    if let Some(v) = res.num(page, b.root, PropKey::Step) {
        b.step = v;
    }
    b
}

/// `step=` on a behavior row: quantized from `min`, then clamped
/// (Range's rule, `snap_slider`'s law over the contract's baked
/// numbers).
fn b_snap(b: &Behavior_, v: f32) -> f32 {
    let v = if b.step > 0.0 { b.min + ((v - b.min) / b.step).round() * b.step } else { v };
    v.clamp(b.min.min(b.max), b.min.max(b.max))
}

/// A behavior slider's value as shown: a this-frame write, the resolved
/// prop, or the baked build-time value before any write lands.
fn b_value_of(rt: &Runtime, res: &Resolved, page: &Page, b: &Behavior_) -> f32 {
    let raw = rt.writes.iter().find(|(n, w)| *n == b.root && w.key == PropKey::Value).and_then(|(_, w)| if let Value::VNum(v) = w.value { Some(v) } else { None }).or_else(|| res.num(page, b.root, PropKey::Value)).unwrap_or(b.rest);
    b_snap(b, raw)
}

/// A Toggle's current side, a this-frame write included (the tap
/// path's derivation): the value when one was ever written, else the
/// `selected` bit.
fn toggle_on(rt: &Runtime, res: &Resolved, page: &Page, node: NodeId) -> bool {
    if rt.writes.iter().any(|(n, w)| *n == node && w.key == PropKey::Value) {
        value_of(rt, res, page, node) >= 0.5
    } else {
        rt.has(node, SELECTED)
    }
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
    let r = rects[node];
    let pad = res.placement[node].padding[0];
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

    /// Adds a host behavior (GP53): a `behavior <name>` line whose name
    /// the behavior's spec carries routes to it. The compiler must know
    /// the same spec (`CompileOptions::behaviors`) — a page naming a
    /// behavior nobody registered is inert (and a debug assert).
    pub fn register(&mut self, behavior: Box<dyn Behavior>) {
        self.registry.host.push(behavior);
    }

    /// One behavior call's write, landed: the value prop (the
    /// `live($value)` labels follow at the tick), `selected` derived
    /// when the contract declares it, and an `edit` when asked and the
    /// value moved. Returns the value shown now.
    fn land_behavior_write(&self, page: &Page, rt: &mut Runtime, events: &mut Vec<Event>, b: &Behavior_, shown: f32, cx: &Cx, edit: bool) -> f32 {
        let Some(v) = cx.out else { return shown };
        if v == shown {
            return shown;
        }
        rt.set_value_deferred(b.root, v);
        if b.selected {
            rt.set_state(b.root, SELECTED, v >= (b.min + b.max) / 2.0);
        }
        if edit {
            self.push_event(rt, events, b.root, EventKind::Edit(Value::VNum(v)), event_action(page, b.root, "edit"));
        }
        v
    }

    /// A key or wheel action on a `behavior` instance: the behavior
    /// answers (`true`), a changed write commits `change` directly —
    /// no `edit` (a step and a programmatic write are one thing at the
    /// seam); one landing on the current value says nothing.
    fn run_key_action(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], b: &Behavior_, mi: u16, action: KeyAction, events: &mut Vec<Event>) -> bool {
        let was = b_value_of(rt, res, page, b);
        let mut cx = Cx::new(rects, b, was);
        if !self.registry.get(mi).key(&mut cx, action) {
            return false;
        }
        let v = self.land_behavior_write(page, rt, events, b, was, &cx, false);
        if changed(b, was, v) {
            self.push_event(rt, events, b.root, EventKind::Change(change_value(b, v)), event_action(page, b.root, "change"));
        }
        true
    }

    /// Is a Scroll gliding or springing back?
    pub fn is_gliding(&self) -> bool {
        self.fling.is_some()
    }

    fn push_event(&self, rt: &mut Runtime, events: &mut Vec<Event>, node: NodeId, kind: EventKind, action: Option<Action>) {
        // `go(x)` / `toggle(x)` flip the view's arrangement right here — a
        // declarative wiring, no host handler.
        match action {
            Some(Action::Go(a)) => rt.go_id(a),
            Some(Action::ToggleArrangement(a)) => rt.toggle_id(a),
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
    fn cancel_press(&mut self, page: &Page, rt: &mut Runtime, events: &mut Vec<Event>, p: &Press) {
        rt.set_state(p.node, HELD, false);
        // GP52/GP53: a reclaimed behavior drag restores the grabbed value
        // (the labels follow at the next tick; never leave a half-drag
        // behind) — and the gesture still gets its closing edge: a host
        // arming UI on `drag_started` must always hear `drag_ended`.
        if let Some(bi) = p.behavior {
            self.registry.get(p.registered).canceled();
            let b = page.behaviors[bi as usize];
            rt.set_value_deferred(b.root, p.value_from);
            if b.selected {
                rt.set_state(b.root, SELECTED, p.value_from >= (b.min + b.max) / 2.0);
            }
            if b.drag_ended != NO_STR {
                self.push_event(rt, events, b.root, EventKind::Signal, Some(Action::Emit(b.drag_ended)));
            }
        }
        match p.kind {
            Kind::Slider => rt.set_prop(p.node, PropKey::Value, Value::VNum(p.value_from)),
            Kind::Toggle => {
                rt.set_prop(p.node, PropKey::Value, Value::VNum(p.value_from));
                rt.set_state(p.node, SELECTED, p.value_from >= 0.5);
            }
            _ => {}
        }
    }

    /// Forgets the gesture in flight and any glide: for the host to call
    /// when the page leaves the screen, so a list closed mid-fling is
    /// where the user left it on return and no `held` ghost survives a
    /// page swap. Nothing is reported (the finger is not on this page).
    pub fn reset(&mut self, page: &Page, rt: &mut Runtime) {
        if let Some(p) = self.press.take() {
            rt.set_state(p.node, HELD, false);
        }
        self.fling = None;
        self.expected = None;
        // Focus is forgotten WITHOUT a commit: the page left the screen,
        // reporting now would land on nobody.
        if let Some(f) = self.focus.take() {
            rt.set_state(f, FOCUSED, false);
            set_chain(page, rt, f, FOCUS_WITHIN, false);
        }
        if let Some(h) = self.hover.take() {
            set_chain(page, rt, h, HOVERED, false);
        }
        self.focus_visible = false;
        rt.caret = None;
        rt.selection = None;
        rt.sel_handles = false;
        rt.edit_bar = None;
        self.anchor = None;
        self.caret = 0;
        self.committed.clear();
    }

    /// One keyboard event (GPAR112 + GP26). Tab/Shift-Tab walk the
    /// reachable boxes in FILE order, wrapping; an arrow moves to the
    /// nearest reachable box in its direction — unless the focused
    /// control claims the key (an Input's caret claims Left/Right;
    /// control value steps come with the §5.5 round). To a focused
    /// Input, characters edit at the caret and stream `edit`, Enter
    /// commits `change` once per genuine change (the shipped
    /// changed-means-CHANGED law), Home/End move the caret.
    pub fn key(&mut self, page: &mut Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], k: Key, mods: Mods) -> Vec<Event> {
        let mut events = Vec::new();
        // D129 modality edge: ANY key press means keyboard driving — a
        // parked (ringless) owner gets its ring and its chain lights, in
        // place, even when the key itself does nothing.
        if let Some(f) = self.focus {
            if !self.focus_visible {
                self.focus_visible = true;
                rt.set_state(f, FOCUSED, true);
                set_chain(page, rt, f, FOCUS_WITHIN, true);
                rt.dirty = true;
            }
        }
        let focus_kind = self.focus.map(|f| page.nodes[f].kind);
        let in_field = focus_kind == Some(Kind::Input);
        // GP52/GP53: a focused `behavior` instance's behavior answers the
        // key groups its spec owns (the compile fence keeps a `keys`
        // claim off them, so no second owner); a refused action goes on
        // to traversal.
        if let Some(f) = self.focus {
            if !rt.disabled(page, f) {
                if let Some(bi) = page.behaviors.iter().position(|b| b.root == f) {
                    let b = live_behavior(page, res, bi as u32);
                    if let Some(mi) = self.registry.find(page.str(b.name)) {
                        let owned = self.registry.get(mi).spec().keys;
                        if let Some(action) = key_action(k, owned) {
                            if self.run_key_action(page, rt, res, rects, &b, mi, action, &mut events) {
                                return events;
                            }
                        }
                    }
                }
            }
        }
        // GP26: a `control` claims key GROUPS by contract (`keys arrows,
        // home_end` — the built-ins' static-claim law, §5.5, extended to
        // user controls): a claimed key reaches the code-behind as a
        // `Key` event on the instance root and never traverses. Tab has
        // no claim bit — always the way off. The claim rides the
        // INSTANCE ROOT id (an instance wears its body root's kind, so
        // there is no kind to test).
        if let Some(f) = self.focus {
            if !rt.disabled(page, f) {
                let mask = page.key_claim(f);
                let claimed = match k {
                    Key::Left | Key::Right | Key::Up | Key::Down => mask & crate::page::KEYS_ARROWS != 0,
                    Key::Home | Key::End => mask & crate::page::KEYS_HOME_END != 0,
                    Key::Enter => mask & crate::page::KEYS_ENTER != 0,
                    Key::Char(' ') => mask & crate::page::KEYS_SPACE != 0,
                    _ => false,
                };
                if claimed {
                    self.push_event(rt, &mut events, f, EventKind::Key { key: k, mods }, event_action(page, f, "key"));
                    return events;
                }
            }
        }
        match k {
            Key::Tab | Key::TabBack => {
                let target = self.tab_target(page, rt, res, k == Key::TabBack);
                self.grant(page, rt, res, rects, target.or(self.focus), true, &mut events);
                return events;
            }
            // Section 5.5: the focused control claims its actions BEFORE
            // traversal — a claimed arrow never moves focus (Tab is the
            // way off a Slider or Toggle, matching the shipped router).
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End if focus_kind == Some(Kind::Slider) => {
                let f = self.focus.unwrap();
                if !rt.disabled(page, f) {
                    self.step_slider(page, rt, res, f, k, &mut events);
                }
                return events;
            }
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End if focus_kind == Some(Kind::Toggle) => {
                let f = self.focus.unwrap();
                if !rt.disabled(page, f) {
                    self.set_toggle(page, rt, res, f, matches!(k, Key::Right | Key::Up | Key::End), &mut events);
                }
                return events;
            }
            Key::Enter | Key::Char(' ') if matches!(focus_kind, Some(kk) if kk.is_press_surface()) => {
                let f = self.focus.unwrap();
                if !rt.disabled(page, f) {
                    self.activate_focused(page, rt, res, f, &mut events);
                }
                return events;
            }
            Key::Up | Key::Down | Key::Left | Key::Right if !(in_field && matches!(k, Key::Left | Key::Right)) => {
                if self.focus.is_none() {
                    return events;
                }
                let (dx, dy) = match k {
                    Key::Left => (-1.0, 0.0),
                    Key::Right => (1.0, 0.0),
                    Key::Up => (0.0, -1.0),
                    _ => (0.0, 1.0),
                };
                // No box that way keeps focus put — the ring still
                // upgrades in place (D129: a key press shows focus).
                let target = self.arrow_target(page, rt, res, rects, dx, dy).or(self.focus);
                self.grant(page, rt, res, rects, target, true, &mut events);
                return events;
            }
            _ => {}
        }
        let Some(f) = self.focus.filter(|_| in_field) else { return events };
        let mut text: Vec<char> = input_text(page, rt, f).chars().collect();
        self.caret = self.caret.min(text.len());
        self.anchor = self.anchor.map(|a| a.min(text.len())).filter(|a| *a != self.caret);
        let sel = self.selection();
        match k {
            Key::Char('a' | 'A') if mods.ctrl => {
                // Ctrl+A: everything selected, the caret at the end.
                self.anchor = Some(0);
                self.caret = text.len();
            }
            // Other Ctrl+letters are the shell's shortcuts (copy/paste);
            // they must never insert.
            Key::Char(_) if mods.ctrl => return events,
            Key::Char(c) if !c.is_control() => {
                if let Some((lo, hi)) = sel {
                    // Typing REPLACES the selection — one edit; the
                    // result is shorter, so `max_length=` cannot grow.
                    text.splice(lo..hi, [c]);
                    self.caret = lo + 1;
                    self.anchor = None;
                } else {
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
            }
            // With a selection both erasers erase JUST the selection.
            Key::Backspace | Key::Delete if sel.is_some() => {
                let (lo, hi) = sel.unwrap();
                text.drain(lo..hi);
                self.caret = lo;
                self.anchor = None;
            }
            Key::Backspace if self.caret > 0 => {
                self.caret -= 1;
                text.remove(self.caret);
            }
            Key::Delete if self.caret < text.len() => {
                text.remove(self.caret);
            }
            Key::Left | Key::Right | Key::Home | Key::End if mods.shift => {
                // Shift extends: the anchor plants where the caret stood
                // and only the caret moves — Shift+Left on a rightward
                // selection SHRINKS it (one anchor, one moving end).
                if self.anchor.is_none() {
                    self.anchor = Some(self.caret);
                }
                self.caret = match k {
                    Key::Left => self.caret.saturating_sub(1),
                    Key::Right => (self.caret + 1).min(text.len()),
                    Key::Home => 0,
                    _ => text.len(),
                };
                if self.anchor == Some(self.caret) {
                    self.anchor = None;
                }
            }
            // A plain arrow COLLAPSES a selection to its side — no move
            // (the everywhere behavior).
            Key::Left if sel.is_some() => {
                self.caret = sel.unwrap().0;
                self.anchor = None;
            }
            Key::Right if sel.is_some() => {
                self.caret = sel.unwrap().1;
                self.anchor = None;
            }
            Key::Left => self.caret = self.caret.saturating_sub(1),
            Key::Right => self.caret = (self.caret + 1).min(text.len()),
            Key::Home => {
                self.caret = 0;
                self.anchor = None;
            }
            Key::End => {
                self.caret = text.len();
                self.anchor = None;
            }
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
        self.publish(rt, f);
        rt.dirty = true;
        if matches!(k, Key::Left | Key::Right | Key::Home | Key::End) || mods.ctrl {
            return events;
        }
        let now: String = text.iter().collect();
        rt.set_text(page, f, &now);
        self.push_event(rt, &mut events, f, EventKind::Edit(text_value(page, rt, f)), event_action(page, f, "edit"));
        events
    }

    /// Focus leaves: the ring and caret go away; an Input commits once
    /// when its text CHANGED since focus (the focus-loss commit door).
    fn blur(&mut self, page: &Page, rt: &mut Runtime, events: &mut Vec<Event>) {
        let Some(f) = self.focus.take() else { return };
        self.focus_visible = false;
        rt.set_state(f, FOCUSED, false);
        set_chain(page, rt, f, FOCUS_WITHIN, false);
        rt.caret = None;
        rt.selection = None;
        rt.sel_handles = false;
        rt.edit_bar = None;
        self.anchor = None;
        if page.nodes[f].kind == Kind::Input {
            let now = input_text(page, rt, f);
            if now != self.committed {
                self.push_event(rt, events, f, EventKind::Change(text_value(page, rt, f)), event_action(page, f, "change"));
            }
        }
        self.committed.clear();
    }

    /// The keyboard focus owner, if any.
    pub fn focused(&self) -> Option<NodeId> {
        self.focus
    }

    /// Hands focus over (None = just blur). The old owner blurs (an
    /// Input's pending text commits), the new one takes the FOCUSED bit
    /// by the D129 visible-focus rule: a key grant always shows the
    /// ring, a tap shows it only on caret controls — a tapped Button
    /// parks focus with no ring residue.
    fn grant(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], node: Option<NodeId>, by_key: bool, events: &mut Vec<Event>) {
        let moved = self.focus != node;
        if moved {
            self.blur(page, rt, events);
        }
        let Some(n) = node else { return };
        self.focus = Some(n);
        // D129 modality: a key grant shows the ring, a pointer press
        // turns it off — re-tapping an already-focused box clears a
        // ring earlier keys put there.
        self.focus_visible = by_key;
        let in_field = page.nodes[n].kind == Kind::Input;
        if in_field {
            if moved {
                self.committed = input_text(page, rt, n);
            }
            self.caret = input_text(page, rt, n).chars().count();
            self.anchor = None;
            self.publish(rt, n);
        }
        let warranted = self.focus_visible || in_field;
        rt.set_state(n, FOCUSED, warranted);
        set_chain(page, rt, n, FOCUS_WITHIN, warranted);
        // follow_focus (GPAR113): a KEY grant scrolls its box into view
        // — a tap is already in view under the finger.
        if by_key && moved {
            self.scroll_into_view(page, rt, res, rects, n);
        }
        rt.dirty = true;
    }

    /// Nudges every Scroll/List above `node` just enough that its rect
    /// sits inside the viewport (the addon's ScrollContainer
    /// `follow_focus`) — clamped to the pannable range, never an
    /// overscroll; a glide on that Scroll stands down.
    fn scroll_into_view(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], node: NodeId) {
        let mut s = scroll_ancestor(page, node);
        while let Some(sc) = s {
            let v = rects[sc];
            let r = rects[node];
            let mut accum = rt.scroll[sc];
            let before = accum;
            if r.bottom() > v.bottom() {
                accum[1] += r.bottom() - v.bottom();
            }
            if r.y < v.y {
                accum[1] -= v.y - r.y;
            }
            if r.right() > v.right() {
                accum[0] += r.right() - v.right();
            }
            if r.x < v.x {
                accum[0] -= v.x - r.x;
            }
            if accum != before {
                let range = scroll_range(page, res, rects, sc);
                accum[0] = accum[0].clamp(0.0, range[0]);
                accum[1] = accum[1].clamp(0.0, range[1]);
                if self.fling.as_ref().map_or(false, |fl| fl.scroll == sc) {
                    self.fling = None;
                }
                if rt.scroll[sc] != accum {
                    rt.scroll[sc] = accum;
                    rt.dirty = true;
                }
            }
            s = scroll_ancestor(page, sc);
        }
    }

    /// A focused Slider's steps (the shipped slider_behavior): arrows
    /// move by `step=` (1 with none), Home/End jump to the ends —
    /// quantized from `min` and clamped (Range's rule). A step commits
    /// `change` directly, no `edit` (a step and a programmatic write
    /// are indistinguishable at the seam); one landing on the current
    /// value reports nothing.
    fn step_slider(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, f: NodeId, k: Key, events: &mut Vec<Event>) {
        let min = res.num(page, f, PropKey::Min).unwrap_or(0.0);
        let max = res.num(page, f, PropKey::Max).unwrap_or(100.0);
        let step = match res.num(page, f, PropKey::Step) {
            Some(s) if s > 0.0 => s,
            _ => 1.0,
        };
        let was = value_of(rt, res, page, f);
        let v = match k {
            Key::Right | Key::Up => snap_step(page, res, f, was + step),
            Key::Left | Key::Down => snap_step(page, res, f, was - step),
            Key::Home => min,
            _ => max,
        };
        if v != was {
            rt.set_prop(f, PropKey::Value, Value::VNum(v));
            self.push_event(rt, events, f, EventKind::Change(Value::VNum(v)), event_action(page, f, "change"));
        }
    }

    /// A Toggle's logical sides (the shipped toggle_behavior):
    /// increment/End = on, decrement/Home = off; `selected` follows
    /// live, `change(bool)` once per genuine transition.
    fn set_toggle(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, f: NodeId, on: bool, events: &mut Vec<Event>) {
        let was = toggle_on(rt, res, page, f);
        rt.set_prop(f, PropKey::Value, Value::VNum(if on { 1.0 } else { 0.0 }));
        rt.set_state(f, SELECTED, on);
        if on != was {
            self.push_event(rt, events, f, EventKind::Change(Value::VBool(on)), event_action(page, f, "change"));
        }
    }

    /// Enter/Space on the focused control — the completions a finished
    /// tap runs (`change` first, then `activate`, the toggling-surface
    /// order), so a key accept and a tap are one law.
    fn activate_focused(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, f: NodeId, events: &mut Vec<Event>) {
        // `tap_pulse=true`: the press feedback plays on the completed tap
        // (the shipped `button.Pressed += Fx.TapPulse` hook — Godot's
        // `pressed` fires on release), a turnstile swing here
        // (`Runtime::tap_pulse`).
        if res.bool(page, f, PropKey::TapPulse).unwrap_or(false) {
            rt.tap_pulse(f);
        }
        match page.nodes[f].kind {
            Kind::Button => {
                if res.bool(page, f, PropKey::ToggleMode).unwrap_or(false) {
                    let on = !rt.has(f, SELECTED);
                    rt.set_state(f, SELECTED, on);
                    self.push_event(rt, events, f, EventKind::Change(Value::VBool(on)), event_action(page, f, "change"));
                }
                self.push_event(rt, events, f, EventKind::Activate, event_action(page, f, "activate"));
            }
            Kind::ListItem => {
                if let Some(list) = list_ancestor(page, f) {
                    if rt.tap_select(page, list, f) {
                        self.push_event(rt, events, list, EventKind::Change(Value::VIdent(page.nodes[f].name)), event_action(page, list, "change"));
                    }
                }
                self.push_event(rt, events, f, EventKind::Activate, event_action(page, f, "activate"));
            }
            Kind::Check => {
                let on = !rt.has(f, SELECTED);
                rt.set_state(f, SELECTED, on);
                self.push_event(rt, events, f, EventKind::Change(Value::VBool(on)), event_action(page, f, "change"));
                self.push_event(rt, events, f, EventKind::Activate, event_action(page, f, "activate"));
            }
            Kind::Toggle => {
                let was = toggle_on(rt, res, page, f);
                self.set_toggle(page, rt, res, f, !was, events);
                self.push_event(rt, events, f, EventKind::Activate, event_action(page, f, "activate"));
            }
            Kind::Radio => {
                let already = rt.has(f, SELECTED);
                let choice = page.nodes[f].choice;
                let mut change_action: Option<Action> = None;
                if choice != NO_CHOICE {
                    for (i, n) in page.nodes.iter().enumerate() {
                        if n.kind == Kind::Radio && n.choice == choice {
                            rt.set_state(NodeId::from_index(i), SELECTED, NodeId::from_index(i) == f);
                        }
                    }
                    change_action = page.choices[choice as usize].action;
                }
                if !already {
                    self.push_event(rt, events, f, EventKind::Change(Value::VIdent(page.nodes[f].name)), change_action);
                }
                self.push_event(rt, events, f, EventKind::Activate, event_action(page, f, "activate"));
            }
            _ => {}
        }
    }

    /// A wheel tick rides the same logical pair the arrows use (spec
    /// section 5.5: keyboard, reader and wheel share one path): up =
    /// increment, down = decrement, on the steppable control under the
    /// POINTER (Slider, Toggle) — which also takes focus, parked like
    /// any pointer grant. Panning a Scroll by wheel is the desktop
    /// conveniences round (GP42), not this.
    pub fn wheel(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], x: f32, y: f32, up: bool) -> Vec<Event> {
        let mut events = Vec::new();
        let mut n = hit(page, res, rects, x, y, false);
        // GP52/GP53: a `behavior` instance is steppable anywhere on its
        // surface — found before the kind walk (its root wears a body
        // kind, not Slider). The behavior hears increment/decrement
        // whether or not it owns the arrow keys (the addon's wheel law).
        if let Some(bi) = n.and_then(|h| behavior_at(page, h)) {
            let b = live_behavior(page, res, bi);
            if rt.disabled(page, b.root) {
                return events;
            }
            if let Some(mi) = self.registry.find(page.str(b.name)) {
                self.grant(page, rt, res, rects, Some(b.root), false, &mut events);
                self.run_key_action(page, rt, res, rects, &b, mi, if up { KeyAction::Increment } else { KeyAction::Decrement }, &mut events);
            }
            return events;
        }
        while let Some(c) = n {
            if matches!(page.nodes[c].kind, Kind::Slider | Kind::Toggle) {
                break;
            }
            let p = page.nodes[c].parent;
            n = if p == NO_NODE { None } else { Some(p) };
        }
        let Some(c) = n else { return events };
        if rt.disabled(page, c) {
            return events;
        }
        self.grant(page, rt, res, rects, Some(c), false, &mut events);
        match page.nodes[c].kind {
            Kind::Slider => self.step_slider(page, rt, res, c, if up { Key::Up } else { Key::Down }, &mut events),
            _ => self.set_toggle(page, rt, res, c, up, &mut events),
        }
        events
    }

    /// The selected span ordered (lo, hi) in chars; `None` = nothing
    /// selected (a collapsed anchor counts as nothing).
    fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        if a == self.caret {
            return None;
        }
        Some((a.min(self.caret), a.max(self.caret)))
    }

    /// The focused field's selected text — the shell's copy door reads
    /// it (gogo never touches a clipboard itself).
    pub fn selected_text(&self, page: &Page, rt: &Runtime) -> Option<String> {
        let f = self.focus.filter(|f| page.nodes[*f].kind == Kind::Input)?;
        let (lo, hi) = self.selection()?;
        Some(input_text(page, rt, f).chars().skip(lo).take(hi - lo).collect())
    }

    /// Pastes `text` into the focused field — a standing selection is
    /// replaced, else it inserts at the caret. One `edit` when the text
    /// genuinely changes; `max_length=` caps the RESULT like typing (the
    /// paste is trimmed to fit); control characters are dropped (a
    /// single-line field). The clipboard itself is the shell's — gogo
    /// only takes text in.
    pub fn paste(&mut self, page: &mut Page, rt: &mut Runtime, paste: &str) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(f) = self.focus.filter(|f| page.nodes[*f].kind == Kind::Input) else { return events };
        let mut text: Vec<char> = input_text(page, rt, f).chars().collect();
        self.caret = self.caret.min(text.len());
        self.anchor = self.anchor.map(|a| a.min(text.len())).filter(|a| *a != self.caret);
        let (lo, hi) = self.selection().unwrap_or((self.caret, self.caret));
        let cap = match page.prop(f, PropKey::MaxLength) {
            Some(Value::VNum(n)) if n > 0.0 => n as usize,
            _ => usize::MAX,
        };
        let room = cap.saturating_sub(text.len() - (hi - lo));
        let ins: Vec<char> = paste.chars().filter(|c| !c.is_control()).take(room).collect();
        let n = ins.len();
        text.splice(lo..hi, ins);
        self.caret = lo + n;
        self.anchor = None;
        self.publish(rt, f);
        rt.dirty = true;
        let now: String = text.iter().collect();
        if now != input_text(page, rt, f) {
            rt.set_text(page, f, &now);
            self.push_event(rt, &mut events, f, EventKind::Edit(text_value(page, rt, f)), event_action(page, f, "edit"));
        }
        events
    }

    /// Removes the selected span (the cut's second half — the shell
    /// copies first, then calls this). One `edit`; nothing selected =
    /// nothing happens.
    pub fn delete_selection(&mut self, page: &mut Page, rt: &mut Runtime) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(f) = self.focus.filter(|f| page.nodes[*f].kind == Kind::Input) else { return events };
        let Some((lo, hi)) = self.selection() else { return events };
        let mut text: Vec<char> = input_text(page, rt, f).chars().collect();
        let hi = hi.min(text.len());
        let lo = lo.min(hi);
        text.drain(lo..hi);
        self.caret = lo;
        self.anchor = None;
        self.publish(rt, f);
        rt.dirty = true;
        let now: String = text.iter().collect();
        rt.set_text(page, f, &now);
        self.push_event(rt, &mut events, f, EventKind::Edit(text_value(page, rt, f)), event_action(page, f, "edit"));
        events
    }

    /// Writes the caret + selection into the runtime — the drawer takes
    /// the caret bar and the highlight from these two.
    fn publish(&self, rt: &mut Runtime, f: NodeId) {
        rt.caret = Some((f, self.caret as u32));
        rt.selection = self.selection().map(|(lo, hi)| (f, lo as u32, hi as u32));
        rt.sel_handles = self.touch_sel && rt.selection.is_some();
    }

    /// Code hands the keyboard to `node` (the shipped `GrabFocus` — the
    /// text page's edit box takes the caret the moment a callout is
    /// tapped for editing, no second tap on the box); `None` takes it
    /// back (`ReleaseFocus`). The field's caret lands at its end and the
    /// host's IME arm reads the runtime's caret as it would after a tap.
    /// Returns the events the hand-over raises (the old owner's commit).
    pub fn focus_on(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], node: Option<NodeId>) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        self.grant(page, rt, res, rects, node, false, &mut events);
        if node.is_none() {
            rt.caret = None;
            rt.selection = None;
            rt.sel_handles = false;
        }
        events
    }

    /// The focused Input's text + selection span for seeding an IME
    /// (GPAR112): `None` when no Input holds the caret. No selection =
    /// a collapsed span at the caret.
    pub fn ime_seed(&self, page: &Page, rt: &Runtime) -> Option<(String, usize, usize)> {
        let f = self.focus.filter(|f| page.nodes[*f].kind == Kind::Input)?;
        let (lo, hi) = self.selection().unwrap_or((self.caret, self.caret));
        Some((input_text(page, rt, f), lo, hi))
    }

    /// One whole-text update FROM an IME (GPAR112, the GameActivity
    /// path): an Android soft keyboard owns the text while a field is
    /// focused — autocorrect and swipe typing rewrite spans, so its
    /// edits arrive as the full text + cursor, never as keys. Applies
    /// `max_length` like typing, moves the caret, streams `edit` once
    /// per genuine change; the commit law is untouched (Enter and the
    /// focus-loss doors still commit once per genuine change).
    pub fn ime_replace(&mut self, page: &mut Page, rt: &mut Runtime, text: &str, start: usize, end: usize) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(f) = self.focus.filter(|f| page.nodes[*f].kind == Kind::Input) else { return events };
        let cap = match page.prop(f, PropKey::MaxLength) {
            Some(Value::VNum(n)) if n > 0.0 => n as usize,
            _ => usize::MAX,
        };
        let new: String = text.chars().take(cap).collect();
        let len = new.chars().count();
        self.caret = end.min(len);
        // The IME's span becomes the selection (a keyboard's own
        // long-press select shows in the field).
        self.anchor = Some(start.min(len)).filter(|a| *a != self.caret);
        self.publish(rt, f);
        rt.dirty = true;
        if new != input_text(page, rt, f) {
            rt.set_text(page, f, &new);
            self.push_event(rt, &mut events, f, EventKind::Edit(text_value(page, rt, f)), event_action(page, f, "edit"));
        }
        events
    }

    /// One mouse-motion sample — the shell calls this for a REAL mouse
    /// only (D75: touch never hovers). The topmost input-taking box
    /// under the cursor and EVERY ancestor hold `hovered` (the chain
    /// reading, user call 2026-09-02); while a press is active hover is
    /// FROZEN — the capture bracket: slipping off a held button keeps
    /// its hovered look, and nothing new hovers until release. Motion
    /// never changes modality (D129).
    pub fn hover(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], x: f32, y: f32) {
        if self.press.is_some() {
            return;
        }
        self.apply_hover(page, rt, hit(page, res, rects, x, y, false));
    }

    /// The mouse left the window (or this page): nothing hovers here.
    pub fn unhover(&mut self, page: &Page, rt: &mut Runtime) {
        self.apply_hover(page, rt, None);
    }

    fn apply_hover(&mut self, page: &Page, rt: &mut Runtime, new: Option<NodeId>) {
        if self.hover == new {
            return;
        }
        if let Some(old) = self.hover {
            set_chain(page, rt, old, HOVERED, false);
        }
        if let Some(n) = new {
            set_chain(page, rt, n, HOVERED, true);
        }
        self.hover = new;
        rt.dirty = true;
    }

    /// Tab's next stop: the reachable boxes in FILE order, wrapping;
    /// nothing focused enters at the first (Shift-Tab at the last).
    fn tab_target(&self, page: &Page, rt: &Runtime, res: &Resolved, back: bool) -> Option<NodeId> {
        let list: Vec<NodeId> = page.node_ids().filter(|n| reachable(page, rt, res, *n)).collect();
        let pos = self.focus.and_then(|f| list.iter().position(|n| *n == f));
        match (pos, back) {
            (Some(i), false) => Some(list[(i + 1) % list.len()]),
            (Some(i), true) => Some(list[(i + list.len() - 1) % list.len()]),
            (None, false) => list.first().copied(),
            (None, true) => list.last().copied(),
        }
    }

    /// An arrow's next stop: the nearest reachable box whose center lies
    /// past the focused box's center in the arrow's direction, scored by
    /// the distance along the arrow plus DOUBLE the sideways offset — an
    /// aligned neighbor beats a nearer diagonal one, so columns stay
    /// columns.
    fn arrow_target(&self, page: &Page, rt: &Runtime, res: &Resolved, rects: &[Rect], dx: f32, dy: f32) -> Option<NodeId> {
        let f = self.focus?;
        let fr = rects[f];
        let (cx, cy) = (fr.x + fr.w / 2.0, fr.y + fr.h / 2.0);
        let mut best: Option<(f32, NodeId)> = None;
        for n in page.node_ids() {
            if n == f || !reachable(page, rt, res, n) {
                continue;
            }
            let r = rects[n.index()];
            let (ex, ey) = (r.x + r.w / 2.0 - cx, r.y + r.h / 2.0 - cy);
            let ahead = ex * dx + ey * dy;
            if ahead <= 0.5 {
                continue;
            }
            let score = ahead + 2.0 * ((ex * dy).abs() + (ey * dx).abs());
            if best.map_or(true, |(b, _)| score < b) {
                best = Some((score, n));
            }
        }
        best.map(|(_, n)| n)
    }

    /// Selects the focused field's whole text, caret at the end — the
    /// edit bar's All (the shell answers that tap), same result as
    /// Ctrl+A. Returns false when no field holds focus.
    pub fn select_all(&mut self, page: &Page, rt: &mut Runtime) -> bool {
        let Some(f) = self.focus.filter(|f| page.nodes[*f].kind == Kind::Input) else { return false };
        let len = input_text(page, rt, f).chars().count();
        self.anchor = Some(0).filter(|_| len > 0);
        self.caret = len;
        self.touch_sel = true;
        self.publish(rt, f);
        rt.dirty = true;
        true
    }

    /// Feeds one touch sample; returns the events it completed.
    pub fn touch(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], measure: &dyn TextMeasure, t: Touch) -> Vec<Event> {
        let mut events = Vec::new();
        match t.phase {
            Phase::Down => {
                if self.press.is_some() {
                    // A second finger mid-gesture is swallowed.
                    return events;
                }
                // A selection handle grab comes FIRST: the circles hang
                // BELOW the field, over whatever sits there — the press
                // adjusts that end and never reaches the page (the bar
                // stays up; adjusting is not dismissing).
                let mut grabbed: Option<(NodeId, bool)> = None;
                if let Some(f) = self.focus.filter(|f| page.nodes[*f].kind == Kind::Input) {
                    if rt.sel_handles {
                        if let Some((lo, hi)) = self.selection() {
                            let bottom = rects[f].y + rects[f].h;
                            let xlo = x_from_index(page, rt, res, rects, measure, f, lo);
                            let xhi = x_from_index(page, rt, res, rects, measure, f, hi);
                            let grab = |x: f32| {
                                let r = sel_handle_rect(bottom, x, SEL_HANDLE_EPX);
                                Rect::new(r.x - SEL_HANDLE_GRAB_EPX, r.y - SEL_HANDLE_GRAB_EPX, r.w + 2.0 * SEL_HANDLE_GRAB_EPX, r.h + 2.0 * SEL_HANDLE_GRAB_EPX).contains(t.x, t.y)
                            };
                            grabbed = match (grab(xlo), grab(xhi)) {
                                // Both in reach (a short selection): the nearer end.
                                (true, true) => Some((f, (t.x - xhi).abs() <= (t.x - xlo).abs())),
                                (false, true) => Some((f, true)),
                                (true, false) => Some((f, false)),
                                (false, false) => None,
                            };
                        }
                    }
                }
                if let Some((f, hi_end)) = grabbed {
                    self.drag_handle = Some(hi_end);
                    self.press = Some(Press { node: f, kind: Kind::Input, id: t.id, start: (t.x, t.y), scroll: None, panning: false, last: (t.x, t.y), last_t: t.t, samples: Samples::EMPTY, caught_fling: false, accum: [0.0; 2], value_from: 0.0, behavior: None, registered: 0, toggle_moved: false, release_owed: false, raw: None });
                    return events;
                }
                // A glide is stopped only by a touch that lands ON the
                // gliding Scroll or its content — chrome drawn over it
                // keeps its own press and the list keeps rolling
                // underneath (D130, GPAR4). A FAST catch is "caught": the
                // child under the finger is not pressed by accident.
                // The edit bar lives on its own page above this one — a
                // press reaching THIS machine is off the bar by
                // construction, and any such press dismisses it (the
                // press then runs its normal course).
                if rt.edit_bar.is_some() {
                    rt.edit_bar = None;
                    rt.dirty = true;
                }
                let hit_node = hit(page, res, rects, t.x, t.y, false);
                let mut caught_accum: Option<(NodeId, [f32; 2])> = None;
                let caught = match self.fling.take() {
                    Some(f) if hit_node.is_some_and(|n| within(page, n, f.scroll)) => {
                        let range = scroll_range(page, res, rects, f.scroll);
                        let raw = [f.accum[0] - f.accum[0].clamp(0.0, range[0]), f.accum[1] - f.accum[1].clamp(0.0, range[1])];
                        // The pull stays under the finger (GPAR5, the
                        // shipped catch): freezing a mid-spring bounce
                        // keeps the softened offset — snapping it to the
                        // edge would jump the content on touch.
                        let mut accum = f.accum;
                        let axes = scroll_axes(page, res, f.scroll);
                        apply_accum(rt, rects, f.scroll, range, axes, &mut accum);
                        caught_accum = Some((f.scroll, accum));
                        self.expected = Some((f.scroll, rt.scroll[f.scroll]));
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
                let kind = page.nodes[node].kind;
                if rt.disabled(page, node) {
                    return events;
                }
                // GP52/GP53: a press anywhere inside a `behavior`
                // instance is the behavior's — the gesture, focus and
                // `held` all land on the instance root (shipped
                // SliderPhone's whole row was the surface). The behavior
                // hears the press FIRST: `Claim::Pass` lets it go on as
                // an ordinary press on the root; a caught fling's press
                // stops the glide and nothing else.
                let mut behavior = behavior_at(page, node);
                let (node, kind) = match behavior {
                    Some(i) => {
                        let root = page.behaviors[i as usize].root;
                        (root, page.nodes[root].kind)
                    }
                    None => (node, kind),
                };
                let mut registered: u16 = 0;
                let mut value_from = 0.0;
                let mut began: Option<Cx> = None;
                if let Some(bi) = behavior {
                    let b = live_behavior(page, res, bi);
                    match self.registry.find(page.str(b.name)) {
                        Some(mi) if !caught => {
                            value_from = b_value_of(rt, res, page, &b);
                            let mut cx = Cx::new(rects, &b, value_from);
                            let r = rects[b.root];
                            if self.registry.get(mi).began(&mut cx, t.x - r.x, t.y - r.y) == Claim::Take {
                                registered = mi;
                                began = Some(cx);
                            } else {
                                behavior = None;
                            }
                        }
                        Some(_) => behavior = None,
                        None => {
                            debug_assert!(false, "`behavior {}` names a behavior nobody registered — Input::register", page.str(b.name));
                            behavior = None;
                        }
                    }
                }
                // A tap moves focus: onto the tapped box when it's
                // keyboard-reachable (an Input also takes the caret;
                // D129 — only caret controls warrant a tap ring), off
                // to nothing otherwise (GPAR112 — the focus-loss commit
                // door: Enter, blur; once per genuine change).
                self.grant(page, rt, res, rects, Some(node).filter(|n| page.nodes[*n].focusable), false, &mut events);
                if kind == Kind::Input && self.focus == Some(node) {
                    // The press puts the caret where it LANDED (it used
                    // to jump to the end of the text); a second press
                    // within DOUBLE_CLICK_S selects the word there.
                    let idx = index_from_x(page, rt, res, rects, measure, node, t.x);
                    let dbl = self.last_field_press.is_some_and(|(pt, pn)| pn == node && t.t - pt < DOUBLE_CLICK_S);
                    self.last_field_press = Some((t.t, node));
                    if dbl {
                        let chars: Vec<char> = input_text(page, rt, node).chars().collect();
                        let (lo, hi) = word_at(&chars, idx);
                        self.anchor = Some(lo).filter(|_| lo != hi);
                        self.caret = hi;
                        self.touch_sel = t.id != MOUSE_ID;
                    } else {
                        self.caret = idx;
                        self.anchor = None;
                    }
                    self.press_index = idx;
                    self.hold = 0.0;
                    self.publish(rt, node);
                }
                let raw = event_action(page, node, "pointer");
                if let Some(action) = raw {
                    let r = rects[node];
                    self.push_event(rt, &mut events, node, EventKind::Pointer { phase: Phase::Down, x: t.x - r.x, y: t.y - r.y }, Some(action));
                    self.press = Some(Press { node, kind, id: t.id, start: (t.x, t.y), scroll: None, panning: false, last: (t.x, t.y), last_t: t.t, samples: Samples::EMPTY, caught_fling: caught, accum: [0.0; 2], value_from: 0.0, behavior: None, registered: 0, toggle_moved: false, release_owed: false, raw });
                    return events;
                }
                let scroll = if matches!(kind, Kind::Scroll | Kind::List) { Some(node) } else { scroll_ancestor(page, node) };
                let pressed = (kind.is_press_surface() || kind == Kind::Slider || behavior.is_some()) && !caught;
                if pressed {
                    rt.set_state(node, HELD, true);
                    if let Some(action) = event_action(page, node, "press") {
                        self.push_event(rt, &mut events, node, EventKind::Press, Some(action));
                    }
                }
                if kind == Kind::Toggle {
                    value_from = if rt.writes.iter().any(|(n, w)| *n == node && w.key == PropKey::Value) { value_of(rt, res, page, node) } else if rt.has(node, SELECTED) { 1.0 } else { 0.0 };
                }
                if let (Some(bi), Some(cx)) = (behavior, began) {
                    let b = live_behavior(page, res, bi);
                    // The gesture's opening edge first, when declared —
                    // the addon's behaviors emitted, then applied; then
                    // whatever the behavior wrote on the press (a slider
                    // jumps under the finger, a toggle writes nothing).
                    if b.drag_started != NO_STR {
                        self.push_event(rt, &mut events, b.root, EventKind::Signal, Some(Action::Emit(b.drag_started)));
                    }
                    self.land_behavior_write(page, rt, &mut events, &b, value_from, &cx, true);
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
                    (Some(s), _) => rt.scroll[s],
                    (None, _) => [0.0; 2],
                };
                self.press = Some(Press { node, kind, id: t.id, start: (t.x, t.y), scroll, panning: false, last: (t.x, t.y), last_t: t.t, samples: Samples::EMPTY, caught_fling: caught, accum, value_from, behavior, registered, toggle_moved: false, release_owed: pressed, raw: None });
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
                        let r = rects[p.node];
                        self.push_event(rt, &mut events, p.node, EventKind::Pointer { phase: Phase::Cancel, x: t.x - r.x, y: t.y - r.y }, Some(action));
                    }
                    self.release(page, rt, &mut events, &mut p);
                    self.cancel_press(page, rt, &mut events, &p);
                    self.press = None;
                    return events;
                }
                if let Some(action) = p.raw {
                    let r = rects[p.node];
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
                        let child_keeps = ((p.kind == Kind::Slider || p.behavior.is_some()) && !vertical) || (p.kind == Kind::Toggle && p.toggle_moved) || (p.kind == Kind::Input && (t.id == MOUSE_ID || self.drag_handle.is_some()));
                        let taker = if child_keeps { None } else { scroll_taker(page, res, rects, p.node, vertical) };
                        match taker {
                            Some(scroll) => {
                                p.scroll = Some(scroll);
                                p.panning = true;
                                self.release(page, rt, &mut events, &mut p);
                                self.cancel_press(page, rt, &mut events, &p);
                                p.accum = rt.scroll[scroll];
                            }
                            None => p.scroll = None,
                        }
                    }
                }
                if let (Some(bi), false) = (p.behavior, p.panning) {
                    // One `edit` per changed write, none for a same-value
                    // sample (GPAR6, same as the built-in below).
                    let b = live_behavior(page, res, bi);
                    let shown = b_value_of(rt, res, page, &b);
                    let mut cx = Cx::new(rects, &b, shown);
                    let r = rects[b.root];
                    self.registry.get(p.registered).moved(&mut cx, t.x - r.x, t.y - r.y);
                    self.land_behavior_write(page, rt, &mut events, &b, shown, &cx, true);
                } else if p.kind == Kind::Slider && !p.panning {
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
                } else if p.kind == Kind::Input && self.drag_handle.is_some() && !p.panning && self.focus == Some(p.node) {
                    // A handle drag moves ITS end; the selection never
                    // collapses — at least one char stays (lift and tap
                    // to place a caret instead).
                    if let Some((lo, hi)) = self.selection() {
                        let idx = index_from_x(page, rt, res, rects, measure, p.node, t.x);
                        let hi_end = self.drag_handle == Some(true);
                        let (fixed, moved) = if hi_end { (lo, idx.max(lo + 1)) } else { (hi, idx.min(hi - 1)) };
                        self.anchor = Some(fixed);
                        self.caret = moved;
                        self.publish(rt, p.node);
                        rt.dirty = true;
                    }
                } else if p.kind == Kind::Input && t.id == MOUSE_ID && !p.panning && self.focus == Some(p.node) {
                    // The mouse drag selects: the anchor is where the
                    // press landed, the caret follows the cursor. A
                    // FINGER pans the Scroll instead — its selection
                    // door is the long-press (step 3). A drag-born
                    // selection shows no handles.
                    self.touch_sel = false;
                    let idx = index_from_x(page, rt, res, rects, measure, p.node, t.x);
                    if self.anchor.is_none() {
                        self.anchor = Some(self.caret);
                    }
                    self.caret = idx;
                    if self.anchor == Some(idx) {
                        self.anchor = None;
                    }
                    self.publish(rt, p.node);
                    rt.dirty = true;
                } else if p.panning {
                    if let Some(scroll) = p.scroll {
                        let axes = scroll_axes(page, res, scroll);
                        let range = scroll_range(page, res, rects, scroll);
                        // An enabled axis with no travel takes no pan
                        // (GP36): a diagonal drag on a vertical list must
                        // not rubber-band sideways.
                        let axes = (axes.0 && range[0] > 0.0, axes.1 && range[1] > 0.0);
                        if axes.0 {
                            p.accum[0] -= delta.0;
                        }
                        if axes.1 {
                            p.accum[1] -= delta.1;
                        }
                        apply_accum(rt, rects, scroll, range, axes, &mut p.accum);
                        self.expected = Some((scroll, rt.scroll[scroll]));
                    }
                }
                if p.kind.is_press_surface() && !p.panning && !p.caught_fling {
                    // Slide-off clears the press, slide-back re-arms it. The
                    // node's OWN rect decides, not the top-most hit — a
                    // plate opening over it mid-press is not a slide-off.
                    let inside = rects[p.node].contains(t.x, t.y);
                    rt.set_state(p.node, HELD, inside);
                }
                p.last = (t.x, t.y);
                p.last_t = t.t;
                self.press = Some(p);
            }
            Phase::Up | Phase::Cancel => {
                // A handle drag ends where the finger lifts — the
                // selection (and the bar) stand.
                self.drag_handle = None;
                // The hover capture bracket closes here: a mouse that
                // slipped off mid-press re-checks where it is — back
                // over the box keeps hover, anywhere else clears it.
                // Gated on hover being SET at all: only mouse motion
                // sets it, so a finger's release changes nothing (D75).
                if self.hover.is_some() && self.press.map_or(false, |p| p.id == t.id) {
                    self.apply_hover(page, rt, hit(page, res, rects, t.x, t.y, false));
                }
                let Some(mut p) = self.press else { return events };
                if p.id != t.id {
                    return events;
                }
                self.press = None;
                if let Some(action) = p.raw {
                    let r = rects[p.node];
                    self.push_event(rt, &mut events, p.node, EventKind::Pointer { phase: t.phase, x: t.x - r.x, y: t.y - r.y }, Some(action));
                    return events;
                }
                rt.set_state(p.node, HELD, false);
                self.release(page, rt, &mut events, &mut p);
                if p.panning {
                    if let Some(scroll) = p.scroll {
                        let axes = scroll_axes(page, res, scroll);
                        let range = scroll_range(page, res, rects, scroll);
                        // An enabled axis with no travel glides nothing (GP36).
                        let axes = (axes.0 && range[0] > 0.0, axes.1 && range[1] > 0.0);
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
                        self.expected = Some((scroll, rt.scroll[scroll]));
                    }
                    return events;
                }
                if t.phase == Phase::Cancel {
                    self.cancel_press(page, rt, &mut events, &p);
                    return events;
                }
                if p.caught_fling {
                    return events;
                }
                // Released inside = inside the pressed node's OWN rect (a dialog
                // opening under the finger does not turn the tap into a miss).
                let inside = rects[p.node].contains(t.x, t.y) && !(rt.disabled(page, p.node) || res.gone(page, p.node) || res.hidden(page, p.node));
                // `tap_pulse=true`: the press feedback plays on the completed
                // tap (the shipped `button.Pressed += Fx.TapPulse` hook —
                // Godot's `pressed` fires on release, inside), a turnstile
                // swing here (`Runtime::tap_pulse`).
                if inside && p.kind.is_press_surface() && res.bool(page, p.node, PropKey::TapPulse).unwrap_or(false) {
                    rt.tap_pulse(p.node);
                }
                // GP52/GP53: the behavior settles (a toggle's tap flips
                // here), then the commit like the built-in — changed
                // means CHANGED (its root wears a body kind, so the
                // match below can't route it). `held` is off by now, so
                // the settle write GLIDES by the control's feel.
                if let Some(bi) = p.behavior {
                    let b = live_behavior(page, res, bi);
                    let shown = b_value_of(rt, res, page, &b);
                    let mut cx = Cx::new(rects, &b, shown);
                    let r = rects[b.root];
                    self.registry.get(p.registered).ended(&mut cx, t.x - r.x, t.y - r.y);
                    let v = self.land_behavior_write(page, rt, &mut events, &b, shown, &cx, false);
                    if changed(&b, p.value_from, v) {
                        self.push_event(rt, &mut events, b.root, EventKind::Change(change_value(&b, v)), event_action(page, b.root, "change"));
                    }
                    // The closing edge, after the commit (the
                    // change-then-activate order's cousin).
                    if b.drag_ended != NO_STR {
                        self.push_event(rt, &mut events, b.root, EventKind::Signal, Some(Action::Emit(b.drag_ended)));
                    }
                }
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
                        // Selection is the mode's side effect (spec §5, the
                        // List owns it): `one` selects — never deselects by
                        // tap — `many` toggles, `off` nothing. `change` on
                        // the LIST first, then the row's `activate` (the
                        // toggling-surface order above).
                        if let Some(list) = list_ancestor(page, p.node) {
                            if rt.tap_select(page, list, p.node) {
                                self.push_event(rt, &mut events, list, EventKind::Change(Value::VIdent(page.nodes[p.node].name)), event_action(page, list, "change"));
                            }
                        }
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
                        let choice = page.nodes[p.node].choice;
                        let mut change_action: Option<Action> = None;
                        if choice != NO_CHOICE {
                            for (i, n) in page.nodes.iter().enumerate() {
                                if n.kind == Kind::Radio && n.choice == choice {
                                    rt.set_state(NodeId::from_index(i), SELECTED, NodeId::from_index(i) == p.node);
                                }
                            }
                            change_action = page.choices[choice as usize].action;
                        }
                        if !already {
                            self.push_event(rt, &mut events, p.node, EventKind::Change(Value::VIdent(page.nodes[p.node].name)), change_action);
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
        // The long-press door — a FINGER only (the mouse has drag-select
        // and Ctrl): a field press held still for LONG_PRESS_S selects
        // the word it landed on and raises the edit bar.
        if let Some(p) = self.press {
            if p.kind == Kind::Input && p.id != MOUSE_ID && !p.panning && self.focus == Some(p.node) {
                let (dx, dy) = (p.last.0 - p.start.0, p.last.1 - p.start.1);
                if (dx * dx + dy * dy).sqrt() <= SCROLL_DEADZONE {
                    let before = self.hold;
                    self.hold += dt;
                    if before < LONG_PRESS_S && self.hold >= LONG_PRESS_S {
                        let chars: Vec<char> = input_text(page, rt, p.node).chars().collect();
                        let (lo, hi) = word_at(&chars, self.press_index);
                        self.anchor = Some(lo).filter(|_| lo != hi);
                        self.caret = hi;
                        self.touch_sel = true;
                        self.publish(rt, p.node);
                        rt.edit_bar = Some(p.node);
                        rt.dirty = true;
                    }
                }
            }
        }
        let Some(mut f) = self.fling else { return false };
        if let Some((s, expected)) = self.expected {
            if s == f.scroll && rt.scroll[s] != expected {
                self.fling = None;
                self.expected = None;
                return false;
            }
        }
        let axes = scroll_axes(page, res, f.scroll);
        let range = scroll_range(page, res, rects, f.scroll);
        let before = rt.scroll[f.scroll];
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
        let moved = rt.scroll[f.scroll] != before;
        let speed = (f.v[0].powi(2) + f.v[1].powi(2)).sqrt();
        if speed < FLING_MIN_SPEED && (raw[0].powi(2) + raw[1].powi(2)).sqrt() < 0.75 {
            // Landed: exactly on the clamped position, nothing left to do.
            let mut accum = [f.accum[0].clamp(0.0, range[0]), f.accum[1].clamp(0.0, range[1])];
            apply_accum(rt, rects, f.scroll, range, axes, &mut accum);
            self.expected = Some((f.scroll, rt.scroll[f.scroll]));
            self.fling = None;
            return moved;
        }
        self.expected = Some((f.scroll, rt.scroll[f.scroll]));
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
            self.input.touch(&self.page, &mut self.rt, &res, &rects, &FixedAdvance, Touch { phase, x, y, t, id })
        }
        fn tick(&mut self, dt: f32) -> bool {
            let (res, rects) = self.lay();
            self.input.tick(&self.page, &mut self.rt, &res, &rects, dt)
        }
    }

    /// `tap_pulse=true`: a completed tap swings the box like a turnstile
    /// door about its LEFT edge — the angle rides the `swing` motion
    /// channel, which the draw walk turns into the 3D push — then it
    /// settles flat and the transform is gone (the D10 disarm law).
    #[test]
    fn tap_pulse_swings_about_the_left_edge_then_disarms() {
        let mut fx = Fx::new("View v { Button b tap_pulse=true, width=200, height=50 }");
        let b = fx.page.find("b").unwrap();
        let (_, rects) = fx.lay();
        fx.rt.glide(rects.clone());
        let r = rects[b];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        fx.touch(Phase::Down, x, y, 0.0);
        assert_eq!(fx.rt.tap_pulse_angle(b), 0.0, "a press alone swings nothing (shipped: the pulse rode `pressed`, the completed tap)");
        fx.touch(Phase::Up, x, y, 0.1);
        assert!(fx.rt.animating(), "the swing is live");
        fx.rt.tick(&mut fx.page, crate::runtime::TAP_PULSE_OUT_SEC);
        let angle = fx.rt.tap_pulse_angle(b);
        assert!((angle - crate::runtime::TAP_PULSE_ANGLE_DEG).abs() < 0.01, "fully out at the end of the out phase: {angle}");
        let res = resolve(&fx.page, &fx.rt);
        let m = res.motion[b];
        assert!((m.swing - angle).abs() < 1e-4, "the angle is the swing channel: {m:?}");
        assert_eq!((m.scale, m.translate), ([1.0, 1.0], [0.0, 0.0]), "a turnstile is a swing, not a squash");
        // The draw walk hinges the door on the left edge.
        let draws = crate::draw::draw(&fx.page, &res, &rects);
        let push = draws.iter().find_map(|d| if let crate::draw::Draw::SwingPush { angle: a, pivot_x, cx, cy, focal } = d { Some((*a, *pivot_x, *cx, *cy, *focal)) } else { None }).expect("a SwingPush");
        assert!((push.0 - angle).abs() < 1e-4 && push.1 == r.x && push.2 == r.x + r.w / 2.0 && push.3 == r.y + r.h / 2.0, "{push:?}");
        assert!((push.4 - crate::draw::swing_focal(r.h)).abs() < 1e-4, "the WP7 camera: {} vs {}", push.4, crate::draw::swing_focal(r.h));
        fx.rt.tick(&mut fx.page, crate::runtime::TAP_PULSE_BACK_SEC + 0.01);
        assert_eq!(fx.rt.tap_pulse_angle(b), 0.0, "disarmed");
        assert!(!fx.rt.animating());
        let res = resolve(&fx.page, &fx.rt);
        assert_eq!(res.motion[b], crate::anim::Motion::IDENTITY, "no standing transform after the swing");
        // A second tap mid-swing restarts the node's swing (one live play per target).
        fx.touch(Phase::Down, x, y, 1.0);
        fx.touch(Phase::Up, x, y, 1.1);
        fx.rt.tick(&mut fx.page, 0.03);
        let mid = fx.rt.tap_pulse_angle(b);
        fx.touch(Phase::Down, x, y, 2.0);
        fx.touch(Phase::Up, x, y, 2.1);
        assert!(fx.rt.tap_pulse_angle(b) < mid, "restarted from flat");
        // A button without the word swings nothing.
        let mut plain = Fx::new("View v { Button b width=200, height=50 }");
        let b2 = plain.page.find("b").unwrap();
        let (_, rects) = plain.lay();
        let r = rects[b2];
        plain.touch(Phase::Down, r.x + 1.0, r.y + 1.0, 0.0);
        plain.touch(Phase::Up, r.x + 1.0, r.y + 1.0, 0.1);
        assert!(!plain.rt.animating());
    }

    /// The arena pushes a parent's children together before their own
    /// children, so a control root's `id+1..subtree_end` range holds its
    /// later siblings' subtrees too. Membership is by ancestry: a press on
    /// the raw surface BESIDE a behavior slider is the surface's, and a
    /// swinging button leaves its neighbour's hit rect alone.
    #[test]
    fn a_control_root_does_not_own_its_later_siblings() {
        let src = "control S $value=0, $min=0, $max=10 {\n behavior slider by $value in $min..$max\n Layers width=fill, height=20 { slot track width=fill, height=fill { Rectangle width=fill, height=fill } }\n}\nView v {\n Column {\n  S a width=fill\n  Layers b interactive=true, width=fill, height=40, on:pointer=raw\n  Button c tap_pulse=true, width=fill, height=30\n  Button d width=fill, height=30\n }\n}\n";
        let mut fx = Fx::new(src);
        let (a, b, c, d) = (fx.page.find("a").unwrap(), fx.page.find("b").unwrap(), fx.page.find("c").unwrap(), fx.page.find("d").unwrap());
        assert!(fx.page.subtree_end(a) > b, "the precondition: b's index sits inside a's range");
        assert!(!fx.page.is_under(b, a) && fx.page.is_under(a, a));
        let (_, rects) = fx.lay();
        let rb = rects[b];
        let ev = fx.touch(Phase::Down, rb.x + 5.0, rb.y + 5.0, 0.0);
        assert!(ev.iter().all(|e| e.node == b && matches!(e.kind, EventKind::Pointer { .. })), "the raw surface's own pointer, not the slider's drag: {ev:?}");
        fx.touch(Phase::Up, rb.x + 5.0, rb.y + 5.0, 0.1);
        // A swing on c: d's drawn rect stands where the layout put it.
        fx.rt.glide(rects.clone());
        let rc = rects[c];
        fx.touch(Phase::Down, rc.x + 5.0, rc.y + 5.0, 1.0);
        fx.touch(Phase::Up, rc.x + 5.0, rc.y + 5.0, 1.1);
        fx.rt.tick(&mut fx.page, 0.04);
        let res = resolve(&fx.page, &fx.rt);
        assert!(res.motion[c].swing > 0.0, "c swings");
        let drawn = crate::draw::moved_rects(&fx.page, &res, &rects).expect("something moves");
        assert_eq!(drawn[d], rects[d], "d is not under c");
        assert_ne!(drawn[c], rects[c]);
        assert_eq!((drawn[c].x, drawn[c].h), (rects[c].x, rects[c].h), "the hinge edge and the near height stand; the far edge came in");
        assert!(drawn[c].w < rects[c].w);
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
        assert_eq!(fx.rt.scroll[s][1], 25.0);
        let ev = fx.touch(Phase::Up, 50.0, 10.0, 0.03);
        assert!(ev.is_empty(), "a pan never activates");
        assert!(fx.input.fling.is_some());
        let before = fx.rt.scroll[s][1];
        assert!(fx.tick(0.016));
        assert!(fx.rt.scroll[s][1] > before, "the glide keeps going");
        // Content is 400 tall in a 200 viewport: past 200 only by the soft
        // give, and the spring brings it home.
        let mut peak = 0.0f32;
        for _ in 0..400 {
            fx.tick(0.016);
            peak = peak.max(fx.rt.scroll[s][1]);
        }
        assert!(peak <= 250.0, "overscroll is capped at a quarter viewport: {peak}");
        assert!(fx.input.fling.is_none());
        assert_eq!(fx.rt.scroll[s][1], 200.0, "landed exactly on the edge");
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
        assert_eq!(fx.rt.scroll[page], [0.0, 0.0], "a sideways drag never scrolls the page");
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
        assert!(fx.rt.scroll[page][1] > 0.0, "the page took the drag");
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
        assert_eq!(fx.rt.scroll[page], [0.0, 0.0]);
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
        assert!(fx.rt.scroll[page][1] > 0.0, "up on the strip scrolls the page");
        assert_eq!(fx.rt.scroll[strip], [0.0, 0.0]);
        let mut fx = Fx::new(nested);
        fx.touch(Phase::Down, 300.0, 30.0, 0.0);
        fx.touch(Phase::Move, 260.0, 32.0, 0.1);
        fx.touch(Phase::Move, 220.0, 34.0, 0.2);
        assert!(fx.rt.scroll[strip][0] > 0.0, "sideways on the strip scrolls the strip");
        assert_eq!(fx.rt.scroll[page], [0.0, 0.0], "and not the page");
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
            let r = rects[n];
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
        fx.input.reset(&fx.page, &mut fx.rt);
        assert!(fx.input.fling.is_none() && fx.input.press.is_none());
        let mut fx = Fx::new(src);
        let b = fx.page.find("b").unwrap();
        fx.touch(Phase::Down, 100.0, 20.0, 1.0);
        assert!(fx.rt.has(b, HELD));
        fx.input.reset(&fx.page, &mut fx.rt);
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
        let y0 = fx.rt.scroll[page][1];
        assert!(y0 > 0.0, "panning");
        fx.rt.set_visible(&mut fx.page, b, false);
        fx.touch(Phase::Move, 104.0, -60.0, 0.2);
        assert!(fx.rt.scroll[page][1] > y0, "the pan continues after the child hid");
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
        let tr = rects[t];
        let pad = res.placement[t].padding[0];
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
        let rd = rects[d];
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
        let sr = rects[s];
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
        assert_eq!(fx.rt.arrangement, ArrangementId::new(0));
        fx.touch(Phase::Down, 10.0, 10.0, 1.0);
        fx.touch(Phase::Up, 10.0, 10.0, 1.1);
        assert_eq!(fx.rt.arrangement, BASE);
    }
}
