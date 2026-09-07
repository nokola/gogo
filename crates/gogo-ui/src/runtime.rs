//! The runtime part of a page: which states hold, the active arrangement
//! and theme variant, scroll positions. `resolve` overlays the active
//! `when` responses onto the base props in the spec's priority order; the
//! layout and draw passes read only the `Resolved` result.

use std::sync::Arc;

use crate::anim::{Channel, Easing, Finish, Motion, Player, Tween, get_pval, lerp_value, prop_lane};
use crate::layout::{BUTTON_PADDING, Rect};
use crate::locale::{KeyName, Locale};
use crate::page::*;

pub const HELD: u8 = 1;

/// How far a `tap_pulse` turnstile swings (degrees about the left edge).
/// 25°: cos = 0.906, about the shipped scale pulse's 0.9 in lost width, so
/// the feedback reads as the same weight of motion.
pub const TAP_PULSE_ANGLE_DEG: f32 = 25.0;
/// The swing out — the shipped `Fx.TapPulse` first track (0.05 s, quad out).
pub const TAP_PULSE_OUT_SEC: f32 = 0.05;
/// The swing back — the shipped second track (0.12 s, back out 0.5).
pub const TAP_PULSE_BACK_SEC: f32 = 0.12;

/// One `tap_pulse` swing in flight on a node.
#[derive(Clone, Copy, Debug)]
struct Pulse {
    node: NodeId,
    elapsed: f32,
}

impl Pulse {
    /// The swing angle at `elapsed`: out on quad-out, back on back-out
    /// (amplitude 0.5 — the shipped `new BackEase(0.5)`; its small
    /// overshoot past flat reads as a hair of swing the other way, the
    /// door settling).
    fn angle(&self) -> f32 {
        if self.elapsed < TAP_PULSE_OUT_SEC {
            let t: f32 = self.elapsed / TAP_PULSE_OUT_SEC;
            let e = Easing { curve: crate::anim::Curve::Quad, mode: crate::anim::Mode::Out, params: [None, None] };
            TAP_PULSE_ANGLE_DEG * e.apply(t)
        } else {
            let t: f32 = ((self.elapsed - TAP_PULSE_OUT_SEC) / TAP_PULSE_BACK_SEC).clamp(0.0, 1.0);
            let e = Easing { curve: crate::anim::Curve::Back, mode: crate::anim::Mode::Out, params: [Some(0.5), None] };
            TAP_PULSE_ANGLE_DEG * (1.0 - e.apply(t))
        }
    }
}
pub const HOVERED: u8 = 2;
pub const FOCUSED: u8 = 4;
pub const FOCUS_WITHIN: u8 = 8;
pub const SELECTED: u8 = 16;
pub const DISABLED: u8 = 32;

/// `base` — no arrangement active.
pub use crate::page::{ArrangementId, BASE};

/// A page build's typed arrangement handle (`enum Arrangement` in the
/// generated module) — `Runtime::go`/`toggle` take one, so a misspelled
/// arrangement is a rustc error (GPAR27, the E7 typed-handles design;
/// `Key`/`KeyName` is the translation-key half).
pub trait ArrangementName: Copy {
    fn id(self) -> ArrangementId;
}

/// A page build's typed anim handle (`enum Anim` in the generated module,
/// one variant per `anim` the file declares) — `Runtime::play` / `stop` /
/// `is_playing` take one, so a misspelled anim is a rustc error and no
/// code-behind interns an anim name per frame. The `*_id` forms take the
/// anim's `StrId` for a runtime-compiled page (tests, hot reload).
pub trait AnimName: Copy {
    fn id(self) -> StrId;
}

/// One field value a repeat item carries — the leaf types the shipped
/// addon binds (gogo_repeat.gd: String, number, bool, Color; structures
/// don't bind whole).
#[derive(Clone, Debug, PartialEq)]
pub enum ItemValue {
    Str(String),
    Int(i64),
    Num(f32),
    Bool(bool),
    Color(Color_Straight_Srgb_4xU8_RGBA),
}

/// One row of a keyed repeat's data: a record (field → value), or a
/// plain value where `by $item` makes the item its own key.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Value(ItemValue),
    Record(Vec<(String, ItemValue)>),
}

/// A row's identity across `set_items` calls — a string or a whole
/// number, never a float (float equality is no identity; the shipped
/// path refuses float keys too).
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKey {
    Str(String),
    Int(i64),
}

/// One repeat's live rows: slots cloned from the template (slot index
/// = stable identity; a slot's nodes are named `<repeat>/<index>/…`),
/// the display order, and — keyed form — each slot's key with a frozen
/// copy of its data (same key + same data next time = the slot is kept
/// untouched, the shipped `_data_equal` diff). Freed slots are REUSED,
/// never abandoned, so memory and per-frame cost stay at the high-water
/// row count (`capacity=`, user design 2026-08-31).
#[derive(Clone, Debug)]
struct Pool {
    repeat: NodeId,
    /// Every slot ever cloned, in slot order.
    slots: Vec<NodeId>,
    /// Slot indices shown right now, in display order.
    live: Vec<u32>,
    /// Per slot: the keyed row it holds (`None` = counted form, or free).
    keyed: Vec<Option<(ItemKey, Item)>>,
    /// The next `live` is built here and swapped in — the buffer keeps
    /// its capacity, so a steady `set_items` allocates nothing.
    scratch: Vec<u32>,
    /// The repeat's `empty { }` children (found once — the per-call scan
    /// would allocate).
    empties: Vec<NodeId>,
}

/// A repeat's pool, created on first touch.
fn new_pool(page: &Page, repeat: NodeId) -> Pool {
    let mut empties: Vec<NodeId> = page.children(repeat).filter(|c| page.nodes[*c].kind == Kind::Empty).collect();
    // A List-level `empty { }` (D128) is this repeat's zero-items view
    // too: it hides once rows exist — and its spacing slot goes with it —
    // exactly like the repeat's own block (GPAR96).
    let mut a = page.nodes[repeat].parent;
    while a != NO_NODE {
        if page.nodes[a].kind == Kind::List {
            empties.extend(page.children(a).filter(|c| page.nodes[*c].kind == Kind::Empty));
            break;
        }
        a = page.nodes[a].parent;
    }
    Pool { repeat, slots: Vec::new(), live: Vec::new(), keyed: Vec::new(), scratch: Vec::new(), empties }
}

/// The key leaf of `item` (validated by `set_items` before use).
fn key_leaf<'a>(page: &'a Page, by: StrId, item: &'a Item) -> &'a ItemValue {
    match item {
        Item::Value(v) => v,
        Item::Record(r) => r.iter().find(|(k, _)| *k == page.str(by)).map(|(_, v)| v).expect("validated by set_items"),
    }
}

/// Does a stored key name the same row as this item's key leaf?
fn key_matches(k: &ItemKey, leaf: &ItemValue) -> bool {
    match (k, leaf) {
        (ItemKey::Str(a), ItemValue::Str(b)) => a == b,
        (ItemKey::Int(a), ItemValue::Int(b)) => a == b,
        _ => false,
    }
}

/// Writes `src` over `dst` reusing `dst`'s buffers — a rebind must not
/// allocate for data of the same shape (the zero-alloc churn contract).
fn assign_item(dst: &mut Item, src: &Item) {
    match (dst, src) {
        (Item::Record(d), Item::Record(s)) if d.len() == s.len() => {
            for ((dk, dv), (sk, sv)) in d.iter_mut().zip(s) {
                if dk != sk {
                    dk.clear();
                    dk.push_str(sk);
                }
                assign_value(dv, sv);
            }
        }
        (d, s) => *d = s.clone(),
    }
}

fn assign_value(d: &mut ItemValue, s: &ItemValue) {
    match (d, s) {
        (ItemValue::Str(d), ItemValue::Str(s)) => {
            d.clear();
            d.push_str(s);
        }
        (d, s) => *d = s.clone(),
    }
}

/// `assign_item`'s twin for the stored key.
fn assign_key(k: &mut ItemKey, leaf: &ItemValue) {
    match (k, leaf) {
        (ItemKey::Str(d), ItemValue::Str(s)) => {
            d.clear();
            d.push_str(s);
        }
        (ItemKey::Int(d), ItemValue::Int(s)) => *d = *s,
        (k, ItemValue::Str(s)) => *k = ItemKey::Str(s.clone()),
        (k, ItemValue::Int(s)) => *k = ItemKey::Int(*s),
        _ => unreachable!("set_items validated the key type"),
    }
}

#[derive(Clone, Debug)]
/// One automatic slot's fraction glide (GP33, the addon's GogoDriver):
/// a value flip travels in FRACTION space by the control's default feel;
/// landing writes the rest truth (disarm HOLDS — the driven layout keeps
/// its last value, because it is truth, not displacement).
struct AutoGlide {
    node: NodeId,
    from: f32,
    to: f32,
    elapsed: f32,
    duration: f32,
    easing: Easing,
    /// The eased value this frame — what layout shows mid-flight.
    shown: f32,
}

pub struct Runtime {
    /// The pose in force (`BASE` = the rest pose); ask `arrangement_is`.
    pub arrangement: ArrangementId,
    pub variant: StrId,
    /// Active `screen.<class>` words.
    pub screen: Vec<StrId>,
    /// Per node: the state bits above.
    pub states: Vec<u8>,
    /// Per node: scroll offset (Scroll nodes).
    pub scroll: Vec<[f32; 2]>,
    /// Set by anything that changes what layout/draw would produce.
    pub dirty: bool,
    /// The language every `text="x"/key` on this page shows (`set_locale`);
    /// `None` = the literals as written. Shared between pages — one table,
    /// swapped whole, so a switch is one relayout and never a half state.
    pub locale: Option<Arc<Locale>>,
    /// Safe-area insets in epx (left, top, right, bottom) for `padding=safe`.
    pub safe: [f32; 4],
    /// Props written from code (`set_prop`); win over everything.
    pub writes: Vec<(NodeId, Prop)>,
    /// State changes since the last `resolve_animated`: (node or NO_NODE
    /// for the view, from, to) — what `transition` rules match against.
    pub changes: Vec<(NodeId, Option<State>, Option<State>)>,
    /// Time ticked since a change was recorded and before its resolve — a
    /// tween born at that resolve starts this far in, so a flip that
    /// happened between frames animates from the FLIP, not the resolve.
    pending_change_dt: f32,
    /// The same flip-time credit for rect glides: `glide` seeds its rect
    /// tweens this far in (set beside `glide_start`).
    glide_late: f32,
    pub tweens: Vec<Tween>,
    pub players: Vec<Player>,
    /// The ANIM LAYER: per node, what players wrote. Only the channels a
    /// player owns (or `held` keeps) reach the picture — `resolve`
    /// composes them over the `when` ladder.
    pub motion: Vec<Motion>,
    /// Finished anims' end poses still standing (WPF HoldEnd): (node,
    /// channel, value). A held value stays on top of the ladder until the
    /// next writer of that property — a `when` flip that writes it, a code
    /// write, another anim — or `stop(restore)`.
    pub held: Vec<(NodeId, Channel, f32)>,
    /// The prop-lane ANIM LAYER (GP40 color/non-motion lanes): per
    /// (node, key), what players wrote. Same law as `motion` — only
    /// entries a player owns (or `held_props` keeps) reach the picture.
    pub prop_anim: Vec<(NodeId, PropKey, Value)>,
    /// Finished prop lanes' end poses still standing (WPF HoldEnd) — the
    /// prop half of `held`. An alpha-only lane holds a `VNum`.
    pub held_props: Vec<(NodeId, PropKey, Value)>,
    /// Roots whose `live($value)` labels are owed a rewrite: the input
    /// machine writes values holding only `&Page` (`set_value_deferred`),
    /// so the label follow waits for `tick`, the frame door that has
    /// `&mut Page`. One frame late at most — a drag streams anyway.
    pub live_follow: Vec<NodeId>,
    /// Live repeat rows, one pool per touched repeat (`set_count` /
    /// `set_items`).
    pools: Vec<Pool>,
    /// How many arrangements the page declares — `go_id`'s refusal
    /// bound (an unknown index keeps the pose, GPAR27).
    arrangement_count: usize,
    /// Raised control states: (control root, declared state name) —
    /// `set_custom` writes, `when <name>` conditions read (GPAR116).
    customs: Vec<(NodeId, StrId)>,
    /// Each List's selection, the keys in the order they entered
    /// (repeat/List spec §5; `one` holds at most one). Kept BY KEY so a
    /// `set_items` reconcile re-finds the rows; a key with no present
    /// item is INERT — it renders nothing and reads back until
    /// reassigned or the item arrives (select-then-load).
    list_sel: Vec<(NodeId, Vec<ItemKey>)>,
    /// Per node: the drawn/not-drawn state the visibility triggers last
    /// answered (GP19). Needed so `on:show`/`on:hide` fire on a real flip
    /// only — the first resolve baselines silently (initial visibility is
    /// state, not a trigger — the shipped armed gate), and nodes appended
    /// later (repeat clones) baseline the same way.
    vis_seen: Vec<bool>,
    /// The focused Input's caret (node, chars) — written by the input
    /// machine, drawn by the renderer (GPAR112).
    pub caret: Option<(NodeId, u32)>,
    /// The focused Input's selected span (node, lo, hi in chars) — the
    /// drawer paints the highlight under the glyphs from it.
    pub selection: Option<(NodeId, u32, u32)>,
    /// The selection was born of TOUCH (long-press, the bar's All):
    /// the drawer hangs the two drag circles below it. A mouse or
    /// keyboard selection shows none — those have drag-select and
    /// Shift+arrows.
    pub sel_handles: bool,
    /// The field whose edit bar (Cut/Copy/Paste/All) is up — raised by a
    /// long-press, dropped by blur or any press off the bar.
    pub edit_bar: Option<NodeId>,
    /// `transition … anim=<name>` players in flight or held (GPAR109):
    /// who flipped, the condition entered, the anim, and what it displaced
    /// at play start — the condition's end restores the displaced
    /// channels (gogo_states.gd `_cancel_state_anim`).
    state_anims: Vec<(NodeId, State, StrId, Vec<(NodeId, Channel, f32, bool)>, Vec<(NodeId, PropKey, Value, bool)>)>,
    /// Anims that ended on their own since the last `take_finished`
    /// (one entry per player that ran out). Needed because a host
    /// sequencing on an anim's end (Godot's `finished`) otherwise polls
    /// `is_playing` every frame and misses a finish that re-plays the
    /// same frame. `stop()` never reports (WPF Storyboard.Stop skips
    /// Completed).
    finished: Vec<StrId>,
    /// Per node, what SHOWED after the last `resolve_animated`: the
    /// composed motion, the node's own opacity and the fill's alpha — the
    /// live value a new play starts from.
    pub shown: Vec<Motion>,
    /// A timed arrangement flip started this frame: every box whose laid-out
    /// rectangle changes glides there over (duration, easing) — alignment,
    /// fill, hug, cap and seat moves all tween through their final rects.
    pub glide_start: Option<(f32, Easing)>,
    /// Per-node pose-pair timing for the CURRENT flip: a node-scope
    /// `transition <pose> -> <pose>` rule times that node's share of the
    /// rect glide, duration 0 = snap; everyone else takes the view rule
    /// (GPAR72, the shipped _node_pose_timing).
    pub glide_node_rules: Vec<(NodeId, f32, Easing)>,
    /// Rect tweens in flight: (node, from, to, elapsed, duration, easing).
    pub rect_tweens: Vec<(NodeId, Rect, Rect, f32, f32, Easing)>,
    /// The rects shown last frame (the `from` of the next glide).
    pub last_rects: Vec<Rect>,
    /// The `tap_pulse` swings in flight (`tap_pulse`), one per node — a
    /// second tap restarts the node's swing, Storyboard-style.
    pulses: Vec<Pulse>,
    /// Paint-order bumps among siblings (the Canvas.ZIndex port): (node,
    /// raise) — higher paints later (on top), lower earlier (below);
    /// absent = 0, the file's order. Layout never moves.
    pub raised: Vec<(NodeId, i32)>,
    /// GP33: each value-driven automatic slot's fraction at REST (the
    /// last landed or snapped value; glides start from it).
    auto_rest: Vec<(NodeId, f32)>,
    auto_glides: Vec<AutoGlide>,
    /// Per node: a Radio's `choice` group (`NO_CHOICE` elsewhere) — a
    /// code write of `selected` clears the group like a tap (GP37).
    radio_choice: Vec<u32>,
    /// Per node: is it a Toggle — its `selected` and value derive from
    /// each other on code writes (GP37).
    toggle: Vec<bool>,
    /// Arrangements flipped by `go_instant` whose changes have not yet
    /// resolved — their flip arms no tweens, glides or rule anims.
    instant_flips: Vec<ArrangementId>,
    /// The flip awaiting `arrangement_settled` (the shipped signal).
    pending_settle: Option<ArrangementId>,
    /// Per node: the string slot a code `set_text` owns (`NO_STR` = none
    /// yet). Needed so a label that changes every few frames (a stats
    /// line) rewrites ONE string in place instead of interning a new one
    /// each time — `Page::strings` would grow without bound and every
    /// `intern` scans it.
    pub owned_text: Vec<StrId>,
    /// The displayed value per (node, key) after the last resolve — the
    /// `from` of the next tween.
    last: Vec<(NodeId, Prop)>,
}

impl Runtime {
    pub fn new(page: &Page) -> Runtime {
        Runtime {
            arrangement: BASE,
            variant: NO_STR,
            screen: Vec::new(),
            // The file's start states (`selected=true`, `disabled=true`, a
            // `choice`'s `value=`).
            states: page.nodes.iter().map(|n| n.initial_states).collect(),
            scroll: vec![[0.0; 2]; page.nodes.len()],
            // GP33: each driven slot's fraction at rest starts at the
            // baked value; drivers move it.
            auto_rest: page.drivers.iter().filter(|d| d.state == NO_STR).map(|d| (d.node, baked_frac(page, d.node))).collect(),
            auto_glides: Vec::new(),
            radio_choice: page.nodes.iter().map(|n| if n.kind == Kind::Radio { n.choice } else { NO_CHOICE }).collect(),
            toggle: page.nodes.iter().map(|n| n.kind == Kind::Toggle).collect(),
            instant_flips: Vec::new(),
            pending_settle: None,
            dirty: true,
            locale: None,
            safe: [0.0; 4],
            writes: Vec::new(),
            changes: Vec::new(),
            pending_change_dt: 0.0,
            glide_late: 0.0,
            tweens: Vec::new(),
            held: Vec::new(),
            prop_anim: Vec::new(),
            held_props: Vec::new(),
            live_follow: Vec::new(),
            shown: vec![Motion::IDENTITY; page.nodes.len()],
            players: Vec::new(),
            state_anims: Vec::new(),
            finished: Vec::new(),
            pools: Vec::new(),
            arrangement_count: page.arrangements.len(),
            customs: Vec::new(),
            list_sel: Vec::new(),
            vis_seen: Vec::new(),
            caret: None,
            selection: None,
            sel_handles: false,
            edit_bar: None,
            motion: vec![Motion::IDENTITY; page.nodes.len()],
            owned_text: vec![NO_STR; page.nodes.len()],
            last: Vec::new(),
            glide_start: None,
            glide_node_rules: Vec::new(),
            rect_tweens: Vec::new(),
            last_rects: Vec::new(),
            raised: Vec::new(),
            pulses: Vec::new(),
        }
    }

    /// Something is moving: keep redrawing.
    pub fn animating(&self) -> bool {
        !self.tweens.is_empty() || !self.players.is_empty() || !self.rect_tweens.is_empty() || !self.auto_glides.is_empty() || !self.pulses.is_empty()
    }

    /// Starts a press surface's `tap_pulse` feedback: the WP turnstile
    /// swing (user call 2026-09-05 — the name stays `tap_pulse`, the
    /// motion is a turnstile, not the shipped scale pulse). The box swings
    /// about its LEFT edge like a door: out to `TAP_PULSE_ANGLE_DEG` over
    /// `TAP_PULSE_OUT_SEC` (quad out), back to flat over `TAP_PULSE_BACK_SEC`
    /// (back out, amplitude 0.5) — the shipped `Fx.TapPulse` timings and
    /// easings on the angle instead of a uniform scale. Drawn in 3D
    /// (user call 2026-09-06: WP7's turnstile has perspective): the angle
    /// rides the `swing` motion channel and the draw walk wraps the node
    /// in a `SwingPush` — the far side of the door shrinks toward the
    /// center as it turns. One swing per node: a tap mid-swing restarts
    /// it (shipped: a new play kills the old one). The input layer calls
    /// this on `activate` for every node whose `tap_pulse=true` (the
    /// shipped `button.Pressed` hook).
    pub fn tap_pulse(&mut self, node: NodeId) {
        self.pulses.retain(|p| p.node != node);
        self.pulses.push(Pulse { node, elapsed: 0.0 });
        self.dirty = true;
    }

    /// The `tap_pulse` swing angle on `node` right now, degrees about its
    /// left edge; 0 when no swing is live (a finished swing is disarmed —
    /// the D10 law: a node has a transform only WHILE it animates).
    pub fn tap_pulse_angle(&self, node: NodeId) -> f32 {
        self.pulses.iter().find(|p| p.node == node).map(|p| p.angle()).unwrap_or(0.0)
    }

    /// The rects to SHOW this frame: the laid-out ones, except boxes still
    /// gliding from a timed arrangement flip, which sit between where they
    /// were and where the layout put them. Call once per layout, after
    /// `tick`. A flip mid-glide retargets from the shown rect.
    pub fn glide(&mut self, rects: Vec<Rect>) -> Vec<Rect> {
        let mut shown = rects.clone();
        if let Some((duration, easing)) = self.glide_start.take() {
            if self.last_rects.len() == rects.len() {
                for i in 0..rects.len() {
                    if self.last_rects[i] != rects[i] {
                        // A node that had NO box (gone last frame, or gone
                        // now) snaps: a gone node's rect is the zero rect
                        // at the origin, and gliding from it inflates the
                        // node out of the screen's top-left corner (the
                        // brush page's color wheel, todo-bugs.md #2).
                        // Shipped tweened only placement-posed nodes from
                        // their live offsets, so an appearing plate never
                        // moved there either.
                        if self.last_rects[i] == Rect::ZERO || rects[i] == Rect::ZERO {
                            self.rect_tweens.retain(|t| t.0 != NodeId::from_index(i));
                            continue;
                        }
                        // The node's own pose-pair rule times its share;
                        // duration 0 snaps (GPAR72).
                        let (d, e) = self.glide_node_rules.iter().find(|(n, _, _)| *n == NodeId::from_index(i)).map(|(_, d, e)| (*d, *e)).unwrap_or((duration, easing));
                        self.rect_tweens.retain(|t| t.0 != NodeId::from_index(i));
                        if d > 0.0 {
                            self.rect_tweens.push((NodeId::from_index(i), self.last_rects[i], rects[i], self.glide_late.min(d), d, e));
                        }
                    }
                }
            }
        }
        for (node, from, to, elapsed, duration, easing) in &mut self.rect_tweens {
            // A relayout (a resize, a text change) moves the target with it.
            *to = rects[*node];
            let t = easing.apply((*elapsed / *duration).min(1.0));
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            shown[*node] = Rect::new(lerp(from.x, to.x), lerp(from.y, to.y), lerp(from.w, to.w), lerp(from.h, to.h));
        }
        self.glide_late = 0.0;
        self.rect_tweens.retain(|t| t.3 < t.4);
        self.last_rects = shown.clone();
        // A `%` anim endpoint on a node that had no box at play (hidden
        // then) reads its fraction now that the node laid out.
        for p in &mut self.players {
            if p.resolve_pending_percent(&self.last_rects, &mut self.motion) {
                self.dirty = true;
            }
        }
        shown
    }

    /// Plays `anim` on this page (`play(name)`); a newer player owns the
    /// channels it animates (WPF SnapshotAndReplace).
    pub fn play<A: AnimName>(&mut self, page: &Page, anim: A, finish: Finish) {
        self.play_id(page, anim.id(), finish);
    }

    /// `play` by the anim's string id, for a runtime-compiled page.
    pub fn play_id(&mut self, page: &Page, anim: StrId, finish: Finish) {
        // What each channel shows right now: a held end pose, else the
        // layer of the player that owns it, else what the last resolve
        // showed (the ladder). Computed here so a play needs no frame in
        // between to know where it starts.
        let mut live: Vec<Motion> = self.shown.clone();
        live.resize(page.nodes.len(), Motion::IDENTITY);
        for p in self.players.iter().filter(|p| !p.stopped) {
            for (node, channel) in &p.owns {
                live[*node].set(*channel, self.motion[*node].get(*channel));
            }
        }
        for (node, channel, v) in &self.held {
            live[*node].set(*channel, *v);
        }
        // GP40 prop lanes: what each lane SHOWS right now. One resolve
        // answers it all (`compose_anim` already lays held poses and
        // owned lanes over the ladder), and it runs only when this anim
        // has prop lanes at all.
        let mut plive: Vec<(NodeId, PropKey, Value)> = Vec::new();
        let lanes: Vec<(NodeId, PropKey, bool)> = page.anims.iter().filter(|t| t.anim == anim).filter_map(|t| { let w = page.str(t.channel); if Channel::parse(w).is_some() { None } else { prop_lane(w).map(|(k, a)| (t.node, k, a)) } }).collect();
        if !lanes.is_empty() {
            let r = resolve(page, self);
            for (node, key, alpha) in lanes {
                if plive.iter().any(|(n, k, _)| *n == node && *k == key) {
                    continue;
                }
                let v = if alpha {
                    Value::VNum(r.color(page, node, key).map(|c| c.0[3] as f32 / 255.0).unwrap_or(1.0))
                } else if key.is_color() {
                    Value::VColor(r.color(page, node, key).unwrap_or(Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT))
                } else {
                    r.prop(page, node, key).unwrap_or(Value::VNum(0.0))
                };
                plive.push((node, key, v));
            }
        }
        let mut player = Player::new(page, anim, &mut self.motion, &live, &self.held, &mut self.prop_anim, &plive, &self.held_props, self.variant, finish, &self.last_rects);
        // The channels move to the new player; the older ones keep their
        // clocks (a deferred hide still hides on time) and hand over the
        // true base, so `stop(restore)` lands where the FIRST play found
        // the channel. A held end pose on the channel is the new play's
        // base (a standing value the author chose) and is no longer held.
        for (node, channel) in player.owns.clone() {
            for p in &mut self.players {
                if let Some(base) = p.hand_over(node, channel) {
                    player.inherit_base(node, channel, base);
                }
            }
            self.held.retain(|h| !(h.0 == node && h.1 == channel));
        }
        for (node, key) in player.owns_props.clone() {
            for p in &mut self.players {
                if let Some(base) = p.hand_over_prop(node, key) {
                    player.inherit_base_prop(node, key, base);
                }
            }
            self.held_props.retain(|h| !(h.0 == node && h.1 == key));
        }
        // Anim beats transition (GP40, user call): a tween flying on a
        // lane this play now owns is cut — two writers would fight.
        self.tweens.retain(|t| !player.owns_props.contains(&(t.node, t.key)));
        self.players.push(player);
        self.dirty = true;
    }

    fn play_action(&mut self, page: &Page, node: NodeId, event: &str, finish: Finish) -> bool {
        let n = &page.nodes[node];
        let id = page.intern_lookup(event);
        let action = page.handlers[n.handlers.0 as usize..(n.handlers.0 + n.handlers.1) as usize].iter().find(|h| h.event == id).map(|h| h.action);
        if let Some(Action::Play(anim)) = action {
            self.play_id(page, anim, finish);
            return true;
        }
        false
    }

    /// Shows or hides with the node's `on:show` / `on:hide` anims: a hide
    /// plays first and hides on finish (deferred hide); showing during a
    /// hide cancels it.
    pub fn show(&mut self, page: &mut Page, node: NodeId, visible: bool) {
        // Entrances play on a REAL flip only (the shipped
        // visibility_changed gate, GPAR53): an overlay calling
        // `show(x, true)` every frame settles instead of restarting. A
        // show mid-hide still cancels the hide below.
        let hiding = self.players.iter().any(|p| p.finish == Finish::Hide(node) && !p.stopped);
        if !hiding && self.visible_now(page, node) == visible {
            return;
        }
        if visible {
            self.players.retain(|p| p.finish != Finish::Hide(node) && p.finish != Finish::HideWhen(node));
            self.set_visible(page, node, true);
            self.play_action(page, node, "show", Finish::Nothing);
        } else if !self.play_action(page, node, "hide", Finish::Hide(node)) {
            self.set_visible(page, node, false);
        }
        // This flip is answered — the `when`-flip watcher in
        // `resolve_animated` (GP19) must not play the anims a second time.
        if let Some(seen) = self.vis_seen.get_mut(node.index()) {
            *seen = visible;
        }
    }

    /// What the node's visibility says right now: the last code write,
    /// else the file's word. Needed so `show` can tell a real flip from a
    /// repeat call; `when`-driven visibility is not consulted — `show` is
    /// the code-side door and gates only its own writes.
    fn visible_now(&self, page: &Page, node: NodeId) -> bool {
        let word = self.writes.iter().rev().find(|(n, p)| *n == node && p.key == PropKey::Visibility).map(|(_, p)| p.value).or_else(|| page.prop(node, PropKey::Visibility));
        match word {
            Some(Value::VIdent(w)) => {
                let s = page.str(w);
                s != "gone" && s != "hidden"
            }
            _ => true,
        }
    }

    /// Plays every visible node's `on:show` once (the entrance on first
    /// build). A node the page does not DRAW at boot — `gone`,
    /// `space_only`/`hidden`, or a `when` posing either in the boot state,
    /// on it or an ancestor — gets none: its anim would burn off-screen
    /// (GPAR56).
    pub fn play_entrances(&mut self, page: &Page) {
        let res = resolve(page, self);
        let unseen: Vec<bool> = (0..page.nodes.len()).map(|i| {
            matches!(res.ident(page, NodeId::from_index(i), PropKey::Visibility), Some("gone") | Some("hidden") | Some("space_only"))
        }).collect();
        drop(res);
        for i in 0..page.nodes.len() {
            let mut n = NodeId::from_index(i);
            let mut hidden = false;
            loop {
                if unseen[n] {
                    hidden = true;
                    break;
                }
                let parent = page.nodes[n].parent;
                if parent == NO_NODE {
                    break;
                }
                n = parent;
            }
            if hidden {
                continue;
            }
            self.play_action(page, NodeId::from_index(i), "show", Finish::Nothing);
        }
    }

    /// A repeat's live rows, in display order (its `empty { }` child is
    /// not one). Rows exist once `set_count`/`set_items`/`reserve` built
    /// them — slots cloned from the hidden template, reused ever after.
    pub fn items(&self, page: &Page, repeat: NodeId) -> Vec<NodeId> {
        let _ = page;
        self.pools.iter().find(|p| p.repeat == repeat).map(|p| p.live.iter().map(|s| p.slots[*s as usize]).collect()).unwrap_or_default()
    }

    /// The keyed-repeat row `node` sits in, if any — the key the shipped
    /// addon binds as a handler's trailing argument; a Rust host reads
    /// it off an event's node instead (`rt.item_key(&page, ev.node)`).
    pub fn item_key(&self, page: &Page, node: NodeId) -> Option<&ItemKey> {
        for p in &self.pools {
            for s in &p.live {
                let root = p.slots[*s as usize];
                if node >= root && node < page.subtree_end(root) {
                    return p.keyed[*s as usize].as_ref().map(|(k, _)| k);
                }
            }
        }
        None
    }

    /// Sizes every per-node table to the arena — the arena grows when
    /// `set_items` clones a template; ids never re-mean, so existing
    /// entries stay put.
    fn grow(&mut self, page: &Page) {
        let n = page.nodes.len();
        while self.states.len() < n {
            self.states.push(page.nodes[self.states.len()].initial_states);
        }
        self.scroll.resize(n, [0.0; 2]);
        self.motion.resize(n, Motion::IDENTITY);
        self.shown.resize(n, Motion::IDENTITY);
        self.owned_text.resize(n, NO_STR);
    }

    /// Hands a keyed repeat (`repeat x by $item.<field>`) its rows,
    /// reconciled by key like the shipped addon (gogo_repeat.gd
    /// `assign`): a kept key with the same data keeps its slot
    /// untouched, changed data re-binds a slot in place, absent keys
    /// free their slot for reuse, order follows `items`. Bad data
    /// refuses the WHOLE assignment (`Err` carrying the G-code) and the
    /// shown rows do not change. The `empty { }` content shows exactly
    /// at zero rows. A steady call allocates nothing; the pool grows
    /// past `capacity=` only when the data does.
    pub fn set_items(&mut self, page: &mut Page, repeat: NodeId, items: &[Item]) -> Result<(), String> {
        let Some(info) = page.repeats.iter().find(|r| r.node == repeat).copied() else {
            return Err(format!("G9101: `{}` is not a repeat with a template", page.str(page.nodes[repeat].name)));
        };
        if !info.keyed {
            return Err(format!("G9101: `{}` is not a keyed repeat — a counted repeat takes `set_count`", page.str(page.nodes[repeat].name)));
        }
        // Keys first; every refusal returns before anything mutates, so
        // the last good rows stand.
        let leaf = |i: usize| -> Result<&ItemValue, String> {
            match (&items[i], info.by) {
                (Item::Value(v), by) if by == NO_STR => Ok(v),
                (Item::Record(_), by) if by == NO_STR => Err(format!("G9102: repeat `{}` refused items — `by $item` keys the item itself; a record keys by field (`by $item.<field>`)", page.str(page.nodes[repeat].name))),
                (Item::Record(r), by) => match r.iter().find(|(k, _)| *k == page.str(by)) {
                    Some((_, v)) => Ok(v),
                    None => Err(format!("G9102: repeat `{}` refused items — item {i} has no `{}` field", page.str(page.nodes[repeat].name), page.str(by))),
                },
                (Item::Value(_), by) => Err(format!("G9102: repeat `{}` refused items — `by` walks `{}` into a plain value; field access needs a record", page.str(page.nodes[repeat].name), page.str(by))),
            }
        };
        for i in 0..items.len() {
            let v = leaf(i)?;
            match v {
                ItemValue::Str(_) | ItemValue::Int(_) => {}
                ItemValue::Num(_) => return Err(format!("G9102: repeat `{}` refused items — item {i}'s key is a float; keys are strings or whole numbers", page.str(page.nodes[repeat].name))),
                _ => return Err(format!("G9102: repeat `{}` refused items — item {i}'s key must be a string or a whole number", page.str(page.nodes[repeat].name))),
            }
            if i > 0 && std::mem::discriminant(leaf(0)?) != std::mem::discriminant(v) {
                return Err(format!("G9102: repeat `{}` refused items — mixed key types (strings and whole numbers)", page.str(page.nodes[repeat].name)));
            }
            for j in 0..i {
                if leaf(j)? == v {
                    return Err(format!("G9103: repeat `{}` refused items — duplicate key {v:?} (items {j} and {i}); every row keys uniquely", page.str(page.nodes[repeat].name)));
                }
            }
        }
        // Every bound field must resolve on every item — the shipped
        // detached build: one failed item refuses the assignment whole.
        let (s0, sc) = info.sites;
        for (i, item) in items.iter().enumerate() {
            for si in s0..s0 + sc {
                let fname = page.str(page.bind_sites[si as usize].field);
                let ok = match item {
                    Item::Record(r) => !fname.is_empty() && r.iter().any(|(k, _)| *k == fname),
                    Item::Value(_) => fname.is_empty(),
                };
                if !ok {
                    let why = if fname.is_empty() { "structures don't bind whole — bind a field (`$item.<field>`)".to_string() } else { format!("no field `{fname}`") };
                    return Err(format!("G9101: repeat `{}` refused items — the item template failed for item {i}: {why}", page.str(page.nodes[repeat].name)));
                }
            }
        }
        // Commit — cannot fail. Kept keys with kept data keep their slot.
        let pi = self.pool_of(page, repeat);
        let mut next = std::mem::take(&mut self.pools[pi].scratch);
        next.clear();
        for item in items {
            let lv = key_leaf(page, info.by, item);
            let kept = (0..self.pools[pi].slots.len() as u32).find(|s| !next.contains(s) && self.pools[pi].keyed[*s as usize].as_ref().is_some_and(|(k, d)| key_matches(k, lv) && d == item));
            next.push(kept.unwrap_or(u32::MAX));
        }
        for i in 0..items.len() {
            if next[i] != u32::MAX {
                continue;
            }
            let free = (0..self.pools[pi].slots.len() as u32).find(|s| !next.contains(s));
            let s = match free {
                Some(s) => s,
                None => self.grow_pool(page, pi, &info),
            };
            next[i] = s;
            self.bind_slot(page, pi, s, &info, &items[i]);
        }
        let zero = next.is_empty();
        let prev = std::mem::replace(&mut self.pools[pi].live, next);
        self.pools[pi].scratch = prev;
        self.show_empties(page, pi, zero);
        // The selection is kept BY KEY across reconciles (list.reorder-keeps):
        // re-assert it over the fresh rows; absent keys stay INERT in the
        // List's memory and re-attach when their item returns.
        let mut a = page.nodes[repeat].parent;
        while a != NO_NODE {
            if page.nodes[a].kind == Kind::List {
                if self.list_sel.iter().any(|(n, _)| *n == a) {
                    self.apply_selected(page, a);
                }
                break;
            }
            a = page.nodes[a].parent;
        }
        self.dirty = true;
        Ok(())
    }

    /// The rows a List owns right now, display order: every live row of
    /// every repeat under it (statics are never rows).
    pub fn list_rows(&self, page: &Page, list: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        for p in &self.pools {
            if p.repeat != list && page.is_under(p.repeat, list) {
                out.extend(p.live.iter().map(|s| p.slots[*s as usize]));
            }
        }
        out
    }

    /// Writes a List's selection from code — one write path with the tap
    /// (the addon's `_write_selected`). Refusals (G9104) keep the last
    /// good selection, never a half-applied write: `select=off` has no
    /// selection to write, `one` takes at most one key, duplicates are
    /// refused. Key SHAPE is the type's own guarantee (`ItemKey` is a
    /// string or a whole number — the addon checks that at its Variant
    /// surface).
    ///
    /// Returns whether the selection actually changed — the addon fires
    /// `change` exactly then and a same-value write is silent; a host
    /// notifies off this bool the same way.
    pub fn set_selected(&mut self, page: &Page, list: NodeId, keys: &[ItemKey]) -> Result<bool, String> {
        let mode = match page.prop(list, PropKey::Select) {
            Some(Value::VIdent(w)) => page.str(w),
            _ => "one",
        };
        let name = page.str(page.nodes[list].name);
        if mode == "off" {
            return Err(format!("G9104: `{name}` has select=off — no selection to write (activation still fires)"));
        }
        if mode == "one" && keys.len() > 1 {
            return Err(format!("G9104: `{name}` has select=one — write one key or none, got {}", keys.len()));
        }
        for (i, k) in keys.iter().enumerate() {
            if keys[..i].contains(k) {
                return Err(format!("G9104: duplicate key {k:?} in `{name}`'s selection"));
            }
        }
        let cur = self.list_sel.iter().find(|(n, _)| *n == list).map(|(_, k)| k.as_slice()).unwrap_or(&[]);
        if cur == keys {
            return Ok(false);
        }
        match self.list_sel.iter_mut().find(|(n, _)| *n == list) {
            Some(e) => e.1 = keys.to_vec(),
            None => self.list_sel.push((list, keys.to_vec())),
        }
        self.apply_selected(page, list);
        Ok(true)
    }

    /// A List's live selection (the addon's `selected` read-back):
    /// present keys in collection order, then inert keys in the order
    /// they entered the selection.
    pub fn selected_keys(&self, page: &Page, list: NodeId) -> Vec<ItemKey> {
        let Some((_, sel)) = self.list_sel.iter().find(|(n, _)| *n == list) else { return Vec::new() };
        let mut out: Vec<ItemKey> = Vec::new();
        for row in self.list_rows(page, list) {
            if let Some(k) = self.item_key(page, row) {
                if sel.contains(k) && !out.contains(k) {
                    out.push(k.clone());
                }
            }
        }
        for k in sel {
            if !out.contains(k) {
                out.push(k.clone());
            }
        }
        out
    }

    /// A tap's selection side effect on `row` (spec §5, user call
    /// 2026-07-15): `one` selects it — a re-tap stays selected, the radio
    /// doctrine — `many` toggles it, `off` nothing. Returns whether the
    /// selection changed (the caller fires `change` exactly then).
    pub fn tap_select(&mut self, page: &Page, list: NodeId, row: NodeId) -> bool {
        let mode = match page.prop(list, PropKey::Select) {
            Some(Value::VIdent(w)) => page.str(w),
            _ => "one",
        };
        let was = self.has(row, SELECTED);
        let flips = match mode {
            "one" => !was,
            "many" => true,
            _ => false,
        };
        if !flips {
            return false;
        }
        let on = !was;
        if let Some(k) = self.item_key(page, row).cloned() {
            let e = match self.list_sel.iter_mut().find(|(n, _)| *n == list) {
                Some(e) => e,
                None => {
                    self.list_sel.push((list, Vec::new()));
                    self.list_sel.last_mut().unwrap()
                }
            };
            if mode == "one" {
                e.1.clear();
                e.1.push(k);
            } else if on {
                if !e.1.contains(&k) {
                    e.1.push(k);
                }
            } else {
                e.1.retain(|x| x != &k);
            }
        }
        if mode == "one" {
            for r in self.list_rows(page, list) {
                if r != row {
                    self.set_state(r, SELECTED, false);
                }
            }
        }
        self.set_state(row, SELECTED, on);
        true
    }

    /// Sets each row's SELECTED to its key's membership in the List's
    /// selection — the state IS the look (`when selected`), so styles
    /// re-assert wherever this runs (code write, reconcile). Rows with
    /// no key (the counted `set_count` form) clear here: keyed memory
    /// cannot vouch for them.
    fn apply_selected(&mut self, page: &Page, list: NodeId) {
        let keys: Vec<ItemKey> = self.list_sel.iter().find(|(n, _)| *n == list).map(|(_, k)| k.clone()).unwrap_or_default();
        for row in self.list_rows(page, list) {
            let on = self.item_key(page, row).is_some_and(|k| keys.contains(k));
            self.set_state(row, SELECTED, on);
        }
    }

    /// Writes one item's `$item.<field>` values onto a slot through the
    /// write layer (`set_text` for text — the owned string mutates in
    /// place, so equal-shape churn allocates nothing) and freezes the
    /// item for the next diff.
    fn bind_slot(&mut self, page: &mut Page, pi: usize, slot: u32, info: &RepeatInfo, item: &Item) {
        let root = self.pools[pi].slots[slot as usize];
        let (s0, sc) = info.sites;
        for si in s0..s0 + sc {
            let site = page.bind_sites[si as usize];
            let node = root.offset(site.node_off);
            let key = page.props[(page.nodes[node.index()].props.0 + site.prop_off) as usize].key;
            let lv_idx = match item {
                Item::Record(r) => {
                    let fname = page.str(site.field);
                    r.iter().position(|(k, _)| *k == fname)
                }
                Item::Value(_) => None,
            };
            let lv: &ItemValue = match item {
                Item::Record(r) => &r[lv_idx.expect("validated by set_items")].1,
                Item::Value(v) => v,
            };
            match lv {
                ItemValue::Str(s) => {
                    if key == PropKey::Text {
                        self.set_text(page, node, s);
                    } else {
                        let id = page.intern(s);
                        self.set_prop(node, key, Value::VText { text: id, key: NO_KEY });
                    }
                }
                ItemValue::Int(v) => self.set_prop(node, key, Value::VNum(*v as f32)),
                ItemValue::Num(v) => self.set_prop(node, key, Value::VNum(*v)),
                ItemValue::Bool(b) => self.set_prop(node, key, Value::VBool(*b)),
                ItemValue::Color(c) => self.set_prop(node, key, Value::VColor(*c)),
            }
        }
        let lv = key_leaf(page, info.by, item);
        match &mut self.pools[pi].keyed[slot as usize] {
            Some((k, d)) => {
                assign_key(k, lv);
                assign_item(d, item);
            }
            e => {
                let k = match lv {
                    ItemValue::Str(s) => ItemKey::Str(s.clone()),
                    ItemValue::Int(v) => ItemKey::Int(*v),
                    _ => unreachable!("set_items validated the key type"),
                };
                *e = Some((k, item.clone()));
            }
        }
    }

    /// Shows the first `n` rows of a counted repeat (`set_text` fills
    /// them); its `empty { }` content shows exactly at zero. The first
    /// call clones slots out to `capacity=` (or `n`, whichever is more)
    /// — never a ceiling: a bigger `n` grows the pool.
    pub fn set_count(&mut self, page: &mut Page, repeat: NodeId, n: usize) {
        let Some(info) = page.repeats.iter().find(|r| r.node == repeat).copied() else { return };
        let pi = self.pool_of(page, repeat);
        let target = n.max(info.capacity as usize);
        while self.pools[pi].slots.len() < target {
            self.grow_pool(page, pi, &info);
        }
        self.pools[pi].live.clear();
        self.pools[pi].live.extend(0..n as u32);
        self.show_empties(page, pi, n == 0);
        self.dirty = true;
    }

    /// Builds a counted repeat out to its `capacity=` and shows every
    /// slot — for a host that drives the rows itself (GradientEdit hides
    /// and places marks per stop). Returns the slot count.
    pub fn reserve(&mut self, page: &mut Page, repeat: NodeId) -> usize {
        let cap = page.repeats.iter().find(|r| r.node == repeat).map(|r| r.capacity as usize).unwrap_or(0);
        self.set_count(page, repeat, cap);
        cap
    }

    /// The pool for `repeat`, created on first touch.
    fn pool_of(&mut self, page: &Page, repeat: NodeId) -> usize {
        match self.pools.iter().position(|p| p.repeat == repeat) {
            Some(i) => i,
            None => {
                self.pools.push(new_pool(page, repeat));
                self.pools.len() - 1
            }
        }
    }

    /// The `empty { }` content shows exactly at zero rows.
    fn show_empties(&mut self, page: &mut Page, pi: usize, zero: bool) {
        for k in 0..self.pools[pi].empties.len() {
            let e = self.pools[pi].empties[k];
            self.set_visible(page, e, zero);
        }
    }

    /// Clones one more slot from the template — the pool's only
    /// allocation event. The clone lands at the arena's end (contiguous
    /// buffers, amortized `Vec` growth — never per-control heap
    /// objects), and its nodes take the slot's name (`<repeat>/<i>/…`)
    /// so per-slot parts stay addressable (GradientEdit's marks).
    fn grow_pool(&mut self, page: &mut Page, pi: usize, info: &RepeatInfo) -> u32 {
        let idx = self.pools[pi].slots.len() as u32;
        let root = page.clone_subtree(info.template);
        let rep = page.str(page.nodes[info.node].name).to_string();
        let from = format!("{rep}/item");
        let to = format!("{rep}/{idx}");
        let end = page.subtree_end(root);
        for ni in NodeId::range(root, end) {
            let nm = page.nodes[ni].name;
            if nm == NO_STR {
                continue;
            }
            let s = page.str(nm);
            let renamed = if s == from {
                to.clone()
            } else if s.len() > from.len() && s.starts_with(&from) && s.as_bytes()[from.len()] == b'/' {
                format!("{to}{}", &s[from.len()..])
            } else {
                continue;
            };
            let id = page.intern(&renamed);
            page.nodes[ni].name = id;
        }
        // The template's own anims (a mark's delete/appear) exist once,
        // aimed at template nodes — each slot takes its copy, aimed at
        // its own nodes under the slot's name. Needed because a host
        // plays them per row (`play("<rep>/2/item/delete")`); without
        // the copy the play finds nothing and "finishes" the same frame.
        let t_end = page.subtree_end(info.template);
        let delta: u32 = root.index() as u32 - info.template.index() as u32;
        let heads: Vec<AnimHead> = page.anim_heads.iter().filter(|h| {
            let s = page.str(h.anim);
            s.starts_with(&from) && s.as_bytes().get(from.len()).is_none_or(|b| *b == b'/')
        }).copied().collect();
        for h in heads {
            let renamed = format!("{to}{}", &page.str(h.anim)[from.len()..]);
            let id = page.intern(&renamed);
            let tracks: Vec<AnimTrack> = page.anims.iter().filter(|t| t.anim == h.anim).copied().collect();
            for mut t in tracks {
                t.anim = id;
                if t.node >= info.template && t.node < t_end {
                    t.node = t.node.offset(delta);
                }
                page.anims.push(t);
            }
            page.anim_heads.push(AnimHead { anim: id, ..h });
        }
        self.grow(page);
        self.pools[pi].slots.push(root);
        self.pools[pi].keyed.push(None);
        idx
    }

    /// Is `anim` mid-play?
    pub fn is_playing<A: AnimName>(&self, anim: A) -> bool {
        self.is_playing_id(anim.id())
    }

    /// `is_playing` by the anim's string id, for a runtime-compiled page.
    pub fn is_playing_id(&self, anim: StrId) -> bool {
        self.players.iter().any(|p| p.anim == anim && !p.stopped)
    }

    /// Is a prop lane (GP40) an anim's right now — a live player's, or a
    /// finished one's held end pose? The tween spawner asks BEFORE
    /// `compose_anim` has stamped the resolved copy.
    pub fn anim_owns_prop_now(&self, node: NodeId, key: PropKey) -> bool {
        self.held_props.iter().any(|h| h.0 == node && h.1 == key) || self.players.iter().filter(|p| !p.stopped).any(|p| p.owns_props.contains(&(node, key)))
    }

    /// The anims that ended on their own since the last call, drained
    /// (GP20's `finished`). `stop()` never lands here.
    pub fn take_finished(&mut self) -> Vec<StrId> {
        std::mem::take(&mut self.finished)
    }

    /// Stops `anim` (WPF Storyboard.Stop): `restore` puts every channel it
    /// owns back to its pre-play value; the player's finish action does
    /// not run. Nothing happens when it is not playing.
    pub fn stop<A: AnimName>(&mut self, anim: A, restore: bool) {
        self.stop_id(anim.id(), restore);
    }

    /// `stop` by the anim's string id, for a runtime-compiled page.
    pub fn stop_id(&mut self, anim: StrId, restore: bool) {
        let mut any = false;
        for p in self.players.iter_mut().filter(|p| p.anim == anim) {
            p.stop(restore, &mut self.motion, &mut self.prop_anim);
            if restore {
                // Release the channels: the ladder shows again — except
                // where the play found a held end pose, which comes back.
                for (node, channel, base, base_held) in p.owned_bases() {
                    if base_held {
                        self.held.push((node, channel, base));
                    }
                }
                for (node, key, base, base_held) in p.owned_bases_props() {
                    if base_held {
                        self.held_props.push((node, key, base));
                    }
                }
            } else {
                // Left where it is: the current values stand as held ends.
                for (node, channel) in &p.owns {
                    self.held.push((*node, *channel, self.motion[*node].get(*channel)));
                }
                for (node, key) in &p.owns_props {
                    if let Some(v) = get_pval(&self.prop_anim, *node, *key) {
                        self.held_props.push((*node, *key, v));
                    }
                }
            }
            any = true;
        }
        if any {
            self.players.retain(|p| !p.stopped);
            let players = &self.players;
            self.prop_anim.retain(|e| players.iter().any(|p| p.owns_props.contains(&(e.0, e.1))));
            self.dirty = true;
        }
    }

    /// Ends a condition's `transition anim=` play (GPAR109): the player
    /// stops and every channel it displaced goes back to the base it had
    /// at play start — Godot tweens them home with the exit rule's
    /// timing, here they snap (no shipped page glides one yet). A channel
    /// a LIVE player owns is that player's; it is left alone.
    fn end_state_anims(&mut self, who: NodeId, state: State) {
        let ended: Vec<(NodeId, State, StrId, Vec<(NodeId, Channel, f32, bool)>, Vec<(NodeId, PropKey, Value, bool)>)> = {
            let (out, keep) = self.state_anims.drain(..).partition(|e| e.0 == who && e.1 == state);
            self.state_anims = keep;
            out
        };
        for (_, _, anim, displaced, displaced_props) in ended {
            for p in self.players.iter_mut().filter(|p| p.anim == anim) {
                p.stop(false, &mut self.motion, &mut self.prop_anim);
            }
            self.players.retain(|p| !p.stopped);
            for (n, channel, base, base_held) in displaced {
                if self.players.iter().any(|p| p.owns.contains(&(n, channel))) {
                    continue;
                }
                self.held.retain(|h| !(h.0 == n && h.1 == channel));
                self.motion[n].set(channel, base);
                if base_held {
                    self.held.push((n, channel, base));
                }
            }
            for (n, key, base, base_held) in displaced_props {
                if self.players.iter().any(|p| p.owns_props.contains(&(n, key))) {
                    continue;
                }
                self.held_props.retain(|h| !(h.0 == n && h.1 == key));
                self.prop_anim.retain(|e| !(e.0 == n && e.1 == key));
                if base_held {
                    self.held_props.push((n, key, base));
                }
            }
            self.dirty = true;
        }
    }

    /// Advances tweens and players by `dt` seconds; true while anything moves.
    pub fn tick(&mut self, page: &mut Page, dt: f32) -> bool {
        let mut moving = false;
        // GP52: the deferred `live($value)` follows (`set_value_deferred`).
        for root in std::mem::take(&mut self.live_follow) {
            let v = self.writes.iter().find(|(n, w)| *n == root && w.key == PropKey::Value).and_then(|(_, w)| if let Value::VNum(v) = w.value { Some(v) } else { None });
            if let Some(v) = v {
                self.follow_live(page, root, v);
            }
        }
        if !self.changes.is_empty() {
            // A flip already recorded but not yet resolved: this frame's
            // time belongs to its tweens-to-be.
            self.pending_change_dt += dt;
        }
        for t in &mut self.tweens {
            t.elapsed += dt;
        }
        for t in &mut self.rect_tweens {
            t.3 += dt;
        }
        moving |= !self.rect_tweens.is_empty() || !self.auto_glides.is_empty();
        let glides = std::mem::take(&mut self.auto_glides);
        for mut g in glides {
            g.elapsed += dt;
            let raw = if g.duration <= 0.0 { 1.0 } else { (g.elapsed / g.duration).clamp(0.0, 1.0) };
            g.shown = g.from + (g.to - g.from) * g.easing.apply(raw);
            if g.elapsed >= g.duration {
                // Landed: the written fraction HOLDS (disarm-holds, §4.5).
                if let Some(e) = self.auto_rest.iter_mut().find(|(n, _)| *n == g.node) {
                    e.1 = g.to;
                }
            } else {
                self.auto_glides.push(g);
            }
        }
        if !self.pulses.is_empty() {
            for p in &mut self.pulses {
                p.elapsed += dt;
            }
            self.pulses.retain(|p| p.elapsed < TAP_PULSE_OUT_SEC + TAP_PULSE_BACK_SEC);
            moving = true;
        }
        let before = self.tweens.len();
        self.tweens.retain(|t| t.elapsed < t.duration);
        moving |= before > 0;
        let mut finished: Vec<(Finish, Vec<(NodeId, Channel)>, Vec<(NodeId, PropKey)>, StrId)> = Vec::new();
        let mut players = std::mem::take(&mut self.players);
        players.retain_mut(|p| {
            let alive = p.tick(dt, &mut self.motion, &mut self.prop_anim);
            if !alive {
                finished.push((p.finish, p.owns.clone(), p.owns_props.clone(), p.anim));
            }
            alive
        });
        moving |= !players.is_empty() || !finished.is_empty();
        self.players = players;
        for (f, owns, owns_props, anim) in finished {
            self.finished.push(anim);
            if let Finish::Hide(node) | Finish::HideWhen(node) = f {
                if let Finish::Hide(_) = f {
                    // The code door hides by writing the prop; a `when`-
                    // driven hide (HideWhen) writes NOTHING — the `when`'s
                    // own gone takes over as `Resolved::hiding` lifts, and
                    // a code write here would outrank it forever (GPAR57).
                    self.set_visible(page, node, false);
                }
                self.motion[node] = Motion::IDENTITY;
                // The hide anim may have moved DESCENDANTS too — every
                // driven channel goes back to identity, or the re-show
                // finds them displaced (GPAR110, gogo_visibility_fx.gd).
                for (n, channel) in owns {
                    self.motion[n].set(channel, Motion::IDENTITY.get(channel));
                }
                // Prop lanes release outright on a hide: the layer entry
                // goes and the ladder look comes back for the re-show.
                for (n, key) in owns_props {
                    self.prop_anim.retain(|e| !(e.0 == n && e.1 == key));
                }
            } else {
                // A finished anim's end pose is a standing value the author
                // chose (WPF HoldEnd): it stays until the next writer. A
                // target HIDDEN mid-anim keeps nothing — its pose means
                // nothing off screen, and holding it would show a
                // displaced box on the next show (GPAR85).
                for (node, channel) in owns {
                    self.held.retain(|h| !(h.0 == node && h.1 == channel));
                    if self.visible_now(page, node) {
                        self.held.push((node, channel, self.motion[node].get(channel)));
                    } else {
                        self.motion[node].set(channel, Motion::IDENTITY.get(channel));
                    }
                }
                for (node, key) in owns_props {
                    self.held_props.retain(|h| !(h.0 == node && h.1 == key));
                    let v = get_pval(&self.prop_anim, node, key);
                    self.prop_anim.retain(|e| !(e.0 == node && e.1 == key));
                    if let Some(v) = v {
                        if self.visible_now(page, node) {
                            self.held_props.push((node, key, v));
                        }
                    }
                }
            }
        }
        if moving {
            self.dirty = true;
        }
        moving
    }

    /// A code write: `text=`, `visibility=`, `value=`… Replaces an earlier
    /// write of the same key; a same-value write changes nothing.
    pub fn set_prop(&mut self, node: NodeId, key: PropKey, value: Value) {
        // GP37: `selected` DERIVES from a Toggle's value — a code write
        // of `value=1` selects (the shipped tgl.derive-on).
        if key == PropKey::Value && self.toggle.get(node.index()).copied().unwrap_or(false) {
            if let Value::VNum(v) = value {
                let on = v >= 0.5;
                if self.has(node, SELECTED) != on {
                    let s = &mut self.states[node];
                    if on { *s |= SELECTED } else { *s &= !SELECTED }
                    self.dirty = true;
                    self.changes.push(if on { (node, None, Some(State::Selected)) } else { (node, Some(State::Selected), None) });
                }
            }
        }
        // A code write is the next writer: a held anim end on that
        // property yields to it.
        let channels: &[Channel] = Channel::of_key(key);
        if !channels.is_empty() {
            self.held.retain(|h| !(h.0 == node && channels.contains(&h.1)));
        }
        if let Some(w) = self.writes.iter_mut().find(|(n, p)| *n == node && p.key == key) {
            if w.1.value == value {
                return;
            }
            w.1.value = value;
        } else {
            self.writes.push((node, Prop { key, value }));
        }
        self.dirty = true;
    }

    /// Takes a code write back: `node`'s `key` reads the file's value
    /// again (the shipped "fixed_w = -1 = unset" — a Tiles cell that was a
    /// 72x72 square becomes a full row with no width of its own). A key
    /// never written is a no-op.
    pub fn clear_prop(&mut self, node: NodeId, key: PropKey) {
        let before = self.writes.len();
        self.writes.retain(|(n, p)| !(*n == node && p.key == key));
        if self.writes.len() != before {
            self.dirty = true;
        }
    }

    /// A code write of a label's text. The first write pushes a fresh
    /// string slot the node owns (never a shared interned one — another
    /// node may show the same words); later writes rewrite that slot in
    /// place. A same-text write changes nothing.
    pub fn set_text(&mut self, page: &mut Page, node: NodeId, text: &str) {
        let owned = self.owned_text[node];
        let id = if owned != NO_STR {
            if page.strings[owned] == text {
                return;
            }
            let slot = &mut page.strings[owned];
            slot.clear();
            slot.push_str(text);
            self.dirty = true;
            owned
        } else {
            if page.strings.is_empty() {
                page.strings.push(String::new());
            }
            page.strings.push(text.to_string());
            let id = StrId::from_index((page.strings.len() - 1));
            self.owned_text[node] = id;
            id
        };
        self.set_prop(node, PropKey::Text, Value::VText { text: id, key: NO_KEY });
    }

    /// Shows the translation key `key` on `node` — the string follows the
    /// locale (`set_locale`). Takes only a build-emitted `Key`, so a key no
    /// page declares is a rustc error. `set_text` is the other write: a
    /// literal shown as given, never translated.
    pub fn set_key<K: KeyName>(&mut self, node: NodeId, key: K) {
        self.set_prop(node, PropKey::Text, Value::VText { text: NO_STR, key: key.id() });
    }

    /// Switches the language: every keyed text re-measures and redraws in
    /// the new strings (one relayout — the table is read at layout time,
    /// nothing per node is rewritten). `None` = literals as written.
    pub fn set_locale(&mut self, locale: Option<Arc<Locale>>) {
        self.locale = locale;
        self.dirty = true;
    }

    /// Code-side lookup for a dynamic string (a toast, a formatted
    /// message) — the shipped `GogoBridge.Translate(key, fallback)`: the
    /// active language's text for `key`, else `fallback` (the English the
    /// caller passes; an empty fallback falls back to the key itself, the
    /// shipped `gogo.translate` rule). No locale set = `fallback`. The
    /// key must be one this build knows (`KEYS` — pages' `/key`s plus the
    /// host's seeded code keys); an unknown key shows the fallback.
    pub fn translate<'a>(&'a self, key: &'a str, fallback: &'a str) -> &'a str {
        match &self.locale {
            Some(l) => l.translate(key, fallback),
            None if fallback.is_empty() => key,
            None => fallback,
        }
    }

    pub fn set_visible(&mut self, page: &mut Page, node: NodeId, visible: bool) {
        if !visible {
            // A hidden node's standing anim values mean nothing: on the
            // next show the ladder shows.
            self.held.retain(|h| h.0 != node);
        }
        let word = page.intern(if visible { "visible" } else { "gone" });
        self.set_prop(node, PropKey::Visibility, Value::VIdent(word));
    }

    /// Raises or clears one of a control's declared states (`state a, b`
    /// in its body) — the code seam the addon's control scripts use
    /// (gogo_control_root.declared_states / set_state). `when <name>`
    /// blocks in that control restyle on the flip; `transition` rules
    /// naming the state time it.
    ///
    /// An undeclared name is refused (`Err` naming the control), so a
    /// typo cannot silently do nothing.
    pub fn set_custom(&mut self, page: &Page, root: NodeId, state: &str, on: bool) -> Result<(), String> {
        let sid = page.intern_lookup(state);
        let name = page.str(page.nodes[root].name);
        if sid == NO_STR || !page.custom_states.iter().any(|(n, s)| *n == root && *s == sid) {
            return Err(format!("`{state}` is not a declared state of `{name}` — its `state` line names them"));
        }
        let has = self.customs.iter().any(|e| *e == (root, sid));
        if has == on {
            return Ok(());
        }
        if on {
            self.customs.push((root, sid));
        } else {
            self.customs.retain(|e| *e != (root, sid));
        }
        self.changes.push(if on { (root, None, Some(State::Custom(sid))) } else { (root, Some(State::Custom(sid)), None) });
        self.dirty = true;
        Ok(())
    }

    /// Reads a declared custom state back (`set_custom` is the write).
    pub fn custom(&self, page: &Page, root: NodeId, state: &str) -> bool {
        let sid = page.intern_lookup(state);
        sid != NO_STR && self.customs.iter().any(|e| *e == (root, sid))
    }

    /// The control seam behind a code write of `$value` (GP33, the
    /// addon's behavior -> driver pipeline): the value prop lands (reads
    /// and `when` conditions see it), every `live($value)` label inside
    /// the control rewrites at its baked display step, and the automatic
    /// slots' fractions re-derive on the next `resolve_animated`, gliding
    /// by the control's default feel.
    pub fn set_value(&mut self, page: &mut Page, root: NodeId, v: f32) {
        self.set_prop(root, PropKey::Value, Value::VNum(v));
        self.follow_live(page, root, v);
    }

    /// See `set_value` — the label half, shared with `tick`'s deferred
    /// follow (GP52).
    fn follow_live(&mut self, page: &mut Page, root: NodeId, v: f32) {
        let labels: Vec<LiveText> = page.live_texts.iter().filter(|lt| lt.root == root).copied().collect();
        for lt in labels {
            self.set_text(page, lt.node, &crate::page::live_text(v, lt.step));
        }
    }

    /// `set_value` for a caller holding only `&Page` — the touch and
    /// wheel paths (GP52): the prop lands now (reads, `when` conditions
    /// and the automatic slots see it), the `live($value)` labels follow
    /// at the next `tick`.
    pub fn set_value_deferred(&mut self, root: NodeId, v: f32) {
        self.set_prop(root, PropKey::Value, Value::VNum(v));
        if !self.live_follow.contains(&root) {
            self.live_follow.push(root);
        }
    }

    pub fn set_state(&mut self, node: NodeId, bit: u8, on: bool) {
        let s = &mut self.states[node];
        let before = *s;
        if on {
            *s |= bit;
        } else {
            *s &= !bit;
        }
        if *s != before {
            self.dirty = true;
            if bit == SELECTED {
                // GP37: a Radio picked from code behaves like a tap — the
                // old pick clears, silently (this pick's flip is the one
                // genuine transition, the shipped code-write-reports read).
                if on {
                    let choice = self.radio_choice.get(node.index()).copied().unwrap_or(NO_CHOICE);
                    if choice != NO_CHOICE {
                        for i in 0..self.states.len() {
                            if NodeId::from_index(i) != node && self.radio_choice[i] == choice {
                                self.states[i] &= !SELECTED;
                            }
                        }
                    }
                }
                // GP37: a Toggle's value derives from the bool — a code
                // `selected=false` puts the value back to 0. Only a
                // wrong-side code-written value is rewritten, so a live
                // drag's fraction (already on the right side) keeps.
                if self.toggle.get(node.index()).copied().unwrap_or(false) {
                    let wrong_side = self.writes.iter().any(|(n, p)| *n == node && p.key == PropKey::Value && matches!(p.value, Value::VNum(v) if (v >= 0.5) != on));
                    if wrong_side {
                        self.set_prop(node, PropKey::Value, Value::VNum(if on { 1.0 } else { 0.0 }));
                    }
                }
            }
            let state = match bit {
                HELD => State::Held,
                HOVERED => State::Hovered,
                FOCUSED => State::Focused,
                FOCUS_WITHIN => State::FocusWithin,
                SELECTED => State::Selected,
                _ => State::StateDisabled,
            };
            self.changes.push(if on { (node, None, Some(state)) } else { (node, Some(state), None) });
        }
    }

    pub fn has(&self, node: NodeId, bit: u8) -> bool {
        self.states[node] & bit != 0
    }

    /// Bumps `node`'s paint order among its siblings (the shipped
    /// Canvas.ZIndex): higher draws on top and takes the touch first, 0 =
    /// back to the file's order. Layout is untouched — only paint and hit.
    pub fn set_raise(&mut self, node: NodeId, raise: i32) {
        let before = self.raised.iter().find(|(n, _)| *n == node).map(|(_, r)| *r).unwrap_or(0);
        if before == raise {
            return;
        }
        self.raised.retain(|(n, _)| *n != node);
        if raise != 0 {
            self.raised.push((node, raise));
        }
        self.dirty = true;
    }

    /// EFFECTIVE `disabled`: the node's own flag or any ancestor's — the
    /// WPF IsEnabled law; a child cannot re-enable under a disabled parent.
    pub fn disabled(&self, page: &Page, node: NodeId) -> bool {
        let mut n = node;
        loop {
            if self.has(n, DISABLED) {
                return true;
            }
            let parent = page.nodes[n].parent;
            if parent == NO_NODE {
                return false;
            }
            n = parent;
        }
    }

    /// Moves the view to an arrangement by its typed handle — the
    /// `enum Arrangement` a page's build emits (GPAR27): an unknown
    /// arrangement is a rustc error, never a silent no-op.
    pub fn go<A: ArrangementName>(&mut self, arrangement: A) {
        self.go_id(arrangement.id());
    }

    /// Is `arrangement` the pose in force? (`rt.arrangement_is(Arrangement::Wide)`.)
    pub fn arrangement_is<A: ArrangementName>(&self, arrangement: A) -> bool {
        self.arrangement == arrangement.id()
    }

    /// `go` by the arrangement's id — a pose saved earlier from
    /// `Runtime::arrangement`, or an index on a runtime-compiled page
    /// (tests, hot reload). An index the page never declared is refused
    /// and the pose KEEPS (the addon's printerr + early return, GPAR27).
    pub fn go_id(&mut self, arrangement: ArrangementId) {
        if arrangement != BASE && arrangement.index() >= self.arrangement_count {
            return;
        }
        if self.arrangement != arrangement {
            let from = self.arrangement;
            self.arrangement = arrangement;
            self.dirty = true;
            self.changes.push((NO_NODE, Some(State::Arrangement(from)), Some(State::Arrangement(arrangement))));
            // The awaitable for hosts sequencing work after a pose change.
            self.pending_settle = Some(arrangement);
        }
    }

    /// Selects the active theme variant (the shipped
    /// `gogo.set_theme_variant`): `theme <variant> { … }` overrides win
    /// over the defaults — tokens, color-pair halves and kind entries all
    /// pick by it on the next resolve. `""` = defaults only; a variant no
    /// file defines simply keeps the defaults.
    pub fn set_theme_variant(&mut self, page: &Page, variant: &str) {
        let v = if variant.is_empty() { NO_STR } else { page.intern_lookup(variant) };
        if self.variant != v {
            self.variant = v;
            self.dirty = true;
        }
    }

    /// `go` without the motion (the shipped `set_arrangement(name, true)`):
    /// the pose flips and every `when <arrangement>` restyles, but no
    /// transition rule times it — no tweens, no rect glides, no rule anim.
    pub fn go_instant<A: ArrangementName>(&mut self, arrangement: A) {
        self.go_instant_id(arrangement.id());
    }

    /// `go_instant` by id, for a runtime-compiled page.
    pub fn go_instant_id(&mut self, arrangement: ArrangementId) {
        let before = self.arrangement;
        self.go_id(arrangement);
        if self.arrangement != before {
            self.instant_flips.push(arrangement);
        }
    }

    /// The settled flip, once (the shipped `arrangement_settled` signal,
    /// polled): `Some(arrangement)` on the first call after the flip's
    /// tweens and glides have all landed — an instant flip settles on the
    /// first call after its resolve. `None` while motion is still going
    /// (or nothing flipped).
    pub fn arrangement_settled(&mut self) -> Option<ArrangementId> {
        if self.pending_settle.is_some() && self.changes.is_empty() && !self.animating() {
            return self.pending_settle.take();
        }
        None
    }

    /// `go`-or-back-to-base on the typed handle (the addon's toggle).
    pub fn toggle<A: ArrangementName>(&mut self, arrangement: A) {
        self.toggle_id(arrangement.id());
    }

    /// `toggle` by id, for a runtime-compiled page.
    pub fn toggle_id(&mut self, arrangement: ArrangementId) {
        let target = if self.arrangement == arrangement { BASE } else { arrangement };
        self.go_id(target);
    }

    /// The host names the live screen orientation and class; every `when
    /// screen.*` re-poses at once, instantly (no transition, like a
    /// rotation). A word the page never mentions is simply not active.
    pub fn set_screen(&mut self, page: &Page, orientation: Orientation, class: ScreenClass) {
        let mut words: Vec<StrId> = Vec::new();
        for w in [orientation.word(), class.word()] {
            let id = page.intern_lookup(w);
            if id != NO_STR {
                words.push(id);
            }
        }
        words.sort();
        if words != self.screen {
            self.screen = words;
            self.dirty = true;
        }
    }

    pub fn state_active(&self, page: &Page, node: NodeId, state: State) -> bool {
        match state {
            State::Held => self.has(node, HELD),
            State::Hovered => self.has(node, HOVERED),
            State::Focused => self.has(node, FOCUSED),
            State::FocusWithin => self.has(node, FOCUS_WITHIN),
            State::Selected => self.has(node, SELECTED),
            State::StateDisabled => self.disabled(page, node),
            State::Arrangement(i) => self.arrangement == i,
            // Raised on the control root; a `when <name>` anywhere in
            // that control responds — the walk finds the root above.
            State::Custom(s) => {
                let mut n = node;
                while n != NO_NODE {
                    if self.customs.iter().any(|(cn, cs)| *cn == n && *cs == s) {
                        return true;
                    }
                    n = page.nodes[n].parent;
                }
                false
            }
            State::ScreenOrientation(w) | State::ScreenClass(w) => self.screen.contains(&w),
            State::ArrangementScreen(i, w) => self.arrangement == i && self.screen.contains(&w),
            State::ScreenPair(o, c) => self.screen.contains(&o) && self.screen.contains(&c),
        }
    }
}

/// The current values: base placement with active poses applied, and the
/// prop overrides the active states contribute.
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    /// Per node: the channels an anim owns right now (`Channel::bit`) —
    /// playing, or a finished anim's held end. Draw reads it to let an anim
    /// outrank the built-in disabled dim on opacity.
    pub anim_owned: Vec<u16>,
    /// Prop lanes an anim owns right now (GP40) — playing, or a held end
    /// pose. `when` writes and `transition` tweens yield to these.
    pub anim_owned_props: Vec<(NodeId, PropKey)>,
    pub placement: Vec<Placement>,
    /// (node, prop) — later entries win.
    pub overrides: Vec<(NodeId, Prop)>,
    pub variant: StrId,
    /// Per node: the Scroll offset (copied from the Runtime).
    pub scroll: Vec<[f32; 2]>,
    pub safe: [f32; 4],
    /// Per node: the motion channels the draw walk applies.
    pub motion: Vec<Motion>,
    /// Per node: the `seat` socket it sits in (`NO_NODE` = its declared home).
    pub seat: Vec<NodeId>,
    /// Per node: the state bits (`SELECTED`, `DISABLED` …) — the native
    /// looks draw the mark from them.
    pub states: Vec<u8>,
    /// Paint-order bumps (`Runtime::set_raise`), copied for draw and hit.
    pub raised: Vec<(NodeId, i32)>,
    /// Keyed repeats' live row roots, in shown order (copied from the
    /// Runtime) — `children` hands them to layout, draw and hit.
    pub repeat_rows: Vec<(NodeId, Vec<NodeId>)>,
    /// Nodes mid deferred hide (GP19): a `when` flipped them hidden but
    /// their `on:hide` anim still plays — layout, draw and input keep them
    /// until the player finishes (`gone`/`hidden` answer false for them).
    pub hiding: Vec<NodeId>,
    /// GP33: each value-driven automatic slot's SHOWN fraction this
    /// frame (rest, re-derived target, or a glide's eased value).
    pub auto: Vec<(NodeId, f32)>,
    /// GP33: each presence slot and whether its driving state holds.
    pub presence: Vec<(NodeId, bool)>,
    /// The focused Input's caret (copied from the Runtime).
    pub caret: Option<(NodeId, u32)>,
    /// The focused Input's selected span (copied from the Runtime).
    pub selection: Option<(NodeId, u32, u32)>,
    /// The selection is touch-born and shows drag handles (copied).
    pub sel_handles: bool,
    /// The language keyed text shows (copied from the Runtime; one Arc
    /// bump per pass, no allocation).
    pub locale: Option<Arc<Locale>>,
}

impl Resolved {
    /// The prop's current value: state overrides → node props → kind
    /// defaults. `$refs` are followed through the theme.
    pub fn prop(&self, page: &Page, node: NodeId, key: PropKey) -> Option<Value> {
        let raw = self
            .overrides
            .iter()
            .rev()
            .find(|(n, p)| *n == node && p.key == key)
            .map(|(_, p)| p.value)
            .or_else(|| page.prop(node, key))
            .or_else(|| page.kind_default(page.nodes[node].kind, key, self.variant))?;
        Some(deref(page, self.variant, raw))
    }

    pub fn color(&self, page: &Page, node: NodeId, key: PropKey) -> Option<Color_Straight_Srgb_4xU8_RGBA> {
        let mut c = match self.prop(page, node, key) {
            Some(Value::VColor(c)) => c,
            Some(Value::VStop { color, .. }) => color,
            // A live stop key nobody wrote yet answers from the DECLARED
            // `stops=` entry (1-based) — the anim door's `.a` lanes (GP40)
            // and code readers see the color the gradient draws.
            None if matches!(key, PropKey::StopColor(_)) => {
                let PropKey::StopColor(n) = key else { return None };
                let list = self.prop(page, node, PropKey::Stops)?;
                match page.list(list).get((n as usize).checked_sub(1)?)? {
                    Value::VStop { color, .. } => *color,
                    Value::VColor(c) => *c,
                    _ => return None,
                }
            }
            _ => return None,
        };
        // `color.a = 0.5` in a `when`: the fill keeps its hue, alpha alone moves.
        if key == PropKey::Color {
            if let Some(Value::VNum(a)) = self.overrides.iter().rev().find(|(n, p)| *n == node && p.key == PropKey::ColorA).map(|(_, p)| p.value) {
                c = c.with_opacity(a);
            }
        }
        Some(c)
    }

    /// A Slider value the way the control shows it: snapped to `step`
    /// from `min` and clamped. One quantizer for every writer — the file's
    /// `value=`, a code write and the finger all land on a step.
    pub fn snap_slider(&self, page: &Page, node: NodeId, v: f32) -> f32 {
        let min = self.num(page, node, PropKey::Min).unwrap_or(0.0);
        let max = self.num(page, node, PropKey::Max).unwrap_or(100.0);
        // Default 1: a fresh Godot Range steps by whole numbers
        // (GPAR92); `step=0` written in the file turns snapping off.
        let step = self.num(page, node, PropKey::Step).unwrap_or(1.0);
        let v = if step > 0.0 { min + ((v - min) / step).round() * step } else { v };
        v.clamp(min.min(max), max.max(min))
    }

    /// The Slider's shown value (`value=` or the last write, quantized).
    pub fn slider_value(&self, page: &Page, node: NodeId) -> f32 {
        let min = self.num(page, node, PropKey::Min).unwrap_or(0.0);
        self.snap_slider(page, node, self.num(page, node, PropKey::Value).unwrap_or(min))
    }

    pub fn num(&self, page: &Page, node: NodeId, key: PropKey) -> Option<f32> {
        match self.prop(page, node, key)? {
            Value::VNum(n) => Some(n),
            _ => None,
        }
    }

    pub fn ident<'p>(&self, page: &'p Page, node: NodeId, key: PropKey) -> Option<&'p str> {
        match self.prop(page, node, key)? {
            Value::VIdent(id) => Some(page.str(id)),
            _ => None,
        }
    }

    pub fn bool(&self, page: &Page, node: NodeId, key: PropKey) -> Option<bool> {
        match self.prop(page, node, key)? {
            Value::VBool(b) => Some(b),
            _ => None,
        }
    }

    /// The font key for `node`: its own `font=`, else the nearest ancestor's,
    /// else the kind default (`theme { font = … }`).
    pub fn font(&self, page: &Page, node: NodeId) -> Option<StrId> {
        match self.inherited(page, node, PropKey::Font) {
            Some(Value::VText { text, .. }) => Some(text),
            Some(Value::VIdent(id)) => Some(id),
            _ => None,
        }
    }

    /// A text default that flows down the tree: the node's own value,
    /// else the nearest ancestor's, else the kind default (`theme {
    /// font_size = … }`, kept on Label).
    ///
    /// Needed so `theme font_size=$x` inside a block reaches every Label
    /// under it (the shipped addon's theme propagation) — without the
    /// walk it lands on the block alone and the Labels draw at the 15
    /// default.
    fn inherited(&self, page: &Page, node: NodeId, key: PropKey) -> Option<Value> {
        let mut n = node;
        loop {
            let own = self.overrides.iter().rev().find(|(i, p)| *i == n && p.key == key).map(|(_, p)| p.value).or_else(|| page.prop(n, key));
            if let Some(v) = own {
                return Some(deref(page, self.variant, v));
            }
            let parent = page.nodes[n].parent;
            if parent == NO_NODE {
                break;
            }
            n = parent;
        }
        page.kind_default(page.nodes[node].kind, key, self.variant).or_else(|| page.kind_default(Kind::Label, key, self.variant)).map(|v| deref(page, self.variant, v))
    }

    /// `font_size=`: own, inherited, or the kind default.
    pub fn font_size(&self, page: &Page, node: NodeId) -> Option<f32> {
        match self.inherited(page, node, PropKey::FontSize)? {
            Value::VNum(n) => Some(n),
            _ => None,
        }
    }

    /// `font_color=`: own, inherited, or the kind default.
    pub fn font_color(&self, page: &Page, node: NodeId) -> Option<Color_Straight_Srgb_4xU8_RGBA> {
        match self.inherited(page, node, PropKey::FontColor)? {
            Value::VColor(c) => Some(c),
            _ => None,
        }
    }

    /// `visibility=gone` — out of layout. An `automatic=presence` slot
    /// whose state is off is gone too.
    pub fn gone(&self, page: &Page, node: NodeId) -> bool {
        // A keyed repeat's template never shows — only its clones do.
        if page.repeats.iter().any(|r| r.template == node) {
            return true;
        }
        if let Auto::Presence(baked) = page.nodes[node].auto {
            // GP33: the mark exists iff its driving state holds — live,
            // through `set_custom` on the control root.
            let on = self.presence.iter().find(|(n, _)| *n == node).map(|(_, b)| *b).unwrap_or(baked);
            if !on {
                return true;
            }
        }
        // Mid deferred hide (GP19): the `when` already says gone but the
        // `on:hide` anim still plays — the node stays until it finishes.
        if self.hiding.contains(&node) {
            return false;
        }
        self.ident(page, node, PropKey::Visibility) == Some("gone")
    }

    /// The nodes laid out and drawn under `id`: its own children that no
    /// `when` seated elsewhere, plus whoever is seated here.
    pub fn children(&self, page: &Page, id: NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = page.children(id).filter(|c| self.seat.get(c.index()).copied().unwrap_or(NO_NODE) == NO_NODE).collect();
        if page.nodes[id].kind == Kind::Seat {
            out.extend(self.seat.iter().enumerate().filter(|(_, s)| **s == id).map(|(i, _)| NodeId::from_index(i)));
        }
        if page.nodes[id].kind == Kind::Repeat {
            if let Some((_, rows)) = self.repeat_rows.iter().find(|(r, _)| *r == id) {
                out.extend(rows.iter().copied());
            }
        }
        out
    }

    /// `children` in PAINT order: siblings sorted by their raise (stable,
    /// so equal raises keep the file's order). Draw and hit use this;
    /// layout keeps the file's order.
    pub fn paint_children(&self, page: &Page, id: NodeId) -> Vec<NodeId> {
        let mut out = self.children(page, id);
        if !self.raised.is_empty() {
            out.sort_by_key(|c| self.raised.iter().find(|(n, _)| n == c).map(|(_, r)| *r).unwrap_or(0));
        }
        out
    }

    /// `visibility=space_only` — laid out (the slot holds), not drawn,
    /// no input.
    pub fn hidden(&self, page: &Page, node: NodeId) -> bool {
        !self.hiding.contains(&node) && self.ident(page, node, PropKey::Visibility) == Some("space_only")
    }
}

/// Follows `$refs` through the theme (variant first, then defaults) and
/// applies a ref's `/opacity` to the color it lands on. The first `/` met
/// is the slot's own and it is the one that counts: `/o` replaces the
/// alpha of the value it is written on, so `$a/0.3` over `$a = $b/0.5`
/// draws 0.3 (decided 2026-08-31, E6). GPAR31 had this the other way
/// round — the theme's number drew, under a clean build.
pub fn deref(page: &Page, variant: StrId, v: Value) -> Value {
    let mut v = v;
    let mut opacity: Option<f32> = None;
    for _ in 0..8 {
        match v {
            Value::VRef { name, opacity: o } => {
                if opacity.is_none() {
                    opacity = o;
                }
                match page.theme_value(name, variant) {
                    Some(next) => v = next,
                    None => return v,
                }
            }
            _ => break,
        }
    }
    // A variant-flipping word collapses to the half the active variant
    // reads: `dark` takes the dark half, anything else the light one.
    if let Value::VColorPair { light, dark } = v {
        v = Value::VColor(if page.str(variant) == "dark" { dark } else { light });
    }
    match (v, opacity) {
        (Value::VColor(c), Some(o)) => Value::VColor(c.with_opacity(o)),
        _ => v,
    }
}

/// The `when` ladder plus the anim layer on top (see `compose_anim`).
pub fn resolve(page: &Page, rt: &Runtime) -> Resolved {
    let mut r = resolve_ladder(page, rt);
    compose_anim(page, rt, &mut r);
    r
}

/// One property, one writer (decided 2026-08-30): an anim that owns a
/// channel — playing, or a finished one whose end pose still stands — is
/// the top of the ladder for that channel. Motion channels land in
/// `Resolved::motion` over the `when` responses; `opacity` and `color.a`
/// land as the LAST override, so `Resolved::num`/`color` answer with the
/// anim's value like any other writer. Nothing multiplies.
fn compose_anim(page: &Page, rt: &Runtime, r: &mut Resolved) {
    let mut apply = |node: NodeId, channel: Channel, v: f32| {
        match channel {
            Channel::Opacity => r.overrides.push((node, Prop { key: PropKey::Opacity, value: Value::VNum(v) })),
            Channel::ColorA => r.overrides.push((node, Prop { key: PropKey::ColorA, value: Value::VNum(v) })),
            Channel::RevealX => r.overrides.push((node, Prop { key: PropKey::RevealX, value: Value::VNum(v) })),
            Channel::RevealY => r.overrides.push((node, Prop { key: PropKey::RevealY, value: Value::VNum(v) })),
            _ => r.motion[node].set(channel, v),
        }
        r.anim_owned[node] |= channel.bit();
    };
    for (node, channel, v) in &rt.held {
        apply(*node, *channel, *v);
    }
    for p in rt.players.iter().filter(|p| !p.stopped) {
        for (node, channel) in &p.owns {
            apply(*node, *channel, rt.motion[*node].get(*channel));
        }
    }
    // The `tap_pulse` turnstile: the swing angle on the node's `swing`
    // channel; the draw walk turns it into the 3D push about the left
    // edge (`Draw::SwingPush`).
    for p in &rt.pulses {
        apply(p.node, Channel::Swing, p.angle());
    }
    for (node, key, v) in &rt.held_props {
        apply_prop_lane(page, r, *node, *key, *v);
    }
    for p in rt.players.iter().filter(|p| !p.stopped) {
        for (node, key) in &p.owns_props {
            if let Some(v) = get_pval(&rt.prop_anim, *node, *key) {
                apply_prop_lane(page, r, *node, *key, v);
            }
        }
    }
}

/// Lands one prop lane's value as the LAST override (GP40) — the anim is
/// the top of the ladder for its lane, like the motion channels. An
/// alpha-only lane arrives as a `VNum` on a color key: the ladder's hue
/// with the anim's alpha, so a `when` can restyle the hue mid-play.
fn apply_prop_lane(page: &Page, r: &mut Resolved, node: NodeId, key: PropKey, v: Value) {
    let value = match v {
        Value::VNum(a) if key.is_color() => match r.color(page, node, key) {
            Some(c) => Value::VColor(c.with_opacity(a)),
            // No declared color to fade: nothing to show.
            None => return,
        },
        v => v,
    };
    r.overrides.push((node, Prop { key, value }));
    r.anim_owned_props.push((node, key));
}

impl Resolved {
    /// Does an anim own this channel on this node right now?
    pub fn anim_owns(&self, node: NodeId, channel: Channel) -> bool {
        self.anim_owned.get(node.index()).is_some_and(|b| b & channel.bit() != 0)
    }

    /// Does an anim own this PROP lane (GP40) right now?
    pub fn anim_owns_prop(&self, node: NodeId, key: PropKey) -> bool {
        self.anim_owned_props.contains(&(node, key))
    }

    /// An automatic slot's SHOWN fraction (GP33): the driver's value this
    /// frame, or the compile-baked rest where nothing drives it.
    pub fn auto_frac(&self, page: &Page, node: NodeId) -> f32 {
        if let Some((_, f)) = self.auto.iter().find(|(n, _)| *n == node) {
            return *f;
        }
        match page.nodes[node].auto {
            Auto::Length(f) | Auto::Position(f) | Auto::Arc { frac: f, .. } | Auto::Angle { frac: f, .. } => f,
            _ => 1.0,
        }
    }
}

fn resolve_ladder(page: &Page, rt: &Runtime) -> Resolved {
    let mut r = Resolved { anim_owned: vec![0; page.nodes.len()], anim_owned_props: Vec::new(), placement: Vec::with_capacity(page.nodes.len()), overrides: Vec::new(), variant: rt.variant, scroll: rt.scroll.clone(), safe: rt.safe, motion: vec![Motion::IDENTITY; page.nodes.len()], seat: vec![NO_NODE; page.nodes.len()], states: rt.states.clone(), raised: rt.raised.clone(), locale: rt.locale.clone(), repeat_rows: rt.pools.iter().map(|p| (p.repeat, p.live.iter().map(|s| p.slots[*s as usize]).collect())).collect(), hiding: rt.players.iter().filter(|p| !p.stopped).filter_map(|p| match p.finish { Finish::HideWhen(n) => Some(n), _ => None }).collect(), auto: Vec::new(), presence: Vec::new(), caret: rt.caret, selection: rt.selection, sel_handles: rt.sel_handles };
    for (i, node) in page.nodes.iter().enumerate() {
        let id = NodeId::from_index(i);
        let mut placement = node.placement;
        // The text-only Button gets air around its label — a kind default,
        // like its font size; a Button holding its own content spells its
        // own padding. Decided here so layout and draw inset by ONE value.
        if node.kind == Kind::Button && placement.padding == [0.0; 4] && node.children.1 == 0 {
            placement.padding = BUTTON_PADDING;
        }
        // Active whens, lowest rank first; equal rank → the later line in
        // the file wins (applied last), so the winner reads off the file.
        let mut active: Vec<&When> = page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize].iter().filter(|w| rt.state_active(page, if w.of == NO_NODE { id } else { w.of }, w.state)).collect();
        active.sort_by_key(|w| (w.state.priority(), w.line));
        for w in active {
            for p in &page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize] {
                if p.key.is_placement() {
                    apply_placement(&mut placement, page, *p);
                } else {
                    r.overrides.push((id, *p));
                }
            }
        }
        r.placement.push(placement);
    }
    // Code writes win over states.
    for (id, p) in &rt.writes {
        if p.key.is_placement() {
            apply_placement(&mut r.placement[*id], page, *p);
        } else {
            r.overrides.push((*id, *p));
        }
    }
    // GP33: the driver — each control's live value maps onto its
    // automatic slots' shown fraction (a glide in flight shows its eased
    // value); a presence slot follows its custom state on the control.
    let mut auto: Vec<(NodeId, f32)> = Vec::new();
    for d in &page.drivers {
        if d.state != NO_STR {
            r.presence.push((d.node, rt.customs.iter().any(|e| *e == (d.root, d.state))));
            continue;
        }
        let shown = if let Some(g) = rt.auto_glides.iter().find(|g| g.node == d.node) {
            g.shown
        } else if let Some(v) = r.num(page, d.root, PropKey::Value) {
            if d.max > d.min { ((v - d.min) / (d.max - d.min)).clamp(0.0, 1.0) } else { 0.0 }
        } else {
            rt.auto_rest.iter().find(|(n, _)| *n == d.node).map(|(_, f)| *f).unwrap_or_else(|| baked_frac(page, d.node))
        };
        auto.push((d.node, shown));
    }
    r.auto = auto;
    fold_seats_and_motion(page, &mut r);
    r
}

/// The keys that are not props but placement facts: `x.seat = name`
/// re-parents for layout, the motion words set the draw transform.
fn fold_seats_and_motion(page: &Page, r: &mut Resolved) {
    for i in 0..page.nodes.len() {
        r.seat[i] = NO_NODE;
    }
    // A motion word DECLARED on the node line is its resting transform
    // (`Image pickerImage rotate=90` — every box takes the motion words,
    // docs/gogo-design.md "Every box takes"); a `when` response then
    // overrides it below. Needed because a declared `rotate=` compiled
    // clean and drew flat (the brush page's vertical picker lay across
    // the rail, todo-bugs.md #1). One pass over each node's own props —
    // the same walk `Resolved::prop` takes for any declared value.
    for i in 0..page.nodes.len() {
        for p in page.node_props(NodeId::from_index(i)) {
            set_motion_word(&mut r.motion[i], p.key, p.value);
        }
    }
    for (id, p) in &r.overrides {
        match p.key {
            PropKey::SeatOf => {
                r.seat[*id] = match p.value {
                    Value::VIdent(name) if page.str(name) == "home" => NO_NODE,
                    Value::VIdent(name) => page.find(page.str(name)).unwrap_or(NO_NODE),
                    _ => NO_NODE,
                };
            }
            _ => set_motion_word(&mut r.motion[*id], p.key, p.value),
        }
    }
}

/// Writes one motion word's number into the node's transform; any other
/// key or a non-number is left alone.
fn set_motion_word(m: &mut Motion, key: PropKey, value: Value) {
    let Value::VNum(n) = value else { return };
    match key {
        PropKey::Scale => m.scale = [n, n],
        PropKey::ScaleX => m.scale[0] = n,
        PropKey::ScaleY => m.scale[1] = n,
        PropKey::TranslateX => m.translate[0] = n,
        PropKey::TranslateY => m.translate[1] = n,
        PropKey::Rotate => m.rotate = n,
        _ => {}
    }
}

/// A motion key's value when no response holds it: the identity.
/// The value a key SHOWS before anything writes it — the renderer's
/// resting look. Needed so a `when held background=` on an undecorated
/// Button TWEENS from its theme gray instead of popping (the shipped
/// capture reads the theme's resting stylebox; GPAR71); opacity rests
/// at 1.
fn kind_visual_default(page: &Page, node: NodeId, key: PropKey) -> Option<Value> {
    match key {
        PropKey::Opacity => Some(Value::VNum(1.0)),
        PropKey::Background if page.nodes[node].kind == Kind::Button => Some(Value::VColor(crate::draw::DEFAULT_BUTTON_BACKGROUND)),
        _ => None,
    }
}

fn motion_default(key: PropKey) -> Option<Value> {
    match key {
        PropKey::Scale | PropKey::ScaleX | PropKey::ScaleY => Some(Value::VNum(1.0)),
        PropKey::TranslateX | PropKey::TranslateY | PropKey::Rotate => Some(Value::VNum(0.0)),
        PropKey::RevealX | PropKey::RevealY => Some(Value::VNum(1.0)),
        _ => None,
    }
}

/// A posed size key behaves as if the file had declared it.
fn apply_placement(p: &mut Placement, page: &Page, prop: Prop) {
    let len = |v: Value| match v {
        Value::VNum(n) => Len::Exact(n),
        Value::VIdent(id) if page.str(id) == "fill" => Len::Fill(1.0),
        Value::VIdent(id) if page.str(id) == "hug" => Len::Hug(0.0),
        _ => Len::Unset,
    };
    let num = |v: Value| if let Value::VNum(n) = v { n } else { 0.0 };
    // Per-axis maps (cross-axis and `start`/`end` are load errors — the
    // compile G0057 fence; an unknown word from a code write stays Unset).
    let align_h = |v: Value| match v {
        Value::VIdent(id) => match page.str(id) {
            "left" => Align::Start,
            "center" => Align::Center,
            "right" => Align::End,
            _ => Align::AlignUnset,
        },
        _ => Align::AlignUnset,
    };
    let align_v = |v: Value| match v {
        Value::VIdent(id) => match page.str(id) {
            "top" => Align::Start,
            "center" => Align::Center,
            "bottom" => Align::End,
            _ => Align::AlignUnset,
        },
        _ => Align::AlignUnset,
    };
    match prop.key {
        PropKey::Width => p.width = len(prop.value),
        PropKey::Height => p.height = len(prop.value),
        PropKey::MinWidth => p.min_width = num(prop.value),
        PropKey::MinHeight => p.min_height = num(prop.value),
        PropKey::MaxWidth => p.max_width = Some(num(prop.value)),
        PropKey::MaxHeight => p.max_height = Some(num(prop.value)),
        PropKey::Halign => p.halign = align_h(prop.value),
        PropKey::Valign => p.valign = align_v(prop.value),
        PropKey::MarginLeft => p.margin[0] = Some(num(prop.value)),
        PropKey::MarginTop => p.margin[1] = Some(num(prop.value)),
        PropKey::MarginRight => p.margin[2] = Some(num(prop.value)),
        PropKey::MarginBottom => p.margin[3] = Some(num(prop.value)),
        PropKey::Margin => p.margin = [Some(num(prop.value)); 4],
        PropKey::PaddingLeft => p.padding[0] = num(prop.value),
        PropKey::PaddingTop => p.padding[1] = num(prop.value),
        PropKey::PaddingRight => p.padding[2] = num(prop.value),
        PropKey::PaddingBottom => p.padding[3] = num(prop.value),
        PropKey::Padding => p.padding = [num(prop.value); 4],
        PropKey::Size => {
            if let Value::VSize(w, h) = prop.value {
                if let Some(w) = w {
                    p.width = Len::Exact(w);
                }
                if let Some(h) = h {
                    p.height = Len::Exact(h);
                }
            }
        }
        _ => {}
    }
}

/// The keys a `transition` rule can move — and (GP40, user call) exactly
/// what the `anim` door may drive too: ONE list, two doors.
const ANIMATABLE: &[PropKey] = &[
    PropKey::Background,
    PropKey::Color,
    PropKey::BorderColor,
    PropKey::Opacity,
    PropKey::CornerRadius,
    PropKey::BorderThickness,
    PropKey::FontSize,
    PropKey::Width,
    PropKey::Height,
    PropKey::MinWidth,
    PropKey::MinHeight,
    PropKey::MarginLeft,
    PropKey::MarginTop,
    PropKey::MarginRight,
    PropKey::RevealX,
    PropKey::RevealY,
    PropKey::MarginBottom,
    PropKey::PaddingLeft,
    PropKey::PaddingTop,
    PropKey::PaddingRight,
    PropKey::PaddingBottom,
    PropKey::ColorA,
    PropKey::Scale,
    PropKey::ScaleX,
    PropKey::ScaleY,
    PropKey::TranslateX,
    PropKey::TranslateY,
    PropKey::Rotate,
    PropKey::FontColor,
    PropKey::Angle,
];

/// May the anim door drive this key (GP40)? The `transition` list, plus
/// the live stop family — their numbers ride in the key, so the const
/// list can't carry them.
pub fn animatable(key: PropKey) -> bool {
    matches!(key, PropKey::StopColor(_) | PropKey::StopOffset(_)) || ANIMATABLE.contains(&key)
}

/// A placement field as a prop value (`None` where the file did not speak).
fn placement_value(p: &Placement, key: PropKey) -> Option<Value> {
    let len = |l: Len| match l {
        Len::Exact(n) => Some(Value::VNum(n)),
        _ => None,
    };
    match key {
        PropKey::Width => len(p.width),
        PropKey::Height => len(p.height),
        PropKey::MinWidth => Some(Value::VNum(p.min_width)),
        PropKey::MinHeight => Some(Value::VNum(p.min_height)),
        PropKey::MarginLeft => p.margin[0].map(Value::VNum),
        PropKey::MarginTop => p.margin[1].map(Value::VNum),
        PropKey::MarginRight => p.margin[2].map(Value::VNum),
        PropKey::MarginBottom => p.margin[3].map(Value::VNum),
        PropKey::PaddingLeft => Some(Value::VNum(p.padding[0])),
        PropKey::PaddingTop => Some(Value::VNum(p.padding[1])),
        PropKey::PaddingRight => Some(Value::VNum(p.padding[2])),
        PropKey::PaddingBottom => Some(Value::VNum(p.padding[3])),
        _ => None,
    }
}

/// An automatic slot's compile-baked rest fraction (GP33).
fn baked_frac(page: &Page, node: NodeId) -> f32 {
    match page.nodes[node].auto {
        Auto::Length(f) | Auto::Position(f) | Auto::Arc { frac: f, .. } | Auto::Angle { frac: f, .. } => f,
        _ => 0.0,
    }
}

/// The rule for a change on `node` (built-in state) or the view
/// (arrangements): exact pair > entering > leaving > bare. Going home with
/// no rule of its own mirrors the entering rule's timing.
fn find_rule(page: &Page, node: NodeId, from: Option<State>, to: Option<State>) -> Option<Transition> {
    // Needed so `transition base -> held` on a node matches a press: a
    // built-in flip comes in as from/to NOTHING (`None`), and `base` written
    // on a node rule means exactly that rest state (GPAR9). The view's
    // `base` is its own arrangement and never meets a `None` end.
    let rest = |x: Option<State>| x == Some(State::Arrangement(BASE)) && node != NO_NODE;
    let same = |a: Option<State>, b: Option<State>| match (a, b) {
        (Some(State::Arrangement(x)), Some(State::Arrangement(y))) => x == y,
        (a, None) if rest(a) => true,
        (None, b) if rest(b) => true,
        (a, b) => a == b,
    };
    // An entering rule to mirror: `any -> X` or `base -> X` (at the view
    // `base` is the home arrangement, on a node the rest state).
    let from_rest = |t: &Transition| t.from.is_none() || t.from == Some(State::Arrangement(BASE));
    let rules: Vec<&Transition> = page.transitions.iter().filter(|t| t.node == node).collect();
    let score = |t: &Transition| -> Option<u8> {
        match (t.from, t.to) {
            (Some(f), Some(tt)) => (same(Some(f), from) && same(Some(tt), to)).then_some(4),
            (None, Some(tt)) => same(Some(tt), to).then_some(3),
            (Some(f), None) => same(Some(f), from).then_some(2),
            (None, None) => Some(1),
        }
    };
    let best = rules.iter().filter_map(|t| score(t).map(|s| (s, **t))).max_by_key(|(s, _)| *s).map(|(_, t)| t);
    if best.is_none() {
        // Home (base) with no rule: mirror the entering rule of where we came from.
        if let Some(State::Arrangement(BASE)) = to {
            return rules.iter().find(|t| from_rest(t) && same(t.to, from)).map(|t| **t);
        }
        // Leaving a built-in state with no rule: mirror its entering rule
        // (`any -> held` or `base -> held`).
        if to.is_none() {
            return rules.iter().find(|t| from_rest(t) && same(t.to, from)).map(|t| **t);
        }
    }
    best
}

/// `when_writes`, for a PROP lane (GP40): does the flip's `when` write
/// this very key?
fn when_writes_key(page: &Page, node: NodeId, key: PropKey, who: NodeId) -> bool {
    let n = &page.nodes[node];
    page.whens[n.whens.0 as usize..(n.whens.0 + n.whens.1) as usize].iter().any(|w| {
        let owner = if w.of == NO_NODE { node } else { w.of };
        let node_state = matches!(w.state, State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled);
        let flipped = if who == NO_NODE { !node_state } else { node_state && owner == who };
        flipped && page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize].iter().any(|p| p.key == key)
    })
}

/// Does a `when` on `node` that the flip on `who` (`NO_NODE` = the view:
/// an arrangement or screen word) turns on or off write `channel`?
fn when_writes(page: &Page, node: NodeId, channel: Channel, who: NodeId) -> bool {
    let n = &page.nodes[node];
    page.whens[n.whens.0 as usize..(n.whens.0 + n.whens.1) as usize].iter().any(|w| {
        let owner = if w.of == NO_NODE { node } else { w.of };
        let node_state = matches!(w.state, State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled);
        let flipped = if who == NO_NODE { !node_state } else { node_state && owner == who };
        flipped && page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize].iter().any(|p| Channel::of_key(p.key).contains(&channel))
    })
}

/// `resolve` plus motion: starts tweens for the state changes since the
/// last call (where a `transition` rule times them) and overlays the
/// in-flight tween values. Call once per frame after `tick`.
pub fn resolve_animated(page: &Page, rt: &mut Runtime) -> Resolved {
    let changes = std::mem::take(&mut rt.changes);
    // Tweens born this resolve start at the FLIP's time, not the
    // resolve's (the ticks in between counted toward them).
    let late = std::mem::take(&mut rt.pending_change_dt);
    // A state flip whose `when` writes a property is that property's next
    // writer: a held anim end on it yields (a faded-out box that is then
    // pressed shows its `when held` look).
    if !changes.is_empty() && !rt.held.is_empty() {
        rt.held.retain(|(node, channel, _)| !changes.iter().any(|(who, _, _)| when_writes(page, *node, *channel, *who)));
    }
    if !changes.is_empty() && !rt.held_props.is_empty() {
        rt.held_props.retain(|(node, key, _)| !changes.iter().any(|(who, _, _)| when_writes_key(page, *node, *key, *who)));
    }
    // `transition … anim=<name>` (GPAR109): entering the rule's `to`
    // condition plays the named anim; leaving the condition ends it and
    // restores what it displaced (gogo_states.gd _play_state_anim).
    for (who, from, to) in &changes {
        if let Some(f) = from {
            rt.end_state_anims(*who, *f);
        }
        // `go_instant` (GP37): an instant flip's rule plays no anim.
        if *who == NO_NODE && matches!(to, Some(State::Arrangement(a)) if rt.instant_flips.contains(a)) {
            continue;
        }
        if to.is_some() {
            if let Some(rule) = find_rule(page, *who, *from, *to) {
                if rule.anim != NO_STR {
                    rt.end_state_anims(*who, to.unwrap());
                    rt.play_id(page, rule.anim, Finish::Nothing);
                    let displaced = rt.players.last().map(|p| p.owned_bases()).unwrap_or_default();
                    let displaced_props = rt.players.last().map(|p| p.owned_bases_props()).unwrap_or_default();
                    rt.state_anims.push((*who, to.unwrap(), rule.anim, displaced, displaced_props));
                }
            }
        }
    }
    let mut r = resolve_ladder(page, rt);
    // `on:show` / `on:hide` fire on ANY writer of visibility (GP19, the
    // shipped visibility_changed door in gogo_visibility_fx.gd) — a `when`
    // response, an arrangement pose or a raw code write all count; the
    // `Runtime::show` door syncs `vis_seen` so its own plays don't double.
    // What flips is what a user SEES: the node's own word folded with its
    // ancestors'. A hide defers — the node keeps drawing
    // (`Resolved::hiding`) while its `on:hide` anim plays and really hides
    // on the player's finish; a show mid-hide cancels the hide and plays
    // the show anim from the current values.
    {
        let n = page.nodes.len();
        // The ladder's own word, before the mid-hide override — the
        // override must not hide the flip it exists to serve.
        let own: Vec<bool> = (0..n).map(|i| {
            let id = NodeId::from_index(i);
            !page.repeats.iter().any(|rp| rp.template == id)
                && match page.nodes[i].auto {
                    // GP33: presence follows its live state, not the bake.
                    Auto::Presence(baked) => r.presence.iter().find(|(n, _)| *n == id).map(|(_, b)| *b).unwrap_or(baked),
                    _ => true,
                }
                && !matches!(r.ident(page, id, PropKey::Visibility), Some("gone") | Some("space_only"))
        }).collect();
        let baselined = rt.vis_seen.len().min(n);
        for i in 0..n {
            let id = NodeId::from_index(i);
            let mut a = id;
            let now = loop {
                if !own[a] {
                    break false;
                }
                let parent = page.nodes[a].parent;
                if parent == NO_NODE {
                    break true;
                }
                a = parent;
            };
            if i >= baselined {
                // First sight of this node (first resolve, or a repeat
                // clone appended since): its visibility is state, not a
                // trigger — the shipped armed gate.
                if i < rt.vis_seen.len() {
                    rt.vis_seen[i] = now;
                } else {
                    rt.vis_seen.push(now);
                }
                continue;
            }
            if rt.vis_seen[i] == now {
                continue;
            }
            let when_hiding = rt.players.iter().any(|p| !p.stopped && p.finish == Finish::HideWhen(id));
            let code_hiding = rt.players.iter().any(|p| !p.stopped && p.finish == Finish::Hide(id));
            if now {
                if code_hiding {
                    // The code door owns this window (its finish writes
                    // gone) — the shipped fx documents the same no-op.
                    continue;
                }
                rt.vis_seen[i] = true;
                rt.players.retain(|p| p.finish != Finish::HideWhen(id));
                rt.play_action(page, id, "show", Finish::Nothing);
            } else {
                if when_hiding || code_hiding {
                    // Already hiding on its own clock.
                    continue;
                }
                rt.vis_seen[i] = false;
                rt.play_action(page, id, "hide", Finish::HideWhen(id));
            }
        }
        // A hide fired or cancelled just above changes who defers — the
        // ladder's copy predates it.
        r.hiding = rt.players.iter().filter(|p| !p.stopped).filter_map(|p| match p.finish { Finish::HideWhen(h) => Some(h), _ => None }).collect();
    }
    // `go_instant` (GP37): the flip restyles but times nothing — its
    // change never meets a rule.
    let instant_flips = std::mem::take(&mut rt.instant_flips);
    let instant_view = |who: NodeId, to: &Option<State>| who == NO_NODE && matches!(to, Some(State::Arrangement(a)) if instant_flips.contains(a));
    // An arrangement flip with a timed view rule: the boxes it re-poses
    // glide to their new rects (`Runtime::glide`).
    for (who, from, to) in &changes {
        if *who == NO_NODE && instant_view(*who, to) {
            continue;
        }
        if *who == NO_NODE {
            let ease_of = |t: &Transition| if t.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(t.easing)).unwrap_or(Easing::LINEAR) };
            // A node's own EXACT pose-pair rule times that node's share
            // of the flip (GPAR72) — collected here, read by `glide`.
            rt.glide_node_rules.clear();
            for i in 0..page.nodes.len() {
                if let Some(nr) = find_rule(page, NodeId::from_index(i), *from, *to).filter(|t| t.from.is_some() && t.to.is_some()) {
                    rt.glide_node_rules.push((NodeId::from_index(i), nr.duration, ease_of(&nr)));
                }
            }
            let view = find_rule(page, NO_NODE, *from, *to);
            let (vd, ve) = view.map(|r| (r.duration, ease_of(&r))).unwrap_or((0.0, Easing::LINEAR));
            if vd > 0.0 || !rt.glide_node_rules.is_empty() {
                rt.glide_start = Some((vd, ve));
                rt.glide_late = late;
            }
        }
    }
    if !changes.is_empty() && !rt.last.is_empty() {
        for (i, node) in page.nodes.iter().enumerate() {
            let id = NodeId::from_index(i);
            let _ = node;
            for key in ANIMATABLE {
                // GP40, user call: anim beats transition — a lane a
                // player owns (or a held pose keeps) spawns no tween.
                // Asked of the runtime: the resolved here is the ladder
                // only, `compose_anim` lands the anim layer later.
                if rt.anim_owns_prop_now(id, *key) {
                    continue;
                }
                let target = if key.is_placement() { placement_value(&r.placement[i], *key) } else { r.prop(page, id, *key).or_else(|| motion_default(*key)).or_else(|| kind_visual_default(page, id, *key)) };
                let Some(target) = target else { continue };
                let current = rt.tweens.iter().find(|t| t.node == id && t.key == *key).map(|t| t.value()).or_else(|| rt.last.iter().find(|(n, p)| *n == id && p.key == *key).map(|(_, p)| p.value)).or_else(|| motion_default(*key)).or_else(|| kind_visual_default(page, id, *key));
                let Some(current) = current else { continue };
                if current == target {
                    continue;
                }
                // Which change moved this node: its own state, a watched owner's, or the view's.
                let mut rule: Option<Transition> = None;
                for (who, from, to) in &changes {
                    if instant_view(*who, to) {
                        continue;
                    }
                    let scope = if *who == NO_NODE { NO_NODE } else { *who };
                    let watches = *who == NO_NODE
                        || *who == id
                        || page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize].iter().any(|w| w.of == *who);
                    if !watches {
                        continue;
                    }
                    // A node-level exact pose pair overrides the view rule for this node.
                    // A watched owner's flip is timed by the DECLARING
                    // scope's rules (GPAR10, the shipped one-timing-per-
                    // flip); the target's own rules are the fallback.
                    let node_rule = if scope == NO_NODE { find_rule(page, id, *from, *to).filter(|t| t.from.is_some() && t.to.is_some()) } else { find_rule(page, scope, *from, *to).or_else(|| find_rule(page, id, *from, *to)) };
                    rule = node_rule.or_else(|| if scope == NO_NODE { find_rule(page, NO_NODE, *from, *to) } else { None });
                    if rule.is_some() {
                        break;
                    }
                }
                let Some(rule) = rule else { continue };
                if rule.duration <= 0.0 {
                    continue;
                }
                let easing = if rule.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(rule.easing)).unwrap_or(Easing::LINEAR) };
                rt.tweens.retain(|t| !(t.node == id && t.key == *key));
                rt.tweens.push(Tween { node: id, key: *key, from: current, to: target, elapsed: late.min(rule.duration), duration: rule.duration, easing });
            }
        }
    }
    // GP33: a driver target that moved re-arms the fraction glide by the
    // control's default feel (the endpoint-less `transition` on the
    // instance root); no rule or duration 0 snaps. A glide already
    // flying retargets from its shown value.
    for d in &page.drivers {
        if d.state != NO_STR {
            continue;
        }
        let Some(v) = r.num(page, d.root, PropKey::Value) else { continue };
        let target = if d.max > d.min { ((v - d.min) / (d.max - d.min)).clamp(0.0, 1.0) } else { 0.0 };
        if let Some(g) = rt.auto_glides.iter_mut().find(|g| g.node == d.node) {
            if (g.to - target).abs() > 1e-6 {
                g.from = g.shown;
                g.to = target;
                g.elapsed = 0.0;
            }
            continue;
        }
        let rest = rt.auto_rest.iter().find(|(n, _)| *n == d.node).map(|(_, f)| *f).unwrap_or_else(|| baked_frac(page, d.node));
        if (target - rest).abs() <= 1e-6 {
            continue;
        }
        let feel = page.transitions.iter().find(|t| t.node == d.root && t.from.is_none() && t.to.is_none());
        match feel {
            Some(rule) if rule.duration > 0.0 => {
                let easing = if rule.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(rule.easing)).unwrap_or(Easing::LINEAR) };
                rt.auto_glides.push(AutoGlide { node: d.node, from: rest, to: target, elapsed: 0.0, duration: rule.duration, easing, shown: rest });
                // This frame still shows the rest value — the glide owns
                // the travel from here.
                if let Some(e) = r.auto.iter_mut().find(|(n, _)| *n == d.node) {
                    e.1 = rest;
                }
            }
            _ => {
                if let Some(e) = rt.auto_rest.iter_mut().find(|(n, _)| *n == d.node) {
                    e.1 = target;
                }
            }
        }
    }
    // Overlay in-flight tweens (they retarget from live values by construction).
    for t in &rt.tweens {
        let v = t.value();
        if t.key.is_placement() {
            apply_placement(&mut r.placement[t.node], page, Prop { key: t.key, value: v });
        } else {
            r.overrides.push((t.node, Prop { key: t.key, value: v }));
        }
    }
    fold_seats_and_motion(page, &mut r);
    // The anim layer goes on last: an owning anim outranks a tween too.
    compose_anim(page, rt, &mut r);
    // What shows now, per node — the live value a new play starts from.
    rt.shown.resize(page.nodes.len(), Motion::IDENTITY);
    for i in 0..page.nodes.len() {
        let id = NodeId::from_index(i);
        let mut m = r.motion[i];
        m.opacity = r.num(page, id, PropKey::Opacity).unwrap_or(1.0);
        m.color_a = r.color(page, id, PropKey::Color).map(|c| c.0[3] as f32 / 255.0).unwrap_or(1.0);
        rt.shown[i] = m;
    }
    // Remember what is displayed now.
    rt.last.clear();
    for i in 0..page.nodes.len() {
        let id = NodeId::from_index(i);
        for key in ANIMATABLE {
            let v = if key.is_placement() { placement_value(&r.placement[i], *key) } else { r.prop(page, id, *key).or_else(|| motion_default(*key)) };
            if let Some(v) = v {
                rt.last.push((id, Prop { key: *key, value: v }));
            }
        }
    }
    let _ = lerp_value;
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;

    /// A stats label rewritten every few frames must not grow the page's
    /// string table (1000 writes = 1 slot), must not steal a shared
    /// string (another label showing the same words keeps them), and a
    /// same-text write leaves the page clean.
    #[test]
    fn set_text_rewrites_one_owned_slot() {
        let c = compile("View v { Label a text=\"same\"\n Label b text=\"same\" }", &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let mut page = c.page;
        let a = page.find("a").unwrap();
        let b = page.find("b").unwrap();
        let mut rt = Runtime::new(&page);
        let before = page.strings.len();
        rt.set_text(&mut page, a, "same");
        let r = resolve_animated(&page, &mut rt);
        let text_of = |page: &Page, r: &Resolved, n: NodeId| -> String { match r.prop(page, n, PropKey::Text) { Some(Value::VText { text, .. }) => page.str(text).to_string(), _ => String::new() } };
        assert_eq!(text_of(&page, &r, a), "same");
        assert_eq!(page.strings.len(), before + 1, "the first write owns a fresh slot");
        for i in 0..1000 {
            rt.set_text(&mut page, a, &format!("frame {i}"));
        }
        assert_eq!(page.strings.len(), before + 1, "later writes reuse the slot");
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(text_of(&page, &r, a), "frame 999");
        assert_eq!(text_of(&page, &r, b), "same", "the shared string was never touched");
        rt.dirty = false;
        rt.set_text(&mut page, a, "frame 999");
        assert!(!rt.dirty, "a same-text write changes nothing");
    }

    #[test]
    fn held_transition_tweens_the_background_and_retargets() {
        let c = compile("View v { Button b text=\"x\", background=#000000FF { transition any -> held duration=1, easing=linear\n when held background=#FFFFFFFF } }", &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let mut page = c.page;
        let b = page.find("b").unwrap();
        let mut rt = Runtime::new(&page);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255])));
        rt.set_state(b, HELD, true);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(rt.tweens.len(), 1);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255])), "starts from the current value");
        rt.tick(&mut page, 0.5);
        let r = resolve_animated(&page, &mut rt);
        // Half-way black->white under the one mixing law (premultiplied
        // Oklab): the perceptual middle, 99, not the byte middle.
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([99, 99, 99, 255])));
        // Releasing mid-flight: the leaving change mirrors the entering rule and retargets from 99.
        rt.set_state(b, HELD, false);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([99, 99, 99, 255])));
        rt.tick(&mut page, 0.5);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([33, 33, 33, 255])));
        rt.tick(&mut page, 0.6);
        assert!(!rt.animating());
    }

    #[test]
    fn arrangement_pose_glides_and_player_moves_channels() {
        let c = compile("arrangement wide\ntransition base -> wide duration=1\nwhen wide { bar.width = 300 }\nanim pop { bar.translate.x from=-40, to=0, duration=0.5\n and bar.opacity to=1, duration=0.5 }\nView v { Panel bar width=100, height=20, halign=left, valign=top, on:show=play(pop) }", &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let mut page = c.page;
        let bar = page.find("bar").unwrap();
        let mut rt = Runtime::new(&page);
        resolve_animated(&page, &mut rt);
        rt.go_id(ArrangementId::new(0));
        resolve_animated(&page, &mut rt);
        rt.tick(&mut page, 0.5);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.placement[bar].width, Len::Exact(200.0));
        // The entrance: translate pinned at -40 from play start, halfway at 0.25 s.
        rt.play_entrances(&page);
        assert_eq!(rt.motion[bar].translate[0], -40.0);
        rt.tick(&mut page, 0.25);
        assert!((rt.motion[bar].translate[0] + 20.0).abs() < 0.01);
        rt.tick(&mut page, 0.3);
        assert_eq!(rt.motion[bar].translate[0], 0.0);
        assert!(rt.players.is_empty());
    }
}
