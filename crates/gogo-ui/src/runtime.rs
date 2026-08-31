//! The runtime part of a page: which states hold, the active arrangement
//! and theme variant, scroll positions. `resolve` overlays the active
//! `when` responses onto the base props in the spec's priority order; the
//! layout and draw passes read only the `Resolved` result.

use crate::anim::{Channel, Easing, Finish, Motion, Player, Tween, lerp_value};
use crate::layout::{BUTTON_PADDING, Rect};
use crate::page::*;

pub const HELD: u8 = 1;
pub const HOVERED: u8 = 2;
pub const FOCUSED: u8 = 4;
pub const FOCUS_WITHIN: u8 = 8;
pub const SELECTED: u8 = 16;
pub const DISABLED: u8 = 32;

/// `base` — no arrangement active.
pub const BASE: u32 = u32::MAX;

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
    /// Per node, what SHOWED after the last `resolve_animated`: the
    /// composed motion, the node's own opacity and the fill's alpha — the
    /// live value a new play starts from.
    pub shown: Vec<Motion>,
    /// A timed arrangement flip started this frame: every box whose laid-out
    /// rectangle changes glides there over (duration, easing) — alignment,
    /// fill, hug, cap and seat moves all tween through their final rects.
    pub glide_start: Option<(f32, Easing)>,
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
            safe: [0.0; 4],
            writes: Vec::new(),
            changes: Vec::new(),
            tweens: Vec::new(),
            held: Vec::new(),
            shown: vec![Motion::IDENTITY; page.nodes.len()],
            players: Vec::new(),
            motion: vec![Motion::IDENTITY; page.nodes.len()],
            owned_text: vec![NO_STR; page.nodes.len()],
            last: Vec::new(),
            glide_start: None,
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
            if duration > 0.0 && self.last_rects.len() == rects.len() {
                for i in 0..rects.len() {
                    if self.last_rects[i] != rects[i] {
                        self.rect_tweens.retain(|t| t.0 != i as NodeId);
                        self.rect_tweens.push((i as NodeId, self.last_rects[i], rects[i], 0.0, duration, easing));
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
        if visible {
            self.players.retain(|p| p.finish != Finish::Hide(node));
            self.set_visible(page, node, true);
            self.play_action(page, node, "show", Finish::Nothing);
        } else if !self.play_action(page, node, "hide", Finish::Hide(node)) {
            self.set_visible(page, node, false);
        }
    }

    /// Plays every visible node's `on:show` once (the entrance on first
    /// build). A node under a `gone` ancestor is not on screen: no entrance.
    pub fn play_entrances(&mut self, page: &Page) {
        let gone: Vec<bool> = (0..page.nodes.len()).map(|i| {
            let n = i as NodeId;
            page.prop(n, PropKey::Visibility).map(|v| matches!(v, Value::VIdent(id) if page.str(id) == "gone")).unwrap_or(false) || self.writes.iter().any(|(w, p)| *w == n && p.key == PropKey::Visibility && matches!(p.value, Value::VIdent(id) if page.str(id) == "gone"))
        }).collect();
        for i in 0..page.nodes.len() {
            let mut n = i as NodeId;
            let mut hidden = false;
            loop {
                if gone[n as usize] {
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

    /// A `repeat x max=N` node's built items, in order (its `empty { }`
    /// child is not one).
    pub fn items(&self, page: &Page, repeat: NodeId) -> Vec<NodeId> {
        page.children(repeat).map(|i| i as NodeId).filter(|c| page.nodes[*c as usize].kind != Kind::Empty).collect()
    }

    /// Shows the first `n` items of a `repeat x max=N` node (the rest are
    /// `gone`); its `empty { }` content shows only at zero. More than N
    /// clamps to N.
    pub fn set_count(&mut self, page: &mut Page, repeat: NodeId, n: usize) {
        let items = self.items(page, repeat);
        for (i, item) in items.iter().enumerate() {
            self.set_visible(page, *item, i < n);
        }
        let empties: Vec<NodeId> = page.children(repeat).map(|i| i as NodeId).filter(|c| page.nodes[*c as usize].kind == Kind::Empty).collect();
        for e in empties {
            self.set_visible(page, e, n == 0);
        }
    }

    /// Is `anim` mid-play?
    pub fn is_playing(&self, anim: StrId) -> bool {
        self.players.iter().any(|p| p.anim == anim && !p.stopped)
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
        let mut finished: Vec<(Finish, Vec<(NodeId, Channel)>)> = Vec::new();
        let mut players = std::mem::take(&mut self.players);
        players.retain_mut(|p| {
            let alive = p.tick(dt, &mut self.motion);
            if !alive {
                finished.push((p.finish, p.owns.clone()));
            }
            alive
        });
        moving |= !players.is_empty() || !finished.is_empty();
        self.players = players;
        for (f, owns) in finished {
            if let Finish::Hide(node) = f {
                self.set_visible(page, node, false);
                self.motion[node as usize] = Motion::IDENTITY;
            } else {
                // A finished anim's end pose is a standing value the author
                // chose (WPF HoldEnd): it stays until the next writer.
                for (node, channel) in owns {
                    self.held.retain(|h| !(h.0 == node && h.1 == channel));
                    self.held.push((node, channel, self.motion[node as usize].get(channel)));
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
        self.set_prop(node, PropKey::Text, Value::VText { text: id, key: NO_STR });
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

    pub fn go(&mut self, arrangement: u32) {
        if self.arrangement != arrangement {
            let from = self.arrangement;
            self.arrangement = arrangement;
            self.dirty = true;
            self.changes.push((NO_NODE, Some(State::Arrangement(from)), Some(State::Arrangement(arrangement))));
        }
    }

    pub fn toggle(&mut self, arrangement: u32) {
        let target = if self.arrangement == arrangement { BASE } else { arrangement };
        self.go(target);
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
            .or_else(|| page.kind_default(page.nodes[node as usize].kind, key))?;
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
        let step = self.num(page, node, PropKey::Step).unwrap_or(0.0);
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
        let mut n = node;
        loop {
            let own = self
                .overrides
                .iter()
                .rev()
                .find(|(i, p)| *i == n && p.key == PropKey::Font)
                .map(|(_, p)| p.value)
                .or_else(|| page.prop(n, PropKey::Font));
            if let Some(v) = own {
                return match deref(page, self.variant, v) {
                    Value::VText { text, .. } => Some(text),
                    Value::VIdent(id) => Some(id),
                    _ => None,
                };
            }
            let parent = page.nodes[n as usize].parent;
            if parent == NO_NODE {
                break;
            }
            n = parent;
        }
        match page.kind_default(Kind::Label, PropKey::Font).map(|v| deref(page, self.variant, v)) {
            Some(Value::VText { text, .. }) => Some(text),
            _ => None,
        }
    }

    /// `visibility=gone` — out of layout. An `automatic=presence` slot
    /// whose state is off is gone too.
    pub fn gone(&self, page: &Page, node: NodeId) -> bool {
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

    /// `visibility=hidden` — laid out, not drawn, no input.
    pub fn hidden(&self, page: &Page, node: NodeId) -> bool {
        self.ident(page, node, PropKey::Visibility) == Some("hidden")
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
    let mut r = Resolved { anim_owned: vec![0; page.nodes.len()], placement: Vec::with_capacity(page.nodes.len()), overrides: Vec::new(), variant: rt.variant, scroll: rt.scroll.clone(), safe: rt.safe, motion: vec![Motion::IDENTITY; page.nodes.len()], seat: vec![NO_NODE; page.nodes.len()], states: rt.states.clone(), raised: rt.raised.clone() };
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
    let align = |v: Value| match v {
        Value::VIdent(id) => match page.str(id) {
            "left" | "top" | "start" => Align::Start,
            "center" => Align::Center,
            "right" | "bottom" | "end" => Align::End,
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
        PropKey::Halign => p.halign = align(prop.value),
        PropKey::Valign => p.valign = align(prop.value),
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
    let mut r = resolve_ladder(page, rt);
    // An arrangement flip with a timed view rule: the boxes it re-poses
    // glide to their new rects (`Runtime::glide`).
    for (who, from, to) in &changes {
        if *who == NO_NODE {
            if let Some(rule) = find_rule(page, NO_NODE, *from, *to) {
                if rule.duration > 0.0 {
                    let easing = if rule.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(rule.easing)).unwrap_or(Easing::LINEAR) };
                    rt.glide_start = Some((rule.duration, easing));
                }
            }
        }
    }
    if !changes.is_empty() && !rt.last.is_empty() {
        for (i, node) in page.nodes.iter().enumerate() {
            let id = i as NodeId;
            let _ = node;
            for key in ANIMATABLE {
                let target = if key.is_placement() { placement_value(&r.placement[i], *key) } else { r.prop(page, id, *key).or_else(|| motion_default(*key)) };
                let Some(target) = target else { continue };
                let current = rt.tweens.iter().find(|t| t.node == id && t.key == *key).map(|t| t.value()).or_else(|| rt.last.iter().find(|(n, p)| *n == id && p.key == *key).map(|(_, p)| p.value)).or_else(|| motion_default(*key));
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
                    let node_rule = if scope == NO_NODE { find_rule(page, id, *from, *to).filter(|t| t.from.is_some() && t.to.is_some()) } else { find_rule(page, id, *from, *to) };
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
        rt.go(0);
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
