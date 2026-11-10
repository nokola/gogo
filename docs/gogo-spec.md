# Gogo — declarative UI language for Godot (v1 spec)

*Change it. See it. Gogo.*

This document is the authoritative v1 reference for the Gogo language. It is written
for both humans and AI assistants generating or editing `.gogo` files and tooling.

## Goals

**Main goals — every language/design decision is measured against these:**

- **Easy to understand, easy to write, easy to find. Ergonomic.** A `.gogo`
  file can be read 6 months later and understood.
- **Rust-inspired design**: every construct keyword-announced, one keyword per
  concept, hidden semantics made visible, shorthand legal only as defined
  sugar.
- **Node-type stability** (D83): a property edit must never change what a
  node IS. Gogo compiles to scenes users script against
  (`get_node("bg") as ColorRect`) — a prop that swapped the underlying class
  would break casts and tweens, spooky action at a distance. When the cost
  or character differs, the TYPE differs (`Rectangle` vs `Gradient` vs
  `Circle` — Rust's `&str`/`String` honesty).
- **The capitalization law** (D83): **capitalized = an addressable node**
  (nameable, stylable, animatable); **lowercase = structure** (`row`,
  `skip`, `gap`, `choice`) — never named, erased at build or anonymous.

1. **Declarative Godot UI** with XAML lineage (panels, styles, themes, includes) but
   none of YAML's fragility (significant whitespace, `children:` ceremony, quoting rules).
2. **Instant hot reload**: files are re-parsed and re-instantiated on save. Parse errors
   never blank the running UI — the previous good tree stays live and the error is shown
   as an overlay with `file:line`, the offending line, a caret, and a fix hint.
3. **Minimal ceremony**: the node name *is* the widget type; braces *are* the children;
   bare numbers *are* epx.
4. **Dumb parser, smart resolver**: the parser knows only the grammar. `theme`, `style`,
   `widget`, `control`, `uses`, `slot`, `signals`, `behavior`, `state`, `state_builtin`,
   `signal` are ordinary nodes/flags that the *resolver* interprets. Property typing,
   unit meaning, enum validation, did-you-mean suggestions, and auto-wiring all live in
   the resolver against the widget registry.

## File model

- Extension: `.gogo`. One UI fragment (or theme/style/widget library) per file.
- **The ROOT KEYWORD SET is {`View`, `Panel`}** (D104/D105) — a file
  announces itself as a SCREEN or a COMPONENT in its first word.
  **`View` — the screen**: the host class (a non-Container `Control`,
  `GogoView`) IS the root; children place FREELY (halign/valign + margins);
  the root reports its biggest child as its minimum — ANSWERED at query
  time since 2026-08-16 (`GogoView`/the C# hosts delegate
  `_get_minimum_size` to the one view formula), so `custom_minimum_size`
  stays purely the declared floor and no build-time transient can be
  stamped into it (a custom host class that extends neither base keeps the
  old stamped wiring, best effort) — and
  `View background=...` draws the root plate (`padding` on a View root
  folds into every declared child's placement, D114 — no wrapper Panel).
  **`Panel` — the component**: the root IS the styled box (a toast banner,
  a progress track) — container regime for its children, native
  padding/border stacking, no wrapper node; the host extends
  `GogoPanelView` (since D283 a plain `Container` playing PanelContainer
  through gogo's own measure — needed so `width=host` axes can ignore
  children; a raw `PanelContainer` host is still accepted, minus host
  axes). Any other root type is a load-time error (G7003); a host class
  that doesn't match the root kind is one too (G7002). Fragment files meant only for `uses` (themes/styles/widgets)
  have no root and aren't built standalone.
- `uses "relative/path.gogo"` imports another file's theme values, styles, and widgets.
- Comments: `//` to end of line. No block comments in v1.
- Whitespace between tokens: space, tab, CR — and NBSP (U+00A0), so text
  pasted from rendered docs (the editor's Info-pane examples harden their
  spaces to stay unwrapped) tokenizes identically (D86).
- WORD JOINERs (U+2060) are stripped before lexing: the same rendered docs
  glue `-` and `/` with them so `row_span` never wraps mid-token, and a
  joiner pasted INSIDE a token must not split or rename it. Zero-width,
  so only caret columns on such lines drift, and only until the file is
  saved clean (D87).

## Values

| Kind   | Syntax                  | Notes |
|--------|-------------------------|-------|
| string | `"Paint"`, `"new stroke"/newStroke` | Escapes: `\"` `\\` `\n` `\t`. No interpolation in v1. Optional `/key` suffix = translation term (see below). |
| path   | `"res://icons/undo.png"`, `"preview://brush"` | A string on a resource property (`source=`, `icon=`, `font=`), scheme always written (G0071 — a bare word could be a label or a path). `res://` is an asset under the build's resource root, checked and embedded at build time. Any other scheme on an `Image`'s `source=` is a **host picture**: the app's code registers pixels under that exact name at run time (a live preview, a canvas tile); the build bakes the name as the picture's id, and the box shows nothing until the code registers it. `""` = no picture. Never translated — `/key` on a path is an error. |
| slice  | `animals[12]`, `toolbar.play`, `bubble` | An **atlas slice** on `source=` (and in a theme value or widget param that feeds one): a grid cell by number, a named slice, or a whole-picture atlas — see Atlases. A bare word on `source=` always names an atlas (a path is quoted); `[n]` touches the name, no space. |
| number | `36`, `0.5`, `-4`       | Optional unit suffix: `epx`, `px`, `%`. Bare number on a *length* property = **epx** (effective pixels, density-independent). `px` = raw device pixels. Units on scalar properties (e.g. `Slider.value`) are a resolver error. |
| bool   | `true`, `false`         | |
| color  | `#RRGGBBAA`, `#RRGGBB`, `red`, `black/0.5`, `white@0.24` | RGBA hex, alpha **last** (Godot/CSS convention). 6 digits ⇒ alpha `FF`. Bare palette names (`red`, `danger`, `surface`, `transparent`) are contextual keywords legal only in color-typed positions (see Resolver-defined vocabulary). Every color spelling takes an optional `/opacity` suffix (a bare 0..1 number) setting the **final alpha**: `black/0.5`, `#FF6347/0.25`, `$accent/0.3`. `/o` **replaces the alpha of the value it is written on** — on a `$ref` or palette word, the alpha that value would have had: `$flash/0` is `$flash` made transparent (a translucent disc's fade stop derives from the disc), `$dim/0.3` over `$dim = #80808080` draws 0.3, `$a/0.3` over `$a = $b/0.5` draws 0.3. An 8-digit hex takes it too (`#FFFFFF6B/0` — a pasted color, faded). The suffix written at the slot is the alpha the slot gets; there is nothing else to know (E6, 2026-08-31). Inside a Gradient's `stops=` list an entry may also carry `@position` (0..1, always after any `/`) — fenced everywhere else. **Porting from WPF/XAML:** WPF writes `#AARRGGBB` (alpha first) — move the leading alpha pair to the end (`#AA000000` → `#000000AA`). |
| ref    | `$name`                 | Reads a widget parameter (innermost scope) or theme value. |
| ident  | `center_bottom`         | Bare word: enum member or handler name; validated by resolver per property type. |
| size   | `220x110`, `220x`, `x110` | Shorthand for an exact size on both axes (the `size=` prop: min_size + don't-stretch). Components are epx only (no units inside). Omitted component = unset. `min_size=` is the minimum-only spelling. |
| call   | `play(appear)`, `fill(2)` | Ident + parenthesized args. Callees are closed per position: `play`/`go`/`emit` on `on:` keys, `fill(n)` in sizing positions (weighted leftover — n may be fractional). The controlled on-ramp for v2 expressions. |
| list   | `[80, hug, fill(2)]`    | Ordinary values in brackets; newlines are free inside the brackets (the list cousin of trailing-comma continuation). Only Grid's `cols=`/`areas=` and Gradient's `stops=` accept one — no other prop is list-accepting. |

**Attached dots form a path**: `listBrush.translate.x` (no spaces) lexes as ONE
identifier — an anim track target or a dotted `when` key. A dot after
whitespace is style application (`Button b .ToolButton`); `position .x` with a
space therefore fails loudly as "unknown style 'x'".

**`->` is the transition arrow** (`transition base -> expanded`): a dedicated
token. `-` is never part of an identifier (identifiers are snake_case, D137) —
outside `->` and a negative number literal it is a lex error, which keeps the
character free for future operators.

**`/` and `@` are the value-suffix qualifiers** — one grammar production
each; what they qualify depends on the value's type. `/` on a color is
opacity (above); `@` on a color is a **gradient-stop position** (D83): 0..1,
always after any `/` suffix (`blue/0.5@0.24`), legal only on entries of a
Gradient's `stops=` list — everywhere else it errors with the route. On a
string, `/` is the **translation term id** (D81): `text="new stroke"/newStroke`.
A term id may start with a digit (`text="4:3"/4by3` — the position after a
string's `/` is unambiguous, so the whole word is the key, never a number).
The literal is the ENGLISH SOURCE TEXT and the design-time value; the key
indexes host-fed locale tables (`gogo.set_translations` / `set_locale`), and
a key the active locale lacks falls back to its literal — never the raw key,
never a placeholder. A host can also RE-KEY a node's text at runtime: in
FanRust the build emits a `Key` per term id and `set_key(node, Key)` swaps
which term a node shows (localized under the active locale), while
`set_text` writes a literal that is never translated — two writes, two
meanings, so a code-fed string can never half-translate. One key carries one English string: repeating a key with
the same literal is legal sharing, a different literal is a build error
(G8001, the drift the fused spelling exists to prevent). Keys are legal
wherever the string is DISPLAYED (node/style props, widget params, custom
properties) and rejected where it isn't (theme `$values`, resource/`uses`
paths). `/` on any other value type is an error. `/_` is no key at all:
the string stays English on purpose (Rust app) — it never interns, so two
`/_` strings may carry different English. A file whose top carries the line
`allow untranslated` keeps all its text English on purpose; a host that
requires keys (`CompileOptions::require_keys`, Rust app) makes every other
displayed string with no key an error (G8002). `allow` is a top-of-file
line and takes exactly `untranslated` (G8003).

Property keys are snake_case — ONE spelling (D137; the kebab alias is gone,
`min-height=` no longer lexes). Gogo keys and raw Godot engine properties now
share the engine's own convention (`min_height`, `margin_bottom`,
`autowrap_mode=word_smart` — copy-paste friendly both ways, and a double-click
selects the whole key).

## Grammar (EBNF)

```ebnf
document    = { node | theme-value } ;
node        = TYPE-IDENT ,
              { name | "." STYLE-IDENT | flag | property | param-decl | arg
                | arrow-endpoint | clause | "," } ,
              [ block ] ,
              terminator ;
arrow-endpoint = "->" , IDENT ;             (* after the name: transition endpoints *)
clause      = ( "as" | "by" ) , value ;     (* repeat lines only; comma-FREE — the keyword
                                               delimits, the arrow-endpoint precedent;
                                               fixed order: as before by *)
theme-value = "$" IDENT , "=" , value , terminator ;  (* "=" required; top level = default, inside `theme <variant> { }` = override *)
name        = IDENT ;                      (* first bare non-flag identifier *)
flag        = "signals" ;                  (* reserved flag words; comma-FREE like a
                                               clause — the keyword delimits, so
                                               `Button signals text=$x` parses; a comma
                                               after the flag stays legal *)
property    = PROP-IDENT , "=" , value ;   (* PROP-IDENT may contain "-" and ":" e.g. min_width, on:activate *)
param-decl  = "$" IDENT , "=" , value ;    (* meaningful only on widget declaration lines *)
arg         = value ;                      (* positional, e.g. uses "path" *)
block       = "{" , { node | theme-value } , "}" ;  (* "{" must be on the node's logical line *)
terminator  = NEWLINE | ";" | EOF | (peek "}") ;
value       = ... | list | index ;
list        = "[" , [ value , { "," , value } , [","] ] , "]" ;  (* newlines free inside *)
index       = IDENT , "[" , DIGITS , "]" ;  (* ATTACHED — no space before "[": a grid atlas cell,
                                               `animals[12]`; a list always follows "=", "," or a space *)
```

Rules:
- **Commas are required between properties/params/args** and forbidden as trailing
  garbage before `{`.
- **Trailing comma continues the node onto the next line** (the only line-continuation
  mechanism; there is no `\`). Indentation is never significant.
- Canonical member order on a line: `Type name .Style prop=..., prop=... { children }`
  (repeat lines: `repeat name as $b by $b.field {` — clauses sit where props would).
- Duplicate property on one node = parse error.
- A `$ref` may carry an ATTACHED dotted field path (`$task.id` — one token; a dot
  after whitespace stays style application). The resolver fences dots to repeat
  item bindings (G9008); theme values and params are flat.

## Node anatomy (AST)

Every node has: `type`, `name` (optional), `styles[]`, `flags[]`, `args[]` (positional
values), `props{}` (name → value), `params{}` ($-declared defaults), `children[]`, `line`.

## Resolver-defined vocabulary

These are ordinary nodes to the parser; the resolver gives them meaning:

- **`$name = value`** (top level) — defines a theme default, referenced anywhere as
  `$name`. Example: `$accent = #E51400FF`. The `=` is required. No `theme` keyword
  needed for defaults.
- **`theme <variant> { $name = value ... }`** — a **variant holder**: overrides that
  win over the defaults while `<variant>` is the active theme
  (`Gogo.set_theme_variant("dark")`; tracked live views rebuild on switch). A file
  that doesn't define the active variant just uses its defaults. A `theme` block
  without a variant name is the DEFAULTS block: its `$name` entries land in the
  same scope as top-level defaults, and it is also the home of palette overrides
  (`red = #E51400`) and control defaults (`font_size = 15`) — see below. Duplicate
  `$name` in the same scope (defaults, or one
  variant) is an error; a variant may freely re-define a default — that's the point.
- **Built-in color palette** (GOGO COLOR PALETTE v1) — color names are
  **contextual keywords** legal only in color-typed positions
  (`background=red`, `color=danger`), the way `fill`/`hug` are contextual on
  `width=`; they are never global identifiers (a node named `red` stays
  legal). Overridable inside theme blocks **without** the `$` prefix:
  `theme { red = #E51400 }`, `theme dark { surface = #141414 }`. Override
  values: a hex color, another color name (validated against the closed set,
  with suggestions), or a `$ref`. The name set is closed — new names belong
  to `$tokens`. Document overrides win over shipped defaults. An override
  by another color NAME is followed at every use, so `theme { red = X }`
  moves `danger` too (the alias of `red`). Palette overrides live in the
  plain block (every mode) or `theme dark { }` (the dark column only) —
  under any OTHER variant name they are refused (G6006, GPAR88): only
  `dark` has a second color column, and a variant-scoped color is a
  `$value` (`theme fancy { $c = … }`), which IS variant-gated.
  - Literals (mode-independent): `transparent` (`#00000000`), `black` `white`
    (exact `#000`/`#FFF`, reserved for testing — leave unoverridden by
    convention), `red` `orange` `yellow` `green` `teal` `blue` `indigo`
    `purple` `pink` `brown`, neutral ramp `smoke` `silver` `gray` `charcoal`.
    Any name takes the `/opacity` suffix: `black/0.5` (see Values).
  - Semantic aliases, **resolved at theme time**: `accent`=`blue`,
    `on_accent`=`white`, `danger`=`red`, `warning`=`yellow`,
    `success`=`green`, `info`=`teal`, `text_muted`=`gray` — remap the literal
    and the alias follows, or remap the alias itself
    (`theme { accent = purple }`).
  - Mode tokens `surface` `panel` `border` `text` `text_muted` carry a light
    and a dark value; the dark column applies while the active variant is the
    **well-known name `dark`** (any other variant reads light). Overriding
    such a token in the default block replaces the built-in pair for all
    modes.
  Exact values: REFERENCE.md "Named colors".
- **Control defaults in `theme { }`** — `prop = value` entries (`font`,
  `font_size`, and `color` — the document's text ink, decided 2026-08-30)
  go on a Godot `Theme` on the view root and propagate to every
  descendant without its own override (REFERENCE "Theme & tokens"). Inside
  a node's body, a one-line `theme font=… / font_size=… / font_color=…`
  carries those text words down that subtree only; any other word on a
  block theme line is refused (G3006), and a nested `theme dark { }`
  block is refused too — variants live at the top of the file.
  **Built-in default: `font_size = 15`** (D88): a document that declares no
  font_size behaves exactly as if it declared 15 epx — text never silently
  keeps Godot's raw, density-blind pixel default. A `default_font_size` the
  host already put on the root wins (gogo never clobbers a user theme).
- **`repeat <name> [as $binding] by <key-path> { empty { ... } <template> }`** —
  N children from a code-set collection (`Gogo.set_items(view, "<name>", items)`),
  reconciled by KEY on each assignment: kept keys keep their nodes, changed data
  rebuilds, absent keys free, order follows the collection. Erased at build (a
  hidden marker anchors the spot; items parent flat into the repeat's parent).
  `as` defaults to `$item` (defined sugar); `by` is required and must key the
  binding — no index identity, ever. `$binding.field` reads item data as a static
  value re-applied per assignment. `empty { }` is REQUIRED, first in the body —
  every list answers the zero-items question visibly (a literal `empty { }` is
  legal: "considered, show nothing"); it shows at zero items, the initial state
  included. It holds ONE child (decided 2026-08-31): two would need an
  arrangement nobody wrote — `empty { Column { … } }` says stacked,
  `empty { Layers { … } }` says overlaid; more than one is a load error
  naming that fix. Inside a `List` the empty block sits on the LIST body instead
  (D128, one seat per context — G9016 routes it): richer zero-items views
  beside the statics, and the List's repeat carries none. Bad data (duplicate keys above all) REFUSES the
  whole assignment loudly; the last good items stay live. Fenced out of widget/
  control bodies and out of other repeats in v1 (addressability). Full design:
  `docs/design/2026-07-15-spec-repeat-and-list.md`; REFERENCE "Repeat".
- **`style Name prop=..., prop=...`** — a named property bag. Applied to a node with
  `.Name`; local properties override style properties. Styles have no structure and no
  parameters.
  - **`for=<Control>`** (first property when present — D109) points the style at one
    control type's contract and turns on WRITE-time checking: `slot.<name>.<prop>`
    paths restyle the control's slot regions (unknown slots list the real ones,
    driven properties error G3002 — the control moves them, content props route to
    filling), and `when` state names verify against the target's states (unknown or
    un-opted contract states error with the list; universals always pass). The target
    must be reachable where the style is WRITTEN (`uses`); widgets aren't targets.
    Slot paths are also legal on contract-bearing instance lines
    (`Fader f slot.track.height=2`). Applying a `for=X` style to another type is an
    error; on a NATIVE built-in, contract features error G5106 with the `render=gogo`
    opt-in as the confirm-level fix (or "drop the `for=`" when the style needs
    nothing from the contract). Plain styles keep taking any state name — contract states check at the
    APPLICATION site ("Button has no state 'busy' — states here: ..."), and a
    plain custom state landing cleanly on exactly one control type draws the add-for=
    advisory (two-plus distinct clean types = deliberately generic, advisory silent).
    `extends=`: same-target chains and for=-over-plain are fine; plain-over-for= and
    cross-target chains error.
- **`widget Name $param=default, ..., rootProp=... { <single root node> }`** — a
  reusable component. On the declaration line: `$x=v` declares a parameter (defaults
  required in v1); bare `k=v` bakes a property onto the root; `.Style` applies to the
  root. Instantiated exactly like a built-in: `Name instName param=..., other=...`.
  - Instance name and any instance property that is **not** a declared parameter pass
    through to the widget's root node.
  - **`slot`** — where a widget takes content (control-customization spec §3). Bare
    `slot` is the default slot (at most one): the inline marker instance children land
    at; it may carry default content in braces, replaced when the instance brings bare
    children. `slot <name> [props] { default content }` declares a named REGION (a real
    node): its line takes placement + the box family (`background`/`corner_radius`/
    `border-*`/`padding` — content props route to filling), content is clipped and
    fills it, and its minimum derives from content + padding. Instances fill by name:
    `slot header { ... }` (fills take no props); unknown names error with the slot
    list; nested slot declarations survive fills and draw above them. Bare children at
    an instance whose widget has no default slot = resolver error listing the named
    fills; duplicate slot names error at the declaration.
  - **`signals`** — a flag on one node inside a widget body marking the signal surface:
    instance-level `on:` wiring, auto-wiring, and code-behind lookup bind to this node.
    Omitted ⇒ the root is the surface.
  - Widgets may use other widgets; cycles are a load-time error (cycle path printed).
  - **`for=<Host>` (D276)** — the announced host contract: `widget Name for=Radio, $p=v
    { ... }` says the body fills a stateful host's body (hosts: `Button`, `Radio`,
    `Check` — the button family). `for=` comes first on the line (the style rule, D11).
    A `for=` widget may carry `when <state> ...` / `transition ...` lines at widget
    scope, beside the body root — they respond on the HOSTING control (exactly as if
    written inline as the host's own children), while their `name.prop=` targets
    resolve in the widget body's per-instance names:
    ```
    widget RadioLook for=Radio, $label="" {
        Row { Circle dot opacity=0 ...; Label text=$label }
        when selected dot.opacity=1     // the HOST Radio's selected
    }
    choice c { Radio a { RadioLook label="A" }; Radio b { RadioLook label="B" } }
    ```
    A `for=` widget instantiates only as the DIRECT body of its host type — anywhere
    else is a build error naming the contract. The host's own inline `when` for the
    same state replaces the widget's response for that state (nearer scope wins, the
    instance-over-body rule). Widget-scope `when`/`transition` without `for=` is a
    build error — there is no host state to answer. Inside any widget body, `when
    selected` on a plain (non-toggle, non-ListItem) node is a build error: nothing
    raises it there, the response could never show (the D276 dead-when trap).
- **`control Name $param=default { ... }`** — a widget plus an INPUT
  BEHAVIOR (control-customization spec §5, D108). Same declaration-line
  grammar as `widget`; the body holds directive lines beside exactly one
  root: `behavior <name> by $value in $min..$max[, step=$step][,
  key_unit=5%]` (the behavior that answers the instance's finger and keys
  — gogo's `slider` and `toggle`, or one the host registered in Rust
  (`gogo::Behavior` + `CompileOptions::behaviors` + `Input::register`);
  optional — without one the control's input is its code-behind's, through
  `on:pointer=` and `keys`; the name is checked, the slots the behavior
  reads must be declared, and the key groups it owns refuse a `keys`
  claim — G0082), `state <names>`
  (custom states the code-behind raises through `Runtime::set_custom`),
  `state_builtin <names>` (opt-in to the runtime-maintained set — read,
  never written, by anything but the runtime; `selected` on a behavior
  control derives from the value), `signal <names>` (the names a
  behavior may raise — `drag_started`/`drag_ended` at a gesture's edges —
  and auto-wiring binds), `keys <groups>`
  (FanRust, 2026-09-02: the key groups a focused instance claims for its
  code-behind — `arrows`, `home_end`, `enter`, `space`, semantic words
  only; a claimed key arrives as a `Key` event on the instance root and
  never moves focus, Tab is never claimable — the built-ins' static-claim
  law made declarable where the addon's behaviors claimed at runtime via
  `key_action`), and a bare
  `transition` rule (the control's default FEEL: it times state flips and
  the fraction glide when no style or node block overrides). `$params` are
  LIVE props (real properties on the root; code-behind reads/writes them);
  default content shows one with `live($param)` — a bare `$param` read in
  the body is an error with the wrap fix-it (G5103). Slots may declare
  an automatic coupling, fully spelled — nothing defaults invisibly
  (D134): `automatic=<kind> by <driver>`. Kinds: `length`/`position`
  (linear), `arc`/`angle` (radial — config REQUIRED in the kind's parens:
  `arc(from=-225, sweep=270)`, degrees, 0° right, clockwise positive),
  `presence` (existence). The fraction kinds drive
  `by $value in $min..$max` (v1: exactly those params — the fraction
  `(value-min)/(max-min)`); presence drives `by <state>` (a bool has no
  range). The runtime driver alone writes the driven geometry; mid-drag
  writes land directly, discrete changes glide. The boundary: if the
  geometry displays DATA, it's `automatic=`; if it's presentational
  motion, it's an `anim` (`repeat=forever`) — there is no clock driver,
  and a timer is code writing a `$param`. A control's own scoped anims
  may target `slot.<name>.<motion word>` — the one place anims address
  slots.
- **`render=gogo|native`** (contract built-ins only — spec §5.7, D111) — picks the
  instance's implementation. Unmarked = native pass-through (byte-identical,
  forever); `render=gogo` swaps in the addon-shipped Gogo composite (same
  contract `for=` styles check against; near-pixel-identical unstyled; behavior
  conformance-tested against the native node). `render=native` is the spelled
  opt-out against a theme entry and a legal, silent defensive pin anywhere else.
  On a user control `render=gogo` is silently redundant, `render=native` errors.
  The one sanctioned exception to node-type stability: explicit, instance-local,
  consequences stated in the diagnostic (D83 carve-out). Composites: Slider
  (both backends) and Toggle (gogo-ONLY — a new control with no native class:
  unmarked instances take the composite, `render=native` errors, `render=gogo`
  is silently redundant; D113). Button deliberately has none (D112): children
  are free-form content, plain styles cover held/selected, widget slots give
  named fill points — `for=Button` styles are write-time state checking and
  apply natively.
- **Theme type defaults** (spec §5.8, D111) — a type-keyed entry inside a theme
  block, `theme { Slider .ThinSlider render=gogo }`, defaults every instance of a
  CONTROL type: an instance-line prototype (no name, no children, no `on:`
  wiring, no placement props). Nearest scope wins (an instance line beats the
  entry); the entry's style seats base-most; `render=` is variant-invariant
  (default block only — variants override the LOOK half wholesale); an
  instance's `render=native` sheds the entry whole, style included.
- **`uses "path.gogo"`** — import (relative to the current file).
- **`on:<event>=HandlerName`** — explicit wiring property. `on:` speaks two
  visible registers (D181): bare stems are gogo events (`activate`,
  `change`, `edit`, `show`, `hide` — see Input vocabulary); snake_case
  past-tense names are raw Godot signals (`on:drag_ended`). Also takes
  `play(animName)`; `on:show`/`on:hide` are visibility pseudo-events (see
  Animations).
- **Extra properties** beyond the engine's: `opacity` (modulate.a),
  `font_size` (epx), `font_color`, `icon_size` (epx, buttons),
  `background`/`corner_radius`/`border-*`/`padding` (Panels AND buttons;
  on buttons the shape keys reshape per-state COPIES of the theme's boxes —
  the theme's own press/hover/disabled feedback survives — while
  `background=` recolors one shared box across all state slots, the pigment
  takeover (D128). On a `Circle`,
  `border_color`/`border_thickness` drive its ring properties instead).
  `padding` alone of the family also applies to the free-placement regions
  (D114) — `Layers`, the plain `View` root, slot lines: no stylebox, the
  declared air folds into every DECLARED child's placement and adds onto the
  region's minimum — and to the text leaves (D118 follow-up, user call):
  `Label`/`Input` inset their own text through the button mechanism (WPF's
  TextBlock/TextBox Padding, verbatim; their other box keys stay refused).
  The doctrine: **paint belongs to boxes; padding is air between a node's
  box and its CONTENT — children or text — and belongs to whoever honestly
  owns that inside**. Containers (`Column`/`Row`/`Grid`/`Tiles`) refuse the
  word toward `gap` / a `Layers`/`Panel` wrapper, never leaving it silently
  dead (Grid/Tiles are gogo-sorted but refuse WITH the family — containers
  behave alike); drawing leaves (`Rectangle`/`Circle`/`Image`) have nothing
  inside — outer air is `margin`.
  `padding` also comes per side — `padding_left/-top/-right/-bottom` —
  mirroring the `margin-*` family (WPF's per-side Padding thickness).
  `corner_radius` also comes per corner (D135) —
  `corner_radius_top_left/-top-right/-bottom-right/-bottom-left`: NAMED
  keys, so there is no corner order to memorize (the WPF/CSS positional
  four-value form is exactly the hidden semantics gogo bans). The uniform
  writes all four corners, a per-corner key writes one, later on the line
  wins — the `padding`/`padding_left` layering law. Slots take them too
  (box-family members everywhere the family goes).
  `padding` is air INSIDE the border (D80): content sits
  `border_thickness + padding` from the node's edge, WPF/CSS stacking —
  never the raw Godot content margin, which the border would overlap. An empty
  string on a resource property means null (no load, no error); `style=X` is
  a load-time error that redirects to `.X`.
- **Input (D103)** — one model: interactive controls (Button/Slider/Input/
  Scroll, …) answer input by default, everything else is transparent, and
  **`interactive=true/false`** overrides that — per control, like any bool
  property (legal in `when` blocks; applies instantly, restores on release).
  `true` claims input for the node, `false` makes it hit-invisible without
  restyling (`disabled` is the restyling one). Region silencing is a catcher:
  an invisible `interactive=true` plate over the region; `interactive=false`
  on an already-passive node is an advisory-lint no-op. Bare `interactive`
  (no `=`) is a load-time error redirecting to `interactive=true` — it never
  silently becomes a node name. The raw `mouse_filter` property is RETIRED
  (a second spelling of the model): its build error branches per value —
  `stop` → `interactive=true`, `ignore` → delete (transparent is the
  default), `pass` → delete (unhandled events bubble by default) — the same
  fence style as the raw `offset_transform_*` props.
- **Placement** (one vocabulary for both parent regimes — compiled to size
  flags for container parents AND an anchor preset for plain-Control
  parents; each regime reads its own encoding):
  - **Default: a child takes the space its parent gives it.** An axis with
    no placement key takes the parent's extent — container slots (cross axis
    fills) and free parents (Layers/plain Control/buttons: the whole rect)
    alike. Exempt: the document root (host-placed) and content of the bare
    default `slot` (the inline marker — its children join the widget's own
    layout, explicit placement only; named slot REGIONS fill like any free
    parent). `width=hug, height=hug` is the opt-out for code-behind-owned
    rects; a ROOT whose box the code sizes declares it outright with
    `width=host` / `height=host` (below).
  - `halign=left/center/right`, `valign=top/center/bottom` — position ONLY.
  - `width`/`height` — the sizing triad: `<epx number>` = exact (min_size +
    don't-stretch), `hug` = natural size, `fill` = take the leftover,
    `fill(n)`/`hug(n)` = weighted (one stretch ratio per control — differing
    per-axis weights are an error). **A box that fills a written box takes
    that box** (decided 2026-08-30): a box with no size word (or `fill`)
    takes the room it is given; when that room is a number the author wrote
    — an exact size or a cap on the box it fills, or on any filler above it
    — that number is the answer, so a written size is hard for the box AND
    for what fills it (`Layers bar height=80 { Layers inner { Rectangle
    height=150 } }`: `inner` is 80, the 150 hangs out and the written box
    cuts it). When the room comes from nowhere the author wrote (the
    screen, a chain of unwritten boxes) the box is never smaller than its
    content. A hug box is sized by content and passes no written number
    down; a `min_*` floor is a written number and wins; a Label keeps the
    width its text needs (text does not bend to a box; the written box cuts
    it). The Rust port does this; the Godot addon lets the engine grow the
    filler to its minimum — a recorded difference. And — on the DOCUMENT
    ROOT only —
    `host` = the host's CODE owns the axis (D283). A host axis contributes
    nothing to the root's minimum (declared `min-*` floors still count),
    gets no size flag and no anchor, takes no alignment, margin or cap
    (each is refused — a second writer would fight the code every frame),
    and so can never be inflated by a child's transient measure: Godot's
    `set_size` clamps to the combined minimum and BAKES the clamped value
    into the offsets, and a later minimum drop never shrinks the box back
    — one frame of a wrap label measured at a stale width (one character
    per line) grew a code-seated ad strip to the whole screen (D282).
    `host` off the root is G7004; on a `Panel` root it needs the
    GogoPanelView host (G7005 — a raw PanelContainer counts its children
    in C++ and ignores gogo's measure). `min_width`/`min_height`/`min_size`
    stay minimum-only (stretchy). Hug means natural size EVERYWHERE (D254):
    wrap-capable text measures at its UNWRAPPED single-line extent, capped
    or not — wrapping happens only where a width binds it (a cap, `fill`,
    or a sized parent). Under an engine container (which grants a
    non-stretched child exactly its minimum) a hug or aligned-no-size
    width carries a live natural-width floor in custom_minimum_size.x, so
    the grant IS the natural size — same meaning as under free parents.
    An autowrap label whose width provably never binds gets the G1004
    lint (dead under a hug chain; a one-char sliver only where no size
    keyword was written at all — a bare label on a Row's main axis).
    Pre-layout, an autowrap label measures ONE LINE, not one char per
    line (D255): the engine computes wrapped min-height at the label's
    current width, which is ~0 until first layout, so every build's last
    applier seeds each wrap label's size with its unwrapped extent —
    hidden or just-built trees (a closed popup, a page on its push
    frame) report sane minimums without waiting for a layout pass. A min next to an exact `width=`/`height=` on
    the same axis is legal and the min still wins (GPAR79 2026-08-31,
    dropping D135's refusal): `size = max(floor, written)` — the WPF
    Min-over-Width order, the same clamp posed sizes obey — so the pair
    never resolves by declaration order. The floor's real job there is
    bounding posed sizes on that axis. A min with `aspect=` is refused the same way the cap is (a
    forced axis would silently break the ratio).
    - **`max_width`/`max_height` — the size caps** (D132; the List caps
      D123/D124 generalized): pair with `hug` (content size, at most the
      cap) or `fill` (available space, at most the cap — D245, below). On
      a hug axis: natural content size, at most the cap. Wrap-capable text hugs its UNWRAPPED
      extent and starts wrapping only when the cap binds (a wrap-Label's
      plain minimum collapses toward its longest word; the cap is what makes
      "hug a wrapping label" expressible). The box tracks content LIVE —
      code-behind text swaps re-fit it. A cap never shears content: hard
      minimums win (WPF's own clamp order — Min beats Max), and since D135
      that clamp is gogo's OWN math (`size = max(floor, min(natural, cap))`
      in the placement seat), so the baked offsets tell the truth instead
      of leaning on the engine's grow-direction rescue. A written size or
      cap that binds is a hard edge BOTH ways: what pokes past it is
      clipped in the picture and takes no taps — a button's clipped half
      is neither drawn nor pressable (GPAR37). V1 binds under
      FREE parents (Layers/plain Control/slot regions); an engine container
      refuses it at load — it grants children their minimum, which a cap
      cannot shrink. On `List` the caps keep their D124 meaning (grow with
      the items, scroll past the cap). Wrap-capable text is `Label` and,
      since D135, a raw-register `RichTextLabel` (give it
      `fit_content=true`: its engine minimum counts no content otherwise,
      and a DETACHED RTL never shapes — engine law — so the shaped minimum
      lands the box on tree entry through the cap re-assert).
      - **The min/max band** (D135): `width=hug, min_width=100,
        max_width=300` clamps the natural size into the pair — the floor
        holds below it, the cap binds above it (wrap-text wraps there),
        and the floor beats the cap (the WPF clamp order above; a runtime
        `$param` floor crossing the cap wins). A literal floor above a
        literal cap refuses at load — "the pair can never both hold".
        Floors under `fill` stay legal everywhere (engine-native).
      - **`max_width=fill`** (and the height twin) caps at what a `fill`
        axis would GET: the parent frame minus region padding minus
        margins — live, so a parent resize re-caps. On a fill-capped axis
        margins bind the MEASURING FRAME exactly as they would inset fill,
        whatever the alignment — which legalizes margins on a centered axis
        there (asymmetric margins shift the midpoint, the padded-frame
        rule); without the fill cap the "centered box pins no edge" fence
        stands. The shipped-toast shape: `width=hug, max_width=fill,
        halign=center, margin_left=12, margin_right=12` — hug the text,
        center it, wrap at live width − 24. Wants a parent whose extent
        doesn't derive from this child (a hugging parent would feed the cap
        its own answer).
      - **The capped fill** (D245): `width=fill, max_width=380,
        halign=center` (`max_height=` on `height=fill` works the same) —
        the available space, never more than the cap (`size = max(floor, min(space, cap))`, same
        clamp order). Narrower frames fill edge to edge like plain `fill`;
        when the cap binds, the stated side places the box in the
        leftover. The side is REQUIRED — without a cap, alignment on a
        fill axis means nothing (and stays refused); with one, the
        leftover has to go somewhere and the file must say where. WPF's
        Stretch + `MaxWidth` silently centers here — a known trap the
        required side keeps out of gogo. Free parents only: a capped fill
        reads its parent's space and places itself in it, which a
        container child can't do — under a container, wrap it in a
        `Layers` (the load error says so). Margins inset the measuring
        space on every side choice, like plain fill. Not on a `Scroll`
        (a filling Scroll axis already scrolls past its frame), and
        `max_*=fill` on a fill axis refuses — a fill axis already takes
        exactly that space. The shipped shape this exists for:
        `VertWrapPanel MaxWidth="380"` — the effects catalog fills a
        phone and centers at 380 on anything wider, by pure width math
        (no per-class poses).
    - **`aspect=` — the ratio relation** (D133): the shape of the node's
      OWN rectangle — the space layout gives the node itself, not the
      Panel/Column holding it — as one number, width/height (`aspect=1`
      square, `aspect=1.78` 16:9), or `natural` — an `Image` bitmap's own
      ratio. One rule: **the axis you spelled drives, the axis you didn't
      is derived** (`width=fill, aspect=1.78` — height follows the live
      width). Both axes `fill` + aspect = the largest ratio rectangle
      that fits the space the parent offers, centered (WPF Uniform — the
      color-wheel shape). Both axes spelled is an error; `hug` and
      `max-*=` don't combine with it (v1). Numbers solve under free
      parents; `natural` uses the engine instead — the Image's MINIMUM
      follows the bitmap from the driving axis (the WPF UniformToFill
      measure), container or free parent alike, lazy textures included.
      Nothing spelled: width drives. An `Image` with no size word and
      no `aspect=` fills what its parent gives it and fits the picture
      inside; a Column gives it no height, so there (Rust app) it is the
      Column's width and the picture's shape sets the height (WPF
      Uniform in a vertical StackPanel) — never 0 tall. `hug` on an
      `Image` side (the written word, not an alignment) is the picture's
      own size on that side: `size=hug` (= `width=hug, height=hug`) = natural size, one
      picture pixel per layout unit (Rust app).
    - **`fit=contain/cover/distort`** (D133, `Image` only): how the bitmap
      maps into the Image's OWN rectangle (in `Column { Image width=96,
      height=48 }`, that 96x48 — never the Column). Orthogonal to how the
      rectangle is sized, so `aspect` and `fit` compose with no joint
      rules. `contain` letterboxes (the default), `cover` fills and clips,
      `distort` stretches — named for what it does; the CSS/WPF word
      `fill` is gogo's sizing word. Slack centers (v1, no alignment knob).
      The raw `stretch_mode=`/`expand_mode=` consts are retired spellings.
      On non-content nodes `fit=` is refused: children are laid out,
      never scaled.
  - **`visibility=visible/space_only/gone`** (D133) — the three-state
    triad, replacing the retired bool `visible=` (a bool can't spell
    three states; every variant named, the Android triple). `gone` hides
    and RELEASES the space; `space_only` hides but the node keeps holding
    its layout slot — toggling `visible <-> space_only` never moves a
    sibling (WPF Hidden). Hidden is hidden either way: hit-inert,
    unfocusable, a11y-skipped; `on:show`/`on:hide` track the PIXELS, so
    entering `visible` from either hidden mode plays the show. Space is a
    measured promise, so `space_only` binds under FREE parents only (an
    engine container skips hidden children in its own sorting and refuses
    the spelling, v1). Code-behind: `gogo.set_visibility(node, mode)`;
    plain `.visible` engine writes keep meaning visible/gone.
    - **Exact loses to content, loudly** (D126): a control's real box is its
      COMBINED minimum, so content that needs more (children, native text)
      beats the declared number — Godot never sizes a control under its
      minimum. The contradiction between the two authored facts is a build
      warning naming both (`height=100 but content needs 316`), the Flow
      overflow rule generalized: silently swallowed overflow is how parity
      bugs hide. Remedies the lint routes to: put the exact size on a
      Scroll around it, `hug` the axis, or shrink the content. Buttons
      instead CLAMP gogo children on an exact axis (WPF explicit-Width; see
      "Buttons size to their content") — their native text still warns.
    - **On `Scroll` the axes settle from three sources** (D138; D125's
      triad rule grown up): `allow=vertical|horizontal|both` names the pan
      axes outright and owns BOTH axes when present (listed axes scroll,
      the rest are off — the fill-width page that must not pan sideways).
      Absent, the triad infers: a `hug` axis is content-sized — nothing
      left to scroll — so it compiles to `SCROLL_MODE_DISABLED` and the
      engine counts content into that axis' minimum; numbers, `fill` and
      unkeyed axes scroll (AUTO). `bars=auto|never|always|reserve` paints
      the scrolling axes' bars (`never` = hidden but panning, the handheld
      look; `reserve` = auto visibility with the bar's space always held).
      A capped hug axis scrolls PAST its cap (D124 generalized): `height=
      hug, max_height=300` follows content to 300, then scrolls — the
      minimum stops at the cap, so engine containers grant it honestly
      (the D132 free-parent refusal excepts Scroll; `max_*=fill` refuses,
      `fill` already scrolls past the frame). Contradictions refuse at
      load naming both facts: `allow=` listing a bare hug axis, a capped
      axis left out of `allow=`, gogo words mixed with the raw props (one
      spelling per Scroll). Both axes hugged bare warns (nothing scrolls).
      Raw `*_scroll_mode=` props own their axis (escape hatches keep
      engine meaning — per-axis bar policy lives there). `Scroll
      height=100, width=hug` = WPF's default ScrollViewer: content-wide,
      exactly 100 tall, scrolls vertically. On `List`, `bars=` reaches the
      internal scroll (the same word one level up); the axis stays the
      layout's.
    - **An axis that does not pan holds its content to the viewport**
      (D109): on that axis the viewport is as hard as a written number,
      so `fill` content stays on screen and wrap text wraps there. What
      cannot bend (one line of text too long to wrap) is cut at the edge,
      alone. The Scroll also asks its parent for no more than it is
      offered on that axis, so the page never grows past the window
      because of one long line. WPF's ScrollViewer with that bar disabled
      reads the same. Example: in `Scroll allow=vertical { Column
      width=fill { Label text="...a very long line..."; Label wrap=true,
      width=fill } }` the Column is the Scroll's width, the paragraph
      wraps inside it, and only the long line is cut. Heights are measured
      without a budget, so on a sideways Scroll this holds when its height
      is a number or comes from one (`height=60`, `height=hug`, a box with
      a written height).
  - `margin`/`margin_left/top/right/bottom` (epx) — SIGNED edge adjustments
    that never pick the sizing mode; they carve the box the other keys
    produce: INSETS of a default/`fill` axis (the unnamed edge stays bound
    at inset 0 — the WPF Margin-under-Stretch reading), near-edge offsets of
    an aligned axis (far/centered edges error), PINS of an unaligned
    `hug`/exact axis (pin ± size; grow points away from the pin). The
    `margin=N` shorthand is breathing room: insets all around on filled
    axes, the touched edges under alignment. Two per-edge keys bind both
    edges (a declared size there over-constrains); document roots are
    exempt from the fill default, so a lone margin there still pins.
    `anchors=` and `offset_*` left the language — placement says it all
    (removed-key errors carry the migration hints).
  - Coherence is validated on the merged style+node+instance state:
    `halign` with `width=fill` errors (fill leaves nothing to position);
    placement never mixes with raw `size_flags_*` on one node (`inset=N`
    was retired — the fill default plus `margin=N` say the same thing;
    document roots spell the fill: `width=fill, height=fill`).
    Post-build, margins under a Container parent are load-time errors (they'd
    be silently dead). "Container" here means the kinds that seat children in
    slots — stacks, Flow, Grid, Tiles. A `Scroll` or a `List` is NOT one
    (decided 2026-08-30): each seats one pannable body, and margins on that
    body inset it like margins anywhere free. `padding` is legal on ANY
    container — air inside a Column or a Scroll is as sayable as air inside
    a Panel (the retired addon refused it there; recorded difference).
  - `Row`/`Column` are plain-English aliases for the H/V box containers.
    Stacks have NO default gap (D244): children touch unless the file says
    `spacing=` (or a one-off `gap`) — Godot's theme separation never leaks,
    the same law as the blank Panel slate. Flow and List rows follow it.
    `Layers` is script-backed (gogo_layers.gd) — free placement + z-stack:
    children overlap in declaration order and free-place themselves; it
    reports a min size (element-wise max of visible children's minimums
    plus their positive margins, D242)
    and, like all passive structure, is input-transparent (see Input —
    `interactive=true` declares it a gesture surface). Raw `Control` stays
    available for the unopinionated version. The full type surface — View,
    Flow, Panel, Scroll, Label, Input, Circle, Gradient, Radio, Check and
    friends — is the Vocabulary section below (and REFERENCE.md's table).
  - `gap <epx N | fill>` — the stack-space structure word (Row/Column only):
    a one-off break where `spacing=` is uniform. Lowercase = never named
    (capitalization law); it leaves an anonymous mouse-ignored Control
    behind (stacks make space with child slots) — being unnameable, that
    node is unobservable. No name, props or children; one size word; errors
    route wrong homes (Flow and Grid → spacing=, Layers → margins).
  - Safe-area insets are a VALUE, not a wrapper (D74): `padding_left=safe` /
    `margin_top=safe` read the host-fed inset for that edge
    (`gogo.set_safe_insets(l, t, r, b)`, px, re-applied live); `padding=safe`
    / `margin=safe` are defined sugar for the four per-edge keys, each edge
    its OWN inset. Only edge keys read it — `safe` on an edge-less key is an
    error routing there. Opt-in per edge: name only the edges the element
    reaches. On scroll CONTENT the inset scrolls (rows clip at the physical
    edge); framing the viewport it doesn't. `SafeArea` was retired — its
    opt-out blanket padded edges docked panels never meet; the error routes
    to the value.
  - **Buttons size to their content** (WPF parity; closed trap #3 — a Godot
    BaseButton ignores free children in its minimum, so `Button { Row {...} }`
    DRAWS right while the hit rect collapses to the stylebox). A button with
    gogo children derives per axis: `min = max(declared min, content min)`,
    content = the Layers formula (visible children's combined minimums plus
    their positive margins — declared empty space is space the parent
    reserves, so `margin_left=49` widens the derived minimum exactly like
    `padding_left=49`; D242). Negative margins mean "stick out past the
    edge on purpose": the Godot addon counts them as zero; the Rust port
    takes the hanging part OFF the claim, clamped at zero (`100 wide,
    margin_right=-30` claims 70; `150 tall, margin_bottom=-70` claims 80 —
    WPF/CSS do the same; decided 2026-08-30, a recorded difference the
    addon keeps). A margin that doesn't bind for the
    axis' alignment never counts (it is a load error anyway). An exact
    `width`/`height` clamps its axis like WPF's explicit Width;
    both-axes-exact buttons are exempt (nothing left for content to grow).
    Live: child min/visibility changes re-derive.
- **Widget body names are per-instance**: nodes named inside a widget body are
  visible to the widget's scoped anims/styles/states and NOT to the view's
  named map (a widget used twice would collide otherwise). The instance name
  itself lands in the view map as usual.

## Scoping for `$refs`

Innermost first: widget parameters shadow theme values. Undefined `$x` = resolver error
listing what *is* in scope.

Code reads theme values by a typed handle (2026-09-27): the build writes a
page's `$name`s as `enum Theme` (`Theme::Accent`), and
`Runtime::theme_color(page, Theme::Accent)` resolves it the way the file
does — active variant first, `$refs` followed. Hidden semantics made
visible: no hex copied into code, no color read off a node that happens to
wear the accent.

## Auto-wiring convention

A named node's events wire automatically to code-behind methods — one
mechanical rule, no hidden tense-shift (D181):
`name + event → "On" + PascalCase(name) + PascalCase(event)`
e.g. `saveButton` + `activate` → `OnSaveButtonActivate()`; `brushSize` +
`change` → `OnBrushSizeChange(value)`; `nameInput` + `edit` →
`OnNameInputEdit(text)`. Raw Godot signals follow the same rule
(`foo` + `drag_ended` → `OnFooDragEnded`), so one grep finds both sides
of a wire. Explicit `on:<event>=Method` overrides. The resolver verifies
the target method exists at load time and reports a missing method as an
error (a typo'd widget name must never silently unwire a handler).

Two D181 guards enforce the convention:

- **Shadowed raw names never auto-wire**: `pressed`, `toggled`,
  `changed`, `activated`, `value_changed`, `text_changed`,
  `text_submitted` are shadowed spellings of gogo events (see Input
  vocabulary) — the generic signal scan skips them, so one moment can
  never wire twice under two names.
- **Stale handlers are load-time errors**: a host method matching
  `On<Name>Pressed` / `Toggled` / `Changed` / `Activated` /
  `ValueChanged` / `TextChanged` / `TextSubmitted`, on a node that
  speaks (or spoke) that signal, errors naming the new spelling —
  after the rename a leftover `OnSaveButtonPressed()` would otherwise
  silently unwire, the exact failure the explicit-wiring check exists
  to prevent.

## Error contract (diagnostics, D68)

Every parser/resolver error is a structured **diagnostic**: severity, optional
stable code, message, one or more source **spans** (the first primary span is
THE location; secondary spans show the other half of two-place conflicts),
teaching `note:` lines, a `help:` fix summary, and — when the fix is
mechanical — behavior-applicable edits the editor offers as *Apply fix*.
When a closed set is known (widget types, style names, property names, theme
keys, parameters) the help carries a did-you-mean suggestion with runner-up
options; with no close candidate it lists the closest available options.

**Message voice** (D84): messages are lowercase sentences. Code fragments in
prose — property spellings (`` `areas=` ``), values (`` `fill` ``), type
words (`` `Panel` ``), keywords, whole example lines — wear `` `backticks` ``
(rustc's convention). What the USER typed stays in `'single quotes'`
(`unknown widget type 'Buttn'`): quotes quote, backticks say "type this".
Plain output keeps the literal backtick; BBCode output renders the fragment
as a `[code]` span (mono against the pane's proportional prose). An odd
backtick count renders literally — a rendered hint must never guess.

Two output formats, **frozen once published** (tools regex against them):

- one-liner (status bars, ui-lint, logs — GCC-shaped):
  `<path>:<line>[:<col>]: <severity>[<code>]: <message>`
- block (panes, terminal detail — rust-shaped, pure ASCII):
  line 1 `<severity>[<code>]: <message>`, line 2 `  --> <path>:<line>:<col>`,
  then per span a numbered source excerpt with `^^^` (primary) / `---`
  (secondary) underlines and a label, `= note:` lines, `help:` with the
  suggested source when edits exist. Further spans open with `:::`.

```
error[G3001]: `anim=` is not a state response — `when` holds values, it doesn't play
  --> res://ui/menu.gogo:3:37
   |
 3 |     when held background=#FFFFFFff, anim=test
   |                                     ^^^^^^^^^ belongs on a `transition`
   |
   = note: `when held` declares what the held state LOOKS like — standing values
           that stand while the state holds.
   = note: `anim=` says how a change FEELS — feel lives on `transition` lines.
help: move it to a transition into `held`
   |
 3 |     when held background=#FFFFFFff
 4 |     transition any -> held anim=test
   |
```

**Codes** (`G` + 4 digits) are stable search keys: assigned when an error
earns a teaching page (`docs/errors/<code>.md`, the editor's *Why?* pane),
never renumbered, never reused; most sites stay uncoded (`error:` plain).
Banding: G0xxx lex/parse · G1xxx placement · G2xxx props/values · G3xxx
styles/when/transition · G4xxx anims · G5xxx widgets/uses · G6xxx
theme/$refs · G7xxx view root/host. The registry (`gogo_diagnostics.gd`) and
the docs folder are test-enforced to match both ways.

**JSON channel** for tooling (`GogoError.to_dict()`; runtime push via
`Gogo.add_diagnostics_listener(cb)` — empty array = file clean again):

```json
{"code": "G3001", "severity": "error", "message": "...",
 "spans": [{"file": "res://ui/menu.gogo", "line": 3, "col": 40, "len": 9,
            "primary": true, "label": "belongs on a `transition`"}],
 "notes": ["..."],
 "help": {"message": "...", "applicability": "safe",
          "edits": [{"file": "...", "line": 3, "col": 38, "len": 11, "replace": ""}]}}
```

Edits are span replacements (`col` = 1-based, `len` chars replaced,
`replace` may contain `\n`; a whole-line replace is `col=1, len=<line len>`),
applied in reverse source order. Only `applicability: "safe"` fixes are
offered mechanically.

## States (`when`), arrangements, transitions

**The UI is a function of its states**: while a state holds, its
`when` responses overlay the declarations; when it ends, the base restores.
One concept at three scopes — responses live at the state's scope, dots
descend from that scope: app (`theme` variants over `$values`), screen
(`arrangement`s over props + placement), control (built-ins `held`,
`hovered`, `focused`, `focus_within`, `disabled`, `selected` over props —
respelled in the D181 input-vocabulary round, see Input vocabulary).
`when` is the ONLY response form; `transition` rules are the only timing
form.

```kdl
style GalleryItem background=$chrome {
    transition any -> selected duration=0.1, easing=cubic_out
    when held background=#FFFFFF22          // momentary, while the press is held
    when selected background=$accent        // persists while toggled on
    when disabled opacity=0.4
}
Button dangerRow .GalleryItem { when held background=#B00020 }  // node override
```

- `when <state> prop=..., ...` or `when <state> { prop = value ... }`.
  Responses layer over the base look and restore when the state ends.
  Merge order, most local wins: style base → style when → node props → node
  when override. A plain prop on the node never switches a style's response
  off — to opt one node out of a style's `when held opacity=0.5`, write the
  same state on the node: `when held opacity=1` (node beats style at equal
  rank). Opting out is written, never implied by an unrelated `opacity=1`.
- Priority when several states hold: arrangements < focus_within <
  focused < hovered < selected < held < disabled. Re-entry cancels the
  opposite tween and starts from current values.
- `focused` is VISIBLE-FOCUS (D129, semantics unchanged by the D181
  respelling) — focused AND an indicator is warranted: focus arrived by
  key/gamepad, or a key was pressed while focused; a
  pointer press anywhere turns the indicator off. A tap parks engine focus
  silently — Godot's click-focus never strands a ring (the D122
  press-residue class, generalized). Caret exception: a focused `Input`
  always indicates. One spelling, one meaning — there is no raw-focus
  state; code-behind reads `has_focus()`. This is the CSS :focus-visible
  model given the short name (a fresh language skips CSS's legacy pair).
  Buttons ship a default response: the engine's always-on focus ring is
  retired and re-shown as the injected default `when focused` (the theme's
  own ring); authoring `when focused` replaces it, per the D122 doctrine.
- `focus_within` (D129) holds while the focus is on the node or any
  DESCENDANT (CSS :focus_within, WPF IsKeyboardFocusWithin) — the
  container highlight for key/gamepad-driven UIs (a settings row lights
  while its inner slider is focused). Visible-focus flavored like
  `focused`; the one non-participle spelling, kept on purpose — it names
  a relation, not a condition of the node.
- `hovered`/`focused`/`focus_within`/`disabled`/`selected` are UNIVERSAL — legal on any node,
  dormant until a provider flips them (`disabled=` on a passive node is a
  styling flag: it greys via `when disabled`, `interactive=` stays the
  input switch; Slider/Input spell it as not-`editable`; `selected` is
  "this one is chosen" — a toggle-mode button holds its own, choice/Radios
  carry it, a `List` flips it on the chosen row's `ListItem` — D118).
  `held` alone needs a press surface — the Button family (templated
  Radio/Check and toggle mode included), a List row's `ListItem` (the
  List's press tracking flips it, D121), the native `Slider` (the
  grabber's drag brackets it), or a composite declaring `state_builtin
  held` (the ROOT's gesture tracking maintains it — behaviors never flip
  it; D181) — checked at the
  site a style is APPLIED, and the load-time error lists what's available.
  `selected`/`disabled` restyle by PUSH from their write paths
  (`GogoStates.set_selected`/`set_disabled`; a toggle button's clicks and
  `button_pressed =` writes arrive through `toggled`). Raw
  `SetPressedNoSignal`/`disabled = true` writes are caught by a shared
  0.1 s sweep — restyled a beat late, with a one-time `gogo:` warning
  naming the instant spelling, mirrored to an on-screen banner (release
  builds too; `gogo.set_raw_write_banner(false)` disables) — so nothing
  silently rots, and the old always-on per-control poll stays gone. A mouse
  press CAPTURES hover: slipping off the control mid-press keeps the
  hovered look until release.
- **`disabled` CASCADES** (D133 — the WPF IsEnabled law): effective =
  own flag OR any ancestor's own, and a child cannot re-enable under a
  disabled ancestor — no opt-out. Two layers: re-enabling the ancestor
  restores each descendant's OWN flag (a child disabled in its own right
  stays disabled); children arriving inside a disabled subtree (repeat/
  List rebuilds, code-behind adds) inherit on entry. `when disabled`
  fires on the EFFECTIVE value, so gray-out styling cascades with it.
  `disabled` is the only state that flows down the tree (`focus_within`
  is the only one that looks up). `explain()` names the source:
  `disabled: yes (inherited from 'settingsPanel')`.
- `ListItem` ships DEFAULT `selected`/`focused`/`held` responses (D122,
  defined sugar — doctrine unchanged by the D181 respelling): a bare row
  fills with the theme's list-selection color
  when chosen, a quarter-strength echo under the cursor, a half-strength
  fill while the press is held — Button doctrine (the theme is the base
  look, `when`
  overrides). An authored `when <state>` on the row replaces that state's
  default entirely, per state; `select=off` rows skip only the selected
  default. `hovered` stays opt-in. Two feel rules (D122 follow-ups): the
  default press shade mirrors "release here would tap" (slide-off clears,
  slide-back re-arms), and the cursor echo draws only for key-driven focus
  — since D129 that IS the `focused` state's one meaning (visible-focus),
  so authored `when focused` behaves identically and the row-local
  `kb_only` gate is gone.
- Friendly response props: `opacity` (modulate.a), `background` (shared
  StyleBoxFlat — tweenable), `font_color` (all theme slots); dotted engine
  paths (`modulate.a`) and plain engine props (a `Rectangle`'s/`Circle`'s own
  `color`) also work. `color` on a node without one is an error routed to
  `font_color=`/`background=`/`opacity=` — never a silent modulate tint
  (known dialect words `foreground`/`text_color` route the same way).
  Bool/enum values apply instantly; floats and colors tween.
- Motion words are responses too (standing values on the motion channel):
  `when held scale=0.95` holds while the state holds and hands back
  when it ends, timed by the scope's `transition` rules. Forms: `scale=`
  (uniform, centered), `scale.x=`/`scale.y=`, `translate.x=`/`translate.y=`
  (epx), `rotate=` (degrees); `translate=` without an axis is an error.
  `scale=` scales the whole picture: the glyphs, corner radii and border
  widths inside the box shrink with it, and nothing around the box moves —
  a picture zoom (WPF's RenderTransform), never a layout resize. The
  runtime arms the offset transform while a state holds it off identity
  and releases it on landing back (the anim player's disarm rule). ONE
  WRITER per property: a transition `anim=` in the same scope driving a
  property a `when` also holds is a load-time error. Static (base-look)
  motion words stay errors — declarations carry no transform.
- Timing keys on a `when` line are an error: how a change feels is a
  `transition`. The retired spellings (`state` directive, bare `pressed`
  blocks, `enter`/`exit` blocks) error with their replacements.
- A style may respond to a declared arrangement (`when expanded ...`): every
  wearer follows the VIEW's flip, with the view's transition timing.

### Arrangements

`arrangement <name>` (top level) declares one configuration of the view's
EXCLUSIVE set — its enum; `base` is predeclared (the declarations themselves)
and `any` is the transition wildcard (both reserved). Declaration only:
responses live in top-level `when <name>` blocks (dotted `node.prop=value`
pairs from the view root), timing in `transition` rules. Placement keys
(`margin-*`, `width`, `height`, `min_width`, `min_height`) compile per
arrangement into the target's placement (load-time validated); other props
go through the state machinery.

"Pose" in this spec is defined shorthand, not a keyword: the set of values
one `when` condition holds — an arrangement's, a screen class's, or a
composed pair's. A rule stated for "poses" applies to all three condition
kinds; rules that differ between them name the kind.

A `when` works the same wherever a `when` is legal (decided 2026-09-01):
any condition, composed pairs included, may sit at the top of the file
(responses address nodes: `bar.opacity = 0.5`) or inside a node's body
(responses are the node's own words). Adding a screen word to a node's
condition never forces the block to move, and a style's blocks — which
attach to nodes — compose like any other. Two blocks on one condition
writing one property are refused wherever the two sit, so a condition's
writers stay auditable. (The retired Godot addon took composed pairs at
view level only — a recorded difference.)

Size keys in a pose (D249): a `width`/`height`/`min_width`/`min_height`
written in a `when` block behaves exactly as if the file had declared it,
for as long as the block applies — when the block stops applying, the
declared value comes back. That includes the minimum size the box reports
to the engine, so a pose CAN shrink a node below its declared size (before
D249 such a write moved only the anchors and the declared minimum silently
kept the box at the old size). The values compose like declarations too: a
posed `width` still loses to a declared `min_width` floor (the floor
clamps every posed value — WPF's MinWidth-over-Width order), a posed
`min_width`/`min_height` replaces the declared floor, and content still
sets a button's non-exact axis above any floor. On a TIMED flip the box's
edges tween; its minimum drops at the start of the flip and raises when
the tween lands, so motion is smooth in both directions. A declared
`min_width` next to an exact `width` on the same axis is always legal
(GPAR79 2026-08-31 dropped the dead-pair refusal): the floor is there to
bound the posed values, and it clamps the declared size by the same
order.
Flipped with `gogo.set_arrangement(root, name)`, `on:<signal>=go(name)`, or
`on:<signal>=toggle(name)` — toggle is one comparison of sugar (D92):
`go(active == name ? base : name)`, the disclosure button without a handler
(from a DIFFERENT pose it goes to `name`, checkbox-style). `toggle(base)` is
a load-time error (base is home — `go(base)` returns there); pairing
`toggle=true` (button toggle MODE) with `toggle(pose)` draws an advisory
lint — two separate on/off states that drift once anything else flips the
pose. The verb set is closed: `play`/`go`/`toggle`/`emit` (cycling through
poses stays code-behind).
The flip tweens with the matching rule; the ACTIVE pose re-asserts on every
`units_changed` and hot reload (re-assertable by construction — the
two-writers cure), and hot reload keeps the active arrangement.

### Screen classes and orientation (`when screen.<class>` M15, `when screen.<orientation>` D265)

Two more pose axes, orthogonal to arrangements: the SCREEN's size class
and its orientation. The host app names the active values
(`gogo.set_screen_class`, `gogo.set_screen_orientation`, typically from
its resize handler); `when screen.landscapeSmall { node.prop=... }` and
`when screen.landscape { node.prop=... }` blocks respond. Both
vocabularies are CLOSED — classes `portraitSmall` / `portraitMedium` /
`portraitLarge` / `landscapeSmall` / `lotsOfSpace`; orientations
`portrait` / `landscape` (landscape = wider than tall) — and need no
declaration line: they exist whether or not a file poses for them. The
two say different things: the class is how much room there is, the
orientation is which way it runs — a layout that means "is it sideways?"
keys the orientation, because enumerating the landscape-ish classes
silently catches big PORTRAIT tablets (`lotsOfSpace` covers both shapes).

One vocabulary per target (G3005): a class and an orientation hold AT THE
SAME TIME, so one property, one node's placement, or one seat move posed
from BOTH would be decided by a rank the file never states — load error;
the composed `when screen.<orientation> screen.<class>` says the
intersection as one condition. Arrangements are exempt (a user pose, not
a screen fact): their contests keep the ladder below.

Rules that differ from arrangements: flips are INSTANT (a rotation is the
OS snapping the window — no transition rules), the axes are app-wide (one
screen; one class, one orientation, every view), and against an
arrangement they rank EQUAL (one condition each) — where both re-pin one
node, each contested key goes to the block written LATER in the file and
the other pose keeps the rest, so leaving an arrangement lands back on
the screen's seat, not on base. Placement keys are the arrangement set plus
`halign`/`valign`: an alignment key RE-DECLARES that axis (base pins and
size mode drop; the pose's own margins/size stand alone) — alignment
needs an instant change (G3004): always true here, and true of an
arrangement `when` exactly while no `transition` rule times the
arrangement. Container children take SIZE poses only: numeric
`width`/`height`/`min_width`/`min_height` behave exactly as if declared
for as long as the condition applies (D249 — a posed `width` makes a fill
child stop taking the leftover; the drop restores the declared axis,
instant — nothing to tween), while margins/alignment on container
children stay load errors (the parent seats its children). Same
re-assert on `units_changed`/hot reload. View-level blocks only.
The editor panel's Layout dropdown (top toolbar) previews a class without
a device: it names the app-wide class for the preview build, and with the
Phone frame on, the frame takes a representative shape for the class.

Composed blocks hold only while BOTH their conditions do, for poses
neither axis owns alone. Three shapes: `when <arrangement>
screen.<class>`, `when <arrangement> screen.<orientation>` (arrangement
first — a reversed spelling is a load error naming the fix), and
`when screen.<orientation> screen.<class>` (orientation first, class
second — the sanctioned intersection of the two screen axes). One rule
decides every contested key, and it reads off the file: **a block that
needs more conditions wins** (a pair beats any single), and **at equal
count the later line in the file wins** (CSS's rule — put the block you
want to win lower). So a pair's keys beat its halves on a contested
property, two active pairs settle by file order, and either half dropping
releases the block back to whichever single-axis pose still holds.
Alignment keys need
an instant change (G3004): legal in a pair (or a plain arrangement
`when`) exactly while the arrangement has NO `transition` rule — with no
rule the change is instant, so there is no animation for the re-anchor to
break; the screen-first pair has no arrangement half and is always
instant. View-level blocks only, and a named arrangement must be declared
in the file.

Seat moves — the one `when` response that REPARENTS. `seat <name>`
declares an empty socket in the tree (lowercase like `slot`/`gap`:
sockets and spacers, never painted controls); `x.seat = <name>` in a
`when` moves the declared node into it, instantly, and
dropping every condition returns it to its declared home — parent AND
position. Legal from every `when` whose change nothing can animate:
screen whens, composed blocks, and arrangement whens while no
`transition` rule times the arrangement — a timed arrangement carrying a
seat move is a load error pointing at both lines (G3003: a node is in
one parent or the other, never partway). Contested seats follow the
placement rule: more conditions win (a composed pair over a single),
equal count → the later line in the file.
`x.seat = home` says the return AS a move: sit at the declared
home while THIS condition holds — for the more specific block that must
pull a node back out of where a less specific one seated it (`home` is
reserved; `seat home` as a declaration is a load error). The socket hugs
to zero alone, to the occupant's minimum when
occupied (occupant fitted to the seat's full rect). That minimum reads
wrap-text by the occupant's own DECLARED width mode: a `width=fill` or
exact-width (`width=N`) occupant measures ORDINARY on both axes (wrapped
— fill asks the socket for size, a number IS the box, there is no hug to
honor), while a hug occupant measures wrap-text UNWRAPPED like every hug
axis (D254 — the crop hint's landscape row). A posed width counts as an
exact width for as long as its block applies. A hug occupant whose
single-line text is wider than the window is the width-feedback trap —
the socket can never fit any pane — and warns at runtime (G1005, the fix
is `width=fill` on the occupant). The rules are all
load errors when broken: seats are empty and take a name only; seats and
resting homes are containers (free-placed nodes re-pin with placement
`when` blocks instead); one occupant per seat under conditions that can
hold together (two different arrangements are exclusive and may share a
seat; an arrangement claim and a screen-class claim can co-hold and may
not); no moves into a node's own subtree.

The smooth-slide primitive: the node's LAYOUT SPACE as a 0..1 fraction.
`visible` is a bool and bools snap; making the space a float puts it on the
same rails as every other response — poses flip it, `transition` rules tween
it, anims may drive it (the one layout channel an anim may own, because the
reveal proxy is its only writer). Declaring either factor wraps the node in
proxy → clip window → node (gogo_reveal.gd): the PROXY reports
`child minimum × factor` to the parent and mirrors the node's size flags
out (so `halign`/`fill` are unchanged); the WINDOW clips to `factor × the
extent the node would otherwise have` — natural size when hugged, the
stretched extent when filled. The clip is deliberately NOT the proxy's
granted rect: containers stretch children on the cross axis (a Column
grants its full width), so a grant-sized clip reveals everything the moment
an ancestor authors extra space (the D93 postmortem). The rules:

- Content pins to the LEADING edge (top/left); the trailing edge clips —
  drawing and hits both (`clip_contents` feeds Godot's input clipping).
- `reveal_y=0` is not `visibility=gone`: a concealed node stays ALIVE (anims
  run, states poll), it just earns no space. `visibility=gone` remains the
  "gone" switch and zeroes the space instantly.
- The channel exists only where DECLARED: a `when` or anim track driving
  `x.reveal_y` without `reveal_y=` on `x`'s own line is a load-time error
  (the resting value is visible in the file).
- Container parents only (`Column`/`Row`/`Grid`/`Panel`/`Scroll`/`Flow`);
  the root and free parents (`Layers`, plain `Control`) refuse it at load
  time — nothing reflows around minimums there, so the honest tools are
  `opacity`/`translate`. Widget instance lines can't add it (v1); it goes on
  the node inside the widget body.
- `fill` children: the factor scales the EXTENT — a label stretched to a
  100-wide column at factor 0.5 shows its first 50 stretched pixels, and
  the minimum contribution to the parent scales the same way.

### Transitions

`transition [A -> B] duration=, easing= (+params), anim=` (state changes)
and `transition node.prop[, …] duration=, easing=` (property changes, below)
— a directive line
(top level for arrangements; inside a style/node block for built-ins), never
a block. Bare = the default for every change (`any -> any`); `any` is the
one-sided wildcard. Specificity: exact pair > entering (`any -> X`) >
leaving (`X -> any`) > bare; duplicate endpoints in one scope = load-time
error. Going home with no rule of its own mirrors the entering rule (timing,
never its anim). At control scope exactly one side is `any` (built-ins
stack) — EXCEPT an exact POSE pair (both endpoints arrangements, `base`
included, 2026-08-16): that times THIS node's share of the flip, overriding
the view rule (a plate sliding slower than the tiles it backs;
`duration=0` = this node snaps to its new seat while the rest glides).
Node pose rules time only; `anim=` and one-sided arrangement endpoints
stay at the view's top level. Untimed = snap — so scoping pair rules to
one pose pair makes OTHER flips instant declaratively (an on/off switch
whose Off entry seats instantly needs no code flag). Pose tweens always
run current value -> new seat, so a mid-flight re-flip retargets from
wherever the node visually is (WPF To-animation feel).

**Property transitions** (2026-09-27) — the same keyword with property
targets instead of an arrow: `transition commandBar.opacity duration=0.4,
easing=expo_in_out`. Top level only, naming the node; a `.{…}` group names
several properties of one node (`commandBar.{margin_left, margin_top,
opacity}`; `margin` / `padding` = the four edges), and a comma lists more
targets. The rule belongs to the PROPERTY: any change of its standing value
slides there from what shows — a code write (`Runtime::set_prop`), a
`when` response, a pose — and a change mid-slide turns around from where
the slide is. Snaps instead: the first value, a node that doesn't show,
`Runtime::set_prop_instant` (a `set_prop` right after it slides from the
instant value — fade-in = instant 0, then 1), and the responses of a
`go_instant` flip. `visibility` never slides (G3007: fade `opacity`). One
timing per property (G3008): a second property rule, or a timed `A -> B`
rule over a `when` that sets the property, is a load error naming both
lines. A slide counts as motion for the frame loop's idle gate.

A rule's endpoint alone SUBSCRIBES the state (D182): `transition any ->
held anim=nudge` on a Button wires the press with no standing `when held`
response — the enter plays the anim, the exit reverts what it displaced.
Naming the flip is interest in the flip; before D182 the rule validated
and then silently never fired. Endpoints carry the same availability gate
as `when` (`held` needs a press surface), and retired spellings redirect.

## Input vocabulary (D181 — designed and built 2026-07-26)

The one-round respelling of interaction states and events. States are
past participles (`held`, `hovered`, `focused`, `selected`, `disabled`;
`focus_within` is the documented non-participle — it names a relation);
gogo events are bare stems, effect-named, input-agnostic. The slot
carries the axis: `when <state>` is a continuous condition, `on:<event>`
a discrete moment. Full rationale, per-control tables and rejected
words: `docs/design/2026-07-26-input-vocabulary-spec.md`.

**The gogo events** — bare stems on `on:` (the existing `show`/`hide`
pair already conformed):

| event | fires | payload |
|---|---|---|
| `activate` | the control's OWN trigger gesture completed — Button: a completed tap (slide-off clears, slide-back re-arms) or key/gamepad accept; List: tap or Enter/Space on a row. Never "fires on release" as a universal law; never fires disabled | — (List: the row's key + item) |
| `change` | the committed value is now different — once per commit, genuine transitions only ("changed means CHANGED": re-pick repairs, never re-reports; programmatic writes report like interaction; a same-value write is no event, which breaks handler→write→event loops) | the new value: Check/Toggle `bool`, Slider `float`, Input `String`, `choice` the picked Radio's name, List the key / Array of keys / `null` |
| `edit` | a live, UNCOMMITTED value moved — Slider per drag tick, Input per keystroke; discrete controls have no edit | the live value |
| `press` / `release` | the finger landed on a press surface, and that gesture ended — `release` fires exactly once however it ends (lift inside or out, cancel, a Scroll steal, disable/hide under the finger). A Slider brackets its drag with the same pair: `press` on down, `release` on up or cancel, one each (the gogo spelling of Godot's HSlider `drag_started`/`drag_ended`). `activate` stays a press surface's completed tap; a Slider never fires it | — |
| `show` / `hide` | visibility choreography (unchanged) | — |

Per control: **Slider** brackets its drag with `press`/`release`
(exactly one each, cancel included), streams `edit` while dragging
(`held`) and commits `change` on release; a keyboard/gamepad step commits directly —
`change` only, no `edit` (a step and a programmatic write are
indistinguishable at the engine seam, and an `edit` landing in the same
instant as its commit adds nothing). **Input** streams `edit` per
keystroke; `change` commits on Enter or focus loss; a programmatic
`text=` is synthesized (Godot emits nothing on set). **Toggle**:
`selected` flips LIVE as the thumb crosses the midpoint (state =
presentation); `change(bool)` fires on settle, only when the settled
side differs from the pre-gesture side — wiggling across the middle is
zero changes. **choice**: `change` lives on the GROUP (payload = the
picked Radio's name; items refuse `on:change`). **List**:
`change(selected)` / `activate(key, item)`; `select=off` refuses
`change` writes, activation still fires.

**The register law** — `on:` stays OPEN: raw Godot signals remain legal
for anything gogo has no word for (`on:drag_ended`, `on:resized`,
`on:mouse_entered`, `on:focus_entered`, `on:button_down`/`on:button_up`,
...). The register a line speaks is visible at a glance: bare stem =
gogo word, snake_case past tense = raw signal. Raw spellings that
duplicate a gogo concept are SHADOWED with a one-keystroke redirect (the
D83 duplicate-raw law applied to signals):

| raw spelling | redirects to |
|---|---|
| `on:pressed` | `on:activate` — or the raw `on:button_down`/`on:button_up` |
| `on:toggled` | `on:change` |
| `on:changed` / `on:changing` | `on:change` (committed) / `on:edit` (live) |
| `on:activated` | `on:activate` (retired gogo spelling) |
| `on:value_changed` | `on:edit` (live) / `on:change` (committed) |
| `on:text_changed` | `on:edit` |
| `on:text_submitted` | `on:change` |
| `on:input` | `on:edit` (this is DOM's `input`) |

Shadowed names also never auto-wire, and stale `On<Name><Retired>` host
methods are load-time errors — see Auto-wiring convention.

**Retired state spellings** (D83-style, one-keystroke redirects):
`pressed`→`held`, `hover`→`hovered`, `focus`→`focused`,
`checked`→`selected`, `dragging`→`held` (the composite Slider/Toggle
drag state — `held` IS the drag there; no separate word). Code surface
renames come along: `GogoStates.set_selected`/`is_selected` (were
`set_checked`/`is_checked`), Toggle's root property `selected` and
signal `change(bool)` (were `checked`/`toggled`), List's
`change`/`activate` signals (were `changed`/`activated`),
`gogo.stranded_conditions` defaults `["held", "hovered"]`.

## Animations

`anim <name> { }` is a top-level directive (imported via `uses`, duplicate =
error). One model (D136): an anim is a CHAIN of steps — property lines,
`group { }` blocks, `play <anim>` steps, `each ... { }` fan-outs and
`delay=` pauses — and every step after the first says when it runs: `and`
joins the previous step (they run together), `then` starts after the
previous step and everything `and`-joined to it, or `after=<group>` places
it by reference instead. Sequencing is always written, never defaulted (a
second step with no connective is an error; deleting a connective is a
compile error, not a silent meaning change). Widgets may declare scoped
`anim`/`style` directives alongside their single body root — they see the
widget's per-instance node names and are invisible outside; `self` targets
the node a style is applied to.

```kdl
anim appear duration_each=0.5, easing=cubic_out {  // one and-cluster;
    listBrush.translate.x from=-60, to=0           // duration_each= is each
    and panelMiddle.opacity from=0, to=1           // step's DEFAULT (a
    and imgArrow.rotate {                          // line's own wins)
        key t=0, v=14, jump=true
        key t=0.8, v=0, easing=cubic_out
    }
}

anim openMenu {                                // groups: share step words +
    group dim { backdrop.opacity to=0.6, duration=0.15 }  // carry the name
    then group drop duration_each=0.25 {       // after= references
        menu.translate.y from=-40, to=0, easing=back_out
        and menu.opacity from=0, to=1
    }
}

anim entrance duration_each=0.2 {              // each: wearers + stagger
    each .MenuButton stagger_by=0.1 {
        translate.y from=40, to=0, easing=back_out
        and opacity from=0,  to=1
        then scale.x to=1.05, duration=0.08, autoreverse=true
    }
}
```

- **Motion channel** ("layout is declared, motion is transformed"): the words
  `translate.x/.y` (epx), `scale` (uniform or `.x`/`.y`, center-pivoted),
  `rotate` (degrees) and `opacity` compile to Godot 4.7 offset transforms
  (visual-only; `opacity` = modulate.a and doubles as a static prop). The
  player owns `offset_transform_enabled` — armed while playing, released at
  identity or hidden (a parked-enabled transform renders a black backing
  rect). Layout geometry (`position`, `offset_*`, sizes, `rotation`) is not
  animatable, and raw `offset_transform_*` props are fenced everywhere.
  FanRust (2026-09-02; the addon predates it): `pivot=` — a 0..1 box
  point (`pivot=0x0`, the Gradient `from=` spelling) — places the fixed
  point `scale` and `rotate` transform about; unset = the center, the
  old behavior verbatim. A look word on every box (`when` responses may
  restyle it), not itself animatable; a full 0..1 pair or the load
  refuses. Hits follow every channel there, rotation included — the
  finger un-rotates about the nearest rotating ancestor-or-self, so
  look and touch agree.

- **Prop lanes and the color-mixing law** (FanRust, 2026-09-02; not in
  the addon): any word a `transition` rule can move is an anim target
  too — `b.background to=#FF3B30FF`, `card.corner_radius from=4,
  to=12`, the live `stop_N_color`/`stop_N_offset` family — and `.a` on
  any color word moves alpha alone while the hue keeps following its
  ladder. ONE ownership law with the motion channels: a playing anim
  owns its (node, property) lane — `when` and code writes yield, a
  `transition` on an owned lane spawns no tween, a newer player takes
  the lane over (SnapshotAndReplace), `stop` restores the pre-play
  value, a finished anim holds its end pose, and a held pose yields to
  a `when` that writes the key. `key` children stay numeric — a color
  lane spells its segments as chained `then` steps. And AUTHORED colors
  mix in premultiplied Oklab everywhere — gradient stops, `transition`
  tweens and anim lanes, one implementation: premultiplying keeps a
  fade to transparent clean (no dark fringe from a transparent color's
  rgb), Oklab keeps cross-hue midpoints from going muddy (CSS
  `color-mix()` made the same default). The addon lerped per byte — a
  recorded divergence.

- The anim header: `repeat=forever`, `speed=`, plus the honestly-named
  distributing defaults `duration_each=`, `easing=` (+ params),
  `autoreverse=`. **`duration_each` is per-step by NAME** — plain
  `duration=` on an anim or group header would read as the container's
  TOTAL and a `then`-chain would make that reading false, so it refuses
  with a teaching error (the D136 clincher, resolved by renaming the word
  rather than banning the position). Property lines carry the journey —
  `from=`/`to=`/`easing=` or `key` children — plus their own `duration=`,
  `autoreverse=` and the signed `offset=` shift; each word resolves from
  the NEAREST enclosing scope that sets it (line, then groups at any
  depth, then the header). A line with `key` children can't also take
  `duration=` (keys own the timeline).
- **The connective law**: the first step stands alone; every later step
  carries `and`/`then` — or `after=<group>` INSTEAD (one placement per
  step; a connective plus a reference is an error, never a precedence
  rule). `then` waits for every end in the previous `and`-cluster,
  autoreverse's way back included. `after=` is the crossing-edge escape
  hatch: only named `group`s can be referenced, names are anim-flat
  (forward references legal, wrappers reachable-into), unknown names and
  cycles refuse at load. `delay=N` is a PAUSE step, always positive; a
  shift is `offset=` (signed; negative overlaps, the final start clamps
  at 0). `play <anim>` inlines another anim as one step at bake (speed
  folded in, internal names private, `repeat=forever` refused — it never
  completes). The old `track` block and the delay ATTRIBUTE retired with
  this grammar (big bang, no legacy machinery — the addon is pre-release).
- `each <node|.Style> [stagger_by=N]` names wearers (a `.Style` = every
  wearer under the play surface, tree order; `stagger_by` shifts
  successive copies); its body is a step chain in BARE paths (the each
  names the target — prefixing is a routed error; dotted paths belong at
  anim/group level). With `stagger_by` > 0 the each's length only exists
  at play time, so nothing may sequence after it — refused, not guessed
  (`and` beside it is fine: that shares its start).
- **Capture rule**: `to=` alone starts from the LIVE value when the step
  begins; `from=` (or a key at t=0) pins the start and HOLDS it from play
  start (staggered entrances never flash their base). Two pinned steps on
  one property = load-time error. **`from=current`** pins the live value
  read at PLAY start (the `to=`-alone capture, made spellable and pinned —
  it holds through a delay window where an open start would flash the
  base). `current` is `from=`-only — `to=current` is refused (an open
  start already reads live). Pose-flip motion is NOT an anim's job: an
  arrangement flip glides via `transition` pair rules (per-node timing
  included), which retarget from live values by construction.
- **Handoff law (WPF Begin, SnapshotAndReplace)**: a new play OWNS every
  property it animates. Other players on the same view still IN FLIGHT stop
  writing those properties (their remaining tracks and their `finished`
  clocks run on), and the new track inherits the old one's restore base —
  so `stop` after a re-play chain restores the true pre-anim value, never a
  mid-flight snapshot. A FINISHED player's held end pose is a standing
  value the author chose: a new play captures it as its base like any other
  live value.
- **Code writes outrank every `when`** (decided 2026-08-30): a value the
  host writes stands for as long as it stands — no `when` overrides it or
  hands it back on exit. The standing order is: host writes, then `when`
  responses (the ladder below), then declarations. Taking a property back
  from code is a code act (clear the write), not a state flip.
- **One property, one writer** (D63, made whole 2026-08-31 in the Rust
  port): a declaration, a `when`, a code write and an anim all write the
  SAME channel of a property — `opacity` is one number, an anim on it
  assigns, it never multiplies the declared value; `x.color.a` is the
  fill's alpha and leaves the node's opacity alone. Who wins: while an anim
  plays it is the top writer for the channels it animates; when it ends
  its end pose stands (above) until the next writer of that property — a
  `when` flip that writes it, a code write, another anim — or
  `stop(restore)`, which puts back what the ladder says without the anim
  (a held end pose the play started from comes back as that base). A
  hidden node holds nothing: hiding a node drops the held poses of
  everything drawn under it, and an anim that ends while its target or a
  parent is hidden holds nothing, so the next show starts at rest. The
  one conflict the compiler can see — a `when` in a scope holding a motion
  word while a `transition … anim=` in the same scope drives that channel
  on the same target — is refused at load (G4003: "two writers would fight
  on every flip; the loser flickers"). `modulate.a` is a Godot engine
  word: the addon still reads it as `opacity`; the Rust port refuses it
  naming `opacity` (G0075).
- Key line: `t=`/`v=`/`easing=`/`jump=true` (snap, no interpolation).
  `duration=0` sets instantly. A keyframed line's duration IS its last key's
  `t`. Numbers on length properties are epx (`px` opts out).
- **Easing names** (WPF-exact math; Godot TransitionType is never used):
  `linear`, `sine`, `quad`, `cubic`, `quart`, `quint`, `circle`, `expo`
  (`exponent=`), `power` (`power=`), `back` (`amplitude=`), `bounce`
  (`bounces=`, `bounciness=`), `elastic` (`oscillations=`, `springiness=`)
  × `_in` / `_out` / `_in_out`. A bare base name means ease-out.
- Runtime: `gogo.play(view, "appear")` (returns the player node; emits
  `finished`), `gogo.stop(view, "appear")` (restores pre-play values, like
  WPF Storyboard.Stop), `gogo.is_playing(...)`. GogoView exposes
  `play_anim`/`stop_anim`/`is_anim_playing` (C#: `PlayAnim`/`PlayAnimAsync`/
  `StopAnim`/`IsAnimPlaying`; arrangements: `SetArrangement`/
  `SetArrangementAsync`/`CurrentArrangement`). `gogo.set_motion_speed(x)`
  scales all motion — fractions
  slow-mo, `INF` instant-complete (finishes, never skips; forever-anims hold
  their base pose).
- Declarative triggers: `on:show=play(appear)`, `on:hide=play(disappear)` —
  `on:hide` plays first and hides on finish (deferred hide), then restores the
  base look; showing during a hide cancels it. A control that lands on screen
  already visible (first build, hot reload) plays its `on:show` once as an
  ENTRANCE; initial `visibility=gone` stays state, not a trigger (the show
  plays on the flip). `gogo.set_entrance_anims(false)` suppresses entrances
  (the editor preview plays them by default since D90; its "Entrance anims"
  toolbar toggle opts out, remembered per project). Any real signal also takes
  `on:<signal>=play(x)`, `on:<signal>=go(arrangementName)` (flips the
  view's arrangement; `go(base)` returns home) or `on:<signal>=toggle(name)`
  (flips between the pose and base, D92). Caveat: during a deferred
  hide the control is still technically visible, so a raw `visible = true`
  write in that window fires no signal — interrupt a running hide from code
  via `play` instead.

## Grid (D82 — designed 2026-07-06, built 2026-07-10)

WPF-grade tracks and spans with none of WPF's coordinates. `spacing=` puts
even gutters between the tracks — the same word Column/Row/Flow use
(decided 2026-08-30). Without `spacing=`, the way to get gutters is to make
them real columns — spacer tracks — and a three-column grid with 8-epx
gutters becomes almost double in columns/rows, plus a `skip` for every
gutter on every row. One Grid uses
exactly ONE of two placement forms (never mixed — errors route between them,
like anim's flat ↔ structured):

**Row form** — placement IS structure:

```kdl
Grid cols=[80, hug, fill(2)] {         // tracks: the sizing triad, as a LIST
    row height=80 { Label a; Label b; Button c }
    row { Image preview col_span=3 }   // heights travel WITH their row block
}
```

**Areas form** — placement by NAME (CSS grid-template-areas; one quoted
string per row, CSS's own spelling, because gogo strings are single-line):

```kdl
Grid cols=[hug, fill],
     areas=["header header : 60",
            "nav    body   : fill",    // `: height` goes with its row
            "footer footer : 40"] {    // `.` marks an empty region
    Panel header { ... }               // children bind by node name
    Column nav { ... }
}
```

- **`cols=` is the signature**: every row (and areas line) is arity-checked
  against it the way a call is checked against a fn declaration; errors
  speak in tracks. It exists only because columns have no structural home.
- **`row { }` blocks** — lowercase structural keyword (the `slot` register:
  case separates structure from controls, so `Row` the stack stays a TYPE).
  Rows are NOT nodes: cells parent flat under the Grid (zero-cost — anim
  paths and names unaffected); no style/background/name on a row
  (zebra-stripe via a spanning Rectangle cell). Only `height=` goes on a row.
- **Reordering is the design driver**: moving a UI row = cut/paste one
  block. Hence NO `rows=` list — parent-level row specs are position-coupled
  data that desyncs on reorder; a row's `height=` (triad, default hug;
  grid-level `row_height=` default for uniform grids) lives on the block and
  moves with it. In the areas form the same principle is the `: height`
  suffix riding each line — REQUIRED there (D89): a defaulted height hid
  "why doesn't my grid fill?" bugs, and `row_height=` does not reach the
  areas form.
- **`skip` spells every hole**: interior empty tracks, and each track a
  `row_span` covers in the rows below (the intruder OWNS them; the
  bookkeeping stays visible where it applies — no CSS-style silent flowing
  around invisible occupants). Trailing tracks may simply be omitted —
  nothing shifts. In areas, holes are `.`.
- **Spans are plain cell props** (`col_span=2`, `row_span=2`) — no
  coordinates, ever. A span past the last column/row is a load error (WPF
  silently clamps; gogo treats it as a miscount). **Grids are partitions**:
  overlapping spans are unrepresentable — overlays are `Layers`' job, and
  every shipped WPF "span everything from (0,0)" case reads better that way
  (shipped census 2026-07-10: of 229 `<Grid>`s only 23 files have real
  definitions; of 22 row-spans, essentially none survives as a true span —
  most are the missing-Layers overlay idiom).
- **One sizing vocabulary, third appearance**: track entries and row heights
  are the same triad as `width`/`height` (`hug` ≡ WPF `auto` — redirected).
  `fill(n)` weights split the leftover space and take fractions (shipped
  stars include `0.4*`/`10*` and CropFramePhone's golden-ratio
  `1.6180339887*`). Weights live in tracks/rows ONLY: on a grid cell
  `width=fill` means "fill my cell" and a `fill(n)` weight there is an error
  routing to the track. Cells place inside their cell with the one placement
  vocabulary (halign/valign; the default fills the cell).
- **Areas rules**: every line spells one name (or `.`) per column; an area
  is one contiguous rectangle; children bind by name (unknown name = error +
  suggestion; a nameless child is an error; an UNBOUND area is only a
  warning — sketching the layout first is fine).
- **`cols=`/`areas=` are the only LIST consumers**: entries are ordinary
  values (epx numbers, idents, `fill(n)` calls, `$refs` — theme-driven track
  widths work), newlines free inside the brackets. No other prop is
  list-accepting (fenced).
- Uniform flowing mosaics (N thumbnails a row, spans, full_row headers) are
  `Tiles` (below, D94) — NOT a Grid form, because auto-placement would
  contradict this section's core promise. Data-driven repetition (calendars,
  tables) is `repeat` list templating's job (a v2 candidate).
- **Shared size groups** (future extension, WPF `SharedSizeGroup`): `hug`
  tracks in SEPARATE Grids may someday name a group (`cols=[hug(labels),
  fill]` — the spelling errors today with a future note) and size to the
  group max — the "label column lines up across form sections" problem.
  Zero shipped uses (census above); not built until a second consumer
  appears. The track grammar leaves room.

**Rejected alternatives** (so they aren't re-derived): WPF attached
coordinates (`grid.row=1` — insert-a-row renumbering; dotted keys collide
with anim paths); pure flow (GridContainer model — row boundaries
invisible); `Row`-reuse/`GridRow` marker controls (a control type that is
really syntax); track INFERENCE from cell widths (no `cols=` — Rust could
infer fn signatures and refuses for the same reason: the signature keeps
errors local; inference re-conflates "size my track" with "fill my cell");
markdown-pipe rows (collapses under real props/children); named tracks as
cell props (better coordinates are still coordinates); unifying
Row/Column/Grid into one container (taxes the 90% case; CSS's flex-vs-grid
confusion — the triad already unifies the right layer, vocabulary);
fractional halign/valign for boundary pins (parked — tracks state ratios as
ratios, `fill(1.618)` self-documents where `halign=0.2764` needs a comment
and an accurate author); multi-line strings for a single areas block
(single-line string law keeps unterminated-string errors crisp and the
highlighter stateless — the list-of-strings spelling is CSS's own).

### Tiles (D94 — designed and built 2026-07-12)

The unit-lattice wrap panel — the WP-start-screen mosaic, and the pattern
CSS files under grid auto-placement. It is a SEPARATE type, not a third Grid
form, on purpose: Grid's promise is *placement is structure*; Tiles promises
the opposite — cells flow in declaration order and placement EMERGES. One
type, one contract each.

```kdl
Tiles gallery cols=2 {
    Label title text="top picks", full_row=true
    Panel a { Image source="a.jpg", aspect=natural }
    Panel wide span=2 { ... }
}
```

Rules:

- **`cols=N` is required** (the signature every `span=` is checked against —
  same explicitness as Grid's `cols=`), takes a whole number, and is LIVE:
  code may re-set it and the mosaic re-packs (list values error toward Grid).
- **Packing is first-fit wrap, advance-by-actual-width**: unit = width/cols;
  a cell is `span=K` units (default 1), `full_row=true` the whole row
  (exclusive with `span=` — a full row IS the whole span), or — runtime
  `set_cell` `fixed_w`/`fixed_h` — its own declared size, packed at its own
  pitch. A cell that doesn't fit the row's remainder wraps whole; no
  backfill, the tail stays empty. The wrap test carries a 0.01 epsilon so
  float unit sums never push the last column out.
- **Heights are content truth, not lattice quanta**: a cell is as tall as its
  own minimum, top-aligned; the row is its tallest cell. Godot has no
  measure-with-constraint, so bitmap-driven tiles say
  `aspect=natural` on the Image — the engine derives
  height from width by the texture's ratio (a box-`aspect=` prop was
  REJECTED: with per-tile padding the shipped height chain is affine, and a
  single declared ratio drifts 1–3 epx across column counts).
- **An invisible cell reflows away** (wrap semantics; Grid keeps the cell).
- **`cells=lattice|natural`** (D112, 2026-10-03): `natural` packs every cell
  at its OWN minimum width (first-fit wrap, same walk), capped at the Tiles'
  width; `span=` has no unit to count and says nothing; `full_row=` and
  exact widths keep their meaning. Live, and a pose may write it — the
  names-only effects catalog is the picture lattice with its picture parts
  `visibility=gone` under an arrangement and `cells = natural` in the same
  `when`. Chosen over a second Flow of chips (the 70 entries declared
  twice, every new effect added in two places) and over Flow growing a
  lattice form (Tiles already owns first-fit wrap; Flow keeps its one
  contract).
- **Width feedback**: minimum height depends on granted width (the autowrap-
  Label idiom) — a build settles over a frame or two; tests await ~3 frames.
  Minimum WIDTH is the widest cell's OWN minimum (FlowContainer's rule):
  a zero minimum would deadlock under hugging parents (user-caught in the
  editor preview: everything packed at the origin "like Layers"), and a
  lattice-scaled claim (content min × cols/span — the first attempt) exploded
  with the column count and pushed content past the viewport (user-caught on
  device). Oversized cell content never widens the mosaic — and never clips
  either: the engine never renders a control below its minimum
  (`Control.set_size` clamps), so it OVERLAPS neighboring cells. Tiles WARNS
  at sort time (one `push_warning` per offender; `overflow_warnings` readable
  by tests). The remedy belongs to the content: `clip_text=true` on a Label,
  a `clip_contents=true` wrapper otherwise, or a wider cell.
- `span=`/`full_row=` are CELL props of direct Tiles children only (styles
  can't place cells; widget instance lines may carry them — they stamp the
  widget root).

**Rejected alternatives**: `cols=` on Flow (grid tracks in a Flow costume —
it would silently change Flow's natural-size contract); a Grid auto-flow
form (contradicts *placement is structure*; three placement forms in one
type multiply cross-form validation); strict WP unit HEIGHTS (`span=2x1` —
the shipped catalog's rows are bitmap-ratio heights and code-sized squares,
neither on the height lattice; content-truth heights subsume both);
`aspect=` cell prop (above — affine drift); per-cell auto-clipping (Godot has
no rect clip without a generated wrapper node per cell — hidden node levels
break tree-is-what-you-write and the `set_cell`/`get_parent`/editor-pick
surfaces, and silently swallowed overflow is how parity bugs hide; the
sort-time warning keeps overflow loud instead).

## Atlases (designed and built 2026-09-27, Rust gogo; FanRust DECISIONS.md D97)

An `atlas` line names one picture and how to cut it into **slices**;
`source=` shows a slice. One keyword, four forms:

```kdl
atlas animals source="res://s/animals.jpg", cell=80x80   // grid:  animals[12]
atlas toolbar source="res://s/toolbar.png" {              // named: toolbar.play
    slice play x=0, y=0, size=48x48, edge=8
}
atlas ui from="res://ui/ui.json"                          // import: ui.<frame>
atlas bubble source="res://ui/bubble.png", edge=20        // whole: bubble
```

Rules:
- **Keyword-announced, top level only.** `atlas` sits at a file's top
  level (a `uses` file shares it); `slice` only inside an atlas block.
  Both are refused as node types (G2011). Atlases are read in a pass
  before the file's other directives, so a theme value may name a
  slice whatever the line order.
- **One namespace for the build.** Names are global (code reaches them
  as `atlas::…`); the same declaration twice (one `uses` file under two
  pages) keeps the first ids, a different one is G2019.
- **`source=` or `from=`, never both.** `source=` is always a picture;
  `from=` always a file describing one (TexturePacker "JSON (Hash)" /
  "JSON (Array)"), which names its picture relative to itself.
- **The form follows from the words**: `cell=WxH` = grid (no block;
  counts from the picture's size, which must divide exactly — G2013);
  a `{ slice … }` block = named slices (`x=`, `y=`, `size=` in picture
  pixels, inside the picture — G2014); `from=` = named slices from the
  JSON (the block may add `edge=` only); none of these = the whole
  picture (with no `edge=` it is exactly the picture, no slice).
- **Shorthand as defined sugar**: `cell=` is sugar for numbered slices;
  a whole-picture atlas is one slice covering the picture.
- **`edge=` = 9-slice bands** in picture pixels, `edge=` then
  `edge_left/top/right/bottom` with the padding layering law (later on
  the line wins); left + right and top + bottom leave a middle (G2015).
  Drawn: corners at one epx per picture pixel, edges stretch along,
  middle both ways; bands shrink together below two corners. A 9-slice
  ignores `fit=` (a static `fit=` is G2018); a trimmed import slice
  cannot take `edge=` (G2015).
- **Import names** are derived by one rule: extension dropped,
  lowercased, every non `[a-z0-9_]` character → `_`, `_` before a
  leading digit; two files meeting on one name = G2017. Trimmed frames
  lay out and fit as their untrimmed box; rotated frames = G2017.
- **Hidden semantics made visible — what a slice is at run time**: a
  slice is an `ImageKey` in a reserved range (top bit) indexing the
  build's one `SLICES` table (picture, rect, edge, untrimmed box). gogo's
  draw step turns it into its picture plus a window (nine for a
  9-slice), so the renderer, uploads and lazy decoding only ever see
  whole pictures, and `Value`/`Draw` do not grow. Layout reads a
  slice's natural size from the table, never from the host.
- **`window=` on a slice** is G2018 at build time; a slice set by code
  on an Image with `window=` draws the slice (the node's window does not
  apply).
- **Code**: the build emits `pub mod atlas` — grid `ANIMALS: [ImageKey;
  N]`, named `toolbar::PLAY`, whole `BUBBLE` — mirroring the `.gogo`
  spellings, plus `pub static SLICES`.
- **`release=unused` (2026-09-28, D98)** on an `atlas` line or an Image
  naming a file: the host may free the picture's decoded copy while no
  live page names it, and decode it again when one does. Default: kept
  for the session. Opt-in, not a heuristic, because a picture shown
  often must never decode twice (user call). A picture is freed only
  when EVERY line declaring it says `release=unused` (the lazy/eager
  rule mirrored); an Image showing a slice declares nothing. Hiding or
  scrolling never frees: the host sweeps only after a page is freed or
  when page code asks after swapping its pictures. G2020 for any other
  value, a slice's Image, or a host picture. The build emits
  `RELEASE_UNUSED: &[ImageKey]`.

## Vocabulary (D83 — designed and built 2026-07-11)

The type surface went **framework-neutral**: the WPF dialect words
(`TextBlock`, `Border`, `ScrollViewer`, `WrapPanel`, `TextBox`) retired into
one-keystroke redirects (`Label`, `Panel`, `Scroll`, `Flow`, `Input`), and
WPF searchability moved into the retired hints and the porting docs — the
surface itself answers to the goals, not to muscle memory. Full table:
REFERENCE.md "Vocabulary". The census that triggered it: `Border`(22) and
`PanelContainer`(12) were two spellings of one class; raw `Control`(20) hid
three roles (body host / spacer / draw surface); every view opened with the
same `width=fill, height=fill, background=#00000000` incantation.

- **`View`** — the SCREEN root (D104): the host class IS this node, and its
  children place FREELY. The old boilerplate is its defaults (transparent
  background, fill both axes), injected only where the file doesn't speak —
  anything spelled wins. Root-only (inside the tree: free placement is
  `Layers`, a styled box is `Panel`). A COMPONENT file — a self-contained
  box like a toast or a progress track — opens with `Panel` instead (D105):
  the root IS the styled box, padding/borders native, host extends
  `GogoPanelView` — since D283 a plain `Container` that plays
  PanelContainer through gogo's one panel-host formula (measure, sort,
  plate), so a `width=host`/`height=host` axis really is left to the
  host's code; a raw `PanelContainer` host still builds (G7002) but
  refuses host axes (G7005). A Panel root gets the blank slate too — `background=
  transparent` injected where the file doesn't speak it, so the ambient
  theme's gray plate never leaks into a component (anything spelled wins).
  Any other root type is a load-time error naming both
  doors. NOT hidden by default: `visibility=gone` is state, never identity.
- **Exact raw duplicates redirect**: `VBoxContainer`→`Column`,
  `HBoxContainer`→`Row`, `PanelContainer`→`Panel`, `FlowContainer`→`Flow`,
  `ColorRect`→`Rectangle`, `HSlider`→`Slider`, `LineEdit`→`Input` — two
  spellings of one node type would split every grep. The raw register stays
  for classes gogo has no name for (`Control`, `TextureRect`,
  `ScrollContainer`, ...); Godot's raw `Panel` class is shadowed by the
  alias (accepted: a bare gray Panel was never reachable under the blank
  slate anyway).
- **`Circle`** (gogo_circle.gd) — discs/rings via `_draw`; radius
  self-derives from the box. `color=`/`border_color=`/`border_thickness=`
  are real properties; transparent fill + border = ring. `Ellipse` redirects
  here; true ellipses stay out of v1.
- **`Gradient`** (gogo_gradient.gd) — generated GradientTexture2D fills:
  `fill=linear|radial|square|conic` × `repeat=none|repeat|mirror` ×
  `stops=[...]` (REQUIRED — no hidden ramp) × `angle=`/`from=`/`to=` aiming.
  Stop rules: `@positions` are all-or-none, never decrease (equal = hard
  step), and under mirror stop at 0.5. One stop = G2003 with a behavior fix
  (a solid is a `Rectangle`). Stops stay live as `stop_N_color`/
  `stop_N_offset` properties — whens/anims drive them. `Radial` (and its
  `solid_until=`) retired into `fill=radial, stops=[C@0, C@S, C/0@1]`.
- **`choice` / `Radio` / `Check`** — one-of-N with a lexical home: `choice`
  is erased structure (children parent flat); every `Radio` descendant joins
  one ButtonGroup (grouped CheckBoxes render radio visuals natively). A
  Radio with a BODY is the TEMPLATED form (D142): the body is the whole
  look — the node builds as a toggle Button in the same group with its
  chrome STRIPPED (D144: normal/hover/pressed plates blank, the focus ring
  kept), `selected` + whens carry the selection visual, and `text=` errors
  (letters live in the body's Labels). `value=` pre-presses by name; a named choice
  auto-wires `On<Name>Change(radio_name)`, `on:change=` / `emit()` are
  the explicit forms — and changed means CHANGED: one report per genuine
  transition (re-tap of the current pick repairs, never re-reports;
  programmatic `button_pressed = true` reports like a tap, a no-signal
  seat stays invisible). Radios need names (the name IS the reported value);
  duplicates
  error; scopes don't nest; disjoint subtrees can't share a group — by
  design. `Check` is the independent toggle and never joins a group; it
  takes the same BODY spelling (the templated Check is the D142 twin:
  blank toggle Button, checkbox reader role, no group, `on:change`
  wiring unchanged).
- **Deliberately NOT types**: Divider (`Rectangle` is honest), Switch/Card/
  Chip (**types name mechanisms, styles name looks, widgets name
  composites** — the capping principle), lists (`repeat`), dialogs
  (patterns).

**Rejected alternatives** (so they aren't re-derived): keeping the WPF
dialect (searchability lost to the goals — the hints + porting table carry
it); a capitalized `Gap` type (the only type that would refuse a name — a
memorized exception; lowercase makes namelessness automatic under the
capitalization law); `fill=` as a Rectangle prop switching it to a gradient
(breaks node-type stability — the class would change under a cast);
group-id props on loose radios (`group="size"` — stringly-typed
spooky-action exclusivity; the lexical scope shows the whole set);
`solid_until=` kept alongside stops (two spellings of one plateau).

## Default look

A bare control is one picture on every engine. The numbers below ARE the
default look — they live in the runtime's draw constants, not in an
engine theme, so a file with no styling renders the same everywhere
(decided 2026-08-30; an engine's native templates are not consulted).
Anything a file styles (`background=`, a `when held` response, a
`theme { Kind … }` default) replaces the matching piece and only that
piece.

Sizes are epx. `accent` and `gray` are the palette's words.

- **Button** — plate gray 0.1 at alpha 0.6 (square unless the file says
  `corner_radius=`); the label centered. No plate under a native mark
  (Check/Radio draw their mark, not a plate). A text-only Button (one
  with no children of its own) puts 12 of air left and right of its
  label and 6 above and below unless the file writes `padding=`; a
  Button holding its own content spells its own padding. The default is
  decided once, at resolve, so layout and draw inset by the same number
  (decided 2026-08-30).
- **Check** — a 20 box, 1.5 white outline; selected fills `accent` and
  draws a white 10 square inside. Mark, then 8 of air, then the text.
- **Radio** — a 20 ring, same outline; selected draws an `accent` 10
  dot. Same mark-then-text run.
- **Toggle** — a 44×22 pill, `gray` off / `accent` on, with a white 18
  knob at the off/on end. A bare Toggle's BOX is at least 44 tall — a
  finger target — with the pill centered in it; the drag reads x only,
  so the tall box changes no math.
- **Slider** — a 4-tall `#5A5A5A` track, white 0.6 fill to the value, a
  white 16 knob. A bare Slider's strip is 32 tall.
- **Input** — a bare Input is 36 tall; the text sits at the padding,
  the caret is a thin bar at the typing point, and `placeholder_text=`
  draws at half the ink's alpha while the field is empty.
- **List rows** — a bare `ListItem` shows selection, the press and the
  keyboard cursor out of the box: the selected fill is white at 0.3,
  a held row shades at half that, the cursor echo is a quarter of it.
  An author's own `when` for a state replaces exactly that state's
  default; `select=off` rows drop the selected one. Untouched rows stay
  transparent.
- **Scroll bars** — a `#737373` grabber in a 4-wide lane on an
  overflowing axis (Scroll and List both), as long as the viewport's
  share of the content; `bars=never` drops lane and grabber.
- **Rectangle** — colorless is WHITE (an engine ColorRect's reading); a
  colorless Circle draws nothing.
- **Disabled** — a disabled control draws its whole subtree at
  `theme { disabled_opacity = … }`, default 0.3. The dim is a built-in
  `when disabled opacity=N` sitting at the BOTTOM of the `when` ladder
  (decided 2026-08-30): a `when disabled opacity=` the file writes on the
  node or on its style replaces it (`when disabled opacity=1` opts out), a
  plain `opacity=` does not cancel it, and an unrelated `when held
  opacity=` changes nothing. An anim that owns the opacity channel wins
  over both.

## Reserved words

`widget`, `style`, `theme`, `uses`, `slot`, `signals`, `anim`, `arrangement`,
`atlas`, `slice`, `when`, `transition`, `each`, `group`, `play`, `and`, `then`, `delay`,
`key`, `row`, `skip`, `gap`, `choice`,
`held`, `hovered`, `focused`, `focus_within`, `disabled`, `selected`,
`base`, `any`, `true`,
`false` — plus the retired `state`, `enter`, `exit`, `pressed`, `hover`,
`focus`, `checked` (errors point at their
replacements; the last four respelled in D181 — see Input vocabulary).
Reserved in TYPE position — node NAMES stay free (the
lowercase structure words also shadow no widget names) — except `base`
and `any`, which are reserved everywhere (decided 2026-08-31): they name
state endpoints, and a node called `base` next to `transition base ->
held` would read as two things. As in Rust, `match` is never a name.
`Button base` is a load error naming the rule (G0046). `true`/`false` are
value words in the name position too (`Check true` errors naming
`selected=true`).

A node name is unique within its page (decided 2026-08-30): a name is an
address — `when x.prop=`, an anim lane, `x.seat =`, the host's lookups —
so two nodes answering to it would be a silent retarget; a duplicate is a
load error. Widget bodies and repeat items are scoped per instance
(`inst/part`, `x/i/item`), so repeated structure never collides. The Rust
port enforces this; the addon owes the same check.

## Explicitly out of scope for v1 (v2 candidates)

String interpolation (`"Size: $value"`), expressions (`$spacing*2` —
`play()`/`go()`/`toggle()`/`emit()`/`fill(n)` are the only call forms), required
parameters, named/multiple slots, block comments, general arrays (the list
kind ships but only Grid's `cols=`/`areas=` and Gradient's `stops=` accept
one), raw/multiline strings, `repeat` list templating (its own design
round — the expressions on-ramp), `Slider orientation=` (no consumer yet),
multiline `Input` (TextEdit), `Gradient resolution=`, true ellipses.
(Style single inheritance shipped as `extends=`; multi-parent composition
stays out.)

## Example

```kdl
// styles/theme.gogo
$accent        = #E51400FF
$swatchDefault = #EB6428FF

theme dark {
    $accent = #FF6B4AFF   // active via Gogo.set_theme_variant("dark")
}

style ToolButton min_height=36, tap_pulse=true

widget LabeledSlider $label="", $min=0, $max=100, $value=0 {
    Column spacing=4 {
        Label text=$label
        Slider signals min=$min, max=$max, value=$value
    }
}
```

```kdl
// brush_options.gogo
uses "styles/theme.gogo"

Panel visibility=gone, halign=center, valign=bottom, margin_bottom=72 {
    Column spacing=8, min_width=240 {
        LabeledSlider sizeSlider label="Size", min=1, max=300, value=80
        LabeledSlider hardnessSlider label="Hardness", value=30
        Control preview size=220x110
    }
}
```
