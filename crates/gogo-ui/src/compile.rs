//! AST → `Page`. Styles, `uses`, widgets and the palette are resolved
//! here (the runtime never sees them); `$theme` refs and translation keys
//! stay live. Diagnostic codes are stable G-numbers (`G0041`, `G7011` …).

use std::collections::HashMap;

use crate::anim::{Channel, EASING_PARAM_KEYS, Easing, PARAMS_FOR_CURVE};
use crate::ast::{AstItem, AstNode, AstProp, AstValue, Document};
use crate::diag::Diagnostic;
use crate::page::*;
use crate::runtime::{DISABLED, SELECTED};
use crate::parser::parse;

/// Reads a `uses "path"` file relative to the current one; `None` = not found.
pub type Loader<'a> = &'a dyn Fn(&str) -> Option<String>;

/// What the host program tells the compiler about its own world. gogo
/// itself knows no host: a node type it cannot name is an error unless the
/// host lists it here.
#[derive(Clone, Copy, Default)]
pub struct CompileOptions<'a> {
    /// Node types the host draws from code (no gogo body): each becomes an
    /// empty box of its declared size with a G7013 warning, so the page
    /// still lays out. Example: `&["GradientEdit"]`.
    pub code_controls: &'a [&'a str],
}

/// The gogo events; anything else on `on:` is a raw signal — gone (§7).
/// `press` fires on finger-down (the undo/redo feel); `release` exactly
/// once when that gesture ends, however it ends; `activate` on a
/// completed tap.
/// `touch` hands the host the raw finger (down/move/up/cancel with positions
/// in the node's own epx space) — a code-drawn control's input.
const EVENTS: &[&str] = &["activate", "press", "release", "change", "edit", "show", "hide", "touch"];

/// The extensions the build embeds (`build_pages` collects every literal
/// carrying one) — a literal naming one must write the `res://` scheme
/// (G0071), so the two lists and the check stay one rule.
pub(crate) const IMAGE_EXTS: &[&str] = &[".png", ".jpg", ".jpeg"];
pub(crate) const FONT_EXTS: &[&str] = &[".ttf", ".otf"];

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
    /// `anim` declarations beside the body: each instance gets its own
    /// copy, named `inst/anim`, its targets scoped to the instance.
    anims: Vec<AstNode>,
    /// `widget X for=Radio`: the body fills a host control, and the
    /// widget-scope `when` lines answer the HOST's states.
    for_host: bool,
    whens: Vec<AstNode>,
    line: u32,
}

/// What a widget instance brings for the body's slots: bare children for
/// the default slot, `slot <name> { … }` blocks by name.
#[derive(Clone, Default)]
struct Fills {
    default: Vec<AstItem>,
    named: HashMap<String, Vec<AstItem>>,
}

struct WhenTmp {
    state: State,
    props: Vec<Prop>,
    /// Descendant responses (`when held face.background=…`): (node name, prop).
    descendants: Vec<(String, Prop)>,
    /// A `for=` widget's scope response: watches the hosting control.
    host: bool,
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
/// A `for=` widget response whose host is found once the tree exists.
const HOST_PENDING: NodeId = u32::MAX - 2;

/// Does a widget body flag a `signals` node anywhere?
fn has_signals_node(n: &AstNode) -> bool {
    n.flags.iter().any(|f| f == "signals") || n.children.iter().any(|c| matches!(c, AstItem::Node(k) if has_signals_node(k)))
}

/// `face.background` → (`face`, `background`); a motion key like
/// `translate.x` stays whole, and `dot.translate.x` splits once.
fn split_target(key: &str) -> Option<(&str, &str)> {
    if PropKey::from_word(key).is_some() {
        return None;
    }
    let (node, rest) = key.split_once('.')?;
    Some((node, rest))
}

pub struct Compiler<'a> {
    page: Page,
    diags: Vec<Diagnostic>,
    loader: Loader<'a>,
    options: CompileOptions<'a>,
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
    /// `anim` declarations, compiled after the tree exists (they name
    /// nodes): (the widget-instance scope they were declared in — `""` at
    /// the top of the file — and the declaration).
    anim_decls: Vec<(String, AstNode)>,
    root_seen: bool,
    names: std::collections::HashSet<String>,
    /// `uses` paths already brought in (a diamond import is one import).
    used: std::collections::HashSet<String>,
    /// Widget instances being expanded, outermost first: body names are
    /// per instance (`inst/name`), so a widget used twice never collides.
    prefix: Vec<String>,
    unnamed_instances: u32,
    fills: Vec<Fills>,
    /// Per widget instance being expanded: the instance's `on:` props still
    /// waiting for the body's `signals` node (`None` = the root took them).
    instance_on: Vec<Option<Vec<AstProp>>>,
}

pub struct Compiled {
    pub page: Page,
    pub diags: Vec<Diagnostic>,
}

/// `compile` with no host options (no code controls).
pub fn compile(source: &str, loader: Loader<'_>) -> Compiled {
    compile_with(source, loader, CompileOptions::default())
}

pub fn compile_with<'a>(source: &str, loader: Loader<'a>, options: CompileOptions<'a>) -> Compiled {
    let mut diags = Vec::new();
    let doc = parse(source, &mut diags);
    let mut c = Compiler {
        page: Page::default(),
        diags,
        loader,
        options,
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
        used: std::collections::HashSet::new(),
        prefix: Vec::new(),
        unnamed_instances: 0,
        fills: Vec::new(),
        instance_on: Vec::new(),
    };
    c.page.intern("");
    c.document(&doc, true);
    c.finish();
    Compiled { page: c.page, diags: c.diags }
}

/// The bare color words: `page::PALETTE` plus its aliases and `transparent`.
fn default_palette() -> HashMap<String, Rgba> {
    let mut p = HashMap::new();
    for (name, _) in PALETTE {
        p.insert(name.to_string(), palette_color(name).expect("palette word"));
    }
    for (alias, _) in PALETTE_ALIASES {
        p.insert(alias.to_string(), palette_color(alias).expect("palette alias"));
    }
    p.insert("transparent".to_string(), Rgba::TRANSPARENT);
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
        // Pass 0: arrangements, so a `transition a -> b` may sit above its
        // `arrangement` lines (files written that way exist).
        for item in &doc.items {
            if let AstItem::Node(n) = item {
                if n.type_word == "arrangement" {
                    self.arrangement(n);
                }
            }
        }
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
                    "widget" | "control" => self.widget(n),
                    "anim" => self.anim(n),
                    "arrangement" => {}
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
                if matches!(n.type_word.as_str(), "uses" | "theme" | "style" | "widget" | "control" | "anim" | "arrangement" | "transition" | "when") {
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
        if !self.used.insert(path.to_string()) {
            return;
        }
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
        // A `control` body holds its behavior lines (`script`, `state`,
        // `state_builtin`, `signal`, a default `transition`) beside the root;
        // with no code-behind here they carry nothing for the picture.
        let bodies: Vec<&AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| !matches!(b.type_word.as_str(), "script" | "state" | "state_builtin" | "signal" | "transition" | "when" | "anim" | "style")).collect();
        if bodies.len() != 1 {
            self.error("G0028", n.line, format!("widget `{name}` needs exactly one root node in its body"));
            return;
        }
        let whens: Vec<AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "when").cloned().collect();
        let anims: Vec<AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "anim").cloned().collect();
        let for_host = n.props.iter().any(|p| p.key == "for");
        self.widgets.insert(name, WidgetDef { params: n.params.clone(), body: bodies[0].clone(), anims, for_host, whens, line: n.line });
    }

    /// `easing=name` plus its parameter words on the same line
    /// (`bounces=1, bounciness=2.4`) as one interned word the runtime
    /// parses back (`bounce_out(1,2.4)`); `None` = no `easing=` here. A
    /// parameter the curve does not take, or an unknown name, is an error.
    fn easing_spec(&mut self, props: &[AstProp], line: u32) -> Option<StrId> {
        let word = props.iter().find(|p| p.key == "easing")?.value.as_ident().map(str::to_string);
        let Some(word) = word else {
            self.error("G0065", line, "`easing=` takes a name like `cubic_out`");
            return None;
        };
        let Some(mut easing) = Easing::parse(&word) else {
            self.error("G0065", line, format!("unknown easing `{word}`"));
            return None;
        };
        let takes: &[&str] = PARAMS_FOR_CURVE.iter().find(|(c, _)| *c == easing.curve).map(|(_, p)| *p).unwrap_or(&[]);
        for p in props.iter().filter(|p| EASING_PARAM_KEYS.contains(&p.key.as_str())) {
            match takes.iter().position(|k| *k == p.key) {
                Some(i) => match p.value.as_num() {
                    Some(v) => easing.params[i] = Some(v),
                    None => self.error("G0065", p.line, format!("`{}=` takes a number", p.key)),
                },
                None => self.error("G0065", p.line, format!("`{word}` takes no `{}=` (its parameters: {})", p.key, if takes.is_empty() { "none".to_string() } else { takes.join(", ") })),
            }
        }
        Some(self.intern(&easing.to_word()))
    }

    fn anim(&mut self, n: &AstNode) {
        let Some(name) = n.name.clone() else {
            self.error("G0029", n.line, "`anim` needs a name");
            return;
        };
        let scope = self.prefix.join("/");
        if self.anim_decls.iter().any(|(s, a)| *s == scope && a.name.as_deref() == Some(&name)) {
            self.error("G0030", n.line, format!("anim `{name}` is defined twice"));
        }
        self.anim_decls.push((scope, n.clone()));
    }

    /// The anim a `play(x)` names from here: the enclosing instance's own
    /// `x` when its widget declared one, else the page's `x`.
    fn anim_ref(&mut self, name: &str) -> StrId {
        for depth in (1..=self.prefix.len()).rev() {
            let scope = self.prefix[..depth].join("/");
            if self.anim_decls.iter().any(|(s, a)| *s == scope && a.name.as_deref() == Some(name)) {
                return self.intern(&format!("{scope}/{name}"));
            }
        }
        self.intern(name)
    }

    /// A header word (`duration_each`, `easing`) from the nearest scope.
    fn anim_num(scopes: &[&AstNode], key: &str) -> Option<f32> {
        scopes.iter().rev().find_map(|s| s.props.iter().find(|p| p.key == key).and_then(|p| p.value.as_num()))
    }

    /// Compiles one anim into flat tracks; returns the end time of the
    /// chain. `scope` = the widget instance the anim belongs to (`""` for a
    /// page anim): `rope.opacity` means that instance's `rope`, `self.`
    /// the instance's root.
    fn anim_chain(&mut self, anim: StrId, scope: &str, items: &[AstItem], scopes: &[&AstNode], chain_start: f32) -> f32 {
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
                    let e = self.anim_chain(anim, scope, &step.children, &scopes, cluster_start);
                    let _ = target;
                    e
                }
                "delay" => {
                    let d = step.props.iter().find(|p| p.key == "delay").and_then(|p| p.value.as_num()).unwrap_or(0.0);
                    cluster_start + d
                }
                "each" => {
                    self.warning("G0062", step.line, "`each` steps are not supported yet — skipped");
                    cluster_start
                }
                "play" | "key" => {
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
                    let found = if node_name == "self" {
                        if scope.is_empty() { None } else { self.page.find(scope) }
                    } else if scope.is_empty() {
                        self.page.find(&node_name)
                    } else {
                        self.page.find(&format!("{scope}/{node_name}")).or_else(|| self.page.find(&node_name))
                    };
                    let Some(node) = found else {
                        self.error("G0060", step.line, format!("`{path}`: no node named `{node_name}`{}", if node_name == "self" { " (`self` is the widget's root, inside a widget's own anim)" } else { "" }));
                        continue;
                    };
                    let num = |key: &str| step.props.iter().find(|p| p.key == key).and_then(|p| p.value.as_num());
                    let channel_id = self.intern(&channel);
                    let keys: Vec<&AstNode> = step.children.iter().filter_map(|i| if let AstItem::Node(k) = i { Some(k) } else { None }).filter(|k| k.type_word == "key").collect();
                    scopes.push(step);
                    let autoreverse = scopes.iter().rev().find_map(|s| s.props.iter().find(|p| p.key == "autoreverse").map(|p| p.value == AstValue::Bool(true))).unwrap_or(false);
                    if !keys.is_empty() {
                        // `key t=, v=, easing=, jump=true` children own the
                        // timeline: one track per segment between keys. A
                        // `jump` key HOLDS the previous value until its own
                        // `t`, then snaps; a first key past t=0 glides from
                        // the live value; a first key at t=0 is pinned from
                        // play start.
                        let key_num = |k: &AstNode, key: &str| k.props.iter().find(|p| p.key == key).and_then(|p| p.value.as_num());
                        let mut parsed: Vec<(f32, f32, bool, StrId)> = Vec::new();
                        for k in &keys {
                            let t = key_num(k, "t").unwrap_or(0.0);
                            let v = key_num(k, "v").unwrap_or(0.0);
                            let jump = k.props.iter().any(|p| p.key == "jump" && p.value == AstValue::Bool(true));
                            let easing = if k.props.iter().any(|p| p.key == "easing") { self.easing_spec(&k.props, k.line).unwrap_or(NO_STR) } else { NO_STR };
                            parsed.push((t, v, jump, easing));
                        }
                        let mut prev: Option<(f32, f32)> = None;
                        let mut end = cluster_start;
                        for (t, v, jump, easing) in parsed.iter().copied() {
                            let (from, start, duration) = match prev {
                                Some((pt, _)) => (None, if jump { cluster_start + t } else { cluster_start + pt }, if jump { 0.0 } else { t - pt }),
                                None if t <= 0.0 => (Some(v), cluster_start, 0.0),
                                None => (None, if jump { cluster_start + t } else { cluster_start }, if jump { 0.0 } else { t }),
                            };
                            self.page.anims.push(AnimTrack { anim, node, channel: channel_id, from, to: v, start, duration, easing, current: false, autoreverse: false });
                            end = start + duration;
                            prev = Some((t, v));
                        }
                        if autoreverse {
                            // Out and back: the keys mirrored after the last
                            // one, each return segment with its outbound easing.
                            let lane = parsed.last().map(|k| k.0).unwrap_or(0.0);
                            if parsed.first().map(|k| k.0 > 0.0).unwrap_or(true) {
                                self.error("G0062", step.line, "`autoreverse=true` on a keyed line needs a key at t=0 to return to");
                            }
                            for i in (0..parsed.len().saturating_sub(1)).rev() {
                                let (t0, v0, _, _) = parsed[i];
                                let (t1, _, jump, easing) = parsed[i + 1];
                                let (start, duration) = if jump { (cluster_start + 2.0 * lane - t0, 0.0) } else { (cluster_start + 2.0 * lane - t1, t1 - t0) };
                                self.page.anims.push(AnimTrack { anim, node, channel: channel_id, from: None, to: v0, start, duration, easing, current: false, autoreverse: false });
                                end = end.max(start + duration);
                            }
                        }
                        cluster_end = cluster_end.max(end);
                        continue;
                    }
                    let (from, current) = match step.props.iter().find(|p| p.key == "from") {
                        Some(p) if p.value.as_ident() == Some("current") => (None, true),
                        Some(p) => (p.value.as_num(), false),
                        None => (None, false),
                    };
                    let Some(to) = num("to") else {
                        self.error("G0064", step.line, format!("`{path}` needs `to=`"));
                        continue;
                    };
                    let duration = num("duration").or_else(|| Self::anim_num(&scopes, "duration_each")).unwrap_or(0.0);
                    // The nearest scope that names an easing supplies it, with
                    // the parameter words written beside it there.
                    let easing = match scopes.iter().rev().find(|s| s.props.iter().any(|p| p.key == "easing")) {
                        Some(scope) => {
                            let props = scope.props.clone();
                            self.easing_spec(&props, step.line).unwrap_or(NO_STR)
                        }
                        None => NO_STR,
                    };
                    let offset = num("offset").unwrap_or(0.0);
                    let start = (cluster_start + offset).max(0.0);
                    self.page.anims.push(AnimTrack { anim, node, channel: channel_id, from, to, start, duration, easing, current, autoreverse });
                    start + if autoreverse { 2.0 * duration } else { duration }
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
                "easing" => t.easing = self.easing_spec(&n.props, p.line).unwrap_or(NO_STR),
                "anim" => t.anim = self.intern(p.value.as_ident().unwrap_or("")),
                other if EASING_PARAM_KEYS.contains(&other) => {}
                other => self.error("G0034", p.line, format!("`transition` takes duration=, easing= (+ its parameters), anim= — not `{other}`")),
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
                let name = &word["screen.".len()..];
                if SCREEN_ORIENTATIONS.contains(&name) {
                    let id = self.intern(name);
                    State::ScreenOrientation(id)
                } else if SCREEN_CLASSES.contains(&name) {
                    let id = self.intern(name);
                    State::ScreenClass(id)
                } else {
                    self.error("G0036", line, format!("unknown screen word `{word}` — the classes are {}; the orientations are {}", SCREEN_CLASSES.join(", "), SCREEN_ORIENTATIONS.join(", ")));
                    return None;
                }
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

    /// The condition of a `when`: one word, or a composed pair
    /// (`when large screen.lotsOfSpace`, `when screen.landscape
    /// screen.lotsOfSpace`) that holds only while BOTH do.
    fn state_word(&mut self, n: &AstNode, top_level: bool) -> Option<State> {
        let Some(word) = n.name.clone() else {
            self.error("G0037", n.line, "`when` needs a state");
            return None;
        };
        let first = self.state_from_word(&word, n.line, top_level)?;
        let second = n.args.iter().find_map(|a| a.as_ident()).map(str::to_string);
        let Some(second) = second else { return Some(first) };
        let second_state = self.state_from_word(&second, n.line, top_level)?;
        match (first, second_state) {
            (State::Arrangement(a), State::ScreenClass(w)) | (State::Arrangement(a), State::ScreenOrientation(w)) => Some(State::ArrangementScreen(a, w)),
            (State::ScreenOrientation(o), State::ScreenClass(c)) => Some(State::ScreenPair(o, c)),
            _ => {
                self.error("G0038", n.line, format!("`when {word} {second}`: a composed condition is `<arrangement> screen.<class|orientation>` or `screen.<orientation> screen.<class>`"));
                None
            }
        }
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
                // Asset paths carry gogo's `res://` scheme, always: the
                // prefix says in the file itself "resolved against the
                // build's resource root and embedded" — a bare spelling
                // would be a second way to write the same thing.
                let lower = text.to_ascii_lowercase();
                if !text.starts_with("res://") && IMAGE_EXTS.iter().chain(FONT_EXTS).any(|e| lower.ends_with(e)) {
                    self.error("G0071", line, format!("`{text}` names an asset — write `res://{text}` (every asset path carries the scheme)"));
                }
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
            AstValue::Call { name, args } if (name == "live" || name == "round") && !args.is_empty() => {
                // `live($value)` — a control prop read; static here (no
                // code-behind writes it), so the value itself.
                self.value(&args[0], line, key)
            }
            AstValue::Range(a, _) => {
                self.error("G0067", line, "`a..b` is only legal after `in` on a slot line");
                self.value(a, line, key)
            }
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
                self.error("G0041", p.line, "`texture=` is `source=\"<image path>\"` (checked and embedded at build)");
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
        let mut value = self.value(&p.value, p.line, Some(key));
        // `text=25` / `text=live($value)` with a number: the digits, so a
        // value label needs no code to show its number.
        if key == PropKey::Text {
            if let Value::VNum(n) = value {
                let digits: String = if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") };
                value = Value::VText { text: self.intern(&digits), key: NO_STR };
            }
        }
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
            let mut fills = Fills::default();
            for c in &n.children {
                match c {
                    AstItem::Node(k) if k.type_word == "slot" && k.name.is_some() => {
                        fills.named.insert(k.name.clone().unwrap_or_default(), k.children.clone());
                    }
                    AstItem::Node(k) if matches!(k.type_word.as_str(), "when" | "transition") => {}
                    other => fills.default.push(other.clone()),
                }
            }
            self.fills.push(fills);
            let prefix = match &n.name {
                Some(name) => name.clone(),
                None => {
                    self.unnamed_instances += 1;
                    format!("#{}", self.unnamed_instances)
                }
            };
            self.prefix.push(prefix);
            // The widget's own anims, one copy per instance.
            for a in &def.anims {
                self.anim(a);
            }
            // `signals` inside the body: the instance's `on:` wiring binds
            // to that node, not the root (the Slider inside LabeledSlider).
            let on_props: Vec<AstProp> = n.props.iter().filter(|p| p.key.starts_with("on:")).cloned().collect();
            self.instance_on.push(if has_signals_node(&def.body) { Some(on_props) } else { None });
            let mut t = self.node(&def.body, parent_kind, Some(n));
            self.instance_on.pop();
            // A `for=` widget's scope responses answer the hosting control.
            for w in &def.whens {
                if let Some(mut tw) = self.when(w, t.node.kind) {
                    tw.host = def.for_host;
                    t.whens.push(tw);
                }
            }
            self.prefix.pop();
            self.fills.pop();
            self.scopes.pop();
            let _ = def.line;
            return t;
        }

        // `layout Flow { … }` inside a List: the items' container, spelled
        // by the word after `layout`.
        let type_word: String = if n.type_word == "layout" { n.name.clone().unwrap_or_default() } else { n.type_word.clone() };
        let n_layout: AstNode;
        let n: &AstNode = if n.type_word == "layout" {
            n_layout = AstNode { type_word: type_word.clone(), name: None, ..n.clone() };
            &n_layout
        } else {
            n
        };
        let kind = match Kind::from_word(&n.type_word) {
            Some(k) => k,
            None if self.options.code_controls.contains(&n.type_word.as_str()) => {
                // A control whose look comes from the host's code: an empty
                // box of its declared size, so the page still lays out.
                self.warning("G7013", n.line, format!("`{}` is drawn by code — an empty box here", n.type_word));
                Kind::Custom
            }
            None => {
                self.error("G7011", n.line, format!("unknown node type `{}`", n.type_word));
                Kind::Custom
            }
        };
        if kind == Kind::View && parent_kind.is_some() {
            self.error("G7012", n.line, "`View` is the screen root only — inside the tree use `Layers` (free placement) or `Panel` (a styled box)");
        }
        let signals = n.flags.iter().any(|f| f == "signals");
        for f in &n.flags {
            if f != "signals" {
                self.error("G0043", n.line, format!("unknown flag `{f}`"));
            }
        }
        let mut node = Node { kind, line: n.line, ..Node::EMPTY };
        node.type_name = self.intern(&n.type_word);
        // The instance's name wins over the widget body's; body names are
        // scoped per instance (`inst/name`).
        let name = match instance.and_then(|i| i.name.clone()) {
            Some(name) => Some(self.scoped_name(&name, true)),
            None => n.name.clone().map(|nm| self.scoped_name(&nm, false)),
        };
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
            let root_takes_on = matches!(self.instance_on.last(), None | Some(None));
            all_props.extend(i.props.iter().filter(|p| !params.contains(&p.key) && (root_takes_on || !p.key.starts_with("on:"))).cloned());
        }
        if signals {
            if let Some(slot) = self.instance_on.last_mut() {
                if let Some(props) = slot.take() {
                    all_props.extend(props);
                }
            }
        }
        // Positional size arg: `Button 44x44`.
        for a in n.args.iter().chain(instance.iter().flat_map(|i| i.args.iter())) {
            if let AstValue::Size { .. } = a {
                all_props.push(AstProp { key: "size".to_string(), value: a.clone(), line: n.line });
            } else if kind != Kind::Gap && kind != Kind::Repeat {
                self.error("G0046", n.line, format!("unexpected value {a:?} on `{}`", n.type_word));
            }
        }
        if !matches!(kind, Kind::Slot | Kind::Repeat) {
            for (k, v) in &n.clauses {
                all_props.push(AstProp { key: k.clone(), value: v.clone(), line: n.line });
            }
        }
        if kind == Kind::Slot {
            tmp.node.auto = self.automatic(n);
        }

        let mut placement_props: Vec<Prop> = Vec::new();
        for p in &all_props {
            if kind == Kind::Repeat && p.key == "max" {
                continue;
            }
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
        // Start states the file declares; the Runtime seeds from them.
        for prop in &tmp.props {
            match (prop.key, prop.value) {
                (PropKey::SelectedProp, Value::VBool(true)) => tmp.node.initial_states |= SELECTED,
                (PropKey::Disabled, Value::VBool(true)) => tmp.node.initial_states |= DISABLED,
                _ => {}
            }
        }
        // A wrap label whose width never binds (G1004, the spec's lint):
        // hug measures unwrapped, so the text stays one line.
        if kind == Kind::Label && tmp.props.iter().any(|p| p.key == PropKey::Wrap && p.value == Value::VBool(true)) {
            let pl = &tmp.node.placement;
            if matches!(pl.width, Len::Hug(_)) && pl.max_width.is_none() && !pl.max_width_fill {
                self.warning("G1004", n.line, "`wrap=true` with `width=hug` and no `max_width` never wraps — the text measures unwrapped (D254); give it `width=fill` or a `max_width`");
            }
        }

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
                AstItem::Node(c) if c.type_word == "slot" => self.slot_child(c, kind, &mut tmp.children),
                // `choice { Radio … }` has no node of its own: its children
                // parent flat here, and every Radio inside (however deep
                // through Rows/Columns) joins ONE group — the group is what
                // reports the pick, to `on:change=` or a handler named
                // after the choice.
                AstItem::Node(c) if c.type_word == "choice" => {
                    let start: Option<String> = c.props.iter().find(|p| p.key == "value").and_then(|p| p.value.as_ident().map(str::to_string));
                    let mut action: Option<Action> = None;
                    for p in &c.props {
                        match p.key.as_str() {
                            "value" => {}
                            "on:change" => action = self.handler("change", p).map(|h| h.action),
                            "on:changed" => self.error("G0070", p.line, "`on:changed` is spelled `on:change` — changed means CHANGED: one report per genuine transition"),
                            other => self.error("G0070", p.line, format!("`{other}` does not belong on a choice line — `choice <name> value=<radioName> on:change=… {{ … }}`; looks and sizes go on the Radios")),
                        }
                    }
                    let name = match &c.name {
                        Some(n) => self.intern(n),
                        None => NO_STR,
                    };
                    if action.is_none() && name != NO_STR {
                        action = Some(Action::Named(name));
                    }
                    let index = self.page.choices.len() as u32;
                    self.page.choices.push(ChoiceGroup { name, action });
                    let mut found = start.is_none();
                    let start_id = match &start {
                        Some(name) => self.intern(name),
                        None => NO_STR,
                    };
                    let mut radios = 0u32;
                    for item in &c.children {
                        if let AstItem::Node(k) = item {
                            if k.type_word == "choice" {
                                self.error("G0070", k.line, "choice scopes don't nest — one exclusive set at a time");
                                continue;
                            }
                            if k.props.iter().any(|p| p.key == "selected") {
                                self.error("G0068", k.line, "`selected=` on a Radio inside `choice` — the group's `value=<name>` picks the start");
                            }
                            let mut child = self.node(k, Some(kind), None);
                            Self::join_choice(&mut child, index, start_id, &mut radios, &mut found);
                            tmp.children.push(child);
                        }
                    }
                    if radios == 0 {
                        self.error("G0069", c.line, "choice holds no Radio — nothing to be exclusive about (nested `Row`/`Column` layout is fine)");
                    }
                    if !found {
                        self.error("G0069", c.line, format!("`choice value={}`: no Radio of that name in the group", start.unwrap_or_default()));
                    }
                }
                // `repeat x max=N { … }`: the template is built N times at
                // compile (`x/0`, `x/1`, …), every copy `gone` until code
                // sets the count (`Runtime::set_count`); with no `max=` the
                // template needs items from code and only `empty { }` shows.
                AstItem::Node(c) if kind == Kind::Repeat && c.type_word != "empty" => {
                    let Some(max) = n.props.iter().find(|p| p.key == "max").and_then(|p| p.value.as_num()) else { continue };
                    let repeat_name = n.name.clone().unwrap_or_else(|| "repeat".to_string());
                    let gone = self.intern("gone");
                    for i in 0..(max.max(0.0) as usize) {
                        self.prefix.push(format!("{repeat_name}/{i}"));
                        // A widget item with no instance name is `item`, so its
                        // parts are addressable (`x/0/item/knob`).
                        let named: AstNode;
                        let template: &AstNode = if c.name.is_none() && self.widgets.contains_key(&c.type_word) {
                            named = AstNode { name: Some("item".to_string()), ..c.clone() };
                            &named
                        } else {
                            c
                        };
                        let mut child = self.node(template, Some(kind), None);
                        self.prefix.pop();
                        child.props.push(Prop { key: PropKey::Visibility, value: Value::VIdent(gone) });
                        tmp.children.push(child);
                    }
                }
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

    /// `inst/name` inside a widget body (the instance's own name sits one
    /// scope out).
    fn scoped_name(&self, name: &str, is_instance: bool) -> String {
        let depth = if is_instance { self.prefix.len().saturating_sub(1) } else { self.prefix.len() };
        if depth == 0 {
            return name.to_string();
        }
        format!("{}/{name}", self.prefix[..depth].join("/"))
    }

    /// `automatic=<kind> by $value in $min..$max` on a slot line, solved
    /// to a fraction now: nothing writes `$value` at runtime here.
    fn automatic(&mut self, n: &AstNode) -> Auto {
        let Some(kind_prop) = n.props.iter().find(|p| p.key == "automatic") else { return Auto::NoAuto };
        let kind = kind_prop.value.as_ident().unwrap_or("").to_string();
        let by = n.clauses.iter().find(|(k, _)| k == "by").map(|(_, v)| v.clone());
        let range = n.clauses.iter().find(|(k, _)| k == "in").map(|(_, v)| v.clone());
        let num = |c: &mut Self, v: &AstValue| -> f32 {
            match c.value(v, n.line, None) {
                Value::VNum(x) => x,
                _ => 0.0,
            }
        };
        match kind.as_str() {
            "length" | "position" => {
                let (Some(by), Some(AstValue::Range(lo, hi))) = (by, range) else {
                    self.error("G5104", n.line, format!("`automatic={kind}` wants `by $value in $min..$max`"));
                    return Auto::NoAuto;
                };
                let v = num(self, &by);
                let lo = num(self, &lo);
                let hi = num(self, &hi);
                let f = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
                if kind == "length" { Auto::Length(f) } else { Auto::Position(f) }
            }
            "presence" => Auto::Presence(false),
            other => {
                self.error("G5104", n.line, format!("`automatic={other}`: want length, position or presence"));
                Auto::NoAuto
            }
        }
    }

    /// Compiles with the innermost widget scope set aside: an instance's
    /// fill content is written one scope out.
    fn outer<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let scope = self.scopes.pop();
        let prefix = self.prefix.pop();
        let fills = self.fills.pop();
        let r = f(self);
        if let Some(s) = scope {
            self.scopes.push(s);
        }
        if let Some(p) = prefix {
            self.prefix.push(p);
        }
        if let Some(f) = fills {
            self.fills.push(f);
        }
        r
    }

    /// A `slot` line inside a widget body: the default slot splices the
    /// instance's bare children in place (else its own default content); a
    /// named region is a real clipped box showing the fill or its default.
    fn slot_child(&mut self, c: &AstNode, parent_kind: Kind, out: &mut Vec<TmpNode>) {
        let fills = self.fills.last().cloned();
        let is_default = c.name.is_none() && c.props.is_empty() && c.clauses.is_empty();
        if is_default {
            match fills.filter(|f| !f.default.is_empty()) {
                Some(f) => {
                    for item in &f.default {
                        if let AstItem::Node(k) = item {
                            let child = self.outer(|me| me.node(k, Some(parent_kind), None));
                            out.push(child);
                        }
                    }
                }
                None => {
                    for item in &c.children {
                        if let AstItem::Node(k) = item {
                            let child = self.node(k, Some(parent_kind), None);
                            out.push(child);
                        }
                    }
                }
            }
            return;
        }
        let fill = fills.and_then(|f| c.name.as_ref().and_then(|n| f.named.get(n).cloned()));
        match fill {
            Some(items) => {
                let mut region = c.clone();
                region.children.clear();
                let mut tmp = self.node(&region, Some(parent_kind), None);
                for item in &items {
                    if let AstItem::Node(k) = item {
                        let child = self.outer(|me| me.node(k, Some(Kind::Slot), None));
                        tmp.children.push(child);
                    }
                }
                out.push(tmp);
            }
            None => {
                let tmp = self.node(c, Some(parent_kind), None);
                out.push(tmp);
            }
        }
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
                "button_down" => "press",
                "button_up" | "released" => "release",
                "toggled" | "changed" | "value_changed" | "text_submitted" => "change",
                "changing" | "text_changed" | "input" => "edit",
                _ => "",
            };
            if hint.is_empty() {
                self.error("G0049", p.line, format!("`on:{event}` — raw signals are gone (§7); the events are activate, press, release, change, edit, show, hide, touch"));
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
                    "play" => Action::Play(self.anim_ref(arg)),
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

    /// Every Radio in a choice's subtree joins its group (however deep
    /// through Rows/Columns); the one named by `value=` starts selected.
    fn join_choice(t: &mut TmpNode, index: u32, start: StrId, radios: &mut u32, found: &mut bool) {
        if t.node.kind == Kind::Radio {
            t.node.choice = index;
            *radios += 1;
            if start != NO_STR && t.node.name == start {
                t.node.initial_states |= SELECTED;
                *found = true;
            }
        }
        for c in &mut t.children {
            Self::join_choice(c, index, start, radios, found);
        }
    }

    fn when(&mut self, w: &AstNode, kind: Kind) -> Option<WhenTmp> {
        let state = self.state_word(w, false)?;
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
            if let Some((node_name, prop_name)) = split_target(&p.key) {
                let ap = AstProp { key: prop_name.to_string(), value: p.value.clone(), line: p.line };
                if let Some(prop) = self.prop(&ap, Kind::Custom) {
                    descendants.push((node_name.to_string(), prop));
                }
            } else if let Some(prop) = self.prop(p, kind) {
                props.push(prop);
            }
        }
        Some(WhenTmp { state, props, descendants, host: false })
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

    /// One margin/padding edge: a number, or `safe` (the host's inset for
    /// that edge, read at layout time).
    fn edge(&mut self, v: Value, line: u32, key: &str, p: &mut Placement, i: usize, margin: bool) {
        if let Value::VIdent(id) = v {
            if self.page.str(id) == "safe" {
                if margin {
                    p.margin[i] = Some(0.0);
                    p.margin_safe[i] = true;
                } else {
                    p.padding_safe[i] = true;
                }
                return;
            }
        }
        let n = self.num(v, line, key);
        if margin {
            p.margin[i] = Some(n);
        } else {
            p.padding[i] = n;
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
                PropKey::MarginLeft => self.edge(v, line, "margin_left", &mut p, 0, true),
                PropKey::MarginTop => self.edge(v, line, "margin_top", &mut p, 1, true),
                PropKey::MarginRight => self.edge(v, line, "margin_right", &mut p, 2, true),
                PropKey::MarginBottom => self.edge(v, line, "margin_bottom", &mut p, 3, true),
                PropKey::Margin => match v {
                    Value::VIdent(id) if self.page.str(id) == "safe" => {
                        p.margin = [Some(0.0); 4];
                        p.margin_safe = [true; 4];
                    }
                    _ => {
                        let m = self.num(v, line, "margin");
                        p.margin = [Some(m); 4];
                    }
                },
                PropKey::PaddingLeft => self.edge(v, line, "padding_left", &mut p, 0, false),
                PropKey::PaddingTop => self.edge(v, line, "padding_top", &mut p, 1, false),
                PropKey::PaddingRight => self.edge(v, line, "padding_right", &mut p, 2, false),
                PropKey::PaddingBottom => self.edge(v, line, "padding_bottom", &mut p, 3, false),
                PropKey::Padding => match v {
                    Value::VIdent(id) if self.page.str(id) == "safe" => p.padding_safe = [true; 4],
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
        for (scope, n) in &decls {
            let name = n.name.clone().unwrap_or_default();
            let full = if scope.is_empty() { name.clone() } else { format!("{scope}/{name}") };
            let anim = self.intern(&full);
            let mut head = AnimHead { anim, repeat: false, speed: 1.0 };
            for p in &n.props {
                match p.key.as_str() {
                    "repeat" => match p.value.as_ident() {
                        Some("forever") => head.repeat = true,
                        _ => self.error("G0066", p.line, "`repeat=` takes `forever` — a one-shot anim just omits it"),
                    },
                    "speed" => match p.value.as_num() {
                        Some(v) if v > 0.0 => head.speed = v,
                        _ => self.error("G0066", p.line, "`speed=` takes a number above 0"),
                    },
                    "duration" => self.error("G0066", p.line, "`duration=` on an anim header reads as the chain's total — write `duration_each=`"),
                    "delay" => self.error("G0066", p.line, "`delay=` was retired — a pause in the chain is its own step (`then delay=0.3`), a shift is `offset=`"),
                    "duration_each" | "easing" | "autoreverse" => {}
                    other if EASING_PARAM_KEYS.contains(&other) => {}
                    other => self.error("G0066", p.line, format!("an anim header takes `repeat=forever`, `speed=`, `duration_each=`, `easing=` (+ its parameters), `autoreverse=` — not `{other}`")),
                }
            }
            if n.props.iter().any(|p| p.key == "easing") {
                let props = n.props.clone();
                self.easing_spec(&props, n.line);
            }
            self.page.anim_heads.push(head);
            let scopes = vec![n];
            self.anim_chain(anim, scope, &n.children, &scopes, 0.0);
        }
        // Top-level `when <arrangement> { node.prop = value }`.
        let deferred = std::mem::take(&mut self.deferred_whens);
        for (state, items, line) in deferred {
            for item in &items {
                let AstItem::Assign { key, dollar: false, value, line: l } = item else {
                    self.error("G0054", line, "a top-level `when` block holds `node.prop = value` lines");
                    continue;
                };
                let Some((node_name, prop_name)) = split_target(key) else {
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
                        None => self.node_whens[id as usize].push(WhenTmp { state, props: vec![prop], descendants: Vec::new(), host: false }),
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
                    // Widget-body names carry their instance prefix (`inst/name`).
                    let target = (owner + 1..end).find(|i| {
                        let full = if self.page.nodes[*i].name != NO_STR { self.page.str(self.page.nodes[*i].name) } else { "" };
                        full == name || full.rsplit('/').next() == Some(name.as_str())
                    });
                    match target {
                        Some(t) => moved.push((t, WhenTmp { state: w.state, props: vec![prop], descendants: Vec::new(), host: w.host })),
                        None => {
                            let line = self.page.nodes[owner].line;
                            self.error("G0060", line, format!("`when … {name}.…`: no node named `{name}` under this one"));
                        }
                    }
                }
            }
        }
        let mut of: Vec<Vec<NodeId>> = whens.iter().map(|l| l.iter().map(|w| if w.host { HOST_PENDING } else { NO_NODE }).collect()).collect();
        for (target, w) in moved {
            // The owner is the node whose subtree held the response: found by walking up until the state is theirs.
            let mut owner = self.page.nodes[target].parent;
            while owner != NO_NODE && !self.page.nodes[owner as usize].kind.is_press_surface() && self.page.nodes[owner as usize].parent != NO_NODE {
                owner = self.page.nodes[owner as usize].parent;
            }
            let host = w.host;
            whens[target].push(w);
            of[target].push(if host { HOST_PENDING } else { owner });
        }
        // `for=` widget responses watch the hosting control (the nearest
        // press surface above).
        for i in 0..of.len() {
            for j in 0..of[i].len() {
                if of[i][j] == HOST_PENDING {
                    let mut a = self.page.nodes[i].parent;
                    while a != NO_NODE && !self.page.nodes[a as usize].kind.is_press_surface() {
                        a = self.page.nodes[a as usize].parent;
                    }
                    of[i][j] = a;
                }
            }
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

    /// G0071: every asset path writes the `res://` scheme — a bare
    /// `source=`, `font=` or theme-var path is refused naming the fix; a
    /// label's TEXT ending in ".png" is refused the same way (any such
    /// string reaches the embed tables). `source=""` (code-filled) passes.
    #[test]
    fn asset_paths_carry_the_res_scheme() {
        assert!(codes("View v { Image i source=\"assets/x.png\" }").contains(&"G0071".to_string()));
        assert!(codes("View v { Label l text=\"see x.png\" }").contains(&"G0071".to_string()));
        assert!(codes("theme { $f = \"fonts/a.ttf\" }\nView v { Label l text=\"hi\", font=$f }").contains(&"G0071".to_string()));
        assert!(!codes("View v { Image i source=\"res://assets/x.png\" }").contains(&"G0071".to_string()));
        assert!(!codes("View v { Image i source=\"\" }").contains(&"G0071".to_string()));
    }

    #[test]
    fn removed_words_and_code_controls() {
        assert!(codes("View v { Border b }").contains(&"G7011".to_string()));
        let code = compile_with("View v { GradientEdit g width=100, height=40 }", &|_| None, CompileOptions { code_controls: &["GradientEdit"], ..Default::default() });
        assert!(code.diags.iter().any(|d| d.code == "G7013"), "{:?}", code.diags);
        assert!(!crate::diag::has_errors(&code.diags), "{:?}", code.diags);
        assert!(codes("View v { Panel p width=host }").contains(&"G0055".to_string()));
        assert!(codes("View v { Button b on:pressed=x }").contains(&"G0049".to_string()));
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
