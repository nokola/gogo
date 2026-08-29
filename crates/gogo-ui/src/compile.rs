//! AST → `Page`. Styles, `uses`, widgets and the palette are resolved
//! here (the runtime never sees them); `$theme` refs and translation keys
//! stay live. Diagnostics keep the Godot G-codes where one existed.

use std::collections::HashMap;

use crate::anim::{Channel, Easing};
use crate::ast::{AstItem, AstNode, AstProp, AstValue, Document};
use crate::diag::Diagnostic;
use crate::page::*;
use crate::parser::parse;

/// Reads a `uses "path"` file relative to the current one; `None` = not found.
pub type Loader<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The gogo events; anything else on `on:` is a raw signal — gone (§7).
const EVENTS: &[&str] = &["activate", "change", "edit", "show", "hide"];

#[derive(Clone, Default)]
struct StyleDef {
    props: Vec<AstProp>,
    whens: Vec<AstNode>,
    transitions: Vec<AstNode>,
}

#[derive(Clone)]
struct WidgetDef {
    params: Vec<AstProp>,
    body: AstNode,
    line: u32,
}

struct WhenTmp {
    state: State,
    props: Vec<Prop>,
    /// Descendant responses (`when held face.background=…`): (node name, prop).
    descendants: Vec<(String, Prop)>,
}

/// A node before it has an arena slot.
struct TmpNode {
    node: Node,
    props: Vec<Prop>,
    handlers: Vec<OnHandler>,
    whens: Vec<WhenTmp>,
    transitions: Vec<Transition>,
    children: Vec<TmpNode>,
}

/// A node-level transition's node before flattening assigns the real id.
const PENDING_NODE: NodeId = u32::MAX - 1;

pub struct Compiler<'a> {
    page: Page,
    diags: Vec<Diagnostic>,
    loader: Loader<'a>,
    styles: HashMap<String, StyleDef>,
    widgets: HashMap<String, WidgetDef>,
    palette: HashMap<String, Rgba>,
    /// Widget param scopes, innermost last.
    scopes: Vec<HashMap<String, AstValue>>,
    node_whens: Vec<Vec<WhenTmp>>,
    /// Translation key → English literal (G8001).
    terms: HashMap<String, String>,
    /// Top-level `when <arrangement> { node.prop = v }`, applied after the tree exists.
    deferred_whens: Vec<(State, Vec<AstItem>, u32)>,
    /// `anim` declarations, compiled after the tree exists (they name nodes).
    anim_decls: Vec<AstNode>,
    root_seen: bool,
    names: std::collections::HashSet<String>,
}

pub struct Compiled {
    pub page: Page,
    pub diags: Vec<Diagnostic>,
}

pub fn compile(source: &str, loader: Loader<'_>) -> Compiled {
    let mut diags = Vec::new();
    let doc = parse(source, &mut diags);
    let mut c = Compiler {
        page: Page::default(),
        diags,
        loader,
        styles: HashMap::new(),
        widgets: HashMap::new(),
        palette: default_palette(),
        scopes: Vec::new(),
        node_whens: Vec::new(),
        terms: HashMap::new(),
        deferred_whens: Vec::new(),
        anim_decls: Vec::new(),
        root_seen: false,
        names: std::collections::HashSet::new(),
    };
    c.page.intern("");
    c.document(&doc, true);
    c.finish();
    Compiled { page: c.page, diags: c.diags }
}

fn default_palette() -> HashMap<String, Rgba> {
    let mut p = HashMap::new();
    let mut add = |name: &str, hex: u32| {
        p.insert(name.to_string(), Rgba([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xFF]));
    };
    add("black", 0x000000);
    add("white", 0xFFFFFF);
    add("red", 0xE51400);
    add("orange", 0xFA6800);
    add("yellow", 0xE3C800);
    add("green", 0x60A917);
    add("teal", 0x00ABA9);
    add("blue", 0x1BA1E2);
    add("indigo", 0x6A00FF);
    add("purple", 0xAA00FF);
    add("pink", 0xF472D0);
    add("brown", 0x825A2C);
    add("smoke", 0xEEEEEE);
    add("silver", 0xBBBBBB);
    add("gray", 0x7F7F7F);
    add("charcoal", 0x333333);
    add("surface", 0x1F1F1F);
    add("panel", 0x2B2B2B);
    add("border", 0x3C3C3C);
    add("text", 0xFFFFFF);
    p.insert("transparent".to_string(), Rgba::TRANSPARENT);
    // Semantic aliases (resolved at theme time in Godot gogo; here at compile).
    for (alias, of) in [("accent", "blue"), ("on_accent", "white"), ("danger", "red"), ("warning", "yellow"), ("success", "green"), ("info", "teal"), ("text_muted", "gray")] {
        let c = p[of];
        p.insert(alias.to_string(), c);
    }
    p
}

impl<'a> Compiler<'a> {
    fn error(&mut self, code: &str, line: u32, message: impl Into<String>) {
        self.diags.push(Diagnostic::error(code, line, message));
    }

    fn warning(&mut self, code: &str, line: u32, message: impl Into<String>) {
        self.diags.push(Diagnostic::warning(code, line, message));
    }

    fn intern(&mut self, s: &str) -> StrId {
        self.page.intern(s)
    }

    // ----- directives -----------------------------------------------------

    /// `is_main`: the file whose root becomes the page (a `uses` file only
    /// contributes directives).
    fn document(&mut self, doc: &Document, is_main: bool) {
        // Pass 1: directives, in order (uses first so later lines can override).
        for item in &doc.items {
            match item {
                AstItem::Assign { key, dollar: true, value, line } => {
                    let v = self.value(value, *line, None);
                    let name = self.intern(key);
                    if self.page.theme.iter().any(|t| t.name == name && t.variant == NO_STR) {
                        self.error("G3001", *line, format!("`${key}` is defined twice"));
                    }
                    self.page.theme.push(ThemeEntry { name, variant: NO_STR, value: v });
                }
                AstItem::Assign { key, dollar: false, line, .. } => {
                    self.error("G0022", *line, format!("`{key} = …` is only legal inside a `when` block"));
                }
                AstItem::Node(n) => match n.type_word.as_str() {
                    "uses" => self.uses(n),
                    "theme" => self.theme(n),
                    "style" => self.style(n),
                    "widget" => self.widget(n),
                    "anim" => self.anim(n),
                    "arrangement" => self.arrangement(n),
                    "transition" => {
                        let t = self.transition(n, NO_NODE);
                        self.page.transitions.push(t);
                    }
                    "when" => {
                        if let Some(state) = self.state_word(n, true) {
                            self.deferred_whens.push((state, n.children.clone(), n.line));
                        }
                    }
                    _ => {}
                },
            }
        }
        if !is_main {
            return;
        }
        // Pass 2: the root.
        for item in &doc.items {
            if let AstItem::Node(n) = item {
                if matches!(n.type_word.as_str(), "uses" | "theme" | "style" | "widget" | "anim" | "arrangement" | "transition" | "when") {
                    continue;
                }
                if self.root_seen {
                    self.error("G7003", n.line, "a file has one root (`View` or `Panel`)");
                    continue;
                }
                self.root_seen = true;
                if !matches!(n.type_word.as_str(), "View" | "Panel") && !self.widgets.contains_key(&n.type_word) {
                    self.error("G7003", n.line, format!("the root must be `View` (a screen) or `Panel` (a component), not `{}`", n.type_word));
                }
                let tmp = self.node(n, None, None);
                self.flatten(tmp);
            }
        }
    }

    fn uses(&mut self, n: &AstNode) {
        let Some(path) = n.args.first().and_then(|a| a.as_str()) else {
            self.error("G0023", n.line, "`uses` needs a quoted path");
            return;
        };
        let Some(text) = (self.loader)(path) else {
            self.error("G0024", n.line, format!("`uses \"{path}\"`: file not found"));
            return;
        };
        let mut diags = Vec::new();
        let doc = parse(&text, &mut diags);
        for d in diags {
            self.diags.push(Diagnostic { message: format!("{path}: {}", d.message), ..d });
        }
        self.document(&doc, false);
    }

    fn theme(&mut self, n: &AstNode) {
        let variant = match &n.name {
            Some(v) => self.intern(v),
            None => NO_STR,
        };
        for item in &n.children {
            match item {
                AstItem::Assign { key, dollar: true, value, line } => {
                    let v = self.value(value, *line, None);
                    let name = self.intern(key);
                    if self.page.theme.iter().any(|t| t.name == name && t.variant == variant) {
                        self.error("G3001", *line, format!("`${key}` is defined twice in this theme block"));
                    }
                    self.page.theme.push(ThemeEntry { name, variant, value: v });
                }
                AstItem::Assign { key, dollar: false, value, line } => {
                    // Control defaults: `font = $x`, `font_size = 15` land on
                    // every text-bearing kind.
                    if let Some(pk) = PropKey::from_word(key) {
                        if matches!(pk, PropKey::Font | PropKey::FontSize | PropKey::Color) {
                            let v = self.value(value, *line, Some(pk));
                            for kind in [Kind::Label, Kind::Button, Kind::Input, Kind::Check, Kind::Toggle, Kind::Radio, Kind::View, Kind::Panel] {
                                let first = self.page.props.len() as u32;
                                self.page.props.push(Prop { key: pk, value: v });
                                self.page.kind_defaults.push(KindDefault { kind, props: (first, 1) });
                            }
                            continue;
                        }
                    }
                    // Palette override: `red = #E51400` (§7: bare names only here).
                    if !self.palette.contains_key(key) {
                        self.error("G3002", *line, format!("`{key}` is not a palette color (new names are `$values`)"));
                        continue;
                    }
                    match self.color_value(value, *line) {
                        Some(c) => {
                            self.palette.insert(key.clone(), c);
                        }
                        None => self.error("G3003", *line, format!("`{key} = …` needs a color")),
                    }
                }
                AstItem::Node(kn) => {
                    // Kind defaults: `Button background=..., font_size=...`.
                    let Some(kind) = Kind::from_word(&kn.type_word) else {
                        self.error("G3004", kn.line, format!("`{}` is not a control kind", kn.type_word));
                        continue;
                    };
                    let first = self.page.props.len() as u32;
                    for p in &kn.props {
                        if let Some(prop) = self.prop(p, kind) {
                            self.page.props.push(prop);
                        }
                    }
                    let count = self.page.props.len() as u32 - first;
                    self.page.kind_defaults.push(KindDefault { kind, props: (first, count) });
                }
            }
        }
    }

    fn style(&mut self, n: &AstNode) {
        let Some(name) = n.name.clone() else {
            self.error("G0025", n.line, "`style` needs a name");
            return;
        };
        let mut def = StyleDef { props: n.props.clone(), ..Default::default() };
        for item in &n.children {
            match item {
                AstItem::Node(c) if c.type_word == "when" => def.whens.push(c.clone()),
                AstItem::Node(c) if c.type_word == "transition" => def.transitions.push(c.clone()),
                AstItem::Node(c) => self.error("G0026", c.line, format!("a style block holds `when` and `transition` lines, not `{}`", c.type_word)),
                AstItem::Assign { key, line, .. } => self.error("G0026", *line, format!("a style block holds `when` and `transition` lines, not `{key} =`")),
            }
        }
        self.styles.insert(name, def);
    }

    fn widget(&mut self, n: &AstNode) {
        let Some(name) = n.name.clone() else {
            self.error("G0027", n.line, "`widget` needs a name");
            return;
        };
        let bodies: Vec<&AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).collect();
        if bodies.len() != 1 {
            self.error("G0028", n.line, format!("widget `{name}` needs exactly one root node in its body"));
            return;
        }
        self.widgets.insert(name, WidgetDef { params: n.params.clone(), body: bodies[0].clone(), line: n.line });
    }

    fn anim(&mut self, n: &AstNode) {
        let Some(name) = n.name.clone() else {
            self.error("G0029", n.line, "`anim` needs a name");
            return;
        };
        if self.anim_decls.iter().any(|a| a.name.as_deref() == Some(&name)) {
            self.error("G0030", n.line, format!("anim `{name}` is defined twice"));
        }
        self.anim_decls.push(n.clone());
    }

    /// A header word (`duration_each`, `easing`) from the nearest scope.
    fn anim_num(scopes: &[&AstNode], key: &str) -> Option<f32> {
        scopes.iter().rev().find_map(|s| s.props.iter().find(|p| p.key == key).and_then(|p| p.value.as_num()))
    }

    fn anim_word(scopes: &[&AstNode], key: &str) -> Option<String> {
        scopes.iter().rev().find_map(|s| s.props.iter().find(|p| p.key == key).and_then(|p| p.value.as_ident()).map(str::to_string))
    }

    /// Compiles one anim into flat tracks; returns the end time of the chain.
    fn anim_chain(&mut self, anim: StrId, items: &[AstItem], scopes: &[&AstNode], chain_start: f32) -> f32 {
        // `and` joins the previous step; `then` starts after the whole and-cluster.
        let mut cluster_start = chain_start;
        let mut cluster_end = chain_start;
        let mut first = true;
        for item in items {
            let AstItem::Node(step) = item else { continue };
            let (word, connective, target) = match step.type_word.as_str() {
                "and" | "then" => {
                    // `then delay=0.2` has no step word: the prop names it.
                    let w = step.name.clone().unwrap_or_else(|| if step.props.iter().any(|p| p.key == "delay") { "delay".to_string() } else { String::new() });
                    (w, step.type_word.as_str(), step.args.first().and_then(|a| a.as_ident()).map(str::to_string))
                }
                other => (other.to_string(), "", step.name.clone()),
            };
            if !first && connective.is_empty() {
                self.error("G0061", step.line, "every step after the first says when it runs: `and` or `then`");
            }
            if first && !connective.is_empty() {
                self.error("G0061", step.line, "the first step stands alone — no `and`/`then`");
            }
            if connective == "then" {
                cluster_start = cluster_end;
            }
            first = false;
            if step.props.iter().any(|p| p.key == "after") {
                self.error("G0062", step.line, "`after=` is not supported yet — use `and`/`then`");
            }
            let mut scopes: Vec<&AstNode> = scopes.to_vec();
            let end = match word.as_str() {
                "group" => {
                    scopes.push(step);
                    let e = self.anim_chain(anim, &step.children, &scopes, cluster_start);
                    let _ = target;
                    e
                }
                "delay" => {
                    let d = step.props.iter().find(|p| p.key == "delay").and_then(|p| p.value.as_num()).unwrap_or(0.0);
                    cluster_start + d
                }
                "play" | "each" | "key" => {
                    self.error("G0062", step.line, format!("`{word}` steps are not supported yet"));
                    cluster_start
                }
                path => {
                    // `node.channel[.axis]` — the last one or two words are the channel.
                    let (node_name, channel) = match path.split_once('.') {
                        Some((n, rest)) if Channel::parse(rest).is_some() => (n.to_string(), rest.to_string()),
                        _ => {
                            if Channel::parse(path).is_some() {
                                self.error("G0063", step.line, format!("`{path}` needs a node: `<node>.{path}` (bare paths only inside `each`)"));
                            } else {
                                self.error("G0063", step.line, format!("`{path}`: want `<node>.translate.x|translate.y|scale|scale.x|scale.y|rotate|opacity`"));
                            }
                            continue;
                        }
                    };
                    let Some(node) = self.page.find(&node_name) else {
                        self.error("G0060", step.line, format!("`{path}`: no node named `{node_name}`"));
                        continue;
                    };
                    let num = |key: &str| step.props.iter().find(|p| p.key == key).and_then(|p| p.value.as_num());
                    let from = match step.props.iter().find(|p| p.key == "from") {
                        Some(p) if p.value.as_ident() == Some("current") => None,
                        Some(p) => p.value.as_num(),
                        None => None,
                    };
                    let Some(to) = num("to") else {
                        self.error("G0064", step.line, format!("`{path}` needs `to=`"));
                        continue;
                    };
                    scopes.push(step);
                    let duration = num("duration").or_else(|| Self::anim_num(&scopes, "duration_each")).unwrap_or(0.0);
                    let easing_word = step.props.iter().find(|p| p.key == "easing").and_then(|p| p.value.as_ident()).map(str::to_string).or_else(|| Self::anim_word(&scopes, "easing"));
                    let easing = match easing_word {
                        Some(w) => {
                            if Easing::parse(&w).is_none() {
                                self.error("G0065", step.line, format!("unknown easing `{w}`"));
                            }
                            self.intern(&w)
                        }
                        None => NO_STR,
                    };
                    let offset = num("offset").unwrap_or(0.0);
                    let start = (cluster_start + offset).max(0.0);
                    let channel_id = self.intern(&channel);
                    self.page.anims.push(AnimTrack { anim, node, channel: channel_id, from, to, start, duration, easing });
                    start + duration
                }
            };
            cluster_end = cluster_end.max(end);
        }
        cluster_end
    }

    fn arrangement(&mut self, n: &AstNode) {
        let Some(name) = n.name.clone() else {
            self.error("G0031", n.line, "`arrangement` needs a name");
            return;
        };
        if name == "base" || name == "any" {
            self.error("G0032", n.line, format!("`{name}` is reserved"));
            return;
        }
        let id = self.intern(&name);
        if self.page.arrangements.contains(&id) {
            self.error("G0033", n.line, format!("arrangement `{name}` is declared twice"));
        }
        self.page.arrangements.push(id);
    }

    /// `any` / `base` / a state / an arrangement name.
    fn endpoint(&mut self, word: &str, line: u32, node: NodeId) -> Option<Option<State>> {
        if word == "any" {
            return Some(None);
        }
        if word == "base" {
            return Some(Some(State::Arrangement(u32::MAX)));
        }
        self.state_from_word(word, line, node == NO_NODE).map(Some)
    }

    fn transition(&mut self, n: &AstNode, node: NodeId) -> Transition {
        let mut t = Transition { node, from: None, to: None, duration: 0.0, easing: NO_STR, anim: NO_STR };
        if let Some(from) = n.args.first().and_then(|a| a.as_ident()) {
            if let Some(f) = self.endpoint(from, n.line, node) {
                t.from = f;
            }
        }
        if let Some(to) = n.arrows.first() {
            if let Some(tt) = self.endpoint(to, n.line, node) {
                t.to = tt;
            }
        }
        for p in &n.props {
            match p.key.as_str() {
                "duration" => t.duration = p.value.as_num().unwrap_or(0.0),
                "easing" => t.easing = self.intern(p.value.as_ident().unwrap_or("")),
                "anim" => t.anim = self.intern(p.value.as_ident().unwrap_or("")),
                other => self.error("G0034", p.line, format!("`transition` takes duration=, easing=, anim= — not `{other}`")),
            }
        }
        t
    }

    fn state_from_word(&mut self, word: &str, line: u32, top_level: bool) -> Option<State> {
        Some(match word {
            "held" => State::Held,
            "hovered" => State::Hovered,
            "focused" => State::Focused,
            "focus_within" => State::FocusWithin,
            "selected" => State::Selected,
            "disabled" => State::StateDisabled,
            "pressed" | "hover" | "focus" | "checked" | "dragging" => {
                let want = match word {
                    "pressed" | "dragging" => "held",
                    "hover" => "hovered",
                    "focus" => "focused",
                    _ => "selected",
                };
                self.error("G0035", line, format!("`{word}` is spelled `{want}`"));
                return None;
            }
            _ if word.starts_with("screen.") => {
                let class = self.intern(&word["screen.".len()..]);
                State::Screen(class)
            }
            _ => {
                let id = self.page.strings.iter().position(|s| s == word).map(|i| i as StrId);
                match id.and_then(|id| self.page.arrangements.iter().position(|a| *a == id)) {
                    Some(i) => State::Arrangement(i as u32),
                    None => {
                        let hint = if top_level { "declare it with `arrangement <name>` first" } else { "built-ins are held, hovered, focused, focus_within, selected, disabled, screen.<class>" };
                        self.error("G0036", line, format!("unknown state `{word}` — {hint}"));
                        return None;
                    }
                }
            }
        })
    }

    fn state_word(&mut self, n: &AstNode, top_level: bool) -> Option<State> {
        let Some(word) = n.name.clone() else {
            self.error("G0037", n.line, "`when` needs a state");
            return None;
        };
        self.state_from_word(&word, n.line, top_level)
    }

    // ----- values ---------------------------------------------------------

    fn prop_key(&mut self, key: &str) -> PropKey {
        PropKey::from_word(key).unwrap_or_else(|| PropKey::Other(self.intern(key)))
    }

    fn is_color_key(key: PropKey) -> bool {
        matches!(key, PropKey::Background | PropKey::Color | PropKey::BorderColor | PropKey::Stops)
    }

    /// A color from a hex, palette word or `$ref` (the ref stays live).
    fn color_value(&mut self, v: &AstValue, line: u32) -> Option<Rgba> {
        match v {
            AstValue::Color { rgba, opacity, .. } => {
                let c = Rgba(*rgba);
                Some(opacity.map_or(c, |o| c.with_opacity(o)))
            }
            AstValue::Ident { word, opacity, .. } => {
                let c = *self.palette.get(word)?;
                Some(opacity.map_or(c, |o| c.with_opacity(o)))
            }
            AstValue::Ref { name, .. } => {
                let param = self.scopes.iter().rev().find_map(|s| s.get(name).cloned());
                param.and_then(|p| self.color_value(&p, line))
            }
            _ => None,
        }
    }

    fn value(&mut self, v: &AstValue, line: u32, key: Option<PropKey>) -> Value {
        match v {
            AstValue::Bool(b) => Value::VBool(*b),
            AstValue::Num { value, unit } => {
                if unit == "px" {
                    self.error("G0038", line, "`px` is gone — lengths are epx (§7)");
                }
                Value::VNum(*value)
            }
            AstValue::Color { rgba, opacity, pos } => {
                let c = Rgba(*rgba);
                let c = opacity.map_or(c, |o| c.with_opacity(o));
                match pos {
                    Some(p) => Value::VStop { color: c, pos: Some(*p) },
                    None if key == Some(PropKey::Stops) => Value::VStop { color: c, pos: None },
                    None => Value::VColor(c),
                }
            }
            AstValue::Str { text, key: term } => {
                let text_id = self.intern(text);
                let key_id = match term {
                    Some(k) => {
                        if let Some(prev) = self.terms.get(k) {
                            if prev != text {
                                self.error("G8001", line, format!("key `{k}` carries \"{prev}\" elsewhere, here \"{text}\" — one key, one English string"));
                            }
                        } else {
                            self.terms.insert(k.clone(), text.clone());
                        }
                        self.intern(k)
                    }
                    None => NO_STR,
                };
                Value::VText { text: text_id, key: key_id }
            }
            AstValue::Ref { name, opacity } => {
                // A widget param in scope substitutes; else a theme ref.
                let param = self.scopes.iter().rev().find_map(|s| s.get(name).cloned());
                if let Some(p) = param {
                    let inner = self.value(&p, line, key);
                    return match (inner, opacity) {
                        (Value::VColor(c), Some(o)) => Value::VColor(c.with_opacity(*o)),
                        (Value::VRef { name, .. }, Some(o)) => Value::VRef { name, opacity: Some(*o) },
                        (v, _) => v,
                    };
                }
                let id = self.intern(name);
                Value::VRef { name: id, opacity: *opacity }
            }
            AstValue::Ident { word, opacity, pos } => {
                let color_position = key.map(Self::is_color_key).unwrap_or(false);
                if color_position {
                    if let Some(c) = self.palette.get(word).copied() {
                        let c = opacity.map_or(c, |o| c.with_opacity(o));
                        return match pos {
                            Some(p) => Value::VStop { color: c, pos: Some(*p) },
                            None if key == Some(PropKey::Stops) => Value::VStop { color: c, pos: None },
                            None => Value::VColor(c),
                        };
                    }
                    self.error("G2001", line, format!("`{word}` is not a color"));
                }
                if opacity.is_some() || pos.is_some() {
                    self.error("G0039", line, format!("`/` and `@` suffixes are for colors and strings, not `{word}`"));
                }
                Value::VIdent(self.intern(word))
            }
            AstValue::Size { w, h } => Value::VSize(*w, *h),
            AstValue::Call { name, args } => {
                let compiled: Vec<Value> = args.iter().map(|a| self.value(a, line, None)).collect();
                let first = self.page.values.len() as u32;
                self.page.values.extend(compiled);
                let count = self.page.values.len() as u32 - first;
                Value::VCall { name: self.intern(name), args: (first, count) }
            }
            AstValue::List(items) => {
                let compiled: Vec<Value> = items.iter().map(|a| self.value(a, line, key)).collect();
                let first = self.page.values.len() as u32;
                self.page.values.extend(compiled);
                let count = self.page.values.len() as u32 - first;
                Value::VList(first, count)
            }
        }
    }

    fn prop(&mut self, p: &AstProp, kind: Kind) -> Option<Prop> {
        match p.key.as_str() {
            "interactive" => {
                self.error("G0040", p.line, "`interactive=` is gone — a node with handlers takes input, `disabled=true` blocks it (§7)");
                return None;
            }
            "overlay" => {
                self.error("G0040", p.line, "`overlay=` is gone (§7)");
                return None;
            }
            "anchors" | "offset_left" | "offset_top" | "offset_right" | "offset_bottom" | "size_flags_horizontal" | "size_flags_vertical" => {
                self.error("G0041", p.line, format!("`{}` is gone — placement says it all (halign/valign, width/height, margins)", p.key));
                return None;
            }
            "stretch_mode" | "expand_mode" => {
                self.error("G0041", p.line, format!("`{}` is `fit=contain|cover|distort`", p.key));
                return None;
            }
            "autowrap_mode" => {
                self.error("G0041", p.line, "`autowrap_mode` is `wrap=true`");
                return None;
            }
            "texture" => {
                self.error("G0041", p.line, "`texture=` is `src=\"assets/…\"` (a path relative to the assets root, checked at build)");
                return None;
            }
            _ => {}
        }
        let key = self.prop_key(&p.key);
        if let PropKey::Other(_) = key {
            if kind != Kind::Custom && !p.key.starts_with("on:") {
                self.warning("G0042", p.line, format!("`{}` is not a known property of `{kind:?}`", p.key));
            }
        }
        let value = self.value(&p.value, p.line, Some(key));
        Some(Prop { key, value })
    }

    // ----- nodes ----------------------------------------------------------

    /// Compiles `n` (a child of `parent_kind`, or the root) into a temporary
    /// tree; `flatten` lays it into the arena.
    fn node(&mut self, n: &AstNode, parent_kind: Option<Kind>, instance: Option<&AstNode>) -> TmpNode {
        // Widget instance: expand the body with the instance's args in scope.
        if let Some(def) = self.widgets.get(&n.type_word).cloned() {
            let mut scope = HashMap::new();
            for p in &def.params {
                scope.insert(p.key.clone(), p.value.clone());
            }
            for p in &n.props {
                if scope.contains_key(&p.key) {
                    scope.insert(p.key.clone(), p.value.clone());
                }
            }
            self.scopes.push(scope);
            let t = self.node(&def.body, parent_kind, Some(n));
            self.scopes.pop();
            let _ = def.line;
            return t;
        }

        let kind = match Kind::from_word(&n.type_word) {
            Some(k) => k,
            None => {
                if let Some(word) = Kind::retired(&n.type_word) {
                    self.error("G7010", n.line, format!("`{}` is spelled `{word}`", n.type_word));
                } else {
                    self.error("G7011", n.line, format!("unknown node type `{}`", n.type_word));
                }
                Kind::Custom
            }
        };
        if kind == Kind::View && parent_kind.is_some() {
            self.error("G7012", n.line, "`View` is the screen root only — inside the tree use `Layers` (free placement) or `Panel` (a styled box)");
        }
        for f in &n.flags {
            self.error("G0043", n.line, format!("`{f}` is gone — handlers are the `OnHandler` enum (§7)"));
        }
        let mut node = Node { kind, line: n.line, ..Node::EMPTY };
        node.type_name = self.intern(&n.type_word);
        // The instance's name wins over the widget body's.
        let name = instance.and_then(|i| i.name.clone()).or_else(|| n.name.clone());
        if let Some(name) = &name {
            if self.names.contains(name) {
                self.error("G0044", n.line, format!("node name `{name}` is used twice"));
            }
            self.names.insert(name.clone());
            node.name = self.intern(name);
        }
        let mut tmp = TmpNode { node, props: Vec::new(), handlers: Vec::new(), whens: Vec::new(), transitions: Vec::new(), children: Vec::new() };

        if kind == Kind::Gap {
            self.gap(&mut tmp.node, n, parent_kind);
            if !n.props.is_empty() {
                self.error("G0045", n.line, "`gap` takes one size word and nothing else");
            }
        }

        // Props: style base → node props → instance props (later wins on lookup).
        let mut all_props: Vec<AstProp> = Vec::new();
        let mut style_whens: Vec<AstNode> = Vec::new();
        let mut style_transitions: Vec<AstNode> = Vec::new();
        let styles: Vec<String> = n.styles.iter().chain(instance.iter().flat_map(|i| i.styles.iter())).cloned().collect();
        for s in &styles {
            match self.styles.get(s).cloned() {
                Some(def) => {
                    all_props.extend(def.props.iter().cloned());
                    style_whens.extend(def.whens);
                    style_transitions.extend(def.transitions);
                }
                None => self.error("G4001", n.line, format!("unknown style `.{s}`")),
            }
        }
        all_props.extend(n.props.iter().cloned());
        for c in &n.children {
            if let AstItem::Node(t) = c {
                if t.type_word == "theme" {
                    // `theme font=…` inside a block: a default for this subtree
                    // (font inherits down the draw walk).
                    all_props.extend(t.props.iter().cloned());
                }
            }
        }
        if let Some(i) = instance {
            let params: Vec<String> = self.scopes.last().map(|s| s.keys().cloned().collect()).unwrap_or_default();
            all_props.extend(i.props.iter().filter(|p| !params.contains(&p.key)).cloned());
        }
        // Positional size arg: `Button 44x44`.
        for a in n.args.iter().chain(instance.iter().flat_map(|i| i.args.iter())) {
            if let AstValue::Size { .. } = a {
                all_props.push(AstProp { key: "size".to_string(), value: a.clone(), line: n.line });
            } else if kind != Kind::Gap && kind != Kind::Repeat {
                self.error("G0046", n.line, format!("unexpected value {a:?} on `{}`", n.type_word));
            }
        }
        for (k, v) in &n.clauses {
            all_props.push(AstProp { key: k.clone(), value: v.clone(), line: n.line });
        }

        let mut placement_props: Vec<Prop> = Vec::new();
        for p in &all_props {
            if let Some(action) = p.key.strip_prefix("on:") {
                if let Some(h) = self.handler(action, p) {
                    tmp.handlers.push(h);
                }
                continue;
            }
            let Some(prop) = self.prop(p, kind) else { continue };
            if prop.key.is_placement() {
                placement_props.push(prop);
            } else {
                tmp.props.push(prop);
            }
        }
        tmp.node.placement = self.placement(&placement_props, n.line, parent_kind, kind);

        // Whens and transitions: style's first, then the node's.
        let own_whens = n.children.iter().filter_map(|c| match c {
            AstItem::Node(w) if w.type_word == "when" => Some(w),
            _ => None,
        });
        for w in style_whens.iter().chain(own_whens) {
            if let Some(tw) = self.when(w, kind) {
                tmp.whens.push(tw);
            }
        }
        let own_transitions = n.children.iter().filter_map(|c| match c {
            AstItem::Node(t) if t.type_word == "transition" => Some(t),
            _ => None,
        });
        for t in style_transitions.iter().chain(own_transitions) {
            let tr = self.transition(t, PENDING_NODE);
            tmp.transitions.push(tr);
        }

        for c in &n.children {
            match c {
                AstItem::Node(c) if c.type_word == "when" || c.type_word == "transition" || c.type_word == "theme" => {}
                AstItem::Node(c) => {
                    let child = self.node(c, Some(kind), None);
                    tmp.children.push(child);
                }
                AstItem::Assign { key, line, .. } => {
                    self.error("G0022", *line, format!("`{key} = …` is only legal inside a `when` block"));
                }
            }
        }
        tmp
    }

    /// Lays a temporary tree into the arena: a node, then ALL its direct
    /// children (contiguous), then each child's own children — so
    /// `children` is a range and `subtree_end` bounds the descendants.
    fn flatten(&mut self, root: TmpNode) {
        let id = self.push_slot(root.node, root.props, root.handlers, root.whens, root.transitions, NO_NODE);
        self.place_children(id, root.children);
    }

    fn push_slot(&mut self, mut node: Node, props: Vec<Prop>, handlers: Vec<OnHandler>, whens: Vec<WhenTmp>, transitions: Vec<Transition>, parent: NodeId) -> NodeId {
        let id = self.page.nodes.len() as NodeId;
        node.parent = parent;
        let pf = self.page.props.len() as u32;
        self.page.props.extend(props);
        node.props = (pf, self.page.props.len() as u32 - pf);
        let hf = self.page.handlers.len() as u32;
        self.page.handlers.extend(handlers);
        node.handlers = (hf, self.page.handlers.len() as u32 - hf);
        self.page.nodes.push(node);
        self.node_whens.push(whens);
        for mut t in transitions {
            t.node = id;
            self.page.transitions.push(t);
        }
        id
    }

    fn place_children(&mut self, id: NodeId, children: Vec<TmpNode>) {
        let first = self.page.nodes.len() as u32;
        let count = children.len() as u32;
        let mut slots: Vec<(NodeId, Vec<TmpNode>)> = Vec::new();
        for child in children {
            let cid = self.push_slot(child.node, child.props, child.handlers, child.whens, child.transitions, id);
            slots.push((cid, child.children));
        }
        self.page.nodes[id as usize].children = (first, count);
        for (cid, rest) in slots {
            self.place_children(cid, rest);
        }
        self.page.nodes[id as usize].subtree_end = self.page.nodes.len() as u32;
    }

    fn gap(&mut self, node: &mut Node, n: &AstNode, parent_kind: Option<Kind>) {
        if !parent_kind.map(Kind::is_stack).unwrap_or(false) {
            self.error("G0047", n.line, "`gap` lives in a Column/Row (Flow → spacing=, Grid → tracks, Layers → margins)");
        }
        match n.args.first() {
            Some(AstValue::Num { value, .. }) => node.gap = *value,
            Some(AstValue::Ident { word, .. }) if word == "fill" => {
                node.gap_fill = true;
                node.gap = 1.0;
            }
            Some(AstValue::Call { name, args }) if name == "fill" => {
                node.gap_fill = true;
                node.gap = args.first().and_then(AstValue::as_num).unwrap_or(1.0);
            }
            _ => self.error("G0048", n.line, "`gap` needs a size: `gap 12` or `gap fill`"),
        }
        if n.name.is_some() {
            self.error("G0045", n.line, "`gap` is never named — space you must address is a real node");
        }
    }

    fn handler(&mut self, event: &str, p: &AstProp) -> Option<OnHandler> {
        if !EVENTS.contains(&event) {
            let hint = match event {
                "pressed" | "activated" => "activate",
                "toggled" | "changed" | "value_changed" | "text_submitted" => "change",
                "changing" | "text_changed" | "input" => "edit",
                _ => "",
            };
            if hint.is_empty() {
                self.error("G0049", p.line, format!("`on:{event}` — raw signals are gone (§7); the events are activate, change, edit, show, hide"));
            } else {
                self.error("G0049", p.line, format!("`on:{event}` is spelled `on:{hint}`"));
            }
            return None;
        }
        let action = match &p.value {
            AstValue::Ident { word, .. } => Action::Named(self.intern(word)),
            AstValue::Call { name, args } => {
                let arg = args.first().and_then(AstValue::as_ident).unwrap_or("");
                match name.as_str() {
                    "go" | "toggle" => {
                        let target = if arg == "base" {
                            if name == "toggle" {
                                self.error("G0050", p.line, "`toggle(base)` — base is home, use `go(base)`");
                            }
                            u32::MAX
                        } else {
                            let id = self.intern(arg);
                            match self.page.arrangements.iter().position(|a| *a == id) {
                                Some(i) => i as u32,
                                None => {
                                    self.error("G0051", p.line, format!("`{name}({arg})`: unknown arrangement"));
                                    return None;
                                }
                            }
                        };
                        if name == "go" { Action::Go(target) } else { Action::ToggleArrangement(target) }
                    }
                    "play" => Action::Play(self.intern(arg)),
                    "emit" => Action::Emit(self.intern(arg)),
                    other => {
                        self.error("G0052", p.line, format!("`{other}(…)` is not an action (play, go, toggle, emit, or a handler name)"));
                        return None;
                    }
                }
            }
            other => {
                self.error("G0052", p.line, format!("`on:{event}` needs a handler name or an action, not {other:?}"));
                return None;
            }
        };
        let event_id = self.intern(event);
        Some(OnHandler { event: event_id, action })
    }

    fn when(&mut self, w: &AstNode, kind: Kind) -> Option<WhenTmp> {
        let state = self.state_word(w, false)?;
        if state == State::Held && !kind.is_press_surface() {
            self.error("G0053", w.line, format!("`when held` needs a press surface (a Button, Check, Toggle, Radio), not `{kind:?}`"));
            return None;
        }
        let mut props = Vec::new();
        let mut descendants = Vec::new();
        let mut lines: Vec<AstProp> = w.props.clone();
        for item in &w.children {
            match item {
                AstItem::Assign { key, dollar: false, value, line } => lines.push(AstProp { key: key.clone(), value: value.clone(), line: *line }),
                AstItem::Assign { key, line, .. } => self.error("G0054", *line, format!("`${key}` — a when block sets properties, not theme values")),
                AstItem::Node(c) => self.error("G0054", c.line, format!("a when block sets properties, not `{}` nodes", c.type_word)),
            }
        }
        for p in &lines {
            if let Some((node_name, prop_name)) = p.key.rsplit_once('.') {
                let ap = AstProp { key: prop_name.to_string(), value: p.value.clone(), line: p.line };
                if let Some(prop) = self.prop(&ap, Kind::Custom) {
                    descendants.push((node_name.to_string(), prop));
                }
            } else if let Some(prop) = self.prop(p, kind) {
                props.push(prop);
            }
        }
        Some(WhenTmp { state, props, descendants })
    }

    // ----- placement ------------------------------------------------------

    fn len(&mut self, v: Value, line: u32, key: &str) -> Len {
        match v {
            Value::VNum(n) => Len::Exact(n),
            Value::VIdent(id) => match self.page.str(id) {
                "fill" => Len::Fill(1.0),
                "hug" => Len::Hug(1.0),
                "host" => {
                    self.error("G0055", line, format!("`{key}=host` is gone — a pure layout needs no host axis (§7)"));
                    Len::Unset
                }
                other => {
                    let other = other.to_string();
                    self.error("G0056", line, format!("`{key}={other}`: want a number, `fill`, `hug`, `fill(n)`"));
                    Len::Unset
                }
            },
            Value::VCall { name, args } => {
                let weight = self.page.values.get(args.0 as usize).and_then(|a| if let Value::VNum(n) = a { Some(*n) } else { None }).unwrap_or(1.0);
                match self.page.str(name) {
                    "fill" => Len::Fill(weight),
                    "hug" => Len::Hug(weight),
                    _ => {
                        self.error("G0056", line, format!("`{key}=…`: want `fill(n)` or `hug(n)`"));
                        Len::Unset
                    }
                }
            }
            _ => {
                self.error("G0056", line, format!("`{key}=`: want a number, `fill` or `hug`"));
                Len::Unset
            }
        }
    }

    fn align(&mut self, v: Value, line: u32, key: &str) -> Align {
        if let Value::VIdent(id) = v {
            match self.page.str(id) {
                "left" | "top" | "start" => return Align::Start,
                "center" => return Align::Center,
                "right" | "bottom" | "end" => return Align::End,
                _ => {}
            }
        }
        self.error("G0057", line, format!("`{key}=`: want left/center/right (top/center/bottom)"));
        Align::AlignUnset
    }

    fn num(&mut self, v: Value, line: u32, key: &str) -> f32 {
        match v {
            Value::VNum(n) => n,
            _ => {
                self.error("G0058", line, format!("`{key}=` wants a number (epx)"));
                0.0
            }
        }
    }

    fn placement(&mut self, props: &[Prop], line: u32, parent: Option<Kind>, kind: Kind) -> Placement {
        let mut p = Placement::DEFAULT;
        for prop in props {
            let v = prop.value;
            match prop.key {
                PropKey::Width => p.width = self.len(v, line, "width"),
                PropKey::Height => p.height = self.len(v, line, "height"),
                PropKey::MinWidth => p.min_width = self.num(v, line, "min_width"),
                PropKey::MinHeight => p.min_height = self.num(v, line, "min_height"),
                PropKey::MaxWidth => match v {
                    Value::VIdent(id) if self.page.str(id) == "fill" => p.max_width_fill = true,
                    _ => p.max_width = Some(self.num(v, line, "max_width")),
                },
                PropKey::MaxHeight => match v {
                    Value::VIdent(id) if self.page.str(id) == "fill" => p.max_height_fill = true,
                    _ => p.max_height = Some(self.num(v, line, "max_height")),
                },
                PropKey::Halign => p.halign = self.align(v, line, "halign"),
                PropKey::Valign => p.valign = self.align(v, line, "valign"),
                PropKey::MarginLeft => p.margin[0] = Some(self.num(v, line, "margin_left")),
                PropKey::MarginTop => p.margin[1] = Some(self.num(v, line, "margin_top")),
                PropKey::MarginRight => p.margin[2] = Some(self.num(v, line, "margin_right")),
                PropKey::MarginBottom => p.margin[3] = Some(self.num(v, line, "margin_bottom")),
                PropKey::Margin => {
                    let m = self.num(v, line, "margin");
                    p.margin = [Some(m); 4];
                }
                PropKey::PaddingLeft => p.padding[0] = self.num(v, line, "padding_left"),
                PropKey::PaddingTop => p.padding[1] = self.num(v, line, "padding_top"),
                PropKey::PaddingRight => p.padding[2] = self.num(v, line, "padding_right"),
                PropKey::PaddingBottom => p.padding[3] = self.num(v, line, "padding_bottom"),
                PropKey::Padding => match v {
                    Value::VIdent(id) if self.page.str(id) == "safe" => p.padding_safe = true,
                    _ => p.padding = [self.num(v, line, "padding"); 4],
                },
                PropKey::Size => {
                    if let Value::VSize(w, h) = v {
                        if let Some(w) = w {
                            p.width = Len::Exact(w);
                        }
                        if let Some(h) = h {
                            p.height = Len::Exact(h);
                        }
                    }
                }
                PropKey::MinSize => {
                    if let Value::VSize(w, h) = v {
                        p.min_width = w.unwrap_or(0.0);
                        p.min_height = h.unwrap_or(0.0);
                    }
                }
                _ => {}
            }
        }
        // `margin=N` shorthand under alignment: the touched edges only.
        if props.iter().any(|x| x.key == PropKey::Margin) {
            match p.halign {
                Align::Start => p.margin[2] = None,
                Align::End => p.margin[0] = None,
                Align::Center => {
                    p.margin[0] = None;
                    p.margin[2] = None;
                }
                Align::AlignUnset => {}
            }
            match p.valign {
                Align::Start => p.margin[3] = None,
                Align::End => p.margin[1] = None,
                Align::Center => {
                    p.margin[1] = None;
                    p.margin[3] = None;
                }
                Align::AlignUnset => {}
            }
        }
        // Contradictions the spec refuses at load.
        if p.halign == Align::Center && matches!(p.width, Len::Fill(_)) && p.max_width.is_none() {
            self.error("G1001", line, "`halign=center` with `width=fill`: fill leaves nothing to position (use `hug`, or a `max_width` cap)");
        }
        if p.valign == Align::Center && matches!(p.height, Len::Fill(_)) && p.max_height.is_none() {
            self.error("G1001", line, "`valign=center` with `height=fill`: fill leaves nothing to position (use `hug`, or a `max_height` cap)");
        }
        if let (Len::Exact(_), true) = (p.width, p.min_width > 0.0) {
            self.error("G1002", line, "`min_width` next to an exact `width` is dead (an exact size is its own floor)");
        }
        if let (Some(max), true) = (p.max_width, p.min_width > 0.0) {
            if p.min_width > max {
                self.error("G1003", line, "`min_width` above `max_width`");
            }
        }
        if p.margin.iter().any(Option::is_some) && parent.map(|k| k.is_stack() || matches!(k, Kind::Flow | Kind::Grid | Kind::GridRow | Kind::Tiles)).unwrap_or(false) {
            self.error("G1005", line, "margins under a Column/Row/Flow/Grid are an error — space children with `spacing=` / `gap` (or `padding` on the parent)");
        }
        if p.margin[0].is_some() && p.margin[2].is_some() && matches!(p.width, Len::Exact(_)) {
            self.error("G1006", line, "both horizontal margins and an exact `width` over-constrain the axis");
        }
        let _ = kind;
        p
    }

    // ----- finish ---------------------------------------------------------

    fn finish(&mut self) {
        // Anims: flat tracks with start times.
        let decls = std::mem::take(&mut self.anim_decls);
        for n in &decls {
            let name = n.name.clone().unwrap_or_default();
            let anim = self.intern(&name);
            if n.props.iter().any(|p| p.key == "duration") {
                self.error("G0066", n.line, "`duration=` on an anim header reads as the chain's total — write `duration_each=`");
            }
            let scopes = vec![n];
            self.anim_chain(anim, &n.children, &scopes, 0.0);
        }
        // Top-level `when <arrangement> { node.prop = value }`.
        let deferred = std::mem::take(&mut self.deferred_whens);
        for (state, items, line) in deferred {
            for item in &items {
                let AstItem::Assign { key, dollar: false, value, line: l } = item else {
                    self.error("G0054", line, "a top-level `when` block holds `node.prop = value` lines");
                    continue;
                };
                let Some((node_name, prop_name)) = key.rsplit_once('.') else {
                    self.error("G0059", *l, format!("`{key}` — write `node.prop = value`"));
                    continue;
                };
                let Some(id) = self.page.find(node_name) else {
                    self.error("G0060", *l, format!("`{key}`: no node named `{node_name}`"));
                    continue;
                };
                let kind = self.page.nodes[id as usize].kind;
                let ap = AstProp { key: prop_name.to_string(), value: value.clone(), line: *l };
                if let Some(prop) = self.prop(&ap, kind) {
                    match self.node_whens[id as usize].iter_mut().find(|w| w.state == state) {
                        Some(w) => w.props.push(prop),
                        None => self.node_whens[id as usize].push(WhenTmp { state, props: vec![prop], descendants: Vec::new() }),
                    }
                }
            }
        }
        // Descendant responses move to the named node, watching their owner.
        let mut whens = std::mem::take(&mut self.node_whens);
        let mut moved: Vec<(usize, WhenTmp)> = Vec::new();
        for (owner, list) in whens.iter_mut().enumerate() {
            for w in list.iter_mut() {
                for (name, prop) in std::mem::take(&mut w.descendants) {
                    let end = self.page.nodes[owner].subtree_end as usize;
                    let target = (owner + 1..end).find(|i| self.page.nodes[*i].name != NO_STR && self.page.str(self.page.nodes[*i].name) == name);
                    match target {
                        Some(t) => moved.push((t, WhenTmp { state: w.state, props: vec![prop], descendants: Vec::new() })),
                        None => {
                            let line = self.page.nodes[owner].line;
                            self.error("G0060", line, format!("`when … {name}.…`: no node named `{name}` under this one"));
                        }
                    }
                }
            }
        }
        let mut of: Vec<Vec<NodeId>> = whens.iter().map(|l| vec![NO_NODE; l.len()]).collect();
        for (target, w) in moved {
            // The owner is the node whose subtree held the response: found by walking up until the state is theirs.
            let mut owner = self.page.nodes[target].parent;
            while owner != NO_NODE && !self.page.nodes[owner as usize].kind.is_press_surface() && self.page.nodes[owner as usize].parent != NO_NODE {
                owner = self.page.nodes[owner as usize].parent;
            }
            whens[target].push(w);
            of[target].push(owner);
        }
        // Whens pool, contiguous per node.
        for (i, list) in whens.into_iter().enumerate() {
            let first = self.page.whens.len() as u32;
            for (j, w) in list.into_iter().enumerate() {
                let pf = self.page.props.len() as u32;
                self.page.props.extend(w.props);
                let pc = self.page.props.len() as u32 - pf;
                self.page.whens.push(When { state: w.state, props: (pf, pc), of: of[i][j] });
            }
            self.page.nodes[i].whens = (first, self.page.whens.len() as u32 - first);
        }
        // Unresolved `$refs` (not a theme value anywhere).
        let refs: Vec<(StrId, u32)> = self
            .page
            .nodes
            .iter()
            .flat_map(|n| {
                let line = n.line;
                self.page.props[n.props.0 as usize..(n.props.0 + n.props.1) as usize].iter().filter_map(move |p| if let Value::VRef { name, .. } = p.value { Some((name, line)) } else { None })
            })
            .collect();
        for (name, line) in refs {
            if !self.page.theme.iter().any(|t| t.name == name) {
                let word = self.page.str(name).to_string();
                self.error("G9001", line, format!("`${word}` is not a theme value or widget parameter"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(src: &str) -> Compiled {
        compile(src, &|_| None)
    }

    fn ok(src: &str) -> Page {
        let c = build(src);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        c.page
    }

    fn codes(src: &str) -> Vec<String> {
        build(src).diags.iter().map(|d| d.code.clone()).collect()
    }

    #[test]
    fn arena_is_preorder_with_contiguous_children() {
        let p = ok("View root {\n Column col spacing=8 {\n  Label a text=\"A\"\n  Row r { Label b text=\"B\"; Label c text=\"C\" }\n  Label d text=\"D\"\n }\n Rectangle bg color=red\n}\n");
        let names: Vec<&str> = p.nodes.iter().map(|n| p.str(n.name)).collect();
        assert_eq!(names, vec!["root", "col", "bg", "a", "r", "d", "b", "c"]);
        let col = p.find("col").unwrap();
        assert_eq!(p.nodes[col as usize].children, (3, 3));
        assert_eq!(p.nodes[col as usize].subtree_end, 8);
        let r = p.find("r").unwrap();
        assert_eq!(p.nodes[r as usize].children, (6, 2));
        assert_eq!(p.nodes[r as usize].parent, col);
        assert_eq!(p.nodes[0].subtree_end, 8);
    }

    #[test]
    fn styles_theme_and_whens_merge_in_spec_order() {
        let p = ok("$accent = #E51400FF\ntheme dark { $accent = #FF6B4AFF }\ntheme { Button font_size=15 }\nstyle Item background=charcoal, min_height=36 { when selected background=$accent }\nView v {\n Button b .Item text=\"x\", background=#112233FF { when held background=white/0.5 }\n}\n");
        let b = p.find("b").unwrap();
        // Node prop wins over style base on lookup (later in the range).
        assert_eq!(p.prop(b, PropKey::Background), Some(Value::VColor(Rgba([0x11, 0x22, 0x33, 0xFF]))));
        assert_eq!(p.nodes[b as usize].placement.min_height, 36.0);
        let whens: Vec<State> = p.whens[p.nodes[b as usize].whens.0 as usize..].iter().map(|w| w.state).collect();
        assert_eq!(whens, vec![State::Selected, State::Held]);
        assert!(p.whens.iter().all(|w| w.of == NO_NODE));
        let held_props = &p.props[p.whens[1].props.0 as usize..];
        assert_eq!(held_props[0].value, Value::VColor(Rgba([255, 255, 255, 128])));
        let accent = p.intern_lookup("accent");
        assert_eq!(p.theme_value(accent, p.intern_lookup("dark")), Some(Value::VColor(Rgba([0xFF, 0x6B, 0x4A, 0xFF]))));
        assert_eq!(p.kind_default(Kind::Button, PropKey::FontSize), Some(Value::VNum(15.0)));
    }

    #[test]
    fn widgets_expand_with_params_and_instance_name() {
        let p = ok("widget LabeledSlider $label=\"\", $min=0, $max=100 {\n Column spacing=4 {\n  Label text=$label\n  Slider min=$min, max=$max\n }\n}\nView v {\n LabeledSlider size label=\"Size\", min=1, valign=top\n}\n");
        let names: Vec<&str> = p.nodes.iter().map(|n| p.str(n.name)).collect();
        assert_eq!(names, vec!["v", "size", "", ""]);
        assert_eq!(p.nodes[1].kind, Kind::Column);
        assert_eq!(p.nodes[1].placement.valign, Align::Start);
        let label = p.prop(2, PropKey::Text).unwrap();
        assert!(matches!(label, Value::VText { text, .. } if p.str(text) == "Size"));
        assert_eq!(p.prop(3, PropKey::Min), Some(Value::VNum(1.0)));
        assert_eq!(p.prop(3, PropKey::Max), Some(Value::VNum(100.0)));
    }

    #[test]
    fn arrangements_handlers_and_transitions() {
        let p = ok("arrangement wide\ntransition base -> wide duration=0.2, easing=cubic_out\nwhen wide { bar.width = 300 }\nView v {\n Panel bar width=200, on:activate=go(wide) { transition any -> held duration=0.1 }\n Button b text=\"Save\", on:activate=save\n}\n");
        let bar = p.find("bar").unwrap();
        let w = p.whens[p.nodes[bar as usize].whens.0 as usize];
        assert_eq!(w.state, State::Arrangement(0));
        assert_eq!(p.props[w.props.0 as usize], Prop { key: PropKey::Width, value: Value::VNum(300.0) });
        assert_eq!(p.handlers[0].action, Action::Go(0));
        let b = p.find("b").unwrap();
        let h = p.handlers[p.nodes[b as usize].handlers.0 as usize];
        assert_eq!(p.str(h.event), "activate");
        assert!(matches!(h.action, Action::Named(id) if p.str(id) == "save"));
        assert_eq!(p.transitions.len(), 2);
        assert_eq!(p.transitions[0].node, NO_NODE);
        assert_eq!(p.transitions[0].to, Some(State::Arrangement(0)));
        assert_eq!(p.transitions[1].node, bar);
    }

    #[test]
    fn removed_words_and_retired_spellings_are_errors() {
        assert!(codes("View v { Border b }").contains(&"G7010".to_string()));
        assert!(codes("View v { Panel p width=host }").contains(&"G0055".to_string()));
        assert!(codes("View v { Button b signals }").contains(&"G0043".to_string()));
        assert!(codes("View v { Button b on:pressed=x }").contains(&"G0049".to_string()));
        assert!(codes("View v { Panel p interactive=true }").contains(&"G0040".to_string()));
        assert!(codes("Column c { Label a }").contains(&"G7003".to_string()));
        assert!(codes("View v { Label a text=\"x\"/k; Label b text=\"y\"/k }").contains(&"G8001".to_string()));
        assert!(codes("View v { Label a color=$nothing }").contains(&"G9001".to_string()));
        assert!(codes("View v { Column c { Label a margin_top=3 } }").contains(&"G1005".to_string()));
        assert!(codes("View v { Label a when held color=red }").is_empty() == false);
    }

    #[test]
    fn placement_words() {
        let p = ok("View v {\n Panel a halign=center, valign=bottom, margin=16, width=fill, max_width=380, height=hug\n Panel b 44x44, margin_left=-10\n Column c { gap 12; gap fill; gap fill(2) }\n}\n");
        let a = p.nodes[p.find("a").unwrap() as usize].placement;
        assert_eq!(a.halign, Align::Center);
        assert_eq!(a.margin, [None, None, None, Some(16.0)]);
        assert_eq!(a.max_width, Some(380.0));
        assert_eq!(a.height, Len::Hug(1.0));
        let b = p.nodes[p.find("b").unwrap() as usize].placement;
        assert_eq!(b.width, Len::Exact(44.0));
        assert_eq!(b.margin[0], Some(-10.0));
        let c = p.find("c").unwrap();
        let gaps: Vec<(f32, bool)> = p.children(c).map(|i| (p.nodes[i].gap, p.nodes[i].gap_fill)).collect();
        assert_eq!(gaps, vec![(12.0, false), (1.0, true), (2.0, true)]);
    }
}

impl Page {
    /// A string id if the string is interned (tests, name lookups).
    pub fn intern_lookup(&self, s: &str) -> StrId {
        self.strings.iter().position(|x| x == s).map(|i| i as StrId).unwrap_or(NO_STR)
    }
}

#[cfg(test)]
mod anim_tests {
    use super::*;

    #[test]
    fn descendant_when_and_anim_tracks() {
        let c = compile("style OkLook { when held face.background=red }\nanim appear duration_each=0.5, easing=cubic_out {\n a.translate.x from=-60, to=0\n and b.opacity from=0, to=1\n then group drop duration_each=0.25 { b.scale to=1.05, easing=back_out }\n then delay=0.2\n then a.opacity to=1, duration=0.1\n}\nView v {\n Button ok .OkLook { Panel face { Label a text=\"x\" } }\n Label b text=\"y\"\n}\n", &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let p = c.page;
        let face = p.find("face").unwrap();
        let ok = p.find("ok").unwrap();
        let w = p.whens[p.nodes[face as usize].whens.0 as usize];
        assert_eq!((w.state, w.of), (State::Held, ok));
        let tracks: Vec<(&str, &str, Option<f32>, f32, f32, f32)> = p.anims.iter().map(|t| (p.str(p.nodes[t.node as usize].name), p.str(t.channel), t.from, t.to, t.start, t.duration)).collect();
        assert_eq!(tracks[0], ("a", "translate.x", Some(-60.0), 0.0, 0.0, 0.5));
        assert_eq!(tracks[1], ("b", "opacity", Some(0.0), 1.0, 0.0, 0.5));
        assert_eq!(tracks[2], ("b", "scale", None, 1.05, 0.5, 0.25));
        assert_eq!(tracks[3], ("a", "opacity", None, 1.0, 0.95, 0.1));
        assert_eq!(p.str(p.anims[2].easing), "back_out");
        assert_eq!(p.str(p.anims[0].easing), "cubic_out");
    }
}
