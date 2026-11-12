//! Motion: WPF-exact easing math, property tweens for `transition`
//! rules, and players for `anim` tracks — the motion channels
//! (`translate.x/.y`, `scale`, `rotate`, `opacity`) plus PROP lanes
//! (GP40: any word a `transition` rule can move — `b.background`,
//! `card.corner_radius`, `g.stop_1_color.a`). Layout is declared, motion
//! is transformed: a player never touches a rect in the arena; the draw
//! walk applies the channels, and prop lanes land as resolve overrides.

use crate::layout::Rect;
use crate::page::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Linear,
    Sine,
    Quad,
    Cubic,
    Quart,
    Quint,
    Circle,
    Expo,
    Power,
    Back,
    Bounce,
    Elastic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    In,
    Out,
    InOut,
}

/// The parameter words a curve takes next to `easing=` (`bounces=1,
/// bounciness=2.4`), in the order `Easing::params` stores them.
pub const PARAMS_FOR_CURVE: &[(Curve, &[&str])] = &[
    (Curve::Expo, &["exponent"]),
    (Curve::Power, &["power"]),
    (Curve::Back, &["amplitude"]),
    (Curve::Bounce, &["bounces", "bounciness"]),
    (Curve::Elastic, &["oscillations", "springiness"]),
];

/// Every parameter word any curve takes.
pub const EASING_PARAM_KEYS: &[&str] = &["exponent", "power", "amplitude", "bounces", "bounciness", "oscillations", "springiness"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Easing {
    pub curve: Curve,
    pub mode: Mode,
    /// The curve's parameters in `PARAMS_FOR_CURVE` order; `None` = the
    /// WPF default (Expo exponent 2, Power 2, Back amplitude 1, Bounce 3
    /// bounces at bounciness 2, Elastic 3 oscillations at springiness 3).
    pub params: [Option<f32>; 2],
}

impl Easing {
    pub const LINEAR: Easing = Easing { curve: Curve::Linear, mode: Mode::Out, params: [None, None] };
    /// `cubic_out`: fast off the mark, easing into the end.
    pub const CUBIC_OUT: Easing = Easing { curve: Curve::Cubic, mode: Mode::Out, params: [None, None] };

    /// `cubic_out`, `back_in_out`, bare `cubic` = ease-out (spec). A
    /// parameter list may follow: `bounce_out(1,2.4)` — the form the
    /// compiler writes into the page's strings so a track carries its
    /// easing as one word.
    pub fn parse(name: &str) -> Option<Easing> {
        let (name, param_text) = match name.split_once('(') {
            Some((n, rest)) => (n, Some(rest.trim_end_matches(')'))),
            None => (name, None),
        };
        let (base, mode) = if let Some(b) = name.strip_suffix("_in_out") {
            (b, Mode::InOut)
        } else if let Some(b) = name.strip_suffix("_out") {
            (b, Mode::Out)
        } else if let Some(b) = name.strip_suffix("_in") {
            (b, Mode::In)
        } else {
            (name, Mode::Out)
        };
        let curve = match base {
            "linear" => Curve::Linear,
            "sine" => Curve::Sine,
            "quad" => Curve::Quad,
            "cubic" => Curve::Cubic,
            "quart" => Curve::Quart,
            "quint" => Curve::Quint,
            "circle" => Curve::Circle,
            "expo" => Curve::Expo,
            "power" => Curve::Power,
            "back" => Curve::Back,
            "bounce" => Curve::Bounce,
            "elastic" => Curve::Elastic,
            _ => return None,
        };
        let mut params: [Option<f32>; 2] = [None, None];
        if let Some(text) = param_text {
            for (i, part) in text.split(',').enumerate().take(2) {
                let part = part.trim();
                if !part.is_empty() {
                    params[i] = part.parse().ok();
                }
            }
        }
        Some(Easing { curve, mode, params })
    }

    /// The one-word form `parse` reads back: `bounce_out(1,2.4)`.
    pub fn to_word(&self) -> String {
        let base = match self.curve {
            Curve::Linear => "linear",
            Curve::Sine => "sine",
            Curve::Quad => "quad",
            Curve::Cubic => "cubic",
            Curve::Quart => "quart",
            Curve::Quint => "quint",
            Curve::Circle => "circle",
            Curve::Expo => "expo",
            Curve::Power => "power",
            Curve::Back => "back",
            Curve::Bounce => "bounce",
            Curve::Elastic => "elastic",
        };
        let mode = match self.mode {
            Mode::In => "_in",
            Mode::Out => "_out",
            Mode::InOut => "_in_out",
        };
        let mut word = format!("{base}{mode}");
        if self.params.iter().any(|p| p.is_some()) {
            let a = self.params[0].map(|v| v.to_string()).unwrap_or_default();
            let b = self.params[1].map(|v| v.to_string()).unwrap_or_default();
            word.push_str(&format!("({a},{b})"));
        }
        word
    }

    /// Eased progress for `t` in 0..1 (WPF EasingFunctionBase math).
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self.mode {
            Mode::In => ease_in(self.curve, self.params, t),
            Mode::Out => 1.0 - ease_in(self.curve, self.params, 1.0 - t),
            Mode::InOut => {
                if t < 0.5 {
                    ease_in(self.curve, self.params, t * 2.0) * 0.5
                } else {
                    (1.0 - ease_in(self.curve, self.params, (1.0 - t) * 2.0)) * 0.5 + 0.5
                }
            }
        }
    }
}

/// The ease-in shapes (WPF *Ease.EaseInCore).
fn ease_in(curve: Curve, params: [Option<f32>; 2], t: f32) -> f32 {
    let p0 = |default: f32| params[0].unwrap_or(default);
    let p1 = |default: f32| params[1].unwrap_or(default);
    match curve {
        Curve::Linear => t,
        Curve::Sine => 1.0 - (std::f32::consts::FRAC_PI_2 * (1.0 - t)).sin(),
        Curve::Quad => t * t,
        Curve::Cubic => t * t * t,
        Curve::Quart => t * t * t * t,
        Curve::Quint => t * t * t * t * t,
        Curve::Circle => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
        Curve::Power => t.powf(p0(2.0).max(0.0)),
        Curve::Expo => {
            let e = p0(2.0);
            if e.abs() < 1e-10 {
                t
            } else {
                ((e * t).exp() - 1.0) / (e.exp() - 1.0)
            }
        }
        Curve::Back => {
            let a = p0(1.0).max(0.0);
            t * t * t - t * a * (t * std::f32::consts::PI).sin()
        }
        Curve::Bounce => bounce_ease_in(p0(3.0), p1(2.0), t),
        Curve::Elastic => {
            let oscillations = p0(3.0).max(0.0);
            let springiness = p1(3.0).max(0.0);
            let expo = if springiness.abs() < 1e-10 { t } else { ((springiness * t).exp() - 1.0) / (springiness.exp() - 1.0) };
            expo * ((std::f32::consts::TAU * oscillations + std::f32::consts::FRAC_PI_2) * t).sin()
        }
    }
}

/// WPF BounceEase.EaseInCore, the reference source's geometric-series
/// form: bounce widths grow by `bounciness` each bounce (the last counted
/// at half weight), and each bounce is a quadratic arc.
fn bounce_ease_in(bounces: f32, bounciness: f32, t: f32) -> f32 {
    let bounces = bounces.floor().max(0.0);
    // Bounciness at or below 1 divides by zero below; WPF clamps the same way.
    let bounciness = if bounciness < 1.0 || (bounciness - 1.0).abs() < 1e-10 { 1.001 } else { bounciness };
    let pow_bb = bounciness.powf(bounces);
    let one_minus = 1.0 - bounciness;
    let sum_of_units = (1.0 - pow_bb) / one_minus + pow_bb * 0.5;
    let unit_at_t = t * sum_of_units;
    // Which bounce holds t: solve unit_at_t = (1 - b^n) / (1 - b) for n.
    let bounce_at_t = (-unit_at_t * (1.0 - bounciness) + 1.0).ln() / bounciness.ln();
    let start = bounce_at_t.floor();
    let end = start + 1.0;
    let start_time = (1.0 - bounciness.powf(start)) / (one_minus * sum_of_units);
    let end_time = (1.0 - bounciness.powf(end)) / (one_minus * sum_of_units);
    let mid_time = (start_time + end_time) * 0.5;
    let time_relative_to_peak = t - mid_time;
    let radius = mid_time - start_time;
    let amplitude = (1.0 / bounciness).powf(bounces - start);
    (-amplitude / (radius * radius)) * (time_relative_to_peak - radius) * (time_relative_to_peak + radius)
}

/// A property tween from a `transition` rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub node: NodeId,
    pub key: PropKey,
    pub from: Value,
    pub to: Value,
    pub elapsed: f32,
    pub duration: f32,
    pub easing: Easing,
}

impl Tween {
    pub fn value(&self) -> Value {
        let t = if self.duration <= 0.0 { 1.0 } else { self.easing.apply(self.elapsed / self.duration) };
        lerp_value(self.from, self.to, t)
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Numbers and colors interpolate; anything else snaps at the end.
pub fn lerp_value(a: Value, b: Value, t: f32) -> Value {
    match (a, b) {
        (Value::VNum(x), Value::VNum(y)) => Value::VNum(lerp(x, y, t)),
        // Authored colors mix in premultiplied Oklab (the one law,
        // `crate::color`) — the same math the gradient shader runs.
        (Value::VColor(x), Value::VColor(y)) => Value::VColor(crate::color::mix_rgba(x, y, t)),
        (Value::VSize(aw, ah), Value::VSize(bw, bh)) => Value::VSize(match (aw, bw) {
            (Some(x), Some(y)) => Some(lerp(x, y, t)),
            _ => bw,
        }, match (ah, bh) {
            (Some(x), Some(y)) => Some(lerp(x, y, t)),
            _ => bh,
        }),
        _ => {
            if t >= 1.0 { b } else { a }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    TranslateX,
    TranslateY,
    ScaleX,
    ScaleY,
    Rotate,
    /// The turnstile swing about the node's left edge, degrees (3D).
    Swing,
    /// The node's own opacity — the SAME value `opacity=` declares and a
    /// `when … opacity=` holds; an anim writes that one channel, it does
    /// not multiply it (decided 2026-08-30, E1: one property, one writer).
    Opacity,
    /// The fill's alpha (`x.color.a`): the color keeps its hue, alpha
    /// alone moves — the node's opacity is untouched.
    ColorA,
    /// How much of the box shows along an axis (GP40 reveal): layout
    /// space grows with the fraction, so these compose as overrides the
    /// layout reads, not as draw transforms.
    RevealX,
    RevealY,
}

impl Channel {
    /// `translate.x`, `scale`, `rotate`, `opacity`, `color.a` → channels
    /// (uniform `scale` = both axes). `modulate.a` is a Godot engine word,
    /// not a gogo one — the compiler names `opacity` as the fix.
    pub fn parse(word: &str) -> Option<Vec<Channel>> {
        Some(match word {
            "translate.x" => vec![Channel::TranslateX],
            "translate.y" => vec![Channel::TranslateY],
            "scale" => vec![Channel::ScaleX, Channel::ScaleY],
            "scale.x" => vec![Channel::ScaleX],
            "scale.y" => vec![Channel::ScaleY],
            "rotate" => vec![Channel::Rotate],
            "swing" => vec![Channel::Swing],
            "opacity" => vec![Channel::Opacity],
            "color.a" => vec![Channel::ColorA],
            "reveal_x" => vec![Channel::RevealX],
            "reveal_y" => vec![Channel::RevealY],
            _ => return None,
        })
    }

    /// The channels a `when`/code write of `key` lands on — what a held
    /// anim value yields to (`color=` rewrites the whole fill, alpha too).
    pub fn of_key(key: PropKey) -> &'static [Channel] {
        match key {
            PropKey::Scale => &[Channel::ScaleX, Channel::ScaleY],
            PropKey::ScaleX => &[Channel::ScaleX],
            PropKey::ScaleY => &[Channel::ScaleY],
            PropKey::TranslateX => &[Channel::TranslateX],
            PropKey::TranslateY => &[Channel::TranslateY],
            PropKey::Rotate => &[Channel::Rotate],
            PropKey::Opacity => &[Channel::Opacity],
            PropKey::ColorA | PropKey::Color => &[Channel::ColorA],
            PropKey::RevealX => &[Channel::RevealX],
            PropKey::RevealY => &[Channel::RevealY],
            _ => &[],
        }
    }

    /// The channel's bit in `Resolved::anim_owned`.
    pub fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// The transform the draw walk applies to a node's rect, plus the two
/// alpha channels an anim can write. In `Runtime::motion` this is the ANIM
/// LAYER (what players wrote); in `Resolved::motion` it is the composed
/// value (the `when` ladder, then the anim layer on the channels an anim
/// owns).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub translate: [f32; 2],
    pub scale: [f32; 2],
    /// Degrees, clockwise, about the node's center.
    pub rotate: f32,
    /// Degrees the node swings about its LEFT edge in 3D (the `tap_pulse`
    /// turnstile, an anim's `swing` lane); 0 = flat on the screen.
    pub swing: f32,
    pub opacity: f32,
    /// The fill's alpha (0..1).
    pub color_a: f32,
    /// Shown fraction per axis (GP40 reveal), 1 = whole.
    pub reveal: [f32; 2],
}

impl Motion {
    pub const IDENTITY: Motion = Motion { translate: [0.0, 0.0], scale: [1.0, 1.0], rotate: 0.0, swing: 0.0, opacity: 1.0, color_a: 1.0, reveal: [1.0, 1.0] };

    pub fn get(&self, c: Channel) -> f32 {
        match c {
            Channel::TranslateX => self.translate[0],
            Channel::TranslateY => self.translate[1],
            Channel::ScaleX => self.scale[0],
            Channel::ScaleY => self.scale[1],
            Channel::Rotate => self.rotate,
            Channel::Swing => self.swing,
            Channel::Opacity => self.opacity,
            Channel::ColorA => self.color_a,
            Channel::RevealX => self.reveal[0],
            Channel::RevealY => self.reveal[1],
        }
    }

    pub fn set(&mut self, c: Channel, v: f32) {
        match c {
            Channel::TranslateX => self.translate[0] = v,
            Channel::TranslateY => self.translate[1] = v,
            Channel::ScaleX => self.scale[0] = v,
            Channel::ScaleY => self.scale[1] = v,
            Channel::Rotate => self.rotate = v,
            Channel::Swing => self.swing = v,
            Channel::Opacity => self.opacity = v,
            Channel::ColorA => self.color_a = v,
            Channel::RevealX => self.reveal[0] = v,
            Channel::RevealY => self.reveal[1] = v,
        }
    }
}

/// Splits an anim target word into a prop lane (GP40): `background` →
/// the key, `background.a` → the key + alpha-only. Motion words are
/// `Channel::parse`'s — callers try that first. The compiler checks
/// legality (animatable word, `.a` on colors only, the target kind's
/// words); here is just the spelling.
pub fn prop_lane(word: &str) -> Option<(PropKey, bool)> {
    let (w, alpha_only) = match word.strip_suffix(".a") {
        Some(w) => (w, true),
        None => (word, false),
    };
    PropKey::from_word(w).map(|k| (k, alpha_only))
}

/// Writes one prop-lane layer entry (find-or-push) — the prop analog of
/// `Motion::set`.
pub fn set_pval(vals: &mut Vec<(NodeId, PropKey, Value)>, node: NodeId, key: PropKey, v: Value) {
    match vals.iter_mut().find(|(n, k, _)| *n == node && *k == key) {
        Some(e) => e.2 = v,
        None => vals.push((node, key, v)),
    }
}

/// Reads one prop-lane layer entry.
pub fn get_pval(vals: &[(NodeId, PropKey, Value)], node: NodeId, key: PropKey) -> Option<Value> {
    vals.iter().find(|(n, k, _)| *n == node && *k == key).map(|(_, _, v)| *v)
}

/// One track of a playing anim, bound to a node and channel.
#[derive(Clone, Copy, Debug)]
struct Track {
    node: NodeId,
    channel: Channel,
    /// `None` = capture the live value when the track starts (`to=` alone).
    from: Option<f32>,
    to: f32,
    start: f32,
    duration: f32,
    easing: Easing,
    /// Plays out and back: the return leg mirrors the outbound one with
    /// the same easing, so the track lasts `2 × duration`.
    autoreverse: bool,
    /// The channel's value before this track first wrote it (`stop`
    /// restores it).
    base: f32,
    /// That value was a finished anim's held end pose (a standing value
    /// the author chose): `stop(restore)` re-holds it instead of releasing
    /// the channel to the ladder.
    base_held: bool,
    /// What the channel shows BEFORE this track starts — the pinned
    /// `from=` (else the play-start value), set for the first track on a
    /// channel only. Needed because a `repeat=forever` wrap replays the
    /// chain: without re-asserting this during the lead-in, the channel
    /// keeps the chain's END value until the track starts — the loop-seam
    /// stutter (GPAR55). A later track on the channel stays `None`: its
    /// lead-in belongs to the previous track.
    lead_in_from: Option<f32>,
    /// `%` endpoints still waiting for the node's box (the node was hidden
    /// at play — a sheet slides in from below its own height before it
    /// ever laid out): the raw percents (from, to), resolved by
    /// `Player::resolve_pending_percent` at the next layout.
    pending_percent: Option<(Option<f32>, Option<f32>)>,
}

impl Track {
    fn end(&self) -> f32 {
        self.start + if self.autoreverse { 2.0 * self.duration } else { self.duration }
    }
}

/// One PROP lane of a playing anim (GP40): `node.key` goes `from` → `to`
/// through `lerp_value`, so colors mix under the one law (`crate::color`).
/// An alpha-only lane (`background.a`) carries `VNum` alphas end to end —
/// the layer stores the number and `compose_anim` puts it on the ladder's
/// hue. The fields mirror `Track`'s, value-typed.
#[derive(Clone, Copy, Debug)]
struct PropTrack {
    node: NodeId,
    key: PropKey,
    from: Option<Value>,
    to: Value,
    start: f32,
    duration: f32,
    easing: Easing,
    autoreverse: bool,
    base: Value,
    base_held: bool,
    lead_in_from: Option<Value>,
}

impl PropTrack {
    fn end(&self) -> f32 {
        self.start + if self.autoreverse { 2.0 * self.duration } else { self.duration }
    }
}

/// What a finished player does (`on:hide=play(x)` hides on finish).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Nothing,
    /// A code-door deferred hide (`Runtime::show(.., false)`): the finish
    /// writes `visibility=gone` and restores every displaced channel.
    Hide(NodeId),
    /// A `when`/arrangement-driven deferred hide (GP19): the node keeps
    /// drawing while its `on:hide` anim plays (`Resolved::hiding`), the
    /// finish restores displaced channels — and writes NO visibility prop,
    /// because a code write would outrank the `when` forever (GPAR57); the
    /// `when`'s own `gone` takes over the moment the override lifts.
    HideWhen(NodeId),
}

#[derive(Clone, Debug)]
pub struct Player {
    pub anim: StrId,
    tracks: Vec<Track>,
    ptracks: Vec<PropTrack>,
    /// Anim seconds (already multiplied by `speed`).
    pub elapsed: f32,
    pub finish: Finish,
    /// Channels this player owns (a newer player takes them over — WPF SnapshotAndReplace).
    pub owns: Vec<(NodeId, Channel)>,
    /// Prop lanes this player owns — the same take-over law, per (node, key).
    pub owns_props: Vec<(NodeId, PropKey)>,
    /// The chain's length at play start. Kept apart from the tracks so a
    /// player whose channels a newer one took still finishes ON TIME (a
    /// deferred hide hides when the file said, not the moment it lost its
    /// last track).
    total: f32,
    /// `repeat=forever`: the clock wraps at the chain's end instead of finishing.
    pub repeat: bool,
    pub speed: f32,
    /// Set by `stop`: the next `tick` reports the player done without
    /// writing (a stop is not a finish — `finish` does not run).
    pub stopped: bool,
}

impl Player {
    /// Instantiates the anim's tracks for the page. A track's authored
    /// `from=` is written at play start and held until the track begins
    /// (a staggered entrance never flashes its base) — for the FIRST
    /// track on a channel only; a later track on the same channel starts
    /// at its own `from=`, and with none it goes on from the value the
    /// channel has when the track begins. `from=current` captures the
    /// live value now, at play start.
    ///
    /// `layer` is the anim layer this player writes; `live` is the value
    /// each channel SHOWS right now (the ladder, or an anim that owns it)
    /// and `held` the finished anims' end poses still standing — the base a
    /// later `stop(restore)` returns to.
    /// `pvals`/`plive`/`pheld` are the prop-lane analogs of
    /// `layer`/`live`/`held` (GP40); `variant` derefs a `to=` written as a
    /// mode-flipping palette word at PLAY start (a variant flip mid-play
    /// keeps the play's colors — the next play reads the new mode).
    /// `boxes` are the nodes' last laid-out rects (the runtime's `glide`
    /// memory): a `%` endpoint on a translate lane is that fraction of
    /// the node's width (x) or height (y); with no layout yet the box is
    /// empty and the endpoint is 0.
    #[allow(clippy::too_many_arguments)]
    pub fn new(page: &Page, anim: StrId, layer: &mut [Motion], live: &[Motion], held: &[(NodeId, Channel, f32)], pvals: &mut Vec<(NodeId, PropKey, Value)>, plive: &[(NodeId, PropKey, Value)], pheld: &[(NodeId, PropKey, Value)], variant: StrId, finish: Finish, boxes: &[Rect]) -> Player {
        let mut tracks: Vec<Track> = Vec::new();
        let mut ptracks: Vec<PropTrack> = Vec::new();
        let mut owns = Vec::new();
        let mut owns_props: Vec<(NodeId, PropKey)> = Vec::new();
        let head = page.anim_heads.iter().find(|h| h.anim == anim);
        for t in page.anims.iter().filter(|t| t.anim == anim) {
            let easing = if t.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(t.easing)).unwrap_or(Easing::LINEAR) };
            let word = page.str(t.channel);
            let Some(channels) = Channel::parse(word) else {
                let Some((key, _)) = prop_lane(word) else { continue };
                let dv = |v: Value| crate::runtime::deref(page, variant, v);
                let shown = get_pval(plive, t.node, key).unwrap_or(if key.is_color() && !word.ends_with(".a") { Value::VColor(Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT) } else { Value::VNum(0.0) });
                let base_held = pheld.iter().any(|h| h.0 == t.node && h.1 == key);
                let first = !ptracks.iter().any(|x| x.node == t.node && x.key == key);
                let t_from = t.from.map(dv);
                let t_to = dv(t.to);
                let from = if t.current {
                    Some(shown)
                } else if first {
                    // The lane is this player's from now on: the layer
                    // starts at the pinned `from=`, else at what shows,
                    // so taking ownership never jumps the picture.
                    set_pval(pvals, t.node, key, t_from.unwrap_or(shown));
                    t_from
                } else {
                    // A later step on the lane: a written `from=` starts
                    // the step there; with none it goes on from where the
                    // lane is when the step starts (`apply_at`).
                    t_from
                };
                let lead_in_from = if first { Some(if t.current { shown } else { t_from.unwrap_or(shown) }) } else { None };
                ptracks.push(PropTrack { node: t.node, key, from, to: t_to, start: t.start, duration: t.duration, easing, autoreverse: t.autoreverse, base: shown, base_held, lead_in_from });
                if !owns_props.contains(&(t.node, key)) {
                    owns_props.push((t.node, key));
                }
                continue;
            };
            for channel in channels {
                let axis: f32 = match channel {
                    Channel::TranslateX => boxes.get(t.node.index()).map(|r| r.w).unwrap_or(0.0),
                    Channel::TranslateY => boxes.get(t.node.index()).map(|r| r.h).unwrap_or(0.0),
                    _ => 100.0,
                };
                let raw_from: Option<f32> = t.from.and_then(Value::as_num);
                let raw_to: f32 = t.to.as_num().unwrap_or(0.0);
                let track_from = raw_from.map(|v| if t.from_percent { v / 100.0 * axis } else { v });
                let track_to = if t.to_percent { raw_to / 100.0 * axis } else { raw_to };
                // No box yet (a hidden node): the percents wait for the
                // next layout instead of reading as 0.
                let pending_percent: Option<(Option<f32>, Option<f32>)> = if (t.from_percent || t.to_percent) && axis <= 0.0 {
                    Some((if t.from_percent { raw_from } else { None }, if t.to_percent { Some(raw_to) } else { None }))
                } else {
                    None
                };
                let shown = live.get(t.node.index()).map(|m| m.get(channel)).unwrap_or(Motion::IDENTITY.get(channel));
                let base_held = held.iter().any(|h| h.0 == t.node && h.1 == channel);
                let first_on_channel = !tracks.iter().any(|x| x.node == t.node && x.channel == channel);
                let from = if t.current {
                    Some(shown)
                } else if first_on_channel {
                    // The channel is this player's from now on: the layer
                    // starts at the pinned `from=`, else at what shows, so
                    // taking ownership never jumps the picture.
                    layer[t.node].set(channel, track_from.unwrap_or(shown));
                    track_from
                } else {
                    // A later step on the channel: a written `from=` starts
                    // the step there; with none it goes on from where the
                    // channel is when the step starts (`apply_at`).
                    track_from
                };
                let lead_in_from = if first_on_channel { Some(if t.current { shown } else { track_from.unwrap_or(shown) }) } else { None };
                tracks.push(Track { node: t.node, channel, from, to: track_to, start: t.start, duration: t.duration, easing, autoreverse: t.autoreverse, base: shown, base_held, lead_in_from, pending_percent });
                if !owns.contains(&(t.node, channel)) {
                    owns.push((t.node, channel));
                }
            }
        }
        let total = tracks.iter().map(Track::end).chain(ptracks.iter().map(PropTrack::end)).fold(0.0, f32::max);
        Player { anim, tracks, ptracks, elapsed: 0.0, finish, owns, owns_props, total, repeat: head.map(|h| h.repeat).unwrap_or(false), speed: head.map(|h| h.speed).unwrap_or(1.0).max(0.001), stopped: false }
    }

    pub fn length(&self) -> f32 {
        self.total
    }

    /// Resolves the `%` endpoints that had no box at play time against
    /// the nodes' fresh rects (the runtime's `glide`): the track's from
    /// and to become the fractions of the box, the layer jumps to the
    /// from pose so the node's first laid-out frame already shows it
    /// displaced (a sheet's first frame is below the screen, not at its
    /// seat). Returns whether anything resolved.
    pub fn resolve_pending_percent(&mut self, boxes: &[Rect], layer: &mut [Motion]) -> bool {
        let mut resolved: bool = false;
        for track in &mut self.tracks {
            let Some((pct_from, pct_to)) = track.pending_percent else { continue };
            let axis: f32 = match track.channel {
                Channel::TranslateX => boxes.get(track.node.index()).map(|r| r.w).unwrap_or(0.0),
                Channel::TranslateY => boxes.get(track.node.index()).map(|r| r.h).unwrap_or(0.0),
                _ => 100.0,
            };
            if axis <= 0.0 {
                continue;
            }
            if let Some(p) = pct_from {
                let from: f32 = p / 100.0 * axis;
                track.from = Some(from);
                if track.lead_in_from.is_some() {
                    track.lead_in_from = Some(from);
                }
                layer[track.node].set(track.channel, from);
            }
            if let Some(p) = pct_to {
                track.to = p / 100.0 * axis;
            }
            track.pending_percent = None;
            resolved = true;
        }
        resolved
    }

    /// A newer player takes `(node, channel)`: this player's tracks on it
    /// go (its clock and finish stay), and the value the channel had
    /// before ANY player wrote it comes back for the taker to inherit, so
    /// a later `stop(restore)` lands there and not mid-flight.
    pub fn hand_over(&mut self, node: NodeId, channel: Channel) -> Option<(f32, bool)> {
        let base = self.tracks.iter().find(|t| t.node == node && t.channel == channel).map(|t| (t.base, t.base_held));
        self.tracks.retain(|t| !(t.node == node && t.channel == channel));
        self.owns.retain(|o| *o != (node, channel));
        base
    }

    pub fn inherit_base(&mut self, node: NodeId, channel: Channel, base: (f32, bool)) {
        for t in self.tracks.iter_mut().filter(|t| t.node == node && t.channel == channel) {
            t.base = base.0;
            t.base_held = base.1;
        }
    }

    /// `hand_over`, for a prop lane (GP40): the taker inherits the true base.
    pub fn hand_over_prop(&mut self, node: NodeId, key: PropKey) -> Option<(Value, bool)> {
        let base = self.ptracks.iter().find(|t| t.node == node && t.key == key).map(|t| (t.base, t.base_held));
        self.ptracks.retain(|t| !(t.node == node && t.key == key));
        self.owns_props.retain(|o| *o != (node, key));
        base
    }

    pub fn inherit_base_prop(&mut self, node: NodeId, key: PropKey, base: (Value, bool)) {
        for t in self.ptracks.iter_mut().filter(|t| t.node == node && t.key == key) {
            t.base = base.0;
            t.base_held = base.1;
        }
    }

    /// `owned_bases`, for the prop lanes.
    pub fn owned_bases_props(&self) -> Vec<(NodeId, PropKey, Value, bool)> {
        self.owns_props.iter().filter_map(|(n, k)| self.ptracks.iter().find(|t| t.node == *n && t.key == *k).map(|t| (*n, *k, t.base, t.base_held))).collect()
    }

    /// Per owned channel: the value it had before this play and whether
    /// that value was a held end pose (see `Track::base_held`).
    pub fn owned_bases(&self) -> Vec<(NodeId, Channel, f32, bool)> {
        self.owns.iter().filter_map(|(n, c)| self.tracks.iter().find(|t| t.node == *n && t.channel == *c).map(|t| (*n, *c, t.base, t.base_held))).collect()
    }

    /// Writes every channel for anim time `t` (idempotent, so a repeat
    /// wrap replays the chain from its start).
    fn apply_at(&mut self, t: f32, motion: &mut [Motion], pvals: &mut Vec<(NodeId, PropKey, Value)>) {
        for track in &mut self.ptracks {
            if t < track.start {
                if let Some(f) = track.lead_in_from {
                    set_pval(pvals, track.node, track.key, f);
                }
                continue;
            }
            if track.from.is_none() {
                track.from = get_pval(pvals, track.node, track.key).or(Some(track.to));
            }
            let from = track.from.unwrap_or(track.to);
            let local = t - track.start;
            let value = if track.duration <= 0.0 {
                track.to
            } else if track.autoreverse && local > track.duration {
                let back = ((local - track.duration) / track.duration).min(1.0);
                lerp_value(track.to, from, track.easing.apply(back))
            } else {
                lerp_value(from, track.to, track.easing.apply((local / track.duration).min(1.0)))
            };
            set_pval(pvals, track.node, track.key, value);
        }
        for track in &mut self.tracks {
            if t < track.start {
                if let Some(f) = track.lead_in_from {
                    motion[track.node].set(track.channel, f);
                }
                continue;
            }
            if track.from.is_none() {
                track.from = Some(motion[track.node].get(track.channel));
            }
            let from = track.from.unwrap_or(track.to);
            let local = t - track.start;
            let value = if track.duration <= 0.0 {
                track.to
            } else if track.autoreverse && local > track.duration {
                let back = ((local - track.duration) / track.duration).min(1.0);
                lerp(track.to, from, track.easing.apply(back))
            } else {
                lerp(from, track.to, track.easing.apply((local / track.duration).min(1.0)))
            };
            motion[track.node].set(track.channel, value);
        }
    }

    /// Advances and writes the channels; true while still playing.
    pub fn tick(&mut self, dt: f32, motion: &mut [Motion], pvals: &mut Vec<(NodeId, PropKey, Value)>) -> bool {
        if self.stopped {
            return false;
        }
        self.elapsed += dt * self.speed;
        let total = self.length();
        if self.repeat && total > 0.0 {
            let t = self.elapsed % total;
            self.apply_at(t, motion, pvals);
            return true;
        }
        let t = self.elapsed.min(total);
        self.apply_at(t, motion, pvals);
        self.elapsed < total
    }

    /// WPF Storyboard.Stop: every channel back to the value it had before
    /// the play (`restore`), or left where it is. Owned channels only —
    /// a channel a newer player took over is that player's to restore.
    /// The layer value only matters while something owns the channel; the
    /// runtime decides what stays owned (`Runtime::stop`).
    pub fn stop(&mut self, restore: bool, layer: &mut [Motion], pvals: &mut Vec<(NodeId, PropKey, Value)>) {
        self.stopped = true;
        if restore {
            for track in &self.tracks {
                if self.owns.contains(&(track.node, track.channel)) {
                    layer[track.node].set(track.channel, track.base);
                }
            }
            for track in &self.ptracks {
                if self.owns_props.contains(&(track.node, track.key)) {
                    set_pval(pvals, track.node, track.key, track.base);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints_and_shapes() {
        for name in ["linear", "sine_in", "quad_out", "cubic_in_out", "quart", "quint_in", "circle_out", "expo_in", "power", "back_out", "elastic_out", "bounce_out", "bounce_in"] {
            let e = Easing::parse(name).unwrap();
            assert!((e.apply(0.0)).abs() < 1e-3, "{name} at 0 = {}", e.apply(0.0));
            assert!((e.apply(1.0) - 1.0).abs() < 1e-3, "{name} at 1 = {}", e.apply(1.0));
        }
        // Bare name = ease-out; ease-out of cubic is above the line.
        let c = Easing::parse("cubic").unwrap();
        assert_eq!(c.mode, Mode::Out);
        assert!(c.apply(0.5) > 0.5);
        assert!(Easing::parse("cubic_in").unwrap().apply(0.5) < 0.5);
        // back_out overshoots past 1 on its way.
        let b = Easing::parse("back_out").unwrap();
        assert!((0..20).any(|i| b.apply(i as f32 / 20.0) > 1.0));
        assert!(Easing::parse("nope").is_none());
    }

    /// WPF BounceEase(Bounces=3, Bounciness=2) ease-in, the numbers the
    /// Godot gogo port produces (gogo_easing.gd): the curve must never be
    /// NaN (a NaN offset made the crop panel vanish).
    #[test]
    fn bounce_matches_the_reference_numbers() {
        let e = Easing::parse("bounce_in").unwrap();
        for (t, want) in [(0.1, 0.0475), (0.25, 0.1094), (0.5, 0.4688)] {
            let got = e.apply(t);
            assert!(got.is_finite() && (got - want).abs() < 2e-3, "bounce_in({t}) = {got}, want {want}");
        }
        // Parameters change the curve (crop_page: bounces=1, bounciness=2.4).
        let p = Easing::parse("bounce_out(1,2.4)").unwrap();
        assert_eq!(p.params, [Some(1.0), Some(2.4)]);
        assert_eq!(p.to_word(), "bounce_out(1,2.4)");
        assert!((p.apply(0.5) - e.apply(0.5)).abs() > 1e-3);
        assert!((0..=20).all(|i| p.apply(i as f32 / 20.0).is_finite()));
        assert!((Easing::parse("power_in(3)").unwrap().apply(0.5) - 0.125).abs() < 1e-5);
    }

    #[test]
    fn tween_lerps_numbers_and_colors() {
        let mut t = Tween { node: NodeId::new(0), key: PropKey::Opacity, from: Value::VNum(0.0), to: Value::VNum(1.0), elapsed: 0.0, duration: 1.0, easing: Easing::LINEAR };
        t.elapsed = 0.25;
        assert_eq!(t.value(), Value::VNum(0.25));
        // Colors mix under the one law (premultiplied Oklab, crate::color)
        // — the midpoint is the perceptual middle, not the byte middle.
        let c = lerp_value(Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, 255])), Value::VColor(Color_Straight_Srgb_4xU8_RGBA([200, 100, 0, 255])), 0.5);
        assert_eq!(c, Value::VColor(Color_Straight_Srgb_4xU8_RGBA([76, 34, 0, 255])));
    }
}
