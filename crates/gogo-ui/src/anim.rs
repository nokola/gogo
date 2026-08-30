//! Motion: WPF-exact easing math, property tweens for `transition`
//! rules, and players for `anim` tracks on the motion channels
//! (`translate.x/.y`, `scale`, `rotate`, `opacity`). Layout is declared,
//! motion is transformed: a player never touches a rect in the arena; the
//! draw walk applies the channels.

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
        (Value::VColor(x), Value::VColor(y)) => {
            let mut c = [0u8; 4];
            for i in 0..4 {
                c[i] = lerp(x.0[i] as f32, y.0[i] as f32, t).round().clamp(0.0, 255.0) as u8;
            }
            Value::VColor(Rgba(c))
        }
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
    Opacity,
}

impl Channel {
    /// `translate.x`, `scale`, `rotate`, `opacity`… → channels (uniform
    /// `scale` = both axes).
    pub fn parse(word: &str) -> Option<Vec<Channel>> {
        Some(match word {
            "translate.x" => vec![Channel::TranslateX],
            "translate.y" => vec![Channel::TranslateY],
            "scale" => vec![Channel::ScaleX, Channel::ScaleY],
            "scale.x" => vec![Channel::ScaleX],
            "scale.y" => vec![Channel::ScaleY],
            "rotate" => vec![Channel::Rotate],
            // A fill's alpha or the engine's modulate.a both fade the node.
            "opacity" | "modulate.a" | "color.a" => vec![Channel::Opacity],
            _ => return None,
        })
    }
}

/// The transform the draw walk applies to a node's rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub translate: [f32; 2],
    pub scale: [f32; 2],
    /// Degrees, clockwise, about the node's center.
    pub rotate: f32,
    pub opacity: f32,
}

impl Motion {
    pub const IDENTITY: Motion = Motion { translate: [0.0, 0.0], scale: [1.0, 1.0], rotate: 0.0, opacity: 1.0 };

    pub fn get(&self, c: Channel) -> f32 {
        match c {
            Channel::TranslateX => self.translate[0],
            Channel::TranslateY => self.translate[1],
            Channel::ScaleX => self.scale[0],
            Channel::ScaleY => self.scale[1],
            Channel::Rotate => self.rotate,
            Channel::Opacity => self.opacity,
        }
    }

    pub fn set(&mut self, c: Channel, v: f32) {
        match c {
            Channel::TranslateX => self.translate[0] = v,
            Channel::TranslateY => self.translate[1] = v,
            Channel::ScaleX => self.scale[0] = v,
            Channel::ScaleY => self.scale[1] = v,
            Channel::Rotate => self.rotate = v,
            Channel::Opacity => self.opacity = v,
        }
    }
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
}

impl Track {
    fn end(&self) -> f32 {
        self.start + if self.autoreverse { 2.0 * self.duration } else { self.duration }
    }
}

/// What a finished player does (`on:hide=play(x)` hides on finish).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Nothing,
    Hide(NodeId),
}

#[derive(Clone, Debug)]
pub struct Player {
    pub anim: StrId,
    tracks: Vec<Track>,
    /// Anim seconds (already multiplied by `speed`).
    pub elapsed: f32,
    pub finish: Finish,
    /// Channels this player owns (a newer player takes them over — WPF SnapshotAndReplace).
    pub owns: Vec<(NodeId, Channel)>,
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
    /// track on a channel only; a later track on the same channel chains
    /// from the value the previous one leaves. `from=current` captures
    /// the live value now, at play start.
    pub fn new(page: &Page, anim: StrId, motion: &mut [Motion], finish: Finish) -> Player {
        let mut tracks: Vec<Track> = Vec::new();
        let mut owns = Vec::new();
        let head = page.anim_heads.iter().find(|h| h.anim == anim);
        for t in page.anims.iter().filter(|t| t.anim == anim) {
            let easing = if t.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(t.easing)).unwrap_or(Easing::LINEAR) };
            let Some(channels) = Channel::parse(page.str(t.channel)) else { continue };
            for channel in channels {
                let live = motion[t.node as usize].get(channel);
                let first_on_channel = !tracks.iter().any(|x| x.node == t.node && x.channel == channel);
                let from = if t.current {
                    Some(live)
                } else if first_on_channel {
                    if let Some(f) = t.from {
                        motion[t.node as usize].set(channel, f);
                    }
                    t.from
                } else {
                    None
                };
                tracks.push(Track { node: t.node, channel, from, to: t.to, start: t.start, duration: t.duration, easing, autoreverse: t.autoreverse, base: live });
                if !owns.contains(&(t.node, channel)) {
                    owns.push((t.node, channel));
                }
            }
        }
        let total = tracks.iter().map(Track::end).fold(0.0, f32::max);
        Player { anim, tracks, elapsed: 0.0, finish, owns, total, repeat: head.map(|h| h.repeat).unwrap_or(false), speed: head.map(|h| h.speed).unwrap_or(1.0).max(0.001), stopped: false }
    }

    pub fn length(&self) -> f32 {
        self.total
    }

    /// A newer player takes `(node, channel)`: this player's tracks on it
    /// go (its clock and finish stay), and the value the channel had
    /// before ANY player wrote it comes back for the taker to inherit, so
    /// a later `stop(restore)` lands there and not mid-flight.
    pub fn hand_over(&mut self, node: NodeId, channel: Channel) -> Option<f32> {
        let base = self.tracks.iter().find(|t| t.node == node && t.channel == channel).map(|t| t.base);
        self.tracks.retain(|t| !(t.node == node && t.channel == channel));
        self.owns.retain(|o| *o != (node, channel));
        base
    }

    pub fn inherit_base(&mut self, node: NodeId, channel: Channel, base: f32) {
        for t in self.tracks.iter_mut().filter(|t| t.node == node && t.channel == channel) {
            t.base = base;
        }
    }

    /// Writes every channel for anim time `t` (idempotent, so a repeat
    /// wrap replays the chain from its start).
    fn apply_at(&mut self, t: f32, motion: &mut [Motion]) {
        for track in &mut self.tracks {
            if t < track.start {
                continue;
            }
            if track.from.is_none() {
                track.from = Some(motion[track.node as usize].get(track.channel));
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
            motion[track.node as usize].set(track.channel, value);
        }
    }

    /// Advances and writes the channels; true while still playing.
    pub fn tick(&mut self, dt: f32, motion: &mut [Motion]) -> bool {
        if self.stopped {
            return false;
        }
        self.elapsed += dt * self.speed;
        let total = self.length();
        if self.repeat && total > 0.0 {
            let t = self.elapsed % total;
            self.apply_at(t, motion);
            return true;
        }
        let t = self.elapsed.min(total);
        self.apply_at(t, motion);
        self.elapsed < total
    }

    /// WPF Storyboard.Stop: every channel back to the value it had before
    /// the play (`restore`), or left where it is. Owned channels only —
    /// a channel a newer player took over is that player's to restore.
    pub fn stop(&mut self, restore: bool, motion: &mut [Motion]) {
        self.stopped = true;
        if restore {
            for track in &self.tracks {
                if self.owns.contains(&(track.node, track.channel)) {
                    motion[track.node as usize].set(track.channel, track.base);
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
        let mut t = Tween { node: 0, key: PropKey::Opacity, from: Value::VNum(0.0), to: Value::VNum(1.0), elapsed: 0.0, duration: 1.0, easing: Easing::LINEAR };
        t.elapsed = 0.25;
        assert_eq!(t.value(), Value::VNum(0.25));
        let c = lerp_value(Value::VColor(Rgba([0, 0, 0, 255])), Value::VColor(Rgba([200, 100, 0, 255])), 0.5);
        assert_eq!(c, Value::VColor(Rgba([100, 50, 0, 255])));
    }
}
