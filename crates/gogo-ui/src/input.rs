//! Touch → the spec's input vocabulary. One finger: a press lands on
//! the top-most input-taking node (`hit`); a Button holds `held` (slide-off
//! clears, slide-back re-arms) and fires `activate` on a completed tap;
//! Check/Toggle/Radio flip `selected` and fire `change`; a Slider streams
//! `edit` while dragged and commits `change` on release; a drag that
//! travels past the deadzone inside a Scroll cancels the child's press and
//! pans, releasing into a fling that `tick` decays. Coordinates are epx.

use crate::draw::hit;
use crate::layout::Rect;
use crate::page::*;
use crate::runtime::{HELD, Resolved, Runtime, SELECTED};

/// Pan starts after this much travel (spec: 16 epx).
pub const SCROLL_DEADZONE: f32 = 16.0;
/// Release-velocity amplifier (spec `fling_boost=2`).
pub const FLING_BOOST: f32 = 2.0;
/// Exponential decay per second (spec `fling_friction=1.5`).
pub const FLING_FRICTION: f32 = 1.5;
/// Below this speed (epx/s) a fling stops.
pub const FLING_MIN_SPEED: f32 = 40.0;

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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    /// Finger down on a press surface (`on:press=`).
    Press,
    Activate,
    Change(Value),
    Edit(Value),
}

/// What happened, on which node, and the `on:` action the file names for
/// it (`None` = the node's own name is the handler, per auto-wiring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub node: NodeId,
    pub kind: EventKind,
    pub action: Option<Action>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    node: NodeId,
    kind: Kind,
    start: (f32, f32),
    /// The Scroll ancestor that may take over the gesture.
    scroll: Option<NodeId>,
    panning: bool,
    last: (f32, f32),
    last_t: f64,
    velocity: [f32; 2],
    /// A Scroll that was gliding when the finger landed: stopped, and the
    /// press must not activate what is under the finger.
    caught_fling: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    press: Option<Press>,
    /// (scroll node, velocity epx/s) while gliding.
    fling: Option<(NodeId, [f32; 2])>,
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

fn scroll_axes(page: &Page, res: &Resolved, scroll: NodeId) -> (bool, bool) {
    match res.ident(page, scroll, PropKey::Allow) {
        Some("horizontal") => (true, false),
        Some("both") => (true, true),
        Some(_) => (false, true),
        None => {
            let p = &res.placement[scroll as usize];
            (matches!(p.width, Len::Exact(_) | Len::Fill(_)) && matches!(p.height, Len::Hug(_)), !matches!(p.height, Len::Hug(_)) || p.max_height.is_some())
        }
    }
}

fn slider_value(page: &Page, res: &Resolved, rects: &[Rect], node: NodeId, x: f32) -> f32 {
    let min = res.num(page, node, PropKey::Min).unwrap_or(0.0);
    let max = res.num(page, node, PropKey::Max).unwrap_or(100.0);
    let r = rects[node as usize];
    let thumb = 20.0;
    let t = if r.w > thumb { ((x - r.x - thumb / 2.0) / (r.w - thumb)).clamp(0.0, 1.0) } else { 0.0 };
    min + (max - min) * t
}

impl Input {
    pub fn new() -> Input {
        Input::default()
    }

    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    /// Feeds one touch sample; returns the events it completed.
    pub fn touch(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], t: Touch) -> Vec<Event> {
        let mut events = Vec::new();
        match t.phase {
            Phase::Down => {
                let caught = self.fling.take().is_some();
                let Some(node) = hit(page, res, rects, t.x, t.y, false) else { return events };
                let kind = page.nodes[node as usize].kind;
                if rt.has(node, crate::runtime::DISABLED) {
                    return events;
                }
                let scroll = if kind == Kind::Scroll { Some(node) } else { scroll_ancestor(page, node) };
                if kind.is_press_surface() && !caught {
                    rt.set_state(node, HELD, true);
                    if let Some(action) = event_action(page, node, "press") {
                        events.push(Event { node, kind: EventKind::Press, action: Some(action) });
                    }
                }
                if kind == Kind::Slider && !caught {
                    rt.set_state(node, HELD, true);
                    let v = slider_value(page, res, rects, node, t.x);
                    rt.set_prop(node, PropKey::Value, Value::VNum(v));
                    events.push(Event { node, kind: EventKind::Edit(Value::VNum(v)), action: event_action(page, node, "edit") });
                }
                self.press = Some(Press { node, kind, start: (t.x, t.y), scroll, panning: false, last: (t.x, t.y), last_t: t.t, velocity: [0.0; 2], caught_fling: caught });
            }
            Phase::Move => {
                let Some(mut p) = self.press else { return events };
                let dt = (t.t - p.last_t).max(1e-4) as f32;
                let delta = (t.x - p.last.0, t.y - p.last.1);
                p.velocity = [delta.0 / dt, delta.1 / dt];
                if p.kind == Kind::Slider && !p.panning {
                    let v = slider_value(page, res, rects, p.node, t.x);
                    rt.set_prop(p.node, PropKey::Value, Value::VNum(v));
                    events.push(Event { node: p.node, kind: EventKind::Edit(Value::VNum(v)), action: event_action(page, p.node, "edit") });
                } else if let Some(scroll) = p.scroll {
                    let travel = ((t.x - p.start.0).powi(2) + (t.y - p.start.1).powi(2)).sqrt();
                    if !p.panning && travel > SCROLL_DEADZONE {
                        // The Scroll takes the gesture: the child's press is cancelled.
                        p.panning = true;
                        rt.set_state(p.node, HELD, false);
                    }
                    if p.panning {
                        let (ax, ay) = scroll_axes(page, res, scroll);
                        let range = scroll_range(page, rects, scroll);
                        let s = &mut rt.scroll[scroll as usize];
                        if ax {
                            s[0] = (s[0] - delta.0).clamp(0.0, range[0]);
                        }
                        if ay {
                            s[1] = (s[1] - delta.1).clamp(0.0, range[1]);
                        }
                        rt.dirty = true;
                    }
                }
                if p.kind.is_press_surface() && !p.panning && !p.caught_fling {
                    // Slide-off clears the press, slide-back re-arms it.
                    let inside = hit(page, res, rects, t.x, t.y, false) == Some(p.node);
                    rt.set_state(p.node, HELD, inside);
                }
                p.last = (t.x, t.y);
                p.last_t = t.t;
                self.press = Some(p);
            }
            Phase::Up | Phase::Cancel => {
                let Some(p) = self.press.take() else { return events };
                rt.set_state(p.node, HELD, false);
                if p.panning {
                    if let Some(scroll) = p.scroll {
                        let v = [p.velocity[0] * FLING_BOOST, p.velocity[1] * FLING_BOOST];
                        if (v[0].powi(2) + v[1].powi(2)).sqrt() > FLING_MIN_SPEED {
                            self.fling = Some((scroll, v));
                        }
                    }
                    return events;
                }
                if t.phase == Phase::Cancel || p.caught_fling {
                    return events;
                }
                let inside = hit(page, res, rects, t.x, t.y, false) == Some(p.node);
                match p.kind {
                    Kind::Slider => {
                        let v = rt.writes.iter().find(|(n, w)| *n == p.node && w.key == PropKey::Value).map(|(_, w)| w.value).unwrap_or(Value::VNum(0.0));
                        events.push(Event { node: p.node, kind: EventKind::Change(v), action: event_action(page, p.node, "change") });
                    }
                    Kind::Button | Kind::ListItem if inside => {
                        events.push(Event { node: p.node, kind: EventKind::Activate, action: event_action(page, p.node, "activate") });
                    }
                    Kind::Check | Kind::Toggle if inside => {
                        let on = !rt.has(p.node, SELECTED);
                        rt.set_state(p.node, SELECTED, on);
                        events.push(Event { node: p.node, kind: EventKind::Activate, action: event_action(page, p.node, "activate") });
                        events.push(Event { node: p.node, kind: EventKind::Change(Value::VBool(on)), action: event_action(page, p.node, "change") });
                    }
                    Kind::Radio if inside => {
                        // One of N: siblings under the same parent clear.
                        let parent = page.nodes[p.node as usize].parent;
                        if parent != NO_NODE {
                            for c in page.children(parent) {
                                if page.nodes[c].kind == Kind::Radio {
                                    rt.set_state(c as NodeId, SELECTED, c as NodeId == p.node);
                                }
                            }
                        } else {
                            rt.set_state(p.node, SELECTED, true);
                        }
                        events.push(Event { node: p.node, kind: EventKind::Activate, action: event_action(page, p.node, "activate") });
                        events.push(Event { node: p.node, kind: EventKind::Change(Value::VIdent(page.nodes[p.node as usize].name)), action: event_action(page, parent, "change") });
                    }
                    _ => {}
                }
            }
        }
        events
    }

    /// Advances a fling by `dt` seconds; true while something still moves.
    pub fn tick(&mut self, page: &Page, rt: &mut Runtime, res: &Resolved, rects: &[Rect], dt: f32) -> bool {
        let Some((scroll, mut v)) = self.fling else { return false };
        let (ax, ay) = scroll_axes(page, res, scroll);
        let range = scroll_range(page, rects, scroll);
        let s = &mut rt.scroll[scroll as usize];
        let mut moved = false;
        if ax {
            let next = (s[0] - v[0] * dt).clamp(0.0, range[0]);
            moved |= next != s[0];
            if next == 0.0 || next == range[0] {
                v[0] = 0.0;
            }
            s[0] = next;
        }
        if ay {
            let next = (s[1] - v[1] * dt).clamp(0.0, range[1]);
            moved |= next != s[1];
            if next == 0.0 || next == range[1] {
                v[1] = 0.0;
            }
            s[1] = next;
        }
        let decay = (-FLING_FRICTION * dt).exp();
        v = [v[0] * decay, v[1] * decay];
        if moved {
            rt.dirty = true;
        }
        if (v[0].powi(2) + v[1].powi(2)).sqrt() < FLING_MIN_SPEED {
            self.fling = None;
            return moved;
        }
        self.fling = Some((scroll, v));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;
    use crate::layout::{FixedAdvance, layout};
    use crate::runtime::resolve;

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
            let (res, rects) = self.lay();
            self.input.touch(&self.page, &mut self.rt, &res, &rects, Touch { phase, x, y, t })
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
        let (res, rects) = fx.lay();
        let before = fx.rt.scroll[s as usize][1];
        assert!(fx.input.tick(&fx.page, &mut fx.rt, &res, &rects, 0.016));
        assert!(fx.rt.scroll[s as usize][1] > before, "the glide keeps going");
        // Content is 400 tall in a 200 viewport: never past 200.
        for _ in 0..200 {
            let (res, rects) = fx.lay();
            fx.input.tick(&fx.page, &mut fx.rt, &res, &rects, 0.016);
        }
        assert!(fx.rt.scroll[s as usize][1] <= 200.0);
        // Touching a gliding list stops it without pressing what is under the finger.
        fx.input.fling = Some((s, [0.0, -500.0]));
        fx.touch(Phase::Down, 50.0, 50.0, 2.0);
        assert!(fx.input.fling.is_none());
        assert!(fx.touch(Phase::Up, 50.0, 50.0, 2.1).is_empty());
    }

    #[test]
    fn slider_edits_then_changes_and_toggle_flips() {
        let mut fx = Fx::new("View v { Column c { Slider s min=0, max=100, value=50, width=220, on:change=size_changed; Toggle t text=\"x\" } }");
        let t = fx.page.find("t").unwrap();
        let ev = fx.touch(Phase::Down, 219.0, 16.0, 0.0);
        assert!(matches!(ev[0].kind, EventKind::Edit(Value::VNum(v)) if v > 99.0), "{ev:?}");
        fx.touch(Phase::Move, 10.0, 16.0, 0.1);
        let ev = fx.touch(Phase::Up, 10.0, 16.0, 0.2);
        assert!(matches!(ev[0].kind, EventKind::Change(Value::VNum(v)) if v == 0.0), "{ev:?}");
        assert!(matches!(ev[0].action, Some(Action::Named(_))));
        let (_, rects) = fx.lay();
        let tr = rects[t as usize];
        fx.touch(Phase::Down, tr.x + 5.0, tr.y + 5.0, 1.0);
        let ev = fx.touch(Phase::Up, tr.x + 5.0, tr.y + 5.0, 1.1);
        assert!(fx.rt.has(t, SELECTED));
        assert!(ev.iter().any(|e| e.kind == EventKind::Change(Value::VBool(true))));
    }
}
