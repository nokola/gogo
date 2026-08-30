//! Motion (G8): WPF-exact easing math, property tweens for `transition`
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Easing {
    pub curve: Curve,
    pub mode: Mode,
}

impl Easing {
    pub const LINEAR: Easing = Easing { curve: Curve::Linear, mode: Mode::Out };

    /// `cubic_out`, `back_in_out`, bare `cubic` = ease-out (spec).
    pub fn parse(name: &str) -> Option<Easing> {
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
            "back" => Curve::Back,
            "bounce" => Curve::Bounce,
            "elastic" => Curve::Elastic,
            _ => return None,
        };
        Some(Easing { curve, mode })
    }

    /// Eased progress for `t` in 0..1 (WPF EasingFunctionBase math).
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self.mode {
            Mode::In => ease_in(self.curve, t),
            Mode::Out => 1.0 - ease_in(self.curve, 1.0 - t),
            Mode::InOut => {
                if t < 0.5 {
                    ease_in(self.curve, t * 2.0) * 0.5
                } else {
                    (1.0 - ease_in(self.curve, (1.0 - t) * 2.0)) * 0.5 + 0.5
                }
            }
        }
    }
}

/// The ease-in shapes; WPF defaults: Back amplitude 1, Bounce 3 bounces
/// bounciness 2, Elastic 3 oscillations springiness 3, Expo exponent 2,
/// Power 2 (= Quad).
fn ease_in(curve: Curve, t: f32) -> f32 {
    match curve {
        Curve::Linear => t,
        Curve::Sine => 1.0 - ((t * std::f32::consts::FRAC_PI_2).cos()),
        Curve::Quad => t * t,
        Curve::Cubic => t * t * t,
        Curve::Quart => t * t * t * t,
        Curve::Quint => t * t * t * t * t,
        Curve::Circle => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
        Curve::Expo => {
            let e = 2.0f32;
            ((e * t).exp() - 1.0) / (e.exp() - 1.0)
        }
        Curve::Back => {
            let a = 1.0f32;
            t * t * t - t * a * (t * std::f32::consts::PI).sin()
        }
        Curve::Bounce => {
            // WPF BounceEase.EaseInCore with Bounces=3, Bounciness=2.
            let bounces = 3.0f32;
            let bounciness = 2.0f32;
            let pow = bounciness.powf(bounces);
            let one_minus = 1.0 - bounciness;
            let sum_units = (1.0 - pow) / one_minus + pow * 0.5;
            let unit_at_t = t * sum_units;
            let bounce_at_t = ((unit_at_t * one_minus + 1.0).ln() / bounciness.ln()).floor();
            let start = (1.0 - bounciness.powf(bounce_at_t)) / one_minus;
            let end = start + bounciness.powf(bounce_at_t);
            let mid = (start + end) * 0.5;
            let radius = mid - start;
            let amplitude = bounciness.powf(bounce_at_t) / pow;
            let x = unit_at_t - mid;
            (-amplitude / (radius * radius) * (x - radius) * (x + radius)).max(0.0).min(1.0) * 0.0 + {
                // WPF returns 1 - (amplitude/radius² · (x−r)(x+r)) shape inverted for ease-in.
                let y = -amplitude / (radius * radius) * (x - radius) * (x + radius);
                1.0 - y.max(0.0)
            } - if bounce_at_t == 0.0 && t == 0.0 { 1.0 } else { 0.0 }
        }
        Curve::Elastic => {
            let oscillations = 3.0f32;
            let springiness = 3.0f32;
            let expo = ((springiness * t).exp() - 1.0) / (springiness.exp() - 1.0);
            expo * ((std::f32::consts::TAU * oscillations + std::f32::consts::FRAC_PI_2) * t).sin()
        }
    }
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
    /// Degrees; drawn as-is only by renderers that rotate (G5 quads do not).
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
    started: bool,
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
    pub elapsed: f32,
    pub finish: Finish,
    /// Channels this player owns (a newer player takes them over — WPF SnapshotAndReplace).
    pub owns: Vec<(NodeId, Channel)>,
}

impl Player {
    /// Instantiates the anim's tracks for the page; `from` values that pin
    /// (`from=`) are applied to the motion at play start so a staggered
    /// entrance never flashes its base.
    pub fn new(page: &Page, anim: StrId, motion: &mut [Motion], finish: Finish) -> Player {
        let mut tracks = Vec::new();
        let mut owns = Vec::new();
        for t in page.anims.iter().filter(|t| t.anim == anim) {
            let easing = if t.easing == NO_STR { Easing::LINEAR } else { Easing::parse(page.str(t.easing)).unwrap_or(Easing::LINEAR) };
            let Some(channels) = Channel::parse(page.str(t.channel)) else { continue };
            for channel in channels {
                if let Some(from) = t.from {
                    motion[t.node as usize].set(channel, from);
                }
                tracks.push(Track { node: t.node, channel, from: t.from, to: t.to, start: t.start, duration: t.duration, easing, started: false });
                owns.push((t.node, channel));
            }
        }
        Player { anim, tracks, elapsed: 0.0, finish, owns }
    }

    pub fn length(&self) -> f32 {
        self.tracks.iter().map(|t| t.start + t.duration).fold(0.0, f32::max)
    }

    /// Advances and writes the channels; true while still playing.
    pub fn tick(&mut self, dt: f32, motion: &mut [Motion]) -> bool {
        self.elapsed += dt;
        for t in &mut self.tracks {
            if self.elapsed < t.start {
                continue;
            }
            if !t.started {
                t.started = true;
                if t.from.is_none() {
                    t.from = Some(motion[t.node as usize].get(t.channel));
                }
            }
            let from = t.from.unwrap_or(t.to);
            let p = if t.duration <= 0.0 { 1.0 } else { ((self.elapsed - t.start) / t.duration).min(1.0) };
            motion[t.node as usize].set(t.channel, lerp(from, t.to, t.easing.apply(p)));
        }
        self.elapsed < self.length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints_and_shapes() {
        for name in ["linear", "sine_in", "quad_out", "cubic_in_out", "quart", "quint_in", "circle_out", "expo_in", "back_out", "elastic_out"] {
            let e = Easing::parse(name).unwrap();
            assert!((e.apply(0.0)).abs() < 1e-4, "{name} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-4, "{name} at 1");
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

    #[test]
    fn tween_lerps_numbers_and_colors() {
        let mut t = Tween { node: 0, key: PropKey::Opacity, from: Value::VNum(0.0), to: Value::VNum(1.0), elapsed: 0.0, duration: 1.0, easing: Easing::LINEAR };
        t.elapsed = 0.25;
        assert_eq!(t.value(), Value::VNum(0.25));
        let c = lerp_value(Value::VColor(Rgba([0, 0, 0, 255])), Value::VColor(Rgba([200, 100, 0, 255])), 0.5);
        assert_eq!(c, Value::VColor(Rgba([100, 50, 0, 255])));
    }
}
