//! The runtime part of a page: which states hold, the active arrangement
//! and theme variant, scroll positions. `resolve` overlays the active
//! `when` responses onto the base props in the spec's priority order; the
//! layout and draw passes read only the `Resolved` result.

use std::sync::Arc;

use crate::anim::{Channel, Easing, Finish, Motion, Player, Tween, get_pval, lerp_value, prop_lane};
use crate::draw::{Draw, DrawCache, DrawStats};
use crate::layout::{LayoutCache, LayoutStats, Rect, TextMeasure};
use crate::locale::{KeyName, Locale};
use crate::page::*;
use crate::tree::{Children, PlacedTree};

/// What the last resolve tells the retained layout and draw list
/// (B36-DES stage 3; `Runtime::resolve_changes`). The two find what
/// changed by comparing the look with the copy they kept
/// (`layout::LookCopy`); this is the part a compare cannot tell.
#[derive(Clone, Debug, Default)]
pub struct ResolveChanges {
    /// Counts the resolves. A cache that last compared at serial k may
    /// patch at k (nothing resolved since) or k + 1; anything else is a
    /// resolve it never compared against, so it takes the full road.
    pub serial: u64,
    /// This resolve rebuilt the whole look (`full_rebuild_needed`).
    pub full: bool,
    /// Nodes whose text was rewritten IN PLACE since the resolve before
    /// (`set_text` on a slot the node owns): the look shows the same
    /// string id before and after, so only this list says the words
    /// changed.
    pub rewritten: Vec<NodeId>,
}

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

/// A page build's typed theme-value handle (`enum Theme` in the generated
/// module, one variant per `$name = value` the page sees) — code reads
/// the file's colors through `Runtime::theme_color`, so a renamed theme
/// value is a rustc error instead of a node's color read on the side.
pub trait ThemeName: Copy {
    fn id(self) -> StrId;
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
    /// Per slot, during one `set_items`: an item of the call took it.
    /// Kept here for its capacity, like `scratch`.
    taken: Vec<bool>,
    /// The slots that hold a key, sorted by that key, during one
    /// `set_items` whose items came in another order than the rows shown.
    /// Kept here for its capacity, like `scratch`.
    by_key: Vec<u32>,
    /// The repeat's `empty { }` children (found once — the per-call scan
    /// would allocate).
    empties: Vec<NodeId>,
}

/// A repeat's pool, created on first touch.
fn new_pool(page: &Page, repeat: NodeId) -> Pool {
    let mut empties: Vec<NodeId> = page.children(repeat).filter(|c| page.nodes[*c].kind == Kind::Empty).collect();
    // A List-level `empty { }` (D128) is this repeat's zero-items view
    // too: it hides once rows exist — and its spacing slot goes with it —
    // exactly like the repeat's own block (GPAR96). Declared on purpose:
    // the List that WROTE the repeat owns it, wherever a seat moves it.
    let mut a = page.declared_parent(repeat);
    while a != NO_NODE {
        if page.nodes[a].kind == Kind::List {
            empties.extend(page.children(a).filter(|c| page.nodes[*c].kind == Kind::Empty));
            break;
        }
        a = page.declared_parent(a);
    }
    Pool { repeat, slots: Vec::new(), live: Vec::new(), keyed: Vec::new(), scratch: Vec::new(), taken: Vec::new(), by_key: Vec::new(), empties }
}

/// The key leaf of `item` (validated by `set_items` before use).
fn key_leaf<'a>(page: &'a Page, by: StrId, item: &'a Item) -> &'a ItemValue {
    match item {
        Item::Value(v) => v,
        Item::Record(r) => r.iter().find(|(k, _)| *k == page.str(by)).map(|(_, v)| v).expect("validated by set_items"),
    }
}

/// An order over row keys: whole numbers before strings, each by its own
/// value. Any total order would do; it lets `set_items` find a key's row
/// by a search and not by reading every row.
fn key_order(a: &ItemKey, b: &ItemKey) -> std::cmp::Ordering {
    match (a, b) {
        (ItemKey::Int(x), ItemKey::Int(y)) => x.cmp(y),
        (ItemKey::Str(x), ItemKey::Str(y)) => x.cmp(y),
        (ItemKey::Int(_), ItemKey::Str(_)) => std::cmp::Ordering::Less,
        (ItemKey::Str(_), ItemKey::Int(_)) => std::cmp::Ordering::Greater,
    }
}

/// `key_order` of a stored key against an item's key leaf (a leaf that is
/// no key sorts after every key; `set_items` has refused those by then).
fn leaf_order(k: &ItemKey, leaf: &ItemValue) -> std::cmp::Ordering {
    match (k, leaf) {
        (ItemKey::Int(x), ItemValue::Int(y)) => x.cmp(y),
        (ItemKey::Str(x), ItemValue::Str(y)) => x.as_str().cmp(y.as_str()),
        (ItemKey::Int(_), ItemValue::Str(_)) => std::cmp::Ordering::Less,
        (ItemKey::Str(_), ItemValue::Int(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Less,
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

/// Whether an item's field can feed the property `key`: text for a text
/// word, a number for a number, true or false for a switch, a color for
/// a color, one of its words for a word key.
///
/// Needed because a field of another type shows nothing (a whole number
/// in `text=`, text in `color=`), with no word about it; `set_items`
/// refuses the whole call instead. A property with no fixed type
/// (`value=`) takes any field.
///
/// Returns what is wrong, naming the field and the property.
fn item_fits(key: PropKey, field: &str, v: &ItemValue) -> Result<(), String> {
    let is: &str = match v {
        ItemValue::Str(_) => "text",
        ItemValue::Int(_) => "a whole number",
        ItemValue::Num(_) => "a number",
        ItemValue::Bool(_) => "true or false",
        ItemValue::Color(_) => "a color",
    };
    let number: bool = matches!(v, ItemValue::Int(_) | ItemValue::Num(_));
    let (takes, fits): (&str, bool) = if key.is_color() {
        ("a color", matches!(v, ItemValue::Color(_)))
    } else if key.is_bool() {
        ("true or false", matches!(v, ItemValue::Bool(_)))
    } else if let Some(words) = key.closed_words() {
        return match v {
            ItemValue::Str(s) if words.contains(&s.as_str()) => Ok(()),
            ItemValue::Str(s) => Err(format!("field `{field}` is `{s}` where `{}=` takes {}", key.word(), words.join("/"))),
            _ => Err(format!("field `{field}` is {is} where `{}=` takes {}", key.word(), words.join("/"))),
        };
    } else if key.is_number() {
        ("a number", number)
    } else if key.is_placement() {
        ("a number or a size word", number || matches!(v, ItemValue::Str(_)))
    } else if matches!(key, PropKey::Text | PropKey::PlaceholderText | PropKey::AccessibilityName | PropKey::Font | PropKey::Source) {
        ("text", matches!(v, ItemValue::Str(_)))
    } else {
        return Ok(());
    };
    if fits { Ok(()) } else { Err(format!("field `{field}` is {is} where `{}=` takes {takes}", key.word())) }
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
    /// Per node: scroll offset (Scroll nodes). Code writes it through
    /// `set_scroll`; a raw write never dirties or clamps (B31).
    pub scroll: Vec<[f32; 2]>,
    /// Set by anything that changes what layout/draw would produce.
    pub dirty: bool,
    /// Scroll nodes whose offset code just set (`set_scroll`): the input
    /// tick clamps each to its content range once the node is idle and
    /// drops it from here. Not clamped at the write — the range needs a
    /// layout — and never at layout, which holds a copy of this runtime
    /// and cannot write back (B31).
    pub(crate) scroll_clamp_pending: Vec<NodeId>,
    /// `reset_view_state` asked the input machine to forget its focus
    /// owner; `Input::tick` reads and clears it. The owner lives on
    /// `Input`, the focus bits and caret here — so the reset has two
    /// halves.
    pub(crate) focus_reset_pending: bool,
    /// A plain copy of `Resolved::seat` from the last resolve
    /// (`resolve_retained`, the road every host's frame takes;
    /// `resolve_animated` wraps it). `disabled` and the state
    /// chains walk PLACED parents from it (B25), because they run inside
    /// the resolve that builds the next table, or from doors that hold
    /// no `Resolved` (`Input::reset`, `unhover`). Empty until the first
    /// frame: every node at its declared home.
    pub(crate) last_seat: Vec<NodeId>,
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
    /// Per property rule (`Page::prop_transitions`, same index): the
    /// standing value the last resolve saw — a change from it starts the
    /// rule's slide. `None` = not seen yet (the first value never slides).
    prop_standing: Vec<Option<Value>>,
    /// `set_prop_instant` writes since the last resolve: the value their
    /// property rule's slide starts from instead of the shown one.
    instant_props: Vec<(NodeId, PropKey, Value)>,
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
    /// Control roots whose value was written and whose readers (the
    /// properties written `$value` in the control's body,
    /// `Page::value_follows`) have not been written yet.
    ///
    /// Needed because a value is written by callers holding only `&Page`
    /// (the input machine, a plain `set_prop`), and rewriting a label
    /// needs `&mut Page`: the readers wait for `tick`, the frame door
    /// that has it. One frame late at most — a drag streams anyway.
    pub value_follow: Vec<NodeId>,
    /// Lists whose `select` was written from code and whose selection has
    /// not been brought in line with the new mode yet.
    ///
    /// Needed because `set_prop` holds no page and a List's rows are
    /// found through one: the selection waits for `tick`, as a value's
    /// readers do.
    pub select_follow: Vec<NodeId>,
    /// The string a following label's number is written into before it
    /// is copied to the label. Kept so a value written on every finger
    /// move of a drag allocates nothing.
    follow_text: String,
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
    /// Per node, what SHOWED after the last resolve: the
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
    /// The current look (B36-DES stage 2): what the last
    /// `resolve_retained` left, kept between frames and patched in
    /// place by the next one — so a flip re-resolves the nodes it
    /// reaches and nothing else, and no snapshot is rebuilt per frame.
    /// It is also "what was shown": a state flip's tween seeding starts
    /// from the values in here before the flip is applied. Empty until
    /// the first resolve. Hosts read it through `resolved()`.
    resolved: Resolved,
    /// The retained resolve's bookkeeping: the reverse table (which
    /// nodes a flip reaches), the ladder's own per-node answers before
    /// the tail, what the last resolve saw, and the scratch buffers.
    retained: Retained,
    /// B36-DES stage (1): per node, can nothing at run time change its
    /// resolved look? `true` = static: its placement, overrides, seat,
    /// motion and visibility word are the file's, so every per-node loop
    /// in `resolve` skips it and fills the declared result. The rule is
    /// `static_nodes`; the switch is `debug_set_static_mask`; the
    /// question every loop asks is `is_static`.
    static_node: Vec<bool>,
    /// Per node: a code write stands on it (`writes` holds an entry for
    /// it). Kept beside `writes` (set in `set_prop`, cleared in
    /// `clear_prop`) because a static node with a write is promoted to
    /// dynamic for as long as the write stands, and the loops need a
    /// flag, not a scan of `writes` per node.
    written: Vec<bool>,
    /// The gate's switch: off = every node dynamic, the road before the
    /// mask (the equivalence pin runs both).
    static_mask_on: bool,
    /// The declared looks the skipped loops fill in for static nodes,
    /// derived once per page and theme variant (`refresh_static_look`).
    static_look: DeclaredLook,
    /// The last resolve's report for the retained layout and draw list
    /// (B36-DES stage 3): `resolve_changes`.
    resolve_changes: ResolveChanges,
    /// Nodes whose text `set_text` rewrote in place since the last
    /// resolve — moved into `resolve_changes.rewritten` there.
    pending_rewrites: Vec<NodeId>,
    /// The retained layout and draw list (B36-DES stage 3):
    /// `layout_retained`, `draw_retained`.
    retained_layout: LayoutCache,
    retained_draw: DrawCache,
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
            scroll_clamp_pending: Vec::new(),
            focus_reset_pending: false,
            last_seat: Vec::new(),
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
            prop_standing: Vec::new(),
            instant_props: Vec::new(),
            held: Vec::new(),
            prop_anim: Vec::new(),
            held_props: Vec::new(),
            value_follow: Vec::new(),
            select_follow: Vec::new(),
            follow_text: String::new(),
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
            resolved: Resolved::default(),
            retained: Retained::new(page),
            static_node: static_nodes(page, 0),
            written: vec![false; page.nodes.len()],
            static_mask_on: true,
            static_look: DeclaredLook::default(),
            resolve_changes: ResolveChanges::default(),
            pending_rewrites: Vec::new(),
            retained_layout: LayoutCache::default(),
            retained_draw: DrawCache::default(),
            glide_start: None,
            glide_node_rules: Vec::new(),
            rect_tweens: Vec::new(),
            last_rects: Vec::new(),
            raised: Vec::new(),
            pulses: Vec::new(),
        }
    }

    /// A runtime for `new_page` — a fresh compile of this page's source
    /// with the same node list (`reload::same_shape`) — carrying THIS
    /// runtime's state, so a hot reload lands on the same screen in the
    /// same state: the locale and safe insets, every scroll offset, every
    /// node's state bits, the theme variant and screen words, the
    /// arrangement, every raised control state, and every code write
    /// (`writes`). Strings those carry index the OLD page's pool and are
    /// re-interned into the new one; a write whose value indexes the old
    /// page's value pool (`VList`, `VCall`) cannot move and is dropped.
    ///
    /// Every field is named below on purpose: a field added to `Runtime`
    /// does not compile until it is listed as carried or as reset, so a
    /// reload never silently forgets a new piece of state.
    pub fn carried_to(&self, old_page: &Page, new_page: &mut Page) -> Runtime {
        let Runtime {
            // Carried: what the user and the code-behind put here.
            arrangement,
            variant,
            screen,
            states,
            scroll,
            locale,
            safe,
            writes,
            customs,
            // Reset — rebuilt from the new page by `new`.
            arrangement_count: _,
            radio_choice: _,
            toggle: _,
            auto_rest: _,
            owned_text: _,
            // Reset — derived again by the next resolve, layout and draw.
            dirty: _,
            last_seat: _,
            resolved: _,
            retained: _,
            retained_layout: _,
            retained_draw: _,
            static_node: _,
            static_look: _,
            static_mask_on: _,
            written: _,
            resolve_changes: _,
            pending_rewrites: _,
            shown: _,
            last_rects: _,
            vis_seen: _,
            prop_standing: _,
            // Reset — motion and gestures in flight end with the old page.
            changes: _,
            pending_change_dt: _,
            glide_late: _,
            tweens: _,
            instant_props: _,
            players: _,
            motion: _,
            held: _,
            prop_anim: _,
            held_props: _,
            value_follow: _,
            select_follow: _,
            follow_text: _,
            state_anims: _,
            finished: _,
            pulses: _,
            glide_start: _,
            glide_node_rules: _,
            rect_tweens: _,
            instant_flips: _,
            pending_settle: _,
            scroll_clamp_pending: _,
            focus_reset_pending: _,
            auto_glides: _,
            raised: _,
            // Reset — not carried yet (the D113 limits): repeat rows code
            // set (`set_items` — the page shows its template until the code
            // sets them again), List selections, the focused Input's caret,
            // selection and edit bar.
            pools: _,
            list_sel: _,
            caret: _,
            selection: _,
            sel_handles: _,
            edit_bar: _,
        } = self;
        let mut rt = Runtime::new(new_page);
        rt.set_locale(locale.clone());
        rt.safe = *safe;
        rt.scroll = scroll.clone();
        rt.states = states.clone();
        rt.variant = reintern(old_page, new_page, *variant);
        rt.screen = screen.iter().map(|s| reintern(old_page, new_page, *s)).collect();
        rt.customs = customs.iter().map(|(node, state)| (*node, reintern(old_page, new_page, *state))).collect();
        if arrangement.index() < new_page.arrangements.len() {
            rt.go_instant_id(*arrangement);
        }
        for (node, prop) in writes {
            let value = match prop.value {
                Value::VText { text, key } => Value::VText { text: reintern(old_page, new_page, text), key },
                Value::VIdent(s) => Value::VIdent(reintern(old_page, new_page, s)),
                Value::VRef { name, opacity } => Value::VRef { name: reintern(old_page, new_page, name), opacity },
                Value::VList(..) | Value::VCall { .. } => continue,
                other => other,
            };
            rt.set_prop_instant(*node, prop.key, value);
        }
        rt.dirty = true;
        rt
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
        let mut shown: Vec<Rect> = rects;
        self.glide_in_place(&mut shown);
        shown
    }

    /// `glide` over the caller's buffer: `rects` holds the laid-out rects
    /// on entry and the shown ones on exit — the frame road's version
    /// (`frame_retained`), one copy fewer than `glide` (audit finding 7).
    pub fn glide_in_place(&mut self, rects: &mut Vec<Rect>) {
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
            // A relayout (a resize, a text change) moves the target with it
            // — `rects[node]` is still the laid-out rect here, each node
            // tweens once.
            *to = rects[*node];
            let t = easing.apply((*elapsed / *duration).min(1.0));
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            rects[*node] = Rect::new(lerp(from.x, to.x), lerp(from.y, to.y), lerp(from.w, to.w), lerp(from.h, to.h));
        }
        self.glide_late = 0.0;
        self.rect_tweens.retain(|t| t.3 < t.4);
        self.last_rects.clone_from(rects);
        // A `%` anim endpoint on a node that had no box at play (hidden
        // then) reads its fraction now that the node laid out — into the
        // anim layer; `compose_motion` carries it to the look this frame.
        for p in &mut self.players {
            if p.resolve_pending_percent(&self.last_rects, &mut self.motion) {
                self.dirty = true;
            }
        }
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
                } else if key.is_placement() {
                    // What the layout holds: the written number; for a
                    // `fill` or `hug` side the size the box was last laid
                    // out at; else 0 (a margin or padding nobody wrote).
                    let laid_out: Option<&Rect> = self.last_rects.get(node.index());
                    placement_value(&r.placement[node.index()], key)
                        .or_else(|| match key {
                            PropKey::Width => laid_out.map(|b| Value::VNum(b.w)),
                            PropKey::Height => laid_out.map(|b| Value::VNum(b.h)),
                            _ => None,
                        })
                        .unwrap_or(Value::VNum(0.0))
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
    /// `visible_now` for the node AND every PLACED ancestor (the tree it
    /// draws under): a node under a hidden parent is off screen whatever
    /// its own visibility says. Needed by an anim's finish, which holds its
    /// end pose only for a target on screen (GPAR85) — a child of a hidden
    /// pooled item held its pose otherwise.
    fn shown_with_ancestors(&self, page: &Page, node: NodeId) -> bool {
        let mut n: NodeId = node;
        while n != NO_NODE && n.index() < page.nodes.len() {
            if !self.visible_now(page, n) {
                return false;
            }
            n = self.placed_parent(page, n);
        }
        true
    }

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
        // Placed parents: a node seated under a hidden host is not drawn
        // either, whatever its declared ancestors show (B25).
        let placed: Vec<NodeId> = (0..page.nodes.len()).map(|i| res.placed_parent(page, NodeId::from_index(i))).collect();
        drop(res);
        for i in 0..page.nodes.len() {
            let mut n = NodeId::from_index(i);
            let mut hidden = false;
            loop {
                if unseen[n] {
                    hidden = true;
                    break;
                }
                let parent = placed[n];
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
                if page.is_under(node, root) { // declared on purpose: a row's key is what the template WROTE under it, seats aside
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
        // A cloned Radio joins its template's group and a cloned Toggle
        // derives like its template (GP37). Both tables must stay as long as
        // the arena: a code pick walks every node to clear its group — on
        // a page with a `choice` whose repeat had cloned any rows (a
        // capacity reserves room, it builds none — the stickers page's
        // sheet cells) the walk indexed past their end.
        let from = self.radio_choice.len();
        self.radio_choice.extend(page.nodes[from..].iter().map(|nd| if nd.kind == Kind::Radio { nd.choice } else { NO_CHOICE }));
        let from = self.toggle.len();
        self.toggle.extend(page.nodes[from..].iter().map(|nd| nd.kind == Kind::Toggle));
        // The new rows take the static rule like the template did — the
        // clone copies the template's `when`s, shares its props and
        // handlers, and the slot's anims and drivers are already pushed
        // (`grow_pool` runs this last); a slot's bound values arrive as
        // code writes, the promotion case.
        if self.static_node.len() < n {
            let from = self.static_node.len();
            self.static_node.extend(static_nodes(page, from));
        }
        self.written.resize(n, false);
        // The new rows' `when`s join the reverse table; the row count
        // change itself makes the next resolve a full rebuild
        // (`full_rebuild_needed`).
        self.retained.watchers.extend(page);
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
        // The first item whose key is wrong, and why. A duplicate among
        // the items before it is reported first, as when the items were
        // read one by one.
        let mut stop: Option<(usize, String)> = None;
        for i in 0..items.len() {
            let wrong: Option<String> = match leaf(i) {
                Err(why) => Some(why),
                Ok(ItemValue::Str(_)) | Ok(ItemValue::Int(_)) if i > 0 && std::mem::discriminant(leaf(0)?) != std::mem::discriminant(leaf(i)?) => Some(format!("G9102: repeat `{}` refused items — mixed key types (strings and whole numbers)", page.str(page.nodes[repeat].name))),
                Ok(ItemValue::Str(_)) | Ok(ItemValue::Int(_)) => None,
                Ok(ItemValue::Num(_)) => Some(format!("G9102: repeat `{}` refused items — item {i}'s key is a float; keys are strings or whole numbers", page.str(page.nodes[repeat].name))),
                Ok(_) => Some(format!("G9102: repeat `{}` refused items — item {i}'s key must be a string or a whole number", page.str(page.nodes[repeat].name))),
            };
            if let Some(why) = wrong {
                stop = Some((i, why));
                break;
            }
        }
        // Duplicates: the items with a good key, sorted by key once, then
        // neighbours compared. Reading every pair took a second for 1000
        // rows in a debug build.
        let checked: usize = stop.as_ref().map_or(items.len(), |(i, _)| *i);
        let pi = self.pool_of(page, repeat);
        let mut order: Vec<u32> = std::mem::take(&mut self.pools[pi].by_key);
        order.clear();
        order.extend(0..checked as u32);
        let leaf_cmp = |x: u32, y: u32| -> std::cmp::Ordering {
            match (leaf(x as usize), leaf(y as usize)) {
                (Ok(ItemValue::Int(a)), Ok(ItemValue::Int(b))) => a.cmp(b),
                (Ok(ItemValue::Str(a)), Ok(ItemValue::Str(b))) => a.as_str().cmp(b.as_str()),
                _ => std::cmp::Ordering::Equal,
            }
        };
        order.sort_unstable_by(|x, y| leaf_cmp(*x, *y).then(x.cmp(y)));
        // Of every pair that shares a key, the one whose second item
        // comes first in the list.
        let mut twice: Option<(u32, u32)> = None;
        for pair in order.windows(2) {
            if leaf_cmp(pair[0], pair[1]) == std::cmp::Ordering::Equal && twice.is_none_or(|(_, i)| pair[1] < i) {
                twice = Some((pair[0], pair[1]));
            }
        }
        self.pools[pi].by_key = order;
        if let Some((j, i)) = twice {
            let v: &ItemValue = leaf(i as usize)?;
            return Err(format!("G9103: repeat `{}` refused items — duplicate key {v:?} (items {j} and {i}); every row keys uniquely", page.str(page.nodes[repeat].name)));
        }
        if let Some((_, why)) = stop {
            return Err(why);
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
                // The field is of the type its property takes. Checked
                // here, the one place every bound field of every item
                // passes, because the build cannot know an item's types.
                let site: BindSite = page.bind_sites[si as usize];
                let node: NodeId = info.template.offset(site.node_off);
                let key: PropKey = page.props[(page.nodes[node].props.0 + site.prop_off) as usize].key;
                let value: &ItemValue = match item {
                    Item::Record(r) => &r.iter().find(|(k, _)| *k == fname).expect("the field is there, checked above").1,
                    Item::Value(v) => v,
                };
                if let Err(why) = item_fits(key, fname, value) {
                    return Err(format!("G9101: repeat `{}` refused items — the item template failed for item {i}: {why}", page.str(page.nodes[repeat].name)));
                }
            }
        }
        // Commit — cannot fail. A row follows its KEY: an item whose key
        // is on a row keeps that row, bound again when its data changed,
        // so what is on the row (a pressed, selected or focused look)
        // stays with its item through an edit or a re-sort. A new key
        // takes a free row.
        let mut next: Vec<u32> = std::mem::take(&mut self.pools[pi].scratch);
        let mut taken: Vec<bool> = std::mem::take(&mut self.pools[pi].taken);
        let mut by_key: Vec<u32> = std::mem::take(&mut self.pools[pi].by_key);
        next.clear();
        taken.clear();
        taken.resize(self.pools[pi].slots.len(), false);
        by_key.clear();
        let mut sorted: bool = false;
        for (i, item) in items.iter().enumerate() {
            let lv: &ItemValue = key_leaf(page, info.by, item);
            let pool: &Pool = &self.pools[pi];
            let key_of = |s: u32| -> &ItemKey { &pool.keyed[s as usize].as_ref().expect("a slot in the index holds a key").0 };
            // First guess: the row shown at this place last time. Items
            // that kept their order are matched in one pass, with no
            // search.
            let mut found: Option<u32> = pool.live.get(i).copied().filter(|s| pool.keyed[*s as usize].as_ref().is_some_and(|(k, _)| key_matches(k, lv)));
            if found.is_none() {
                // Another order: the rows that hold a key, sorted once,
                // and one search per item.
                if !sorted {
                    by_key.extend((0..pool.slots.len() as u32).filter(|s| pool.keyed[*s as usize].is_some()));
                    by_key.sort_unstable_by(|x, y| key_order(key_of(*x), key_of(*y)));
                    sorted = true;
                }
                found = by_key.binary_search_by(|s| leaf_order(key_of(*s), lv)).ok().map(|at| by_key[at]);
            }
            match found.filter(|s| !taken[*s as usize]) {
                Some(s) => {
                    taken[s as usize] = true;
                    next.push(s);
                }
                None => next.push(u32::MAX),
            }
        }
        let mut free: usize = 0;
        for i in 0..items.len() {
            let s: u32 = if next[i] != u32::MAX {
                // The key's own row: untouched when the data is the same.
                if self.pools[pi].keyed[next[i] as usize].as_ref().is_some_and(|(_, d)| d == &items[i]) {
                    continue;
                }
                next[i]
            } else {
                while free < taken.len() && taken[free] {
                    free += 1;
                }
                let s: u32 = if free < taken.len() {
                    free as u32
                } else {
                    let s: u32 = self.grow_pool(page, pi, &info);
                    taken.resize(self.pools[pi].slots.len(), false);
                    s
                };
                taken[s as usize] = true;
                next[i] = s;
                s
            };
            self.bind_slot(page, pi, s, &info, &items[i]);
        }
        // A row no item took is free and forgets its key, so a key is
        // on one row at most.
        for (s, was_taken) in taken.iter().enumerate() {
            if !*was_taken {
                self.pools[pi].keyed[s] = None;
            }
        }
        self.pools[pi].taken = taken;
        self.pools[pi].by_key = by_key;
        let zero = next.is_empty();
        let prev = std::mem::replace(&mut self.pools[pi].live, next);
        self.pools[pi].scratch = prev;
        self.show_empties(page, pi, zero);
        // The selection is kept BY KEY across reconciles (list.reorder-keeps):
        // re-assert it over the fresh rows; absent keys stay INERT in the
        // List's memory and re-attach when their item returns. Declared
        // on purpose: the List that WROTE the repeat owns its selection.
        let mut a = page.declared_parent(repeat);
        while a != NO_NODE {
            if page.nodes[a].kind == Kind::List {
                if self.list_sel.iter().any(|(n, _)| *n == a) {
                    self.apply_selected(page, a);
                }
                break;
            }
            a = page.declared_parent(a);
        }
        self.dirty = true;
        Ok(())
    }

    /// The rows a List owns right now, display order: every live row of
    /// every repeat under it (statics are never rows).
    pub fn list_rows(&self, page: &Page, list: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        for p in &self.pools {
            if p.repeat != list && page.is_under(p.repeat, list) { // declared on purpose: a repeat belongs to the List that wrote it
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
        let mode: &str = self.select_mode(page, list);
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

    /// A List's `select` mode: the word code wrote last, else the file's,
    /// else `one`.
    fn select_mode<'p>(&self, page: &'p Page, list: NodeId) -> &'p str {
        let written: Option<Value> = self.writes.iter().rev().find(|(n, p)| *n == list && p.key == PropKey::Select).map(|(_, p)| p.value);
        match written.or_else(|| page.prop(list, PropKey::Select)) {
            Some(Value::VIdent(w)) => page.str(w),
            _ => "one",
        }
    }

    /// Brings a List's selection in line with its `select` mode after
    /// code changed the mode: `off` holds no selection, `one` holds the
    /// first selected row in the List's order, `many` keeps them all.
    ///
    /// Needed because a tap only ever adds or flips one row: without this
    /// the rows picked under the old mode stay picked for good under
    /// `off`, and two stay picked under `one`.
    fn settle_selection(&mut self, page: &Page, list: NodeId) {
        let keep: usize = match self.select_mode(page, list) {
            "off" => 0,
            "one" => 1,
            _ => return,
        };
        let keys: Vec<ItemKey> = self.selected_keys(page, list);
        if keys.len() <= keep {
            return;
        }
        if let Some(e) = self.list_sel.iter_mut().find(|(n, _)| *n == list) {
            e.1 = keys[..keep].to_vec();
        }
        self.apply_selected(page, list);
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
        let mode: &str = self.select_mode(page, list);
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
                        // A word key (`visibility=`, `halign=`) and a size
                        // word (`width=fill`) take a bare word, not text.
                        let word: bool = key.closed_words().is_some() || key.is_placement();
                        self.set_prop(node, key, if word { Value::VIdent(id) } else { Value::VText { text: id, key: NO_KEY } });
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
        let cloned: Cloned = page.clone_subtree(info.template);
        let root: NodeId = cloned.root;
        let rep = page.str(page.nodes[info.node].name).to_string();
        let from = format!("{rep}/item");
        let to = format!("{rep}/{idx}");
        // The clone's driven slots start at their baked rest like the
        // template's did at `Runtime::new` — a landed glide writes its
        // rest here, and without an entry the next resolve re-armed the
        // glide from the baked fraction, forever.
        let rests: Vec<(NodeId, f32)> = page.drivers.iter().filter(|d| d.state == NO_STR && cloned.contains(d.node)).map(|d| (d.node, baked_frac(page, d.node))).collect();
        self.auto_rest.extend(rests);
        for ni in cloned.ids() {
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
                if page.is_under(t.node, info.template) { // declared on purpose: the template's own body, as written
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
    /// Is any motion pump armed — a `when`/arrangement glide, a driven
    /// slot's glide, a tween, an anim player, a tap pulse, a flip not
    /// yet resolved? Needed by the frame loop's idle gate AFTER the
    /// layout pass: glides are armed there (`resolve_animated`, `glide`),
    /// so what `tick` reported at the frame's start misses a glide this
    /// frame's write started — and a desktop loop that then WAITS for an
    /// OS event froze it mid-flight (the effect page's switch settle, a
    /// code-opened band). The Godot addon polled its `gogo_anims` group
    /// the same way: every pump keeps its membership current.
    pub fn is_moving(&self) -> bool {
        !self.rect_tweens.is_empty() || !self.auto_glides.is_empty() || !self.tweens.is_empty() || !self.players.is_empty() || !self.pulses.is_empty() || !self.changes.is_empty()
    }

    pub fn tick(&mut self, page: &mut Page, dt: f32) -> bool {
        let mut moving = false;
        // The controls whose value was written since the last tick: their
        // `$value` readers are written now (`value_follow`).
        if !self.value_follow.is_empty() {
            let mut roots: Vec<NodeId> = std::mem::take(&mut self.value_follow);
            for root in roots.drain(..) {
                let v = self.writes.iter().find(|(n, w)| *n == root && w.key == PropKey::Value).and_then(|(_, w)| if let Value::VNum(v) = w.value { Some(v) } else { None });
                if let Some(v) = v {
                    self.follow_value(page, root, v);
                }
            }
            // The emptied list goes back, so its room is kept for the
            // next write.
            if self.value_follow.is_empty() {
                self.value_follow = roots;
            }
        }
        // The Lists whose `select` was written since the last tick: their
        // selection is brought in line with the new mode.
        if !self.select_follow.is_empty() {
            let mut lists: Vec<NodeId> = std::mem::take(&mut self.select_follow);
            for list in lists.drain(..) {
                self.settle_selection(page, list);
            }
            self.select_follow = lists;
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
                    if self.shown_with_ancestors(page, node) {
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
                        if self.shown_with_ancestors(page, node) {
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
        // `disabled` and `selected` are STATES. The file's word is read
        // once, at build, into the node's starting state, so a write that
        // only stored the value changed nothing on screen. The write sets
        // the state: one store, the one `set_state` writes, so the two
        // calls cannot disagree.
        match key {
            PropKey::Disabled => return self.set_state(node, DISABLED, value == Value::VBool(true)),
            PropKey::SelectedProp => return self.set_state(node, SELECTED, value == Value::VBool(true)),
            _ => {}
        }
        // `keyboard_target` is read when the page is built and nothing
        // reads it after: a write here would be accepted and dropped.
        debug_assert!(key != PropKey::KeyboardTarget, "`keyboard_target` is read when the page is built — a write while the page runs changes nothing; write it in the `.gogo` file");
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
            // The write promotes the node: a static node with a code
            // write resolves the long way while the write stands.
            if self.written.len() <= node.index() {
                self.written.resize(node.index() + 1, false);
            }
            self.written[node] = true;
        }
        // A write is a flip on its own node for the retained resolve:
        // the node re-runs its ladder (its static/dynamic standing may
        // have changed) and its shown values are re-read.
        self.retained.write_flips.push(node);
        self.dirty = true;
        // A changed value: the properties written `$value` in this
        // control's body are owed a write, at the next `tick`. Needed so
        // every road that writes a value moves its readers, not only
        // `set_value`.
        if key == PropKey::Value && !self.value_follow.contains(&node) {
            self.value_follow.push(node);
        }
        // A changed `select`: the rows the new mode could not have picked
        // let go at the next `tick` (`select_follow`).
        if key == PropKey::Select && !self.select_follow.contains(&node) {
            self.select_follow.push(node);
        }
    }

    /// A theme color as the page shows it now — the active variant's value
    /// (`theme dark { $accent = … }`), else the default, `$refs` followed
    /// the way `color=$accent` in the file follows them. Needed so code
    /// that paints with the file's colors (an outline drawn on the GPU)
    /// follows theme switches.
    ///
    /// Returns `None` when the value isn't a color.
    pub fn theme_color<T: ThemeName>(&self, page: &Page, name: T) -> Option<Color_Straight_Srgb_4xU8_RGBA> {
        match deref(page, self.variant, Value::VRef { name: name.id(), opacity: None }) {
            Value::VColor(c) => Some(c),
            _ => None,
        }
    }

    /// `theme_color`'s number twin: the active variant's number for the
    /// value (else the default block's), `$refs` followed; `None` when it
    /// is not a number. So code that sizes by the file's numbers
    /// (`$backBoxHeight`) reads them by name instead of repeating them.
    pub fn theme_number<T: ThemeName>(&self, page: &Page, name: T) -> Option<f32> {
        match deref(page, self.variant, Value::VRef { name: name.id(), opacity: None }) {
            Value::VNum(n) => Some(n),
            _ => None,
        }
    }

    /// A code write that lands at once, even where a property rule
    /// (`transition commandBar.opacity duration=0.4`) would slide it — the
    /// `go_instant` of property writes. A `set_prop` of the same property
    /// before the next frame slides from this value: a bar shown faded out
    /// and then faded in is `set_prop_instant(opacity, 0)` +
    /// `set_prop(opacity, 1)`. Without a rule it is `set_prop`.
    pub fn set_prop_instant(&mut self, node: NodeId, key: PropKey, value: Value) {
        match self.instant_props.iter_mut().find(|(n, k, _)| *n == node && *k == key) {
            Some(e) => e.2 = value,
            None => self.instant_props.push((node, key, value)),
        }
        self.set_prop(node, key, value);
        // A same-value write changes no write, but a slide toward it must
        // still stop.
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
            // The last write gone = the node is back to its declared look.
            if let Some(w) = self.written.get_mut(node.index()) {
                *w = self.writes.iter().any(|(n, _)| *n == node);
            }
            self.retained.write_flips.push(node);
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
            // The look keeps the same string id: the retained layout
            // learns of the new words from this list alone.
            self.pending_rewrites.push(node);
            owned
        } else {
            if page.strings.is_empty() {
                page.strings.push(String::new());
            }
            page.strings.push(text.to_string());
            let id = StrId::from_index(page.strings.len() - 1);
            self.owned_text[node] = id;
            id
        };
        self.set_prop(node, PropKey::Text, Value::VText { text: id, key: NO_KEY });
    }

    /// The text `node` shows: code's last `set_text`, else the declared
    /// `text=` (a `when` write is not read here). For a code-behind that
    /// needs the typed words at a tap (the caption page's name box) —
    /// read off the writes, no look built, nothing allocated; the free
    /// `resolve` for one string was the pattern the frame road ended (D43).
    pub fn text_of<'a>(&self, page: &'a Page, node: NodeId) -> Option<&'a str> {
        let written: Option<Value> = self.writes.iter().rev().find(|(n, p)| *n == node && p.key == PropKey::Text).map(|(_, p)| p.value);
        match written.or_else(|| page.prop(node, PropKey::Text))? {
            Value::VText { text, .. } => Some(page.str(text)),
            _ => None,
        }
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
            // next show the ladder shows. Its DESCENDANTS' too — a pooled
            // item hidden after its delete anim came back with the anim's
            // squashed child still held (a gradient mark's short rope).
            let seat: &[NodeId] = &self.last_seat;
            self.held.retain(|h| !placed_under_in(seat, page, h.0, node));
            self.held_props.retain(|h| !placed_under_in(seat, page, h.0, node));
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

    /// Writes a control's value from code, and everything that shows it
    /// at once.
    ///
    /// Needed when the code reads the page back in the same frame (a
    /// test, a layout it measures): the value lands (reads and `when`
    /// conditions see it), and every property written `$value` in the
    /// control's body is written now, not at the next `tick`.
    ///
    /// The automatic slots' fractions follow on the next
    /// `resolve_animated`, gliding by the control's default feel.
    pub fn set_value(&mut self, page: &mut Page, root: NodeId, v: f32) {
        self.set_prop(root, PropKey::Value, Value::VNum(v));
        self.follow_value(page, root, v);
        // Followed here, so the `tick` owes this control nothing.
        self.value_follow.retain(|n| *n != root);
    }

    /// Writes every property that reads `root`'s `$value`
    /// (`Page::value_follows`): a `text=` gets the number as a label
    /// shows it, any other property gets the number.
    ///
    /// Used by `set_value` and by `tick` for the values written since
    /// the last one. Allocates nothing once each label has its own
    /// string: the rows are read by index and the text is built in
    /// `follow_text`.
    fn follow_value(&mut self, page: &mut Page, root: NodeId, v: f32) {
        let mut text: String = std::mem::take(&mut self.follow_text);
        self.follow_value_with(page, root, v, &mut text);
        self.follow_text = text;
    }

    /// `follow_value` with the text buffer passed in, so a control that
    /// follows another control's value (`Inner value=$value` in a control
    /// body) passes the write on to its own readers with the same buffer.
    fn follow_value_with(&mut self, page: &mut Page, root: NodeId, v: f32, text: &mut String) {
        for i in 0..page.value_follows.len() {
            let f: ValueFollow = page.value_follows[i];
            // A row naming its own control would pass the write on forever.
            if f.root != root || f.node == root {
                continue;
            }
            let shown: f32 = crate::page::snap_to_step(v, f.step);
            match f.key {
                PropKey::Text => {
                    crate::page::value_text_into(v, f.step, text);
                    self.set_text(page, f.node, text);
                }
                PropKey::Value => {
                    self.set_prop(f.node, f.key, Value::VNum(shown));
                    self.follow_value_with(page, f.node, shown, text);
                    // Followed here, so the `tick` owes that control nothing.
                    self.value_follow.retain(|n| *n != f.node);
                }
                _ => self.set_prop(f.node, f.key, Value::VNum(shown)),
            }
        }
    }

    /// `set_value` for a caller holding only `&Page` — the touch and
    /// wheel paths: the value lands now (reads, `when` conditions and
    /// the automatic slots see it), and the properties written `$value`
    /// in the control's body follow at the next `tick`.
    pub fn set_value_deferred(&mut self, root: NodeId, v: f32) {
        self.set_prop(root, PropKey::Value, Value::VNum(v));
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

    /// EFFECTIVE `disabled`: the node's own flag or any PLACED ancestor's
    /// — the WPF IsEnabled law; a child cannot re-enable under a disabled
    /// parent, and a button seated into a disabled panel is disabled
    /// (B25). The placed parents come from the last frame's seat table
    /// (`last_seat`).
    pub fn disabled(&self, page: &Page, node: NodeId) -> bool {
        let mut n = node;
        loop {
            if self.has(n, DISABLED) {
                return true;
            }
            let parent = self.placed_parent(page, n);
            if parent == NO_NODE {
                return false;
            }
            n = parent;
        }
    }

    /// The parent `n` is laid out and drawn under, from the last frame's
    /// seat table (`last_seat`) — for the walks that run without a
    /// `Resolved` in hand; everything that holds one asks
    /// `Resolved::placed_parent`.
    pub(crate) fn placed_parent(&self, page: &Page, n: NodeId) -> NodeId {
        placed_parent_in(&self.last_seat, page, n)
    }

    /// Writes a Scroll's offset from code — the one door for it (B31).
    /// Marks the page dirty (a raw `scroll[n] = …` never did, so the
    /// strip drew stale) and asks the input tick to clamp the value to
    /// the content range once the node is idle. The clamp waits because
    /// the range needs a layout, and a pan or spring in flight on the
    /// node owns the offset until it settles (the rubber band stores an
    /// out-of-range offset on purpose).
    pub fn set_scroll(&mut self, node: NodeId, offset: [f32; 2]) {
        self.scroll[node] = offset;
        if !self.scroll_clamp_pending.contains(&node) {
            self.scroll_clamp_pending.push(node);
        }
        self.dirty = true;
    }

    /// Scrolls `scroll` the least that puts `item` fully inside its
    /// viewport, on each axis on its own: nothing when it already is; an
    /// item past the far edge lands on that edge, one before the near edge
    /// on that edge, each with `margin` epx of air past it so a neighbor
    /// peeks in (0 = flush). `rects` are the frame's drawn rects, which
    /// carry the current offset, so the write is the current offset plus
    /// the overshoot. Clamped at 0 here; the far end clamps on the next
    /// idle tick like every `set_scroll`. Returns whether it wrote.
    ///
    /// The one door for "show me this row" from code (a list opening on
    /// its selection, a strip following a pick) — each page spelled its own
    /// before, with its own edge rule (D116).
    pub fn scroll_into_view(&mut self, scroll: NodeId, item: NodeId, rects: &[Rect], margin: f32) -> bool {
        let (Some(v), Some(r)) = (rects.get(scroll.index()).copied(), rects.get(item.index()).copied()) else { return false };
        if v.w <= 0.0 || v.h <= 0.0 || r.w <= 0.0 || r.h <= 0.0 {
            return false; // no box yet (hidden, not laid out)
        }
        let overshoot = |near: f32, far: f32, item_near: f32, item_far: f32| -> f32 {
            if item_near < near {
                item_near - near - margin
            } else if item_far > far {
                item_far - far + margin
            } else {
                0.0
            }
        };
        let dx: f32 = overshoot(v.x, v.x + v.w, r.x, r.x + r.w);
        let dy: f32 = overshoot(v.y, v.y + v.h, r.y, r.y + r.h);
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        let [x, y]: [f32; 2] = self.scroll_of(scroll);
        let target: [f32; 2] = [(x + dx).max(0.0), (y + dy).max(0.0)];
        if target == [x, y] {
            return false;
        }
        self.set_scroll(scroll, target);
        true
    }

    /// A Scroll's offset in epx: how far its content moved up and left.
    pub fn scroll_of(&self, node: NodeId) -> [f32; 2] {
        self.scroll.get(node.index()).copied().unwrap_or([0.0; 2])
    }

    /// Forgets the view state the ENGINE holds, for a page reuse (B31):
    /// every scroll offset back to 0 (through `set_scroll`, so the page
    /// dirties), keyboard focus, caret, selection and the edit bar. A
    /// page's own reset list is built from its fields and never sees
    /// these — a reused effect page showed the preset strip where the
    /// last effect left it. The focus OWNER sits on `Input`; the flag
    /// asks its next tick to drop it.
    pub fn reset_view_state(&mut self) {
        for i in 0..self.scroll.len() {
            if self.scroll[i] != [0.0; 2] {
                self.set_scroll(NodeId::from_index(i), [0.0; 2]);
            }
        }
        for s in &mut self.states {
            *s &= !(FOCUSED | FOCUS_WITHIN);
        }
        self.focus_reset_pending = true;
        self.caret = None;
        self.selection = None;
        self.sel_handles = false;
        self.edit_bar = None;
        self.dirty = true;
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

    /// Can `resolve` skip this node and fill its declared look (B36-DES
    /// stage 1)? True when the page's rule says static (`static_nodes`)
    /// and no code write stands on it — a write promotes the node to
    /// dynamic until `clear_prop` takes the last one back. One question
    /// for every per-node loop, so they all skip by the same rule.
    /// With the mask switched off (`debug_set_static_mask`) every node
    /// is dynamic.
    ///
    /// A node past the tables (the page grew and `grow` has not run)
    /// answers dynamic — the safe side.
    #[inline]
    pub fn is_static(&self, node: NodeId) -> bool {
        self.static_mask_on && self.static_node.get(node.index()).copied().unwrap_or(false) && !self.written.get(node.index()).copied().unwrap_or(true)
    }

    /// Switches the static-node mask (B36-DES stage 1) — on by default;
    /// off = every node resolves the long way, the road before the mask.
    /// For the equivalence pin (both roads must give the same `Resolved`)
    /// and for bisecting a look that differs between them.
    pub fn debug_set_static_mask(&mut self, on: bool) {
        self.static_mask_on = on;
        self.dirty = true;
    }

    /// Switches the retained resolve (B36-DES stage 2) — on by default;
    /// off = every resolve rebuilds the whole snapshot (the stage-1
    /// road, every node through its ladder). For the equivalence pin
    /// (the retained result must equal a full rebuild) and for
    /// bisecting a look that differs between them.
    pub fn debug_set_retained(&mut self, on: bool) {
        self.retained.on = on;
        self.dirty = true;
    }

    /// The current look: what the last `resolve_retained` (or
    /// `resolve_animated`) left. Empty (no nodes) before the first
    /// resolve. Layout, draw and hit read it; a host that must hold it
    /// beside `&mut Runtime` copies it (`Resolved::clone_from` reuses
    /// the copy's buffers).
    pub fn resolved(&self) -> &Resolved {
        &self.resolved
    }

    /// How many nodes the last resolve ran through their `when` ladder
    /// — the whole page on a full rebuild, the reached set on the
    /// retained road. For the pin that proves a flip stays local.
    pub fn debug_laddered(&self) -> u32 {
        self.retained.laddered
    }

    /// The last resolve's report for the retained layout and draw list
    /// (B36-DES stage 3): its serial, whether it rebuilt everything, and
    /// the strings rewritten in place since the one before.
    pub fn resolve_changes(&self) -> &ResolveChanges {
        &self.resolve_changes
    }

    /// The retained layout (B36-DES stage 3): the rects of the current
    /// look (`resolved()`) in `viewport` — the same answer `layout`
    /// gives, re-laid only where the look changed since the last call
    /// (`layout::LayoutCache`). Call once per frame after
    /// `resolve_retained`; hand the answer to `glide`.
    pub fn layout_retained(&mut self, page: &Page, viewport: Rect, text: &dyn TextMeasure) -> &[Rect] {
        self.retained_layout.run(page, &self.resolved, &self.resolve_changes, viewport, text)
    }

    /// The retained draw list (B36-DES stage 3): brings `out` — the list
    /// this runtime handed out last time — up to date with the current
    /// look and the shown `rects` (after `glide`): the same list `draw`
    /// gives, every clean subtree's commands copied from last frame's
    /// (`draw::DrawCache`). The `above_children` split (`draw_except`,
    /// `draw_subtree`) has no retained road.
    pub fn draw_retained(&mut self, page: &Page, rects: &[Rect], out: &mut Vec<Draw>) {
        self.retained_draw.run(page, &self.resolved, &self.resolve_changes, rects, out);
    }

    /// Stamps the anim layer's motion channels — translate, scale, rotate,
    /// swing: the held end poses, the live players, the tap pulses — onto
    /// the kept look's `motion`, after the glide. The resolve stamped them
    /// too, but a `%` endpoint on a node that had no box at play lands in
    /// the glide, after that resolve (B38: the settings sheet drew at its
    /// seat for one frame, then jumped below the screen and slid). Players
    /// × owned channels: no allocation, no second resolve, no change serial
    /// bumped — the draw cache diffs `motion` per node and walks only the
    /// mover's subtree.
    pub fn compose_motion(&mut self) {
        let Runtime { resolved, held, players, motion, pulses, .. } = self;
        let n: usize = resolved.motion.len();
        let mut stamp = |node: NodeId, channel: Channel, v: f32| {
            if node.index() < n && !matches!(channel, Channel::Opacity | Channel::ColorA | Channel::RevealX | Channel::RevealY) {
                resolved.motion[node].set(channel, v);
            }
        };
        for (node, channel, v) in held.iter() {
            stamp(*node, *channel, *v);
        }
        for p in players.iter().filter(|p| !p.stopped) {
            for (node, channel) in &p.owns {
                stamp(*node, *channel, motion[*node].get(*channel));
            }
        }
        for p in pulses.iter() {
            stamp(p.node, Channel::Swing, p.angle());
        }
    }

    /// One frame's look, layout and draw list in the one order that is
    /// right — resolve, the retained layout, glide, the anim motion, the
    /// retained draw list — into the caller's buffers, kept between frames
    /// so nothing allocates once warm. Every host draws through here (or
    /// through `KeptFrame::frame_retained`, which adds the look's copy a
    /// host with an input machine needs): the order is the runtime's, never
    /// a host's to get wrong (B38 was a host drawing from the resolve taken
    /// before the glide; three hosts spelled the sequence by hand). A hit
    /// or a draw off this road is unspellable: `draw` and `hit` take a
    /// `Frame`, made only by the roads (D43). `split`: the `above_children`
    /// road — the page without that subtree into `draws`, the subtree into
    /// the second buffer; no retained road there, the full walk each frame
    /// as before. Call after `tick` and the input tick; clears `dirty`.
    pub fn frame_retained(&mut self, page: &Page, viewport: Rect, text: &dyn TextMeasure, rects: &mut Vec<Rect>, draws: &mut Vec<Draw>, split: Option<(NodeId, &mut Vec<Draw>)>) {
        self.resolve_retained(page);
        rects.clear();
        rects.extend_from_slice(self.layout_retained(page, viewport, text));
        self.glide_in_place(rects);
        self.compose_motion();
        match split {
            Some((top, above)) => {
                let f = crate::frame::Frame::new(&self.resolved, rects);
                crate::draw::draw_except_into(page, f, top, draws);
                crate::draw::draw_subtree_into(page, f, top, above);
            }
            None => self.retained_draw.run(page, &self.resolved, &self.resolve_changes, rects, draws),
        }
        self.dirty = false;
    }

    /// Switches the retained layout and draw list (B36-DES stage 3) —
    /// on by default; off = the full roads every frame. For the
    /// equivalence pin (the retained rects and list must equal the full
    /// roads') and for bisecting a picture that differs between them.
    pub fn debug_set_retained_layout(&mut self, on: bool) {
        self.retained_layout.set_on(on);
        self.retained_draw.set_on(on);
        self.dirty = true;
    }

    /// What the last `layout_retained` did — for the pin that proves a
    /// change stays local.
    pub fn debug_layout_stats(&self) -> LayoutStats {
        self.retained_layout.stats()
    }

    /// What the last `draw_retained` did.
    pub fn debug_draw_stats(&self) -> DrawStats {
        self.retained_draw.stats()
    }

    /// The retained resolve (B36-DES stage 2): brings `resolved()` up to
    /// date with the runtime and returns it. Call once per frame after
    /// `tick`. After it returns, `resolved()` is the current look — the
    /// `when` ladder for every node a change since the last call can
    /// reach (see `Watchers`), the code writes, the in-flight tweens and
    /// the anim layer on top, and the tweens a `transition` rule starts
    /// for this frame's state changes are seeded from what the previous
    /// look showed.
    ///
    /// The snapshot is patched in place: only the reached nodes re-run
    /// their ladder, the rest keep last frame's answers. A full rebuild
    /// happens when the patch cannot be trusted (`full_rebuild_needed`:
    /// the first resolve, a node-count change, a theme-variant or locale
    /// switch, a mask switch, or the road switched off).
    pub fn resolve_retained(&mut self, page: &Page) -> &Resolved {
        // The snapshot leaves the runtime for the duration so the
        // resolve can read `&self` and write `&mut Resolved` at once; a
        // move of the struct's headers, no copy of its buffers.
        let mut r: Resolved = std::mem::take(&mut self.resolved);
        resolve_into(page, self, &mut r);
        self.resolved = r;
        &self.resolved
    }

    /// Brings the declared-look table up to date with the page and the
    /// theme variant. Needed because `deref` and `kind_default` read the
    /// variant: a dark-mode switch changes what a static node's declared
    /// `color=$ink` means, so the table is keyed by the variant it was
    /// derived under and rebuilt on a mismatch, or when rows were added.
    fn refresh_static_look(&mut self, page: &Page) {
        if self.static_look.variant == self.variant && self.static_look.opacity.len() == page.nodes.len() {
            return;
        }
        self.static_look = DeclaredLook::build(page, self.variant);
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
            // Declared on purpose: the root that WROTE the node raises
            // its states, wherever a seat moves the node.
            State::Custom(s) => {
                let mut n = node;
                while n != NO_NODE {
                    if self.customs.iter().any(|(cn, cs)| *cn == n && *cs == s) {
                        return true;
                    }
                    n = page.declared_parent(n);
                }
                false
            }
            State::ScreenOrientation(w) | State::ScreenClass(w) => self.screen.contains(&w),
            State::ArrangementScreen(i, w) => self.arrangement == i && self.screen.contains(&w),
            State::ScreenPair(o, c) => self.screen.contains(&o) && self.screen.contains(&c),
        }
    }
}

/// The static rule (B36-DES stage 1): a node is STATIC when nothing that
/// runs at run time can change what `resolve` answers for it — it keeps
/// its declared look, so every per-node loop in `resolve` skips it. All
/// of these must hold:
/// - no `when` block responds on it (its own `whens` range is empty;
///   another node's `when x.held` watches x's STATE, not x's look);
/// - it declares no seat word and no motion word on its own line (the
///   keys `fold_seats_and_motion` reads: `seat`, `scale`, `translate`,
///   `rotate`) — those set the seat table and the draw transform;
/// - it has no `on:` handler (`on:show`/`on:hide` play anims, and the
///   visibility watcher fires them — kept dynamic, conservatively);
/// - no anim lane in the page aims at it (a player would own channels
///   and prop lanes on it);
/// - no driver (GP33) targets it or is rooted on it (the fraction and
///   presence loops write it);
/// - it is not a `repeat`, a repeat's direct child (the template root,
///   a pool slot's root, the `empty` content) or a repeat template.
/// Slot notes: a slot restyle on an instance line is stamped at compile
/// time, so slot nodes bake like any other; a run-time slot change is a
/// code write or a `set_items` — the promotion case, `Runtime::written`.
///
/// Answers for the nodes from index `from` on (the whole page at
/// `Runtime::new`, the appended rows at `grow`); the anim and driver
/// targets are gathered in one walk, not one per node.
fn static_nodes(page: &Page, from: usize) -> Vec<bool> {
    let n = page.nodes.len();
    let mut targeted: Vec<bool> = vec![false; n];
    for t in &page.anims {
        if let Some(slot) = targeted.get_mut(t.node.index()) {
            *slot = true;
        }
    }
    for d in &page.drivers {
        if let Some(slot) = targeted.get_mut(d.node.index()) {
            *slot = true;
        }
        if let Some(slot) = targeted.get_mut(d.root.index()) {
            *slot = true;
        }
    }
    for rp in &page.repeats {
        if let Some(slot) = targeted.get_mut(rp.template.index()) {
            *slot = true;
        }
        if let Some(slot) = targeted.get_mut(rp.node.index()) {
            *slot = true;
        }
    }
    (from..n)
        .map(|i| {
            let id = NodeId::from_index(i);
            let node = &page.nodes[i];
            !targeted[i]
                && node.whens.1 == 0
                && node.handlers.1 == 0
                && node.kind != Kind::Repeat
                // Declared on purpose: the repeat that WROTE the row is
                // what makes it a slot, wherever a seat moves it.
                && { let parent = page.declared_parent(id); parent == NO_NODE || page.nodes[parent].kind != Kind::Repeat }
                && !page.node_props(id).iter().any(|p| matches!(p.key, PropKey::SeatOf | PropKey::Scale | PropKey::ScaleX | PropKey::ScaleY | PropKey::TranslateX | PropKey::TranslateY | PropKey::Rotate))
        })
        .collect()
}

/// A node's declared value for `key` — the file's word, else the kind's
/// theme default, `$refs` followed — exactly what `Resolved::prop`
/// answers once the override walk finds nothing. For the tables a static
/// node's skipped loops read from.
fn declared_value(page: &Page, variant: StrId, node: NodeId, key: PropKey) -> Option<Value> {
    let raw = page.prop(node, key).or_else(|| page.kind_default(page.nodes[node].kind, key, variant))?;
    Some(deref(page, variant, raw))
}

/// What a static node's skipped loops fill in (B36-DES stage 1): the
/// three answers `resolve_animated` otherwise looks up per node per
/// frame — its own visibility word, its opacity and its fill's alpha —
/// derived once from the declared props under one theme `variant`
/// (`Runtime::refresh_static_look` rebuilds on a variant switch or new
/// rows). Dynamic nodes' entries are filled too but never read.
#[derive(Clone, Debug, Default)]
struct DeclaredLook {
    variant: StrId,
    /// Per node: the node's own word shows it (not `gone`/`space_only`)
    /// — the `own` half of the visibility fold.
    own_visible: Vec<bool>,
    /// Per node: `opacity=` (1 when unspoken) — `Motion::opacity` in `shown`.
    opacity: Vec<f32>,
    /// Per node: the declared `color=`'s alpha (1 when unspoken) —
    /// `Motion::color_a` in `shown`.
    color_a: Vec<f32>,
}

impl DeclaredLook {
    fn build(page: &Page, variant: StrId) -> DeclaredLook {
        let n = page.nodes.len();
        let mut look = DeclaredLook { variant, own_visible: Vec::with_capacity(n), opacity: Vec::with_capacity(n), color_a: Vec::with_capacity(n) };
        for i in 0..n {
            let id = NodeId::from_index(i);
            let word: Option<&str> = match declared_value(page, variant, id, PropKey::Visibility) {
                Some(Value::VIdent(w)) => Some(page.str(w)),
                _ => None,
            };
            look.own_visible.push(!matches!(word, Some("gone") | Some("space_only")));
            look.opacity.push(match declared_value(page, variant, id, PropKey::Opacity) {
                Some(Value::VNum(o)) => o,
                _ => 1.0,
            });
            look.color_a.push(match declared_value(page, variant, id, PropKey::Color) {
                Some(Value::VColor(c)) | Some(Value::VStop { color: c, .. }) => c.0[3] as f32 / 255.0,
                _ => 1.0,
            });
        }
        look
    }
}

/// The reverse table (B36-DES stage 2): for each thing a flip can land
/// on, the nodes whose `when` blocks watch it — built once per page at
/// `Runtime::new`, extended with the rows `grow` appends, never rebuilt.
/// A flip on node W re-runs the ladder of `by_owner[W]` and of no other
/// node, because a node's ladder reads only the states of the owners its
/// own `when`s name (`state_active`).
struct Watchers {
    /// Per owner node: the nodes with a `when` on its built-in state
    /// (`when held` on the node itself, `when x.held` on x) or on a
    /// custom state it declares (`when dragging` anywhere inside the
    /// control whose `state` line names it — registered under every
    /// declared ancestor that declares that state, the roots
    /// `set_custom` raises it on).
    by_owner: Vec<Vec<NodeId>>,
    /// The nodes with an arrangement or `screen.` `when`: a view-level
    /// flip (`go`, `set_screen`) reaches them all.
    view: Vec<NodeId>,
    /// The nodes with a `when disabled`: `disabled` folds the owner's
    /// PLACED ancestors, so a DISABLED flip anywhere, or a seat move,
    /// reaches every one of them — the safe side, and the list is short.
    disabled: Vec<NodeId>,
}

impl Watchers {
    fn new(page: &Page) -> Watchers {
        let mut w = Watchers { by_owner: Vec::new(), view: Vec::new(), disabled: Vec::new() };
        w.extend(page);
        w
    }

    /// Registers the `when`s of every node past the table's end (the
    /// whole page the first time, the appended rows after a `grow`).
    fn extend(&mut self, page: &Page) {
        let from = self.by_owner.len();
        let n = page.nodes.len();
        self.by_owner.resize_with(n, Vec::new);
        for i in from..n {
            let id = NodeId::from_index(i);
            let node = &page.nodes[i];
            for w in &page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize] {
                let owner: NodeId = if w.of == NO_NODE { id } else { w.of };
                match w.state {
                    State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected => self.by_owner[owner].push(id),
                    State::StateDisabled => self.disabled.push(id),
                    State::Arrangement(_) | State::ArrangementScreen(..) | State::ScreenOrientation(_) | State::ScreenClass(_) | State::ScreenPair(..) => self.view.push(id),
                    // Declared on purpose: the root that WROTE the node
                    // raises its states (`state_active`'s walk).
                    State::Custom(s) => {
                        let mut a = owner;
                        while a != NO_NODE {
                            if page.custom_states.iter().any(|(root, st)| *root == a && *st == s) {
                                self.by_owner[a].push(id);
                            }
                            a = page.declared_parent(a);
                        }
                    }
                }
            }
        }
        // Every list, not only the new owners': an appended row may watch
        // an older node.
        for list in &mut self.by_owner {
            list.sort();
            list.dedup();
        }
        self.view.sort();
        self.view.dedup();
        self.disabled.sort();
        self.disabled.dedup();
    }
}

/// One concerned node's values before a flip — what its animatable
/// keys showed, read from the retained look before the ladder moves it;
/// the `from` of the tweens the flip's `transition` rule starts.
#[derive(Clone, Copy)]
struct Before {
    node: NodeId,
    /// Bit k set = an anim owns `ANIMATABLE[k]` on this node: no tween is
    /// born on it (GP40: anim beats transition).
    owned: u64,
    /// Per `ANIMATABLE` key: the value shown (an in-flight tween's, else
    /// the look's); `None` = nothing shown yet, the key's rest default
    /// applies.
    current: [Option<Value>; ANIMATABLE_COUNT],
}

/// The retained resolve's bookkeeping (B36-DES stage 2). The per-node
/// tables hold the LADDER's answers — the file's placement and words
/// with the active `when`s applied, before the tail (code writes,
/// tweens, the anim layer) — so a resolve can put a node the tail
/// touched last frame back to its ladder look and re-apply this frame's
/// tail, without re-running the ladder.
struct Retained {
    /// Has a resolve filled the snapshot yet? `false` = the next one is
    /// a full rebuild.
    valid: bool,
    /// The switch (`debug_set_retained`): off = every resolve is a full
    /// rebuild.
    on: bool,
    watchers: Watchers,
    /// Per node: the placement after the active `when`s (before writes
    /// and placement tweens).
    ladder_placement: Vec<Placement>,
    /// Per node: the motion words declared on the node with the active
    /// `when`s' on top (before tail motion words and the anim layer).
    ladder_motion: Vec<Motion>,
    /// Per node: the seat the active `when`s say (before a code write
    /// of `seat`).
    ladder_seat: Vec<NodeId>,
    /// Per node: does the node's own visibility word show it — the
    /// `own` half of the visibility fold (GP19), kept so the fold over
    /// ancestors runs only when some word or seat changed.
    own_visible: Vec<bool>,
    /// The nodes whose final placement, motion, seat or override list
    /// the tail touched last resolve — put back to the ladder tables
    /// before the tail is rebuilt, and re-read into `shown`.
    tail_touched: Vec<NodeId>,
    /// Nodes with a code write added, changed or taken back since the
    /// last resolve (`set_prop`, `clear_prop`) — each is a flip on
    /// itself.
    write_flips: Vec<NodeId>,
    /// What the last resolve saw, diffed against the runtime to find
    /// the flips (the state bits are diffed against `Resolved::states`,
    /// the copy the last resolve took).
    seen_arrangement: ArrangementId,
    seen_screen: Vec<StrId>,
    seen_customs: Vec<(NodeId, StrId)>,
    seen_mask_on: bool,
    /// The last resolve moved a seat: the `when disabled` watchers re-run
    /// next time, because `disabled` reads placed parents from
    /// `last_seat` — one resolve late, on every road.
    seat_moved: bool,
    /// Nodes the last resolve ran through their ladder (`debug_laddered`).
    laddered: u32,
    // Scratch buffers, kept so a steady frame allocates nothing.
    reach: Vec<NodeId>,
    entries: Vec<(NodeId, Prop)>,
    before: Vec<Before>,
    now: Vec<bool>,
    touched: Vec<NodeId>,
    presence: Vec<(NodeId, bool)>,
    fixes: Vec<(usize, i64)>,
}

impl Retained {
    fn new(page: &Page) -> Retained {
        Retained {
            valid: false,
            on: true,
            watchers: Watchers::new(page),
            ladder_placement: Vec::new(),
            ladder_motion: Vec::new(),
            ladder_seat: Vec::new(),
            own_visible: Vec::new(),
            tail_touched: Vec::new(),
            write_flips: Vec::new(),
            seen_arrangement: BASE,
            seen_screen: Vec::new(),
            seen_customs: Vec::new(),
            seen_mask_on: true,
            seat_moved: false,
            laddered: 0,
            reach: Vec::new(),
            entries: Vec::new(),
            before: Vec::new(),
            now: Vec::new(),
            touched: Vec::new(),
            presence: Vec::new(),
            fixes: Vec::new(),
        }
    }
}

/// The parent `n` is laid out and drawn under, read from a seat table
/// (`Resolved::seat`, or the Runtime's copy of it): the seat host while
/// a `when … n.seat = name` holds, else the parent that wrote `n`. A
/// table shorter than the arena (rows cloned since) means "at home".
fn placed_parent_in(seat: &[NodeId], page: &Page, n: NodeId) -> NodeId {
    match seat.get(n.index()).copied().unwrap_or(NO_NODE) {
        NO_NODE => page.declared_parent(n),
        host => host,
    }
}

/// Whether `node` is `root` or draws anywhere under it — the PLACED tree
/// (`placed_parent_in`), read from a seat table.
fn placed_under_in(seat: &[NodeId], page: &Page, node: NodeId, root: NodeId) -> bool {
    let mut n: NodeId = node;
    while n != NO_NODE && n.index() < page.nodes.len() {
        if n == root {
            return true;
        }
        n = placed_parent_in(seat, page, n);
    }
    false
}

/// The current values: base placement with active poses applied, and the
/// prop overrides the active states contribute.
#[derive(Debug, Default, PartialEq)]
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
    /// Per node: where its own `when` overrides sit in `overrides`
    /// (start, len) — the ladder pushes them node by node, so a lookup
    /// reads a node's few entries instead of walking the whole list
    /// (B36 trace: 30 thousand walks over a few hundred entries were the
    /// 14 ms resolve on the effect page). Entries from `override_tail`
    /// on are the code writes, anim channels and tweens, pushed after the
    /// ladder in no node order; they win, so a lookup reads them first.
    pub override_span: Vec<(u32, u32)>,
    pub override_tail: u32,
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
    /// The placed tree as tables (`PlacedTree`): every walk's children,
    /// parents and subtrees — built once per resolve that moved a seat,
    /// changed a repeat's rows or a raise, or grew the page.
    pub tree: PlacedTree,
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

/// Written by hand so `clone_from` copies INTO a host's existing buffers
/// (a derived `Clone` would allocate a fresh snapshot per frame, which
/// the retained resolve exists to avoid — B36-DES stage 2).
impl Clone for Resolved {
    fn clone(&self) -> Resolved {
        let mut out = Resolved::default();
        out.clone_from(self);
        out
    }

    fn clone_from(&mut self, from: &Resolved) {
        self.anim_owned.clone_from(&from.anim_owned);
        self.anim_owned_props.clone_from(&from.anim_owned_props);
        self.placement.clone_from(&from.placement);
        self.overrides.clone_from(&from.overrides);
        self.override_span.clone_from(&from.override_span);
        self.override_tail = from.override_tail;
        self.variant = from.variant;
        self.scroll.clone_from(&from.scroll);
        self.safe = from.safe;
        self.motion.clone_from(&from.motion);
        self.seat.clone_from(&from.seat);
        self.states.clone_from(&from.states);
        self.raised.clone_from(&from.raised);
        self.repeat_rows.clone_from(&from.repeat_rows);
        self.tree.clone_from(&from.tree);
        self.hiding.clone_from(&from.hiding);
        self.auto.clone_from(&from.auto);
        self.presence.clone_from(&from.presence);
        self.caret = from.caret;
        self.selection = from.selection;
        self.sel_handles = from.sel_handles;
        self.locale.clone_from(&from.locale);
    }
}

impl Resolved {
    /// The prop's current value: state overrides → node props → kind
    /// defaults. `$refs` are followed through the theme.
    /// The override that wins for `key` on `node`, if any: the tail (code
    /// writes, anims, tweens — pushed last, so they win) newest first,
    /// then the node's own `when` entries newest first.
    pub fn override_of(&self, node: NodeId, key: PropKey) -> Option<Value> {
        let tail = self.override_tail as usize;
        if let Some((_, p)) = self.overrides[tail.min(self.overrides.len())..].iter().rev().find(|(n, p)| *n == node && p.key == key) {
            return Some(p.value);
        }
        let (start, len) = *self.override_span.get(node.index())?;
        self.overrides[start as usize..(start + len) as usize].iter().rev().find(|(_, p)| p.key == key).map(|(_, p)| p.value)
    }

    pub fn prop(&self, page: &Page, node: NodeId, key: PropKey) -> Option<Value> {
        let raw = self
            .override_of(node, key)
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
            if let Some(Value::VNum(a)) = self.override_of(node, PropKey::ColorA) {
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
        self.written_from(page, node, key).or_else(|| self.kind_default_of(page, node, key))
    }

    /// What `node` shows for an inherited word when it writes none of its
    /// own: the word of the nearest box above it, else its kind's default.
    ///
    /// Needed for the value a timed rule slides from and back to: with the
    /// node's own word in force (`when held font_color=…`), `inherited`
    /// answers that word, and a slide from it to itself is no slide.
    fn inherited_above(&self, page: &Page, node: NodeId, key: PropKey) -> Option<Value> {
        let parent = self.placed_parent(page, node);
        let above = if parent == NO_NODE { None } else { self.written_from(page, parent, key) };
        above.or_else(|| self.kind_default_of(page, node, key))
    }

    /// `key` as `from` or the nearest box above it writes it.
    fn written_from(&self, page: &Page, from: NodeId, key: PropKey) -> Option<Value> {
        let mut n = from;
        loop {
            let own = self.override_of(n, key).or_else(|| page.prop(n, key));
            if let Some(v) = own {
                return Some(deref(page, self.variant, v));
            }
            // Placed: a seated Label takes its host's font words, the
            // way the addon's theme propagation follows the live tree.
            let parent = self.placed_parent(page, n);
            if parent == NO_NODE {
                return None;
            }
            n = parent;
        }
    }

    /// The theme's default of `key` for `node`'s kind (a `theme { }`
    /// block's defaults are kept on Label too).
    fn kind_default_of(&self, page: &Page, node: NodeId, key: PropKey) -> Option<Value> {
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
        // Mid deferred hide (GP19): the `when` or the presence state
        // already says gone but the `on:hide` anim still plays — the node
        // stays until it finishes. Asked BEFORE the presence state, so a
        // presence slot leaves the way every other node does.
        if self.hiding.contains(&node) {
            return false;
        }
        if let Auto::Presence(baked) = page.nodes[node].auto {
            // GP33: the mark exists iff its driving state holds — live,
            // through `set_custom` on the control root.
            let on = self.presence.iter().find(|(n, _)| *n == node).map(|(_, b)| *b).unwrap_or(baked);
            if !on {
                return true;
            }
        }
        self.ident(page, node, PropKey::Visibility) == Some("gone")
    }

    /// The parent `n` is LAID OUT and DRAWN under: its seat host while a
    /// `when … n.seat = name` holds, else the parent that wrote it
    /// (`Page::declared_parent`).
    ///
    /// Why a named accessor: a running page has two trees — the declared
    /// tree (the file's nesting) and this placed tree (after seat moves).
    /// Layout and paint follow the placed one, so a walk that reads the
    /// parent field bare picks the declared tree by accident, and a
    /// seated button's taps, focus and clips then follow a parent it no
    /// longer sits in (todo-bugs B25). Every walk names its tree: hit,
    /// clip, focus, hover, scroll ancestor, disabled, presence, entrances,
    /// inherited props and moved rects want this one.
    pub fn placed_parent(&self, page: &Page, n: NodeId) -> NodeId {
        placed_parent_in(&self.seat, page, n)
    }

    /// Whether `node` is `ancestor` or sits anywhere under it in the
    /// PLACED tree (`placed_parent`); the declared one is
    /// `Page::is_under`. `NO_NODE` is under nothing.
    pub fn placed_is_under(&self, page: &Page, node: NodeId, ancestor: NodeId) -> bool {
        let mut n: NodeId = node;
        loop {
            if n == ancestor {
                return true;
            }
            if n == NO_NODE || n.index() >= page.nodes.len() {
                return false;
            }
            n = self.placed_parent(page, n);
        }
    }

    /// The nodes laid out and drawn under `id`: its own children that no
    /// `when` seated elsewhere, plus whoever is seated here.
    pub fn children(&self, page: &Page, id: NodeId) -> Children<'_> {
        let _ = page; // the tree was built from it; kept so every caller reads the same
        self.tree.children(id)
    }

    /// `children` in PAINT order: siblings sorted by their raise (stable,
    /// so equal raises keep the file's order). Draw and hit use this;
    /// layout keeps the file's order.
    pub fn paint_children(&self, page: &Page, id: NodeId) -> Children<'_> {
        let _ = page;
        self.tree.paint_children(id)
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

/// The `when` ladder plus the anim layer on top (see `compose_anim`),
/// built from scratch — no tweens, no visibility triggers, nothing kept.
/// For code that holds `&Runtime` only (tests, `draw` self-checks); a
/// frame takes `Runtime::resolve_retained`.
pub fn resolve(page: &Page, rt: &Runtime) -> Resolved {
    let mut r = resolve_ladder(page, rt);
    let mut touched: Vec<NodeId> = Vec::new();
    compose_anim(page, rt, &mut r, &mut touched);
    r.tree.build(page, &r.seat, &r.repeat_rows, &r.raised);
    r
}

/// One property, one writer (decided 2026-08-30): an anim that owns a
/// channel — playing, or a finished one whose end pose still stands — is
/// the top of the ladder for that channel. Motion channels land in
/// `Resolved::motion` over the `when` responses; `opacity` and `color.a`
/// land as the LAST override, so `Resolved::num`/`color` answer with the
/// anim's value like any other writer. Nothing multiplies. Every node
/// written is listed in `touched` (the retained resolve puts them back
/// to their ladder look next frame).
fn compose_anim(page: &Page, rt: &Runtime, r: &mut Resolved, touched: &mut Vec<NodeId>) {
    let mut apply = |node: NodeId, channel: Channel, v: f32| {
        match channel {
            Channel::Opacity => r.overrides.push((node, Prop { key: PropKey::Opacity, value: Value::VNum(v) })),
            Channel::ColorA => r.overrides.push((node, Prop { key: PropKey::ColorA, value: Value::VNum(v) })),
            Channel::RevealX => r.overrides.push((node, Prop { key: PropKey::RevealX, value: Value::VNum(v) })),
            Channel::RevealY => r.overrides.push((node, Prop { key: PropKey::RevealY, value: Value::VNum(v) })),
            _ => r.motion[node].set(channel, v),
        }
        r.anim_owned[node] |= channel.bit();
        touched.push(node);
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
        touched.push(*node);
    }
    for p in rt.players.iter().filter(|p| !p.stopped) {
        for (node, key) in &p.owns_props {
            if let Some(v) = get_pval(&rt.prop_anim, *node, *key) {
                apply_prop_lane(page, r, *node, *key, v);
                touched.push(*node);
            }
        }
    }
}

/// Lands one prop lane's value as the LAST override (GP40) — the anim is
/// the top of the ladder for its lane, like the motion channels. An
/// alpha-only lane arrives as a `VNum` on a color key: the ladder's hue
/// with the anim's alpha, so a `when` can restyle the hue mid-play.
fn apply_prop_lane(page: &Page, r: &mut Resolved, node: NodeId, key: PropKey, v: Value) {
    // A size, a margin or a padding lives in the placement the layout
    // reads, not among the look's values: written there the box follows
    // the lane, the road a `transition card.width` tween takes.
    if key.is_placement() {
        apply_placement(&mut r.placement[node], page, Prop { key, value: v });
        r.anim_owned_props.push((node, key));
        return;
    }
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
    let mut r = Resolved { anim_owned: vec![0; page.nodes.len()], anim_owned_props: Vec::new(), placement: Vec::with_capacity(page.nodes.len()), overrides: Vec::new(), override_span: Vec::with_capacity(page.nodes.len()), override_tail: 0, variant: rt.variant, scroll: rt.scroll.clone(), safe: rt.safe, motion: vec![Motion::IDENTITY; page.nodes.len()], seat: vec![NO_NODE; page.nodes.len()], states: rt.states.clone(), raised: rt.raised.clone(), locale: rt.locale.clone(), repeat_rows: rt.pools.iter().map(|p| (p.repeat, p.live.iter().map(|s| p.slots[*s as usize]).collect())).collect(), tree: PlacedTree::default(), hiding: rt.players.iter().filter(|p| !p.stopped).filter_map(|p| match p.finish { Finish::HideWhen(n) => Some(n), _ => None }).collect(), auto: Vec::new(), presence: Vec::new(), caret: rt.caret, selection: rt.selection, sel_handles: rt.sel_handles };
    for i in 0..page.nodes.len() {
        let id = NodeId::from_index(i);
        let span_start: u32 = r.overrides.len() as u32;
        let placement: Placement = ladder_node(page, rt, id, &mut r.overrides);
        r.override_span.push((span_start, r.overrides.len() as u32 - span_start));
        r.placement.push(placement);
    }
    r.override_tail = r.overrides.len() as u32;
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
            driver_fraction(page, &r, d, v)
        } else {
            rt.auto_rest.iter().find(|(n, _)| *n == d.node).map(|(_, f)| *f).unwrap_or_else(|| baked_frac(page, d.node))
        };
        auto.push((d.node, shown));
    }
    r.auto = auto;
    fold_seats_and_motion(page, rt, &mut r);
    r
}

/// One node through its `when` ladder: the declared placement with the
/// active `when`s' placement words applied is returned; their other
/// words are pushed onto `out` in the order they win. That order is the
/// order the compiler stored the node's `when`s in (lowest rank first;
/// within a rank a style's before the node's own, the later line last),
/// so nothing is collected or sorted here. The one per-node rule every
/// road shares — the from-scratch `resolve_ladder`, the retained full
/// fill and the retained patch.
///
/// A static node (B36-DES stage 1) keeps its declared placement and has
/// no `when` to scan: nothing pushed, nothing asked of the states.
fn ladder_node(page: &Page, rt: &Runtime, id: NodeId, out: &mut Vec<(NodeId, Prop)>) -> Placement {
    let node = &page.nodes[id];
    let mut placement = node.placement;
    if rt.is_static(id) {
        return placement;
    }
    for w in &page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize] {
        if !rt.state_active(page, if w.of == NO_NODE { id } else { w.of }, w.state) {
            continue;
        }
        for p in &page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize] {
            if p.key.is_placement() {
                apply_placement(&mut placement, page, *p);
            } else {
                out.push((id, *p));
            }
        }
    }
    placement
}

/// One node's motion and seat from its ladder alone: the motion words
/// DECLARED on the node line are its resting transform (`Image
/// pickerImage rotate=90` — every box takes the motion words,
/// docs/gogo-design.md "Every box takes"; a declared `rotate=` once
/// compiled clean and drew flat, todo-bugs.md #1), then the node's
/// `when` entries on top, in ladder order. The tail (a code write of a
/// motion word or `seat`, a motion tween, the anim layer) is applied
/// after, by whoever holds the tail. A static node (B36-DES stage 1)
/// sits at home with no motion words of its own — the rule refuses a
/// node that declares any — so its props are not read.
fn ladder_motion_and_seat(page: &Page, rt: &Runtime, id: NodeId, entries: &[(NodeId, Prop)]) -> (Motion, NodeId) {
    let mut m = Motion::IDENTITY;
    let mut seat = NO_NODE;
    if !rt.is_static(id) {
        for p in page.node_props(id) {
            set_motion_word(&mut m, p.key, p.value);
        }
    }
    for (_, p) in entries {
        match p.key {
            PropKey::SeatOf => seat = seat_of(page, id, p.value),
            _ => set_motion_word(&mut m, p.key, p.value),
        }
    }
    (m, seat)
}

/// The seat a `seat = <name>` word names for `node`: `home` (or a
/// non-word) = the parent `node`'s line declares.
///
/// Needed because a seat written from code is checked nowhere else: a
/// file's `node.seat = name` is checked at build (G1016), a code write
/// takes any name. A name that is no declared seat, or a seat inside
/// `node`, stops a debug build with a message and leaves `node` at home
/// in a release build. A node seated inside itself never finishes laying
/// out.
fn seat_of(page: &Page, node: NodeId, value: Value) -> NodeId {
    let Value::VIdent(name) = value else { return NO_NODE };
    let word: &str = page.str(name);
    if word == "home" {
        return NO_NODE;
    }
    let Some(seat) = page.find(word).filter(|s| page.nodes[*s].kind == Kind::Seat) else {
        debug_assert!(false, "`{}.seat = {word}`: `{word}` is not a declared seat (a `seat {word}` line of the file)", page.str(page.nodes[node].name));
        return NO_NODE;
    };
    if page.is_under(seat, node) { // declared on purpose: the file's nesting, as G1016 reads it
        debug_assert!(false, "`{0}.seat = {word}`: seat `{word}` sits inside `{0}` — a node can't move into its own subtree", page.str(page.nodes[node].name));
        return NO_NODE;
    }
    seat
}

/// The keys that are not props but placement facts: `x.seat = name`
/// re-parents for layout, the motion words set the draw transform.
/// Folds every node from its declared words and the whole override
/// list in order (the from-scratch road); the retained road keeps the
/// ladder half per node (`ladder_motion_and_seat`) and applies the tail
/// as it pushes it.
fn fold_seats_and_motion(page: &Page, rt: &Runtime, r: &mut Resolved) {
    for i in 0..page.nodes.len() {
        r.seat[i] = NO_NODE;
    }
    for i in 0..page.nodes.len() {
        let id = NodeId::from_index(i);
        if rt.is_static(id) {
            continue;
        }
        for p in page.node_props(id) {
            set_motion_word(&mut r.motion[i], p.key, p.value);
        }
    }
    for (id, p) in &r.overrides {
        match p.key {
            PropKey::SeatOf => r.seat[*id] = seat_of(page, *id, p.value),
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
fn kind_visual_default(page: &Page, r: &Resolved, node: NodeId, key: PropKey) -> Option<Value> {
    match key {
        PropKey::Opacity => Some(Value::VNum(1.0)),
        PropKey::Background if page.nodes[node].kind == Kind::Button => Some(Value::VColor(crate::draw::DEFAULT_BUTTON_BACKGROUND)),
        // The ink a text shows when its node wrote no `font_color`: the
        // one a box above it or the theme gives, else gogo's own. Needed
        // so a timed `when held font_color=…` slides from it and back to
        // it, where it used to change at once.
        PropKey::FontColor => match r.inherited_above(page, node, PropKey::FontColor) {
            Some(Value::VColor(ink)) => Some(Value::VColor(ink)),
            _ => Some(Value::VColor(crate::draw::DEFAULT_TEXT_COLOR)),
        },
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

/// Writes a placement word that arrives while the page runs (a
/// view-level `when`, a code write, an anim line) into a node's placement,
/// with the reader a node's line is read by (`Placement::write`).
///
/// A value that reader cannot read can only come from code; the field
/// then holds the reader's fallback: no size, 0, no alignment.
fn apply_placement(p: &mut Placement, page: &Page, prop: Prop) {
    let _ = p.write(page, prop);
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

/// The `ANIMATABLE` list's length — the width of a `Before` row (one
/// slot per key) and of its owned-keys bitmask (`u64`, so at most 64).
const ANIMATABLE_COUNT: usize = ANIMATABLE.len();
const _: () = assert!(ANIMATABLE_COUNT <= 64, "the owned-keys mask is a u64");

/// May the anim door drive this key (GP40)? The `transition` list, plus
/// the live stop family — their numbers ride in the key, so the const
/// list can't carry them.
pub fn animatable(key: PropKey) -> bool {
    matches!(key, PropKey::StopColor(_) | PropKey::StopOffset(_)) || ANIMATABLE.contains(&key)
}

/// What a property rule compares: `node`'s `key` as the ladder and the
/// code writes leave it, before any slide. An unset margin or padding is
/// 0, so the first write to one slides from where the node sits.
fn standing_value(page: &Page, r: &Resolved, node: NodeId, key: PropKey) -> Option<Value> {
    if key.is_placement() {
        let edge: bool = matches!(key, PropKey::MarginLeft | PropKey::MarginTop | PropKey::MarginRight | PropKey::MarginBottom | PropKey::PaddingLeft | PropKey::PaddingTop | PropKey::PaddingRight | PropKey::PaddingBottom);
        placement_value(&r.placement[node.index()], key).or_else(|| edge.then_some(Value::VNum(0.0)))
    } else {
        r.prop(page, node, key).or_else(|| motion_default(key)).or_else(|| kind_visual_default(page, r, node, key))
    }
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
/// A driven slot's fraction for value `v`: over the control's LIVE
/// `min=`/`max=` when code wrote them on the root, else the contract's
/// baked numbers — the same rule the behavior reads (`live_behavior`).
/// Needed because the effect page's rows are pooled: one declared
/// control serves every filter's range, written at open. Colorize's
/// Saturation runs 0..1, and against the contract's baked 0..10 the
/// fill showed 7% at 70% (todo-bugs.md #6).
fn driver_fraction(page: &Page, r: &Resolved, d: &crate::page::AutoDriver, v: f32) -> f32 {
    let min = r.num(page, d.root, PropKey::Min).unwrap_or(d.min);
    let max = r.num(page, d.root, PropKey::Max).unwrap_or(d.max);
    if max > min { ((v - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 }
}

fn baked_frac(page: &Page, node: NodeId) -> f32 {
    match page.nodes[node].auto {
        Auto::Length(f) | Auto::Position(f) | Auto::Arc { frac: f, .. } | Auto::Angle { frac: f, .. } => f,
        _ => 0.0,
    }
}

/// The rule for a change on `node` (built-in state) or the view
/// (arrangements): exact pair > entering > leaving > bare. Going home with
/// no rule of its own mirrors the entering rule's timing, never its
/// `anim=`: an anim describes one journey, and played again on the way
/// back it would end at the far end.
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
            return rules.iter().find(|t| from_rest(t) && same(t.to, from)).map(|t| Transition { anim: NO_STR, ..**t });
        }
        // Leaving a built-in state with no rule: mirror its entering rule
        // (`any -> held` or `base -> held`).
        if to.is_none() {
            return rules.iter().find(|t| from_rest(t) && same(t.to, from)).map(|t| Transition { anim: NO_STR, ..**t });
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

/// Does a `when` on `node` for either end of a view flip (`from` or
/// `to`) set `key`? Tells a `go_instant` flip's own responses from other
/// changes in the same frame.
fn when_sets_key(page: &Page, node: NodeId, key: PropKey, from: Option<State>, to: Option<State>) -> bool {
    let n = &page.nodes[node];
    page.whens[n.whens.0 as usize..(n.whens.0 + n.whens.1) as usize].iter().any(|w| (Some(w.state) == from || Some(w.state) == to) && page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize].iter().any(|p| p.key == key))
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
/// in-flight tween values. Call once per frame after `tick`. The
/// by-value door: runs `Runtime::resolve_retained` and hands back a copy
/// of the look — for tests and one-off reads; a host that resolves every
/// frame borrows `Runtime::resolved()` instead.
pub fn resolve_animated(page: &Page, rt: &mut Runtime) -> Resolved {
    rt.resolve_retained(page).clone()
}

/// May this resolve patch the retained look `r` in place? `false` = a
/// full rebuild. The one list of the fallbacks (B36-DES stage 2):
/// - the road is switched off (`debug_set_retained`);
/// - nothing has been resolved yet;
/// - the node count changed (`grow` under `set_items`/`set_count`, or
///   any table out of step with the page);
/// - the theme variant changed (every declared color re-derefs; the
///   static look table is rebuilt);
/// - the locale changed;
/// - the static mask was switched.
/// Everything else — a state, custom-state, arrangement or screen flip,
/// a code write or its clearing, a tick — is found by diffing the
/// runtime against what the last resolve saw (`gather_reach`).
fn full_rebuild_needed(rt: &Runtime, r: &Resolved, n: usize) -> bool {
    let t = &rt.retained;
    let same_locale: bool = match (&r.locale, &rt.locale) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    };
    !t.on || !t.valid || r.placement.len() != n || r.override_span.len() != n || r.states.len() != n || t.ladder_placement.len() != n || t.watchers.by_owner.len() != n || r.variant != rt.variant || !same_locale || t.seen_mask_on != rt.static_mask_on
}

/// The retained resolve's body (`Runtime::resolve_retained`): `r` is the
/// look the last resolve left (empty before the first), patched into
/// the current one. The order is the shape of the old from-scratch
/// road, each pass narrowed to the nodes it can change:
/// 1. the rule anims for this frame's state changes (whole page, small);
/// 2. what the concerned nodes SHOWED, captured before anything moves —
///    the `from` of the tweens seeded in step 7;
/// 3. the `when` ladder: every node on a full rebuild, else the reached
///    set (`gather_reach`, `reladder`);
/// 4. the per-resolve copies (states, scroll, raised, rows, …);
/// 5. the tail's first half — the code writes — over the ladder tables,
///    then the drivers (GP33);
/// 6. the visibility triggers (GP19), folded only when some own word or
///    seat changed;
/// 7. the rect glides and the tween seeding, then the driver glides;
/// 8. the tail's second half — the in-flight tweens and the anim layer;
/// 9. `shown`, re-read for the reached and tail-touched nodes.
fn resolve_into(page: &Page, rt: &mut Runtime, r: &mut Resolved) {
    // The declared looks the static nodes' skipped loops read (B36-DES
    // stage 1) follow the theme variant and the row count.
    rt.refresh_static_look(page);
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
    let n = page.nodes.len();
    let full: bool = full_rebuild_needed(rt, r, n);
    // `go_instant` (GP37): the flip restyles but times nothing — its
    // change never meets a rule.
    let instant_flips = std::mem::take(&mut rt.instant_flips);
    let instant_view = |who: NodeId, to: &Option<State>| who == NO_NODE && matches!(to, Some(State::Arrangement(a)) if instant_flips.contains(a));
    // What the concerned nodes showed before this frame's flips — read
    // from the retained look now, before the ladder moves it. Only a
    // node the flip can reach is worth the per-key lookups: its own
    // state, a watched owner's, or the view's (the same `watches` test
    // the rule search below makes). A tile's HELD flip touches the tile
    // and its watchers, not all 1175 nodes — asking every node × key
    // first cost 10-17 ms per press on the effect page (B36 trace).
    // A static node (B36-DES stage 1) shows the same declared value
    // before and after any flip — no tween can be born on it — unless
    // the theme variant changed since the last look, when its declared
    // colors DO differ and the long road decides (it never spawns one
    // either way in practice; the fallback keeps the roads identical).
    let prev_variant: StrId = r.variant;
    let mut before: Vec<Before> = std::mem::take(&mut rt.retained.before);
    before.clear();
    if !changes.is_empty() && rt.retained.valid {
        for (i, node) in page.nodes.iter().enumerate() {
            let id = NodeId::from_index(i);
            if rt.is_static(id) && prev_variant == rt.variant {
                continue;
            }
            let concerned: bool = changes.iter().any(|(who, _, to)| !instant_view(*who, to) && (*who == NO_NODE || *who == id || page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize].iter().any(|w| w.of == *who)));
            if !concerned {
                continue;
            }
            let mut b = Before { node: id, owned: 0, current: [None; ANIMATABLE_COUNT] };
            for (k, key) in ANIMATABLE.iter().enumerate() {
                // GP40, user call: anim beats transition — a lane a
                // player owns (or a held pose keeps) spawns no tween.
                if rt.anim_owns_prop_now(id, *key) {
                    b.owned |= 1u64 << k;
                    continue;
                }
                b.current[k] = rt.tweens.iter().find(|t| t.node == id && t.key == *key).map(|t| t.value()).or_else(|| if key.is_placement() { r.placement.get(i).and_then(|p| placement_value(p, *key)) } else { r.prop(page, id, *key) });
            }
            before.push(b);
        }
    }
    // The ladder. The tail (everything from `override_tail` on) is
    // rebuilt below; the ladder part is rebuilt whole or patched for
    // the reached nodes.
    let mut reach: Vec<NodeId> = std::mem::take(&mut rt.retained.reach);
    reach.clear();
    if full {
        fill_ladder_full(page, rt, r);
    } else {
        r.overrides.truncate(r.override_tail as usize);
        rt.gather_reach(r, &mut reach);
        reladder(page, rt, r, &reach);
    }
    rt.retained.laddered = if full { n as u32 } else { reach.len() as u32 };
    // The per-resolve copies. `states` AFTER `gather_reach`, which diffs
    // the copy the last resolve took against the runtime's.
    r.variant = rt.variant;
    r.scroll.clone_from(&rt.scroll);
    r.safe = rt.safe;
    r.states.clone_from(&rt.states);
    let raised_changed: bool = r.raised != rt.raised;
    r.raised.clone_from(&rt.raised);
    r.locale.clone_from(&rt.locale);
    r.caret = rt.caret;
    r.selection = rt.selection;
    r.sel_handles = rt.sel_handles;
    let mut rows_changed: bool = r.repeat_rows.len() != rt.pools.len();
    r.repeat_rows.truncate(rt.pools.len());
    for (k, p) in rt.pools.iter().enumerate() {
        let rows = p.live.iter().map(|s| p.slots[*s as usize]);
        match r.repeat_rows.get_mut(k) {
            Some(e) => {
                if e.0 != p.repeat || !e.1.iter().copied().eq(rows.clone()) {
                    rows_changed = true;
                    e.0 = p.repeat;
                    e.1.clear();
                    e.1.extend(rows);
                }
            }
            None => {
                rows_changed = true;
                r.repeat_rows.push((p.repeat, rows.collect()));
            }
        }
    }
    // The tail, first half: the code writes win over states. A node the
    // tail touched last frame goes back to its ladder look first.
    r.anim_owned.clear();
    r.anim_owned.resize(n, 0);
    r.anim_owned_props.clear();
    let prev_touched: Vec<NodeId> = std::mem::take(&mut rt.retained.tail_touched);
    let mut touched: Vec<NodeId> = std::mem::take(&mut rt.retained.touched);
    touched.clear();
    if !full {
        for id in &prev_touched {
            let i = id.index();
            r.placement[i] = rt.retained.ladder_placement[i];
            r.motion[i] = rt.retained.ladder_motion[i];
            r.seat[i] = rt.retained.ladder_seat[i];
        }
    }
    for (id, p) in &rt.writes {
        if p.key.is_placement() {
            apply_placement(&mut r.placement[*id], page, *p);
        } else {
            push_tail(page, r, *id, *p);
        }
        touched.push(*id);
    }
    // GP33: the driver — each control's live value maps onto its
    // automatic slots' shown fraction (a glide in flight shows its eased
    // value); a presence slot follows its custom state on the control.
    // The presence list is compared with last frame's: a flip there
    // changes a presence slot's own visibility word below.
    let mut old_presence: Vec<(NodeId, bool)> = std::mem::take(&mut rt.retained.presence);
    std::mem::swap(&mut old_presence, &mut r.presence);
    r.presence.clear();
    r.auto.clear();
    for d in &page.drivers {
        if d.state != NO_STR {
            r.presence.push((d.node, rt.customs.iter().any(|e| *e == (d.root, d.state))));
            continue;
        }
        let shown = if let Some(g) = rt.auto_glides.iter().find(|g| g.node == d.node) {
            g.shown
        } else if let Some(v) = r.num(page, d.root, PropKey::Value) {
            driver_fraction(page, r, d, v)
        } else {
            rt.auto_rest.iter().find(|(n, _)| *n == d.node).map(|(_, f)| *f).unwrap_or_else(|| baked_frac(page, d.node))
        };
        r.auto.push((d.node, shown));
    }
    let presence_changed: bool = old_presence != r.presence;
    rt.retained.presence = old_presence;
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
        // The ladder's own word, before the mid-hide override — the
        // override must not hide the flip it exists to serve. Kept per
        // node between resolves (`own_visible`): re-read for the reached
        // nodes, and for every node after a rebuild or a presence flip.
        let mut own: Vec<bool> = std::mem::take(&mut rt.retained.own_visible);
        let mut own_changed: bool = false;
        if full || presence_changed || own.len() != n {
            own.clear();
            own.extend((0..n).map(|i| own_word(page, rt, r, NodeId::from_index(i))));
            own_changed = true;
        } else {
            for id in &reach {
                let word: bool = own_word(page, rt, r, *id);
                if own[id.index()] != word {
                    own[id.index()] = word;
                    own_changed = true;
                }
            }
        }
        // What the user sees is the node's own word folded with its
        // placed ancestors' — unchanged for every node when no own word
        // and no seat changed since the last resolve, so the fold and
        // the trigger scan run only then (or while nodes still await
        // their baseline).
        let seat_changed: bool = seat_table_moved(&rt.last_seat, &r.seat);
        let mut now: Vec<bool> = std::mem::take(&mut rt.retained.now);
        now.clear();
        if own_changed || seat_changed || rt.vis_seen.len() < n {
            for i in 0..n {
                let mut a = NodeId::from_index(i);
                now.push(loop {
                    if !own[a] {
                        break false;
                    }
                    // Placed: what the user sees folds the host a seat
                    // moved the node under, not the parent that wrote
                    // it (B25).
                    let parent = r.placed_parent(page, a);
                    if parent == NO_NODE {
                        break true;
                    }
                    a = parent;
                });
            }
        }
        rt.retained.own_visible = own;
        let baselined = rt.vis_seen.len().min(n);
        for i in 0..now.len() {
            let id = NodeId::from_index(i);
            let now: bool = now[i];
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
        rt.retained.now = now;
        // A hide fired or cancelled just above changes who defers — the
        // kept list predates it.
        r.hiding.clear();
        r.hiding.extend(rt.players.iter().filter(|p| !p.stopped).filter_map(|p| match p.finish {
            Finish::HideWhen(h) => Some(h),
            _ => None,
        }));
    }
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
    // The tween seeding: for every concerned node (captured above), a
    // key whose value the flip moved gets a tween from what showed to
    // the new ladder value, timed by the rule the flip meets. The
    // targets read the ladder with the code writes on it — the anim
    // layer lands later.
    for b in &before {
        let id: NodeId = b.node;
        let i = id.index();
        let node = &page.nodes[i];
        for (k, key) in ANIMATABLE.iter().enumerate() {
            if b.owned & (1u64 << k) != 0 {
                continue;
            }
            // A property rule times this key's every change (below); the
            // load check refuses a timed state rule over it.
            if page.prop_transitions.iter().any(|t| t.node == id && t.key == *key) {
                continue;
            }
            let target = if key.is_placement() { placement_value(&r.placement[i], *key) } else { r.prop(page, id, *key).or_else(|| motion_default(*key)).or_else(|| kind_visual_default(page, r, id, *key)) };
            let Some(target) = target else { continue };
            let current = b.current[k].or_else(|| motion_default(*key)).or_else(|| kind_visual_default(page, r, id, *key));
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
    // Property rules (`transition commandBar.opacity duration=0.4`): a
    // standing value (the ladder with the code writes on it) that moved
    // since the last resolve slides there from what shows — an in-flight
    // slide turns around from where it is. Snaps instead: the first value
    // seen, a node that doesn't show (nothing to watch), a lane an anim
    // owns (anim beats transition), a `set_prop_instant` value, and a
    // `go_instant` flip's own `when` responses (it times nothing).
    rt.prop_standing.resize(page.prop_transitions.len(), None);
    for (k, pt) in page.prop_transitions.iter().enumerate() {
        let id: NodeId = pt.node;
        let Some(target) = standing_value(page, r, id, pt.key) else { continue };
        if let Some((_, _, v)) = rt.instant_props.iter().find(|(n, key, _)| *n == id && *key == pt.key) {
            rt.prop_standing[k] = Some(*v);
            rt.tweens.retain(|t| !(t.node == id && t.key == pt.key));
        }
        let Some(standing) = rt.prop_standing[k] else {
            rt.prop_standing[k] = Some(target);
            continue;
        };
        if standing == target {
            continue;
        }
        rt.prop_standing[k] = Some(target);
        let from: Value = rt.tweens.iter().find(|t| t.node == id && t.key == pt.key).map(|t| t.value()).unwrap_or(standing);
        rt.tweens.retain(|t| !(t.node == id && t.key == pt.key));
        let shows: bool = rt.vis_seen.get(id.index()).copied().unwrap_or(false);
        let by_instant_flip: bool = changes.iter().any(|(who, from, to)| instant_view(*who, to) && when_sets_key(page, id, pt.key, *from, *to));
        if !shows || by_instant_flip || rt.anim_owns_prop_now(id, pt.key) {
            continue;
        }
        let easing = if pt.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(pt.easing)).unwrap_or(Easing::LINEAR) };
        rt.tweens.push(Tween { node: id, key: pt.key, from, to: target, elapsed: 0.0, duration: pt.duration, easing });
    }
    rt.instant_props.clear();
    // GP33: a driver target that moved re-arms the fraction glide by the
    // control's default feel (the endpoint-less `transition` on the
    // instance root); no rule or duration 0 snaps. A glide already
    // flying retargets from its shown value.
    for d in &page.drivers {
        if d.state != NO_STR {
            continue;
        }
        let Some(v) = r.num(page, d.root, PropKey::Value) else { continue };
        let target = driver_fraction(page, r, d, v);
        // GP53: while the finger holds the control (`held` on its root)
        // a write lands DIRECT — the thumb stays under the finger, and a
        // glide in flight is dropped at the value; the release's settle
        // glides again, `held` being off by then (the addon's mid-drag
        // rule: "mid-drag writes land directly, discrete changes
        // glide"). Without this the effect page's switch, feel 0.3 s,
        // trailed the finger by a glide per sample.
        if rt.has(d.root, HELD) {
            rt.auto_glides.retain(|g| g.node != d.node);
            if let Some(e) = rt.auto_rest.iter_mut().find(|(n, _)| *n == d.node) {
                e.1 = target;
            }
            if let Some(e) = r.auto.iter_mut().find(|(n, _)| *n == d.node) {
                e.1 = target;
            }
            continue;
        }
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
    // The tail, second half. In-flight tweens overlay (they retarget
    // from live values by construction); a motion tween lands in the
    // node's transform as it is pushed.
    for t in &rt.tweens {
        let v = t.value();
        if t.key.is_placement() {
            apply_placement(&mut r.placement[t.node], page, Prop { key: t.key, value: v });
        } else {
            push_tail(page, r, t.node, Prop { key: t.key, value: v });
        }
        touched.push(t.node);
    }
    // The anim layer goes on last: an owning anim outranks a tween too.
    compose_anim(page, rt, r, &mut touched);
    // The Runtime's copy of the seat table (`last_seat`): the walks that
    // run without this snapshot read placed parents from it. A move
    // since the last copy sends the `when disabled` watchers through
    // their ladder next resolve (`gather_reach`).
    rt.retained.seat_moved = seat_table_moved(&rt.last_seat, &r.seat);
    rt.last_seat.clone_from(&r.seat);
    // The placed tree the walks read (`Resolved::tree`): rebuilt only when
    // something that shapes it moved — a seat, a repeat's rows, a raise,
    // the node count — else last resolve's tables stand.
    if full || raised_changed || rows_changed || rt.retained.seat_moved || r.tree.len() != n {
        r.tree.build(page, &r.seat, &r.repeat_rows, &r.raised);
    }
    // What shows now, per node — the live value a new play starts from.
    // Re-read for the nodes this resolve moved: the reached ones and
    // the ones the tail touched, last frame or this; every node after
    // a rebuild.
    // A static node (B36-DES stage 1) shows its declared opacity and
    // fill alpha, read from the table instead of two override walks;
    // its motion is kept as is — a `tap_pulse` swing lands there through
    // `compose_anim` whatever the node's rule. The one exception is a
    // tween standing on a static node (born on the variant-switch road
    // above): that node reads the long way while the tween lives.
    rt.shown.resize(n, Motion::IDENTITY);
    let shown_of = |rt: &Runtime, r: &Resolved, id: NodeId| -> Motion {
        let i = id.index();
        let mut m = r.motion[i];
        if rt.is_static(id) && !rt.tweens.iter().any(|t| t.node == id) {
            m.opacity = rt.static_look.opacity[i];
            m.color_a = rt.static_look.color_a[i];
        } else {
            m.opacity = r.num(page, id, PropKey::Opacity).unwrap_or(1.0);
            m.color_a = r.color(page, id, PropKey::Color).map(|c| c.0[3] as f32 / 255.0).unwrap_or(1.0);
        }
        m
    };
    if full {
        for i in 0..n {
            let m = shown_of(rt, r, NodeId::from_index(i));
            rt.shown[i] = m;
        }
    } else {
        for k in 0..reach.len() + prev_touched.len() + touched.len() {
            let id: NodeId = if k < reach.len() {
                reach[k]
            } else if k < reach.len() + prev_touched.len() {
                prev_touched[k - reach.len()]
            } else {
                touched[k - reach.len() - prev_touched.len()]
            };
            let m = shown_of(rt, r, id);
            rt.shown[id] = m;
        }
    }
    // The report for the retained layout and draw list (B36-DES stage 3).
    rt.resolve_changes.serial = rt.resolve_changes.serial.wrapping_add(1);
    rt.resolve_changes.full = full;
    rt.resolve_changes.rewritten.clear();
    rt.resolve_changes.rewritten.append(&mut rt.pending_rewrites);
    // What the next resolve diffs against, and the scratch buffers back.
    rt.retained.valid = true;
    rt.retained.seen_arrangement = rt.arrangement;
    rt.retained.seen_screen.clone_from(&rt.screen);
    rt.retained.seen_customs.clone_from(&rt.customs);
    rt.retained.seen_mask_on = rt.static_mask_on;
    rt.retained.tail_touched = touched;
    rt.retained.touched = prev_touched;
    rt.retained.reach = reach;
    rt.retained.before = before;
    let _ = lerp_value;
}

/// The nodes a resolve must run through their ladder, from what changed
/// since the last one (B36-DES stage 2): the runtime's state bits
/// against the copy the last look took (a changed bit on W reaches
/// `by_owner[W]`; a changed DISABLED bit anywhere, or a seat move last
/// resolve, reaches every `when disabled`), the arrangement and screen
/// words against what was seen (a change reaches every view `when`),
/// the raised custom states against what was seen (a root raised or
/// cleared reaches `by_owner[root]`), plus the nodes with a write added
/// or taken back. Sorted, no duplicates.
///
/// Diffed, not logged, on purpose: `states`, `arrangement` and `screen`
/// are public fields and tests write `variant` bare; the data is the
/// truth, whoever wrote it. The write list is the exception (it can be
/// hundreds of entries on a pooled page), and its only doors are
/// `set_prop` and `clear_prop`, which log.
impl Runtime {
    fn gather_reach(&mut self, r: &Resolved, out: &mut Vec<NodeId>) {
        let t = &mut self.retained;
        // A seat move changes which ancestors `disabled` folds — it
        // matters only while some node carries DISABLED at all.
        let mut disabled_flip: bool = std::mem::take(&mut t.seat_moved) && self.states.iter().any(|s| s & DISABLED != 0);
        for (i, (now, seen)) in self.states.iter().zip(&r.states).enumerate() {
            let flipped: u8 = now ^ seen;
            if flipped == 0 {
                continue;
            }
            out.extend_from_slice(&t.watchers.by_owner[i]);
            if flipped & DISABLED != 0 {
                disabled_flip = true;
            }
        }
        if self.arrangement != t.seen_arrangement || self.screen != t.seen_screen {
            out.extend_from_slice(&t.watchers.view);
        }
        for e in self.customs.iter().chain(&t.seen_customs) {
            if !(self.customs.contains(e) && t.seen_customs.contains(e)) {
                out.extend_from_slice(&t.watchers.by_owner[e.0]);
            }
        }
        out.append(&mut t.write_flips);
        if disabled_flip {
            out.extend_from_slice(&t.watchers.disabled);
        }
        out.sort();
        out.dedup();
    }
}

/// The full rebuild of the ladder part, in place: every node through
/// `ladder_node`, the override list in node order with the spans, and
/// the ladder tables refilled. The buffers keep their capacity.
fn fill_ladder_full(page: &Page, rt: &mut Runtime, r: &mut Resolved) {
    let n = page.nodes.len();
    r.overrides.clear();
    r.override_span.clear();
    r.placement.clear();
    r.motion.clear();
    r.seat.clear();
    let mut ladder_placement: Vec<Placement> = std::mem::take(&mut rt.retained.ladder_placement);
    let mut ladder_motion: Vec<Motion> = std::mem::take(&mut rt.retained.ladder_motion);
    let mut ladder_seat: Vec<NodeId> = std::mem::take(&mut rt.retained.ladder_seat);
    ladder_placement.clear();
    ladder_motion.clear();
    ladder_seat.clear();
    for i in 0..n {
        let id = NodeId::from_index(i);
        let start: usize = r.overrides.len();
        let placement: Placement = ladder_node(page, rt, id, &mut r.overrides);
        r.override_span.push((start as u32, (r.overrides.len() - start) as u32));
        let (m, seat) = ladder_motion_and_seat(page, rt, id, &r.overrides[start..]);
        r.placement.push(placement);
        ladder_placement.push(placement);
        r.motion.push(m);
        ladder_motion.push(m);
        r.seat.push(seat);
        ladder_seat.push(seat);
    }
    r.override_tail = r.overrides.len() as u32;
    rt.retained.ladder_placement = ladder_placement;
    rt.retained.ladder_motion = ladder_motion;
    rt.retained.ladder_seat = ladder_seat;
    // Everything is fresh: nothing to put back, no write left to reach.
    rt.retained.tail_touched.clear();
    rt.retained.write_flips.clear();
}

/// Patches the reached nodes' ladder answers in place. `r.overrides`
/// holds the ladder part only (the tail was cut off) in node order, so
/// a node whose entries changed has its span spliced — the entries
/// after it shift by the difference, and one pass at the end moves
/// every later span start by the deltas before it, so the list reads
/// exactly as a full rebuild would write it (`override_of` stays a read
/// of the node's own few entries).
fn reladder(page: &Page, rt: &mut Runtime, r: &mut Resolved, reach: &[NodeId]) {
    let mut entries: Vec<(NodeId, Prop)> = std::mem::take(&mut rt.retained.entries);
    let mut fixes: Vec<(usize, i64)> = std::mem::take(&mut rt.retained.fixes);
    fixes.clear();
    // The sum of the deltas spliced so far: the recorded start of every
    // later span is stale by this much until the pass below.
    let mut shift: i64 = 0;
    for id in reach {
        let i = id.index();
        entries.clear();
        let placement: Placement = ladder_node(page, rt, *id, &mut entries);
        let (start0, len) = r.override_span[i];
        let start: usize = (start0 as i64 + shift) as usize;
        let len: usize = len as usize;
        if r.overrides[start..start + len] != entries[..] {
            r.overrides.splice(start..start + len, entries.iter().copied());
            let delta: i64 = entries.len() as i64 - len as i64;
            if delta != 0 {
                fixes.push((i, delta));
                shift += delta;
            }
            r.override_span[i].1 = entries.len() as u32;
        }
        let (m, seat) = ladder_motion_and_seat(page, rt, *id, &entries);
        r.placement[i] = placement;
        rt.retained.ladder_placement[i] = placement;
        r.motion[i] = m;
        rt.retained.ladder_motion[i] = m;
        r.seat[i] = seat;
        rt.retained.ladder_seat[i] = seat;
    }
    // Any splice moves the spans after it, even when the deltas sum to
    // nothing (+2 then -2 leaves the spans between them shifted).
    if !fixes.is_empty() {
        let mut k: usize = 0;
        let mut run: i64 = 0;
        for (j, span) in r.override_span.iter_mut().enumerate() {
            while k < fixes.len() && fixes[k].0 < j {
                run += fixes[k].1;
                k += 1;
            }
            if run != 0 {
                span.0 = (span.0 as i64 + run) as u32;
            }
        }
        r.override_tail = (r.override_tail as i64 + shift) as u32;
    }
    rt.retained.entries = entries;
    rt.retained.fixes = fixes;
}

/// The node's own visibility word: does it show itself (the `own` half
/// of the GP19 fold — the ancestors are folded by the caller). A static
/// node's word is its declared one (B36-DES stage 1), read from the
/// table with no override walk; a repeat template never shows; a
/// presence slot follows its live state, not the bake (GP33).
fn own_word(page: &Page, rt: &Runtime, r: &Resolved, id: NodeId) -> bool {
    let i = id.index();
    if rt.is_static(id) {
        return rt.static_look.own_visible[i];
    }
    !page.repeats.iter().any(|rp| rp.template == id)
        && match page.nodes[i].auto {
            Auto::Presence(baked) => r.presence.iter().find(|(n, _)| *n == id).map(|(_, b)| *b).unwrap_or(baked),
            _ => true,
        }
        && !matches!(r.ident(page, id, PropKey::Visibility), Some("gone") | Some("space_only"))
}

/// Does any node sit under a different placed parent in `now` than in
/// `last` (the seat table a previous resolve left)? A table shorter
/// than the arena — empty before the first resolve, or behind a `grow`
/// — reads as "at home" for the missing rows, the way
/// `placed_parent_in` reads it.
fn seat_table_moved(last: &[NodeId], now: &[NodeId]) -> bool {
    now.iter().enumerate().any(|(i, s)| last.get(i).copied().unwrap_or(NO_NODE) != *s)
}

/// Pushes one tail entry (a code write, a tween value) and lands what
/// it means for the placed tree and the draw transform at once — the
/// seat word re-parents, a motion word sets the transform — over the
/// node's ladder look. Placement keys never come here (they apply to
/// `Resolved::placement`).
fn push_tail(page: &Page, r: &mut Resolved, id: NodeId, p: Prop) {
    r.overrides.push((id, p));
    match p.key {
        PropKey::SeatOf => r.seat[id] = seat_of(page, id, p.value),
        _ => set_motion_word(&mut r.motion[id], p.key, p.value),
    }
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

/// `s` (a string of `old_page`) as an id in `new_page`'s pool — a hot
/// reload's carry (`Runtime::carried_to`). `NO_STR` stays `NO_STR`.
fn reintern(old_page: &Page, new_page: &mut Page, s: StrId) -> StrId {
    if s == NO_STR {
        return NO_STR;
    }
    new_page.intern(old_page.str(s))
}
