//! AST → `Page`. Styles, `uses`, widgets and the palette are resolved
//! here (the runtime never sees them); `$theme` refs and translation keys
//! stay live. Diagnostic codes are stable G-numbers (`G0041`, `G7011` …).

use std::collections::{HashMap, HashSet};

use crate::anim::{Channel, prop_lane, EASING_PARAM_KEYS, Easing, PARAMS_FOR_CURVE};
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
    /// The build's image table, shared the same way: a `source=` path is
    /// one `ImageKey` across every page, indexing the one `IMAGES` table
    /// the build emits. `None` = the page gets its own table
    /// (`Compiled::images`), ids local to it.
    pub images: Option<&'a std::cell::RefCell<Images>>,
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
    /// `slot.<name>.<prop>=` writes for a `for=<control>` target —
    /// checked at the declaration (customization spec §3.4), delivered to each
    /// wearer's slot node as its body builds (baked per instance,
    /// approved 2026-09-02).
    slot_props: Vec<(String, AstProp)>,
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
    /// The `state a, b` lines' names — the control's own state
    /// vocabulary; `when <name>` in the control binds to them (GPAR116).
    states: Vec<String>,
    /// The `state_builtin` line's names — the built-ins the control's
    /// script raises; an instance `when` may name only these and
    /// `states` (GP37, the shipped contract-state fence).
    state_builtins: Vec<String>,
    /// The `keys` line's claim (GP26, `KEYS_*` bits) — stamped onto
    /// every instance root at build.
    keys: u8,
    /// The `behavior slider` line (GP52) — baked per instance at stamp
    /// time (the param scope is live there).
    behavior: Option<AstNode>,
    /// The `signal` lines' declared names — what GP52's machine may
    /// raise (`drag_started`/`drag_ended` when declared).
    signals: Vec<String>,
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
    /// Carried in by a style — a node's own block overriding it per
    /// state is the normal pattern, never a duplicate (GPAR94).
    styled: bool,
}

/// A node before it has an arena slot.
struct TmpNode {
    node: Node,
    props: Vec<Prop>,
    handlers: Vec<OnHandler>,
    whens: Vec<WhenTmp>,
    transitions: Vec<Transition>,
    children: Vec<TmpNode>,
    /// A control instance root's declared states (GPAR116) — recorded
    /// into `Page::custom_states` once the arena id exists.
    custom_states: Vec<StrId>,
    /// The contract's `keys` claim (GP26) — recorded into
    /// `Page::key_claims` once the arena id exists.
    key_claim: u8,
    /// The styles this node wears — recorded into `style_wearers` once
    /// the arena id exists (`each .Style` fan-outs and self-anim binding
    /// need the wearer list, GP40).
    wearing: Vec<String>,
    /// An `automatic=` coupling's driver half (GP33): the control root's
    /// PATH plus min/max (value couplings) or the state word (presence),
    /// resolved to `Page::drivers` once the arena ids exist.
    driver: Option<(String, f32, f32, String)>,
    /// A `text=live($value)` label: (control root path, display step) —
    /// resolved to `Page::live_texts` once the arena ids exist.
    live: Option<(String, f32)>,
}

/// The slot names in a control body whose `automatic=` is a radial kind
/// (`arc(...)`/`angle(...)`) — their aperture belongs to the driver, so a
/// `for=` style may not restyle it (G3002).
/// The contract slots a `control` body declares: each named `slot` and
/// its `automatic=` word ("" when static).
///
/// Needed because a `slot.<name>.…` path names a region the writer
/// can't see from the style line — unknown names and driven words are
/// checked against this list at the declaration, instead of dying
/// silently inside the control.
fn slot_contract(body: &AstNode, widgets: &HashMap<String, WidgetDef>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.type_word == "slot" {
            if let Some(name) = &n.name {
                let auto = n.props.iter().find(|p| p.key == "automatic").map(|p| match &p.value {
                    AstValue::Call { name, .. } => name.clone(),
                    v => v.as_ident().unwrap_or("").to_string(),
                }).unwrap_or_default();
                out.push((name.clone(), auto));
            }
        }
        for c in &n.children {
            if let AstItem::Node(k) = c {
                // A nested widget instance's `slot x { }` children FILL
                // that widget — they are not this control's contract.
                if !widgets.contains_key(&k.type_word) {
                    stack.push(k);
                }
            }
        }
    }
    out
}

/// The built-in kinds that declare contract slots. A built-in draws its
/// parts itself (no child nodes), so its paths land on node words via
/// `builtin_slot_prop`; a kind absent here has no slots to restyle.
fn builtin_slots(kind: Kind) -> Option<&'static [&'static str]> {
    match kind {
        Kind::Slider => Some(&["track", "fill", "thumb", "label"]),
        _ => None,
    }
}

/// Where a built-in's `slot.<name>.<prop>=` lands: the node word its
/// drawing already reads for that part. None = the part exists but that
/// word isn't restylable yet — an error, never a silent drop (E8).
fn builtin_slot_prop(kind: Kind, slot: &str, prop: &str) -> Option<&'static str> {
    match (kind, slot, prop) {
        (Kind::Slider, "track", "background") => Some("background"),
        (Kind::Slider, "track", "corner_radius") => Some("corner_radius"),
        (Kind::Slider, "fill", "background") => Some("color"),
        _ => None,
    }
}

/// The universal control states every press surface and built-in
/// raises natively; a declared `control` raises only what its contract
/// opts into (`state_builtin`).
const CONTROL_STATE_WORDS: &[&str] = &["held", "hovered", "focused", "focus_within", "selected", "disabled"];

/// The condition words a `when` line names (`when held`, `when
/// screen.compact hovered` — the name slot plus the ident args).
fn when_words(k: &AstNode) -> impl Iterator<Item = &str> {
    k.name.iter().map(String::as_str).chain(k.args.iter().filter_map(|a| a.as_ident()))
}

/// Whether a `when` block answers with any `slot.` path — those need a
/// contract resolved before the block can be rewritten.
fn when_has_slot_path(w: &AstNode) -> bool {
    w.props.iter().any(|p| p.key.starts_with("slot."))
        || w.children.iter().any(|c| matches!(c, AstItem::Assign { key, dollar: false, .. } if key.starts_with("slot.")))
}

/// What a `slot.<name>.<prop>=` path checks against: a built-in kind
/// drawing its own parts, or a declared `control`'s slot list (from
/// `slot_contract`). The String is the target's display word.
enum SlotTarget {
    Builtin(Kind, String),
    Control(String, Vec<(String, String)>),
}

/// A node-level transition's node before flattening assigns the real id.
const PENDING_NODE: NodeId = NodeId::new(u32::MAX - 1);
/// A `for=` widget response whose host is found once the tree exists.
const HOST_PENDING: NodeId = NodeId::new(u32::MAX - 2);

/// Per-anim compile state threaded through `anim_chain` (D136): the decl
/// table (so a `play` step can pull a later decl in early), the shared
/// compiled/cycle bookkeeping, and the current namespace of named groups
/// (`after=` anchors are anim-flat; an `each` body is its own namespace
/// per wearer).
struct AnimSt<'d, 'x> {
    decls: &'d [(String, AstNode)],
    compiled: &'x mut Vec<bool>,
    stack: &'x mut Vec<String>,
    /// Named groups seen (the duplicate fence, and unknown-vs-cycle calls).
    names: HashSet<String>,
    /// Named group -> its end time, once emitted.
    ends: HashMap<String, f32>,
    /// `after=` groups parked until their anchor has an end: (the group
    /// step, the scope chain to compile it under, the anchor, the offset).
    deferred: Vec<(&'d AstNode, Vec<&'d AstNode>, String, f32)>,
    /// Inside an `each` body: the wearer bare paths target.
    each_of: Option<NodeId>,
}


/// Joins a `uses` path onto the including file's directory, folding
/// `.` and `..` — the include chain's one path shape (GPAR35).
fn join_uses(base: &str, path: &str) -> String {
    let mut parts: Vec<&str> = if base.is_empty() { Vec::new() } else { base.split('/').collect() };
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// GP52: one `behavior slider` line's bake, waiting for the end of the
/// build (the instance root and its `slot track` have no arena ids yet).
struct PendingBehavior {
    path: String,
    min: f32,
    max: f32,
    step: f32,
    key_unit: f32,
    rest: f32,
    drag_started: StrId,
    drag_ended: StrId,
    line: u32,
}

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
        PropKey::StopColor(_) | PropKey::StopOffset(_) => format!("`stop_<n>_color`/`stop_<n>_offset` keep a Gradient's declared stops live — a {name} has no stops"),
        PropKey::Color | PropKey::ColorA => format!("`{word}=` is the fill of a Rectangle or Circle — a {name}'s plate is `background=`, a fade is `opacity=`"),
        PropKey::Background if kind == Kind::Gradient => "`background=` on a Gradient — a Gradient paints its `stops=`".to_string(),
        PropKey::Background => format!("`background=` on a {name} — its fill is `color=`"),
        PropKey::CornerRadius | PropKey::BorderThickness | PropKey::BorderColor | PropKey::ClipContents if kind == Kind::Circle => format!("`{word}=` on a Circle — a Circle is not a box; it has a `color=` and a ring (`border_thickness=`, `border_color=`)"),
        PropKey::CornerRadius | PropKey::BorderThickness | PropKey::BorderColor | PropKey::ClipContents => format!("`{word}=` shapes a box's plate — a {name} has none; wrap it in a Panel"),
        PropKey::Padding | PropKey::PaddingLeft | PropKey::PaddingTop | PropKey::PaddingRight | PropKey::PaddingBottom if leaf => format!("`{word}=` is air inside a box around its children or text — a {name} has neither; outer air is `margin=`"),
        PropKey::Allow if kind == Kind::List => "a List picks its pan axis with `layout`, not `allow=` — `layout Row` pans sideways, anything else pans down".to_string(),
        PropKey::Allow | PropKey::Bars => format!("`{word}=` picks a Scroll's pan axes — a {name} does not scroll"),
        PropKey::Cols | PropKey::RowHeight => format!("`{word}=` lays out a Grid or Tiles — a {name} has no tracks"),
        PropKey::Spacing => format!("`spacing=` separates a stack's children (Column, Row, Flow, Grid, Tiles, List) — a {name} stacks nothing"),
        PropKey::Text | PropKey::FontSize | PropKey::FontColor | PropKey::Wrap | PropKey::ClipText | PropKey::TextHalign | PropKey::TextValign => format!("`{word}=` belongs to a box that shows text (Label, Button, Check, Toggle, Radio, Input) — a {name} shows none"),
        PropKey::Source | PropKey::Fit | PropKey::Loading => format!("`{word}=` is an Image word — a {name} shows no picture"),
        PropKey::TextFit | PropKey::TextFlip => format!("`{word}=` stretches a Label's one glyph over its box — a {name} has no glyph to stretch"),
        PropKey::Stops | PropKey::GradientFill | PropKey::Angle | PropKey::GradientRepeat => format!("`{word}=` is a Gradient word — a {name}'s one color is `background=`"),
        PropKey::From | PropKey::Sweep if kind == Kind::Slot => format!("`{word}=` is a radial kind's config — it goes inside the kind: `automatic=arc(from=-225, sweep=270) by $value in $min..$max`, in a `control`"),
        PropKey::From | PropKey::To => format!("`{word}=` aims a Gradient (0..1 box points) or an Arc (degrees) — a {name} takes neither"),
        PropKey::Sweep | PropKey::Thickness => format!("`{word}=` is an Arc word — a {name} sweeps nothing"),
        PropKey::Dash => format!("`dash=` dashes an Arc's stroke — a {name}'s ring or border dashes with `border_dash=`"),
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
    palette: HashMap<String, Color_Straight_Srgb_4xU8_RGBA>,
    /// The variant-flipping words, (light, dark) — `page::MODE_COLORS`.
    /// Overrides live in `palette`/`palette_dark`, which `palette_lookup`
    /// checks first, so a plain-block override replaces both halves and a
    /// `theme dark { }` one the dark half (REFERENCE.md).
    mode_palette: HashMap<String, (Color_Straight_Srgb_4xU8_RGBA, Color_Straight_Srgb_4xU8_RGBA)>,
    /// `theme dark { red = … }` overrides — read only while the active
    /// variant is `dark` (GPAR88).
    palette_dark: HashMap<String, Color_Straight_Srgb_4xU8_RGBA>,
    /// Word-to-word overrides in a `theme dark { }` block.
    palette_links_dark: HashMap<String, String>,
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
    /// While a control instance's body builds: its declared states, so
    /// `when <name>`/`transition` words bind to `State::Custom` (GPAR116).
    custom_state_stack: Vec<Vec<String>>,
    /// Directory stack of the include chain (GPAR35): a nested `uses`
    /// resolves against the INCLUDING file's directory, so the loader
    /// always sees one path shape — from the top file's own directory.
    use_dirs: Vec<String>,
    /// `<instance>/<body-root name>` -> `<instance>` (GPAR95): the root
    /// keeps its body name as an address after the instance name replaces
    /// it, so `a/root.opacity` in an anim or response still binds.
    root_aliases: HashMap<String, String>,
    node_whens: Vec<Vec<WhenTmp>>,
    /// The key table when `options.keys` is `None` (see `CompileOptions`).
    local_keys: Keys,
    /// The image table when `options.images` is `None`.
    local_images: Images,
    /// Top-level `when <arrangement> { node.prop = v }`, applied after the tree exists.
    /// Top-level poses, resolved at finish: (state, `node.prop = value`
    /// lines, block line, default). `default` = a hostless widget's own
    /// `when screen.*` pose (D14): it ranks as line 0 and never clashes, so
    /// a page's pose on the same instance wins.
    deferred_whens: Vec<(State, Vec<AstItem>, u32, bool)>,
    /// `anim` declarations, compiled after the tree exists (they name
    /// nodes): (the widget-instance scope they were declared in — `""` at
    /// the top of the file — and the declaration).
    anim_decls: Vec<(String, AstNode)>,
    /// style name -> the nodes wearing it (file order): `each .Style`
    /// fan-outs and per-wearer `self` anim binding read it (GP40).
    style_wearers: HashMap<String, Vec<NodeId>>,
    /// Anim lanes whose written target was `self`: (anim, index into
    /// `page.anims`). A style's `transition anim=` re-binds these to each
    /// wearer (the shipped rule: `self` targets the node a style is
    /// applied to); everywhere else `self` stays the scope root.
    self_lanes: Vec<(StrId, u32)>,
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
    /// Set by `automatic()` for the slot being compiled; `node()` moves it
    /// onto the TmpNode (GP33).
    pending_driver: Option<(String, f32, f32, String)>,
    /// Set by `value()` when a `live($value)` bakes into a text prop
    /// inside a control scope; `node()` moves it onto the TmpNode.
    pending_live: Option<(String, f32)>,
    /// (node, root path, min, max, state) rows awaiting arena lookups.
    pending_drivers: Vec<(NodeId, String, f32, f32, String)>,
    /// GP52 `behavior slider` rows awaiting arena ids.
    pending_behaviors: Vec<PendingBehavior>,
    pending_lives: Vec<(NodeId, String, f32)>,
    fills: Vec<Fills>,
    /// Per widget instance being expanded: the instance's `on:` props still
    /// waiting for the body's `signals` node (`None` = the root took them).
    instance_on: Vec<Option<Vec<AstProp>>>,
    /// Per widget instance being expanded: `slot.<name>.<prop>=` writes
    /// (already checked at their declaration) waiting for their slot
    /// node to build — style writes first, instance-line writes after,
    /// so the later-wins read gives the closest writer the last word.
    slot_writes: Vec<HashMap<String, Vec<AstProp>>>,
    /// A `for=` style's `when` state words, checked in `finish` (GP40
    /// step 2): an arrangement word is the view's to declare, and the
    /// view may not have built when the style is read. (style, target,
    /// word, line).
    pending_style_states: Vec<(String, String, String, u32)>,
    /// A plain style's custom state landing on a control type that
    /// declares it (style, word, type, style line) — `finish` draws the
    /// D2 advisory when exactly one type is in the set.
    d2_lands: Vec<(String, String, String, u32)>,
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
    theme_entry_kinds: Vec<(String, StrId)>,
    /// A DEFAULT-variant theme entry's `.Style` names per kind word
    /// (`theme {{ Fader .Chunky }}`), worn FIRST — weakest — at every
    /// node build of that kind, plus the entry line's own prop keys
    /// (the line out-ranks its style, so the style's copies of those
    /// keys are filtered at the wear).
    entry_styles: HashMap<String, (Vec<String>, Vec<String>)>,
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
        AstValue::Ref { name, opacity, pos } => format!("${name}{}", suffixes(opacity, pos)),
        AstValue::Named { name, value } => format!("{name}={}", spell(value)),
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
    /// The image table this compile filled, the same way: `images.name(k)`
    /// spells a `VImage` when the compile stood alone.
    pub images: Images,
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
        mode_palette: MODE_COLORS.iter().map(|(w, l, d)| ((*w).to_string(), (Color_Straight_Srgb_4xU8_RGBA::opaque(*l), Color_Straight_Srgb_4xU8_RGBA::opaque(*d)))).collect(),
        palette_dark: HashMap::new(),
        palette_links_dark: HashMap::new(),
        scopes: Vec::new(),
        style_scopes: Vec::new(),
        control_scopes: HashSet::new(),
        custom_state_stack: Vec::new(),
        root_aliases: HashMap::new(),
        use_dirs: Vec::new(),
        node_whens: Vec::new(),
        local_keys: Keys::default(),
        local_images: Images::default(),
        deferred_whens: Vec::new(),
        anim_decls: Vec::new(),
        style_wearers: HashMap::new(),
        self_lanes: Vec::new(),
        root_seen: false,
        names: std::collections::HashSet::new(),
        used: std::collections::HashSet::new(),
        prefix: Vec::new(),
        unnamed_instances: 0,
        widget_stack: Vec::new(),
        pending_driver: None,
        pending_live: None,
        pending_drivers: Vec::new(),
        pending_behaviors: Vec::new(),
        pending_lives: Vec::new(),
        choice_depth: 0,
        fills: Vec::new(),
        instance_on: Vec::new(),
        slot_writes: Vec::new(),
        pending_style_states: Vec::new(),
        d2_lands: Vec::new(),
        stops_ctx: None,
        repeat_binding: None,
        in_when: 0,
        theme_entry_kinds: Vec::new(),
        entry_styles: HashMap::new(),
        palette_links: std::collections::HashMap::new(),
        pending_repeats: Vec::new(),
    };
    c.page.intern("");
    // The well-known variant name exists on every page, so a host can set
    // `Runtime::variant` to it whether or not the file has a `theme dark`
    // block (GPAR111 — mode tokens flip on pages that never mention dark).
    c.page.intern("dark");
    c.document(&doc, true);
    c.finish();
    Compiled { page: c.page, diags: c.diags, keys: c.local_keys, images: c.local_images }
}

/// The bare color words: `page::PALETTE` plus `transparent`. Aliases are
/// NOT baked in — `palette_lookup` follows them at every use, so an
/// override of `red` moves `danger` too (GPAR88).
fn default_palette() -> HashMap<String, Color_Straight_Srgb_4xU8_RGBA> {
    let mut p = HashMap::new();
    for (name, _) in PALETTE {
        p.insert(name.to_string(), palette_color(name).expect("palette word"));
    }
    p.insert("transparent".to_string(), Color_Straight_Srgb_4xU8_RGBA::TRANSPARENT);
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

    /// The id of image path `name` — from the build's shared table when
    /// there is one, else this page's own (see `CompileOptions::images`).
    fn image_key(&mut self, name: &str) -> ImageKey {
        match self.options.images {
            Some(shared) => shared.borrow_mut().intern(name),
            None => self.local_images.intern(name),
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
                    // Keys live on DISPLAYED text only (GPAR89, the
                    // addon's rule): a keyed theme value would translate
                    // in some uses and not others.
                    if let AstValue::Str { key: Some(tk), .. } = value {
                        self.error("G8001", *line, format!("`${key} = \"…\"/{tk}` — a theme value never carries a translation key; the key goes on the displayed text (`text=$title/{tk}` is not a form either: write the key at the `text=` use)"));
                    }
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
                            if items.is_empty() {
                                // GPAR93: it can never do anything — a
                                // silent no-op reads as a working pose.
                                self.error("G0054", n.line, "this `when` holds no responses — a top-level block poses nodes (`node.prop = value`)");
                            }
                            self.deferred_whens.push((state, items, n.line, false));
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
        // A nested `uses` resolves against the INCLUDING file's
        // directory (GPAR35, the addon's rule): `sub/widgets.gogo`
        // writing `uses "inner.gogo"` loads `sub/inner.gogo`.
        let base = self.use_dirs.last().cloned().unwrap_or_default();
        let full = join_uses(&base, path);
        if !self.used.insert(full.clone()) {
            return;
        }
        let Some(text) = (self.loader)(&full) else {
            let looked = if full == path { String::new() } else { format!(" (looked for `{full}`, from the including file's directory)") };
            self.error("G0024", n.line, format!("`uses \"{path}\"`: file not found{looked}"));
            return;
        };
        let mut diags = Vec::new();
        let doc = parse(&text, &mut diags);
        for d in diags {
            self.diags.push(Diagnostic { message: format!("{full}: {}", d.message), ..d });
        }
        self.use_dirs.push(full.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default());
        self.document(&doc, false);
        self.use_dirs.pop();
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
                    // Keys live on displayed text only (GPAR89) — see
                    // the top-level `$` arm.
                    if let AstValue::Str { key: Some(tk), .. } = value {
                        self.error("G8001", *line, format!("`${key} = \"…\"/{tk}` — a theme value never carries a translation key; keys live on displayed text"));
                    }
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
                            self.page.kind_defaults.push(KindDefault { kind: Kind::View, props: (first, 1), variant });
                            continue;
                        }
                        if matches!(pk, PropKey::Font | PropKey::FontSize | PropKey::Color) {
                            let v = self.value(value, *line, Some(pk));
                            for kind in [Kind::Label, Kind::Button, Kind::Input, Kind::Check, Kind::Toggle, Kind::Radio, Kind::View, Kind::Panel] {
                                let first = self.page.props.len() as u32;
                                self.page.props.push(Prop { key: pk, value: v });
                                self.page.kind_defaults.push(KindDefault { kind, props: (first, 1), variant });
                            }
                            continue;
                        }
                    }
                    // Palette override: `red = #E51400` (§7: bare names only here).
                    if self.palette_lookup(key, false).is_none() {
                        self.error("G6003", *line, format!("`{key}` is not a palette color (new names are `$values`)"));
                        continue;
                    }
                    let is_dark_block = self.page.str(variant) == "dark";
                    if variant != NO_STR && !is_dark_block {
                        // Only `dark` has a second color column; a color
                        // scoped to any other variant is a `$value`
                        // (which IS variant-gated) — recoloring a palette
                        // word there would silently apply everywhere.
                        self.error("G6006", *line, format!("a palette override in `theme {} {{ }}` has no color column of its own — only the well-known `dark` block does; a variant-scoped color is a `$value` (`theme {} {{ $name = … }}`)", self.page.str(variant), self.page.str(variant)));
                        continue;
                    }
                    if matches!(value, AstValue::Color { pos: Some(_), .. } | AstValue::Ident { pos: Some(_), .. }) {
                        self.error("G0040", *line, "`@` positions a gradient stop — it only goes on entries of a Gradient's `stops=` list");
                        continue;
                    }
                    // A BARE-word override is a link, followed at every
                    // use (`red = green` then `green = X` moves red;
                    // GPAR88); a chain that returns to its start can
                    // never resolve (G6005). A worded value with `/o`
                    // bakes bytes below instead — the alpha is applied
                    // once, here.
                    if let AstValue::Ident { word, opacity: None, pos: None } = value {
                        if self.palette_lookup(word, false).is_none() {
                            let mut pool: Vec<String> = self.palette.keys().cloned().collect();
                            pool.extend(self.mode_palette.keys().cloned());
                            pool.extend(PALETTE_ALIASES.iter().map(|(a, _)| (*a).to_string()));
                            let hint = suggest(word, &pool);
                            let tail = if hint.is_empty() { String::new() } else { format!(" — {hint}") };
                            self.error("G6004", *line, format!("`{key} = …` needs a color{tail}"));
                            continue;
                        }
                        if is_dark_block {
                            self.palette_links_dark.insert(key.clone(), word.clone());
                            self.palette_dark.remove(key);
                        } else {
                            self.palette_links.insert(key.clone(), word.clone());
                            self.palette.remove(key);
                        }
                        let mut cur = word.clone();
                        for _ in 0..16 {
                            if cur == *key {
                                self.error("G6005", *line, format!("circular color override chain at '{key}'"));
                                break;
                            }
                            let next = if is_dark_block { self.palette_links_dark.get(&cur) } else { self.palette_links.get(&cur) };
                            if let Some(nxt) = next {
                                cur = nxt.clone();
                                continue;
                            }
                            match PALETTE_ALIASES.iter().find(|(a, _)| *a == cur) {
                                Some((_, of)) => cur = (*of).to_string(),
                                None => break,
                            }
                        }
                        continue;
                    }
                    match self.color_value(value, *line) {
                        Some(c) => {
                            if is_dark_block {
                                self.palette_dark.insert(key.clone(), c);
                                self.palette_links_dark.remove(key);
                            } else {
                                self.palette.insert(key.clone(), c);
                                self.palette_links.remove(key);
                            }
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
                    // A declared `control` is a legal entry too (GP40
                    // step 3, user 2026-09-02) — its contract makes the
                    // entry checkable like a built-in's. A plain widget
                    // has no contract: style its nodes by name.
                    let kind = Kind::from_word(&kn.type_word);
                    let user_control = kind.is_none() && self.widgets.get(&kn.type_word).map_or(false, |d| d.control);
                    if kind.is_none() && !user_control {
                        self.error("G3006", kn.line, format!("`{}` is not a control kind", kn.type_word));
                        continue;
                    }
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
                    if self.theme_entry_kinds.contains(&(kn.type_word.clone(), variant)) {
                        self.error("G3006", kn.line, format!("duplicate theme entry for {} — one prototype per type per block; fold the props into it", kn.type_word));
                        continue;
                    }
                    // A style named on the entry must dress this kind
                    // (its `for=` says which) — the mismatch would default
                    // one control with another's slot words.
                    for sname in &kn.styles {
                        let target = self.find_style(sname).and_then(|sd| sd.for_target.clone());
                        if let Some(t) = target {
                            if t != kn.type_word {
                                self.error("G3006", kn.line, format!("style `{sname}` is for {t} — this entry defaults {}", kn.type_word));
                            }
                        }
                    }
                    self.theme_entry_kinds.push((kn.type_word.clone(), variant));
                    if user_control {
                        // A user control's entry takes a style; its prop
                        // defaults are its `$param=` line — an entry
                        // prop would silently shadow the param scope.
                        if let Some(p) = kn.props.first() {
                            self.error("G3006", p.line, format!("a `{}` entry takes a style (`theme {{ {} .X }}`) — prop defaults live on the control's `$param=` line", kn.type_word, kn.type_word));
                        }
                        if variant != NO_STR {
                            // A user control's style writes bake per
                            // instance at compile — they can't switch
                            // with the variant (variant-stamped writes,
                            // LATER IF NEEDED: todo-gogo.md GP47).
                            if !kn.styles.is_empty() {
                                self.error("G3006", kn.line, format!("a variant entry can't restyle `{}` yet — its style's writes bake at compile and don't switch with the variant", kn.type_word));
                            }
                            continue;
                        }
                        if !kn.styles.is_empty() {
                            let e = self.entry_styles.entry(kn.type_word.clone()).or_default();
                            e.0.extend(kn.styles.iter().cloned());
                        }
                        continue;
                    }
                    let kind = kind.unwrap_or(Kind::Layers);
                    if variant == NO_STR {
                        // The entry's style dresses every node of the
                        // kind — worn at each node build, weakest.
                        if !kn.styles.is_empty() {
                            let e = self.entry_styles.entry(kn.type_word.clone()).or_default();
                            e.0.extend(kn.styles.iter().cloned());
                            e.1.extend(kn.props.iter().map(|p| p.key.clone()));
                        }
                    }
                    let first = self.page.props.len() as u32;
                    for p in &kn.props {
                        if let Some(prop) = self.prop(p, kind) {
                            self.page.props.push(prop);
                        }
                    }
                    let count = self.page.props.len() as u32 - first;
                    self.page.kind_defaults.push(KindDefault { kind, props: (first, count), variant });
                    if variant != NO_STR {
                        // A VARIANT entry's style: base-look props only,
                        // riding the variant-gated kind defaults (pushed
                        // AFTER the line's run — the first matching run
                        // wins the read, so the line out-ranks it).
                        // `when`/`transition` can't switch with the
                        // variant yet (variant-stamped writes, LATER IF
                        // NEEDED: todo-gogo.md GP47).
                        for sname in &kn.styles {
                            let Some(def) = self.resolve_style(sname, kn.line) else { continue };
                            if !def.whens.is_empty() || !def.transitions.is_empty() {
                                self.error("G3006", kn.line, format!("style `{sname}` carries `when`/`transition` — those don't switch with the variant yet; a variant entry's style holds base-look props"));
                                continue;
                            }
                            let first = self.page.props.len() as u32;
                            for p in &def.props {
                                if let Some(prop) = self.prop(p, kind) {
                                    self.page.props.push(prop);
                                }
                            }
                            let count = self.page.props.len() as u32 - first;
                            self.page.kind_defaults.push(KindDefault { kind, props: (first, count), variant });
                        }
                    }
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
                // A declared `control` is a legal `for=` target — its
                // contract is checkable, unlike a plain widget's body.
                Some(word) if self.widgets.get(word).map_or(false, |d| d.control) => {
                    for_target = Some(word.to_string());
                }
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
        // `slot.<name>.<prop>=` paths are checked HERE, at the
        // declaration (customization spec §3.4) — a wearer only ever receives writes
        // that already passed. A control's go to `slot_props` for its
        // slot node; a built-in's land back in `props` on the node word
        // its drawing reads.
        let has_paths = props.iter().any(|p| p.key.starts_with("slot."));
        let when_paths = n.children.iter().any(|c| matches!(c, AstItem::Node(k) if k.type_word == "when" && when_has_slot_path(k)));
        let mut slot_target: Option<SlotTarget> = None;
        if has_paths || when_paths {
            slot_target = match &for_target {
                None => {
                    self.error("G3002", n.line, format!("a slot path needs a contract to check against — name the control: `style {name} for=<Control>, slot.…`"));
                    None
                }
                Some(t) => Some(self.slot_target_of(t)),
            };
        }
        let mut slot_props: Vec<(String, AstProp)> = Vec::new();
        if has_paths {
            let mut kept: Vec<AstProp> = Vec::new();
            for p in std::mem::take(&mut props) {
                if !p.key.starts_with("slot.") {
                    kept.push(p);
                } else if let Some(t) = &slot_target {
                    if let Some((slot, ap)) = self.slot_path(&p, t, false) {
                        if slot.is_empty() {
                            kept.push(ap);
                        } else {
                            slot_props.push((slot, ap));
                        }
                    }
                }
            }
            props = kept;
        }
        let mut def = StyleDef { props, for_target, extends, slot_props, line: n.line, ..Default::default() };
        for item in &n.children {
            match item {
                AstItem::Node(c) if c.type_word == "when" => def.whens.push(c.clone()),
                AstItem::Node(c) if c.type_word == "transition" => def.transitions.push(c.clone()),
                AstItem::Node(c) => self.error("G0026", c.line, format!("a style block holds `when` and `transition` lines, not `{}`", c.type_word)),
                AstItem::Assign { key, line, .. } => self.error("G0026", *line, format!("a style block holds `when` and `transition` lines, not `{key} =`")),
            }
        }
        if when_paths {
            for w in &mut def.whens {
                self.rewrite_slot_when(w, slot_target.as_ref());
            }
        }
        // GP40 step 2 (2026-09-02): a `for=` style's `when` states check
        // at the DECLARATION — a typo'd contract state would otherwise
        // wait silently forever on every wearer. Checked in `finish`,
        // where the view's arrangement words exist.
        if let Some(t) = &def.for_target {
            for w in &def.whens {
                for word in when_words(w) {
                    self.pending_style_states.push((name.to_string(), t.clone(), word.to_string(), w.line));
                }
            }
        }
        def
    }

    /// The contract behind a `for=` word: a built-in kind, or a declared
    /// control's slot list.
    fn slot_target_of(&self, word: &str) -> SlotTarget {
        match Kind::from_word(word) {
            Some(k) => SlotTarget::Builtin(k, word.to_string()),
            None => {
                let slots = self.widgets.get(word).map(|d| slot_contract(&d.body, &self.widgets)).unwrap_or_default();
                SlotTarget::Control(word.to_string(), slots)
            }
        }
    }

    /// Checks one `slot.<name>.<prop>=` path against a contract and says
    /// where it lands.
    ///
    /// Needed because the region behind a path is invisible from the
    /// writing line — unknown slots, driven words, and content words
    /// must refuse at the declaration, never die inside the control.
    ///
    /// Returns (slot to deliver to, rewritten prop); an empty slot name
    /// means the prop lands on the control node itself (a built-in reads
    /// its part looks from its own words). None = refused, the error
    /// already points at the writing line.
    fn slot_path(&mut self, p: &AstProp, target: &SlotTarget, in_when: bool) -> Option<(String, AstProp)> {
        let rest = p.key.strip_prefix("slot.").unwrap_or(&p.key);
        let Some((slot_name, prop_word)) = rest.split_once('.') else {
            self.error("G3002", p.line, "a slot path is `slot.<name>.<prop>=` — one region, one word");
            return None;
        };
        let auto: &str = match target {
            SlotTarget::Builtin(k, w) => {
                let Some(names) = builtin_slots(*k) else {
                    self.error("G3002", p.line, format!("a {w} has no slots to restyle — its states still check"));
                    return None;
                };
                if !names.contains(&slot_name) {
                    self.error("G3002", p.line, format!("{w} has no slot '{slot_name}' — slots: {}", names.join(", ")));
                    return None;
                }
                ""
            }
            SlotTarget::Control(w, slots) => match slots.iter().find(|(n, _)| n == slot_name) {
                None => {
                    let names: Vec<&str> = slots.iter().map(|(n, _)| n.as_str()).collect();
                    self.error("G3002", p.line, format!("{w} has no slot '{slot_name}' — slots: {}", names.join(", ")));
                    return None;
                }
                Some((_, auto)) => auto.as_str(),
            },
        };
        // The driven word has ONE writer — the slot's `automatic=`
        // config (the shipped G3002).
        let driven: &[&str] = match auto {
            "length" => &["width", "size"],
            "position" => &["halign"],
            "presence" => &["visibility"],
            "arc" | "angle" => &["from", "sweep"],
            _ => &[],
        };
        if driven.contains(&prop_word) {
            self.error("G3002", p.line, format!("slot '{slot_name}' moves by itself — `{prop_word}=` is the driver's to write"));
            return None;
        }
        // Restyling content IS filling (customization spec §3.4): the region keeps box
        // words; a content word means bring your own node.
        let content_node = match prop_word {
            "text" | "font_size" | "font_color" | "font" | "wrap" | "clip_text" | "text_halign" | "text_valign" => Some("Label"),
            "source" | "fit" => Some("Image"),
            _ => None,
        };
        if let Some(kw) = content_node {
            self.error("G3002", p.line, format!("`{prop_word}` styles the slot's content, not the region — fill it: `slot {slot_name} {{ {kw} {prop_word}=… }}`"));
            return None;
        }
        let motion = matches!(prop_word, "scale" | "scale.x" | "scale.y" | "translate.x" | "translate.y" | "rotate");
        if motion && !in_when {
            self.error("G3002", p.line, format!("motion is `when` response vocabulary — a base look declares no transform (`when held slot.{slot_name}.{prop_word}=…`)"));
            return None;
        }
        if in_when && !motion && !matches!(prop_word, "opacity" | "background" | "border_color") {
            self.error("G3002", p.line, format!("a `when` response on a slot moves and tints (`scale`, `translate.x/.y`, `rotate`, `opacity`, `background`, `border_color`) — `{prop_word}=` is the region's base look; write it on the style line: `slot.{slot_name}.{prop_word}=`"));
            return None;
        }
        let (out_slot, out_key) = match target {
            SlotTarget::Builtin(k, w) => match builtin_slot_prop(*k, slot_name, prop_word) {
                Some(mapped) => (String::new(), mapped.to_string()),
                None => {
                    self.error("G3002", p.line, format!("the built-in {w}'s '{slot_name}' doesn't take `{prop_word}=` yet — restylable today: track.background, track.corner_radius, fill.background"));
                    return None;
                }
            },
            SlotTarget::Control(..) => (slot_name.to_string(), prop_word.to_string()),
        };
        let mut value = p.value.clone();
        if out_key == "size" {
            if let AstValue::Num { value: v, .. } = &value {
                // `slot.thumb.size=8` — a bare number is a square (customization spec §3.4).
                value = AstValue::Size { w: Some(*v), h: Some(*v) };
            }
        }
        Some((out_slot, AstProp { key: out_key, value, line: p.line }))
    }

    /// Rewrites a `when` block's `slot.<name>.<prop>` responses onto
    /// their real addresses: a control's slot becomes a descendant
    /// response (`<slot>.<prop>`, resolved to the wearer's node), a
    /// built-in's becomes the node word its drawing reads. A refused
    /// path is removed here — its error already points at the line.
    /// `target` None = no contract stood (that error was raised once);
    /// the paths are dropped without a second message.
    fn rewrite_slot_when(&mut self, w: &mut AstNode, target: Option<&SlotTarget>) {
        let rewrite = |me: &mut Self, key: String, value: AstValue, line: u32| -> Option<(String, AstValue)> {
            let t = target?;
            let (slot, ap) = me.slot_path(&AstProp { key, value, line }, t, true)?;
            let key = if slot.is_empty() { ap.key } else { format!("{slot}.{}", ap.key) };
            Some((key, ap.value))
        };
        let mut kept: Vec<AstProp> = Vec::new();
        for p in std::mem::take(&mut w.props) {
            if !p.key.starts_with("slot.") {
                kept.push(p);
            } else if let Some((key, value)) = rewrite(self, p.key.clone(), p.value.clone(), p.line) {
                kept.push(AstProp { key, value, line: p.line });
            }
        }
        w.props = kept;
        let mut kids: Vec<AstItem> = Vec::new();
        for item in std::mem::take(&mut w.children) {
            match item {
                AstItem::Assign { key, dollar: false, value, line } if key.starts_with("slot.") => {
                    if let Some((key, value)) = rewrite(self, key, value, line) {
                        kids.push(AstItem::Assign { key, dollar: false, value, line });
                    }
                }
                other => kids.push(other),
            }
        }
        w.children = kids;
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
            // Slot writes ride the chain like props: parent first, so
            // the child's write to the same region wins the later read.
            let mut slot_props = parent.slot_props.clone();
            slot_props.extend(merged.slot_props);
            merged.slot_props = slot_props;
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
            AstValue::Ident { word, .. } if word.eq_ignore_ascii_case("hug") => Some(Len::Hug(1.0)),
            AstValue::Ident { word, .. } if word.eq_ignore_ascii_case("fill") => Some(Len::Fill(1.0)),
            AstValue::Ident { word, .. } if word.eq_ignore_ascii_case("auto") => {
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
        let says_fill = |key: &str| n.props.iter().any(|p| p.key == key && matches!(&p.value, AstValue::Ident { word, .. } if word.eq_ignore_ascii_case("fill")));
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
        match tok.to_ascii_lowercase().as_str() {
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
        if Kind::from_word(&name).is_some() || matches!(name.as_str(), "when" | "transition" | "theme" | "style" | "anim" | "widget" | "control" | "uses" | "layout" | "gap" | "choice" | "slot" | "empty" | "repeat") {
            self.error("G5001", n.line, format!("widget name `{name}` shadows a built-in — pick another name"));
            return;
        }
        // A widget body is a FRAGMENT (GPAR97): `View` is the screen
        // root, so a body wearing it could never sit anywhere.
        if let Some(AstItem::Node(b)) = n.children.iter().find(|i| matches!(i, AstItem::Node(b) if b.type_word == "View")) {
            self.error("G7012", b.line, "a widget body is a fragment — `View` is the screen root; use `Layers` (free placement) or `Panel` (a styled box)");
        }
        // A `control` body holds its behavior lines (`script`, `state`,
        // `state_builtin`, `signal`, a default `transition`) beside the root;
        // with no code-behind here they carry nothing for the picture. A
        // `widget` has no script to raise or read them, so there they are
        // refused, not skipped; `state_builtin` names must be built-ins.
        let mut keys_mask = 0u8;
        let mut behavior: Option<AstNode> = None;
        for b in n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }) {
            match b.type_word.as_str() {
                "script" | "state" | "state_builtin" | "signal" | "keys" | "behavior" if n.type_word == "widget" => {
                    self.error("G0077", b.line, format!("`{}` lines belong to a `control` body (they describe its script) — a `widget` has none", b.type_word));
                }
                // GP52 (user-approved 2026-09-03): `behavior slider by
                // $value in $min..$max, step=$step, key_unit=5%` — the
                // contract names the INPUT MACHINE that answers this
                // control. The vocabulary is closed; custom input stays
                // the code-behind's (`on:pointer=` + `keys`).
                "behavior" => {
                    match b.name.as_deref() {
                        Some("slider") => {}
                        Some(w) => {
                            self.error("G0082", b.line, format!("`behavior {w}` — the machine vocabulary is closed: `slider` (custom input is code-behind work: `on:pointer=` + `keys`)"));
                            continue;
                        }
                        None => {
                            self.error("G0082", b.line, "`behavior` names its machine — `behavior slider by $value in $min..$max`");
                            continue;
                        }
                    }
                    if behavior.is_some() {
                        self.error("G0082", b.line, "one `behavior` line per control — a machine is already declared");
                        continue;
                    }
                    behavior = Some(b.clone());
                }
                // GP26: `keys arrows, home_end` — the control CLAIMS key
                // groups for its code-behind, the built-ins' static-claim
                // law (a Slider owns its arrows) made declarable. A
                // claimed key reaches the instance root as a `Key` event
                // and never traverses.
                "keys" => {
                    let words: Vec<&str> = b.args.iter().filter_map(|a| a.as_ident()).collect();
                    if words.is_empty() {
                        self.error("G0077", b.line, "`keys` names its claim — `keys arrows, home_end, enter, space` (any subset)");
                    }
                    for word in words {
                        keys_mask |= match word {
                            "arrows" => KEYS_ARROWS,
                            "home_end" => KEYS_HOME_END,
                            "enter" => KEYS_ENTER,
                            "space" => KEYS_SPACE,
                            "tab" => {
                                self.error("G0077", b.line, "`keys tab` — Tab is the way OFF a control (the router's law), never claimable");
                                0
                            }
                            _ => {
                                self.error("G0077", b.line, format!("`keys {word}`: the claimable groups are arrows, home_end, enter, space"));
                                0
                            }
                        };
                    }
                }
                "state_builtin" => {
                    for word in b.args.iter().filter_map(|a| a.as_ident()) {
                        match self.state_from_word(word, b.line, false) {
                            Some(State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled) | None => {}
                            Some(_) => self.error("G0077", b.line, format!("`state_builtin {word}`: the built-ins are held, hovered, focused, focus_within, selected, disabled")),
                        }
                    }
                }
                "state" => {
                    // `state` declares the control's OWN words — a
                    // built-in name here reads as claiming the engine's
                    // state (the addon's d9 check).
                    for word in b.args.iter().filter_map(|a| a.as_ident()) {
                        if matches!(word, "held" | "hovered" | "focused" | "focus_within" | "selected" | "disabled" | "pressed" | "hover" | "focus" | "checked") {
                            self.error("G0077", b.line, format!("`state {word}` names a built-in — declare it `state_builtin {word}`; `state` declares the control's own words"));
                        }
                    }
                }
                _ => {}
            }
        }
        let bodies: Vec<&AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| !matches!(b.type_word.as_str(), "script" | "state" | "state_builtin" | "signal" | "keys" | "behavior" | "transition" | "when" | "anim" | "style")).collect();
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
                // A hostless widget may pose ITS OWN body on the screen
                // class (`when screen.landscapeSmall { card.width = 420 }`,
                // D14) — the pose compiles per instance below. Any other
                // state word has nothing here to answer.
                if w.type_word == "when" && w.name.as_deref().is_some_and(|s| s.starts_with("screen.")) {
                    continue;
                }
                self.error("G5001", w.line, format!("a widget-scope `{}` responds to the HOST's states — this widget declares no host; announce it: `widget {name} for=Radio, …` (hosts: Button, Radio, Check), or pose the body on the screen class: `when screen.landscapeSmall {{ card.width = 420 }}`", w.type_word));
            }
        }
        let states: Vec<String> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "state").flat_map(|b| b.args.iter().filter_map(|a| a.as_ident()).map(str::to_string)).collect();
        let state_builtins: Vec<String> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "state_builtin").flat_map(|b| b.args.iter().filter_map(|a| a.as_ident()).map(str::to_string)).collect();
        let signals: Vec<String> = n.children.iter().filter_map(|i| if let AstItem::Node(b) = i { Some(b) } else { None }).filter(|b| b.type_word == "signal").flat_map(|b| b.args.iter().filter_map(|a| a.as_ident()).map(str::to_string)).collect();
        // GP52: one owner per key group — the slider machine steps
        // `$value` with arrows/Home/End, so a `keys` claim on those
        // groups would be a second owner. Refused, never a precedence.
        if behavior.is_some() && keys_mask & (KEYS_ARROWS | KEYS_HOME_END) != 0 {
            self.error("G0082", n.line, "the slider behavior already claims arrows and Home/End (they step `$value`) — drop them from `keys`; `enter`/`space` stay claimable");
            keys_mask &= !(KEYS_ARROWS | KEYS_HOME_END);
        }
        self.widgets.insert(name, WidgetDef { params: n.params.clone(), body: bodies[0].clone(), anims, host, props, whens, transitions, styles, states, state_builtins, signals, keys: keys_mask, behavior, control: n.type_word == "control", line: n.line });
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
    /// Compiles one `anim` declaration — header words, then the step
    /// chain, then the parked `after=` groups, then the double-pin fence.
    /// `compiled`/`stack` are shared across the whole decl table so a
    /// `play` can pull a later decl in early exactly once.
    fn compile_anim_decl(&mut self, i: usize, decls: &[(String, AstNode)], compiled: &mut Vec<bool>, stack: &mut Vec<String>) {
        if compiled[i] {
            return;
        }
        compiled[i] = true;
        let (scope, n) = &decls[i];
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
        stack.push(full);
        {
            let mut st = AnimSt { decls, compiled: &mut *compiled, stack: &mut *stack, names: HashSet::new(), ends: HashMap::new(), deferred: Vec::new(), each_of: None };
            let scopes = vec![n];
            self.anim_chain(anim, scope, &n.children, &scopes, 0.0, &mut st);
            self.flush_deferred(anim, scope, &mut st);
        }
        stack.pop();
        self.double_pin_check(anim, n.line);
    }

    /// Emits the parked `after=` groups once their anchors have ends,
    /// iterating — an anchored group can itself be an anchor. What never
    /// resolves is a cycle (groups waiting on each other) or a name no
    /// group carries.
    fn flush_deferred(&mut self, anim: StrId, scope: &str, st: &mut AnimSt<'_, '_>) -> f32 {
        let mut end = 0.0f32;
        loop {
            if st.deferred.is_empty() {
                return end;
            }
            let pending = std::mem::take(&mut st.deferred);
            let mut progressed = false;
            for (step, sc, anchor, offset) in pending {
                match st.ends.get(&anchor).copied() {
                    Some(anchor_end) => {
                        progressed = true;
                        let mut scopes = sc.clone();
                        scopes.push(step);
                        let e = self.anim_chain(anim, scope, &step.children, &scopes, (anchor_end + offset).max(0.0), st);
                        if let Some(nm) = &step.name {
                            st.ends.insert(nm.clone(), e);
                        }
                        end = end.max(e);
                    }
                    None => st.deferred.push((step, sc, anchor, offset)),
                }
            }
            if !progressed {
                let stuck = std::mem::take(&mut st.deferred);
                for (step, _, anchor, _) in stuck {
                    if st.names.contains(&anchor) {
                        self.error("G0062", step.line, format!("`after={anchor}` cycle — groups waiting on each other never start; a group can't wait on itself, however indirectly"));
                    } else {
                        self.error("G0062", step.line, format!("`after={anchor}` names no group — `after=` anchors on a NAMED `group` in this anim"));
                    }
                }
                return end;
            }
        }
    }

    /// Two steps that both pin `from=` on one property hold two different
    /// start values from play start (the capture rule holds a pin through
    /// its wait) — they'd fight from frame one. Load-time error, as
    /// shipped.
    fn double_pin_check(&mut self, anim: StrId, line: u32) {
        let mut seen: Vec<(NodeId, StrId)> = Vec::new();
        let mut errs: Vec<String> = Vec::new();
        // GP40: `background` and `background.a` in one anim would both
        // own the (node, property) lane and fight — one spelling per
        // property per anim.
        let mut lanes: Vec<(NodeId, PropKey, bool)> = Vec::new();
        for t in self.page.anims.iter().filter(|t| t.anim == anim) {
            let w = self.page.str(t.channel);
            if Channel::parse(w).is_some() {
                continue;
            }
            let Some((key, alpha)) = prop_lane(w) else { continue };
            if lanes.iter().any(|(n, k, a)| *n == t.node && *k == key && *a != alpha) {
                let nm = self.page.str(self.page.nodes[t.node].name).to_string();
                let bw = w.strip_suffix(".a").unwrap_or(w);
                errs.push(format!("{nm} animates both the whole `{bw}` and `{bw}.a` — the lane has one writer; pick one spelling"));
            } else if !lanes.contains(&(t.node, key, alpha)) {
                lanes.push((t.node, key, alpha));
            }
        }
        for t in self.page.anims.iter().filter(|t| t.anim == anim) {
            if t.from.is_none() {
                continue;
            }
            if seen.contains(&(t.node, t.channel)) {
                let nm = self.page.str(self.page.nodes[t.node].name).to_string();
                let ch = self.page.str(t.channel).to_string();
                errs.push(format!("two steps pin `from=` on {nm}.{ch} — both hold from play start and would fight from frame one"));
            } else {
                seen.push((t.node, t.channel));
            }
        }
        for e in errs {
            self.error("G0064", line, e);
        }
    }

    /// The shipped rule: `self` in an anim targets the node a style is
    /// applied to. A `transition … anim=X` carried to a wearer by a style
    /// (or written on the node) gets a per-wearer copy of X with the
    /// `self` lanes re-bound to that wearer; the original X keeps `self` =
    /// the file's root for code plays (GPAR86). Needed so the G4003
    /// two-writers fence — and the play itself — see the real target.
    fn bind_self_anims(&mut self) {
        if self.self_lanes.is_empty() {
            return;
        }
        let mut made: HashMap<(StrId, NodeId), StrId> = HashMap::new();
        for ti in 0..self.page.transitions.len() {
            let t = self.page.transitions[ti];
            if t.anim == NO_STR || t.node == NO_NODE || t.node >= NodeId::from_index(self.page.nodes.len()) {
                continue;
            }
            if !self.self_lanes.iter().any(|(a, _)| *a == t.anim) {
                continue;
            }
            let node = t.node;
            let new_id = match made.get(&(t.anim, node)) {
                Some(id) => *id,
                None => {
                    let wearer = self.page.str(self.page.nodes[node].name).to_string();
                    let full = format!("{wearer}/{}", self.page.str(t.anim));
                    let new_id = self.intern(&full);
                    if let Some(h) = self.page.anim_heads.iter().find(|h| h.anim == t.anim).copied() {
                        self.page.anim_heads.push(AnimHead { anim: new_id, ..h });
                    }
                    let src: Vec<(u32, AnimTrack)> = self.page.anims.iter().enumerate().filter(|(_, a)| a.anim == t.anim).map(|(i, a)| (i as u32, *a)).collect();
                    for (si, tr) in src {
                        let bound = if self.self_lanes.iter().any(|(a, i)| *a == t.anim && *i == si) { node } else { tr.node };
                        self.page.anims.push(AnimTrack { anim: new_id, node: bound, ..tr });
                    }
                    made.insert((t.anim, node), new_id);
                    new_id
                }
            };
            self.page.transitions[ti].anim = new_id;
        }
    }

    fn anim_chain<'d>(&mut self, anim: StrId, scope: &str, items: &'d [AstItem], scopes: &[&'d AstNode], chain_start: f32, st: &mut AnimSt<'d, '_>) -> f32 {
        // `and` joins the previous step; `then` starts after the whole and-cluster.
        let mut cluster_start = chain_start;
        let mut cluster_end = chain_start;
        // A staggered `each` in this cluster: its length only exists at
        // play time, so a `then` waiting on the cluster is refused.
        let mut cluster_unwaitable = false;
        let mut first = true;
        for item in items {
            let AstItem::Node(step) = item else {
                if let AstItem::Assign { key, line, .. } = item {
                    // GPAR86: a set would snap with no track to play —
                    // it was skipped without a word before.
                    self.error("G0063", *line, format!("`{key} = …` in an anim — anims are journeys, not assignments; write `{key} from=…, to=…, duration=…`"));
                }
                continue;
            };
            let (word, connective, target) = match step.type_word.as_str() {
                "and" | "then" => {
                    // `then delay=0.2` has no step word: the prop names it.
                    let w = step.name.clone().unwrap_or_else(|| if step.props.iter().any(|p| p.key == "delay") { "delay".to_string() } else { String::new() });
                    (w, step.type_word.as_str(), step.args.first().and_then(|a| a.as_ident()).map(str::to_string))
                }
                other => (other.to_string(), "", step.name.clone()),
            };
            // `after=<group>` places a step by REFERENCE — instead of a
            // connective, never beside one (one placement per step).
            let has_after = step.props.iter().any(|p| p.key == "after");
            if !first && connective.is_empty() && !has_after {
                self.error("G0061", step.line, "every step after the first says when it runs: `and` or `then`");
            }
            if first && !connective.is_empty() {
                self.error("G0061", step.line, "the first step stands alone — no `and`/`then`");
            }
            if connective == "then" {
                if cluster_unwaitable {
                    self.error("G0062", step.line, "`then` here waits on a play-time length — an `each … stagger_by=` has no end until it plays, so nothing sequences after it (`and` beside it is fine: that shares its start)");
                }
                cluster_start = cluster_end;
                cluster_unwaitable = false;
            }
            if !has_after {
                first = false;
            }
            let mut scopes: Vec<&AstNode> = scopes.to_vec();
            let end = match word.as_str() {
                "group" => {
                    for p in &step.props {
                        match p.key.as_str() {
                            "duration" => self.error("G0066", p.line, "`duration=` on a group reads as the chain's total — write `duration_each=` (each step's default)"),
                            "delay" => self.error("G0066", p.line, "`delay=` was retired — a pause is its own step (`then delay=0.3`), a shift is `offset=`"),
                            _ => {}
                        }
                    }
                    if step.children.is_empty() {
                        self.error("G0062", step.line, "a group with no steps plays nothing — give it lanes or delete it");
                    }
                    // Group names are `after=` anchors: anim-flat, unique.
                    if let Some(name) = &target {
                        if !st.names.insert(name.clone()) {
                            self.error("G0062", step.line, format!("two groups named `{name}` — group names are `after=` anchors, unique within an anim"));
                        }
                    }
                    let offset = step.props.iter().find(|p| p.key == "offset").and_then(|p| p.value.as_num()).unwrap_or(0.0);
                    if let Some(anchor) = step.props.iter().find(|p| p.key == "after").and_then(|p| p.value.as_ident()).map(str::to_string) {
                        if !connective.is_empty() {
                            self.error("G0062", step.line, "a step is positional (`and`/`then`) or referenced (`after=<group>`) — never both; one placement per step");
                        }
                        // Anchors resolve after the whole chain (forward
                        // references are legal) — park it; the positional
                        // flow doesn't see referenced steps.
                        st.deferred.push((step, scopes.clone(), anchor, offset));
                        cluster_start
                    } else {
                        scopes.push(step);
                        let e = self.anim_chain(anim, scope, &step.children, &scopes, (cluster_start + offset).max(0.0), st);
                        if let Some(name) = &target {
                            st.ends.insert(name.clone(), e);
                        }
                        e
                    }
                }
                "delay" => {
                    let d = step.props.iter().find(|p| p.key == "delay").and_then(|p| p.value.as_num()).unwrap_or(0.0);
                    if d < 0.0 {
                        self.error("G0062", step.line, "a pause can't be negative — overlapping a start is `offset=` on the shifted step");
                    }
                    cluster_start + d.max(0.0)
                }
                "each" => {
                    if st.each_of.is_some() {
                        self.error("G0062", step.line, "eaches don't nest — one fan-out names the wearers, its body stays bare");
                        cluster_start
                    } else {
                        let stagger = step.props.iter().find(|p| p.key == "stagger_by").and_then(|p| p.value.as_num()).unwrap_or(0.0);
                        // The wearers: one node, `self`, or everyone
                        // wearing `.Style` (file order — the shipped
                        // tree order).
                        let wearers: Vec<NodeId> = if let Some(style) = step.styles.first() {
                            match self.style_wearers.get(style) {
                                Some(w) if !w.is_empty() => w.clone(),
                                _ => {
                                    self.error("G0060", step.line, format!("`each .{style}`: no node wears that style — the fan-out would drive nothing"));
                                    Vec::new()
                                }
                            }
                        } else if let Some(nm) = &target {
                            let found = if nm == "self" {
                                if scope.is_empty() { if self.page.nodes.is_empty() { None } else { Some(NodeId::new(0)) } } else { self.page.find(scope) }
                            } else if scope.is_empty() {
                                self.page.find(nm).or_else(|| self.root_aliases.get(nm.as_str()).and_then(|t| self.page.find(t)))
                            } else {
                                let scoped = format!("{scope}/{nm}");
                                self.page.find(&scoped).or_else(|| self.root_aliases.get(&scoped).and_then(|t| self.page.find(t))).or_else(|| self.page.find(nm))
                            };
                            match found {
                                Some(id) => vec![id],
                                None => {
                                    self.error("G0060", step.line, format!("`each {nm}`: no node named `{nm}`"));
                                    Vec::new()
                                }
                            }
                        } else {
                            self.error("G0062", step.line, "`each` names its wearers — `each <node>` or `each .Style`");
                            Vec::new()
                        };
                        scopes.push(step);
                        let mut end = cluster_start;
                        for (k, w) in wearers.iter().enumerate() {
                            // Each wearer's copy is its own little world:
                            // bare paths target the wearer, group names
                            // don't leak in or out.
                            let mut sub = AnimSt { decls: st.decls, compiled: &mut *st.compiled, stack: &mut *st.stack, names: HashSet::new(), ends: HashMap::new(), deferred: Vec::new(), each_of: Some(*w) };
                            let e = self.anim_chain(anim, scope, &step.children, &scopes, cluster_start + k as f32 * stagger, &mut sub);
                            let e2 = self.flush_deferred(anim, scope, &mut sub);
                            end = end.max(e).max(e2);
                        }
                        if stagger > 0.0 {
                            cluster_unwaitable = true;
                        }
                        end
                    }
                }
                "play" => {
                    if st.each_of.is_some() {
                        self.error("G0062", step.line, "`play` doesn't go inside an each — its body is bare property lines for the wearers");
                        cluster_start
                    } else if let Some(pname) = &target {
                        // The decl table resolves the name (the widget
                        // scope's copy first); a not-yet-compiled target
                        // compiles on demand — forward references are
                        // legal, the stack breaks cycles.
                        let idx = st.decls.iter().position(|(sc, d)| sc == scope && d.name.as_deref() == Some(pname.as_str())).or_else(|| st.decls.iter().position(|(sc, d)| sc.is_empty() && d.name.as_deref() == Some(pname.as_str())));
                        match idx {
                            None => {
                                self.error("G0062", step.line, format!("`play {pname}`: unknown anim"));
                                cluster_start
                            }
                            Some(idx) => {
                                let (dscope, dnode) = &st.decls[idx];
                                let dname = dnode.name.clone().unwrap_or_default();
                                let full = if dscope.is_empty() { dname } else { format!("{dscope}/{dname}") };
                                if st.stack.contains(&full) {
                                    self.error("G0062", step.line, format!("play cycle: {} -> {full} — an anim can't wait on itself, however indirectly", st.stack.join(" -> ")));
                                    cluster_start
                                } else {
                                    if !st.compiled[idx] {
                                        self.compile_anim_decl(idx, st.decls, st.compiled, st.stack);
                                    }
                                    let played = self.intern(&full);
                                    let head = self.page.anim_heads.iter().find(|h| h.anim == played).map(|h| (h.repeat, h.speed));
                                    if head.map(|(r, _)| r).unwrap_or(false) {
                                        self.error("G0062", step.line, format!("`play {pname}`: a `repeat=forever` anim never completes — it can't be a step in a chain"));
                                        cluster_start
                                    } else {
                                        // Inline the played anim's tracks at
                                        // this step's start, its `speed=`
                                        // folded in; internal group names
                                        // stay private (only lanes copy).
                                        let speed = head.map(|(_, sp)| sp).unwrap_or(1.0).max(0.001);
                                        let src: Vec<(u32, AnimTrack)> = self.page.anims.iter().enumerate().filter(|(_, t)| t.anim == played).map(|(i, t)| (i as u32, *t)).collect();
                                        let mut total = 0.0f32;
                                        for (si, t) in &src {
                                            total = total.max(t.start + t.duration * if t.autoreverse { 2.0 } else { 1.0 });
                                            let idx_new = self.page.anims.len() as u32;
                                            self.page.anims.push(AnimTrack { anim, start: cluster_start + t.start / speed, duration: t.duration / speed, ..*t });
                                            if self.self_lanes.iter().any(|(a, i)| *a == played && i == si) {
                                                self.self_lanes.push((anim, idx_new));
                                            }
                                        }
                                        cluster_start + total / speed
                                    }
                                }
                            }
                        }
                    } else {
                        self.error("G0062", step.line, "`play` names an anim — `play <name>`");
                        cluster_start
                    }
                }
                "key" => {
                    self.error("G0062", step.line, "`key` frames are children of a property line — `box.opacity { key t=0, v=0 }`");
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
                                if self.page.find(&full).map(|id| self.page.nodes[id].kind) != Some(Kind::Slot) {
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
                        // Inside an `each` the fan-out names the target:
                        // paths stay BARE, a prefix is refused.
                        _ if st.each_of.is_some() && Channel::parse(path).is_some() => (String::new(), path.to_string()),
                        Some((n, rest)) if Channel::parse(rest).is_some() => {
                            if st.each_of.is_some() {
                                self.error("G0062", step.line, format!("`{path}`: the `each` already names the target — write the property bare (`{rest}`)"));
                                continue;
                            }
                            (n.to_string(), rest.to_string())
                        }
                        // GP40 prop lanes: `<node>.<prop word>[.a]` — any
                        // word a `transition` rule can move (checked below,
                        // once the node and its kind are known). Bare inside
                        // `each`, like the motion words.
                        Some((n, rest)) if prop_lane(rest).is_some() => {
                            if st.each_of.is_some() {
                                self.error("G0062", step.line, format!("`{path}`: the `each` already names the target — write the property bare (`{rest}`)"));
                                continue;
                            }
                            (n.to_string(), rest.to_string())
                        }
                        _ if st.each_of.is_some() && prop_lane(path).is_some() => (String::new(), path.to_string()),
                        _ => {
                            if path.ends_with(".modulate.a") || path == "modulate.a" {
                                self.error("G0075", step.line, "`modulate.a` is a Godot engine word — write `opacity`");
                            } else if Channel::parse(path).is_some() || prop_lane(path).is_some() {
                                self.error("G0063", step.line, format!("`{path}` needs a node: `<node>.{path}` (bare paths only inside `each`)"));
                            } else {
                                self.error("G0063", step.line, format!("`{path}`: want `<node>.` + a motion word (`translate.x|translate.y|scale|scale.x|scale.y|rotate|opacity`) or an animatable property word (`background`, `corner_radius`, `stop_1_color.a`…)"));
                            }
                            continue;
                        }
                        }
                    };
                    let found = if node_name.is_empty() {
                        st.each_of
                    } else if node_name == "self" {
                        if scope.is_empty() {
                            // Page scope: `self` is the file's root
                            // (GPAR86, the addon's reading).
                            if self.page.nodes.is_empty() { None } else { Some(NodeId::new(0)) }
                        } else {
                            self.page.find(scope)
                        }
                    } else if scope.is_empty() {
                        self.page.find(&node_name).or_else(|| self.root_aliases.get(&node_name).and_then(|t| self.page.find(t)))
                    } else {
                        let scoped = format!("{scope}/{node_name}");
                        self.page.find(&scoped).or_else(|| self.root_aliases.get(&scoped).and_then(|t| self.page.find(t))).or_else(|| self.page.find(&node_name))
                    };
                    let Some(node) = found else {
                        self.error("G0060", step.line, format!("`{path}`: no node named `{node_name}`{}", if node_name == "self" { " (`self` is the widget's root, inside a widget's own anim)" } else { "" }));
                        continue;
                    };
                    // GP40 prop lanes: the word must be one anims may
                    // drive (the `transition` list + live stops), `.a`
                    // needs a color word, and the target kind must take
                    // the word at all — the same kind-words table `when`
                    // responses answer to.
                    let plane: Option<(PropKey, bool)> = if Channel::parse(&channel).is_some() { None } else { prop_lane(&channel) };
                    if let Some((key, alpha_only)) = plane {
                        let base_word = channel.strip_suffix(".a").unwrap_or(&channel).to_string();
                        if !crate::runtime::animatable(key) {
                            self.error("G0063", step.line, format!("`{base_word}` isn't an animatable word — anims move what `transition` rules move (colors, opacity, corner_radius, border_thickness, font_size, sizes, margins, paddings) plus the live stop family"));
                            continue;
                        }
                        if alpha_only && !key.is_color() {
                            self.error("G0063", step.line, format!("`.a` moves a color's alpha alone — `{base_word}` isn't a color word"));
                            continue;
                        }
                        if !self.check_prop_kind(&base_word, key, self.page.nodes[node].kind, step.line) {
                            continue;
                        }
                    }
                    let num = |key: &str| step.props.iter().find(|p| p.key == key).and_then(|p| p.value.as_num());
                    if step.props.iter().any(|p| p.key == "delay") {
                        self.error("G0066", step.line, "`delay=` on a property line was retired — a pause is its own step (`then delay=0.3`), a shift is `offset=`");
                    }
                    if has_after {
                        self.error("G0062", step.line, "`after=` goes on `group` lines — wrap the step: `group <name> after=<anchor> { … }`");
                    }
                    // A style transition's `anim=` re-binds `self` lanes to
                    // each wearer (`bind_self_anims`) — remember which
                    // tracks were written as `self`.
                    let self_target = node_name == "self" && scope.is_empty();
                    let lane_mark = self.page.anims.len();
                    let channel_id = self.intern(&channel);
                    let keys: Vec<&AstNode> = step.children.iter().filter_map(|i| if let AstItem::Node(k) = i { Some(k) } else { None }).filter(|k| k.type_word == "key").collect();
                    scopes.push(step);
                    let autoreverse = scopes.iter().rev().find_map(|s| s.props.iter().find(|p| p.key == "autoreverse").map(|p| p.value == AstValue::Bool(true))).unwrap_or(false);
                    if !keys.is_empty() {
                        if let Some((key, false)) = plane {
                            if key.is_color() {
                                self.error("G0064", step.line, "`key` children carry numbers — a color lane spells its segments as chained steps (`then <node>.background to=…`)");
                                continue;
                            }
                        }
                        if num("duration").is_some() {
                            self.error("G0064", step.line, "`duration=` beside `key` children — the keys own the timeline; the last key's `t` is the length");
                        }
                        if step.props.iter().any(|p| p.key == "to") {
                            self.error("G0064", step.line, "`to=` beside `key` children — the keys own the values");
                        }
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
                            self.page.anims.push(AnimTrack { anim, node, channel: channel_id, from: from.map(Value::VNum), to: Value::VNum(v), start, duration, easing, current: false, autoreverse: false, from_percent: false, to_percent: false });
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
                                self.page.anims.push(AnimTrack { anim, node, channel: channel_id, from: None, to: Value::VNum(v0), start, duration, easing, current: false, autoreverse: false, from_percent: false, to_percent: false });
                                end = end.max(start + duration);
                            }
                        }
                        if self_target {
                            for i in lane_mark..self.page.anims.len() {
                                self.self_lanes.push((anim, i as u32));
                            }
                        }
                        cluster_end = cluster_end.max(end);
                        continue;
                    }
                    // A whole-value prop lane's endpoints go through the
                    // general value reader, so palette words and mode
                    // pairs spell the same as anywhere; motion channels
                    // and `.a` lanes stay numbers.
                    let value_key: Option<PropKey> = match plane {
                        Some((key, false)) => Some(key),
                        _ => None,
                    };
                    let from_ast = step.props.iter().find(|p| p.key == "from").map(|p| (p.value.clone(), p.line));
                    let (from, current): (Option<Value>, bool) = match &from_ast {
                        Some((v, _)) if v.as_ident() == Some("current") => (None, true),
                        Some((v, line)) => match value_key {
                            Some(key) => (Some(self.value(v, *line, Some(key))), false),
                            None => (v.as_num().map(Value::VNum), false),
                        },
                        None => (None, false),
                    };
                    // `from=-100%` / `to=100%`: a fraction of the node's own
                    // box, translate lanes only — a percent of a scale, a
                    // rotation or an opacity has no axis to measure.
                    let to_ast = step.props.iter().find(|p| p.key == "to").map(|p| (p.value.clone(), p.line));
                    let is_percent = |v: &Option<(AstValue, u32)>| matches!(v, Some((AstValue::Num { unit, .. }, _)) if unit == "%");
                    let from_percent: bool = is_percent(&from_ast);
                    let to_percent: bool = is_percent(&to_ast);
                    if (from_percent || to_percent) && !(channel == "translate.x" || channel == "translate.y") {
                        self.error("G0064", step.line, format!("`{path}`: a `%` value is a fraction of the node's own box — only `translate.x` (of its width) and `translate.y` (of its height) take one"));
                        continue;
                    }
                    let to: Value = match (&to_ast, value_key) {
                        (Some((v, line)), Some(key)) => self.value(v, *line, Some(key)),
                        (Some((v, _)), None) => match v.as_num() {
                            Some(n) => Value::VNum(n),
                            None => {
                                self.error("G0064", step.line, format!("`{path}` takes a number `to=`"));
                                continue;
                            }
                        },
                        (None, _) => {
                            self.error("G0064", step.line, format!("`{path}` needs `to=`"));
                            continue;
                        }
                    };
                    let Some(duration) = num("duration").or_else(|| Self::anim_num(&scopes, "duration_each")) else {
                        self.error("G0064", step.line, format!("`{path}` has no length — write `duration=` on the line or `duration_each=` on an enclosing group or the anim header"));
                        continue;
                    };
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
                    self.page.anims.push(AnimTrack { anim, node, channel: channel_id, from, to, start, duration, easing, current, autoreverse, from_percent, to_percent });
                    if self_target {
                        for i in lane_mark..self.page.anims.len() {
                            self.self_lanes.push((anim, i as u32));
                        }
                    }
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
        // A state word as an arrangement name would make `when held`
        // two things (GPAR93, the addon's fence).
        if matches!(name.as_str(), "held" | "hovered" | "focused" | "focus_within" | "selected" | "disabled" | "pressed" | "hover" | "focus" | "checked" | "dragging") {
            self.error("G0032", n.line, format!("`{name}` is a state word — an arrangement cannot wear it (`when {name}` must mean one thing)"));
            return;
        }
        // Arrangements declare NAMES, nothing else — poses go in
        // `when <name> { }` blocks (GPAR93).
        if !n.props.is_empty() || !n.children.is_empty() {
            self.error("G0032", n.line, format!("`arrangement {name}` declares a name, nothing else — poses go in a `when {name} {{ … }}` block"));
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
            return Some(Some(State::Arrangement(BASE)));
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
            // The enclosing control's own words come before the respell
            // aliases: a declared `state dragging` IS that control's
            // state (GPAR116), not a misspelling of `held`.
            _ if self.custom_state_stack.iter().any(|l| l.iter().any(|s| s == word)) => {
                let id = self.intern(word);
                State::Custom(id)
            }
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
                let id = self.page.strings.iter().position(|s| s == word).map(|i| StrId::from_index(i));
                match id.and_then(|id| self.page.arrangements.iter().position(|a| *a == id)) {
                    Some(i) => State::Arrangement(ArrangementId::from_index(i)),
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
        matches!(key, PropKey::Background | PropKey::Color | PropKey::BorderColor | PropKey::Stops | PropKey::FontColor | PropKey::SelectionColor | PropKey::StopColor(_))
    }

    /// A palette word's bytes for one mode, following override links and
    /// aliases at USE time — so `theme { red = X }` moves `danger` too
    /// (GPAR88). Dark reads the `theme dark { }` overrides first and falls
    /// back to the light chain; `text_muted`'s light half is the alias
    /// `gray` (the addon's rule), its dark half the mode table's.
    fn palette_lookup(&self, word: &str, dark: bool) -> Option<Color_Straight_Srgb_4xU8_RGBA> {
        let mut cur = word;
        for _ in 0..16 {
            if dark {
                if let Some(next) = self.palette_links_dark.get(cur) {
                    cur = next;
                    continue;
                }
                if let Some(&c) = self.palette_dark.get(cur) {
                    return Some(c);
                }
            }
            if let Some(next) = self.palette_links.get(cur) {
                cur = next;
                continue;
            }
            if let Some(&c) = self.palette.get(cur) {
                return Some(c);
            }
            if let Some((_, of)) = PALETTE_ALIASES.iter().find(|(a, _)| *a == cur) {
                cur = of;
                continue;
            }
            if let Some(&(l, d)) = self.mode_palette.get(cur) {
                if !dark && cur == "text_muted" {
                    cur = "gray";
                    continue;
                }
                return Some(if dark { d } else { l });
            }
            return None;
        }
        None
    }

    /// A color from a hex, palette word or `$ref` (the ref stays live).
    fn color_value(&mut self, v: &AstValue, line: u32) -> Option<Color_Straight_Srgb_4xU8_RGBA> {
        match v {
            AstValue::Color { rgba, opacity, .. } => {
                let c = Color_Straight_Srgb_4xU8_RGBA(*rgba);
                Some(opacity.map_or(c, |o| c.with_opacity(o)))
            }
            AstValue::Ident { word, opacity, .. } => {
                let c = self.palette_lookup(word, false)?;
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
    fn stop_value(&mut self, c: Color_Straight_Srgb_4xU8_RGBA, pos: Option<f32>, key: Option<PropKey>, line: u32) -> Value {
        if key == Some(PropKey::Stops) {
            return Value::VStop { color: c, pos };
        }
        if let Some(p) = pos {
            self.error("G0040", line, format!("`@{p}` positions a gradient stop — it only goes on entries of a Gradient's `stops=` list (`stops=[white@0, blue/0.5@0.24, accent@1]`)"));
        }
        Value::VColor(c)
    }

    /// `$edge@0.24` — a theme color as a gradient stop (GPAR86): the
    /// ref bakes to its color here like every stop (a mode pair takes
    /// its light half — G2008 already fences mode words in `stops=`).
    fn ref_stop(&mut self, v: Value, pos: Option<f32>, name: &str, line: u32) -> Value {
        let Some(p) = pos else { return v };
        match crate::runtime::deref(&self.page, NO_STR, v) {
            Value::VColor(c) => Value::VStop { color: c, pos: Some(p) },
            _ => {
                self.error("G2007", line, format!("`${name}@{p}` — `@` places a color stop, and `${name}` is not a color"));
                v
            }
        }
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
                let c = Color_Straight_Srgb_4xU8_RGBA(*rgba);
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
                // An image is its own value: whatever `source=` names (a
                // host picture like `preview://brush` too), and a literal
                // naming an image file anywhere (a theme `$icon` a
                // `source=$icon` reads) — so a draw carries the build-wide
                // id and never a string.
                let is_image = key == Some(PropKey::Source) || IMAGE_EXTS.iter().any(|e| lower.ends_with(e));
                if is_image {
                    return if text.is_empty() { Value::VImage(NO_IMAGE) } else { Value::VImage(self.image_key(text)) };
                }
                let text_id = self.intern(text);
                let key_id = match term {
                    Some(k) => self.key(k, text, line),
                    None => NO_KEY,
                };
                Value::VText { text: text_id, key: key_id }
            }
            AstValue::Ref { name, opacity, pos } => {
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
                    let v = match (inner, opacity) {
                        (Value::VColor(c), Some(o)) => Value::VColor(c.with_opacity(*o)),
                        (Value::VRef { name, .. }, Some(o)) => Value::VRef { name, opacity: Some(*o) },
                        (v, _) => v,
                    };
                    return self.ref_stop(v, *pos, name, line);
                }
                let id = self.intern(name);
                let v = Value::VRef { name: id, opacity: *opacity };
                self.ref_stop(v, *pos, name, line)
            }
            AstValue::Ident { word, opacity, pos } => {
                let color_position = key.map(Self::is_color_key).unwrap_or(false);
                if color_position {
                    if key == Some(PropKey::Stops) && word == "transparent" {
                        // Straight-alpha blending fades THROUGH the hidden
                        // RGB; the file says which.
                        self.error("G2006", line, "`transparent` in `stops=` hides which color the fade passes through — spell it: `white/0` or `black/0`");
                    }
                    if let Some(l) = self.palette_lookup(word, false) {
                        let d = self.palette_lookup(word, true).unwrap_or(l);
                        if l != d {
                            if key == Some(PropKey::Stops) {
                                // A stop bakes ONE color; baking the light
                                // half would silently un-flip the gradient.
                                self.error("G2008", line, format!("`{word}` flips with the dark variant and a gradient stop bakes one color — write the hex for the mode you mean"));
                                return self.stop_value(l, *pos, key, line);
                            }
                            if let Some(p) = pos {
                                self.error("G0040", line, format!("`@{p}` positions a gradient stop — it only goes on entries of a Gradient's `stops=` list"));
                            }
                            let (l, d) = opacity.map_or((l, d), |o| (l.with_opacity(o), d.with_opacity(o)));
                            return Value::VColorPair { light: l, dark: d };
                        }
                        let c = opacity.map_or(l, |o| l.with_opacity(o));
                        return self.stop_value(c, *pos, key, line);
                    }
                    let mut pool: Vec<String> = self.palette.keys().cloned().collect();
                    pool.extend(self.mode_palette.keys().cloned());
                    pool.extend(PALETTE_ALIASES.iter().map(|(a, _)| (*a).to_string()));
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
            // A named call argument: the value passes through; the name
            // is the call site's to read (GPAR32 — none does yet, GP40).
            AstValue::Named { value, .. } => self.value(value, line, key),
            AstValue::Call { name, args } if name == "live" && !args.is_empty() => {
                // `live($value)` — a control prop the host drives; the
                // value here is its start. In a TEXT position the number
                // SHOWS, so display rounding applies: 0.1 by default
                // (the addon's user-caught drag that printed
                // 10.234235), `round($value, step)` picks the step.
                let (inner, step) = match &args[0] {
                    AstValue::Call { name: rname, args: rargs } if rname == "round" && !rargs.is_empty() => {
                        let step = rargs.get(1).map(|v| self.value(v, line, None)).and_then(|v| if let Value::VNum(x) = v { Some(x) } else { None }).unwrap_or(1.0);
                        (self.value(&rargs[0], line, key), step)
                    }
                    a => (self.value(a, line, key), 0.1),
                };
                if key == Some(PropKey::Text) {
                    if let Value::VNum(x) = crate::runtime::deref(&self.page, NO_STR, inner) {
                        let text = crate::page::live_text(x, step);
                        let id = self.intern(&text);
                        // A later code write of the control's value
                        // rewrites this label (GP33 live-label follow).
                        if let Some(root) = self.control_root_path() {
                            self.pending_live = Some((root, step));
                        }
                        return Value::VText { text: id, key: NO_KEY };
                    }
                }
                inner
            }
            AstValue::Call { name, args } if name == "round" && !args.is_empty() => {
                self.error("G0080", line, "`round()` goes inside live() — it picks the display step (`text=live(round($value, 1))`)");
                self.value(&args[0], line, key)
            }
            AstValue::Call { name, args } if name == "bind" && !args.is_empty() => {
                self.error("G0081", line, "`bind()` is gone — write `live($value)`");
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
        // GP40 step 4: `stop_<n>_color`/`_offset` are the LIVE half of a
        // Gradient's stops (a `when` response or a code write) — the
        // declared look has ONE spelling, `stops=`.
        if let Some(PropKey::StopColor(_) | PropKey::StopOffset(_)) = PropKey::from_word(&p.key) {
            if self.in_when == 0 && kind == Some(Kind::Gradient) {
                self.error("G2005", p.line, format!("`{}=` writes a stop LIVE (a `when` response, or code) — the declared look is spelled in `stops=`", p.key));
                return None;
            }
        }
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
            let takes_list = matches!(key, PropKey::Cols | PropKey::Areas | PropKey::Stops | PropKey::BorderDash | PropKey::Dash | PropKey::Window);
            if !takes_list {
                self.error("G0040", p.line, format!("`{}=` doesn't take a list — only Grid's `cols=`/`areas=`, Gradient's `stops=`, `border_dash=`, an Arc's `dash=` and an Image's `window=` do", p.key));
                return None;
            }
        }
        if key == PropKey::Stops {
            self.check_stops(&p.value, p.line);
        }
        if key == PropKey::BorderDash || key == PropKey::Dash {
            self.check_border_dash(key.word(), &p.value, p.line);
        }
        if key == PropKey::Window {
            self.check_window(&p.value, p.line);
        }
        let mut value = self.value(&p.value, p.line, Some(key));
        // `pivot=` is a 0..1 box point — the fixed spot `scale` and
        // `rotate` transform about. A bare number or an out-of-box point
        // would silently transform about nowhere.
        if key == PropKey::Pivot && !matches!(value, Value::VSize(Some(x), Some(y)) if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)) {
            self.error("G2005", p.line, format!("`pivot=` is a 0..1 box point — `pivot=0.5x0.5` (the center), `pivot=0x0` (top-left), not `{}`", spell(&p.value)));
            return None;
        }
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
            PropKey::TextFit => Some(&["none", "stretch"]),
            PropKey::TextFlip => Some(&["none", "horizontal", "vertical", "both"]),
            PropKey::Allow => Some(&["horizontal", "vertical", "both"]),
            PropKey::Bars => Some(&["auto", "never", "always", "reserve"]),
            PropKey::Orientation => Some(&["horizontal", "vertical"]),
            PropKey::Select => Some(&["one", "many", "off"]),
            PropKey::TouchTarget | PropKey::KeyboardTarget => Some(&["off"]),
            PropKey::GradientFill => Some(&["linear", "radial", "conic"]),
            PropKey::GradientRepeat => Some(&["none", "repeat", "mirror"]),
            PropKey::Loading => Some(&["lazy", "eager"]),
            PropKey::Sampling => Some(&["smooth", "pixel"]),
            _ => None,
        };
        if let Some(words) = closed_words {
            let ok = match value {
                Value::VIdent(id) => {
                    // Words compare case-folded (the addon's `_enum_fold`:
                    // `halign=Center` means center) and the CANONICAL
                    // spelling is what gets stored, so every later reader
                    // stays an exact compare. The addon's punctuation
                    // stripping is not ported — it matched Godot editor
                    // labels ("Word (Smart)"), a surface Rust doesn't
                    // have; underscores are part of the word.
                    let canon = words.iter().copied().find(|w| w.eq_ignore_ascii_case(self.page.str(id)));
                    if let Some(w) = canon {
                        if w != self.page.str(id) {
                            value = Value::VIdent(self.intern(w));
                        }
                    }
                    canon.is_some()
                }
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
        if matches!(key, PropKey::RevealX | PropKey::RevealY) {
            match value {
                Value::VNum(v) if (0.0..=1.0).contains(&v) => {}
                Value::VRef { .. } | Value::VCall { .. } => {}
                Value::VNum(_) => {
                    self.error("G0078", p.line, format!("`{}=` is the SHOWN fraction — 0..1 (0 concealed, 1 whole)", p.key));
                    return None;
                }
                _ => {
                    self.error("G0078", p.line, format!("`{}=` takes a 0..1 number — got `{}`", p.key, spell(&p.value)));
                    return None;
                }
            }
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
    /// `border_dash=[4, 1]`: a non-empty list of positive lengths in
    /// `border_thickness` units (WPF StrokeDashArray — `4 1` at a 1-epx
    /// border is 4-epx dashes, 1-epx gaps). G2009 names the bad spelling.
    /// `border_dash=` (plates, Circles) and an Arc's `dash=`: the same list
    /// of positive lengths in stroke units.
    fn check_border_dash(&mut self, word: &str, v: &AstValue, line: u32) {
        let unit: &str = if word == "dash" { "thickness" } else { "border_thickness" };
        let AstValue::List(items) = v else {
            self.error("G2009", line, format!("`{word}=` takes the dash/gap lengths — `{word}=[4, 1]` (in {unit} units)"));
            return;
        };
        if items.is_empty() {
            self.error("G2009", line, format!("`{word}=` is empty — give the dash and gap lengths (`{word}=[4, 1]`)"));
            return;
        }
        for it in items {
            match it {
                AstValue::Num { value, .. } if *value > 0.0 => {}
                other => {
                    self.error("G2009", line, format!("`{word}=` entries are positive lengths in {unit} units — got `{}`", spell(other)));
                    return;
                }
            }
        }
    }

    /// `window=[left, top, width, height]`: four numbers in the texture's
    /// own pixels, the corner at or past 0 and a positive size — a
    /// zero-sized or negative window would draw nothing and say nothing.
    fn check_window(&mut self, v: &AstValue, line: u32) {
        let nums: Option<Vec<f32>> = match v {
            AstValue::List(items) if items.len() == 4 => items.iter().map(AstValue::as_num).collect(),
            _ => None,
        };
        match nums.as_deref() {
            Some([l, t, w, h]) if *l >= 0.0 && *t >= 0.0 && *w > 0.0 && *h > 0.0 => {}
            _ => self.error("G2010", line, format!("`window=` is the texture rectangle to show — `window=[left, top, width, height]` in the texture's pixels, corner at or past 0 and a positive size, not `{}`", spell(v))),
        }
    }

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
                AstValue::Color { pos, .. } | AstValue::Ident { pos, .. } | AstValue::Ref { pos, .. } => *pos,
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
        let fill = find("fill").and_then(|p| p.value.as_ident()).unwrap_or("linear").to_ascii_lowercase();
        if !matches!(fill.as_str(), "linear" | "radial" | "conic") {
            self.error("G2005", n.line, format!("`fill={fill}`: want linear, radial or conic"));
        }
        if find("angle").is_some() && fill != "linear" {
            self.error("G2005", n.line, "`angle=` only aims LINEAR gradients — a round fill (radial) has no direction");
        }
        // `from=`/`to=` place the ramp as 0..1 box points; `angle=` aims
        // it — the two say the same thing two ways, so spell one.
        let fromto = find("from").or_else(|| find("to"));
        if find("angle").is_some() && fromto.is_some() {
            self.error("G2005", n.line, "`angle=` next to `from=`/`to=` — both aim the ramp; spell one");
        }
        for key in ["from", "to"] {
            if let Some(p) = find(key) {
                match &p.value {
                    AstValue::Size { w: Some(x), h: Some(y) } if (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y) => {}
                    AstValue::Size { .. } => self.error("G2005", p.line, format!("`{key}=` parts are 0..1 fractions of the box — `{key}=0.25x0.75`")),
                    _ => self.error("G2005", p.line, format!("`{key}=` on a Gradient is a 0..1 box point — `{key}=0.25x0.75`")),
                }
            }
        }
        // With `repeat=mirror` the ramp folds at the middle: stop
        // positions live in 0..0.5, the mirror paints the way back.
        if find("repeat").and_then(|p| p.value.as_ident()) == Some("mirror") {
            if let Some(sp) = find("stops") {
                if let AstValue::List(items) = &sp.value {
                    for it in items {
                        let pos = match it {
                            AstValue::Ref { pos, .. } => *pos,
                            AstValue::Ident { pos, .. } => *pos,
                            AstValue::Color { pos, .. } => *pos,
                            _ => None,
                        };
                        if pos.map(|p| p > 0.5).unwrap_or(false) {
                            self.error("G2005", sp.line, "`repeat=mirror` folds the ramp at the middle — stop positions stop at 0.5 (the mirror paints the way back)");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// `repeat x [as $b] by $b.field`: `as` names the binding (a `$ref`),
    /// `by` keys the item on that binding, and the binding may not shadow
    /// a `$` value in scope (G9002/G9003/G9004, the addon's codes). `by`
    /// is optional here: the `max=N` form has no items to key (GP27).
    fn repeat_clauses(&mut self, n: &AstNode) {
        let binding = match n.clauses.iter().find(|(k, _)| k == "as") {
            Some((_, AstValue::Ref { name, opacity: None, pos: None })) if !name.contains('.') => name.clone(),
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
                return TmpNode { node: Node { kind: Kind::Layers, line: n.line, ..Node::EMPTY }, props: Vec::new(), handlers: Vec::new(), whens: Vec::new(), transitions: Vec::new(), children: Vec::new(), custom_states: Vec::new(), key_claim: 0, wearing: Vec::new(), driver: None, live: None };
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
            // An instance-line `when` may answer with `slot.` paths —
            // rewritten onto their real addresses with the same check a
            // `for=` style gets at its declaration.
            if def.control && inst_states.iter().any(|k| k.type_word == "when" && when_has_slot_path(k)) {
                let target = SlotTarget::Control(n.type_word.clone(), slot_contract(&def.body, &self.widgets));
                for k in &mut inst_states {
                    if k.type_word == "when" {
                        self.rewrite_slot_when(k, Some(&target));
                    }
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
            self.slot_writes.push(HashMap::new());
            if def.control {
                self.control_scopes.insert(self.prefix.join("/"));
            }
            self.custom_state_stack.push(if def.control { def.states.clone() } else { Vec::new() });
            // The body root's own name stays addressable (`a/root` IS the
            // instance `a` — the addon's lookup.root-self): an alias, so
            // the widget's anims and responses can target the root by the
            // name its body wrote.
            if let Some(rn) = &def.body.name {
                let full = self.prefix.join("/");
                self.root_aliases.insert(format!("{full}/{rn}"), full.clone());
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
            // The body's own responses are the definition's DEFAULTS
            // relative to this instance: an instance-site `when`
            // overriding one per state is the normal pattern, not a
            // duplicate (GPAR94's one-writer check skips them).
            for w in &mut t.whens {
                w.styled = true;
            }
            self.instance_on.pop();
            self.style_scopes.pop();
            self.widget_stack.pop();
            self.slot_writes.pop();
            // `when`/`transition` blocks written under the INSTANCE line
            // land on the instance root (the shipped _collect_states on
            // the root; GPAR68) — they were dropped without a word.
            for k in &inst_states {
                if k.type_word == "when" {
                    // GP37: a control's contract lists its states — a
                    // `when held` on an instance whose script never
                    // raises `held` would wait forever; refused naming
                    // what IS available (the shipped
                    // err-undeclared-contract-state).
                    if def.control {
                        let mut bad = false;
                        for word in k.name.iter().map(String::as_str).chain(k.args.iter().filter_map(|a| a.as_ident())) {
                            let control_word = matches!(word, "held" | "hovered" | "focused" | "focus_within" | "selected" | "disabled");
                            let declared = def.states.iter().any(|w| w == word) || def.state_builtins.iter().any(|w| w == word);
                            let view_word = word.starts_with("screen.") || self.page.arrangements.iter().any(|a| self.page.str(*a) == word);
                            if (control_word || !view_word) && !declared {
                                let mut avail: Vec<String> = def.state_builtins.iter().chain(def.states.iter()).map(|w| format!("`{w}`")).collect();
                                if avail.is_empty() {
                                    avail.push("none".to_string());
                                }
                                self.error("G0036", k.line, format!("'{word}' isn't available on `{}` — its contract raises {}", n.type_word, avail.join(", ")));
                                bad = true;
                            }
                        }
                        if bad {
                            continue;
                        }
                    }
                    let before = self.diags.len();
                    if let Some(tw) = self.when(k, t.node.kind) {
                        t.whens.push(tw);
                    } else {
                        // A control's OWN `state` word (`when dragging`)
                        // has nothing to bind until contract states land
                        // (GP37) — parked as a warning, never a silent
                        // drop, so copied files keep building.
                        self.diags.truncate(before);
                        self.warning("G0036", k.line, "an instance `when` names no declared state of this control (`state a, b` in its body would declare it) — the block is parked");
                    }
                } else {
                    let tr = self.transition(k, PENDING_NODE);
                    t.transitions.push(tr);
                }
            }
            if def.host.is_none() && !def.control {
                // A hostless widget's `when screen.*` blocks pose its own
                // body (D14): one copy per instance, the body's names under
                // the instance's prefix, resolved with the page's top-level
                // whens so the same one-writer and seat checks apply.
                let inst_full: String = self.prefix.join("/");
                for w in &def.whens {
                    let Some(state) = self.state_word(w, true) else { continue };
                    let mut items: Vec<AstItem> = w.props.iter().map(|p| AstItem::Assign { key: format!("{inst_full}/{}", p.key), dollar: false, value: p.value.clone(), line: p.line }).collect();
                    for item in &w.children {
                        match item {
                            AstItem::Assign { key, dollar, value, line } => items.push(AstItem::Assign { key: format!("{inst_full}/{key}"), dollar: *dollar, value: value.clone(), line: *line }),
                            other => items.push(other.clone()),
                        }
                    }
                    self.deferred_whens.push((state, items, w.line, true));
                }
            } else {
                // A `for=` widget's scope responses answer the hosting control.
                for w in &def.whens {
                    if let Some(mut tw) = self.when(w, t.node.kind) {
                        tw.host = def.host.is_some();
                        t.whens.push(tw);
                    }
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
            self.custom_state_stack.pop();
            if def.control && !def.states.is_empty() {
                t.custom_states = def.states.iter().map(|w| self.intern(w)).collect();
            }
            if def.control {
                t.key_claim = def.keys;
                if let Some(b) = def.behavior.clone() {
                    let drag_started = if def.signals.iter().any(|s| s == "drag_started") { self.intern("drag_started") } else { NO_STR };
                    let drag_ended = if def.signals.iter().any(|s| s == "drag_ended") { self.intern("drag_ended") } else { NO_STR };
                    self.slider_behavior(&b, drag_started, drag_ended);
                }
            }
            // The contract's `state_builtin focused` line is the opt-in
            // that makes the instance root keyboard-reachable (GP26);
            // push_slot folds in the `keyboard_target=off` opt-out.
            if def.control && def.state_builtins.iter().any(|w| w == "focused") {
                t.node.focusable = true;
            }
            // The root remembers its widget: the page build emits one
            // `<Widget>Names` struct per widget from these roots.
            t.node.widget = self.intern(&n.type_word);
            self.prefix.pop();
            self.fills.pop();
            self.scopes.pop();
            let _ = def.line;
            return t;
        }

        // `layout Flow { … }` inside a List: the items' container, spelled
        // by the word after `layout`. Its rules (G9017, the addon's):
        // inside a List only, a closed kind set, and it holds the rows —
        // statics stay on the List body.
        if n.type_word == "layout" {
            if parent_kind != Some(Kind::List) {
                self.error("G9017", n.line, "a `layout` block picks a List's rows' container — it lives inside a `List` body");
            }
            match n.name.as_deref() {
                Some("Row" | "Flow" | "Tiles" | "Column") => {}
                Some(w) => self.error("G9017", n.line, format!("`layout {w}` — the set is closed: Row, Flow, Tiles or Column")),
                None => self.error("G9017", n.line, "`layout` names its container kind: `layout Row {{ … }}`"),
            }
            let inner: Vec<&AstNode> = n.children.iter().filter_map(|i| if let AstItem::Node(k) = i { Some(k) } else { None }).collect();
            if inner.iter().any(|k| !matches!(k.type_word.as_str(), "repeat" | "when" | "transition" | "theme")) {
                self.error("G9017", n.line, "a `layout` block holds the rows (`repeat`) — statics stay on the List body, above or below it");
            }
            if !inner.iter().any(|k| k.type_word == "repeat") {
                self.error("G9017", n.line, "an empty `layout` block seats nothing — the rows' `repeat` goes inside it");
            }
        }
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
        if n.props.iter().any(|p| p.key == "overlay" && matches!(p.value, AstValue::Bool(true))) && n.props.iter().any(|p| p.key == "visibility" && p.value.as_ident().is_some_and(|w| w.eq_ignore_ascii_case("space_only"))) {
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
        let mut tmp = TmpNode { node, props: Vec::new(), handlers: Vec::new(), whens: Vec::new(), transitions: Vec::new(), children: Vec::new(), custom_states: Vec::new(), key_claim: 0, wearing: n.styles.iter().cloned().chain(instance.iter().flat_map(|i| i.styles.iter().cloned())).collect(), driver: None, live: None };

        if kind == Kind::Gap {
            self.gap(&mut tmp.node, n, parent_kind);
            if !n.props.is_empty() {
                self.error("G0045", n.line, "`gap` takes one size word and nothing else");
            }
            // A gap IS the space (GPAR97): a child in it would never be
            // laid out.
            if !n.children.is_empty() {
                self.error("G0045", n.line, "a `gap` IS the space — it holds nothing; put the content beside it");
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
        // The kind's theme-entry style is worn FIRST — the weakest
        // writer (GP40 step 3): every later style and prop out-ranks it
        // by the later-wins read. The entry line's own prop keys are
        // filtered out of it (the line out-ranks its style).
        let wear_word = instance.map_or(n.type_word.as_str(), |i| i.type_word.as_str());
        let entry_line_keys: Vec<String> = self.entry_styles.get(wear_word).map(|e| e.1.clone()).unwrap_or_default();
        let styles: Vec<(String, bool, bool)> = self.entry_styles.get(wear_word).map(|e| e.0.clone()).unwrap_or_default().into_iter().map(|s| (s, instance.is_some(), true)).chain(n.styles.iter().map(|s| (s.clone(), false, false))).chain(instance.iter().flat_map(|i| i.styles.iter().map(|s| (s.clone(), true, false)))).collect();
        for (s, from_instance, from_entry) in &styles {
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
                        // An instance-worn style is worn by the INSTANCE
                        // (`Fader f .Thin`) — the body root underneath
                        // keeps its own kind word.
                        let worn_on = if *from_instance { instance.map_or(n.type_word.as_str(), |i| i.type_word.as_str()) } else { n.type_word.as_str() };
                        if target != worn_on {
                            self.error("G3006", n.line, format!("style `{s}` is for {target} — applied here to {worn_on}"));
                        }
                    }
                    if !def.slot_props.is_empty() {
                        if let Some(w) = self.slot_writes.last_mut() {
                            for (slot, p) in &def.slot_props {
                                w.entry(slot.clone()).or_default().push(p.clone());
                            }
                        }
                    }
                    // GP40 step 2: a PLAIN style's contract states check
                    // where the style is APPLIED — only here is the
                    // wearer's contract known (`for=` styles checked at
                    // their declaration; a plain box has no contract, so
                    // its wear stays unchecked). A declared custom state
                    // also feeds the D2 advisory.
                    if def.for_target.is_none() && !def.whens.is_empty() {
                        let wearer: Option<(String, Option<(Vec<String>, Vec<String>)>)> = if *from_instance {
                            self.widget_stack.last().and_then(|w| self.widgets.get(w)).filter(|d| d.control).map(|d| (self.widget_stack.last().cloned().unwrap_or_default(), Some((d.states.clone(), d.state_builtins.clone()))))
                        } else if kind.is_press_surface() || matches!(kind, Kind::Slider | Kind::Input) {
                            Some((n.type_word.clone(), None))
                        } else {
                            None
                        };
                        if let Some((type_word, contract)) = wearer {
                            for w in &def.whens {
                                let words: Vec<String> = when_words(w).map(str::to_string).collect();
                                for word in words {
                                    let view_word = word.starts_with("screen.") || self.page.arrangements.iter().any(|a| self.page.str(*a) == word);
                                    let control_word = CONTROL_STATE_WORDS.contains(&word.as_str());
                                    match &contract {
                                        Some((states, builtins)) => {
                                            let declared = states.iter().any(|x| x == &word) || builtins.iter().any(|x| x == &word);
                                            if (control_word || !view_word) && !declared {
                                                let mut avail: Vec<String> = builtins.iter().chain(states.iter()).map(|x| format!("`{x}`")).collect();
                                                if avail.is_empty() {
                                                    avail.push("none".to_string());
                                                }
                                                self.error("G0036", w.line, format!("'{word}' isn't available on `{type_word}` — its contract raises {} (worn via style `{s}`)", avail.join(", ")));
                                            } else if declared && !control_word {
                                                self.d2_lands.push((s.clone(), word.clone(), type_word.clone(), def.line));
                                            }
                                        }
                                        None => {
                                            if !control_word && !view_word {
                                                self.error("G0036", w.line, format!("'{word}' isn't available on `{type_word}` — a built-in raises `held`, `hovered`, `focused`, `focus_within`, `selected`, `disabled` (worn via style `{s}`)"));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if *from_entry && !entry_line_keys.is_empty() {
                        all_props.extend(def.props.iter().filter(|p| !entry_line_keys.contains(&p.key)).cloned());
                    } else {
                        all_props.extend(def.props.iter().cloned());
                    }
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
        // `slot.<name>.<prop>=` written on a line (a control instance,
        // or a built-in with parts): checked against the contract
        // standing here, then delivered to the slot node / the node word
        // its drawing reads — the same check a `for=` style gets at its
        // declaration.
        if all_props.iter().any(|p| p.key.starts_with("slot.")) {
            let shown = instance.map_or(n.type_word.as_str(), |i| i.type_word.as_str()).to_string();
            let target: Option<SlotTarget> = if builtin_slots(kind).is_some() {
                Some(SlotTarget::Builtin(kind, n.type_word.clone()))
            } else if instance.is_some() {
                match self.widget_stack.last().and_then(|w| self.widgets.get(w)) {
                    Some(d) if d.control => Some(SlotTarget::Control(shown.clone(), slot_contract(&d.body, &self.widgets))),
                    _ => None,
                }
            } else {
                None
            };
            let mut kept: Vec<AstProp> = Vec::new();
            for p in std::mem::take(&mut all_props) {
                if !p.key.starts_with("slot.") {
                    kept.push(p);
                    continue;
                }
                match &target {
                    None => self.error("G3002", p.line, format!("`{}`: a {shown} has no slots to restyle — slot paths ride a control's contract (a `for=` style, or a control instance line)", p.key)),
                    Some(t) => {
                        if let Some((slot, ap)) = self.slot_path(&p, t, false) {
                            if slot.is_empty() {
                                kept.push(ap);
                            } else if let Some(w) = self.slot_writes.last_mut() {
                                w.entry(slot).or_default().push(ap);
                            }
                        }
                    }
                }
            }
            all_props = kept;
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
            // The writes routed to this slot land now — after the slot
            // line's own words, so the wearer's write wins the
            // later-wins read (body default < style < instance line,
            // approved 2026-09-02).
            if let Some(name) = &n.name {
                if let Some(w) = self.slot_writes.last_mut() {
                    if let Some(list) = w.remove(name) {
                        all_props.extend(list);
                    }
                }
            }
            tmp.node.auto = self.automatic(n, parent_kind);
            // A position slot rides its content's size: `width=fill`
            // content in an unsized slot has nothing to fill and would
            // vanish silently (the addon's user-caught thumb). The slot
            // line sizing itself keeps the form legal.
            if matches!(tmp.node.auto, Auto::Position(_)) && !all_props.iter().any(|p| p.key == "width") {
                let fill_child = n.children.iter().any(|c| matches!(c, AstItem::Node(k) if k.props.iter().any(|p| p.key == "width" && matches!(&p.value, AstValue::Ident { word, .. } if word.eq_ignore_ascii_case("fill")))));
                if fill_child {
                    self.error("G5106", n.line, "an `automatic=position` slot takes its width from its content — `width=fill` there has nothing to fill; size the slot line (`slot thumb width=20`) or the content");
                }
            }
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
        if signals {
            // Auto-wiring (the addon's On<Instance><Event> contract): with
            // no `on:` written, the surface's events report under the
            // INSTANCE name — the name the file gave the widget IS the
            // handler; an explicit `on:` on the instance line wins.
            let inst = self.prefix.join("/");
            if !inst.is_empty() {
                let action = Action::Named(self.intern(&inst));
                for ev in ["activate", "press", "release", "change", "edit"] {
                    let id = self.intern(ev);
                    if !tmp.handlers.iter().any(|h| h.event == id) {
                        tmp.handlers.push(OnHandler { event: id, action });
                    }
                }
            }
        }
        tmp.node.placement = self.placement(&placement_props, n.line, parent_kind, kind);
        tmp.driver = self.pending_driver.take();
        tmp.live = self.pending_live.take();
        self.container_child_guard(n, &tmp.node.placement, parent_kind, kind);
        if kind == Kind::List {
            // The List self-scrolls on its layout's axis (D121/D128): a
            // `layout Row` strip pans sideways, everything else scrolls
            // down. Viewport doctrine (D121): an unplaced List EXPANDS on
            // that axis to the room its parent offers; a hug there (D123)
            // is the author's opt-out — the box is the content — and the
            // scroll axis' `max_*` cap (D124) bounds only that hug (on any
            // other size the cap would fight the viewport: refused whole).
            let horizontal = n.children.iter().any(|c| matches!(c, AstItem::Node(k) if k.type_word == "layout" && k.name.as_deref() == Some("Row")));
            let p = &mut tmp.node.placement;
            if horizontal {
                if p.width == Len::Unset {
                    p.width = Len::Fill(1.0);
                }
                if (p.max_width.is_some() || p.max_width_fill) && !matches!(p.width, Len::Hug(_)) {
                    self.error("G1018", n.line, "a row-layout List's `max_width=` caps `width=hug` — write `width=hug` (the strip grows with its rows to the cap, then scrolls)");
                }
                if p.max_height.is_some() || p.max_height_fill {
                    self.error("G9017", n.line, "a Row strip runs SIDEWAYS — it caps with `max_width=`, not `max_height=`");
                }
            } else {
                if p.height == Len::Unset {
                    p.height = Len::Fill(1.0);
                }
                if (p.max_height.is_some() || p.max_height_fill) && !matches!(p.height, Len::Hug(_)) {
                    self.error("G1018", n.line, "a List's `max_height=` caps `height=hug` — write `height=hug` (the box grows with its rows to the cap, then scrolls)");
                }
            }
        }
        // `aspect=` is a relation: ONE spelled axis drives, the other is
        // derived; both `fill` = the largest ratio box that fits (GPAR80,
        // the addon's _aspect_fit rules). Fenced here — the checks need
        // the compiled placement AND the prop value, and `aspect` is not a
        // placement key (layout reads it from the props).
        if let Some(av) = tmp.props.iter().rev().find(|pr| pr.key == PropKey::Aspect).map(|pr| pr.value) {
            let pl = tmp.node.placement;
            let natural = matches!(av, Value::VIdent(w) if self.page.str(w) == "natural");
            match av {
                Value::VNum(x) if x <= 0.0 => {
                    self.error("G1017", n.line, "`aspect=` wants a positive ratio (width/height)");
                }
                Value::VNum(_) | Value::VRef { .. } => {}
                Value::VIdent(_) if natural && kind == Kind::Image => {}
                Value::VIdent(_) if natural => {
                    self.error("G1017", n.line, "`aspect=natural` reads a bitmap's own ratio — only an Image has one");
                }
                _ => {
                    self.error("G1017", n.line, "`aspect=` wants a positive ratio (width/height) or, on an Image, `natural`");
                }
            }
            if !natural && !matches!(av, Value::VRef { .. }) {
                if matches!(pl.width, Len::Hug(_)) || matches!(pl.height, Len::Hug(_)) {
                    self.error("G1017", n.line, "`aspect=` with a hug axis: hug is content-sized, nothing is left to derive");
                } else if matches!(pl.width, Len::Exact(_)) && matches!(pl.height, Len::Exact(_)) {
                    self.error("G1017", n.line, "`aspect=` with both sizes spelled: the ratio IS the second size — drop one");
                }
                if pl.max_width.is_some() || pl.max_height.is_some() || pl.max_width_fill || pl.max_height_fill {
                    self.error("G1017", n.line, "a `max_*` cap cannot combine with `aspect=` — the clamp would break the ratio");
                }
                if parent_kind.map(|k| k.is_stack() || matches!(k, Kind::Flow | Kind::Grid | Kind::GridRow | Kind::Tiles)).unwrap_or(false) {
                    self.error("G1017", n.line, "`aspect=` needs a free parent (Layers, View, Panel) — a container seats its children and owns both axes");
                }
            }
        }
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
        for w in &style_whens {
            if let Some(mut tw) = self.when(w, kind) {
                tw.styled = true;
                tmp.whens.push(tw);
            }
        }
        for w in own_whens {
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

    /// GP52: bakes a contract's `behavior slider by $value in
    /// $min..$max, step=$step, key_unit=5%` for the instance being
    /// stamped (the param scope is live, like a slot's `automatic=`).
    /// The root and its `slot track` resolve to arena ids at the end of
    /// the build, when the subtree exists (`pending_behaviors`).
    fn slider_behavior(&mut self, b: &AstNode, drag_started: StrId, drag_ended: StrId) {
        let by = b.clauses.iter().find(|(k, _)| k == "by").map(|(_, v)| v.clone());
        let range = b.clauses.iter().find(|(k, _)| k == "in").map(|(_, v)| v.clone());
        let Some(by) = by else {
            self.error("G0082", b.line, "`behavior slider` spells its wiring — `by $value in $min..$max`");
            return;
        };
        if !matches!(&by, AstValue::Ref { name, .. } if name == "value") {
            self.error("G0082", b.line, format!("`behavior slider` drives `$value` — `by $value` (got `{}`)", spell(&by)));
            return;
        }
        let Some(AstValue::Range(lo, hi)) = range else {
            self.error("G0082", b.line, "`behavior slider` needs its range — `in $min..$max`");
            return;
        };
        if !matches!((&*lo, &*hi), (AstValue::Ref { name: l, .. }, AstValue::Ref { name: h, .. }) if l == "min" && h == "max") {
            self.error("G0082", b.line, format!("the range is spelled `in $min..$max` (got `in {}..{}`)", spell(&lo), spell(&hi)));
            return;
        }
        let num = |c: &mut Self, v: &AstValue| -> f32 {
            match c.value(v, b.line, None) {
                Value::VNum(x) => x,
                _ => 0.0,
            }
        };
        let rest = num(self, &by);
        let min = num(self, &lo);
        let max = num(self, &hi);
        let mut step = 0.0f32;
        let mut key_unit = 0.0f32;
        for p in &b.props {
            match p.key.as_str() {
                "step" => {
                    step = num(self, &p.value);
                    if step < 0.0 {
                        self.error("G0082", p.line, "`step=` is a positive amount (0 = continuous)");
                        step = 0.0;
                    }
                }
                // `key_unit=5%` = per key/wheel tick, of the RANGE; a
                // bare number is value units. Continuous sliders only —
                // a stepped instance steps by `step=` (the stock law).
                "key_unit" => {
                    key_unit = match &p.value {
                        AstValue::Num { value, unit } if unit == "%" => *value / 100.0 * (max - min),
                        other => num(self, other),
                    };
                    if key_unit <= 0.0 {
                        self.error("G0082", p.line, "`key_unit=` is a positive amount per key/wheel tick — value units, or `%` of the range (`key_unit=5%`)");
                        key_unit = 0.0;
                    }
                }
                other => self.error("G0082", p.line, format!("`{other}=` isn't a `behavior slider` word — the words are `step=`, `key_unit=`")),
            }
        }
        let path = self.prefix.join("/");
        self.pending_behaviors.push(PendingBehavior { path, min, max, step, key_unit, rest, drag_started, drag_ended, line: b.line });
    }

    /// `automatic=<kind> by $value in $min..$max` on a slot line (D134,
    /// spec 4.5/4.6): everything is spelled -- the kind (config in parens
    /// for the radial kinds `arc`/`angle`), the driver after `by`, the
    /// range after `in`. The fraction baked here is the REST start; a code
    /// write of the control's value re-derives it live and glides it by
    /// the control's default feel (`Page::drivers`, GP33).
    fn automatic(&mut self, n: &AstNode, parent_kind: Option<Kind>) -> Auto {
        let by = n.clauses.iter().find(|(k, _)| k == "by").map(|(_, v)| v.clone());
        let range = n.clauses.iter().find(|(k, _)| k == "in").map(|(_, v)| v.clone());
        let Some(kind_prop) = n.props.iter().find(|p| p.key == "automatic") else {
            if by.is_some() || range.is_some() {
                self.error("G5107", n.line, "`by`/`in` couple a slot to an `automatic=` driver — this slot declares none");
            }
            return Auto::NoAuto;
        };
        // Only a `control` has a value to drive the geometry (the shipped
        // needs-a-control fence): in a plain widget the coupling would
        // read as working and drive nothing.
        if let Some(w) = self.widget_stack.last() {
            if self.widgets.get(w).map_or(false, |d| !d.control) {
                self.error("G5107", kind_prop.line, "`automatic=` is automatic geometry — it needs a `control` (a value to drive it); widgets have no value");
                return Auto::NoAuto;
            }
        }
        // The kind word, with the radial kinds' parenthesized config.
        let (kind, cfg): (String, Vec<(String, AstValue)>) = match &kind_prop.value {
            AstValue::Ident { word, .. } => (word.clone(), Vec::new()),
            AstValue::Call { name, args } => {
                let mut cfg = Vec::new();
                let mut positional = false;
                for a in args {
                    match a {
                        AstValue::Named { name: an, value } => cfg.push((an.clone(), (**value).clone())),
                        _ => positional = true,
                    }
                }
                if positional {
                    if name == "presence" {
                        self.error("G5107", kind_prop.line, "presence takes no config — `by` picks the driver (`automatic=presence by selected`)");
                    } else {
                        self.error("G5107", kind_prop.line, "config arguments are named — they say what they set (`automatic=arc(from=-225, sweep=270) by $value in $min..$max`)");
                    }
                    return Auto::NoAuto;
                }
                (name.clone(), cfg)
            }
            other => {
                self.error("G5107", kind_prop.line, format!("`automatic=` names a kind — got `{}`", spell(other)));
                return Auto::NoAuto;
            }
        };
        if !matches!(kind.as_str(), "length" | "position" | "presence" | "arc" | "angle") {
            self.error("G5107", n.line, format!("unknown automatic kind '{kind}' — the set is closed: length, position, presence, arc, angle"));
            return Auto::NoAuto;
        }
        let is_radial = kind == "arc" || kind == "angle";
        if !is_radial && !cfg.is_empty() {
            self.error("G5107", n.line, format!("only the radial kinds take config — `{kind}` has none (`automatic=length by $value in $min..$max`)"));
            return Auto::NoAuto;
        }
        // The aperture (spec 4.6): REQUIRED on a radial — nothing defaults
        // invisibly (D134); declared once, inside the kind\'s parens.
        let mut aperture = [0.0f32; 2];
        if is_radial {
            for (key, _) in &cfg {
                if key != "from" && key != "sweep" {
                    self.error("G5107", n.line, format!("`{kind}` config is `from=`/`sweep=` — not '{key}' (`automatic={kind}(from=-225, sweep=270) by $value in $min..$max`)"));
                    return Auto::NoAuto;
                }
            }
            for (i, key) in ["from", "sweep"].iter().enumerate() {
                match cfg.iter().find(|(k, _)| k == key) {
                    None => {
                        self.error("G5107", n.line, format!("`{kind}` spells its aperture — `{key}=` is missing (`automatic={kind}(from=-225, sweep=270) by $value in $min..$max`)"));
                        return Auto::NoAuto;
                    }
                    Some((_, AstValue::Num { value, unit })) if unit.is_empty() => aperture[i] = *value,
                    Some(_) => {
                        self.error("G5107", n.line, format!("`{key}=` takes degrees (a bare number; Godot's screen convention: 0° = right, clockwise positive)"));
                        return Auto::NoAuto;
                    }
                }
            }
        }
        let num = |c: &mut Self, v: &AstValue| -> f32 {
            match c.value(v, n.line, None) {
                Value::VNum(x) => x,
                _ => 0.0,
            }
        };
        // The coupling is fully spelled and type-checked (D134): v1 drives
        // from `$value` over `$min..$max`, a presence driver is a state word.
        match kind.as_str() {
            "length" | "position" | "arc" | "angle" => {
                if kind == "length" || kind == "position" {
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
                if let Some(root) = self.control_root_path() {
                    self.pending_driver = Some((root, lo, hi, String::new()));
                }
                match kind.as_str() {
                    "length" => Auto::Length(f),
                    "position" => Auto::Position(f),
                    "arc" => Auto::Arc { from: aperture[0], sweep: aperture[1], frac: f },
                    _ => Auto::Angle { from: aperture[0], sweep: aperture[1], frac: f },
                }
            }
            "presence" => {
                if range.is_some() {
                    self.error("G5107", n.line, "`automatic=presence`: a bool has no range — drop the `in` clause");
                }
                match &by {
                    Some(AstValue::Ident { word, .. }) => {
                        if let Some(root) = self.control_root_path() {
                            self.pending_driver = Some((root, 0.0, 0.0, word.clone()));
                        }
                    }
                    Some(other) => self.error("G5107", n.line, format!("a presence driver is a state word — `by` names one (no `$`): `by selected` (got `{}`)", spell(other))),
                    None => self.error("G5107", n.line, "`automatic=presence` wants `by <state>` (`by selected`)"),
                }
                Auto::Presence(false)
            }
            _ => unreachable!(),
        }
    }

    /// The innermost enclosing `control` instance's path ("a/b") — the
    /// node whose `$value` and states drive this scope's couplings.
    fn control_root_path(&self) -> Option<String> {
        for k in (1..=self.prefix.len()).rev() {
            let p = self.prefix[..k].join("/");
            if self.control_scopes.contains(&p) {
                return Some(p);
            }
        }
        None
    }

    /// The shipped container-child fences (GP34, resolver
    /// _validate_layout_parents + the D139/D144 cap rules): under a parent
    /// that seats its children, margins are provably dead, an overlay
    /// can't refuse its seat, and a size cap works only where the box
    /// reports its own minimum (a Layers) or scrolls past it.
    fn container_child_guard(&mut self, n: &AstNode, p: &Placement, parent_kind: Option<Kind>, kind: Kind) {
        let container = matches!(parent_kind, Some(k) if matches!(k, Kind::Column | Kind::Row | Kind::Flow | Kind::Grid | Kind::GridRow | Kind::Tiles | Kind::List | Kind::Scroll));
        if !container || kind == Kind::Gap {
            return;
        }
        let pname = parent_name(parent_kind);
        // Margins under a seating parent never move anything — the seat
        // is the parent's to hand out (uniform rhythm is `spacing=`, one
        // divergent gap is a `gap N` line).
        if let Some(mp) = n.props.iter().find(|q| matches!(q.key.as_str(), "margin" | "margin_left" | "margin_top" | "margin_right" | "margin_bottom")) {
            self.error("G1002", mp.line, format!("a {pname} lays out its children — margins only apply under a plain box (Layers, Panel, View); uniform rhythm is the parent's `spacing=`, a single divergent gap is a `gap N` line at the break"));
        }
        // D139: an overlay can't refuse a seat it was dealt — `Column {{
        // Layers {{ content; badge overlay=true }} }}` already spells the
        // flowing case.
        if n.props.iter().any(|q| q.key == "overlay" && matches!(q.value, AstValue::Bool(true))) {
            self.error("G1002", n.line, format!("a {pname} seats its children — an overlay can't refuse a seat it was dealt; overlay= needs a free parent: wrap the flowed content in `Layers {{ … }}` and declare the overlay beside it inside"));
        }
        // The cap law (decided with the user 2026-09-01, REPLACING the
        // shipped D144 Layers special case): a box never reports more
        // than its cap — ANY box, under any parent — and `max_*=fill`
        // caps at the parent's inner space. The measure already clamps
        // (`size_at`), so no wrapper ceremony is needed. What's left
        // refused is the one shape with nothing separate to bind
        // against: on a stack's OWN stacking axis, the slot IS the
        // frame, so a cap that reads the parent's space (`max_*=fill`,
        // or any cap on a `fill` slot) is circular — cap the stack
        // itself. A Scroll scrolls past its cap and a List hugs then
        // scrolls (their own doctrines).
        if parent_kind == Some(Kind::Scroll) || matches!(kind, Kind::Scroll | Kind::List) {
            return;
        }
        let main: Option<usize> = match parent_kind {
            Some(Kind::Column) | Some(Kind::List) => Some(1),
            Some(Kind::Row) => Some(0),
            _ => None,
        };
        for (axis, axis_word, len, cap, cap_fill) in [(0, "width", p.width, p.max_width, p.max_width_fill), (1, "height", p.height, p.max_height, p.max_height_fill)] {
            if cap.is_none() && !cap_fill {
                continue;
            }
            if Some(axis) == main && (matches!(len, Len::Fill(_)) || cap_fill) {
                self.error("G1002", n.line, format!("`max_{axis_word}=` on a {pname}'s stacking axis can't read the parent's space — the slot IS the frame there; a number caps a hugging box (`{axis_word}=hug, max_{axis_word}=60`), or cap the {pname} itself"));
            } else if parent_kind == Some(Kind::Flow) && cap_fill {
                // A Flow line is packed from measures, not a frame the
                // child can read back (v1) — a number still caps.
                self.error("G1002", n.line, format!("`max_{axis_word}=fill` has no frame to read inside a Flow (v1) — cap at a number, or move the box under a free parent"));
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
                            let mut child = self.outer(|me| me.node(k, Some(parent_kind), None));
                            // Content of the bare default `slot` is
                            // EXEMPT from the free-fill default (the
                            // spec's D71 exemption, GPAR97): a sizeless
                            // box there keeps its content size.
                            if child.node.placement.width == Len::Unset {
                                child.node.placement.width = Len::Hug(0.0);
                            }
                            if child.node.placement.height == Len::Unset {
                                child.node.placement.height = Len::Hug(0.0);
                            }
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
                // The fill replaces the slot's DEFAULT content only: a
                // nested `slot` is the widget's own machinery (the thumb
                // inside a track) and survives, seated after the fill
                // (the addon's slotn.fill-below-nested shape).
                let kept: Vec<AstItem> = region.children.drain(..).filter(|i| matches!(i, AstItem::Node(k) if k.type_word == "slot")).collect();
                let mut tmp = self.node(&region, Some(parent_kind), None);
                for item in &items {
                    if let AstItem::Node(k) = item {
                        let child = self.outer(|me| me.node(k, Some(Kind::Slot), None));
                        tmp.children.push(child);
                    }
                }
                for item in &kept {
                    if let AstItem::Node(k) = item {
                        self.slot_child(k, Kind::Slot, &mut tmp.children);
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
        let id = self.push_slot(root.node, root.props, root.handlers, root.whens, root.transitions, root.custom_states, root.key_claim, root.wearing, root.driver, root.live, NO_NODE);
        self.place_children(id, root.children);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_slot(&mut self, mut node: Node, props: Vec<Prop>, handlers: Vec<OnHandler>, whens: Vec<WhenTmp>, transitions: Vec<Transition>, custom_states: Vec<StrId>, key_claim: u8, wearing: Vec<String>, driver: Option<(String, f32, f32, String)>, live: Option<(String, f32)>, parent: NodeId) -> NodeId {
        // Keyboard reach bakes here (GP26): the kinds that answer a tap
        // answer the keyboard — plus a control root whose contract said
        // so (above); `keyboard_target=off` is the deliberate opt-out
        // (the addon's focus_mode=none + lint pair).
        node.focusable = (node.focusable || matches!(node.kind, Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio | Kind::ListItem | Kind::Slider | Kind::Input))
            && !props.iter().any(|p| p.key == PropKey::KeyboardTarget && matches!(p.value, Value::VIdent(w) if self.page.str(w) == "off"));
        let id = NodeId::from_index(self.page.nodes.len());
        if let Some((root, min, max, state)) = driver {
            self.pending_drivers.push((id, root, min, max, state));
        }
        if let Some((root, step)) = live {
            self.pending_lives.push((id, root, step));
        }
        for cs in custom_states {
            self.page.custom_states.push((id, cs));
        }
        if key_claim != 0 {
            self.page.key_claims.push((id, key_claim));
        }
        for s in wearing {
            self.style_wearers.entry(s).or_default().push(id);
        }
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
                while a != NO_NODE && !self.page.nodes[a].kind.is_press_surface() {
                    a = self.page.nodes[a].parent;
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
            let cid = self.push_slot(child.node, child.props, child.handlers, child.whens, child.transitions, child.custom_states, child.key_claim, child.wearing, child.driver, child.live, id);
            slots.push((cid, child.children));
        }
        self.page.nodes[id].children = (first, count);
        for (cid, rest) in slots {
            self.place_children(cid, rest);
        }
        self.page.nodes[id].subtree_end = self.page.nodes.len() as u32;
    }

    fn gap(&mut self, node: &mut Node, n: &AstNode, parent_kind: Option<Kind>) {
        if !parent_kind.map(Kind::is_stack).unwrap_or(false) {
            self.error("G0047", n.line, "`gap` lives in a Column/Row (Flow → spacing=, Grid → tracks, Layers → margins)");
        }
        match n.args.first() {
            Some(AstValue::Num { value, .. }) => node.gap = *value,
            Some(AstValue::Ident { word, .. }) if word.eq_ignore_ascii_case("fill") => {
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
            "activate" if !kind.is_press_surface() && kind != Kind::List => {
                self.error("G0072", line, format!("`on:{event}` fires only on a press surface (Button, Check, Toggle, Radio, ListItem) or the List that owns them — a {kind:?} never emits it"));
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
                // One argument per action (GPAR93): the extras would be
                // dropped silently — `emit` carries the name alone, a
                // flip goes to one arrangement.
                if matches!(name.as_str(), "go" | "toggle" | "play" | "emit") && args.len() > 1 {
                    self.error("G0050", p.line, format!("`{name}(…)` takes one argument, got {} — the extras would carry nothing", args.len()));
                    return None;
                }
                match name.as_str() {
                    "go" | "toggle" => {
                        let target = if arg == "base" {
                            if name == "toggle" {
                                self.error("G0050", p.line, "`toggle(base)` — base is home, use `go(base)`");
                            }
                            BASE
                        } else {
                            let id = self.intern(arg);
                            match self.page.arrangements.iter().position(|a| *a == id) {
                                Some(i) => ArrangementId::from_index(i),
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
        let oname = self.page.str(self.page.nodes[occupant].name).to_string();
        let Value::VIdent(vid) = prop.value else {
            self.error("G1016", line, format!("`{oname}.seat` names a declared seat — `{oname}.seat = sideSeat`"));
            return None;
        };
        if self.page.nodes[occupant].kind == Kind::Seat {
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
        let target = self.page.find(&vname).filter(|s| self.page.nodes[*s].kind == Kind::Seat);
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
            p = self.page.nodes[p].parent;
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
            if p.key == "margin" || p.key.ends_with(".margin") {
                // GPAR94 (the addon's pose rule): a pose moves one edge
                // at a time; `margin=` re-declares all four.
                self.error("G1014", p.line, "a pose moves one edge at a time — write `margin_left=`/`margin_top=`/… (`margin=` re-declares all four edges)");
                continue;
            }
            if p.key.starts_with("slot.") {
                // A built-in's path lands on the node word its drawing
                // reads; anything else has no contract here (a control
                // instance's paths were rewritten before this runs).
                if builtin_slots(kind).is_some() {
                    if let Some((_, ap)) = self.slot_path(p, &SlotTarget::Builtin(kind, format!("{kind:?}")), true) {
                        if let Some(prop) = {
                            self.in_when += 1;
                            let r = self.prop(&ap, kind);
                            self.in_when -= 1;
                            r
                        } {
                            props.push(prop);
                        }
                    }
                } else {
                    self.error("G3002", p.line, format!("`{}`: slot paths ride a control's contract — nothing here declares slots", p.key));
                }
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
        // Placement answers the VIEW's conditions — `when <arrangement>`,
        // `when screen.*` (GP1's poses). An input or control state
        // styles LOOKS: a held box re-anchoring mid-press would fight
        // the finger (GPAR97, the addon's control-scope refusal).
        if matches!(state, State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled | State::Custom(_)) {
            for pr in &props {
                if pr.key.is_placement() {
                    self.error("G1014", w.line, format!("`{:?}` under `when {}` — placement answers the view's conditions (`when <arrangement>`, `when screen.*`); a state styles looks (background, opacity, translate…)", pr.key, state_word(&self.page, state)));
                    break;
                }
            }
        }
        Some(WhenTmp { state, line: w.line, props, descendants, host: false, styled: false })
    }

    // ----- placement ------------------------------------------------------

    fn len(&mut self, v: Value, line: u32, key: &str) -> Len {
        match v {
            Value::VNum(n) => Len::Exact(n),
            Value::VIdent(id) => match self.page.str(id).to_ascii_lowercase().as_str() {
                // Size words fold case like every enum word (`width=FILL`);
                // width/height skip the closed-words fence, so the fold
                // lives here.
                "fill" => Len::Fill(1.0),
                // Weight 0 = plain hug (content-sized, no slot share);
                // `hug(n)` is the weighted SLOT around a hugged box (GP34).
                "hug" => Len::Hug(0.0),
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
            // A numeric theme `$value` in a length (`min_width=$rowFont`,
            // GPAR88): placement bakes at compile and theme entries are
            // compile-known, so the ref resolves here. Only default
            // entries — a variant block cannot flip a baked length.
            Value::VRef { .. } => match crate::runtime::deref(&self.page, NO_STR, v) {
                Value::VNum(n) => n,
                _ => {
                    self.error("G0058", line, format!("`{key}=` wants a number (epx) — the `$value` it names is not a number"));
                    0.0
                }
            },
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
            if self.page.str(id).eq_ignore_ascii_case("safe") {
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
                    Value::VIdent(id) if self.page.str(id).eq_ignore_ascii_case("fill") => p.max_width_fill = true,
                    _ => p.max_width = Some(self.num(v, line, "max_width")),
                },
                PropKey::MaxHeight => match v {
                    Value::VIdent(id) if self.page.str(id).eq_ignore_ascii_case("fill") => p.max_height_fill = true,
                    _ => p.max_height = Some(self.num(v, line, "max_height")),
                },
                PropKey::Halign => p.halign = self.align(v, line, "halign"),
                PropKey::Valign => p.valign = self.align(v, line, "valign"),
                PropKey::MarginLeft => self.edge(v, line, "margin_left", &mut p, 0, true),
                PropKey::MarginTop => self.edge(v, line, "margin_top", &mut p, 1, true),
                PropKey::MarginRight => self.edge(v, line, "margin_right", &mut p, 2, true),
                PropKey::MarginBottom => self.edge(v, line, "margin_bottom", &mut p, 3, true),
                PropKey::Margin => match v {
                    Value::VIdent(id) if self.page.str(id).eq_ignore_ascii_case("safe") => {
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
                    Value::VIdent(id) if self.page.str(id).eq_ignore_ascii_case("safe") => p.padding_safe = [true; 4],
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
        // GP40 step 2: `for=` style states against their contract (the
        // wear-time twin for plain styles lives in the styles loop) —
        // run first so a bad style errors even when nothing wears it.
        for (sname, target, word, line) in std::mem::take(&mut self.pending_style_states) {
            let view_word = word.starts_with("screen.") || self.page.arrangements.iter().any(|a| self.page.str(*a) == word);
            let control_word = CONTROL_STATE_WORDS.contains(&word.as_str());
            match self.widgets.get(&target) {
                Some(def) => {
                    let declared = def.states.iter().any(|w| w == &word) || def.state_builtins.iter().any(|w| w == &word);
                    if (control_word || !view_word) && !declared {
                        let mut avail: Vec<String> = def.state_builtins.iter().chain(def.states.iter()).map(|w| format!("`{w}`")).collect();
                        if avail.is_empty() {
                            avail.push("none".to_string());
                        }
                        self.error("G0036", line, format!("'{word}' isn't available on `{target}` — its contract raises {} (style `{sname}`)", avail.join(", ")));
                    }
                }
                // A built-in raises the universal states natively.
                None => {
                    if !control_word && !view_word {
                        self.error("G0036", line, format!("'{word}' isn't available on `{target}` — a built-in raises `held`, `hovered`, `focused`, `focus_within`, `selected`, `disabled` (style `{sname}`)"));
                    }
                }
            }
        }
        // The D2 advisory (customization spec): a plain style whose
        // custom state lands on exactly ONE control type would read
        // better `for=` that type — two distinct types is deliberately
        // generic and the advisory stands down.
        let mut lands: Vec<(String, String, Vec<String>, u32)> = Vec::new();
        for (s, word, t, line) in std::mem::take(&mut self.d2_lands) {
            match lands.iter_mut().find(|(ls, lw, ..)| *ls == s && *lw == word) {
                Some((_, _, types, _)) => {
                    if !types.contains(&t) {
                        types.push(t);
                    }
                }
                None => lands.push((s, word, vec![t], line)),
            }
        }
        for (s, word, types, line) in lands {
            if let [t] = types.as_slice() {
                self.warning("G0036", line, format!("style `{s}`'s `when {word}` lands only on {t} — `for={t}` checks it at the declaration (customization spec D2)"));
            }
        }

        // Repeats: name -> node id; the non-`empty` child is the hidden
        // template the Runtime clones into row slots (GPAR108/capacity).
        // The template's `$item.<field>` sites are recorded here, once —
        // a reused slot re-binds through them with no marker left on it.
        for (name, binding, by, capacity, keyed) in std::mem::take(&mut self.pending_repeats) {
            if name == NO_STR {
                continue;
            }
            let Some(node) = self.page.nodes.iter().position(|n| n.name == name && n.kind == Kind::Repeat) else { continue };
            let node = NodeId::from_index(node);
            let template = self.page.children(node).find(|c| self.page.nodes[*c].kind != Kind::Empty);
            if let Some(template) = template {
                let first = self.page.bind_sites.len() as u32;
                let t_end = self.page.subtree_end(template);
                for ni in NodeId::range(template, t_end) {
                    let (p0, pc) = self.page.nodes[ni].props;
                    for k in 0..pc {
                        if let Value::VItem(f) = self.page.props[(p0 + k) as usize].value {
                            self.page.bind_sites.push(BindSite { node_off: ni.index() as u32 - template.index() as u32, prop_off: k, field: f });
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
            if self.page.prop(NodeId::from_index(i), PropKey::Interactive) != Some(Value::VBool(true)) {
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
        // Anims: flat tracks with start times. A `play` step inlines
        // another anim's tracks, so a decl compiles ON DEMAND when a play
        // names it before its turn (forward references; the shared stack
        // breaks cycles).
        let decls = std::mem::take(&mut self.anim_decls);
        let mut anim_compiled: Vec<bool> = vec![false; decls.len()];
        let mut anim_stack: Vec<String> = Vec::new();
        for i in 0..decls.len() {
            self.compile_anim_decl(i, &decls, &mut anim_compiled, &mut anim_stack);
        }
        self.bind_self_anims();
        // Top-level `when <arrangement> { node.prop = value }`.
        let deferred = std::mem::take(&mut self.deferred_whens);
        let mut seat_claims: Vec<(NodeId, StrId, State, u32)> = Vec::new();
        let mut screen_writes: Vec<(NodeId, PropKey, State, u32)> = Vec::new();
        for (state, items, block_line, default_pose) in deferred {
            // A widget's default pose ranks under every page line (the
            // runtime applies a node's whens by (priority, line)).
            let line: u32 = if default_pose { 0 } else { block_line };
            for item in &items {
                let AstItem::Assign { key, dollar: false, value, line: l } = item else {
                    self.error("G0054", line, "a top-level `when` block holds `node.prop = value` lines");
                    continue;
                };
                let Some((node_name, prop_name)) = split_target(key) else {
                    self.error("G0059", *l, format!("`{key}` — write `node.prop = value`"));
                    continue;
                };
                if prop_name == "margin" {
                    // GPAR94 (the addon's pose rule): per-edge only.
                    self.error("G1014", *l, "a pose moves one edge at a time — write `margin_left=`/`margin_top=`/… (`margin=` re-declares all four edges)");
                    continue;
                }
                let Some(id) = self.page.find(node_name).or_else(|| self.root_aliases.get(node_name).and_then(|t| self.page.find(t))) else {
                    self.error("G0060", *l, format!("`{key}`: no node named `{node_name}`"));
                    continue;
                };
                let kind = self.page.nodes[id].kind;
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
                    match self.node_whens[id].iter_mut().find(|w| w.state == state && w.line == line) {
                        Some(w) => w.props.push(prop),
                        None => self.node_whens[id].push(WhenTmp { state, line, props: vec![prop], descendants: Vec::new(), host: false, styled: default_pose }),
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
            if self.page.prop(NodeId::from_index(i), PropKey::Wrap) != Some(Value::VBool(true)) {
                continue;
            }
            let mut a = nd.parent;
            let mut sliver = false;
            while a != NO_NODE {
                let ap = self.page.nodes[a].placement;
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
                        a = self.page.nodes[a].parent;
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
                    let nname = self.page.str(self.page.nodes[na].name).to_string();
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
                    let oname = self.page.str(self.page.nodes[oa].name).to_string();
                    self.error("G1016", la, format!("two `when` blocks both set '{oname}.seat' under conditions that can hold together"));
                } else if sa == sb && sa != home_id {
                    let sname = self.page.str(sa).to_string();
                    let bname = self.page.str(self.page.nodes[ob].name).to_string();
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
                    // Widget-body names carry their instance prefix (`inst/name`).
                    let named = |i: &usize| {
                        let full = if self.page.nodes[*i].name != NO_STR { self.page.str(self.page.nodes[*i].name) } else { "" };
                        full == name || full.rsplit('/').next() == Some(name.as_str()) || self.root_aliases.get(&format!("{full}/{name}")).map(String::as_str) == Some(full)
                    };
                    // Own subtree first; then anywhere — the shipped
                    // scope resolves targets through the named map, so a
                    // SIBLING shield is a legal target (GPAR83). The
                    // subtree is asked per node (`is_under`): the arena is
                    // children-first, so `owner+1..subtree_end` also holds
                    // LATER SIBLINGS and their bodies — a scan over that
                    // range gave every later widget instance the FIRST
                    // instance's `box`/`glyph` (the settings page's second
                    // checkbox lit the first one's mark, todo-bugs.md #7).
                    let owner_id: NodeId = NodeId::from_index(owner);
                    let target = (owner + 1..self.page.nodes.len()).find(|i| self.page.is_under(NodeId::from_index(*i), owner_id) && named(i))
                        .or_else(|| (0..self.page.nodes.len()).find(|i| *i != owner && named(i)));
                    match target {
                        // The word check a node line gets, now that the
                        // target's kind is known. Needed so a misspelled
                        // response is a build error, not a silently dead
                        // line (docs/todo-gogo-parity.md GPAR82).
                        Some(t) if !self.check_prop_kind(&word, prop.key, self.page.nodes[t].kind, line) => {}
                        Some(t) => moved.push((t, owner, WhenTmp { state: w.state, line: w.line, props: vec![prop], descendants: Vec::new(), host: w.host, styled: w.styled })),
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
            // A control instance root stops the climb too: it holds its
            // own contract states (`when dragging slot.thumb.…` on a
            // Fader answers the FADER, not some surface above it).
            let is_watchable = |me: &Self, id: NodeId| {
                me.page.nodes[id].kind.is_press_surface()
                    || (me.page.nodes[id].name != NO_STR && me.control_scopes.contains(me.page.str(me.page.nodes[id].name)))
            };
            let mut of_node = NodeId::from_index(owner);
            while of_node != NO_NODE && !is_watchable(self, of_node) && self.page.nodes[of_node].parent != NO_NODE {
                of_node = self.page.nodes[of_node].parent;
            }
            let host = w.host;
            whens[target].push(w);
            of[target].push(if host { HOST_PENDING } else { of_node });
        }
        // GP40 step 4: a live stop write names a DECLARED stop — the
        // count is the `stops=` list's; `stop_3_color` on a two-stop
        // gradient would die silently in the draw fold.
        for (id, list) in whens.iter().enumerate() {
            if self.page.nodes[id].kind != Kind::Gradient {
                continue;
            }
            let count = self.page.prop(NodeId::from_index(id), PropKey::Stops).map_or(0, |v| self.page.list(v).len());
            for w in list {
                for pr in &w.props {
                    let n = match pr.key {
                        PropKey::StopColor(n) | PropKey::StopOffset(n) => n as usize,
                        _ => continue,
                    };
                    if n > count {
                        self.error("G2005", w.line, format!("this gradient declares {count} stops — stop {n} names nothing"));
                    }
                }
            }
        }
        // `for=` widget responses watch the hosting control (the nearest
        // press surface above).
        for i in 0..of.len() {
            for j in 0..of[i].len() {
                if of[i][j] == HOST_PENDING {
                    let mut a = self.page.nodes[i].parent;
                    while a != NO_NODE && !self.page.nodes[a].kind.is_press_surface() {
                        a = self.page.nodes[a].parent;
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
            if !matches!(self.page.prop(NodeId::from_index(i), PropKey::Overlay), Some(Value::VBool(true))) {
                continue;
            }
            let parent = node.parent;
            if parent == NO_NODE {
                continue;
            }
            let pp = &self.page.nodes[parent].placement;
            let siblings_size: [bool; 2] = {
                let p = &self.page.nodes[parent];
                let mut any = [false, false];
                for s in self.page.children(parent) {
                    if s.index() == i || matches!(self.page.prop(s, PropKey::Overlay), Some(Value::VBool(true))) {
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
                    let owner = if of[i][j] == NO_NODE { NodeId::from_index(i) } else { of[i][j] };
                    authored.push((owner, w.state));
                }
            }
        }
        // Two blocks on ONE condition writing one prop, authored in the
        // same place: the loser would silently vanish (last-wins), so
        // the pair is refused (GPAR94). A style's block under a node's
        // own is the normal per-state override and stays legal.
        for (i, list) in whens.iter().enumerate() {
            for a in 0..list.len() {
                for b in a + 1..list.len() {
                    let (wa, wb) = (&list[a], &list[b]);
                    if wa.state != wb.state || wa.styled || wb.styled || wa.host || wb.host || of[i][a] != of[i][b] || wa.line == wb.line {
                        continue;
                    }
                    if let Some(p) = wa.props.iter().find(|p| wb.props.iter().any(|q| q.key == p.key)) {
                        self.error("G0054", wb.line, format!("two `when {}` blocks both set `{:?}` (lines {} and {}) — one writer per condition and property", state_word(&self.page, wa.state), p.key, wa.line, wb.line));
                    }
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
        // A posed margin on a stack child (GPAR94, the addon's pose rule
        // = the node-line G1014): the parent seats its children — the
        // pose could never bind. The addon's OTHER pose refusals (a
        // margin against the declared alignment) are NOT ported: in Rust
        // a margin on an aligned or centered axis is load-bearing — it
        // feeds a hug parent's claim (D241/D242) and shifts a centered
        // box (GPAR38) — so those forms mean something here.
        for i in 0..self.page.nodes.len() {
            let (w0, wc) = self.page.nodes[i].whens;
            if wc == 0 {
                continue;
            }
            let parent = self.page.nodes[i].parent;
            let in_stack = parent != NO_NODE && {
                let k = self.page.nodes[parent].kind;
                k.is_stack() || matches!(k, Kind::Flow | Kind::Grid | Kind::GridRow | Kind::Tiles)
            };
            if !in_stack {
                continue;
            }
            for wi in w0..w0 + wc {
                let w = self.page.whens[wi as usize];
                for pi in w.props.0..w.props.0 + w.props.1 {
                    if matches!(self.page.props[pi as usize].key, PropKey::MarginLeft | PropKey::MarginTop | PropKey::MarginRight | PropKey::MarginBottom) {
                        self.error("G1014", w.line, "a posed margin on a stack child — the parent seats its children; space them with `spacing=`/`gap`");
                    }
                }
            }
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
        // List structure (GP28's fence families, round 6 — the addon's
        // G9010/G9011/G9013/G9016 + the two advisory lints). The List is
        // the one control that owns selection: its body is statics plus
        // exactly ONE `repeat` whose row root is a `ListItem`, and its
        // zero-items view is ONE List-level `empty { }`.
        for i in 0..self.page.nodes.len() {
            let node = &self.page.nodes[i];
            let line = node.line;
            if node.kind == Kind::ListItem {
                let parent = node.parent;
                let ok = parent != NO_NODE && self.page.nodes[parent].kind == Kind::Repeat && {
                    let mut a = self.page.nodes[parent].parent;
                    let mut in_list = false;
                    while a != NO_NODE {
                        if self.page.nodes[a].kind == Kind::List {
                            in_list = true;
                            break;
                        }
                        if !matches!(self.page.nodes[a].kind, Kind::Row | Kind::Flow | Kind::Tiles | Kind::Column) {
                            break;
                        }
                        a = self.page.nodes[a].parent;
                    }
                    in_list
                };
                if !ok {
                    self.error("G9013", line, "`ListItem` is a List row's root — it belongs at the root of the `repeat` inside a `List`; anywhere else, a `Panel` is the box you want");
                }
                continue;
            }
            if node.kind != Kind::List {
                continue;
            }
            let list = NodeId::from_index(i);
            let mut empties = 0u32;
            let mut repeats: Vec<NodeId> = Vec::new();
            for c in self.page.children(list).collect::<Vec<NodeId>>() {
                match self.page.nodes[c].kind {
                    Kind::Empty => empties += 1,
                    Kind::Repeat => repeats.push(c),
                    Kind::Slot => self.error("G9013", self.page.nodes[c].line, "a List has no slots — its rows come from the `repeat`, its statics sit in the body"),
                    // the `layout` panel: rows one level down
                    Kind::Row | Kind::Flow | Kind::Tiles | Kind::Column => {
                        for g in self.page.children(c) {
                            if self.page.nodes[g].kind == Kind::Repeat {
                                repeats.push(g);
                            }
                        }
                    }
                    _ => {}
                }
            }
            match empties {
                0 => self.error("G9016", line, "a List declares its zero-items view: one `empty { }` in the List body (bare is fine — `empty { }`)"),
                1 => {}
                _ => self.error("G9016", line, "one `empty { }` per List — the zero-items view is a single seat"),
            }
            match repeats.len() {
                0 => self.error("G9010", line, "a List holds exactly one `repeat` — none here; statics alone are a `Column`'s job"),
                1 => {}
                _ => self.error("G9010", line, "a List holds exactly one `repeat` — its selection and rows are one roster"),
            }
            for r in &repeats {
                let mut roots = 0u32;
                let mut root: Option<NodeId> = None;
                for c in self.page.children(*r).collect::<Vec<NodeId>>() {
                    match self.page.nodes[c].kind {
                        Kind::Empty => self.error("G9016", self.page.nodes[c].line, "in a List the zero-items view lives at the LIST level — move `empty { }` beside the repeat, not inside it"),
                        _ => {
                            roots += 1;
                            root = Some(c);
                        }
                    }
                }
                if roots > 1 {
                    self.error("G9011", self.page.nodes[*r].line, "a List row has ONE root — wrap the parts in a single `ListItem { … }`");
                } else if let Some(root) = root {
                    if self.page.nodes[root].kind != Kind::ListItem {
                        self.error("G9011", self.page.nodes[root].line, "a List row's root is a `ListItem` — the one tap-and-select surface per row; wrap this in one (`ListItem { … }`)");
                    } else {
                        // G9014 (advisory): the List flips SELECTED on the
                        // ROW root — a `when selected` below it never fires.
                        for d in self.page.subtree_scan(root).collect::<Vec<NodeId>>() {
                            let (w0, wc) = self.page.nodes[d].whens;
                            for w in &self.page.whens[w0 as usize..(w0 + wc) as usize] {
                                if w.state != State::Selected {
                                    continue;
                                }
                                // A control's OWN selected (a Toggle in
                                // the row) and a block watching the row
                                // root both fire — only a passive box's
                                // dead condition warns.
                                let watched = if w.of == NO_NODE { d as NodeId } else { w.of };
                                if watched == root || self.page.nodes[watched].kind.is_press_surface() {
                                    continue;
                                }
                                self.warning("G9014", self.page.nodes[d.index()].line, "`when selected` below the row's root never fires — the List checks the ListItem; style the root (or watch it: the row's own state)");
                                break;
                            }
                        }
                    }
                }
            }
            // G9015 (advisory): a viewport List whose ancestors provably
            // hand it its minimum — near zero — before any sized box.
            let pl = &self.page.nodes[list].placement;
            let hug_scroll = matches!(pl.height, Len::Hug(_));
            if !hug_scroll {
                let mut a = self.page.nodes[list].parent;
                while a != NO_NODE {
                    let an = &self.page.nodes[a];
                    if matches!(an.placement.height, Len::Exact(_)) || an.placement.min_height > 0.0 {
                        break;
                    }
                    if an.kind == Kind::Scroll {
                        self.warning("G9015", line, "a viewport List inside a Scroll gets its minimum — near zero — and starves; give the List a `height=` (or `height=hug` if the page should scroll as one)");
                        break;
                    }
                    if an.kind.is_stack() && (an.placement.valign != Align::AlignUnset || matches!(an.placement.height, Len::Hug(_))) {
                        self.warning("G9015", line, "an anchored or hug ancestor asks this List for its minimum — near zero — and starves it; write `height=` on the List (or `height=hug` for a content-sized menu)");
                        break;
                    }
                    if an.parent == NO_NODE {
                        break;
                    }
                    a = an.parent;
                }
            }
        }
        // Default looks (D122): a BARE row shows selection, the press and
        // the keyboard cursor out of the box — the addon's resolver
        // injects `selected`, `focused` and `held` responses where the
        // author styled none (per-state: an author's `when selected`
        // replaces only the selected default). `select=off` rows carry
        // the focus and held defaults but never the selected one.
        for i in 0..self.page.nodes.len() {
            if self.page.nodes[i].kind != Kind::ListItem {
                continue;
            }
            let mut off = false;
            let mut a = self.page.nodes[i].parent;
            while a != NO_NODE {
                if self.page.nodes[a].kind == Kind::List {
                    off = matches!(self.page.prop(a, PropKey::Select), Some(Value::VIdent(w)) if self.page.str(w) == "off");
                    break;
                }
                a = self.page.nodes[a].parent;
            }
            let (w0, wc) = self.page.nodes[i].whens;
            let styled = |st: State| self.page.whens[w0 as usize..(w0 + wc) as usize].iter().any(|w| w.state == st);
            let mut add: Vec<(State, Color_Straight_Srgb_4xU8_RGBA)> = Vec::new();
            // Priority comes from `State::priority` (focused < selected <
            // held), so the injection order carries no weight.
            if !styled(State::Focused) {
                add.push((State::Focused, LIST_FOCUS_FILL));
            }
            if !off && !styled(State::Selected) {
                add.push((State::Selected, LIST_SELECTED_FILL));
            }
            if !styled(State::Held) {
                add.push((State::Held, LIST_HELD_FILL));
            }
            if add.is_empty() {
                continue;
            }
            // Whens are one contiguous range per node: move the node's to
            // the table's end and append the injected ones.
            let n0 = self.page.whens.len() as u32;
            for wi in w0..w0 + wc {
                let w = self.page.whens[wi as usize];
                self.page.whens.push(w);
            }
            let line = self.page.nodes[i].line;
            for (state, color) in add {
                let p0 = self.page.props.len() as u32;
                self.page.props.push(Prop { key: PropKey::Background, value: Value::VColor(color) });
                self.page.whens.push(When { state, props: (p0, 1), of: NO_NODE, line });
            }
            let count = self.page.whens.len() as u32 - n0;
            self.page.nodes[i].whens = (n0, count);
        }
        // D129: the Button family ships a default `when focused` ring —
        // the engine's always-on ring, re-owned so it shows only for
        // key-driven focus (GP26 step 2). Authoring `when focused` (node
        // or style) replaces it, the D122 per-state doctrine; `Input` is
        // untouched — its caret is the focus look.
        for i in 0..self.page.nodes.len() {
            if !matches!(self.page.nodes[i].kind, Kind::Button | Kind::Check | Kind::Toggle | Kind::Radio) {
                continue;
            }
            let (w0, wc) = self.page.nodes[i].whens;
            if self.page.whens[w0 as usize..(w0 + wc) as usize].iter().any(|w| w.state == State::Focused) {
                continue;
            }
            let n0 = self.page.whens.len() as u32;
            for wi in w0..w0 + wc {
                let w = self.page.whens[wi as usize];
                self.page.whens.push(w);
            }
            let p0 = self.page.props.len() as u32;
            self.page.props.push(Prop { key: PropKey::BorderColor, value: Value::VColor(BUTTON_FOCUS_RING) });
            self.page.props.push(Prop { key: PropKey::BorderThickness, value: Value::VNum(BUTTON_FOCUS_RING_EPX) });
            self.page.whens.push(When { state: State::Focused, props: (p0, 2), of: NO_NODE, line: self.page.nodes[i].line });
            self.page.nodes[i].whens = (n0, wc + 1);
        }
        // GP33: the automatic couplings' driver rows — the control roots
        // exist now, so the recorded paths resolve to arena ids.
        for (node, root, min, max, state) in std::mem::take(&mut self.pending_drivers) {
            if let Some(rid) = self.page.find(&root) {
                let sid = if state.is_empty() { NO_STR } else { self.intern(&state) };
                self.page.drivers.push(AutoDriver { node, root: rid, min, max, state: sid });
            }
        }
        for (node, root, step) in std::mem::take(&mut self.pending_lives) {
            if let Some(rid) = self.page.find(&root) {
                self.page.live_texts.push(LiveText { node, root: rid, step });
            }
        }
        // GP52: the contracts' `behavior slider` rows — the instance
        // roots exist now; the machine also needs the `slot track` the
        // finger maps over.
        for pb in std::mem::take(&mut self.pending_behaviors) {
            let Some(rid) = self.page.find(&pb.path) else { continue };
            let track = NodeId::range(rid, self.page.subtree_end(rid)).find(|i| self.page.nodes[*i].kind == Kind::Slot && self.page.str(self.page.nodes[*i].name).rsplit('/').next() == Some("track"));
            let Some(track) = track else {
                self.error("G0082", pb.line, "`behavior slider` maps the finger over a `slot track` — the control body declares none");
                continue;
            };
            self.page.behaviors.push(crate::page::SliderBehavior { root: rid, track, min: pb.min, max: pb.max, step: pb.step, key_unit: pb.key_unit, rest: pb.rest, drag_started: pb.drag_started, drag_ended: pb.drag_ended });
        }
        self.transition_scope_guard();
        self.one_writer_guard();
        self.reveal_channel_guard();
    }

    /// A `when` response or an anim lane may drive `reveal_x`/`reveal_y`
    /// only where the node DECLARES the channel (a static `reveal_*=` on
    /// its line) — the shipped rule: an undeclared reveal driven from a
    /// pose would conjure a disclosure the resting layout never shows.
    fn reveal_channel_guard(&mut self) {
        let declared = |page: &Page, node: NodeId, key: PropKey| {
            let n = &page.nodes[node];
            page.props[n.props.0 as usize..(n.props.0 + n.props.1) as usize].iter().any(|p| p.key == key)
        };
        let mut errs: Vec<(u32, String)> = Vec::new();
        for (i, n) in self.page.nodes.iter().enumerate() {
            for w in &self.page.whens[n.whens.0 as usize..(n.whens.0 + n.whens.1) as usize] {
                for p in &self.page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize] {
                    if matches!(p.key, PropKey::RevealX | PropKey::RevealY) && !declared(&self.page, NodeId::from_index(i), p.key) {
                        errs.push((n.line, format!("a `when` drives `{}` only where the node declares it — give the node a resting `{}=` first", p.key.word(), p.key.word())));
                    }
                }
            }
        }
        for t in &self.page.anims {
            let ch = self.page.str(t.channel);
            let key = match ch {
                "reveal_x" => PropKey::RevealX,
                "reveal_y" => PropKey::RevealY,
                _ => continue,
            };
            if !declared(&self.page, t.node, key) {
                errs.push((0, format!("an anim drives `{ch}` only where the node declares it — give the node a resting `{ch}=` first")));
            }
        }
        for (line, msg) in errs {
            self.error("G1017", line, msg);
        }
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
                let target = &self.page.nodes[lane.node];
                for w in &self.page.whens[target.whens.0 as usize..(target.whens.0 + target.whens.1) as usize] {
                    let owner = if w.of == NO_NODE { lane.node } else { w.of };
                    let node_state = matches!(w.state, State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled | State::Custom(_));
                    let same_scope = if t.node == NO_NODE { !node_state } else { node_state && owner == t.node };
                    if !same_scope {
                        continue;
                    }
                    let holds = self.page.props[w.props.0 as usize..(w.props.0 + w.props.1) as usize].iter().any(|p| Channel::of_key(p.key).iter().any(|c| channels.contains(c)));
                    if holds {
                        // The channel's head word, the way the file spells
                        // the property ('scale', not 'scale.x').
                        let word = self.page.str(lane.channel).split('.').next().unwrap_or_default().to_string();
                        let tname = if target.name == NO_STR { format!("{:?}", target.kind) } else { self.page.str(target.name).to_string() };
                        // A per-wearer self-bound copy shows its author name.
                        let aname = self.page.str(t.anim).rsplit('/').next().unwrap_or_default().to_string();
                        let state = state_word(&self.page, w.state);
                        found.push((t.line, format!("'{word}' on {tname} is held by its `when {state}` AND driven by anim '{aname}' — two writers would fight on every flip; the loser flickers")));
                    }
                }
            }
        }
        for (line, msg) in found {
            self.error("G4003", line, msg);
        }
    }

    /// GP31's scope rules and the duplicate fence (the shipped
    /// _bake_view_transitions / _validate_control_transition): a view
    /// rule's endpoints are arrangements only — a control state's
    /// transitions live inside the style/node block; a node rule takes an
    /// arrangement change only as an exact pair; control states stack, so
    /// a node rule times one side at a time; and two rules covering one
    /// pair in one scope would pick silently — refused.
    fn transition_scope_guard(&mut self) {
        let is_state = |st: &State| matches!(st, State::Held | State::Hovered | State::Focused | State::FocusWithin | State::Selected | State::StateDisabled | State::Custom(_));
        // `base` (Arrangement(u32::MAX)) is exempt from the one-sided
        // fence: it doubles as the rest end of a built-in state flip
        // (GPAR9 — `transition base -> held` on a node is a decided Rust
        // form), so only a DECLARED arrangement flags one-sided.
        let is_arr = |st: &State| matches!(st, State::Arrangement(_));
        let is_named_arr = |st: &State| matches!(st, State::Arrangement(i) if *i != BASE);
        let mut errs: Vec<(u32, String)> = Vec::new();
        let mut seen: Vec<(NodeId, Option<State>, Option<State>)> = Vec::new();
        for t in &self.page.transitions {
            let live_node = t.node != NO_NODE && t.node.index() < self.page.nodes.len();
            if t.node == NO_NODE {
                for e in [t.from, t.to].into_iter().flatten() {
                    if is_state(&e) {
                        errs.push((t.line, format!("'{}' is a control state — its transitions live inside the style/node block", state_word(&self.page, e))));
                    }
                }
            } else if live_node {
                match (t.from, t.to) {
                    // The endpoint-less rule is the scope's default feel.
                    (None, None) => {}
                    (Some(a), Some(b)) if is_arr(&a) && is_arr(&b) => {}
                    (Some(a), Some(b)) if is_state(&a) && is_state(&b) => {
                        errs.push((t.line, format!("control states stack — time them one side at a time (`any -> {}` enters, `{}` -> any` leaves)", state_word(&self.page, b), state_word(&self.page, a))));
                    }
                    (a, b) => {
                        let one_arr = a.map(|x| is_named_arr(&x)).unwrap_or(false) != b.map(|x| is_named_arr(&x)).unwrap_or(false);
                        if one_arr {
                            errs.push((t.line, "a node times an arrangement change only as an exact pair — one-sided arrangement rules live at the top level".to_string()));
                        }
                    }
                }
            }
            if t.node == NO_NODE || live_node {
                let key = (t.node, t.from, t.to);
                if seen.contains(&key) {
                    let w = |e: Option<State>| e.map(|x| state_word(&self.page, x)).unwrap_or_else(|| "any".to_string());
                    errs.push((t.line, format!("two transitions cover {} -> {}", w(t.from), w(t.to))));
                } else {
                    seen.push(key);
                }
            }
        }
        for (line, msg) in errs {
            self.error("G0034", line, msg);
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
        State::Arrangement(i) => page.arrangements.get(i.index()).map(|a| page.str(*a).to_string()).unwrap_or_else(|| "base".to_string()),
        State::ScreenOrientation(w) | State::ScreenClass(w) => format!("screen.{}", page.str(w)),
        State::ArrangementScreen(i, w) => format!("{} screen.{}", page.arrangements.get(i.index()).map(|a| page.str(*a)).unwrap_or("base"), page.str(w)),
        State::ScreenPair(a, b) => format!("screen.{} screen.{}", page.str(a), page.str(b)),
        State::Custom(s) => page.str(s).to_string(),
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
        assert_eq!(p.nodes[col].children, (3, 3));
        assert_eq!(p.nodes[col].subtree_end, 8);
        let r = p.find("r").unwrap();
        assert_eq!(p.nodes[r].children, (6, 2));
        assert_eq!(p.nodes[r].parent, col);
        assert_eq!(p.nodes[0].subtree_end, 8);
    }

    #[test]
    fn styles_theme_and_whens_merge_in_spec_order() {
        let p = ok("$accent = #E51400FF\ntheme dark { $accent = #FF6B4AFF }\ntheme { Button font_size=15 }\nstyle Item background=charcoal, min_height=36 { when selected background=$accent }\nView v {\n Button b .Item text=\"x\", background=#112233FF { when held background=white/0.5 }\n}\n");
        let b = p.find("b").unwrap();
        // Node prop wins over style base on lookup (later in the range).
        assert_eq!(p.prop(b, PropKey::Background), Some(Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0x11, 0x22, 0x33, 0xFF]))));
        assert_eq!(p.nodes[b].placement.min_height, 36.0);
        let whens: Vec<State> = p.whens[p.nodes[b].whens.0 as usize..].iter().map(|w| w.state).collect();
        // The trailing Focused is the injected default focus ring (D129,
        // GP26 step 2) — every Button without an authored `when focused`
        // carries it.
        assert_eq!(whens, vec![State::Selected, State::Held, State::Focused]);
        assert!(p.whens.iter().all(|w| w.of == NO_NODE));
        let held_props = &p.props[p.whens[1].props.0 as usize..];
        assert_eq!(held_props[0].value, Value::VColor(Color_Straight_Srgb_4xU8_RGBA([255, 255, 255, 128])));
        let accent = p.intern_lookup("accent");
        assert_eq!(p.theme_value(accent, p.intern_lookup("dark")), Some(Value::VColor(Color_Straight_Srgb_4xU8_RGBA([0xFF, 0x6B, 0x4A, 0xFF]))));
        assert_eq!(p.kind_default(Kind::Button, PropKey::FontSize, NO_STR), Some(Value::VNum(15.0)));
    }

    #[test]
    fn widgets_expand_with_params_and_instance_name() {
        let p = ok("widget LabeledSlider $label=\"\", $min=0, $max=100 {\n Column spacing=4 {\n  Label text=$label\n  Slider min=$min, max=$max\n }\n}\nView v {\n LabeledSlider size label=\"Size\", min=1, valign=top\n}\n");
        let names: Vec<&str> = p.nodes.iter().map(|n| p.str(n.name)).collect();
        assert_eq!(names, vec!["v", "size", "", ""]);
        assert_eq!(p.nodes[1].kind, Kind::Column);
        assert_eq!(p.nodes[1].placement.valign, Align::Start);
        let label = p.prop(NodeId::new(2), PropKey::Text).unwrap();
        assert!(matches!(label, Value::VText { text, .. } if p.str(text) == "Size"));
        assert_eq!(p.prop(NodeId::new(3), PropKey::Min), Some(Value::VNum(1.0)));
        assert_eq!(p.prop(NodeId::new(3), PropKey::Max), Some(Value::VNum(100.0)));
    }

    #[test]
    fn arrangements_handlers_and_transitions() {
        let p = ok("arrangement wide\ntransition base -> wide duration=0.2, easing=cubic_out\nwhen wide { bar.width = 300 }\nView v {\n Button bar width=200, on:activate=go(wide) { transition any -> held duration=0.1 }\n Button b text=\"Save\", on:activate=save\n}\n");
        let bar = p.find("bar").unwrap();
        let w = p.whens[p.nodes[bar].whens.0 as usize];
        assert_eq!(w.state, State::Arrangement(ArrangementId::new(0)));
        assert_eq!(p.props[w.props.0 as usize], Prop { key: PropKey::Width, value: Value::VNum(300.0) });
        assert_eq!(p.handlers[0].action, Action::Go(ArrangementId::new(0)));
        let b = p.find("b").unwrap();
        let h = p.handlers[p.nodes[b].handlers.0 as usize];
        assert_eq!(p.str(h.event), "activate");
        assert!(matches!(h.action, Action::Named(id) if p.str(id) == "save"));
        assert_eq!(p.transitions.len(), 2);
        assert_eq!(p.transitions[0].node, NO_NODE);
        assert_eq!(p.transitions[0].to, Some(State::Arrangement(ArrangementId::new(0))));
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
        let a = p.nodes[p.find("a").unwrap().index()].placement;
        assert_eq!(a.halign, Align::Center);
        assert_eq!(a.margin, [None, None, None, Some(16.0)]);
        assert_eq!(a.max_width, Some(380.0));
        assert_eq!(a.height, Len::Hug(0.0));
        let b = p.nodes[p.find("b").unwrap().index()].placement;
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
        self.strings.iter().position(|x| x == s).map(|i| StrId::from_index(i)).unwrap_or(NO_STR)
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
        let w = p.whens[p.nodes[face].whens.0 as usize];
        assert_eq!((w.state, w.of), (State::Held, ok));
        let tracks: Vec<(&str, &str, Option<f32>, f32, f32, f32)> = p.anims.iter().map(|t| (p.str(p.nodes[t.node].name), p.str(t.channel), t.from.and_then(Value::as_num), t.to.as_num().unwrap_or(f32::NAN), t.start, t.duration)).collect();
        assert_eq!(tracks[0], ("a", "translate.x", Some(-60.0), 0.0, 0.0, 0.5));
        assert_eq!(tracks[1], ("b", "opacity", Some(0.0), 1.0, 0.0, 0.5));
        assert_eq!(tracks[2], ("b", "scale", None, 1.05, 0.5, 0.25));
        assert_eq!(tracks[3], ("a", "opacity", None, 1.0, 0.95, 0.1));
        assert_eq!(p.str(p.anims[2].easing), "back_out");
        assert_eq!(p.str(p.anims[0].easing), "cubic_out");
    }

    /// Every widget instance's descendant `when` lands on ITS OWN body
    /// node (todo-bugs.md #7, 2026-09-06): two instances of one widget,
    /// the second's `when selected glyph.opacity` must host on the
    /// second's glyph, watching the second — never the first's.
    #[test]
    fn each_widget_instance_hosts_its_own_descendant_whens() {
        let c = compile("widget Tick {\n Check width=100, height=20 {\n  Panel box width=20, height=20 { Rectangle glyph width=12, height=12, opacity=0 }\n  when held box.opacity=0.5\n  when selected glyph.opacity=1\n }\n}\nView v {\n Tick a\n Tick b\n}\n", &|_| None);
        assert!(!crate::diag::has_errors(&c.diags), "{:#?}", c.diags);
        let p = c.page;
        for inst in ["a", "b"] {
            let root = p.find(inst).unwrap();
            let glyph = p.find(&format!("{inst}/glyph")).unwrap();
            let boxn = p.find(&format!("{inst}/box")).unwrap();
            let (g0, g_count) = p.nodes[glyph].whens;
            let (b0, b_count) = p.nodes[boxn].whens;
            assert_eq!(g_count, 1, "{inst}/glyph hosts exactly its own `when selected`");
            assert_eq!(b_count, 1, "{inst}/box hosts exactly its own `when held`");
            assert_eq!((p.whens[g0 as usize].state, p.whens[g0 as usize].of), (State::Selected, root), "{inst}: the glyph's rule watches ITS instance");
            assert_eq!((p.whens[b0 as usize].state, p.whens[b0 as usize].of), (State::Held, root));
        }
    }
}
