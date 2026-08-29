//! The runtime part of a page: which states hold, the active arrangement
//! and theme variant, scroll positions. `resolve` overlays the active
//! `when` responses onto the base props in the spec's priority order; the
//! layout and draw passes read only the `Resolved` result.

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
}

impl Runtime {
    pub fn new(page: &Page) -> Runtime {
        Runtime { arrangement: BASE, variant: NO_STR, screen: Vec::new(), states: vec![0; page.nodes.len()], scroll: vec![[0.0; 2]; page.nodes.len()], dirty: true }
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
        }
    }

    pub fn has(&self, node: NodeId, bit: u8) -> bool {
        self.states[node as usize] & bit != 0
    }

    pub fn go(&mut self, arrangement: u32) {
        if self.arrangement != arrangement {
            self.arrangement = arrangement;
            self.dirty = true;
        }
    }

    pub fn toggle(&mut self, arrangement: u32) {
        let target = if self.arrangement == arrangement { BASE } else { arrangement };
        self.go(target);
    }

    fn state_active(&self, node: NodeId, state: State) -> bool {
        match state {
            State::Held => self.has(node, HELD),
            State::Hovered => self.has(node, HOVERED),
            State::Focused => self.has(node, FOCUSED),
            State::FocusWithin => self.has(node, FOCUS_WITHIN),
            State::Selected => self.has(node, SELECTED),
            State::StateDisabled => self.has(node, DISABLED),
            State::Arrangement(i) => self.arrangement == i,
            State::Screen(class) => self.screen.contains(&class),
        }
    }
}

/// The current values: base placement with active poses applied, and the
/// prop overrides the active states contribute.
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    pub placement: Vec<Placement>,
    /// (node, prop) — later entries win.
    pub overrides: Vec<(NodeId, Prop)>,
    pub variant: StrId,
    /// Per node: the Scroll offset (copied from the Runtime).
    pub scroll: Vec<[f32; 2]>,
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
        match self.prop(page, node, key)? {
            Value::VColor(c) => Some(c),
            Value::VStop { color, .. } => Some(color),
            _ => None,
        }
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

    /// `visibility=gone` — out of layout.
    pub fn gone(&self, page: &Page, node: NodeId) -> bool {
        self.ident(page, node, PropKey::Visibility) == Some("gone")
    }

    /// `visibility=hidden` — laid out, not drawn, no input.
    pub fn hidden(&self, page: &Page, node: NodeId) -> bool {
        self.ident(page, node, PropKey::Visibility) == Some("hidden")
    }
}

/// Follows `$refs` through the theme (variant first, then defaults) and
/// applies a ref's `/opacity` to the color it lands on.
pub fn deref(page: &Page, variant: StrId, v: Value) -> Value {
    let mut v = v;
    let mut opacity: Option<f32> = None;
    for _ in 0..8 {
        match v {
            Value::VRef { name, opacity: o } => {
                if o.is_some() {
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

pub fn resolve(page: &Page, rt: &Runtime) -> Resolved {
    let mut r = Resolved { placement: Vec::with_capacity(page.nodes.len()), overrides: Vec::new(), variant: rt.variant, scroll: rt.scroll.clone() };
    for (i, node) in page.nodes.iter().enumerate() {
        let id = i as NodeId;
        let mut placement = node.placement;
        // Active whens, lowest priority first (stable: file order within a level).
        let mut active: Vec<&When> = page.whens[node.whens.0 as usize..(node.whens.0 + node.whens.1) as usize].iter().filter(|w| rt.state_active(id, w.state)).collect();
        active.sort_by_key(|w| w.state.priority());
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
    r
}

/// A posed size key behaves as if the file had declared it (D249).
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
