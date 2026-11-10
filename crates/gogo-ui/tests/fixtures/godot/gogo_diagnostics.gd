extends RefCounted
## D68 — gogo diagnostic code registry. A code is a stable search key: it
## appears in output (`error[G3001]: ...`), maps to a long-form explanation in
## docs/errors/<code>.md (the editor panel's "Why?" link), and never changes
## meaning once published. Codes are domain-banded for findability:
##   G0xxx lex/parse          G1xxx placement/layout    G2xxx props/values
##   G3xxx styles/when/transition   G4xxx anims         G5xxx widgets/uses
##   G6xxx theme/$refs        G7xxx view root/host      G8xxx translations
##   G90xx repeat (build)     G91xx repeat (runtime refusals)
## Most sites stay uncoded (plain `error:`) — a code is added when an error
## earns a teaching page, never renumbered, never reused. run_tests.gd
## asserts registry <-> docs/errors consistency both ways.

const REGISTRY := {
    "G0002": "unclosed block — a '{' without its '}'",
    "G1001": "grid cells place by structure (row blocks) or by name (areas) — never coordinates",
    "G1002": "grid track bookkeeping — arity, spans and the skip they owe",
    "G1003": "areas-form rules — rectangles, one name per child, tracks per line",
    "G1004": "`autowrap_mode` with no width to wrap at — dead under hug, a sliver under a minimum grant (lint)",
    "G1005": "a seated hug occupant's wrap-text is wider than the window — hug measures unwrapped; `width=fill` wraps at the pane (lint)",
    "G2001": "unknown property",
    "G2002": "property spelled in another dialect (color / foreground / ...)",
    "G2003": "a one-stop gradient renders as a solid — Rectangle says it honestly",
    "G2004": "invisible pigment — alpha 00 under color channels (the XAML #AARRGGBB copy trap)",
    "G3001": "feel keys (anim= / duration= / easing=) on a `when` line",
    "G3002": "an automatic (value-driven) slot property assigned — the control drives it",
    "G3003": "`seat` and `transition` cannot apply to the same arrangement — a node is never partway between two parents",
    "G3004": "alignment and `transition` cannot apply to the same arrangement — an anchor is never partway between two alignments",
    "G3005": "one target posed from both screen vocabularies — a class and an orientation can hold at once; compose or pick one",
    "G4001": "layout geometry driven by an anim",
    "G4003": "two writers on one property — a `when` holds it AND an anim drives it",
    "G5002": "unknown style",
    "G5101": "a control needs a `script` — without a behavior, it's a widget",
    "G5102": "a built-in state name declared with `state` — it's `state_builtin`",
    "G5103": "live() reads a contract prop — bare reads freeze, other positions don't bind",
    "G5105": "an automatic slot in a host that measures children — driven layout must stay relayout-local",
    "G5106": "a contract feature on a NATIVE built-in instance — Gogo has to draw it",
    "G6001": "undefined $ref",
    "G7001": "a buildable file needs exactly one root node",
    "G7002": "the host class must match the root kind (View -> Control, Panel -> GogoPanelView/PanelContainer)",
    "G7003": "a file's root announces what it is — `View` (screen) or `Panel` (component)",
    "G7004": "`width=host`/`height=host` is a root-only value — inside the tree the parent owns the box",
    "G7005": "a host-owned root axis needs a GogoPanelView host — a raw PanelContainer counts its children",
    "G8001": "translation key already carries different English text",
    "G9001": "a repeat needs a unique name — it's the `set_items` address",
    "G9002": "repeat clauses — `as $binding` declares the item, `by` is required",
    "G9003": "`by` must key the item binding — a field path on it, or the binding itself",
    "G9004": "the item binding would shadow a $value in scope — rename it with `as`",
    "G9005": "repeats don't nest (v1) — the future form sources the inner collection from the outer item",
    "G9006": "repeat can't live inside a widget/control body (v1) — its name is the set_items address",
    "G9007": "the `empty` block — required first in a repeat body (even literally empty), one, nothing on its line",
    "G9008": "a dotted $ref outside a repeat item binding — field paths exist only there",
    "G9009": "a repeat line takes a name and clauses only",
    "G9010": "a List's body takes exactly one direct child `repeat` — the List IS its selection",
    "G9011": "a List row is one `ListItem` root — the unit selection/focus/taps attach to",
    "G9012": "select= picks a List's selection mode — `one`, `many`, or `off`",
    "G9013": "`ListItem` lives exactly one place — the row root of a List's repeat",
    "G9014": "`when selected` inside a List row never fires — the List drives the row ROOT (lint)",
    "G9015": "this List provably gets no height here — hug parents grant only minimums (lint)",
    "G9016": "a List's `empty` lives on the List body — required, shown while no items are present",
    "G9017": "`layout` picks a List's items panel — `Column`, `Row`, `Flow`, or `Tiles`, holding the one repeat",
    "G9101": "items assignment refused — the collection itself (null, non-Array, template failure)",
    "G9102": "items assignment refused — a key (missing field, wrong type, mixed types)",
    "G9103": "items assignment refused — duplicate key within one repeat",
    "G9104": "a List `selected` write refused — wrong shape for the mode, duplicate key, or select=off",
}

const _DOC_DIR := "res://addons/gogo/docs/errors"


static func has_code(code: String) -> bool:
    return REGISTRY.has(code)


static func title(code: String) -> String:
    return String(REGISTRY.get(code, ""))


static func doc_path(code: String) -> String:
    return "%s/%s.md" % [_DOC_DIR, code] if REGISTRY.has(code) else ""
