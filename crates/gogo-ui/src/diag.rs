//! Diagnostics, each with a stable G-code. At build
//! time they fail `cargo build` (file:line); at hot reload the overlay
//! shows them.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub line: u32,
    pub message: String,
}

impl Diagnostic {
    pub fn error(code: &str, line: u32, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Error, code: code.to_string(), line, message: message.into() }
    }

    pub fn warning(code: &str, line: u32, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, code: code.to_string(), line, message: message.into() }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{sev} {} line {}: {}", self.code, self.line, self.message)
    }
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}

/// Every G-code this crate emits → a one-line meaning. Needed because a
/// code is a published search key ("never renumbered, never reused"):
/// without one table, sites drift onto one number and a published code
/// silently changes meaning — the GPAR61 mess this closed (13 published
/// codes carried other meanings here). Bands follow the addon registry
/// (gogo_diagnostics.gd): G0xxx lex/parse · G1xxx placement · G2xxx
/// props/values · G3xxx styles/when/transition · G4xxx anims · G5xxx
/// widgets/uses · G6xxx theme/$refs · G7xxx view root · G8xxx
/// translations · G9xxx repeat. `godot_controls.rs`
/// `diag_source_and_registry_match_both_ways` holds source and table
/// together, both ways.
pub const REGISTRY: &[(&str, &str)] = &[
    ("G0001", "unexpected character"),
    ("G0002", "unclosed block — a `{` without its `}`"),
    ("G0003", "unterminated string"),
    ("G0004", "unknown escape"),
    ("G0005", "one hex color form — `#RRGGBB` or `#RRGGBBAA`"),
    ("G0006", "`$` must be followed by a name"),
    ("G0007", "`/opacity` is 0..1"),
    ("G0008", "`@pos` is 0..1"),
    ("G0009", "bad number"),
    ("G0011", "unexpected `}`"),
    ("G0012", "`$name` needs `= value` here"),
    ("G0013", "expected a node type or `$value`"),
    ("G0014", "expected end of line"),
    ("G0015", "a comma before `{`"),
    ("G0016", "`->` needs a state name"),
    ("G0017", "missing `,` between names"),
    ("G0018", "duplicate parameter / property"),
    ("G0019", "missing `)`"),
    ("G0020", "missing `]`"),
    ("G0021", "expected a value"),
    ("G0022", "`key = …` is only legal inside a `when` block"),
    ("G0023", "`uses` path / translation-key-on-a-path rules"),
    ("G0024", "`uses` file not found"),
    ("G0025", "`style` needs a name"),
    ("G0026", "a style block holds `when` and `transition` lines"),
    ("G0027", "`widget` needs a name"),
    ("G0028", "a widget body has one root"),
    ("G0029", "`anim` needs a name"),
    ("G0030", "anim defined twice"),
    ("G0031", "`arrangement` needs a name"),
    ("G0032", "reserved arrangement name"),
    ("G0033", "arrangement declared twice"),
    ("G0034", "a transition rule is `A -> B`; a control's default feel takes no endpoints"),
    ("G0035", "misspelled word, with the spelling"),
    ("G0036", "unknown screen word / state"),
    ("G0037", "`when` needs a state"),
    ("G0038", "retired spellings — composed `when` limit, `px`, `%`"),
    ("G0039", "`/` and `@` suffixes are for colors and strings"),
    ("G0040", "`@pos` / a list where none belongs"),
    ("G0041", "retired placement words"),
    ("G0043", "unknown flag"),
    ("G0044", "node name used twice"),
    ("G0045", "`gap` takes one size word"),
    ("G0046", "a reserved word cannot name a node"),
    ("G0047", "`gap` lives in a Column/Row"),
    ("G0048", "`gap` needs a size"),
    ("G0049", "raw signals are gone"),
    ("G0050", "`toggle(base)` — use `go(base)`"),
    ("G0051", "unknown arrangement in an action"),
    ("G0052", "not an action"),
    ("G0054", "a `when` block sets properties, not theme values"),
    ("G0055", "`=host` is gone"),
    ("G0056", "a size is a number, `fill`, `hug`, `fill(n)`"),
    ("G0057", "cross-axis alignment word"),
    ("G0058", "a number key given a non-number"),
    ("G0059", "a top-level `when` writes `node.prop = value`"),
    ("G0060", "no slot/node of that name"),
    ("G0061", "an anim step says when it runs — `and` / `then`"),
    ("G0062", "`after=` is not supported yet"),
    ("G0063", "`slot.` anim lanes — a control's OWN scoped anims, motion words only"),
    ("G0064", "an anim lane needs `to=`"),
    ("G0065", "`easing=` takes a name"),
    ("G0066", "`repeat=` takes `forever`"),
    ("G0067", "`a..b` only after `in` on a slot line"),
    ("G0068", "`choice` rules — Check stays standalone, bodied text"),
    ("G0069", "Radio naming / `choice value=` rules"),
    ("G0070", "retired event spellings / nested `choice`"),
    ("G0071", "an asset path carries `res://`"),
    ("G0072", "`on:<event>` fires only on a press surface"),
    ("G0073", "`on:pointer` needs `interactive=true`"),
    ("G0074", "a 1x1 box clips its children (warning)"),
    ("G0075", "`modulate.a` is an engine word — write `opacity`"),
    ("G0076", "a name-list head needs at least one name"),
    ("G0077", "control-body lines in a widget body"),
    ("G0078", "a bool key / `overlay` with `space_only`"),
    ("G0079", "unknown unit on a number"),
    ("G1002", "grid track bookkeeping — arity, spans and the skip they owe"),
    ("G1003", "areas-form rules — rectangles, one name per child, tracks per line"),
    ("G1004", "`wrap=true` with no width to wrap at (lint)"),
    ("G1006", "both margins and an exact size over-constrain the axis"),
    ("G1010", "`height=fill` but no row takes the leftover (lint)"),
    ("G1011", "an aligned axis with `fill` leaves nothing to position"),
    ("G1013", "`min_width` above `max_width`"),
    ("G1014", "margins under a Column/Row/Flow/Grid"),
    ("G1015", "a seat declaration's shape — a named, empty socket in a container"),
    ("G1016", "a seat move's rules — a declared seat, a container home, one writer, no cycles"),
    ("G2001", "unknown property"),
    ("G2002", "a property spelled in another dialect / another kind's word"),
    ("G2003", "a one-stop gradient renders as a solid"),
    ("G2004", "invisible pigment — alpha 00 under color channels (the XAML #AARRGGBB copy trap)"),
    ("G2005", "`stops=` takes the gradient's color list"),
    ("G2006", "`transparent` in `stops=` hides the fade's color"),
    ("G2007", "the word is not a color"),
    ("G2008", "a variant-flipping color inside `stops=`"),
    ("G3002", "an automatic (value-driven) slot property assigned — the control drives it"),
    ("G3005", "one target posed from both screen vocabularies — a class and an orientation can hold at once; compose or pick one"),
    ("G3006", "style targeting / theme-entry shape — `for=`/`extends=` misuse, `theme` in a node, a named/wired/placed/duplicate entry"),
    ("G4003", "two writers on one property"),
    ("G4004", "unknown anim"),
    ("G5001", "widget naming / `for=` host contract"),
    ("G5002", "unknown style"),
    ("G5003", "widget cycle"),
    ("G5105", "an automatic slot in a host that measures children — driven layout must stay relayout-local"),
    ("G5107", "`automatic=` driver coupling"),
    ("G6001", "undefined `$ref`"),
    ("G6002", "`$value` defined twice"),
    ("G6003", "not a palette color"),
    ("G6004", "a palette override needs a color"),
    ("G6005", "a circular palette override chain"),
    ("G6006", "a palette override under a variant other than `dark`"),
    ("G7003", "a file has one root — `View` or `Panel`"),
    ("G7011", "unknown node type"),
    ("G7012", "`View` is the screen root only"),
    ("G7013", "a code-drawn control — an empty box (warning)"),
    ("G8001", "a translation key carries different English"),
    ("G9002", "repeat/slot clause shape (`as $binding`)"),
    ("G9003", "`by` keys the item binding"),
    ("G9004", "the item binding shadows a `$` value"),
    ("G9007", "`empty { }` block rules"),
    ("G9008", "a dotted `$ref` outside a repeat item binding"),
];

/// The codes above that also stand in the addon's PUBLISHED registry —
/// shared on purpose, same meaning. Needed because the consistency pin
/// refuses any OTHER overlap: a new Rust code can never land on a
/// published number by accident.
pub const SHARED_WITH_ADDON: &[&str] = &[
    "G0002", "G1002", "G1003", "G1004", "G2001", "G2002", "G2003", "G2004", "G3002", "G3005", "G4003", "G5002", "G5105", "G6001", "G7003", "G8001", "G9002", "G9003", "G9004", "G9007", "G9008",
];

/// Numbers this crate once used and vacated (or skipped) in the GPAR61
/// renumber — kept so they never come back: an old log or doc citing one
/// must never match a new, different error.
pub const RETIRED: &[&str] = &["G0010", "G0042", "G0053", "G1007", "G1008", "G1012", "G5104"];
