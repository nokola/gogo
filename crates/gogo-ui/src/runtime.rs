//! The runtime part of a page: which states hold, the active arrangement
//! and theme variant, scroll positions. `resolve` overlays the active
//! `when` responses onto the base props in the spec's priority order; the
//! layout and draw passes read only the `Resolved` result.

use std::sync::Arc;

use crate::anim::{Channel, Easing, Finish, Motion, Player, Tween, lerp_value};
use crate::layout::{BUTTON_PADDING, Rect};
use crate::locale::{KeyName, Locale};
use crate::page::*;

pub const HELD: u8 = 1;
pub const HOVERED: u8 = 2;
pub const FOCUSED: u8 = 4;
pub const FOCUS_WITHIN: u8 = 8;
pub const SELECTED: u8 = 16;
pub const DISABLED: u8 = 32;

/// `base` — no arrangement active.
pub const BASE: u32 = u32::MAX;

/// A page build's typed arrangement handle (`enum Arrangement` in the
/// generated module) — `Runtime::go`/`toggle` take one, so a misspelled
/// arrangement is a rustc error (GPAR27, the E7 typed-handles design;
/// `Key`/`KeyName` is the translation-key half).
pub trait ArrangementName: Copy {
    fn id(self) -> u32;
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
    Color(Rgba),
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
    let mut empties: Vec<NodeId> = page.children(repeat).map(|i| i as NodeId).filter(|c| page.nodes[*c as usize].kind == Kind::Empty).collect();
    // A List-level `empty { }` (D128) is this repeat's zero-items view
    // too: it hides once rows exist — and its spacing slot goes with it —
    // exactly like the repeat's own block (GPAR96).
    let mut a = page.nodes[repeat as usize].parent;
    while a != NO_NODE {
        if page.nodes[a as usize].kind == Kind::List {
            empties.extend(page.children(a).map(|i| i as NodeId).filter(|c| page.nodes[*c as usize].kind == Kind::Empty));
            break;
        }
        a = page.nodes[a as usize].parent;
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
pub struct Runtime {
    pub arrangement: u32,
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
    /// Live repeat rows, one pool per touched repeat (`set_count` /
    /// `set_items`).
    pools: Vec<Pool>,
    /// How many arrangements the page declares — `go_id`'s refusal
    /// bound (an unknown index keeps the pose, GPAR27).
    arrangement_count: u32,
    /// Raised control states: (control root, declared state name) —
    /// `set_custom` writes, `when <name>` conditions read (GPAR116).
    customs: Vec<(NodeId, StrId)>,
    /// Each List's selection, the keys in the order they entered
    /// (repeat/List spec §5; `one` holds at most one). Kept BY KEY so a
    /// `set_items` reconcile re-finds the rows; a key with no present
    /// item is INERT — it renders nothing and reads back until
    /// reassigned or the item arrives (select-then-load).
    list_sel: Vec<(NodeId, Vec<ItemKey>)>,
    /// The focused Input's caret (node, chars) — written by the input
    /// machine, drawn by the renderer (GPAR112).
    pub caret: Option<(NodeId, u32)>,
    /// `transition … anim=<name>` players in flight or held (GPAR109):
    /// who flipped, the condition entered, the anim, and what it displaced
    /// at play start — the condition's end restores the displaced
    /// channels (gogo_states.gd `_cancel_state_anim`).
    state_anims: Vec<(NodeId, State, StrId, Vec<(NodeId, Channel, f32, bool)>)>,
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
    /// Paint-order bumps among siblings (the Canvas.ZIndex port): (node,
    /// raise) — higher paints later (on top), lower earlier (below);
    /// absent = 0, the file's order. Layout never moves.
    pub raised: Vec<(NodeId, i32)>,
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
            dirty: true,
            locale: None,
            safe: [0.0; 4],
            writes: Vec::new(),
            changes: Vec::new(),
            tweens: Vec::new(),
            held: Vec::new(),
            shown: vec![Motion::IDENTITY; page.nodes.len()],
            players: Vec::new(),
            state_anims: Vec::new(),
            finished: Vec::new(),
            pools: Vec::new(),
            arrangement_count: page.arrangements.len() as u32,
            customs: Vec::new(),
            list_sel: Vec::new(),
            caret: None,
            motion: vec![Motion::IDENTITY; page.nodes.len()],
            owned_text: vec![NO_STR; page.nodes.len()],
            last: Vec::new(),
            glide_start: None,
            glide_node_rules: Vec::new(),
            rect_tweens: Vec::new(),
            last_rects: Vec::new(),
            raised: Vec::new(),
        }
    }

    /// Something is moving: keep redrawing.
    pub fn animating(&self) -> bool {
        !self.tweens.is_empty() || !self.players.is_empty() || !self.rect_tweens.is_empty()
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
                        // The node's own pose-pair rule times its share;
                        // duration 0 snaps (GPAR72).
                        let (d, e) = self.glide_node_rules.iter().find(|(n, _, _)| *n == i as NodeId).map(|(_, d, e)| (*d, *e)).unwrap_or((duration, easing));
                        self.rect_tweens.retain(|t| t.0 != i as NodeId);
                        if d > 0.0 {
                            self.rect_tweens.push((i as NodeId, self.last_rects[i], rects[i], 0.0, d, e));
                        }
                    }
                }
            }
        }
        for (node, from, to, elapsed, duration, easing) in &mut self.rect_tweens {
            // A relayout (a resize, a text change) moves the target with it.
            *to = rects[*node as usize];
            let t = easing.apply((*elapsed / *duration).min(1.0));
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            shown[*node as usize] = Rect::new(lerp(from.x, to.x), lerp(from.y, to.y), lerp(from.w, to.w), lerp(from.h, to.h));
        }
        self.rect_tweens.retain(|t| t.3 < t.4);
        self.last_rects = shown.clone();
        shown
    }

    /// Plays `anim` on this page (`play(name)`); a newer player owns the
    /// channels it animates (WPF SnapshotAndReplace).
    pub fn play(&mut self, page: &Page, anim: StrId, finish: Finish) {
        // What each channel shows right now: a held end pose, else the
        // layer of the player that owns it, else what the last resolve
        // showed (the ladder). Computed here so a play needs no frame in
        // between to know where it starts.
        let mut live: Vec<Motion> = self.shown.clone();
        live.resize(page.nodes.len(), Motion::IDENTITY);
        for p in self.players.iter().filter(|p| !p.stopped) {
            for (node, channel) in &p.owns {
                live[*node as usize].set(*channel, self.motion[*node as usize].get(*channel));
            }
        }
        for (node, channel, v) in &self.held {
            live[*node as usize].set(*channel, *v);
        }
        let mut player = Player::new(page, anim, &mut self.motion, &live, &self.held, finish);
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
        self.players.push(player);
        self.dirty = true;
    }

    fn play_action(&mut self, page: &Page, node: NodeId, event: &str, finish: Finish) -> bool {
        let n = &page.nodes[node as usize];
        let id = page.intern_lookup(event);
        let action = page.handlers[n.handlers.0 as usize..(n.handlers.0 + n.handlers.1) as usize].iter().find(|h| h.event == id).map(|h| h.action);
        if let Some(Action::Play(anim)) = action {
            self.play(page, anim, finish);
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
            self.players.retain(|p| p.finish != Finish::Hide(node));
            self.set_visible(page, node, true);
            self.play_action(page, node, "show", Finish::Nothing);
        } else if !self.play_action(page, node, "hide", Finish::Hide(node)) {
            self.set_visible(page, node, false);
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
            matches!(res.ident(page, i as NodeId, PropKey::Visibility), Some("gone") | Some("hidden") | Some("space_only"))
        }).collect();
        drop(res);
        for i in 0..page.nodes.len() {
            let mut n = i as NodeId;
            let mut hidden = false;
            loop {
                if unseen[n as usize] {
                    hidden = true;
                    break;
                }
                let parent = page.nodes[n as usize].parent;
                if parent == NO_NODE {
                    break;
                }
                n = parent;
            }
            if hidden {
                continue;
            }
            self.play_action(page, i as NodeId, "show", Finish::Nothing);
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
                if node >= root && node < page.nodes[root as usize].subtree_end {
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
            return Err(format!("G9101: `{}` is not a repeat with a template", page.str(page.nodes[repeat as usize].name)));
        };
        if !info.keyed {
            return Err(format!("G9101: `{}` is not a keyed repeat — a counted repeat takes `set_count`", page.str(page.nodes[repeat as usize].name)));
        }
        // Keys first; every refusal returns before anything mutates, so
        // the last good rows stand.
        let leaf = |i: usize| -> Result<&ItemValue, String> {
            match (&items[i], info.by) {
                (Item::Value(v), by) if by == NO_STR => Ok(v),
                (Item::Record(_), by) if by == NO_STR => Err(format!("G9102: repeat `{}` refused items — `by $item` keys the item itself; a record keys by field (`by $item.<field>`)", page.str(page.nodes[repeat as usize].name))),
                (Item::Record(r), by) => match r.iter().find(|(k, _)| *k == page.str(by)) {
                    Some((_, v)) => Ok(v),
                    None => Err(format!("G9102: repeat `{}` refused items — item {i} has no `{}` field", page.str(page.nodes[repeat as usize].name), page.str(by))),
                },
                (Item::Value(_), by) => Err(format!("G9102: repeat `{}` refused items — `by` walks `{}` into a plain value; field access needs a record", page.str(page.nodes[repeat as usize].name), page.str(by))),
            }
        };
        for i in 0..items.len() {
            let v = leaf(i)?;
            match v {
                ItemValue::Str(_) | ItemValue::Int(_) => {}
                ItemValue::Num(_) => return Err(format!("G9102: repeat `{}` refused items — item {i}'s key is a float; keys are strings or whole numbers", page.str(page.nodes[repeat as usize].name))),
                _ => return Err(format!("G9102: repeat `{}` refused items — item {i}'s key must be a string or a whole number", page.str(page.nodes[repeat as usize].name))),
            }
            if i > 0 && std::mem::discriminant(leaf(0)?) != std::mem::discriminant(v) {
                return Err(format!("G9102: repeat `{}` refused items — mixed key types (strings and whole numbers)", page.str(page.nodes[repeat as usize].name)));
            }
            for j in 0..i {
                if leaf(j)? == v {
                    return Err(format!("G9103: repeat `{}` refused items — duplicate key {v:?} (items {j} and {i}); every row keys uniquely", page.str(page.nodes[repeat as usize].name)));
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
                    return Err(format!("G9101: repeat `{}` refused items — the item template failed for item {i}: {why}", page.str(page.nodes[repeat as usize].name)));
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
        let mut a = page.nodes[repeat as usize].parent;
        while a != NO_NODE {
            if page.nodes[a as usize].kind == Kind::List {
                if self.list_sel.iter().any(|(n, _)| *n == a) {
                    self.apply_selected(page, a);
                }
                break;
            }
            a = page.nodes[a as usize].parent;
        }
        self.dirty = true;
        Ok(())
    }

    /// The rows a List owns right now, display order: every live row of
    /// every repeat under it (statics are never rows).
    pub fn list_rows(&self, page: &Page, list: NodeId) -> Vec<NodeId> {
        let end = page.nodes[list as usize].subtree_end;
        let mut out = Vec::new();
        for p in &self.pools {
            if p.repeat > list && p.repeat < end {
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
        let name = page.str(page.nodes[list as usize].name);
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
            let node = root + site.node_off;
            let key = page.props[(page.nodes[node as usize].props.0 + site.prop_off) as usize].key;
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
        let rep = page.str(page.nodes[info.node as usize].name).to_string();
        let from = format!("{rep}/item");
        let to = format!("{rep}/{idx}");
        let end = page.nodes[root as usize].subtree_end;
        for ni in root..end {
            let nm = page.nodes[ni as usize].name;
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
            page.nodes[ni as usize].name = id;
        }
        // The template's own anims (a mark's delete/appear) exist once,
        // aimed at template nodes — each slot takes its copy, aimed at
        // its own nodes under the slot's name. Needed because a host
        // plays them per row (`play("<rep>/2/item/delete")`); without
        // the copy the play finds nothing and "finishes" the same frame.
        let t_end = page.nodes[info.template as usize].subtree_end;
        let delta = root - info.template;
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
                    t.node += delta;
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
    pub fn is_playing(&self, anim: StrId) -> bool {
        self.players.iter().any(|p| p.anim == anim && !p.stopped)
    }

    /// The anims that ended on their own since the last call, drained
    /// (GP20's `finished`). `stop()` never lands here.
    pub fn take_finished(&mut self) -> Vec<StrId> {
        std::mem::take(&mut self.finished)
    }

    /// Stops `anim` (WPF Storyboard.Stop): `restore` puts every channel it
    /// owns back to its pre-play value; the player's finish action does
    /// not run. Nothing happens when it is not playing.
    pub fn stop(&mut self, anim: StrId, restore: bool) {
        let mut any = false;
        for p in self.players.iter_mut().filter(|p| p.anim == anim) {
            p.stop(restore, &mut self.motion);
            if restore {
                // Release the channels: the ladder shows again — except
                // where the play found a held end pose, which comes back.
                for (node, channel, base, base_held) in p.owned_bases() {
                    if base_held {
                        self.held.push((node, channel, base));
                    }
                }
            } else {
                // Left where it is: the current values stand as held ends.
                for (node, channel) in &p.owns {
                    self.held.push((*node, *channel, self.motion[*node as usize].get(*channel)));
                }
            }
            any = true;
        }
        if any {
            self.players.retain(|p| !p.stopped);
            self.dirty = true;
        }
    }

    /// Ends a condition's `transition anim=` play (GPAR109): the player
    /// stops and every channel it displaced goes back to the base it had
    /// at play start — Godot tweens them home with the exit rule's
    /// timing, here they snap (no shipped page glides one yet). A channel
    /// a LIVE player owns is that player's; it is left alone.
    fn end_state_anims(&mut self, who: NodeId, state: State) {
        let ended: Vec<(NodeId, State, StrId, Vec<(NodeId, Channel, f32, bool)>)> = {
            let (out, keep) = self.state_anims.drain(..).partition(|e| e.0 == who && e.1 == state);
            self.state_anims = keep;
            out
        };
        for (_, _, anim, displaced) in ended {
            for p in self.players.iter_mut().filter(|p| p.anim == anim) {
                p.stop(false, &mut self.motion);
            }
            self.players.retain(|p| !p.stopped);
            for (n, channel, base, base_held) in displaced {
                if self.players.iter().any(|p| p.owns.contains(&(n, channel))) {
                    continue;
                }
                self.held.retain(|h| !(h.0 == n && h.1 == channel));
                self.motion[n as usize].set(channel, base);
                if base_held {
                    self.held.push((n, channel, base));
                }
            }
            self.dirty = true;
        }
    }

    /// Advances tweens and players by `dt` seconds; true while anything moves.
    pub fn tick(&mut self, page: &mut Page, dt: f32) -> bool {
        let mut moving = false;
        for t in &mut self.tweens {
            t.elapsed += dt;
        }
        for t in &mut self.rect_tweens {
            t.3 += dt;
        }
        moving |= !self.rect_tweens.is_empty();
        let before = self.tweens.len();
        self.tweens.retain(|t| t.elapsed < t.duration);
        moving |= before > 0;
        let mut finished: Vec<(Finish, Vec<(NodeId, Channel)>, StrId)> = Vec::new();
        let mut players = std::mem::take(&mut self.players);
        players.retain_mut(|p| {
            let alive = p.tick(dt, &mut self.motion);
            if !alive {
                finished.push((p.finish, p.owns.clone(), p.anim));
            }
            alive
        });
        moving |= !players.is_empty() || !finished.is_empty();
        self.players = players;
        for (f, owns, anim) in finished {
            self.finished.push(anim);
            if let Finish::Hide(node) = f {
                self.set_visible(page, node, false);
                self.motion[node as usize] = Motion::IDENTITY;
                // The hide anim may have moved DESCENDANTS too — every
                // driven channel goes back to identity, or the re-show
                // finds them displaced (GPAR110, gogo_visibility_fx.gd).
                for (n, channel) in owns {
                    self.motion[n as usize].set(channel, Motion::IDENTITY.get(channel));
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
                        self.held.push((node, channel, self.motion[node as usize].get(channel)));
                    } else {
                        self.motion[node as usize].set(channel, Motion::IDENTITY.get(channel));
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

    /// A code write of a label's text. The first write pushes a fresh
    /// string slot the node owns (never a shared interned one — another
    /// node may show the same words); later writes rewrite that slot in
    /// place. A same-text write changes nothing.
    pub fn set_text(&mut self, page: &mut Page, node: NodeId, text: &str) {
        let owned = self.owned_text[node as usize];
        let id = if owned != NO_STR {
            if page.strings[owned as usize] == text {
                return;
            }
            let slot = &mut page.strings[owned as usize];
            slot.clear();
            slot.push_str(text);
            self.dirty = true;
            owned
        } else {
            if page.strings.is_empty() {
                page.strings.push(String::new());
            }
            page.strings.push(text.to_string());
            let id = (page.strings.len() - 1) as StrId;
            self.owned_text[node as usize] = id;
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
        let name = page.str(page.nodes[root as usize].name);
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

    pub fn set_state(&mut self, node: NodeId, bit: u8, on: bool) {
        let s = &mut self.states[node as usize];
        let before = *s;
        if on {
            *s |= bit;
        } else {
            *s &= !bit;
        }
        if *s != before {
            self.dirty = true;
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
        self.states[node as usize] & bit != 0
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
            let parent = page.nodes[n as usize].parent;
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

    /// `go` for a runtime-compiled page (tests, hot reload), by the
    /// arrangement's index. An index the page never declared is refused
    /// and the pose KEEPS (the addon's printerr + early return, GPAR27).
    pub fn go_id(&mut self, arrangement: u32) {
        if arrangement != BASE && arrangement >= self.arrangement_count {
            return;
        }
        if self.arrangement != arrangement {
            let from = self.arrangement;
            self.arrangement = arrangement;
            self.dirty = true;
            self.changes.push((NO_NODE, Some(State::Arrangement(from)), Some(State::Arrangement(arrangement))));
        }
    }

    /// `go`-or-back-to-base on the typed handle (the addon's toggle).
    pub fn toggle<A: ArrangementName>(&mut self, arrangement: A) {
        self.toggle_id(arrangement.id());
    }

    /// `toggle` by index, for a runtime-compiled page.
    pub fn toggle_id(&mut self, arrangement: u32) {
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
                    n = page.nodes[n as usize].parent;
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
    pub anim_owned: Vec<u8>,
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
    /// The focused Input's caret (copied from the Runtime).
    pub caret: Option<(NodeId, u32)>,
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
            .or_else(|| page.kind_default(page.nodes[node as usize].kind, key, self.variant))?;
        Some(deref(page, self.variant, raw))
    }

    pub fn color(&self, page: &Page, node: NodeId, key: PropKey) -> Option<Rgba> {
        let mut c = match self.prop(page, node, key)? {
            Value::VColor(c) => c,
            Value::VStop { color, .. } => color,
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
            let parent = page.nodes[n as usize].parent;
            if parent == NO_NODE {
                break;
            }
            n = parent;
        }
        page.kind_default(page.nodes[node as usize].kind, key, self.variant).or_else(|| page.kind_default(Kind::Label, key, self.variant)).map(|v| deref(page, self.variant, v))
    }

    /// `font_size=`: own, inherited, or the kind default.
    pub fn font_size(&self, page: &Page, node: NodeId) -> Option<f32> {
        match self.inherited(page, node, PropKey::FontSize)? {
            Value::VNum(n) => Some(n),
            _ => None,
        }
    }

    /// `font_color=`: own, inherited, or the kind default.
    pub fn font_color(&self, page: &Page, node: NodeId) -> Option<Rgba> {
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
        if page.nodes[node as usize].auto == Auto::Presence(false) {
            return true;
        }
        self.ident(page, node, PropKey::Visibility) == Some("gone")
    }

    /// The nodes laid out and drawn under `id`: its own children that no
    /// `when` seated elsewhere, plus whoever is seated here.
    pub fn children(&self, page: &Page, id: NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = page.children(id).map(|i| i as NodeId).filter(|c| self.seat.get(*c as usize).copied().unwrap_or(NO_NODE) == NO_NODE).collect();
        if page.nodes[id as usize].kind == Kind::Seat {
            out.extend(self.seat.iter().enumerate().filter(|(_, s)| **s == id).map(|(i, _)| i as NodeId));
        }
        if page.nodes[id as usize].kind == Kind::Repeat {
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
        self.ident(page, node, PropKey::Visibility) == Some("space_only")
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
    let _ = page;
    let mut apply = |node: NodeId, channel: Channel, v: f32| {
        match channel {
            Channel::Opacity => r.overrides.push((node, Prop { key: PropKey::Opacity, value: Value::VNum(v) })),
            Channel::ColorA => r.overrides.push((node, Prop { key: PropKey::ColorA, value: Value::VNum(v) })),
            _ => r.motion[node as usize].set(channel, v),
        }
        r.anim_owned[node as usize] |= channel.bit();
    };
    for (node, channel, v) in &rt.held {
        apply(*node, *channel, *v);
    }
    for p in rt.players.iter().filter(|p| !p.stopped) {
        for (node, channel) in &p.owns {
            apply(*node, *channel, rt.motion[*node as usize].get(*channel));
        }
    }
}

impl Resolved {
    /// Does an anim own this channel on this node right now?
    pub fn anim_owns(&self, node: NodeId, channel: Channel) -> bool {
        self.anim_owned.get(node as usize).is_some_and(|b| b & channel.bit() != 0)
    }
}

fn resolve_ladder(page: &Page, rt: &Runtime) -> Resolved {
    let mut r = Resolved { anim_owned: vec![0; page.nodes.len()], placement: Vec::with_capacity(page.nodes.len()), overrides: Vec::new(), variant: rt.variant, scroll: rt.scroll.clone(), safe: rt.safe, motion: vec![Motion::IDENTITY; page.nodes.len()], seat: vec![NO_NODE; page.nodes.len()], states: rt.states.clone(), raised: rt.raised.clone(), locale: rt.locale.clone(), repeat_rows: rt.pools.iter().map(|p| (p.repeat, p.live.iter().map(|s| p.slots[*s as usize]).collect())).collect(), caret: rt.caret };
    for (i, node) in page.nodes.iter().enumerate() {
        let id = i as NodeId;
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
            apply_placement(&mut r.placement[*id as usize], page, *p);
        } else {
            r.overrides.push((*id, *p));
        }
    }
    fold_seats_and_motion(page, &mut r);
    r
}

/// The keys that are not props but placement facts: `x.seat = name`
/// re-parents for layout, the motion words set the draw transform.
fn fold_seats_and_motion(page: &Page, r: &mut Resolved) {
    for i in 0..page.nodes.len() {
        r.seat[i] = NO_NODE;
    }
    for (id, p) in &r.overrides {
        match p.key {
            PropKey::SeatOf => {
                r.seat[*id as usize] = match p.value {
                    Value::VIdent(name) if page.str(name) == "home" => NO_NODE,
                    Value::VIdent(name) => page.find(page.str(name)).unwrap_or(NO_NODE),
                    _ => NO_NODE,
                };
            }
            PropKey::Scale | PropKey::ScaleX | PropKey::ScaleY | PropKey::TranslateX | PropKey::TranslateY | PropKey::Rotate => {
                let Value::VNum(n) = p.value else { continue };
                let m = &mut r.motion[*id as usize];
                match p.key {
                    PropKey::Scale => m.scale = [n, n],
                    PropKey::ScaleX => m.scale[0] = n,
                    PropKey::ScaleY => m.scale[1] = n,
                    PropKey::TranslateX => m.translate[0] = n,
                    PropKey::TranslateY => m.translate[1] = n,
                    PropKey::Rotate => m.rotate = n,
                    _ => {}
                }
            }
            _ => {}
        }
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
        PropKey::Background if page.nodes[node as usize].kind == Kind::Button => Some(Value::VColor(crate::draw::DEFAULT_BUTTON_BACKGROUND)),
        _ => None,
    }
}

fn motion_default(key: PropKey) -> Option<Value> {
    match key {
        PropKey::Scale | PropKey::ScaleX | PropKey::ScaleY => Some(Value::VNum(1.0)),
        PropKey::TranslateX | PropKey::TranslateY | PropKey::Rotate => Some(Value::VNum(0.0)),
        _ => None,
    }
}

/// A posed size key behaves as if the file had declared it.
fn apply_placement(p: &mut Placement, page: &Page, prop: Prop) {
    let len = |v: Value| match v {
        Value::VNum(n) => Len::Exact(n),
        Value::VIdent(id) if page.str(id) == "fill" => Len::Fill(1.0),
        Value::VIdent(id) if page.str(id) == "hug" => Len::Hug(1.0),
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

/// The keys a `transition` rule can move.
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

/// Does a `when` on `node` that the flip on `who` (`NO_NODE` = the view:
/// an arrangement or screen word) turns on or off write `channel`?
fn when_writes(page: &Page, node: NodeId, channel: Channel, who: NodeId) -> bool {
    let n = &page.nodes[node as usize];
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
    // A state flip whose `when` writes a property is that property's next
    // writer: a held anim end on it yields (a faded-out box that is then
    // pressed shows its `when held` look).
    if !changes.is_empty() && !rt.held.is_empty() {
        rt.held.retain(|(node, channel, _)| !changes.iter().any(|(who, _, _)| when_writes(page, *node, *channel, *who)));
    }
    // `transition … anim=<name>` (GPAR109): entering the rule's `to`
    // condition plays the named anim; leaving the condition ends it and
    // restores what it displaced (gogo_states.gd _play_state_anim).
    for (who, from, to) in &changes {
        if let Some(f) = from {
            rt.end_state_anims(*who, *f);
        }
        if to.is_some() {
            if let Some(rule) = find_rule(page, *who, *from, *to) {
                if rule.anim != NO_STR {
                    rt.end_state_anims(*who, to.unwrap());
                    rt.play(page, rule.anim, Finish::Nothing);
                    let displaced = rt.players.last().map(|p| p.owned_bases()).unwrap_or_default();
                    rt.state_anims.push((*who, to.unwrap(), rule.anim, displaced));
                }
            }
        }
    }
    let mut r = resolve_ladder(page, rt);
    // An arrangement flip with a timed view rule: the boxes it re-poses
    // glide to their new rects (`Runtime::glide`).
    for (who, from, to) in &changes {
        if *who == NO_NODE {
            let ease_of = |t: &Transition| if t.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(t.easing)).unwrap_or(Easing::LINEAR) };
            // A node's own EXACT pose-pair rule times that node's share
            // of the flip (GPAR72) — collected here, read by `glide`.
            rt.glide_node_rules.clear();
            for i in 0..page.nodes.len() {
                if let Some(nr) = find_rule(page, i as NodeId, *from, *to).filter(|t| t.from.is_some() && t.to.is_some()) {
                    rt.glide_node_rules.push((i as NodeId, nr.duration, ease_of(&nr)));
                }
            }
            let view = find_rule(page, NO_NODE, *from, *to);
            let (vd, ve) = view.map(|r| (r.duration, ease_of(&r))).unwrap_or((0.0, Easing::LINEAR));
            if vd > 0.0 || !rt.glide_node_rules.is_empty() {
                rt.glide_start = Some((vd, ve));
            }
        }
    }
    if !changes.is_empty() && !rt.last.is_empty() {
        for (i, node) in page.nodes.iter().enumerate() {
            let id = i as NodeId;
            let _ = node;
            for key in ANIMATABLE {
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
                rt.tweens.push(Tween { node: id, key: *key, from: current, to: target, elapsed: 0.0, duration: rule.duration, easing });
            }
        }
    }
    // Overlay in-flight tweens (they retarget from live values by construction).
    for t in &rt.tweens {
        let v = t.value();
        if t.key.is_placement() {
            apply_placement(&mut r.placement[t.node as usize], page, Prop { key: t.key, value: v });
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
        let id = i as NodeId;
        let mut m = r.motion[i];
        m.opacity = r.num(page, id, PropKey::Opacity).unwrap_or(1.0);
        m.color_a = r.color(page, id, PropKey::Color).map(|c| c.0[3] as f32 / 255.0).unwrap_or(1.0);
        rt.shown[i] = m;
    }
    // Remember what is displayed now.
    rt.last.clear();
    for i in 0..page.nodes.len() {
        let id = i as NodeId;
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
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([0, 0, 0, 255])));
        rt.set_state(b, HELD, true);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(rt.tweens.len(), 1);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([0, 0, 0, 255])), "starts from the current value");
        rt.tick(&mut page, 0.5);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([128, 128, 128, 255])));
        // Releasing mid-flight: the leaving change mirrors the entering rule and retargets from 128.
        rt.set_state(b, HELD, false);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([128, 128, 128, 255])));
        rt.tick(&mut page, 0.5);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([64, 64, 64, 255])));
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
        rt.go_id(0);
        resolve_animated(&page, &mut rt);
        rt.tick(&mut page, 0.5);
        let r = resolve_animated(&page, &mut rt);
        assert_eq!(r.placement[bar as usize].width, Len::Exact(200.0));
        // The entrance: translate pinned at -40 from play start, halfway at 0.25 s.
        rt.play_entrances(&page);
        assert_eq!(rt.motion[bar as usize].translate[0], -40.0);
        rt.tick(&mut page, 0.25);
        assert!((rt.motion[bar as usize].translate[0] + 20.0).abs() < 0.01);
        rt.tick(&mut page, 0.3);
        assert_eq!(rt.motion[bar as usize].translate[0], 0.0);
        assert!(rt.players.is_empty());
    }
}
