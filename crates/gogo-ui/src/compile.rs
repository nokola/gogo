//! AST → `Page`. Styles, `uses`, widgets and the palette are resolved
//! here (the runtime never sees them); `$theme` refs and translation keys
//! stay live. Diagnostic codes are stable G-numbers (`G0041`, `G7011` …).

use std::collections::{HashMap, HashSet};

use crate::anim::{Channel, EASING_PARAM_KEYS, Easing, PARAMS_FOR_CURVE};
use crate::ast::{AstItem, AstNode, AstProp, AstValue, Document};
use crate::diag::Diagnostic;
use crate::locale::Keys;
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
    /// The build's translation-key table, shared by every page `build_pages`
    /// compiles — keys are global (one `/ok` across files), so the id a page
    /// bakes into `VText { key }` must index the one `KEYS` table the build
    /// emits. `None` (a standalone compile, tests, hot reload of one file)
    /// = the page gets its own table, ids local to it.
    pub keys: Option<&'a std::cell::RefCell<Keys>>,
    /// The name diagnostics use for THIS file in cross-file facts (G8001
    /// names the file a key was first seen in). `compile_file_with` sets
    /// it; `""` = unknown.
    pub source_name: &'a str,
}

/// The gogo events; anything else on `on:` is a raw signal — gone (§7).
/// `press` fires on finger-down (the undo/redo feel); `release` exactly
/// once when that gesture ends, however it ends; `activate` on a
/// completed tap.
/// `pointer` hands the host the raw pointer (down/move/up/cancel with
/// positions in the node's own epx space) — a code-drawn control's input.
/// A mouse is one pointer: the shell feeds it as one fixed finger.
const EVENTS: &[&str] = &["activate", "press", "release", "change", "edit", "show", "hide", "pointer"];

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
    /// `style X for=Slider`: the one control type the style may dress.
    for_target: Option<String>,
    /// `style X extends=Y`: the parent whose props/whens come first.
    extends: Option<String>,
    line: u32,
}

#[derive(Clone)]
struct WidgetDef {
    params: Vec<AstProp>,
    body: AstNode,
    /// `anim` declarations beside the body: each instance gets its own
    /// copy, named `inst/anim`, its targets scoped to the instance.
    anims: Vec<AstNode>,
    /// `widget X for=Radio`: the body fills THAT host control, and the
    /// widget-scope `when` lines answer the HOST's states.
    host: Option<Kind>,
    /// Widget-line props (`widget X for=Radio, min_height=44`): baked
    /// onto the body root, under its own (`for=` excluded).
    props: Vec<AstProp>,
    whens: Vec<AstNode>,
    /// `transition` lines beside the body (GP32): on a `for=` widget they
    /// time the HOSTING control's flips (attached to it at lay time); on
    /// a `control` they are its default feel — endpoint-less, the weakest
    /// rule on the instance root.
    transitions: Vec<AstNode>,
    /// `style` blocks beside the body (GP32), visible only while that
    /// body builds: they shadow page styles, innermost widget first (the
    /// shipped scoping). An instance-site `style=` never sees them.
    styles: HashMap<String, StyleDef>,
    /// A `control` declaration (vs a `widget`): its own scoped anims may
    /// drive `slot.<name>.<motion word>` lanes.
    control: bool,
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
    /// The `when` block's source line — the equal-rank tie-break
    /// (`State::priority`): later line wins.
    line: u32,
    props: Vec<Prop>,
    /// Descendant responses (`when held face.background=…`): (node name,
    /// prop, the prop word as written, its line) — the word and line feed
    /// the kind check once the target is found.
    descendants: Vec<(String, Prop, String, u32)>,
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

/// An areas token that names a region (`.` marks a hole): a plain ident.
fn is_area_name(t: &str) -> bool {
    let mut chars = t.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

// ----- property words per kind (E8) --------------------------------------

/// A kind as an error names it: its word, or "code control" for a node
/// the host draws.
fn kind_name(kind: Kind) -> &'static str {
    if kind == Kind::Custom { "code control" } else { kind.word() }
}

fn parent_name(parent: Option<Kind>) -> &'static str {
    parent.map_or("the root", kind_name)
}

/// "did you mean 'X'? (or: …)" over ranked candidates, or an
/// "options:" list when nothing is close — gogo-design §7 promises
/// every unknown word a suggestion (GPAR87, the shipped ranking).
fn suggest(word: &str, candidates: &[String]) -> String {
    let mut ranked: Vec<(usize, &String)> = Vec::new();
    for c in candidates {
        if c.is_empty() || ranked.iter().any(|(_, r)| *r == c) {
            continue;
        }
        ranked.push((edit_distance(word, c), c));
    }
    if ranked.is_empty() {
        return String::new();
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    let confident = ranked[0].0 <= 2 || (word.len() >= 3 && ranked[0].1.starts_with(word));
    if confident {
        let others: Vec<&str> = ranked.iter().skip(1).take(4).map(|(_, c)| c.as_str()).collect();
        if others.is_empty() {
            format!("did you mean '{}'?", ranked[0].1)
        } else {
            format!("did you mean '{}'? (or: {})", ranked[0].1, others.join(", "))
        }
    } else {
        let options: Vec<&str> = ranked.iter().take(5).map(|(_, c)| c.as_str()).collect();
        format!("options: {}", options.join(", "))
    }
}

/// The legal word closest to a misspelling (`textt` → `text`): at most two
/// edits away, or the typed word is a prefix of it. Nothing for a word
/// that is not close to anything — a wrong guess would mislead.
fn nearest_word<'w>(word: &str, candidates: &[&'w str]) -> Option<&'w str> {
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        let d = edit_distance(word, c);
        let close = d <= 2 || (word.len() >= 3 && c.starts_with(word));
        if close && best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

/// Levenshtein distance on chars — small words, small lists, no need for
/// anything cleverer.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur: Vec<usize> = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The message for a word gogo has no key for: a known other-dialect word
/// gets its gogo spelling, a near-miss the closest legal word, anything
/// else the kind's own words.
fn unknown_property(word: &str, kind: Kind) -> String {
    let dialect: Option<&str> = match word {
        "foreground" | "text_color" | "font_colour" | "colour" => Some("text color is `font_color=`, a plate is `background=`"),
        "columns" => Some("a Grid's tracks are `cols=[…]`"),
        "rows" => Some("Grid rows are `row { }` lines (`row height=40 { … }`)"),
        "mouse_filter" => Some("input is `interactive=true` (claims it) or `interactive=false` (hit-invisible)"),
        "duration" | "easing" | "anim" | "delay" | "duration_each" => Some("timing belongs on a `transition` line (`transition any -> held duration=0.1`); a `when` holds standing values"),
        "solid_until" => Some("a plateau is spelled in `stops=` (`stops=[c@0, c@0.94, c/0@1]`)"),
        "inset" | "anchors_margin" => Some("placement is `margin=` / `halign=` / `valign=` / `width=` / `height=`"),
        "modulate" | "tint" => Some("a whole-node fade is `opacity=`; a fill is `color=` (Rectangle, Circle) or `background=`"),
        "translate" => Some("motion has no axisless translate — `translate.x=` / `translate.y=`"),
        "orientation" => Some("direction is the kind: `Column` runs down, `Row` runs across"),
        "flat" => Some("a plain button is `background=transparent`"),
        "visible" => Some("presence is `visibility=visible|space_only|gone`"),
        "horizontal_scroll_mode" | "vertical_scroll_mode" | "scroll_horizontal" | "scroll_vertical" => Some("a Scroll's pan axes are `allow=horizontal|vertical|both`"),
        _ => None,
    };
    if let Some(hint) = dialect {
        return format!("`{word}=` is not a gogo word — {hint}");
    }
    let words = kind_words(kind);
    if let Some(near) = nearest_word(word, &words) {
        return format!("`{word}=` is not a property — did you mean `{near}=`?");
    }
    let own = kind_own_words(kind);
    let name = kind_name(kind);
    if own.is_empty() {
        format!("`{word}=` is not a property — a `{name}` line takes no properties")
    } else {
        format!("`{word}=` is not a property of a {name} — its own words: {}", own.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", "))
    }
}

/// The message for a real gogo word on a kind that does not take it: says
/// whose word it is and, where one exists, the word this kind uses instead.
fn wrong_kind(key: PropKey, kind: Kind) -> String {
    let word = key.word();
    let name = kind_name(kind);
    let shows_text = legal(kind, PropKey::Text);
    let leaf = matches!(kind, Kind::Rectangle | Kind::Circle | Kind::Gradient | Kind::Image);
    match key {
        PropKey::Color if shows_text => format!("`color=` on a {name} — text is `font_color=`, the plate is `background=`"),
        PropKey::Color if kind == Kind::Gradient => "`color=` on a Gradient — a Gradient paints its `stops=`".to_string(),
        PropKey::Color | PropKey::ColorA => format!("`{word}=` is the fill of a Rectangle or Circle — a {name}'s plate is `background=`, a fade is `opacity=`"),
        PropKey::Background if kind == Kind::Gradient => "`background=` on a Gradient — a Gradient paints its `stops=`".to_string(),
        PropKey::Background => format!("`background=` on a {name} — its fill is `color=`"),
        PropKey::CornerRadius | PropKey::BorderThickness | PropKey::BorderColor | PropKey::ClipContents if kind == Kind::Circle => format!("`{word}=` on a Circle — a Circle is not a box; it has a `color=` and a ring (`border_thickness=`, `border_color=`)"),
        PropKey::CornerRadius | PropKey::BorderThickness | PropKey::BorderColor | PropKey::ClipContents => format!("`{word}=` shapes a box's plate — a {name} has none; wrap it in a Panel"),
        PropKey::Padding | PropKey::PaddingLeft | PropKey::PaddingTop | PropKey::PaddingRight | PropKey::PaddingBottom if leaf => format!("`{word}=` is air inside a box around its children or text — a {name} has neither; outer air is `margin=`"),
        PropKey::Allow | PropKey::Bars => format!("`{word}=` picks a Scroll's pan axes — a {name} does not scroll"),
        PropKey::Cols | PropKey::RowHeight => format!("`{word}=` lays out a Grid or Tiles — a {name} has no tracks"),
        PropKey::Spacing => format!("`spacing=` separates a stack's children (Column, Row, Flow, Grid, Tiles, List) — a {name} stacks nothing"),
        PropKey::Text | PropKey::FontSize | PropKey::FontColor | PropKey::Wrap | PropKey::ClipText | PropKey::TextHalign | PropKey::TextValign => format!("`{word}=` belongs to a box that shows text (Label, Button, Check, Toggle, Radio, Input) — a {name} shows none"),
        PropKey::Source | PropKey::Fit | PropKey::Loading => format!("`{word}=` is an Image word — a {name} shows no picture"),
        PropKey::Stops | PropKey::GradientFill | PropKey::Angle => format!("`{word}=` is a Gradient word — a {name}'s one color is `background=`"),
        PropKey::Value | PropKey::Min | PropKey::Max | PropKey::Step => format!("`{word}=` belongs to a Slider, Input, Toggle or choice — a {name} holds no value"),
        PropKey::SelectedProp | PropKey::ToggleMode | PropKey::TapPulse | PropKey::IconSrc => format!("`{word}=` belongs to a press surface (Button, Check, Toggle, Radio, ListItem) — a {name} is not pressed"),
        PropKey::Select => format!("`select=` is a List word — a {name} selects nothing"),
        PropKey::Automatic => format!("`automatic=` is a slot word — a {name} is not a slot"),
        PropKey::DisabledOpacity => "`disabled_opacity=` is page-wide — write it in the top `theme { }` block".to_string(),
        _ => {
            let own = kind_own_words(kind);
            if own.is_empty() {
                format!("`{word}=` on a `{name}` line — it takes no properties")
            } else {
                format!("`{word}=` is not a {name} word — its own words: {}", own.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", "))
            }
        }
    }
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
    /// Widget-scoped styles, alive while that widget's body builds
    /// (innermost last). Lookups walk this before the page styles, so a
    /// scoped `Row` shadows a page `Row` and vanishes when the body is
    /// done (GP32, the shipped scoping).
    style_scopes: Vec<HashMap<String, StyleDef>>,
    /// Instance prefixes that are CONTROL instances: only their own
    /// scoped anims may target `slot.<name>.<motion word>` lanes.
    control_scopes: HashSet<String>,
    node_whens: Vec<Vec<WhenTmp>>,
    /// The key table when `options.keys` is `None` (see `CompileOptions`).
    local_keys: Keys,
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
    /// Depth of `choice { }` scopes around the node being compiled — the
    /// Check/unnamed-Radio fences fire anywhere inside one.
    choice_depth: u32,
    /// Widget instances being expanded, outermost first: body names are
    /// per instance (`inst/name`), so a widget used twice never collides.
    prefix: Vec<String>,
    unnamed_instances: u32,
    /// Widget declarations being expanded, outermost first. Needed
    /// because a widget that instances itself — directly or through
    /// another widget — would expand forever: the guard turns the cycle
    /// into a named build error instead of a compiler stack overflow
    /// (GPAR69). Slot-fill content is written one scope out (`outer`),
    /// so a widget CAN fill its own slot from an instance site.
    widget_stack: Vec<String>,
    fills: Vec<Fills>,
    /// Per widget instance being expanded: the instance's `on:` props still
    /// waiting for the body's `signals` node (`None` = the root took them).
    instance_on: Vec<Option<Vec<AstProp>>>,
    /// While a Gradient's own line compiles: its name and the
    /// gradient-only words it carries, so G2003's Rectangle rewrite can be
    /// spelled out (`check_stops`).
    stops_ctx: Option<(String, Vec<String>)>,
    /// The item binding while a repeat template compiles (`item`, or
    /// `as $brush`): `$<binding>.<field>` values become `Value::VItem`
    /// for `set_items` to fill per clone (GPAR108).
    repeat_binding: Option<String>,
    /// Above 0 while a `when` response compiles: G2004 (invisible
    /// pigment) guards STATIC pigment only — a `when` endpoint keeps its
    /// alpha-0 value (fading a color out is the point).
    in_when: u32,
    /// Theme entry kinds seen per variant — a second `theme { Slider … }`
    /// prototype for one kind is a silent first-wins without this.
    theme_entry_kinds: Vec<(Kind, StrId)>,
    /// Palette overrides whose value was a bare palette word (`red =
    /// green`): the chain the circular-override fence walks (G6005).
    palette_links: std::collections::HashMap<String, String>,
    /// Repeats found while building: (the repeat's name, the binding,
    /// the key field — `NO_STR` = the item is its own key, `capacity=`,
    /// keyed?). `finish` resolves them to `Page::repeats` once ids exist.
    pending_repeats: Vec<(StrId, StrId, StrId, u32, bool)>,
}

/// A value as the file spells it, for a message.
fn spell(v: &AstValue) -> String {
    let suffixes = |opacity: &Option<f32>, pos: &Option<f32>| {
        let mut s = String::new();
        if let Some(o) = opacity {
            s.push_str(&format!("/{o}"));
        }
        if let Some(p) = pos {
            s.push_str(&format!("@{p}"));
        }
        s
    };
    match v {
        AstValue::Bool(b) => b.to_string(),
        AstValue::Num { value, unit } => format!("{value}{unit}"),
        AstValue::Color { rgba, opacity, pos } => format!("#{:02X}{:02X}{:02X}{:02X}{}", rgba[0], rgba[1], rgba[2], rgba[3], suffixes(opacity, pos)),
        AstValue::Str { text, .. } => format!("\"{text}\""),
        AstValue::Ref { name, opacity } => format!("${name}{}", suffixes(opacity, &None)),
        AstValue::Ident { word, opacity, pos } => format!("{word}{}", suffixes(opacity, pos)),
        AstValue::Size { w, h } => format!("{}x{}", w.map(|x| x.to_string()).unwrap_or_default(), h.map(|x| x.to_string()).unwrap_or_default()),
        AstValue::Call { name, args } => format!("{name}({})", args.iter().map(spell).collect::<Vec<_>>().join(", ")),
        AstValue::List(items) => format!("[{}]", items.iter().map(spell).collect::<Vec<_>>().join(", ")),
        AstValue::Range(a, b) => format!("{}..{}", spell(a), spell(b)),
    }
}

pub struct Compiled {
    pub page: Page,
    pub diags: Vec<Diagnostic>,
    /// The key table this compile filled: its own when `options.keys` was
    /// `None` (ids local to this page — `keys.names[key]` spells a `VText`
    /// key), empty when the build's shared table took them.
    pub keys: Keys,
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
        style_scopes: Vec::new(),
        control_scopes: HashSet::new(),
        node_whens: Vec::new(),
        local_keys: Keys::default(),
        deferred_whens: Vec::new(),
        anim_decls: Vec::new(),
        root_seen: false,
        names: std::collections::HashSet::new(),
        used: std::collections::HashSet::new(),
        prefix: Vec::new(),
        unnamed_instances: 0,
        widget_stack: Vec::new(),
        choice_depth: 0,
        fills: Vec::new(),
        instance_on: Vec::new(),
        stops_ctx: None,
        repeat_binding: None,
        in_when: 0,
        theme_entry_kinds: Vec::new(),
        palette_links: std::collections::HashMap::new(),
        pending_repeats: Vec::new(),
    };
    c.page.intern("");
    c.document(&doc, true);
    c.finish();
    Compiled { page: c.page, diags: c.diags, keys: c.local_keys }
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

    /// The id of translation key `name` carrying `english` — from the
    /// build's shared table when there is one, else this page's own. G8001
    /// when the name already carries another English string (checked in
    /// the table, so it holds across every file of the build).
    fn key(&mut self, name: &str, english: &str, line: u32) -> KeyId {
        let origin = self.options.source_name;
        let result = match self.options.keys {
            Some(shared) => shared.borrow_mut().intern(name, english, origin),
            None => self.local_keys.intern(name, english, origin),
        };
        match result {
            Ok(id) => id,
            Err((prev, first_file)) => {
                let where_ = if first_file.is_empty() { "elsewhere".to_string() } else { format!("in {first_file}") };
                self.error("G8001", line, format!("key `{name}` carries \"{prev}\" {where_}, here \"{english}\" — one key, one English string"));
                match self.options.keys {
                    Some(shared) => shared.borrow().get(name).unwrap_or(NO_KEY),
                    None => self.local_keys.get(name).unwrap_or(NO_KEY),
                }
            }
        }
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
                    // Color context (GPAR30, the shipped rule): `$dim =
                    // gray` must be the COLOR gray at its use site, and
                    // `$scrim = black/0.5` carries its alpha. Numbers and
                    // strings pass through unchanged.
                    let v = self.value(value, *line, Some(PropKey::Background));
                    let name = self.intern(key);
                    if self.page.theme.iter().any(|t| t.name == name && t.variant == NO_STR) {
                        self.error("G6002", *line, format!("`${key}` is defined twice"));
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
                            // The one-line form (`when s a.b=1, c.d=2`)
                            // carries its responses as PROPS — read them
                            // like block lines (GPAR70; the shipped
                            // fixtures pose through it).
                            let mut items: Vec<AstItem> = n.props.iter().map(|p| AstItem::Assign { key: p.key.clone(), dollar: false, value: p.value.clone(), line: p.line }).collect();
                            items.extend(n.children.iter().cloned());
                            self.deferred_whens.push((state, items, n.line));
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
        if let Some(AstValue::Str { key: Some(k), .. }) = n.args.first() {
            self.error("G0023", n.line, format!("`uses` takes one quoted path — `/{k}` is a translation key, and a path is never translated"));
            return;
        }
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
        // `theme font=…` written as a top-level LINE: the node-body
        // override form, misplaced — dropped silently before (GPAR88).
        if n.name.is_none() && !n.props.is_empty() && n.children.is_empty() {
            self.error("G3006", n.line, format!("subtree theme overrides go inside a node's body, not at top level — for a default use a block: `theme {{ {} = … }}`", n.props[0].key));
            return;
        }
        for item in &n.children {
            match item {
                AstItem::Assign { key, dollar: true, value, line } => {
                    // Color context — see the top-level `$` arm (GPAR30).
                    let v = self.value(value, *line, Some(PropKey::Background));
                    let name = self.intern(key);
                    if self.page.theme.iter().any(|t| t.name == name && t.variant == variant) {
                        self.error("G6002", *line, format!("`${key}` is defined twice in this theme block"));
                    }
                    self.page.theme.push(ThemeEntry { name, variant, value: v });
                }
                AstItem::Assign { key, dollar: false, value, line } => {
                    // Control defaults: `font = $x`, `font_size = 15` land on
                    // every text-bearing kind.
                    if let Some(pk) = PropKey::from_word(key) {
                        if pk == PropKey::DisabledOpacity {
                            // Page-wide: parked on the root kind's defaults,
                            // `Page::disabled_opacity` reads it back.
                            let v = self.value(value, *line, Some(pk));
                            let first = self.page.props.len() as u32;
                            self.page.props.push(Prop { key: pk, value: v });
                            self.page.kind_defaults.push(KindDefault { kind: Kind::View, props: (first, 1) });
                            continue;
                        }
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
                        self.error("G6003", *line, format!("`{key}` is not a palette color (new names are `$values`)"));
                        continue;
                    }
                    if matches!(value, AstValue::Color { pos: Some(_), .. } | AstValue::Ident { pos: Some(_), .. }) {
                        self.error("G0040", *line, "`@` positions a gradient stop — it only goes on entries of a Gradient's `stops=` list");
                        continue;
                    }
                    // A bare-word override links palette names; a chain
                    // that returns to its start can never resolve
                    // (G6005) — eager resolution here would silently
                    // paint the OLD color instead.
                    if let AstValue::Ident { word, .. } = value {
                        self.palette_links.insert(key.clone(), word.clone());
                        let mut cur = word.clone();
                        for _ in 0..12 {
                            if cur == *key {
                                self.error("G6005", *line, format!("circular color override chain at '{key}'"));
                                break;
                            }
                            match self.palette_links.get(&cur) {
                                Some(nxt) => cur = nxt.clone(),
                                None => break,
                            }
                        }
                    }
                    match self.color_value(value, *line) {
                        Some(c) => {
                            self.palette.insert(key.clone(), c);
                        }
                        None => {
                            let pool: Vec<String> = self.palette.keys().cloned().collect();
                            let hint = suggest(key, &pool);
                            let tail = if hint.is_empty() { String::new() } else { format!(" — {hint}") };
                            self.error("G6004", *line, format!("`{key} = …` needs a color{tail}"));
                        }
                    }
                }
                AstItem::Node(kn) => {
                    // Kind defaults: `Button background=..., font_size=...`.
                    let Some(kind) = Kind::from_word(&kn.type_word) else {
                        self.error("G3006", kn.line, format!("`{}` is not a control kind", kn.type_word));
                        continue;
                    };
                    // A theme entry is the prototype for EVERY node of
                    // its kind (GPAR88, the shipped fences): no name, no
                    // wiring, no placement, one entry per kind per block.
                    if let Some(nm) = &kn.name {
                        self.error("G3006", kn.line, format!("a theme entry has no name — it is the prototype for EVERY {}; names (and wiring) belong to instances (`{nm}`)", kn.type_word));
                        continue;
                    }
                    if let Some(wp) = kn.props.iter().find(|p| p.key.starts_with("on:")) {
                        self.error("G3006", wp.line, format!("wiring belongs to instances — a theme entry has no name to wire; drop `{}`", wp.key));
                        continue;
                    }
                    const ENTRY_PLACEMENT_KEYS: &[&str] = &["width", "height", "min_width", "min_height", "max_width", "max_height", "size", "min_size", "x", "y", "position", "halign", "valign", "margin", "margin_left", "margin_top", "margin_right", "margin_bottom", "overlay"];
                    if let Some(pp) = kn.props.iter().find(|p| ENTRY_PLACEMENT_KEYS.contains(&p.key.as_str())) {
                        self.error("G3006", pp.line, format!("`{}=` is placement — it belongs where the instance sits, not on a type default", pp.key));
                        continue;
                    }
                    if kn.args.iter().any(|a| matches!(a, AstValue::Size { .. })) {
                        self.error("G3006", kn.line, "a WxH size is placement — it belongs where the instance sits, not on a type default");
                        continue;
                    }
                    if self.theme_entry_kinds.contains(&(kind, variant)) {
                        self.error("G3006", kn.line, format!("duplicate theme entry for {} — one prototype per type per block; fold the props into it", kn.type_word));
                        continue;
                    }
                    self.theme_entry_kinds.push((kind, variant));
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
        let def = self.style_def(&name, n);
        self.styles.insert(name, def);
    }

    /// One style block's definition (`for=`/`extends=` pulled, `when`/
    /// `transition` children collected) — shared by page-level `style`
    /// lines and a widget's scoped ones (GP32).
    fn style_def(&mut self, name: &str, n: &AstNode) -> StyleDef {
        // `for=Button` names the ONE control type the style may dress —
        // wearing it elsewhere is refused where the style is applied.
        let mut props: Vec<AstProp> = n.props.clone();
        let mut for_target: Option<String> = None;
        if let Some(i) = props.iter().position(|p| p.key == "for") {
            if i != 0 {
                self.error("G3006", props[i].line, format!("`for=` comes first — the reader knows the target before any other word; reorder: `style {name} for=…, …`"));
            }
            let fp = props.remove(i);
            match fp.value.as_ident() {
                Some(word) if Kind::from_word(word).is_some() => for_target = Some(word.to_string()),
                Some(word) if self.widgets.contains_key(word) => {
                    self.error("G3006", fp.line, format!("`{word}` is a widget — widgets are styled through their own body's nodes, not `for=`"));
                }
                Some(word) => self.error("G3006", fp.line, format!("unknown control `{word}` — `for=` names a control kind (`for=Button`)")),
                None => self.error("G3006", fp.line, format!("`for=` names a control kind (`for=Button`), not `{}`", spell(&fp.value))),
            }
        }
        // `extends=Base`: the parent's props and whens come first, this
        // style's win — resolved (with cycle/target checks) where worn.
        let mut extends: Option<String> = None;
        if let Some(i) = props.iter().position(|p| p.key == "extends") {
            let ep = props.remove(i);
            match ep.value.as_ident() {
                Some(word) => extends = Some(word.to_string()),
                None => self.error("G3006", ep.line, format!("`extends=` takes a style name (`style Warning extends=Base, …`), not `{}`", spell(&ep.value))),
            }
        }
        for p in &props {
            if matches!(p.key.as_str(), "cols" | "areas" | "row_height") {
                self.error("G1002", p.line, format!("`{}=` is grid structure and can't come from a style — write it on the Grid line", p.key));
            }
        }
        let mut def = StyleDef { props, for_target, extends, line: n.line, ..Default::default() };
        for item in &n.children {
            match item {
                AstItem::Node(c) if c.type_word == "when" => def.whens.push(c.clone()),
                AstItem::Node(c) if c.type_word == "transition" => def.transitions.push(c.clone()),
                AstItem::Node(c) => self.error("G0026", c.line, format!("a style block holds `when` and `transition` lines, not `{}`", c.type_word)),
                AstItem::Assign { key, line, .. } => self.error("G0026", *line, format!("a style block holds `when` and `transition` lines, not `{key} =`")),
            }
        }
        def
    }

    /// The style `name` resolves to here: the innermost widget scope
    /// first, then the page's (GP32, the shipped shadowing order).
    fn find_style(&self, name: &str) -> Option<&StyleDef> {
        self.style_scopes.iter().rev().find_map(|s| s.get(name)).or_else(|| self.styles.get(name))
    }

    /// A style with its `extends=` chain folded in: the parent's props
    /// and whens first, the child's after (later wins on lookup). The
    /// chain is judged here, where the style is worn — an unknown parent,
    /// a cycle, and a `for=` mismatch across the chain all refuse.
    fn resolve_style(&mut self, name: &str, line: u32) -> Option<StyleDef> {
        let Some(mut def) = self.find_style(name).cloned() else {
            let mut pool: Vec<String> = self.styles.keys().cloned().collect();
            for s in &self.style_scopes {
                pool.extend(s.keys().cloned());
            }
            let hint = suggest(name, &pool);
            let tail = if hint.is_empty() { String::new() } else { format!(" — {hint}") };
            self.error("G5002", line, format!("unknown style `.{name}`{tail}"));
            return None;
        };
        let mut chain: Vec<String> = vec![name.to_string()];
        let mut merged = def.clone();
        while let Some(parent_name) = def.extends.clone() {
            if chain.contains(&parent_name) {
                self.error("G3006", def.line, format!("style `{name}` extends itself — the chain: {} → {parent_name}", chain.join(" → ")));
                return None;
            }
            let Some(parent) = self.find_style(&parent_name).cloned() else {
                self.error("G5002", def.line, format!("`extends={parent_name}` names no style"));
                return None;
            };
            match (&merged.for_target, &parent.for_target) {
                (None, Some(t)) => {
                    self.error("G3006", def.line, format!("plain style `{name}` extends `{parent_name}`, which is `for={t}` — it inherits words it can't check; give `{name}` the same `for=`"));
                    return None;
                }
                (Some(a), Some(b)) if a != b => {
                    self.error("G3006", def.line, format!("`{name}` is for {a} but extends `{parent_name}`, which is for {b} — another control's words"));
                    return None;
                }
                _ => {}
            }
            let mut props = parent.props.clone();
            props.extend(merged.props);
            merged.props = props;
            let mut whens = parent.whens.clone();
            whens.extend(merged.whens);
            merged.whens = whens;
            let mut transitions = parent.transitions.clone();
            transitions.extend(merged.transitions);
            merged.transitions = transitions;
            chain.push(parent_name.clone());
            def = parent;
        }
        Some(merged)
    }

    // ----- grid structure (E8/GPAR99) ---------------------------------

    /// One `cols=` / row-height track: an epx number, `hug`, or `fill(n)`.
    /// A `$ref` passes as `Unset` — layout resolves it.
    fn track_spec(&mut self, v: &AstValue, line: u32, key: &str) -> Option<Len> {
        match v {
            AstValue::Num { value, .. } => Some(Len::Exact(*value)),
            AstValue::Ident { word, .. } if word == "hug" => Some(Len::Hug(1.0)),
            AstValue::Ident { word, .. } if word == "fill" => Some(Len::Fill(1.0)),
            AstValue::Ident { word, .. } if word == "auto" => {
                self.error("G1002", line, format!("`auto` is WPF's word — gogo tracks say `hug` (content-sized); {key} takes epx numbers, `hug`, or `fill(n)`"));
                None
            }
            AstValue::Ident { word, .. } => {
                self.error("G1002", line, format!("`{word}` is not a track size — {key} entries are epx numbers, `hug` (content) or `fill(n)` (weighted leftover)"));
                None
            }
            AstValue::Call { name, .. } if name == "hug" => {
                self.error("G1002", line, "`hug` takes no arguments — shared size groups (`hug(name)`) are a future extension; plain `hug` for now");
                None
            }
            AstValue::Call { name, args } if name == "fill" => match args.as_slice() {
                [AstValue::Num { value, .. }] if *value > 0.0 => Some(Len::Fill(*value)),
                [AstValue::Num { .. }] => {
                    self.error("G1002", line, "a `fill(…)` weight must be > 0 — use `hug` to opt out of the leftover space");
                    None
                }
                _ => {
                    self.error("G1002", line, "`fill(…)` takes one bare number — the weight; fill tracks split the leftover space by weight (`fill(2)`)");
                    None
                }
            },
            AstValue::Call { name, .. } => {
                self.error("G1002", line, format!("`{name}(…)` is not a track size — {key} entries are epx numbers, `hug` or `fill(n)`"));
                None
            }
            AstValue::Ref { .. } => Some(Len::Unset),
            _ => {
                self.error("G1002", line, format!("{key} can't take `{}` — track sizes are epx numbers, `hug`, or `fill(n)`", spell(v)));
                None
            }
        }
    }

    /// A Grid's lattice at load: `cols=` validated, then the row form
    /// walked or the areas form expanded (the shipped resolver's
    /// `_apply_grid_children`). Needed because an unchecked
    /// lattice lays out silently wrong — a three-cell row over two
    /// tracks clips, an `areas=` typo is an empty grid.
    ///
    /// Returns the replacement node when `areas=` expanded into rows.
    fn grid_structure(&mut self, n: &AstNode) -> Option<AstNode> {
        let Some(cols_prop) = n.props.iter().find(|p| p.key == "cols").cloned() else {
            self.error("G1002", n.line, "Grid needs `cols=` — the track signature every row is checked against (`Grid cols=[hug, fill] { row { … } }`)");
            return None;
        };
        let AstValue::List(entries) = &cols_prop.value else {
            self.error("G1002", cols_prop.line, "`cols=` takes a track list — epx numbers, `hug` (content) or `fill(n)` (weighted leftover): `cols=[80, hug, fill(2)]`");
            return None;
        };
        if entries.is_empty() {
            self.error("G1002", cols_prop.line, "`cols=` declares no tracks — at least one: `cols=[fill]`");
            return None;
        }
        let mut col_specs: Vec<Len> = Vec::new();
        for e in entries.clone() {
            col_specs.push(self.track_spec(&e, cols_prop.line, "cols=")?);
        }
        let cols = col_specs.len();
        let mut row_specs: Vec<Len> = Vec::new();
        let replaced = if n.props.iter().any(|p| p.key == "areas") {
            if let Some(rh) = n.props.iter().find(|p| p.key == "row_height") {
                self.error("G1003", rh.line, "areas rows carry their own heights — `row_height=` sizes nothing here; put the height on each row: `areas=[\"header : 56\", \"main : fill\"]`");
                return None;
            }
            self.grid_areas(n, cols, &mut row_specs)
        } else {
            self.grid_rows(n, cols, &mut row_specs);
            None
        };
        // The D89 dead-space lint: a grid told to FILL an axis where no
        // track takes the leftover parks dead space after the last
        // row/column.
        let clean = !crate::diag::has_errors(&self.diags);
        let says_fill = |key: &str| n.props.iter().any(|p| p.key == key && matches!(&p.value, AstValue::Ident { word, .. } if word == "fill"));
        if clean && says_fill("height") && !row_specs.iter().any(|l| matches!(l, Len::Fill(_))) {
            self.warning("G1010", n.line, "`height=fill` but no row takes the leftover — the space under the last row is dead; give one row `: fill` / `row height=fill { }`, or size the grid `height=hug`");
        }
        if clean && says_fill("width") && !col_specs.iter().any(|l| matches!(l, Len::Fill(_))) {
            self.warning("G1010", n.line, "`width=fill` but no column takes the leftover — the space right of the last column is dead; make one `cols=` track `fill`, or size the grid `width=hug`");
        }
        replaced
    }

    /// The row form: each `row { }` walks left to right against the
    /// `cols=` signature; `skip` covers one track, and a track covered by
    /// a `row_span` from above must be spelled `skip`.
    fn grid_rows(&mut self, n: &AstNode, cols: usize, row_specs: &mut Vec<Len>) {
        let default_h: Option<Len> = match n.props.iter().find(|p| p.key == "row_height").cloned() {
            Some(p) => self.track_spec(&p.value, p.line, "row_height="),
            None => None,
        };
        let mut shadows: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
        let mut pending: Vec<(u32, usize, usize)> = Vec::new();
        let mut r = 0usize;
        for item in &n.children {
            let AstItem::Node(c) = item else { continue };
            match c.type_word.as_str() {
                // `skip`/`gap` here get their own fences on the child walk.
                "when" | "transition" | "theme" | "skip" | "gap" => continue,
                "slot" => {
                    self.error("G1002", c.line, "a slot can't sit directly in a Grid — put the slot inside a container cell");
                    continue;
                }
                "row" => {}
                _ => {
                    let hint = if c.type_word == "Row" { " (`Row` — the stack — can be a CELL inside one; a Grid's direct children are lowercase `row { }` blocks)" } else { "" };
                    self.error("G1002", c.line, format!("a Grid places by structure — `{}` isn't in a row; wrap this cell in a `row {{ }}` block{hint}", c.type_word));
                    continue;
                }
            }
            if let Some(name) = &c.name {
                self.error("G1002", c.line, format!("rows aren't nodes — no names (got `{name}`); name the cells instead"));
            }
            if !c.styles.is_empty() || !c.args.is_empty() || !c.flags.is_empty() {
                self.error("G1002", c.line, "rows carry only `height=` — they aren't drawable nodes");
            }
            let mut row_h: Option<Len> = None;
            for p in c.props.clone() {
                if p.key == "height" {
                    row_h = self.track_spec(&p.value, p.line, "row height=");
                } else {
                    self.error("G1002", p.line, format!("rows carry only `height=` — they aren't drawable nodes (got `{}=`)", p.key));
                }
            }
            row_specs.push(row_h.or(default_h).unwrap_or(Len::Unset));
            let mut at = 0usize;
            for cc in &c.children {
                let AstItem::Node(cell) = cc else { continue };
                match cell.type_word.as_str() {
                    "when" | "transition" => {
                        self.error("G1002", cell.line, "a `when` sits on a cell, not between cells");
                        continue;
                    }
                    "theme" => continue,
                    "row" => {
                        self.error("G1002", cell.line, "rows don't nest — a row holds cells");
                        continue;
                    }
                    "skip" => {
                        if cell.name.is_some() || !cell.args.is_empty() || !cell.props.is_empty() || !cell.children.is_empty() {
                            self.error("G1002", cell.line, "`skip` takes nothing — one bare `skip` per empty track");
                        }
                        at += 1;
                        continue;
                    }
                    _ => {}
                }
                let cspan = cell.props.iter().find(|p| p.key == "col_span").and_then(|p| p.value.as_num()).unwrap_or(1.0).max(1.0) as usize;
                let rspan = cell.props.iter().find(|p| p.key == "row_span").and_then(|p| p.value.as_num()).unwrap_or(1.0).max(1.0) as usize;
                self.cell_weight_check(cell);
                for k in at..at + cspan {
                    if shadows.contains(&(r, k)) {
                        self.error("G1002", cell.line, format!("track {} is covered by a `row_span` from the row above — write `skip` here", k + 1));
                    }
                }
                if rspan > 1 {
                    for rr in r + 1..r + rspan {
                        for k in at..at + cspan {
                            shadows.insert((rr, k));
                        }
                    }
                    pending.push((cell.line, r, rspan));
                }
                at += cspan;
            }
            if at > cols {
                self.error("G1002", c.line, format!("row {} covers {at} tracks; cols= declares {cols}", r + 1));
            }
            r += 1;
        }
        // A row_span must land inside the grid — WPF silently clamps these.
        for (line, row, rs) in pending {
            if row + rs > r {
                self.error("G1002", line, format!("row_span={rs} reaches past the last row ({r} declared)"));
            }
        }
    }

    /// `width=fill(2)` on a grid cell: the weight is inert — a cell fills
    /// its cell; weights belong on `cols=` tracks and row heights.
    fn cell_weight_check(&mut self, cell: &AstNode) {
        for p in &cell.props {
            if p.key == "width" || p.key == "height" {
                if let AstValue::Call { name, args } = &p.value {
                    if name == "fill" && !matches!(args.as_slice(), [AstValue::Num { value, .. }] if *value == 1.0) {
                        self.error("G1002", p.line, "a `fill(n)` weight is inert on a grid cell — a cell fills its cell; weights belong on `cols=` tracks and row heights");
                    }
                }
            }
        }
    }

    /// The areas form (CSS grid-template-areas): the strings are the
    /// layout picture, children bind by NAME, `.` marks holes, `: height`
    /// rides each row. Expanded here into the same `row { }` structure
    /// the row form builds, so layout walks one lattice.
    ///
    /// Every error points at the `areas=` line — values carry no own
    /// line here (Godot additionally pins the row string's line+column).
    fn grid_areas(&mut self, n: &AstNode, cols: usize, row_specs: &mut Vec<Len>) -> Option<AstNode> {
        struct AreaRect {
            name: String,
            r0: usize,
            c0: usize,
            r1: usize,
            c1: usize,
            count: usize,
            /// The row that STARTED this area — where the
    /// not-a-rectangle error points (GPAR99, the shipped caret).
    line: u32,
}
        let ap = n.props.iter().find(|p| p.key == "areas").cloned().expect("areas prop");
        // (row text, the line its errors point at): the string's own
        // line; a row after an escaped `\n` falls back to the `areas=`
        // line — offset math can't cross the two-chars-per-`\n` seam
        // (GPAR99, the shipped rule).
        let mut raw_rows: Vec<(String, u32)> = Vec::new();
        fn push_text(raw: &mut Vec<(String, u32)>, text: &str, line: u32, fallback: u32) {
            for (i, piece) in text.split('\n').enumerate() {
                let stripped = piece.replace('\t', " ").trim().to_string();
                if !stripped.is_empty() {
                    raw.push((stripped, if i == 0 { line } else { fallback }));
                }
            }
        }
        match &ap.value {
            AstValue::List(items) => {
                for it in items {
                    match it {
                        AstValue::Str { text, key: None, line } => push_text(&mut raw_rows, text, *line, ap.line),
                        AstValue::Str { key: Some(k), .. } => {
                            self.error("G1003", ap.line, format!("areas rows don't translate — no /{k} here"));
                            return None;
                        }
                        other => {
                            self.error("G1003", ap.line, format!("`areas=` rows are strings — got `{}`; `areas=[\"header header : 60\", \"nav body : fill\"]`", spell(other)));
                            return None;
                        }
                    }
                }
            }
            AstValue::Str { text, key: None, line } => push_text(&mut raw_rows, text, *line, ap.line),
            _ => {
                self.error("G1003", ap.line, "`areas=` takes the row strings, one per grid row: `areas=[\"header header : 60\", \"nav body : fill\"]`");
                return None;
            }
        }
        let mut rects: Vec<AreaRect> = Vec::new();
        let mut nrows = 0usize;
        for (line, rline) in &raw_rows {
            let r = nrows;
            let rline = *rline;
            // D89 (user call): every areas row states its height — a
            // silent `hug` default made dead space hard to diagnose.
            let Some(colon) = line.find(':') else {
                self.error("G1003", rline, format!("areas row {} has no height — end the row with `: <height>` (epx, `hug` or `fill`): `\"sidebar main : fill\"`", r + 1));
                return None;
            };
            if line[colon + 1..].contains(':') {
                self.error("G1003", rline, "one `:` per areas line — the names, then the row height");
                return None;
            }
            let htok = line[colon + 1..].trim();
            if htok.is_empty() {
                self.error("G1003", rline, "missing row height after `:` in areas");
                return None;
            }
            let h = self.area_height(htok, rline)?;
            let toks: Vec<&str> = line[..colon].split_whitespace().collect();
            if toks.len() != cols {
                self.error("G1003", rline, format!("areas line {} names {} tracks; cols= declares {cols} — every line spells one name (or `.` for a hole) per column", r + 1, toks.len()));
                return None;
            }
            for (c, t) in toks.iter().enumerate() {
                if *t == "." {
                    continue;
                }
                if !is_area_name(t) {
                    self.error("G1003", rline, format!("`{t}` is not an area name — area names are plain idents; `.` marks an empty region"));
                    return None;
                }
                match rects.iter_mut().find(|a| a.name == *t) {
                    Some(a) => {
                        a.r1 = a.r1.max(r);
                        a.c0 = a.c0.min(c);
                        a.c1 = a.c1.max(c);
                        a.count += 1;
                    }
                    None => rects.push(AreaRect { name: t.to_string(), r0: r, c0: c, r1: r, c1: c, count: 1, line: rline }),
                }
            }
            row_specs.push(h);
            nrows += 1;
        }
        if nrows == 0 {
            self.error("G1003", ap.line, "`areas=` declares no rows");
            return None;
        }
        for a in &rects {
            if (a.r1 - a.r0 + 1) * (a.c1 - a.c0 + 1) != a.count {
                self.error("G1003", a.line, format!("area `{}` isn't a rectangle — an area is one contiguous rectangle of its name; a split shape needs two names", a.name));
                return None;
            }
        }
        let mut kept: Vec<AstItem> = Vec::new();
        let mut cells: Vec<(String, AstNode)> = Vec::new();
        for item in &n.children {
            let AstItem::Node(c) = item else {
                kept.push(item.clone());
                continue;
            };
            match c.type_word.as_str() {
                "when" | "transition" | "theme" => {
                    kept.push(item.clone());
                    continue;
                }
                "row" => {
                    self.error("G1003", c.line, "this Grid places by areas= — row blocks are the other form (one Grid uses one placement form)");
                    return None;
                }
                "skip" => {
                    self.error("G1003", c.line, "`skip` belongs to row blocks — areas mark holes with `.`");
                    return None;
                }
                "slot" => {
                    self.error("G1003", c.line, "a slot can't sit directly in a Grid — put the slot inside a container cell");
                    return None;
                }
                _ => {}
            }
            let Some(name) = c.name.clone() else {
                self.error("G1003", c.line, format!("children of an areas Grid bind by name — this one has none; areas here: {}", rects.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")));
                return None;
            };
            if !rects.iter().any(|a| a.name == name) {
                self.error("G1003", c.line, format!("no area named `{name}`"));
                return None;
            }
            if cells.iter().any(|(nm, _)| *nm == name) {
                self.error("G1003", c.line, format!("area `{name}` is already bound"));
                return None;
            }
            self.cell_weight_check(c);
            cells.push((name, c.clone()));
        }
        for a in &rects {
            if !cells.iter().any(|(nm, _)| *nm == a.name) {
                self.warning("G1003", n.line, format!("area `{}` has no child bound to it — a child named `{}` fills the area", a.name, a.name));
            }
        }
        let mut new_children = kept;
        for r in 0..nrows {
            let mut row = AstNode { type_word: "row".to_string(), line: n.line, ..Default::default() };
            match row_specs[r] {
                Len::Exact(v) => row.props.push(AstProp { key: "height".to_string(), value: AstValue::Num { value: v, unit: String::new() }, line: n.line }),
                Len::Fill(w) if w == 1.0 => row.props.push(AstProp { key: "height".to_string(), value: AstValue::Ident { word: "fill".to_string(), opacity: None, pos: None }, line: n.line }),
                Len::Fill(w) => row.props.push(AstProp { key: "height".to_string(), value: AstValue::Call { name: "fill".to_string(), args: vec![AstValue::Num { value: w, unit: String::new() }] }, line: n.line }),
                _ => {}
            }
            let mut c = 0usize;
            while c < cols {
                if let Some(a) = rects.iter().find(|a| a.r0 == r && a.c0 == c) {
                    if let Some((_, cell)) = cells.iter().find(|(nm, _)| *nm == a.name) {
                        let mut cell = cell.clone();
                        let cspan = a.c1 - a.c0 + 1;
                        let rspan = a.r1 - a.r0 + 1;
                        if cspan > 1 {
                            cell.props.push(AstProp { key: "col_span".to_string(), value: AstValue::Num { value: cspan as f32, unit: String::new() }, line: cell.line });
                        }
                        if rspan > 1 {
                            cell.props.push(AstProp { key: "row_span".to_string(), value: AstValue::Num { value: rspan as f32, unit: String::new() }, line: cell.line });
                        }
                        row.children.push(AstItem::Node(cell));
                    } else {
                        // An unbound area: `skip`s hold its tracks.
                        for _ in a.c0..=a.c1 {
                            row.children.push(AstItem::Node(AstNode { type_word: "skip".to_string(), line: n.line, ..Default::default() }));
                        }
                    }
                    c = a.c1 + 1;
                } else {
                    // A hole, or a track covered by a rect from an
                    // earlier row.
                    row.children.push(AstItem::Node(AstNode { type_word: "skip".to_string(), line: n.line, ..Default::default() }));
                    c += 1;
                }
            }
            new_children.push(AstItem::Node(row));
        }
        let mut out = n.clone();
        out.props.retain(|p| p.key != "areas");
        out.children = new_children;
        Some(out)
    }

    /// A height token from an areas row (`: 30`, `: hug`, `: fill(2)`).
    /// It lives inside a string, so `$refs` can't reach it — theme-driven
    /// heights use the row form.
    fn area_height(&mut self, tok: &str, line: u32) -> Option<Len> {
        if let Ok(v) = tok.parse::<f32>() {
            return Some(Len::Exact(v));
        }
        if let Some(inner) = tok.strip_prefix("fill(").and_then(|s| s.strip_suffix(')')) {
            let Ok(w) = inner.trim().parse::<f32>() else {
                self.error("G1003", line, format!("`fill(…)` in areas takes one bare number, got `{inner}`"));
                return None;
            };
            return self.track_spec(&AstValue::Call { name: "fill".to_string(), args: vec![AstValue::Num { value: w, unit: String::new() }] }, line, "areas");
        }
        match tok {
            "hug" => Some(Len::Hug(1.0)),
            "fill" => Some(Len::Fill(1.0)),
            "auto" => {
                self.error("G1003", line, "`auto` is WPF's word — areas heights say `hug` (content-sized)");
                None
            }
            _ => {
                self.error("G1003", line, format!("`{tok}` is not a row height — areas heights are epx numbers, `hug`, or `fill(n)`; $refs need the row form"));
                None
            }
        }
    }

    /// The Tiles lattice at load: `cols=` is a whole column count and
    /// every `span=` fits it. Needed because without it a
    /// missing `cols=` is one silent column and an overlong span clamps
    /// quietly.
    fn tiles_structure(&mut self, n: &AstNode) {
        let cols: Option<usize> = match n.props.iter().find(|p| p.key == "cols") {
            None => {
                self.error("G1002", n.line, "Tiles needs `cols=` — the column count every `span=` is checked against (`Tiles cols=3 { … }`)");
                None
            }
            Some(p) => match &p.value {
                AstValue::Num { value, .. } if *value >= 1.0 && value.fract() == 0.0 => Some(*value as usize),
                AstValue::Ref { .. } => None,
                AstValue::List(_) => {
                    self.error("G1002", p.line, "Tiles `cols=` is one whole number of columns — track lists belong to Grid");
                    None
                }
                other => {
                    self.error("G1002", p.line, format!("Tiles `cols=` is a whole number of columns, at least 1 — got `{}`", spell(other)));
                    None
                }
            },
        };
        for item in &n.children {
            let AstItem::Node(c) = item else { continue };
            if matches!(c.type_word.as_str(), "when" | "transition" | "theme") {
                continue;
            }
            let span = c.props.iter().find(|p| p.key == "span").cloned();
            if let Some(fp) = c.props.iter().find(|p| p.key == "full_row").cloned() {
                if !matches!(fp.value, AstValue::Bool(_) | AstValue::Ref { .. }) {
                    self.error("G1002", fp.line, format!("`full_row=` is true or false — got `{}`", spell(&fp.value)));
                }
                if span.is_some() && matches!(fp.value, AstValue::Bool(true)) {
                    self.error("G1002", fp.line, "`full_row=true` takes the whole row — `span=` says nothing beside it");
                }
            }
            if let Some(sp) = span {
                if let Some(s) = sp.value.as_num() {
                    if s < 1.0 || s.fract() != 0.0 {
                        self.error("G1002", sp.line, format!("`span=` is a whole number of columns, at least 1 — got `{}`", spell(&sp.value)));
                    } else if let Some(cols) = cols {
                        if s as usize > cols {
                            self.error("G1002", sp.line, format!("`span={}` exceeds the lattice — this Tiles has cols={cols}", s as usize));
                        }
                    }
                }
            }
        }
    }

    fn widget(&mut self, n: &AstNode) {
        let Some(name) = n.name.clone() else {
            self.error("G0027", n.line, "`widget` needs a name");
            return;
        };
        // A widget wearing a built-in word's name is dead: the built-in
        // is matched first, so the widget could never be instantiated.
        if Kind::from_word(&name).is_some() || matches!(name.as_str(), "when" | "transition" | "theme" | "style" | "anim" | "widget" | "control" | "uses" | "layout") {
            self.error("G5001", n.line, format!("widget name `{name}` shadows a built-in — pick another name"));
            return;
        }
        // A `control` body holds its behavior lines (`script`, `state`,
        // `state_builtin`, `signal`, a default `transition`) beside the root;
        // with no code-behind here they carry nothing for the picture. A
        // `widget` has no script to raise or read them, so there they are
        // refused, not skipped; `state_builtin` names must be built-ins.
        for b in n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }) {
            match b.type_word.as_str() {
                "script" | "state" | "state_builtin" | "signal" if n.type_word == "widget" => {
                    self.error("G0077", b.line, format!("`{}` lines belong to a `control` body (they describe its script) — a `widget` has none", b.type_word));
                }
                "state_builtin" => {
                    for word in b.args.iter().filter_map(|a| a.as_ident()) {
                        match self.state_from_word(word, b.line, false) {
                            Some(State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled) | None => {}
                            Some(_) => self.error("G0077", b.line, format!("`state_builtin {word}`: the built-ins are held, hovered, focused, focus_within, selected, disabled")),
                        }
                    }
                }
                _ => {}
            }
        }
        let bodies: Vec<&AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| !matches!(b.type_word.as_str(), "script" | "state" | "state_builtin" | "signal" | "transition" | "when" | "anim" | "style")).collect();
        if bodies.len() != 1 {
            self.error("G0028", n.line, format!("widget `{name}` needs exactly one root node in its body"));
            return;
        }
        let whens: Vec<AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "when").cloned().collect();
        let anims: Vec<AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "anim").cloned().collect();
        let transitions: Vec<AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "transition").cloned().collect();
        // Scoped styles (GP32): built once here, pushed as a lookup scope
        // for every instance's body build. `for=` stays a top-level
        // affair — a scoped style dresses its own body's nodes.
        let mut styles: HashMap<String, StyleDef> = HashMap::new();
        for s in n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "style") {
            let Some(sname) = s.name.clone() else {
                self.error("G0025", s.line, "`style` needs a name");
                continue;
            };
            if s.props.iter().any(|p| p.key == "for") {
                self.error("G3006", s.line, "`for=` styles are top-level declarations — a widget's scoped styles style its own body's nodes");
                continue;
            }
            let def = self.style_def(&sname, s);
            styles.insert(sname, def);
        }
        // `widget X for=Radio`: the body fills a stateful host, so the
        // widget-scope `when` lines answer the HOST's states (D276).
        let mut props: Vec<AstProp> = n.props.clone();
        let mut host: Option<Kind> = None;
        if let Some(i) = props.iter().position(|p| p.key == "for") {
            if i != 0 {
                self.error("G5001", props[i].line, format!("`for=` comes first — the reader knows the host before the body; reorder: `widget {name} for=…, …`"));
            }
            let fp = props.remove(i);
            match fp.value.as_ident().and_then(Kind::from_word) {
                Some(k @ (Kind::Button | Kind::Radio | Kind::Check)) => host = Some(k),
                _ => self.error("G5001", fp.line, "widget for= takes the stateful host type the body fills: Button, Radio, Check"),
            }
        }
        // The no-host fence is the WIDGET's: a `control` body legitimately
        // carries a default `transition` line for its own states.
        if host.is_none() && n.type_word == "widget" {
            for w in n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "when" || b.type_word == "transition") {
                self.error("G5001", w.line, format!("a widget-scope `{}` responds to the HOST's states — this widget declares no host; announce it: `widget {name} for=Radio, …` (hosts: Button, Radio, Check)", w.type_word));
            }
        }
        self.widgets.insert(name, WidgetDef { params: n.params.clone(), body: bodies[0].clone(), anims, host, props, whens, transitions, styles, control: n.type_word == "control", line: n.line });
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
    fn anim_ref(&mut self, name: &str, line: u32) -> StrId {
        for depth in (1..=self.prefix.len()).rev() {
            let scope = self.prefix[..depth].join("/");
            if self.anim_decls.iter().any(|(s, a)| *s == scope && a.name.as_deref() == Some(name)) {
                return self.intern(&format!("{scope}/{name}"));
            }
        }
        // A play naming no declared anim is a build error (the shipped
        // bake refuses it; GPAR84) — a silent no-op loses the tap.
        // Slash-qualified names address another scope's copy and pass.
        if !name.contains('/') && !self.anim_decls.iter().any(|(_, a)| a.name.as_deref() == Some(name)) {
            self.error("G4004", line, format!("unknown anim `{name}` — `play`/`stop` name a declared `anim`"));
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
                    // `slot.<name>.<motion word>` (GP32): a control's own
                    // anims may drive a slot region's MOTION channel
                    // (visual offsets; layout, and anything the value
                    // drives, has ONE writer — the driver). From outside
                    // a control its internals stay invisible (the widget
                    // rule), so the lane is refused there.
                    let slot_lane: Option<(String, String)> = if let Some(rest) = path.strip_prefix("slot.") {
                        if !self.control_scopes.contains(scope) {
                            self.error("G0063", step.line, "`slot.` anim targets work only in a control's OWN scoped anims — from outside, a control's internals stay invisible (the widget rule)");
                            continue;
                        }
                        match rest.split_once('.') {
                            None => {
                                self.error("G0063", step.line, "a slot lane names its slot: `slot.<name>.<motion word>`");
                                continue;
                            }
                            Some((_sname, ch)) if Channel::parse(ch).is_none() => {
                                self.error("G0063", step.line, format!("slot lanes use the MOTION channel — `{ch}` isn't a motion word; animate translate.x|translate.y|scale|rotate|opacity"));
                                continue;
                            }
                            Some((sname, ch)) => {
                                let full = format!("{scope}/{sname}");
                                if self.page.find(&full).map(|id| self.page.nodes[id as usize].kind) != Some(Kind::Slot) {
                                    let slots: Vec<String> = self.page.nodes.iter().filter(|nd| nd.kind == Kind::Slot && nd.name != NO_STR).map(|nd| self.page.str(nd.name).to_string()).filter(|nm| nm.starts_with(&format!("{scope}/"))).map(|nm| nm.rsplit('/').next().unwrap_or_default().to_string()).collect();
                                    self.error("G0060", step.line, format!("`{path}`: no slot named `{sname}` in this control{}", if slots.is_empty() { String::new() } else { format!(" — its slots: {}", slots.join(", ")) }));
                                    continue;
                                }
                                Some((full, ch.to_string()))
                            }
                        }
                    } else {
                        None
                    };
                    let (node_name, channel) = if let Some(sl) = slot_lane {
                        sl
                    } else {
                        match path.split_once('.') {
                        Some((n, rest)) if Channel::parse(rest).is_some() => (n.to_string(), rest.to_string()),
                        _ => {
                            if path.ends_with(".modulate.a") || path == "modulate.a" {
                                self.error("G0075", step.line, "`modulate.a` is a Godot engine word — write `opacity`");
                            } else if Channel::parse(path).is_some() {
                                self.error("G0063", step.line, format!("`{path}` needs a node: `<node>.{path}` (bare paths only inside `each`)"));
                            } else {
                                self.error("G0063", step.line, format!("`{path}`: want `<node>.translate.x|translate.y|scale|scale.x|scale.y|rotate|opacity`"));
                            }
                            continue;
                        }
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
        let mut t = Transition { node, from: None, to: None, duration: 0.0, easing: NO_STR, anim: NO_STR, line: n.line };
        // One arrow, one endpoint each side. A lone endpoint used to read
        // as `s -> any` and `-> held` as `any -> held`, silently (GPAR23).
        let ends: Vec<&str> = n.args.iter().filter_map(|a| a.as_ident()).collect();
        if ends.len() > 1 || n.arrows.len() > 1 {
            self.error("G0034", n.line, "a transition rule is `transition A -> B` — one arrow, one endpoint each side (`any` is the wildcard)");
        }
        match (ends.first(), n.arrows.first()) {
            (Some(from), None) => self.error("G0034", n.line, format!("`transition {from}`: an endpoint needs the arrow — `{from} -> any` (leaving) or `any -> {from}` (entering)")),
            (None, Some(to)) => self.error("G0034", n.line, format!("`transition -> {to}`: say where from — `any -> {to}`")),
            _ => {}
        }
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
                        let hint = if top_level {
                            let pool: Vec<String> = self.page.arrangements.iter().map(|a| self.page.str(*a).to_string()).collect();
                            let s = suggest(word, &pool);
                            if s.is_empty() { "declare it with `arrangement <name>` first".to_string() } else { s }
                        } else {
                            "built-ins are held, hovered, focused, focus_within, selected, disabled, screen.<class>".to_string()
                        };
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
        let conditions: Vec<&str> = n.args.iter().filter_map(|a| a.as_ident()).collect();
        if conditions.len() > 1 {
            self.error("G0038", n.line, format!("a `when` composes at most two conditions — `{}` is a third; conditions AND (the block holds only while ALL hold): `when <arrangement> screen.<class>` / `when screen.<orientation> screen.<class>`. If two screen words meant EITHER-OR (this class or that one), say the axis they share: `screen.landscape` / `screen.portrait`", conditions[1]));
            return None;
        }
        let first = self.state_from_word(&word, n.line, top_level)?;
        let second = conditions.first().map(|s| s.to_string());
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

    /// A key whose bare words are palette lookups. `font_color=` counts
    /// (GPAR30 — `font_color=accent` used to fall through to the white
    /// default with a clean build).
    fn is_color_key(key: PropKey) -> bool {
        matches!(key, PropKey::Background | PropKey::Color | PropKey::BorderColor | PropKey::Stops | PropKey::FontColor)
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

    /// A color where it lands: a `stops=` entry (with its `@pos`), else a
    /// plain color — an `@pos` anywhere else is refused (G0040) instead of
    /// being thrown away.
    fn stop_value(&mut self, c: Rgba, pos: Option<f32>, key: Option<PropKey>, line: u32) -> Value {
        if key == Some(PropKey::Stops) {
            return Value::VStop { color: c, pos };
        }
        if let Some(p) = pos {
            self.error("G0040", line, format!("`@{p}` positions a gradient stop — it only goes on entries of a Gradient's `stops=` list (`stops=[white@0, blue/0.5@0.24, accent@1]`)"));
        }
        Value::VColor(c)
    }

    fn value(&mut self, v: &AstValue, line: u32, key: Option<PropKey>) -> Value {
        match v {
            AstValue::Bool(b) => Value::VBool(*b),
            AstValue::Num { value, unit } => {
                if unit == "px" {
                    self.error("G0038", line, "`px` is gone — lengths are epx (§7)");
                }
                if unit == "%" {
                    self.error("G0038", line, "unit `%` is not supported in gogo v1 — lengths are epx, shares are `fill(n)`");
                }
                Value::VNum(*value)
            }
            AstValue::Color { rgba, opacity, pos } => {
                let c = Rgba(*rgba);
                let c = opacity.map_or(c, |o| c.with_opacity(o));
                self.stop_value(c, *pos, key, line)
            }
            AstValue::Str { text, key: term, .. } => {
                // Asset paths carry gogo's `res://` scheme, always: the
                // prefix says in the file itself "resolved against the
                // build's resource root and embedded" — a bare spelling
                // would be a second way to write the same thing.
                let lower = text.to_ascii_lowercase();
                let is_asset = IMAGE_EXTS.iter().chain(FONT_EXTS).any(|e| lower.ends_with(e));
                if !text.starts_with("res://") && is_asset {
                    self.error("G0071", line, format!("`{text}` names an asset — write `res://{text}` (every asset path carries the scheme)"));
                }
                if let (Some(k), true) = (term, is_asset) {
                    self.error("G0023", line, format!("`/{k}` on an asset path — a path is never translated; the key goes on text"));
                }
                let text_id = self.intern(text);
                let key_id = match term {
                    Some(k) => self.key(k, text, line),
                    None => NO_KEY,
                };
                Value::VText { text: text_id, key: key_id }
            }
            AstValue::Ref { name, opacity } => {
                // `$item` / `$item.<field>` under a keyed repeat's
                // binding: a per-item value `set_items` fills at clone
                // time (GPAR108) — compiled here so the undefined-$ref
                // check never meets it.
                if let Some(b) = &self.repeat_binding {
                    if name == b || (name.starts_with(b.as_str()) && name.as_bytes().get(b.len()) == Some(&b'.')) {
                        let field = if name.len() > b.len() { name[b.len() + 1..].to_string() } else { String::new() };
                        let id = self.intern(&field);
                        return Value::VItem(id);
                    }
                }
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
                    if key == Some(PropKey::Stops) && word == "transparent" {
                        // Straight-alpha blending fades THROUGH the hidden
                        // RGB; the file says which.
                        self.error("G2006", line, "`transparent` in `stops=` hides which color the fade passes through — spell it: `white/0` or `black/0`");
                    }
                    if let Some(c) = self.palette.get(word).copied() {
                        let c = opacity.map_or(c, |o| c.with_opacity(o));
                        return self.stop_value(c, *pos, key, line);
                    }
                    let pool: Vec<String> = self.palette.keys().cloned().collect();
                    let hint = suggest(word, &pool);
                    let tail = if hint.is_empty() { String::new() } else { format!(" — {hint}") };
                    self.error("G2007", line, format!("`{word}` is not a color{tail}"));
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
        self.prop_checked(p, Some(kind))
    }

    /// A `when` response aimed at a child (`when held flash.opacity=0`):
    /// the child's kind is known only once the tree is built, so the word
    /// check waits for `check_prop_kind` where the response moves to it.
    fn prop_unchecked(&mut self, p: &AstProp) -> Option<Prop> {
        self.prop_checked(p, None)
    }

    /// Is `word` a property `kind` takes? Else an error naming the gogo
    /// word.
    ///
    /// Needed so every spelling passes ONE check — node lines, styles,
    /// theme kind defaults, `when` responses (own and child-aimed), code
    /// controls; a path that skips it is a silent drop. `on:` handlers
    /// are `check_event_kind`'s.
    fn check_prop_kind(&mut self, word: &str, key: PropKey, kind: Kind, line: u32) -> bool {
        if word.starts_with("on:") {
            return true;
        }
        match key {
            PropKey::Other(_) => {
                self.error("G2001", line, unknown_property(word, kind));
                false
            }
            _ if legal(kind, key) => true,
            _ => {
                self.error("G2002", line, wrong_kind(key, kind));
                false
            }
        }
    }

    fn prop_checked(&mut self, p: &AstProp, kind: Option<Kind>) -> Option<Prop> {
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
            "modulate.a" => {
                self.error("G0075", p.line, "`modulate.a` is a Godot engine word — write `opacity`");
                return None;
            }
            _ => {}
        }
        let key = self.prop_key(&p.key);
        if let Some(kind) = kind {
            if !self.check_prop_kind(&p.key, key, kind, p.line) {
                return None;
            }
        }
        // The list kind ships with Grid's `cols=`/`areas=` and Gradient's
        // `stops=` and nothing else — no prop becomes list-accepting by
        // accident (the spec's Values table).
        if let AstValue::List(_) = p.value {
            let takes_list = matches!(key, PropKey::Cols | PropKey::Areas | PropKey::Stops);
            if !takes_list {
                self.error("G0040", p.line, format!("`{}=` doesn't take a list — only Grid's `cols=`/`areas=` and Gradient's `stops=` do", p.key));
                return None;
            }
        }
        if key == PropKey::Stops {
            self.check_stops(&p.value, p.line);
        }
        let mut value = self.value(&p.value, p.line, Some(key));
        // A number key given a word, color or string: refused, never read
        // as "unset" (`opacity=red`, `when held scale=big`).
        if key.is_number() && !matches!(value, Value::VNum(_) | Value::VRef { .. } | Value::VCall { .. }) {
            self.error("G0058", p.line, format!("`{}=` wants a number, not `{}`", p.key, spell(&p.value)));
            return None;
        }
        // Alpha 00 under non-zero color channels is a pigment that can
        // never paint, spelled as if it were a color — the classic
        // source is a WPF/XAML #AARRGGBB literal copied verbatim (gogo
        // hex is #RRGGBBAA, alpha LAST). STATIC pigment only: `when` and
        // anim endpoints keep alpha-0 values (G2004, GPAR62).
        if self.in_when == 0 && matches!(key, PropKey::Background | PropKey::BorderColor) {
            if let AstValue::Color { rgba, opacity: None, pos: None } = &p.value {
                if rgba[3] == 0 && (rgba[0] != 0 || rgba[1] != 0 || rgba[2] != 0) {
                    let from_xaml = format!("#{:02X}{:02X}{:02X}{:02X}", rgba[1], rgba[2], rgba[3], rgba[0]);
                    self.error("G2004", p.line, format!("`{}=#{:02X}{:02X}{:02X}00` is an invisible pigment — alpha 00 with color in the channels; gogo hex is #RRGGBBAA, alpha LAST (a XAML #AARRGGBB literal reads this way — if this color came from XAML it spells `{from_xaml}` here; deliberate invisibility is `#00000000` or `opacity=0`)", p.key, rgba[0], rgba[1], rgba[2]));
                    return None;
                }
            }
        }
        // Word-valued keys are CLOSED sets — without this fence a typo
        // (`when held halign=top`, `visibility=maybe`) compiles clean and
        // silently reads as unset. Alignment words are per-axis and
        // physical (the invented `start`/`end` wait for the RTL
        // milestone, G14). References pass; the runtime maps them.
        let closed_words: Option<&[&str]> = match key {
            PropKey::Halign | PropKey::TextHalign => Some(&["left", "center", "right"]),
            PropKey::Valign | PropKey::TextValign => Some(&["top", "center", "bottom"]),
            PropKey::Visibility => Some(&["visible", "space_only", "gone"]),
            PropKey::Fit => Some(&["contain", "cover", "distort"]),
            PropKey::Allow => Some(&["horizontal", "vertical", "both"]),
            PropKey::Bars => Some(&["auto", "never", "always", "reserve"]),
            PropKey::Select => Some(&["one", "many", "off"]),
            PropKey::TouchTarget | PropKey::KeyboardTarget => Some(&["off"]),
            PropKey::GradientFill => Some(&["linear", "radial"]),
            _ => None,
        };
        if let Some(words) = closed_words {
            let ok = match value {
                Value::VIdent(id) => words.contains(&self.page.str(id)),
                Value::VRef { .. } => true,
                _ => false,
            };
            if !ok {
                self.error("G0057", p.line, format!("`{}=`: want {}", p.key, words.join("/")));
                return None;
            }
        }
        // Bool keys take true/false — without the check `overlay=maybe`
        // reads as not-overlay with a clean build.
        let is_bool = matches!(key, PropKey::Overlay | PropKey::Interactive | PropKey::Wrap | PropKey::ClipText | PropKey::ClipContents | PropKey::Disabled | PropKey::SelectedProp | PropKey::FullRow | PropKey::ToggleMode);
        if is_bool && !matches!(value, Value::VBool(_) | Value::VRef { .. } | Value::VCall { .. }) {
            self.error("G0078", p.line, format!("`{}=` is true or false — got `{}`", p.key, spell(&p.value)));
            return None;
        }
        // `text=25` / `text=live($value)` with a number: the digits, so a
        // value label needs no code to show its number.
        if key == PropKey::Text {
            if let Value::VNum(n) = value {
                let digits: String = if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") };
                value = Value::VText { text: self.intern(&digits), key: NO_KEY };
            }
        }
        Some(Prop { key, value })
    }

    /// The `stops=` list's authoring rules (the addon's
    /// `_apply_gradient_stops`): a non-empty list of colors; one stop is a
    /// solid (G2003 — that's a Rectangle, the rewrite is spelled out);
    /// `@` positions all-or-none and never decreasing. Checked on the
    /// file's spelling so a message can quote it.
    fn check_stops(&mut self, v: &AstValue, line: u32) {
        let AstValue::List(items) = v else {
            self.error("G2005", line, "`stops=` takes the gradient's color list — `stops=[white, black]` spaces evenly; @positions pin entries (`white@0, blue@0.24`)");
            return;
        };
        if items.is_empty() {
            self.error("G2005", line, "`stops=` is empty — a gradient needs colors (`stops=[white, black]`)");
            return;
        }
        let mut positioned = 0;
        let mut prev: Option<f32> = None;
        let mut decreasing: Option<(f32, f32)> = None;
        for it in items {
            let pos = match it {
                AstValue::Color { pos, .. } | AstValue::Ident { pos, .. } => *pos,
                AstValue::Ref { .. } => None,
                other => {
                    self.error("G2005", line, format!("`stops=` entries are colors — got `{}`", spell(other)));
                    return;
                }
            };
            if let Some(p) = pos {
                positioned += 1;
                if let Some(q) = prev {
                    if p < q && decreasing.is_none() {
                        decreasing = Some((q, p));
                    }
                }
                prev = Some(p);
            }
        }
        if items.len() == 1 {
            let c = spell(&items[0]);
            let (name, extras) = self.stops_ctx.clone().unwrap_or_default();
            let name = if name.is_empty() { String::new() } else { format!("{name} ") };
            let drop = if extras.is_empty() { String::new() } else { format!(", dropping gradient-only `{}=`", extras.join("=`, `")) };
            self.error("G2003", line, format!("a one-stop gradient renders as a solid — that's a Rectangle: `Rectangle {name}color={c}`{drop}"));
            return;
        }
        if positioned != 0 && positioned != items.len() {
            self.error("G2005", line, format!("@positions are all-or-none — {positioned} of {} stops pinned; pin every stop (`white@0, gray@0.3, black@1`) or none (even spacing)", items.len()));
        }
        if let Some((q, p)) = decreasing {
            self.error("G2005", line, format!("stop positions never decrease — `@{p}` after `@{q}`"));
        }
    }

    /// A Gradient's own line: `stops=` is required (no hidden ramp — the
    /// box would be invisible), `fill=` is linear or radial, `angle=` aims
    /// linear fills only. The list's own rules: `check_stops`.
    fn gradient_line(&mut self, n: &AstNode, props: &[AstProp]) {
        let find = |k: &str| props.iter().find(|p| p.key == k);
        if find("stops").is_none() {
            self.error("G2005", n.line, "Gradient needs `stops=` — the colors it paints; `stops=[white, black]` fades white to black; one color is a `Rectangle`");
        }
        let fill = find("fill").and_then(|p| p.value.as_ident()).unwrap_or("linear").to_string();
        if !matches!(fill.as_str(), "linear" | "radial") {
            self.error("G2005", n.line, format!("`fill={fill}`: want linear or radial (Godot's square|conic are not drawn by this runtime)"));
        }
        if find("angle").is_some() && fill != "linear" {
            self.error("G2005", n.line, "`angle=` only aims LINEAR gradients — a round fill (radial) has no direction");
        }
    }

    /// `repeat x [as $b] by $b.field`: `as` names the binding (a `$ref`),
    /// `by` keys the item on that binding, and the binding may not shadow
    /// a `$` value in scope (G9002/G9003/G9004, the addon's codes). `by`
    /// is optional here: the `max=N` form has no items to key (GP27).
    fn repeat_clauses(&mut self, n: &AstNode) {
        let binding = match n.clauses.iter().find(|(k, _)| k == "as") {
            Some((_, AstValue::Ref { name, opacity: None })) if !name.contains('.') => name.clone(),
            Some((_, v)) => {
                self.error("G9002", n.line, format!("`as` declares the item binding — `as $name`, sigil and all (got `{}`)", spell(v)));
                return;
            }
            None => "item".to_string(),
        };
        if let Some((_, v)) = n.clauses.iter().find(|(k, _)| k == "by") {
            match v {
                AstValue::Ref { name, .. } if *name == binding || name.starts_with(&format!("{binding}.")) => {}
                other => self.error("G9003", n.line, format!("`by` is a field path on the item binding — `by ${binding}.<field>` (got `{}`)", spell(other))),
            }
        }
        let id = self.page.intern_lookup(&binding);
        let shadows = (id != NO_STR && self.page.theme.iter().any(|t| t.name == id)) || self.scopes.iter().any(|s| s.contains_key(&binding));
        if shadows {
            self.error("G9004", n.line, format!("the item binding `${binding}` would shadow a `$` value in scope — rename it with `as $<other>`"));
        }
    }

    // ----- nodes ----------------------------------------------------------

    /// Compiles `n` (a child of `parent_kind`, or the root) into a temporary
    /// tree; `flatten` lays it into the arena.
    fn node(&mut self, n: &AstNode, parent_kind: Option<Kind>, instance: Option<&AstNode>) -> TmpNode {
        // Widget instance: expand the body with the instance's args in scope.
        if let Some(def) = self.widgets.get(&n.type_word).cloned() {
            // Refused before any expansion: `A -> B -> A` would recurse
            // forever (the addon errors the cycle at bake too; Rust used
            // to abort the whole build with a stack overflow and no
            // message).
            if self.widget_stack.iter().any(|w| w == &n.type_word) {
                let path = self.widget_stack.join(" -> ");
                self.error("G5003", n.line, format!("widget cycle: {path} -> {} — a widget cannot contain itself", n.type_word));
                return TmpNode { node: Node { kind: Kind::Layers, line: n.line, ..Node::EMPTY }, props: Vec::new(), handlers: Vec::new(), whens: Vec::new(), transitions: Vec::new(), children: Vec::new() };
            }
            // A `for=` widget fills its host's body and nothing else —
            // anywhere else its `when` lines would answer the wrong node.
            if let Some(host) = def.host {
                if parent_kind != Some(host) {
                    self.error("G5001", n.line, format!("`{}` is `for={:?}` — it fills a {:?}'s body; instantiate it as the direct body: `{:?} name {{ {} … }}`", n.type_word, host, host, host, n.type_word));
                }
            }
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
            let mut inst_states: Vec<AstNode> = Vec::new();
            for c in &n.children {
                match c {
                    AstItem::Node(k) if k.type_word == "slot" && k.name.is_some() => {
                        fills.named.insert(k.name.clone().unwrap_or_default(), k.children.clone());
                    }
                    AstItem::Node(k) if matches!(k.type_word.as_str(), "when" | "transition") => inst_states.push(k.clone()),
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
            self.style_scopes.push(def.styles.clone());
            self.widget_stack.push(n.type_word.clone());
            if def.control {
                self.control_scopes.insert(self.prefix.join("/"));
            }
            // The widget's own anims, one copy per instance.
            for a in &def.anims {
                self.anim(a);
            }
            // `signals` inside the body: the instance's `on:` wiring binds
            // to that node, not the root (the Slider inside LabeledSlider).
            let on_props: Vec<AstProp> = n.props.iter().filter(|p| p.key.starts_with("on:")).cloned().collect();
            self.instance_on.push(if has_signals_node(&def.body) { Some(on_props) } else { None });
            // Widget-line props bake onto the body root, under its own
            // (`widget X for=Radio, min_height=44` — dropping them would
            // lose the 44 silently).
            let body_baked: AstNode;
            let body: &AstNode = if def.props.is_empty() {
                &def.body
            } else {
                let mut b = def.body.clone();
                let mut props = def.props.clone();
                props.extend(b.props);
                b.props = props;
                body_baked = b;
                &body_baked
            };
            let mut t = self.node(body, parent_kind, Some(n));
            self.instance_on.pop();
            self.style_scopes.pop();
            self.widget_stack.pop();
            // `when`/`transition` blocks written under the INSTANCE line
            // land on the instance root (the shipped _collect_states on
            // the root; GPAR68) — they were dropped without a word.
            for k in &inst_states {
                if k.type_word == "when" {
                    let before = self.diags.len();
                    if let Some(tw) = self.when(k, t.node.kind) {
                        t.whens.push(tw);
                    } else {
                        // A control's OWN `state` word (`when dragging`)
                        // has nothing to bind until contract states land
                        // (GP37) — parked as a warning, never a silent
                        // drop, so copied files keep building.
                        self.diags.truncate(before);
                        self.warning("G0036", k.line, "an instance `when` naming a control's own state waits for contract states (GP37) — the block is parked");
                    }
                } else {
                    let tr = self.transition(k, PENDING_NODE);
                    t.transitions.push(tr);
                }
            }
            // A `for=` widget's scope responses answer the hosting control.
            for w in &def.whens {
                if let Some(mut tw) = self.when(w, t.node.kind) {
                    tw.host = def.host.is_some();
                    t.whens.push(tw);
                }
            }
            // A `for=` widget's `transition` rules time the HOSTING
            // control's flips (resolved at lay time, like its `when`
            // lines); a `control`'s is its default feel — endpoint-less,
            // the weakest rule on the instance root (the shipped
            // contract). A hostless widget's were refused at declaration.
            for tr in &def.transitions {
                if def.control {
                    if !tr.args.is_empty() || !tr.arrows.is_empty() {
                        self.error("G0034", tr.line, "a control declaration's transition is its default feel — no endpoints (`transition duration=0.15, easing=cubic_out`); state pairs go on styles or instances");
                        continue;
                    }
                    let t2 = self.transition(tr, PENDING_NODE);
                    t.transitions.push(t2);
                } else if def.host.is_some() {
                    let t2 = self.transition(tr, HOST_PENDING);
                    t.transitions.push(t2);
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
                let mut pool: Vec<String> = KIND_WORDS.iter().map(|(w, _)| w.to_string()).collect();
                pool.extend(self.widgets.keys().cloned());
                pool.extend(self.options.code_controls.iter().map(|s| s.to_string()));
                let hint = suggest(&n.type_word, &pool);
                let tail = if hint.is_empty() { String::new() } else { format!(" — {hint}") };
                self.error("G7011", n.line, format!("unknown node type `{}`{tail}", n.type_word));
                Kind::Custom
            }
        };
        if kind == Kind::View && parent_kind.is_some() {
            self.error("G7012", n.line, "`View` is the screen root only — inside the tree use `Layers` (free placement) or `Panel` (a styled box)");
        }
        // The Grid lattice is checked at load (E8/GPAR99): the row form
        // walked against `cols=`, the areas form expanded into that same
        // row structure — one lattice downstream.
        let n_grid: AstNode;
        let n: &AstNode = if kind == Kind::Grid {
            match self.grid_structure(n) {
                Some(expanded) => {
                    n_grid = expanded;
                    &n_grid
                }
                None => n,
            }
        } else {
            n
        };
        if kind == Kind::Tiles {
            self.tiles_structure(n);
        }
        if kind == Kind::Seat {
            // A seat is an EMPTY, NAMED socket in a container (the
            // shipped fences, GPAR73): it holds only what a `when` moves
            // into it, and it sizes from its occupant.
            if n.name.is_none() {
                self.error("G1015", n.line, "seat needs a name: `seat sideSeat` — `when <state> { x.seat = sideSeat }` targets it by name");
            } else if n.name.as_deref() == Some("home") {
                self.error("G1015", n.line, "'home' is reserved — in a seat move it names the occupant's declared home; pick another seat name");
            }
            if n.children.iter().any(|c| matches!(c, AstItem::Node(k) if !matches!(k.type_word.as_str(), "when" | "transition" | "theme"))) {
                self.error("G1015", n.line, "a seat is an empty socket — it holds only what a `when` moves into it; declare the occupant in its resting home");
            }
            // FLIPs from the shipped fences (GP18's fixed sockets are
            // standing Rust design): placement words on a seat size the
            // SOCKET itself (`seat socket width=200` under a Layers), so
            // props and free parents stay legal — the kind table still
            // refuses non-box words on a seat.
        }
        if kind == Kind::GridRow && parent_kind != Some(Kind::Grid) {
            self.error("G1002", n.line, "a `row { }` is grid structure — it lives inside a Grid");
        }
        if kind == Kind::Skip && parent_kind != Some(Kind::GridRow) {
            self.error("G1002", n.line, "`skip` is grid structure — one per empty track inside a `row { }` block");
        }
        if kind == Kind::Check && self.choice_depth > 0 {
            self.error("G0068", n.line, "independent toggles don't join exclusive groups — inside `choice`, use Radio (`Check` is standalone on/off)");
        }
        if kind == Kind::Radio && self.choice_depth > 0 && n.name.is_none() {
            self.error("G0069", n.line, "Radio needs a name — the choice reports the chosen NAME (`Radio medium text=\"Medium\"`)");
        }
        // `overlay=true` cuts the box's upward size contribution;
        // `space_only` exists to HOLD a slot — the two contradict.
        if n.props.iter().any(|p| p.key == "overlay" && matches!(p.value, AstValue::Bool(true))) && n.props.iter().any(|p| p.key == "visibility" && p.value.as_ident() == Some("space_only")) {
            self.error("G0078", n.line, "`overlay=true` with `visibility=space_only` — a seat-holder is not an overlay; a hidden overlay is `gone`");
        }
        // A bodied Radio/Check IS its body — the body replaces every
        // native visual, the label included.
        if matches!(kind, Kind::Radio | Kind::Check) && n.props.iter().any(|p| p.key == "text") && n.children.iter().any(|c| matches!(c, AstItem::Node(k) if !matches!(k.type_word.as_str(), "when" | "transition" | "theme"))) {
            self.error("G0068", n.line, format!("`text=` is the native {}'s label — a bodied {} IS its body; put a `Label` inside instead", n.type_word, n.type_word));
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
        if kind == Kind::Empty {
            // The zero-items view holds ONE child (decided 2026-08-31, E4):
            // Godot stacked several, Rust overlaid them, and neither
            // arrangement was written anywhere.
            let kids = n.children.iter().filter(|c| matches!(c, AstItem::Node(k) if !matches!(k.type_word.as_str(), "when" | "transition" | "theme"))).count();
            if kids > 1 {
                self.error("G9007", n.line, format!("`empty {{ }}` holds one child, this one has {kids} — wrap them in a Column or Layers (the arrangement is written, not guessed)"));
            }
            if n.name.is_some() || !n.args.is_empty() || !n.props.is_empty() {
                self.error("G9007", n.line, "`empty { … }` takes nothing on its line — its look lives on the child");
            }
        }
        if kind == Kind::Repeat {
            self.repeat_clauses(n);
        }

        // Props: style base → node props → instance props (later wins on lookup).
        let mut all_props: Vec<AstProp> = Vec::new();
        let mut style_whens: Vec<AstNode> = Vec::new();
        let mut style_transitions: Vec<AstNode> = Vec::new();
        let mut subtree_defaults: Vec<Prop> = Vec::new();
        // The body's own `style=` sees the widget's scoped styles; the
        // instance line's does NOT — the scope belongs to the body (GP32,
        // the shipped rule).
        let styles: Vec<(String, bool)> = n.styles.iter().map(|s| (s.clone(), false)).chain(instance.iter().flat_map(|i| i.styles.iter().map(|s| (s.clone(), true)))).collect();
        for (s, from_instance) in &styles {
            let resolved = if *from_instance {
                let top = self.style_scopes.pop();
                let r = self.resolve_style(s, n.line);
                if let Some(top) = top {
                    self.style_scopes.push(top);
                }
                r
            } else {
                self.resolve_style(s, n.line)
            };
            match resolved {
                Some(def) => {
                    if let Some(target) = &def.for_target {
                        if *target != n.type_word {
                            self.error("G3006", n.line, format!("style `{s}` is for {target} — applied here to {}", n.type_word));
                        }
                    }
                    all_props.extend(def.props.iter().cloned());
                    style_whens.extend(def.whens);
                    style_transitions.extend(def.transitions);
                }
                None => {}
            }
        }
        all_props.extend(n.props.iter().cloned());
        for c in &n.children {
            if let AstItem::Node(t) = c {
                if t.type_word == "theme" {
                    // `theme font=…` inside a block: a default for this subtree
                    // (font inherits down the draw walk). A variant block
                    // here (`theme dark { }`) would be a silent no-op.
                    if t.name.is_some() || !t.children.is_empty() || !t.args.is_empty() {
                        self.error("G3006", t.line, "a theme override inside a node is props on one line: `theme font=$x` — theme variants (`theme dark { $name = value }`) go at the top of the file");
                        continue;
                    }
                    // A subtree's text defaults: the words flow down to
                    // the text inside (`Resolved::inherited`), so they are
                    // legal on any box here — checked against Label's row,
                    // the kind that shows text.
                    for p in &t.props {
                        let key = PropKey::from_word(&p.key);
                        if !matches!(key, Some(PropKey::Font | PropKey::FontSize | PropKey::FontColor)) {
                            self.error("G3006", p.line, format!("`theme {}=` inside a block sets a text default for everything under it — the words are `font`, `font_size`, `font_color`", p.key));
                            continue;
                        }
                        if let Some(prop) = self.prop(p, Kind::Label) {
                            subtree_defaults.push(prop);
                        }
                    }
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
                let msg = match a {
                    AstValue::Ident { word, .. } if word == "true" || word == "false" => format!("`{word}` is a value, not a name — a property is `key=value` (`selected={word}`)"),
                    // Needed so a state word never doubles as a node's
                    // address: `transition base -> held` next to a child
                    // called `base` would read as two things (reserved
                    // everywhere, as `match` is in Rust — E5, 2026-08-31).
                    AstValue::Ident { word, .. } if word == "base" || word == "any" => format!("`{word}` is reserved (a state endpoint) and cannot name a node — pick another name"),
                    other => format!("unexpected `{}` on `{}` — a property is `key=value`", spell(other), n.type_word),
                };
                self.error("G0046", n.line, msg);
            }
        }
        if kind == Kind::Slot {
            tmp.node.auto = self.automatic(n, parent_kind);
        }
        if kind == Kind::Gradient {
            self.gradient_line(n, &all_props);
            let extras: Vec<String> = all_props.iter().filter(|p| matches!(p.key.as_str(), "fill" | "angle" | "repeat" | "from" | "to")).map(|p| p.key.clone()).collect();
            self.stops_ctx = Some((n.name.clone().unwrap_or_default(), extras));
        }

        let mut placement_props: Vec<Prop> = Vec::new();
        for p in &all_props {
            // `live()` drives this prop from code; a `when` on the
            // same node holding the same key is a second writer — they
            // would fight on every flip (the shipped G4003).
            if let AstValue::Call { name: cname, .. } = &p.value {
                if cname == "live" {
                    let fights = n.children.iter().any(|c| match c {
                        AstItem::Node(k) if k.type_word == "when" => k.props.iter().any(|wp| wp.key == p.key) || k.children.iter().any(|ci| matches!(ci, AstItem::Assign { key, dollar: false, .. } if *key == p.key)),
                        _ => false,
                    });
                    if fights {
                        self.error("G4003", p.line, format!("'{}' is driven by `live(...)` AND held by a `when` on this node — two writers would fight on every flip", p.key));
                    }
                }
            }
            if p.key == "seat" {
                // Re-parenting is view geometry, not a node's own look
                // (GPAR73, the shipped rule).
                self.error("G1016", p.line, "a seat move is a view-level `when` response — `when <state> { node.seat = seatName }`; the declared home is where the node rests");
                continue;
            }
            if kind == Kind::Repeat && p.key == "capacity" {
                continue;
            }
            if kind == Kind::Repeat && p.key == "max" {
                // A max that grows would lie and a max that refused would
                // truncate rows on screen — the reservation word says
                // what it is (user design 2026-08-31).
                self.error("G0035", p.line, "`max=` is spelled `capacity=` — a reservation the rows grow past, never a ceiling");
                continue;
            }
            if let Some(action) = p.key.strip_prefix("on:") {
                if let Some(h) = self.handler(action, p) {
                    self.check_event_kind(action, kind, p.line);
                    tmp.handlers.push(h);
                }
                continue;
            }
            let Some(prop) = self.prop(p, kind) else { continue };
            // Cell words belong to the PARENT's lattice. Needed because a
            // cell word off its lattice lays out nothing — refused, naming
            // the parent it sits in.
            if GRID_CELL.contains(&prop.key) && !matches!(parent_kind, Some(Kind::Grid | Kind::GridRow)) {
                self.error("G2002", p.line, format!("`{}=` is a Grid cell word — this box sits in a {}", p.key, parent_name(parent_kind)));
                continue;
            }
            if TILES_CELL.contains(&prop.key) && parent_kind != Some(Kind::Tiles) {
                self.error("G2002", p.line, format!("`{}=` is a Tiles cell word — this box sits in a {}", p.key, parent_name(parent_kind)));
                continue;
            }
            if prop.key.is_placement() {
                placement_props.push(prop);
            } else {
                tmp.props.push(prop);
            }
        }
        self.stops_ctx = None;
        tmp.props.extend(subtree_defaults);
        tmp.node.placement = self.placement(&placement_props, n.line, parent_kind, kind);
        // Start states the file declares; the Runtime seeds from them.
        for prop in &tmp.props {
            match (prop.key, prop.value) {
                (PropKey::SelectedProp, Value::VBool(true)) => tmp.node.initial_states |= SELECTED,
                // `Toggle value=1` starts ON (shipped D113): the declared
                // fraction is birth truth — no phantom change event.
                (PropKey::Value, Value::VNum(v)) if tmp.node.kind == Kind::Toggle && v >= 0.5 => tmp.node.initial_states |= SELECTED,
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
                    self.choice_depth += 1;
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
                    self.choice_depth -= 1;
                    if radios == 0 {
                        self.error("G0069", c.line, "choice holds no Radio — nothing to be exclusive about (nested `Row`/`Column` layout is fine)");
                    }
                    if !found {
                        self.error("G0069", c.line, format!("`choice value={}`: no Radio of that name in the group", start.unwrap_or_default()));
                    }
                }
                // `repeat x [capacity=N] { … }` (counted — `set_count` +
                // `set_text`) and `repeat x by $item.key` (keyed —
                // `set_items`, GPAR108) both build ONE hidden template;
                // the Runtime clones it into row slots on demand.
                // `capacity=` is the reservation the first growth jumps
                // to, never a ceiling — nothing truncates (user design
                // 2026-08-31). Only `empty { }` shows until data arrives.
                AstItem::Node(c) if kind == Kind::Repeat && c.type_word != "empty" => {
                    let repeat_name = n.name.clone().unwrap_or_else(|| "repeat".to_string());
                    let binding = n.clauses.iter().find(|(k, _)| k == "as").and_then(|(_, v)| if let AstValue::Ref { name, .. } = v { Some(name.clone()) } else { None }).unwrap_or_else(|| "item".to_string());
                    // A widget item with no instance name is `item`, so its
                    // parts are addressable (`x/0/item/knob` once cloned).
                    let named: AstNode;
                    let template: &AstNode = if c.name.is_none() && self.widgets.contains_key(&c.type_word) {
                        named = AstNode { name: Some("item".to_string()), ..c.clone() };
                        &named
                    } else {
                        c
                    };
                    if self.pending_repeats.iter().any(|(n2, ..)| *n2 != NO_STR && *n2 == tmp.node.name) {
                        self.error("G9007", c.line, "a repeat's template is one root — wrap the rows in a Column or Layers");
                        continue;
                    }
                    let capacity = n.props.iter().find(|p| p.key == "capacity").and_then(|p| p.value.as_num()).unwrap_or(0.0).max(0.0) as u32;
                    let by = n.clauses.iter().find(|(k, _)| k == "by").and_then(|(_, v)| if let AstValue::Ref { name, .. } = v { Some(name.clone()) } else { None });
                    let keyed = by.is_some();
                    let by_id = by.and_then(|b| b.strip_prefix(binding.as_str()).and_then(|r| r.strip_prefix('.')).map(str::to_string)).map_or(NO_STR, |f| self.intern(&f));
                    self.prefix.push(format!("{repeat_name}/item"));
                    self.repeat_binding = Some(binding.clone());
                    let child = self.node(template, Some(kind), None);
                    self.repeat_binding = None;
                    self.prefix.pop();
                    let binding_id = self.intern(&binding);
                    self.pending_repeats.push((tmp.node.name, binding_id, by_id, capacity, keyed));
                    tmp.children.push(child);
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
        // A written box of 1 epx or less is an anchor, not a container: it
        // clips its content like any written size, so a child hanging off
        // it vanishes unless the file says the child is a satellite.
        let anchor = matches!(tmp.node.placement.width, Len::Exact(w) if w <= 1.0) && matches!(tmp.node.placement.height, Len::Exact(h) if h <= 1.0);
        if anchor && tmp.children.iter().any(|c| !c.props.iter().any(|p| p.key == PropKey::Overlay && p.value == Value::VBool(true))) {
            let name = if tmp.node.name == NO_STR { n.type_word.clone() } else { self.page.str(tmp.node.name).to_string() };
            self.warning("G0074", n.line, format!("`{name}` is a 1x1 box: it clips its children — mark the parts that hang off it `overlay=true` (satellites, not content)"));
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
    fn automatic(&mut self, n: &AstNode, parent_kind: Option<Kind>) -> Auto {
        let by = n.clauses.iter().find(|(k, _)| k == "by").map(|(_, v)| v.clone());
        let range = n.clauses.iter().find(|(k, _)| k == "in").map(|(_, v)| v.clone());
        let Some(kind_prop) = n.props.iter().find(|p| p.key == "automatic") else {
            if by.is_some() || range.is_some() {
                self.error("G5107", n.line, "`by`/`in` couple a slot to an `automatic=` driver — this slot declares none");
            }
            return Auto::NoAuto;
        };
        let kind = kind_prop.value.as_ident().unwrap_or("").to_string();
        let num = |c: &mut Self, v: &AstValue| -> f32 {
            match c.value(v, n.line, None) {
                Value::VNum(x) => x,
                _ => 0.0,
            }
        };
        // The coupling is fully spelled and type-checked (D134): v1 drives
        // from `$value` over `$min..$max`, a presence driver is a state word.
        match kind.as_str() {
            "length" | "position" => {
                // Driven geometry must stay relayout-local: a measuring
                // host would re-lay out the whole page every frame
                // (G5105, the shipped fence).
                if matches!(parent_kind, Some(Kind::Column | Kind::Row | Kind::Flow | Kind::Grid | Kind::Tiles | Kind::Scroll)) {
                    self.error("G5105", n.line, format!("'{}' geometry is automatic — it can't live in a {:?}, which measures children on that axis; seat the slot in a free host (a named slot region, Layers, Circle)", n.name.clone().unwrap_or_default(), parent_kind.unwrap_or(Kind::Column)));
                }
                // The driven aspect has ONE writer — the driver (§4.5,
                // the shipped G3002): the slot line must not place it.
                let fenced: &[&str] = if kind == "length" { &["width", "height"] } else { &["margin_left", "margin_top", "margin_right", "margin_bottom", "x", "y"] };
                for fp in &n.props {
                    if fenced.contains(&fp.key.as_str()) {
                        self.error("G3002", fp.line, format!("slot '{}' moves by itself — `{}=` is the driver's to write", n.name.clone().unwrap_or_default(), fp.key));
                    }
                }
                let Some(by) = by else {
                    self.error("G5107", n.line, format!("`automatic={kind}` wants `by $value in $min..$max`"));
                    return Auto::NoAuto;
                };
                if !matches!(&by, AstValue::Ref { name, .. } if name == "value") {
                    self.error("G5107", n.line, format!("`automatic={kind}` v1 drives from `$value` — `by $value` (got `{}`)", spell(&by)));
                    return Auto::NoAuto;
                }
                let Some(AstValue::Range(lo, hi)) = range else {
                    self.error("G5107", n.line, format!("`automatic={kind}` needs its range — `in $min..$max`"));
                    return Auto::NoAuto;
                };
                match (&*lo, &*hi) {
                    (AstValue::Ref { name: l, .. }, AstValue::Ref { name: h, .. }) if l == "min" && h == "max" => {}
                    (AstValue::Ref { .. }, AstValue::Ref { .. }) => {
                        self.error("G5107", n.line, format!("`automatic={kind}` normalizes over `$min..$max` — `in $min..$max` (got `in {}..{}`)", spell(&lo), spell(&hi)));
                        return Auto::NoAuto;
                    }
                    _ => {
                        self.error("G5107", n.line, format!("`in` spells the driver's range as two `$params` — `in $min..$max` (got `in {}..{}`)", spell(&lo), spell(&hi)));
                        return Auto::NoAuto;
                    }
                }
                let v = num(self, &by);
                let lo = num(self, &lo);
                let hi = num(self, &hi);
                let f = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
                if kind == "length" { Auto::Length(f) } else { Auto::Position(f) }
            }
            "presence" => {
                if range.is_some() {
                    self.error("G5107", n.line, "`automatic=presence`: a bool has no range — drop the `in` clause");
                }
                match &by {
                    Some(AstValue::Ident { .. }) => {}
                    Some(other) => self.error("G5107", n.line, format!("a presence driver is a state word — `by` names one (no `$`): `by selected` (got `{}`)", spell(other))),
                    None => self.error("G5107", n.line, "`automatic=presence` wants `by <state>` (`by selected`)"),
                }
                Auto::Presence(false)
            }
            other => {
                self.error("G5107", n.line, format!("`automatic={other}`: want length, position or presence"));
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
        let styles = self.style_scopes.pop();
        let widget = self.widget_stack.pop();
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
        if let Some(s) = styles {
            self.style_scopes.push(s);
        }
        if let Some(w) = widget {
            self.widget_stack.push(w);
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
            // A `for=` widget's rule watches the hosting control (the
            // nearest press surface above), exactly like its `when` lines.
            t.node = if t.node == HOST_PENDING {
                let mut a = parent;
                while a != NO_NODE && !self.page.nodes[a as usize].kind.is_press_surface() {
                    a = self.page.nodes[a as usize].parent;
                }
                if a == NO_NODE { id } else { a }
            } else {
                id
            };
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

    /// Wiring a gesture event where it can never fire is an error, so the
    /// on-one-node conflict between `on:pointer` and the interpreted
    /// vocabulary cannot be written at all: a press surface refuses
    /// `pointer`, a passive box refuses `press`/`release`/`activate` —
    /// otherwise the pointer wire would win the finger and the others
    /// would sit dead without a word.
    fn check_event_kind(&mut self, event: &str, kind: Kind, line: u32) {
        match event {
            // A Slider brackets its drag with press/release (no activate).
            "press" | "release" if !kind.is_press_surface() && kind != Kind::Slider => {
                self.error("G0072", line, format!("`on:{event}` fires only on a press surface (Button, Check, Toggle, Radio, ListItem) or a Slider — a {kind:?} never emits it"));
            }
            "change" if kind == Kind::Radio => {
                self.error("G0072", line, "`change` lives on the `choice` group — the value is \"which one\"; wire `on:change=` on the choice line (a Radio exposes `when selected`)");
            }
            "activate" if !kind.is_press_surface() => {
                self.error("G0072", line, format!("`on:{event}` fires only on a press surface (Button, Check, Toggle, Radio, ListItem) — a {kind:?} never emits it"));
            }
            "pointer" if kind.is_press_surface() || matches!(kind, Kind::Scroll | Kind::Slider | Kind::Input) => {
                self.error("G0072", line, format!("`on:pointer` takes the raw finger, so a {kind:?}'s own gestures would never fire — put the pointer surface on a passive box (Layers, Panel)"));
            }
            _ => {}
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
                "touch" | "gui_input" => "pointer",
                _ => "",
            };
            if hint.is_empty() {
                self.error("G0049", p.line, format!("`on:{event}` — raw signals are gone (§7); the events are activate, press, release, change, edit, show, hide, pointer"));
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
                    "play" => Action::Play(self.anim_ref(arg, p.line)),
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

    /// The shipped seat-move fences (GPAR73): the value names a declared
    /// seat (or `home`), the occupant is a plain node whose resting home
    /// is a container, and a move never enters its own subtree. `Some` =
    /// the seat's name id (`home` included); `None` = refused, error out.
    fn seat_move_ok(&mut self, occupant: NodeId, prop: &Prop, line: u32) -> Option<StrId> {
        let oname = self.page.str(self.page.nodes[occupant as usize].name).to_string();
        let Value::VIdent(vid) = prop.value else {
            self.error("G1016", line, format!("`{oname}.seat` names a declared seat — `{oname}.seat = sideSeat`"));
            return None;
        };
        if self.page.nodes[occupant as usize].kind == Kind::Seat {
            self.error("G1016", line, "a seat can't be reseated — move the occupant node");
            return None;
        }
        // FLIP from the shipped home-must-be-container fence: the Rust
        // dialect PARKS occupants under free parents (D263; caption_page
        // parks panelMain under the View) and the runtime seats them.
        let vname = self.page.str(vid).to_string();
        if vname == "home" {
            return Some(vid);
        }
        let target = self.page.find(&vname).filter(|s| self.page.nodes[*s as usize].kind == Kind::Seat);
        let Some(sid) = target else {
            let seats: Vec<String> = self.page.nodes.iter().filter(|nn| nn.kind == Kind::Seat && nn.name != NO_STR).map(|nn| self.page.str(nn.name).to_string()).collect();
            let hint = if seats.is_empty() { "declare one inside a container: `seat sideSeat`".to_string() } else { format!("the declared seats: {}", seats.join(", ")) };
            self.error("G1016", line, format!("'{vname}' is not a declared seat — {hint}"));
            return None;
        };
        // Cycle: walk the seat's parents — `subtree_end` over-reaches
        // into later siblings (the flatten order), a range test would
        // refuse legal sibling seats.
        let mut p = sid;
        while p != NO_NODE {
            if p == occupant {
                self.error("G1016", line, format!("seat '{vname}' sits inside '{oname}' — a node can't move into its own subtree"));
                return None;
            }
            p = self.page.nodes[p as usize].parent;
        }
        Some(vid)
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
            if p.key == "seat" || p.key.ends_with(".seat") {
                // Same rule as the node line: re-parenting is view
                // geometry (GPAR73).
                self.error("G1016", p.line, "a seat move is a view-level `when` response — top level: `when <state> { node.seat = seatName }`");
                continue;
            }
            if let Some((node_name, prop_name)) = split_target(&p.key) {
                let ap = AstProp { key: prop_name.to_string(), value: p.value.clone(), line: p.line };
                if let Some(prop) = {
                    self.in_when += 1;
                    let r = self.prop_unchecked(&ap);
                    self.in_when -= 1;
                    r
                } {
                    descendants.push((node_name.to_string(), prop, prop_name.to_string(), p.line));
                }
            } else if let Some(prop) = {
                self.in_when += 1;
                let r = self.prop(p, kind);
                self.in_when -= 1;
                r
            } {
                props.push(prop);
            }
        }
        Some(WhenTmp { state, line: w.line, props, descendants, host: false })
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
        // Per-axis physical words only (spec: halign=left/center/right,
        // valign=top/center/bottom) — a cross-axis word would silently
        // re-mean (`halign=top` pinned left), and the logical `start`/`end`
        // wait for the RTL milestone (G14) where margins and direction
        // flip together. Backstop only: `prop_checked` refuses these first.
        let horizontal = key == "halign";
        if let Value::VIdent(id) = v {
            match (self.page.str(id), horizontal) {
                ("left", true) | ("top", false) => return Align::Start,
                ("center", _) => return Align::Center,
                ("right", true) | ("bottom", false) => return Align::End,
                _ => {}
            }
        }
        let expected = if horizontal { "left/center/right" } else { "top/center/bottom" };
        self.error("G0057", line, format!("`{key}=`: want {expected}"));
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
            self.error("G1011", line, "`halign=center` with `width=fill`: fill leaves nothing to position (use `hug`, or a `max_width` cap)");
        }
        if p.valign == Align::Center && matches!(p.height, Len::Fill(_)) && p.max_height.is_none() {
            self.error("G1011", line, "`valign=center` with `height=fill`: fill leaves nothing to position (use `hug`, or a `max_height` cap)");
        }
        if let (Some(max), true) = (p.max_width, p.min_width > 0.0) {
            if p.min_width > max {
                self.error("G1013", line, "`min_width` above `max_width`");
            }
        }
        if p.margin.iter().any(Option::is_some) && parent.map(|k| k.is_stack() || matches!(k, Kind::Flow | Kind::Grid | Kind::GridRow | Kind::Tiles)).unwrap_or(false) {
            self.error("G1014", line, "margins under a Column/Row/Flow/Grid are an error — space children with `spacing=` / `gap` (or `padding` on the parent)");
        }
        if p.margin[0].is_some() && p.margin[2].is_some() && matches!(p.width, Len::Exact(_)) {
            self.error("G1006", line, "both horizontal margins and an exact `width` over-constrain the axis");
        }
        let _ = kind;
        p
    }

    // ----- finish ---------------------------------------------------------

    fn finish(&mut self) {
        // Repeats: name -> node id; the non-`empty` child is the hidden
        // template the Runtime clones into row slots (GPAR108/capacity).
        // The template's `$item.<field>` sites are recorded here, once —
        // a reused slot re-binds through them with no marker left on it.
        for (name, binding, by, capacity, keyed) in std::mem::take(&mut self.pending_repeats) {
            if name == NO_STR {
                continue;
            }
            let Some(node) = self.page.nodes.iter().position(|n| n.name == name && n.kind == Kind::Repeat) else { continue };
            let node = node as NodeId;
            let template = self.page.children(node).map(|i| i as NodeId).find(|c| self.page.nodes[*c as usize].kind != Kind::Empty);
            if let Some(template) = template {
                let first = self.page.bind_sites.len() as u32;
                let t_end = self.page.nodes[template as usize].subtree_end;
                for ni in template..t_end {
                    let (p0, pc) = self.page.nodes[ni as usize].props;
                    for k in 0..pc {
                        if let Value::VItem(f) = self.page.props[(p0 + k) as usize].value {
                            self.page.bind_sites.push(BindSite { node_off: ni - template, prop_off: k, field: f });
                        }
                    }
                }
                let count = self.page.bind_sites.len() as u32 - first;
                self.page.repeats.push(RepeatInfo { node, binding, by, template, capacity, keyed, sites: (first, count) });
            }
        }
        // The D103 input model: everything passive is transparent, always —
        // only `interactive=true` declares a surface, a wired handler alone
        // never does. So `on:pointer` on a node that never declared itself
        // interactive is a wire nothing can ever reach; error now, not a
        // control that silently ignores every finger. (Checked here so
        // style-carried props are already merged into the node's range.)
        let pointer_ev = self.page.intern("pointer");
        for i in 0..self.page.nodes.len() {
            let node = &self.page.nodes[i];
            let (hf, hn) = node.handlers;
            if !self.page.handlers[hf as usize..(hf + hn) as usize].iter().any(|h| h.event == pointer_ev) {
                continue;
            }
            if self.page.prop(i as NodeId, PropKey::Interactive) != Some(Value::VBool(true)) {
                let line = node.line;
                self.error("G0073", line, "`on:pointer` needs `interactive=true` on the same node — passive boxes are transparent to input, always; the wire alone claims nothing");
            }
        }
        // A Radio outside any `choice` has nothing to be exclusive with:
        // refused, as the addon does (Rust used to invent sibling
        // exclusivity and report through the parent, GPAR22).
        for i in 0..self.page.nodes.len() {
            let n = &self.page.nodes[i];
            if n.kind == Kind::Radio && n.choice == NO_CHOICE {
                let line = n.line;
                self.error("G0069", line, "exclusivity needs a choice scope — Radio lives inside `choice { }` (`choice size value=medium { Radio small text=\"Small\"; … }`); an independent toggle is `Check`");
            }
        }
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
        let mut seat_claims: Vec<(NodeId, StrId, State, u32)> = Vec::new();
        let mut screen_writes: Vec<(NodeId, PropKey, State, u32)> = Vec::new();
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
                if let Some(prop) = {
                    self.in_when += 1;
                    let r = self.prop(&ap, kind);
                    self.in_when -= 1;
                    r
                } {
                    screen_writes.push((id, prop.key, state, *l));
                    if prop.key == PropKey::SeatOf {
                        match self.seat_move_ok(id, &prop, *l) {
                            Some(vid) => seat_claims.push((id, vid, state, *l)),
                            None => continue,
                        }
                    }
                    // One entry per BLOCK (state + line), not per state: two
                    // `when open` blocks straddling a competing block must
                    // each keep their own line for the later-line tie-break.
                    match self.node_whens[id as usize].iter_mut().find(|w| w.state == state && w.line == line) {
                        Some(w) => w.props.push(prop),
                        None => self.node_whens[id as usize].push(WhenTmp { state, line, props: vec![prop], descendants: Vec::new(), host: false }),
                    }
                }
            }
        }
        // A `width=fill` wrap label under a content-sized chain wraps
        // at a sliver — the row's leftover for it is the label's own raw
        // minimum, about one word (G1004's leftover flavor, GPAR77):
        // bind the chain or cap it. USER SEES (before): a one-word-wide
        // caption column, silently.
        for i in 0..self.page.nodes.len() {
            let nd = self.page.nodes[i];
            if nd.kind != Kind::Label || !matches!(nd.placement.width, Len::Fill(_)) {
                continue;
            }
            if self.page.prop(i as NodeId, PropKey::Wrap) != Some(Value::VBool(true)) {
                continue;
            }
            let mut a = nd.parent;
            let mut sliver = false;
            while a != NO_NODE {
                let ap = self.page.nodes[a as usize].placement;
                match ap.width {
                    Len::Exact(_) | Len::Fill(_) => break,
                    Len::Hug(_) => {
                        sliver = true;
                        break;
                    }
                    Len::Unset => {
                        if ap.max_width.is_some() || ap.max_width_fill {
                            break;
                        }
                        if ap.halign != Align::AlignUnset {
                            sliver = true;
                            break;
                        }
                        a = self.page.nodes[a as usize].parent;
                    }
                }
            }
            if sliver {
                let name = if nd.name == NO_STR { "the label".to_string() } else { format!("`{}`", self.page.str(nd.name)) };
                self.warning("G1004", nd.line, format!("`wrap=true` on {name} wraps at a sliver — it fills the leftover of its row, but the chain above sizes to content, so that leftover is the label's own raw minimum (about one word); bind the chain (`width=fill` down to the label) or cap it (`max_width=fill`)"));
            }
        }
        // One screen vocabulary per posed target (G3005, the shipped
        // fence): a class and an orientation hold at the SAME time on
        // every screen, so of two single-vocabulary writers the earlier
        // line NEVER wins — dead code, not a ranking; the composed pair
        // (`when screen.<orientation> screen.<class>`) says the
        // intersection.
        fn vocab(s: State) -> Option<bool> {
            match s {
                State::ScreenOrientation(_) => Some(true),
                State::ScreenClass(_) => Some(false),
                _ => None,
            }
        }
        let mut vocab_done: Vec<NodeId> = Vec::new();
        for i in 0..screen_writes.len() {
            for j in 0..i {
                let (na, ka, sa, la) = screen_writes[i];
                let (nb, kb, sb, _) = screen_writes[j];
                if na != nb || vocab_done.contains(&na) {
                    continue;
                }
                let (Some(va), Some(vb)) = (vocab(sa), vocab(sb)) else { continue };
                if va == vb {
                    continue;
                }
                let clash = ka == kb || (ka.is_placement() && kb.is_placement()) || (ka == PropKey::SeatOf && kb == PropKey::SeatOf);
                if clash {
                    vocab_done.push(na);
                    let nname = self.page.str(self.page.nodes[na as usize].name).to_string();
                    self.error("G3005", la, format!("'{nname}' is posed from BOTH screen vocabularies — an orientation and a class can hold at the same time, so the earlier line would never win; pick one vocabulary, or say the intersection: `when screen.<orientation> screen.<class> {{ … }}`"));
                }
            }
        }
        // One occupant per seat and one writer per occupant, per
        // CONDITION (GPAR73). FLIP from the shipped cross-vocabulary
        // fence: an arrangement claim and a screen claim on one seat are
        // legal here — the flattened placement rule (E2: equal rank →
        // later line wins) settles them, and brush_options' previewAbove
        // seat depends on exactly that ranking.
        let home_id = self.page.intern_lookup("home");
        for i in 0..seat_claims.len() {
            for j in 0..i {
                let (oa, sa, ka, la) = seat_claims[i];
                let (ob, sb, kb, _) = seat_claims[j];
                if ka != kb {
                    continue;
                }
                if oa == ob {
                    let oname = self.page.str(self.page.nodes[oa as usize].name).to_string();
                    self.error("G1016", la, format!("two `when` blocks both set '{oname}.seat' under conditions that can hold together"));
                } else if sa == sb && sa != home_id {
                    let sname = self.page.str(sa).to_string();
                    let bname = self.page.str(self.page.nodes[ob as usize].name).to_string();
                    self.error("G1016", la, format!("seat '{sname}' already receives '{bname}' — one occupant per seat"));
                }
            }
        }
        // Descendant responses move to the named node, watching their owner.
        let mut whens = std::mem::take(&mut self.node_whens);
        let mut moved: Vec<(usize, usize, WhenTmp)> = Vec::new();
        for (owner, list) in whens.iter_mut().enumerate() {
            for w in list.iter_mut() {
                for (name, prop, word, line) in std::mem::take(&mut w.descendants) {
                    let end = self.page.nodes[owner].subtree_end as usize;
                    // Widget-body names carry their instance prefix (`inst/name`).
                    let named = |i: &usize| {
                        let full = if self.page.nodes[*i].name != NO_STR { self.page.str(self.page.nodes[*i].name) } else { "" };
                        full == name || full.rsplit('/').next() == Some(name.as_str())
                    };
                    // Own subtree first; then anywhere — the shipped
                    // scope resolves targets through the named map, so a
                    // SIBLING shield is a legal target (GPAR83).
                    let target = (owner + 1..end).find(named).or_else(|| (0..self.page.nodes.len()).find(|i| *i != owner && named(i)));
                    match target {
                        // The word check a node line gets, now that the
                        // target's kind is known. Needed so a misspelled
                        // response is a build error, not a silently dead
                        // line (docs/todo-gogo-parity.md GPAR82).
                        Some(t) if !self.check_prop_kind(&word, prop.key, self.page.nodes[t].kind, line) => {}
                        Some(t) => moved.push((t, owner, WhenTmp { state: w.state, line: w.line, props: vec![prop], descendants: Vec::new(), host: w.host })),
                        None => {
                            let line = self.page.nodes[owner].line;
                            self.error("G0060", line, format!("`when … {name}.…`: no node named `{name}`"));
                        }
                    }
                }
            }
        }
        let mut of: Vec<Vec<NodeId>> = whens.iter().map(|l| l.iter().map(|w| if w.host { HOST_PENDING } else { NO_NODE }).collect()).collect();
        for (target, owner, w) in moved {
            // The DECLARING node owns the response wherever the target
            // lives (GPAR83, the shipped scope rule): climb from IT to
            // the press surface whose state the `when` watches — a
            // sibling target no longer parks the response on the View.
            let mut of_node = owner as NodeId;
            while of_node != NO_NODE && !self.page.nodes[of_node as usize].kind.is_press_surface() && self.page.nodes[of_node as usize].parent != NO_NODE {
                of_node = self.page.nodes[of_node as usize].parent;
            }
            let host = w.host;
            whens[target].push(w);
            of[target].push(if host { HOST_PENDING } else { of_node });
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
        // An overlay `fill` axis under a hug parent that no non-overlay
        // child sizes: nothing determines the extent (the spec's overlay
        // law) — refused rather than laid out 0-wide.
        for i in 0..self.page.nodes.len() {
            let node = &self.page.nodes[i];
            if !matches!(self.page.prop(i as NodeId, PropKey::Overlay), Some(Value::VBool(true))) {
                continue;
            }
            let parent = node.parent;
            if parent == NO_NODE {
                continue;
            }
            let pp = &self.page.nodes[parent as usize].placement;
            let siblings_size: [bool; 2] = {
                let p = &self.page.nodes[parent as usize];
                let mut any = [false, false];
                for s in p.children.0..p.children.0 + p.children.1 {
                    if s as usize == i || matches!(self.page.prop(s, PropKey::Overlay), Some(Value::VBool(true))) {
                        continue;
                    }
                    any = [true, true];
                }
                any
            };
            let np = &self.page.nodes[i].placement;
            let hug_w = matches!(pp.width, Len::Hug(_) | Len::Unset) && pp.min_width <= 0.0;
            let hug_h = matches!(pp.height, Len::Hug(_) | Len::Unset) && pp.min_height <= 0.0;
            if matches!(np.width, Len::Fill(_)) && hug_w && !siblings_size[0] || matches!(np.height, Len::Fill(_)) && hug_h && !siblings_size[1] {
                self.error("G0078", self.page.nodes[i].line, "an overlay `fill` axis under a hug parent with no non-overlay child — nothing determines the size; give the parent a size or the overlay an exact one");
            }
        }
        // The host's own `when` replaces a `for=` widget's response for
        // that state (the shipped `merge_defs`: nearer scope wins, per
        // state) — collect what each host authored itself, then drop the
        // widget-sourced whens it overrides.
        let mut authored: Vec<(NodeId, State)> = Vec::new();
        for (i, list) in whens.iter().enumerate() {
            for (j, w) in list.iter().enumerate() {
                if !w.host {
                    let owner = if of[i][j] == NO_NODE { i as NodeId } else { of[i][j] };
                    authored.push((owner, w.state));
                }
            }
        }
        // Whens pool, contiguous per node.
        for (i, list) in whens.into_iter().enumerate() {
            let first = self.page.whens.len() as u32;
            for (j, w) in list.into_iter().enumerate() {
                if w.host && authored.contains(&(of[i][j], w.state)) {
                    continue;
                }
                let pf = self.page.props.len() as u32;
                self.page.props.extend(w.props);
                let pc = self.page.props.len() as u32 - pf;
                self.page.whens.push(When { state: w.state, props: (pf, pc), of: of[i][j], line: w.line });
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
                let mut pool: Vec<String> = self.page.theme.iter().map(|t| self.page.str(t.name).to_string()).collect();
                for s in &self.scopes {
                    pool.extend(s.keys().cloned());
                }
                let hint = {
                    let h = suggest(word.as_str(), &pool);
                    if !h.is_empty() {
                        h
                    } else if pool.is_empty() {
                        "in scope: (nothing)".to_string()
                    } else {
                        format!("in scope: {}", pool.join(", "))
                    }
                };
                self.error("G6001", line, format!("`${word}` is not a theme value or widget parameter — {hint}"));
            }
        }
        self.one_writer_guard();
    }

    /// D63's one-writer guard (Godot G4003): a `when` in a scope holds a
    /// motion word on a target while a `transition … anim=` in the SAME
    /// scope drives the same channel on it — two writers would fight on
    /// every flip and the loser flickers, so the file is refused. Only the
    /// static pair is visible here; an anim a code `play()` starts over a
    /// standing `when` is ordered by the runtime instead (the anim is the
    /// top writer while it plays).
    fn one_writer_guard(&mut self) {
        let mut found: Vec<(u32, String)> = Vec::new();
        for t in &self.page.transitions {
            if t.anim == NO_STR {
                continue;
            }
            for lane in self.page.anims.iter().filter(|a| a.anim == t.anim) {
                let Some(channels) = Channel::parse(self.page.str(lane.channel)) else { continue };
                let target = &self.page.nodes[lane.node as usize];
                for w in &self.page.whens[target.whens.0 as usize..(target.whens.0 + target.whens.1) as usize] {
                    let owner = if w.of == NO_NODE { lane.node } else { w.of };
                    let node_state = matches!(w.state, State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled);
                    let same_scope = if t.node == NO_NODE { !node_state } else { node_state && owner == t.node };
                    if !same_scope {
                        continue;
                    }
                    let holds = self.page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize].iter().any(|p| Channel::of_key(p.key).iter().any(|c| channels.contains(c)));
                    if holds {
                        let word = self.page.str(lane.channel);
                        let tname = if target.name == NO_STR { format!("{:?}", target.kind) } else { self.page.str(target.name).to_string() };
                        let aname = self.page.str(t.anim);
                        let state = state_word(&self.page, w.state);
                        found.push((t.line, format!("`{word}` on `{tname}` is held by its `when {state}` AND driven by anim `{aname}` from this transition — two writers would fight on every flip; the loser flickers")));
                    }
                }
            }
        }
        for (line, msg) in found {
            self.error("G4003", line, msg);
        }
    }
}

/// The state as the file spells it, for a message.
fn state_word(page: &Page, state: State) -> String {
    match state {
        State::Held => "held".to_string(),
        State::Hovered => "hovered".to_string(),
        State::Focused => "focused".to_string(),
        State::FocusWithin => "focus_within".to_string(),
        State::Selected => "selected".to_string(),
        State::StateDisabled => "disabled".to_string(),
        State::Arrangement(i) => page.arrangements.get(i as usize).map(|a| page.str(*a).to_string()).unwrap_or_else(|| "base".to_string()),
        State::ScreenOrientation(w) | State::ScreenClass(w) => format!("screen.{}", page.str(w)),
        State::ArrangementScreen(i, w) => format!("{} screen.{}", page.arrangements.get(i as usize).map(|a| page.str(*a)).unwrap_or("base"), page.str(w)),
        State::ScreenPair(a, b) => format!("screen.{} screen.{}", page.str(a), page.str(b)),
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
        let p = ok("arrangement wide\ntransition base -> wide duration=0.2, easing=cubic_out\nwhen wide { bar.width = 300 }\nView v {\n Button bar width=200, on:activate=go(wide) { transition any -> held duration=0.1 }\n Button b text=\"Save\", on:activate=save\n}\n");
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

    /// G0074: a written 1x1 anchor with a child that is not `overlay=true`
    /// warns — the clip rule would swallow the child (the gradient marks,
    /// user-caught 2026-08-30); satellites marked overlay pass.
    #[test]
    fn one_by_one_anchor_warns_about_non_overlay_children() {
        assert!(codes("View v { Layers a width=1, height=1 { Rectangle r 10x10, margin_top=20 } }").contains(&"G0074".to_string()));
        assert!(!codes("View v { Layers a width=1, height=1 { Rectangle r 10x10, margin_top=20, overlay=true } }").contains(&"G0074".to_string()));
        assert!(!codes("View v { Layers a width=100, height=100 { Rectangle r 10x10 } }").contains(&"G0074".to_string()));
    }

    #[test]
    fn removed_words_and_code_controls() {
        assert!(codes("View v { Border b }").contains(&"G7011".to_string()));
        let code = compile_with("View v { GradientEdit g width=100, height=40 }", &|_| None, CompileOptions { code_controls: &["GradientEdit"], ..Default::default() });
        assert!(code.diags.iter().any(|d| d.code == "G7013"), "{:?}", code.diags);
        assert!(!crate::diag::has_errors(&code.diags), "{:?}", code.diags);
        assert!(codes("View v { Panel p width=host }").contains(&"G0055".to_string()));
        assert!(codes("View v { Button b on:pressed=x }").contains(&"G0049".to_string()));
        // Kind-checked gesture events: the pointer/interpreted conflict is
        // unrepresentable, and a dead wire is an error, not silence.
        assert!(codes("View v { Button b on:pointer=x }").contains(&"G0072".to_string()));
        assert!(codes("View v { Layers l interactive=true, on:press=x }").contains(&"G0072".to_string()));
        assert!(codes("View v { Layers l on:pointer=x }").contains(&"G0073".to_string()), "pointer without interactive=true is a dead wire");
        assert!(!crate::diag::has_errors(&compile("View v { Layers l interactive=true, on:pointer=x }", &|_| None).diags));
        assert!(codes("Column c { Label a }").contains(&"G7003".to_string()));
        assert!(codes("View v { Label a text=\"x\"/k; Label b text=\"y\"/k }").contains(&"G8001".to_string()));
        assert!(codes("View v { Label a font_color=$nothing }").contains(&"G6001".to_string()));
        assert!(codes("View v { Column c { Label a margin_top=3 } }").contains(&"G1014".to_string()));
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
