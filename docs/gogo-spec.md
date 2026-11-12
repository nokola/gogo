# Gogo — a declarative UI language (v1 spec)

*Change it. See it. Gogo.*

This document is the authoritative v1 reference for the Gogo language. It is written
for both humans and AI assistants generating or editing `.gogo` files and tooling.

The language was first built as an addon for the Godot engine and is
implemented today by the Rust crate `gogo-ui`. This spec describes the
Rust crate. Where an earlier behavior is worth remembering, the text says
"the Godot addon" for it; decision ids like D83 point into the project's
history. "The resolver" is this spec's word for the compile step that
follows parsing. How a program hosts gogo (compiling pages, feeding input,
painting) is in REFERENCE.md, "Host API".

## Goals

**Main goals — every language/design decision is measured against these:**

- **Easy to understand, easy to write, easy to find. Ergonomic.** A `.gogo`
  file can be read 6 months later and understood.
- **Rust-inspired design**: every construct keyword-announced, one keyword per
  concept, hidden semantics made visible, shorthand legal only as defined
  sugar.
- **Node-type stability** (D83): a property edit must never change what a
  node IS. Code holds nodes by name and reads them by kind — a prop that
  swapped the kind underneath would break that code from a distance. When the cost
  or character differs, the TYPE differs (`Rectangle` vs `Gradient` vs
  `Circle` — Rust's `&str`/`String` honesty).
- **The capitalization law** (D83): **capitalized = an addressable node**
  (nameable, stylable, animatable); **lowercase = structure** (`row`,
  `skip`, `gap`, `empty`) — never named, erased at build or anonymous.
  Four lowercase words do take a name, as a handle: `choice`, `repeat`,
  `slot`, `seat`.

1. **Declarative UI** with XAML lineage (panels, styles, themes, includes) but
   none of YAML's fragility (significant whitespace, `children:` ceremony, quoting rules).
2. **Hot reload**: a page follows its file inside the running program
   (`gogo_ui::reload::Reloader`, the cargo feature `hot-reload`). Errors
   never blank the running UI — the previous good page stays and the errors
   come back with their line, code and fix hint. A build without the
   feature carries none of it.
3. **Minimal ceremony**: the node name *is* the widget type; braces *are* the children;
   bare numbers *are* epx.
4. **Dumb parser, smart resolver**: the parser knows only the grammar. `theme`, `style`,
   `widget`, `control`, `uses`, `slot`, `signals`, `behavior`, `state`, `state_builtin`,
   `signal` are ordinary nodes/flags that the *resolver* interprets. Property typing,
   unit meaning, enum validation and did-you-mean suggestions all live in
   the resolver, against one table of words per kind.

## File model

- Extension: `.gogo`. One UI fragment (or theme/style/widget library) per file.
- **The ROOT KEYWORD SET is {`View`, `Panel`}** (D104/D105) — a file
  announces itself as a SCREEN or a COMPONENT in its first word.
  **`View` — the screen**: the host program lays this node out in the
  rectangle it passes; children place FREELY (halign/valign + margins),
  and `View background=...` draws the root plate (`padding` on a View root
  folds into every child's placement, D114 — no wrapper Panel).
  **`Panel` — the component**: the root IS the styled box (a toast banner,
  a progress track) — container regime for its children, padding/border
  stacking as on any Panel, no wrapper node. Any other root type is a
  build error (G7003); the one exception is an instance of a widget the
  file declares or imports, which `compile` accepts (`build_pages` takes
  only files whose root line starts with `View` or `Panel`). Fragment files meant only for `uses`
  (themes/styles/widgets) have no root and aren't built standalone.
- `uses "relative/path.gogo"` imports another file's theme values, styles, and widgets.
- Comments: `//` to end of line. No block comments in v1.
- Whitespace between tokens: space, tab, CR — and NBSP (U+00A0), so text
  pasted from rendered docs tokenizes identically (D86).
- WORD JOINERs (U+2060) are stripped before lexing: the same rendered docs
  glue `-` and `/` with them so `row_span` never wraps mid-token, and a
  joiner pasted INSIDE a token must not split or rename it. Zero-width,
  and a diagnostic names a line, not a column, so nothing it reports
  moves (D87).

## Values

| Kind   | Syntax                  | Notes |
|--------|-------------------------|-------|
| string | `"Paint"`, `"new stroke"/newStroke` | Escapes: `\"` `\\` `\n` `\t`. No interpolation in v1. Optional `/key` suffix = translation term (see below). |
| path   | `"res://icons/undo.png"`, `"preview://brush"` | A string on a resource property (`source=`, `font=`), scheme always written (G0071 — a bare word could be a label or a path). `res://` is an asset under the build's resource root, checked and embedded at build time. Any other scheme on an `Image`'s `source=` is a **host picture**: the app's code registers pixels under that exact name at run time (a live preview, a canvas tile); the build bakes the name as the picture's id, and the box shows nothing until the code registers it. `""` = no picture. Never translated — `/key` on a path is an error. |
| slice  | `animals[12]`, `toolbar.play`, `bubble` | An **atlas slice** on `source=` (and in a theme value or widget param that feeds one): a grid cell by number, a named slice, or a whole-picture atlas — see Atlases. A bare word on `source=` always names an atlas (a path is quoted); `[n]` touches the name, no space. |
| number | `36`, `0.5`, `-4`       | A number on a *length* property is **epx** (effective pixels, density-independent); the suffix `epx` may be written. There is no device-pixel unit: `px` is refused (G0038). `%` is legal only where a rule names it (an anim's `from=`/`to=`, `key_unit=`). On a scalar property (e.g. `Slider.value`) `px` and `%` are refused the same way; `epx` there is accepted and means nothing. |
| bool   | `true`, `false`         | |
| color  | `#RRGGBBAA`, `#RRGGBB`, `red`, `black/0.5`, `white@0.24` | RGBA hex, alpha **last** (the CSS convention). 6 digits ⇒ alpha `FF`. Bare palette names (`red`, `danger`, `surface`, `transparent`) are contextual keywords legal only in color-typed positions (see Resolver-defined vocabulary). Every color spelling takes an optional `/opacity` suffix (a bare 0..1 number) setting the **final alpha**: `black/0.5`, `#FF6347/0.25`, `$accent/0.3`. `/o` **replaces the alpha of the value it is written on** — on a `$ref` or palette word, the alpha that value would have had: `$flash/0` is `$flash` made transparent (a translucent disc's fade stop derives from the disc), `$dim/0.3` over `$dim = #80808080` draws 0.3, `$a/0.3` over `$a = $b/0.5` draws 0.3. An 8-digit hex takes it too (`#FFFFFF6B/0` — a pasted color, faded). The suffix written at the slot is the alpha the slot gets; there is nothing else to know (E6, 2026-08-31). Inside a Gradient's `stops=` list an entry may also carry `@position` (0..1, always after any `/`) — fenced everywhere else. **Porting from WPF/XAML:** WPF writes `#AARRGGBB` (alpha first) — move the leading alpha pair to the end (`#AA000000` → `#000000AA`). |
| ref    | `$name`                 | Reads a widget parameter (innermost scope) or theme value. |
| ident  | `center_bottom`         | Bare word: enum member or handler name; validated by resolver per property type. |
| size   | `220x110`, `220x`, `x110` | Shorthand for an exact size on both axes (the `size=` prop: exactly that size). Components are epx only (no units inside). Omitted component = unset. `min_size=` is the minimum-only spelling; there one number is both sides (`min_size=44`). |
| call   | `play(appear)`, `fill(2)` | Ident + parenthesized args. Callees are closed per position: `play`/`go`/`toggle`/`emit` on `on:` keys, `fill(n)`/`hug(n)` in sizing positions (weights — n may be fractional), `round(n, step)` where a number or a text goes (n rounded to a multiple of step; step is 1 when not written), `arc(…)`/`angle(…)` on a slot's `automatic=`. A call outside this list, or a sizing call where no size is written, is refused (G0085). The controlled on-ramp for v2 expressions. |
| list   | `[80, hug, fill(2)]`    | Ordinary values in brackets; newlines are free inside the brackets (the list cousin of trailing-comma continuation). Six props accept one: Grid's `cols=`/`areas=`, Gradient's `stops=`, `border_dash=`, an Arc's `dash=` and an Image's `window=`; a list on any other prop is refused (G0040). |

**Attached dots form a path**: `listBrush.translate.x` (no spaces) lexes as ONE
identifier — an anim track target or a dotted `when` key. A dot after
whitespace is style application (`Button b .ToolButton`); `position .x` with a
space therefore fails loudly (on a node line as "unknown style `.x`", G5002).

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
indexes the language table the host hands the page (`Runtime::set_locale`), and
a key the active language lacks falls back to its literal — never the raw key,
never a placeholder. A host can also RE-KEY a node's text while the program
runs: a build generates a `Key` per term id and `Runtime::set_key(node, Key)`
swaps which term a node shows (localized under the active locale), while
`set_text` writes a literal that is never translated — two writes, two
meanings, so a code-fed string can never half-translate. One key carries one English string: repeating a key with
the same literal is legal sharing, a different literal is a build error
(G8001, the drift the fused spelling exists to prevent). Keys are legal
wherever the string is DISPLAYED (node/style props, widget params)
and rejected where it isn't (theme `$values`, resource/`uses`
paths). `/` on any other value type is an error. `/_` is no key at all:
the string stays English on purpose — it never interns, so two
`/_` strings may carry different English. A file whose top carries the line
`allow untranslated` keeps all its text English on purpose; a host that
requires keys (`CompileOptions::require_keys`) makes every other
displayed string with no key an error (G8002). `allow` is a top-of-file
line and takes exactly `untranslated` (G8003).

Property keys are snake_case — ONE spelling (D137; the kebab alias is gone,
`min-height=` no longer lexes): `min_height`, `margin_bottom` — a
double-click selects the whole key.

## Grammar (EBNF)

```ebnf
document    = { node | theme-value } ;
node        = TYPE-IDENT ,
              { name | "." STYLE-IDENT | flag | property | param-decl | arg
                | arrow-endpoint | clause | "," } ,
              [ block ] ,
              terminator ;
arrow-endpoint = "->" , IDENT ;             (* after the name: transition endpoints *)
clause      = ( "as" | "by" | "in" ) , value ;  (* `as`/`by` on repeat lines, `by`/`in` on slot
                                               and behavior lines; comma-FREE — the keyword
                                               delimits, the arrow-endpoint precedent;
                                               fixed order: as before by, by before in *)
theme-value = "$" IDENT , "=" , value , terminator ;  (* "=" required; top level = default, inside `theme <variant> { }` = override *)
name        = IDENT ;                      (* first bare non-flag identifier *)
flag        = "signals" ;                  (* reserved flag words; comma-FREE like a
                                               clause — the keyword delimits, so
                                               `Button signals text=$x` parses; a comma
                                               after the flag stays legal *)
property    = PROP-IDENT , "=" , value ;   (* PROP-IDENT may contain "_", "." and ":" e.g. min_width, slot.track.height, on:activate *)
param-decl  = "$" IDENT , "=" , value ;    (* meaningful only on widget declaration lines *)
arg         = value ;                      (* positional, e.g. uses "path" *)
block       = "{" , { node | theme-value | assignment } , "}" ;  (* "{" must be on the node's logical line *)
assignment  = IDENT , "=" , value , terminator ;  (* `node.prop = value` in a `when` block, `red = #E51400` in a `theme` block *)
terminator  = NEWLINE | ";" | EOF | (peek "}") ;
value       = ... | list | index ;
list        = "[" , [ value , { "," , value } , [","] ] , "]" ;  (* newlines free inside *)
index       = IDENT , "[" , DIGITS , "]" ;  (* ATTACHED — no space before "[": a grid atlas cell,
                                               `animals[12]`; a list always follows "=", "," or a space *)
```

Rules:
- **Commas are required between properties/params/args** and forbidden as trailing
  garbage before `{`.
- **Trailing comma continues the node onto the next line**; there is no `\`. One
  more line continues: a line holding only a type and a name (`Layers box`) runs
  on when the next line opens with a size (`100x50`) or a `key=` word.
  Indentation is never significant.
- Canonical member order on a line: `Type name .Style prop=..., prop=... { children }`
  (repeat lines: `repeat name as $b by $b.field {` — clauses sit where props would).
- Duplicate property on one node = parse error.
- A `$ref` may carry an ATTACHED dotted field path (`$task.id` — one token; a dot
  after whitespace stays style application). Dots belong to repeat item
  bindings: a dotted name in a `$value` or parameter definition is refused
  (G9008), and a dotted `$ref` that reads no item binding is an undefined
  ref (G6001); theme values and params are flat.

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
  (`Runtime::set_theme_variant`). A file
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
  values: a hex color or another color name (validated against the closed set,
  with suggestions); a theme `$ref` is refused (G6004). The name set is closed — new names belong
  to `$tokens`. Document overrides win over shipped defaults. An override
  by another color NAME is followed at every use, so `theme { red = X }`
  moves `danger` too (the alias of `red`). Palette overrides live in the
  plain block (every mode) or `theme dark { }` (the dark column only) —
  under any OTHER variant name they are refused (G6006, GPAR88): only
  `dark` has a second color column, and a variant-scoped color is a
  `$value` (`theme fancy { $c = … }`), which IS variant-gated.
  - Literals (mode-independent): `transparent` (`#00000000`), `black` `white`
    (exact `#000000`/`#FFFFFF`, reserved for testing — leave unoverridden by
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
  `font_size` and `font_color`, the page's text color)
  apply to every node of the page without its own override, and between
  two blocks that set the same default the later one wins: the page's
  over an included file's. The text color was `color` here until
  2026-11-11 (decided 2026-08-30) and did nothing; `color` in a `theme`
  block is refused now (G3006), so text color has one word (REFERENCE
  "Themes"). Inside
  a node's body, a one-line `theme font=… / font_size=… / font_color=…`
  carries those text words down that subtree only; any other word on a
  block theme line is refused (G3006), and a nested `theme dark { }`
  block is refused too — variants live at the top of the file.
  **Built-in default: `font_size = 15`** (D88): a document that declares no
  font_size behaves exactly as if it declared 15 epx.
- **`repeat <name> [as $binding] by <key-path> { empty { ... } <template> }`** —
  N children from a code-set collection (`Runtime::set_items`),
  reconciled by KEY on each assignment: kept keys keep their rows, changed data
  re-binds its row, absent keys free theirs, order follows the collection. Erased
  at build: the rows lay out as children of the repeat's parent.
  `as` defaults to `$item` (defined sugar); `by` must key the
  binding — no index identity, ever. A repeat with no `by` is the counted
  form (`Runtime::set_count`, REFERENCE "Repeat"). `$binding.field` reads item data as a static
  value re-applied per assignment. `empty { }`, first in the body, is the
  zero-items content (a literal `empty { }` is legal: "considered, show
  nothing"); it shows at zero items, the initial state included. A `List` must
  have one; a plain repeat may leave it out. It holds ONE child (decided 2026-08-31): two would need an
  arrangement nobody wrote — `empty { Column { … } }` says stacked,
  `empty { Layers { … } }` says overlaid; more than one is a build error
  naming that fix. Inside a `List` the empty block sits on the LIST body instead
  (D128, one seat per context — G9016 routes it): richer zero-items views
  beside the statics, and the List's repeat carries none. Bad data (duplicate keys above all) REFUSES the
  whole assignment (`set_items` returns the error); the last good items stay
  live. A repeat inside a widget body works (code reaches it through the
  instance, `inst/rows`). A repeat inside another repeat's row is a build
  error (G9018): code fills a repeat by its one name, and every outer row
  would hold one. Full design:
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
    error; a built-in kind takes a `for=` style directly. Plain styles keep taking any state name — contract states check at the
    APPLICATION site ("Button has no state 'busy' — states here: ..."), and a
    plain custom state landing cleanly on exactly one control type draws the add-for=
    advisory (two-plus distinct clean types = deliberately generic, advisory silent).
    `extends=`: same-target chains and for=-over-plain are fine; plain-over-for= and
    cross-target chains error.
- **`widget Name $param=default, ..., rootProp=... { <single root node> }`** — a
  reusable component. On the declaration line: `$x=v` declares a parameter (defaults
  required in v1); bare `k=v` bakes a property onto the root. A `.Style` on this
  line is refused (G3006): the line declares a name, it is not a node. A style goes
  on the first node of the body (every instance wears it) or on an instance line
  (that one does). Instantiated exactly like a built-in: `Name instName param=..., other=...`.
  The name is no built-in kind and no gogo keyword (`widget each`, `widget held`,
  `widget fill`: G5001).
  - Instance name and any instance property that is **not** a declared parameter pass
    through to the widget's root node.
  - **`slot`** — where a widget takes content (control-customization spec §3). Bare
    `slot` is the default slot (write at most one): the inline marker instance children land
    at; it may carry default content in braces, replaced when the instance brings bare
    children. `slot <name> [props] { default content }` declares a named REGION (a real
    node): its line takes placement + the box family (`background`/`corner_radius`/
    `border_*`/`padding` — content props route to filling), content is clipped and
    fills it, and its minimum derives from content + padding. Instances fill by name:
    `slot header { ... }` — the fill's line takes the name and nothing else; nested
    slot declarations survive fills and draw above them. A fill is checked where it
    is written (G5004): a fill whose name matches no slot of the widget, a word on a
    fill's line, one slot filled twice, and bare children at an instance whose widget
    has no default slot are build errors; so is a second default slot in a widget's
    body. A `slot` line written in a fill, or among an instance's bare children, is
    a slot of the widget that instance is written in.
    Two slots with one name are refused where the widget is used (G0044).
  - **`signals`** — a flag on one node inside a widget body marking the event surface:
    an `on:` entry written on an instance lands on this node.
    Omitted ⇒ the root is the surface.
  - Widgets may use other widgets; cycles are a build error (cycle path printed).
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
    build error — there is no host state to answer; the one exception is
    `when screen.<word>`, which poses the widget's own body. Inside any widget body, `when
    selected` on a plain (non-toggle, non-ListItem) node never shows: nothing
    raises it there (the D276 dead-when trap; the Godot addon refused it,
    the Rust compiler builds it).
- **`control Name $param=default { ... }`** — a widget plus an INPUT
  BEHAVIOR (control-customization spec §5, D108). Same declaration-line
  grammar as `widget`; the body holds directive lines beside exactly one
  root: `behavior <name> by $value in $min..$max[, step=$step][,
  key_unit=5%]` (the behavior that answers the instance's finger and keys
  — gogo's `slider` and `toggle`, or one the host registered in Rust
  (`gogo_ui::Behavior` + `CompileOptions::behaviors` + `Input::register`);
  optional — without one the control's input is its code-behind's, through
  `on:pointer=` and `keys`; the name is checked, the slots the behavior
  reads must be declared, and the key groups it owns refuse a `keys`
  claim — G0082), `state <names>`
  (custom states the code-behind raises through `Runtime::set_custom`),
  `state_builtin <names>` (opt-in to the runtime-maintained set — read,
  never written, by anything but the runtime; `selected` on a behavior
  control derives from the value), `signal <names>` (the names a
  behavior may raise — `drag_started`/`drag_ended` at a gesture's edges;
  each reaches code as a `Signal` event), `keys <groups>`
  (the key groups a focused instance claims for its
  code-behind — `arrows`, `home_end`, `enter`, `space`, semantic words
  only; a claimed key arrives as a `Key` event on the instance root and
  never moves focus, Tab is never claimable), and a bare
  `transition` rule (the control's default FEEL: it times state flips and
  the fraction glide when no style or node block overrides). A property
  in the body that reads `$value` follows the control's value, written
  bare (`Label text=$value`, `Rectangle opacity=$value`; code writes the
  value with `Runtime::set_value`); every other `$param` is fixed when
  the page is built. Slots may declare
  an automatic coupling, fully spelled — nothing defaults invisibly
  (D134): `automatic=<kind> by <driver>`. Kinds: `length`/`position`
  (linear), `arc`/`angle` (radial — config REQUIRED in the kind's parens:
  `arc(from=-135, sweep=270)`, degrees on the compass as an `Arc`'s: 0°
  up, clockwise positive),
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
- **The built-in controls draw themselves.** There is one renderer and no
  `render=` word (it is refused as an unknown property). The built-in
  `Slider` takes three slot words, from a `for=Slider` style or on its own
  line: `slot.track.background`, `slot.track.corner_radius`,
  `slot.fill.background`. The built-in `Toggle` and `Button` have no
  restylable slots; their states still answer in styles. A control that
  needs restylable parts is a `control` declaration.
- **Theme type defaults** (spec §5.8, D111) — a type-keyed entry inside a theme
  block, `theme { Slider .ThinSlider }`, defaults every instance of a
  CONTROL type: an instance-line prototype (no name, no children, no `on:`
  wiring, no placement props — G3006). Nearest scope wins (an instance line
  beats the entry); the entry's style seats base-most.
- **`uses "path.gogo"`** — import (relative to the current file).
- **`on:<event>=HandlerName`** — wiring property. The events are a closed
  set of bare stems (D181): `activate`, `press`, `release`, `change`,
  `edit`, `show`, `hide`, `pointer` (see Input vocabulary); any other word
  after `on:` is refused (G0049). Also takes `play(animName)`, `go(name)`,
  `toggle(name)`, `emit(Name)`; `on:show`/`on:hide` are the visibility
  pair and take `play(animName)` only (G0084; see Animations).
- **Look properties**: `opacity` (the whole node's alpha),
  `font_size` (epx), `font_color` (a Button has no `icon=` word: a
  Button with a picture holds an `Image`),
  `background`/`corner_radius`/`border_*`/`padding` (any box: the plate
  draws behind the content and covers the padding; a Button starts with
  its default plate, which `background=` recolors. On a `Circle`,
  `border_color`/`border_thickness` drive its ring instead).
  `padding` is air between a node's box and its CONTENT — children or
  text: on `Layers`, the `View` root, stacks and slot lines it insets
  every child's placement and adds onto the box's minimum, and
  `Label`/`Input` inset their own text (WPF's TextBlock/TextBox Padding,
  verbatim). Drawing leaves (`Rectangle`/`Circle`/`Arc`/`Gradient`/`Image`)
  have nothing inside — outer air is `margin`, and a child written in a
  shape's block is refused (G7014): hold the two in a `Layers`, or use a
  `Panel`.
  `padding` also comes per side — `padding_left`/`_top`/`_right`/`_bottom` —
  mirroring the `margin_*` family (WPF's per-side Padding thickness).
  `corner_radius` also comes per corner (D135) —
  `corner_radius_top_left`/`_top_right`/`_bottom_right`/`_bottom_left`: NAMED
  keys, so there is no corner order to memorize (the WPF/CSS positional
  four-value form is exactly the hidden semantics gogo bans). The uniform
  writes all four corners, a per-corner key writes one, later on the line
  wins — the `padding`/`padding_left` layering law. Slots take them too
  (box-family members everywhere the family goes).
  `padding` is air INSIDE the border (D80): content sits
  `border_thickness + padding` from the node's edge, WPF/CSS stacking. An empty
  string on a resource property means null (no load, no error); `style=X` is
  a build error (G2001: `style` is not a property); a style is applied with `.X`.
- **Input (D103)** — one model: interactive controls (Button/Slider/Input/
  Scroll, …) answer input by default, everything else is transparent, and
  **`interactive=true/false`** overrides that — per control, like any bool
  property (legal in `when` blocks; applies instantly, restores on release).
  `true` claims input for the node, `false` makes it hit-invisible without
  restyling (`disabled` is the restyling one). Region silencing is a catcher:
  an invisible `interactive=true` plate over the region; `interactive=false`
  on an already-passive node does nothing, and the compiler says nothing. Bare `interactive`
  (no `=`) is a build error redirecting to `interactive=true` — it never
  silently becomes a node name.
- **Placement** (one vocabulary under every parent):
  - **Default: a child takes the space its parent gives it.** An axis with
    no placement key takes the parent's extent — container slots (cross axis
    fills) and free parents (Layers/buttons: the whole rect)
    alike; the document root takes the rectangle the host passes the same
    way. Exempt: content of the bare
    default `slot` (the inline marker — its children join the widget's own
    layout, explicit placement only; named slot REGIONS fill like any free
    parent). `width=hug, height=hug` is the opt-out: the box takes its
    content's size.
  - `halign=left/center/right`, `valign=top/center/bottom` — position ONLY.
  - `width`/`height` — the sizing triad: `<epx number>` = exactly that size,
    `hug` = natural size, `fill` = take the leftover,
    `fill(n)`/`hug(n)` = weighted. **A box that fills a written box takes
    that box** (decided 2026-08-30): a box with no size word (or `fill`)
    takes the room it is given; when that room is a number the author wrote
    — an exact size or a cap on the box it fills, or on any filler above it
    — that number is the answer, so a written size is hard for the box AND
    for what fills it (`Layers bar height=80 { Layers inner { Rectangle
    height=150 } }`: `inner` is 80, the 150 hangs out and the written box
    cuts it). When the room comes from nowhere the author wrote (the
    screen, a chain of unwritten boxes) the box is never smaller than its
    content. A hug box is sized by content and passes no written number
    down; a `min_*` floor is a written number and wins; a Label that does
    not wrap keeps the width its text needs (one line of text does not bend
    to a box; the written box cuts it), and a wrapping Label takes the
    written width like any box and folds its text there, inside a word
    where a word is wider than the box. Where the room is no written number
    (a Row's leftover, a chain of unwritten boxes) a wrapping `fill` Label
    is never narrower than its longest word. The own text of a Button, a
    Check, a Radio and a Toggle folds by the Label's rule with
    `wrap=true`; such a control takes a written width like any box.
    (The Godot addon let the engine grow the filler to its minimum —
    a recorded difference.) `width=host` / `height=host`, the addon's word
    for an axis the host's code sizes, are gone (G0055): a layout that
    writes nothing back has no such axis to guard, and the host program
    sizes a page by the rectangle it lays it out in.
    `min_width`/`min_height`/`min_size`
    stay minimum-only (stretchy). Hug means natural size EVERYWHERE (D254):
    wrap-capable text measures at its UNWRAPPED single-line extent, capped
    or not — wrapping happens only where a width binds it (a cap, `fill`,
    or a sized parent). A wrapping label whose width provably never binds gets the G1004
    lint, in two shapes: `width=hug` with no `max_width` never wraps, and
    `width=fill` under a chain that sizes to content wraps at about one word.
    A min next to an exact `width=`/`height=` for the same side bounds
    POSED sizes (GPAR79 2026-08-31): where a `when` writes that size on
    the node the pair builds, and `size = max(floor, written)` — the WPF
    Min-over-Width order — in every parent, a stack's child included. The
    same holds when the two words come from different lines: a style or
    a control's template carries the floor, the line that uses it the
    size. On ONE line whose size nothing poses the pair is a build error
    (G1019, decided 2026-11-11): the floor says nothing or contradicts
    the size. A cap beside an exact size follows the same rule; an exact
    size is never cut by a cap (the cap bounds a `fill` or a `hug` that a
    `when` poses). A min on the side `aspect=` works out is a build error
    too (G1017): the layout never reads it.
    - **`max_width`/`max_height` — the size caps** (D132; the List caps
      D123/D124 generalized): pair with `hug` (content size, at most the
      cap) or `fill` (available space, at most the cap — D245, below). On
      a hug axis: natural content size, at most the cap. Wrap-capable text hugs its UNWRAPPED
      extent and starts wrapping only when the cap binds (a wrap-Label's
      plain minimum collapses toward its longest word; the cap is what makes
      "hug a wrapping label" expressible). Once the cap binds, the box is
      as wide as the cap (a number and `fill` alike) and as tall as the
      wrapped lines, whichever width word stands beside the cap, and a
      parent that hugs the box is that tall too. The box tracks content LIVE —
      code-behind text swaps re-fit it. A cap never shears content: hard
      minimums win (WPF's own clamp order — Min beats Max:
      `size = max(floor, min(natural, cap))`). A written size or
      cap that binds is a hard edge BOTH ways: what pokes past it is
      clipped in the picture and takes no taps — a button's clipped half
      is neither drawn nor pressable (GPAR37). The cap holds for any box
      under any parent: a box never reports more than its cap. One refusal
      remains (G1002): a cap that reads the parent's space on a stack's OWN
      stacking axis is circular — cap the stack itself. On `List` the caps
      keep their D124 meaning (grow with the items, scroll past the cap).
      Text that wraps is a `Label`'s, or the own text of a Button, a
      Check, a Radio or a Toggle with `wrap=true`.
      - **The min/max band** (D135): `width=hug, min_width=100,
        max_width=300` clamps the natural size into the pair — the floor
        holds below it, the cap binds above it (wrap-text wraps there),
        and the floor beats the cap (the WPF clamp order above; a runtime
        `$param` floor crossing the cap wins). A literal floor above a
        literal cap refuses at build — "the pair can never both hold".
        Floors under `fill` stay legal everywhere.
      - **`max_width=fill`** (and the height twin) caps at what a `fill`
        axis would GET: the parent frame minus region padding minus
        margins — live, so a parent resize re-caps. On a fill-capped axis
        margins bind the MEASURING FRAME exactly as they would inset fill,
        whatever the alignment. On a centered axis the box is centered in
        the room the written margins leave: two margins shift it by half
        their difference, one margin by half of itself — with the fill
        cap or without (the Godot addon refused
        margins on a centered axis without the cap). The shipped-toast shape: `width=hug, max_width=fill,
        halign=center, margin_left=12, margin_right=12` — hug the text,
        center it, wrap at live width − 24. Wants a parent whose extent
        doesn't derive from this child (a hugging parent would feed the cap
        its own answer).
      - **The capped fill** (D245): `width=fill, max_width=380,
        halign=center` (`max_height=` on `height=fill` works the same) —
        the available space, never more than the cap (`size = max(floor, min(space, cap))`, same
        clamp order). Narrower frames fill edge to edge like plain `fill`;
        when the cap binds, the stated side places the box in the
        leftover; with no side stated the box is centered, as WPF's
        Stretch + `MaxWidth` does (the Godot addon required a side).
        Without a cap, alignment on a fill axis means nothing:
        `halign=center` there is refused (G1011), `left` and `right`
        build and move nothing. On a stack's own stacking axis
        it is refused (G1002: the slot is the frame there) — cap the stack
        itself. Margins inset the measuring
        space on every side choice, like plain fill. A `Scroll` takes the
        cap like any box (the Godot addon refused it there), and
        `max_*=fill` on a fill axis changes nothing — a fill axis already takes
        exactly that space (with `halign=center` it is refused, G1011). The shipped shape this exists for:
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
      `max_*=` don't combine with it (v1). Numbers solve under free
      parents; `natural` follows the picture — the Image's MINIMUM
      follows the bitmap from the driving axis (the WPF UniformToFill
      measure), under any parent, lazy pictures included.
      Nothing spelled: width drives. An `Image` with no size word and
      no `aspect=` fills what its parent gives it and fits the picture
      inside; a Column gives it no height, so there it is the
      Column's width and the picture's shape sets the height (WPF
      Uniform in a vertical StackPanel) — never 0 tall. `hug` on an
      `Image` side (the written word, not an alignment) is the picture's
      own size on that side: `size=hug` (= `width=hug, height=hug`) = natural size, one
      picture pixel per layout unit.
    - **`fit=contain/cover/distort`** (D133, `Image` only): how the bitmap
      maps into the Image's OWN rectangle (in `Column { Image width=96,
      height=48 }`, that 96x48 — never the Column). Orthogonal to how the
      rectangle is sized, so `aspect` and `fit` compose with no joint
      rules. `contain` letterboxes (the default), `cover` fills and clips,
      `distort` stretches — named for what it does; the CSS/WPF word
      `fill` is gogo's sizing word. Slack centers (v1, no alignment knob).
      On non-content nodes `fit=` is refused: children are laid out,
      never scaled.
  - **`visibility=visible/space_only/gone`** (D133) — the three-state
    triad, replacing the retired bool `visible=` (a bool can't spell
    three states; every variant named, the Android triple). `gone` hides
    and RELEASES the space; `space_only` hides but the node keeps holding
    its layout slot — toggling `visible <-> space_only` never moves a
    sibling (WPF Hidden). Hidden is hidden either way: hit-inert and
    unfocusable; `on:show`/`on:hide` track the PIXELS, so
    entering `visible` from either hidden mode plays the show.
    `space_only` holds its slot under any parent. Code flips visible/gone
    with `Runtime::set_visible`.
    - **A written size is a hard edge** (GPAR37): a box with an exact size
      is exactly that size. Content that needs more hangs out and is cut
      at the edge — neither drawn nor pressable past it. (The Godot addon
      grew the box to its content and warned, D126 — a recorded
      difference; the Rust compiler warns only for a 1x1 box that clips
      its children, G0074.) Remedies: put the exact size on a Scroll
      around the content, `hug` the axis, or shrink the content. A part
      that is MEANT to hang off its box is `overlay=true`.
    - **On `Scroll` the axes settle from three sources** (D138; D125's
      triad rule grown up): `allow=vertical|horizontal|both` names the pan
      axes outright and owns BOTH axes when present (listed axes scroll,
      the rest are off — the fill-width page that must not pan sideways).
      Absent, the triad infers: a `hug` axis is content-sized — nothing
      left to scroll; numbers, `fill` and unkeyed axes scroll. `bars=auto|never|always|reserve` paints
      the scrolling axes' bars (`never` = hidden but panning, the handheld
      look; `reserve` = auto visibility with the bar's space always held).
      A capped hug axis scrolls PAST its cap (D124 generalized): `height=
      hug, max_height=300` follows content to 300, then scrolls
      (`fill` already scrolls past the frame).
      The contradictions are build errors (G1020, decided 2026-11-11): a
      side that pans and hugs with no cap, named in `allow=` or aligned
      with no size word (the Scroll would be 0 long on it: a side that
      pans asks its parent for no room); a capped side left out of
      `allow=` (what lies past the cap could not be reached); a hug side
      capped with `fill` (it would grow with its content and never
      scroll). Both sides hugged bare is a warning (G1021): nothing
      scrolls. `Scroll
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
    an aligned axis (a far-edge margin moves nothing but still counts in a
    hugging parent's size; on a centered axis the box is centered in the
    room the written margins leave, so a lone margin shifts it by half:
    `halign=center, margin_left=20` sits 10 right of the middle, and an
    Image centered by its kind's default shifts the same way — none of
    these is an error, where the Godot addon refused them), PINS of an unaligned
    `hug`/exact axis (pin ± size; grow points away from the pin). The
    `margin=N` shorthand is breathing room: insets all around on filled
    axes, the touched edges under alignment. Two per-edge keys bind both
    edges (a declared size there over-constrains: G1006, for the width
    and for the height); the document root is
    placed like any box, so a lone margin there insets it.
    `anchors=` and `offset_*` left the language — placement says it all
    (removed-key errors carry the migration hints).
  - Coherence is validated on the merged style+node+instance state:
    `halign=center` with `width=fill` errors (G1011: fill leaves nothing to
    position; `valign=center` with `height=fill` the same; the other sides
    there build and move nothing); `size_flags_horizontal` and
    `size_flags_vertical` are gone (G0041) and `inset=N` is not a word
    (G2001) — the fill default plus `margin=N` say the same thing, and the
    document root fills the host's rectangle with no size word. Margins under a Container parent are
    build errors (G1014: they'd be silently dead). "Container" here means the kinds that deal each child
    its place — the stacks (Column, Row, Choice), Flow, Grid, Tiles, and a
    `List` with its rows. A `Scroll` is NOT one (decided 2026-08-30): it
    holds one pannable body, and margins on that body inset it like
    margins anywhere free, and scroll with it. `padding` is legal on ANY
    container — air inside a Column or a Scroll is as sayable as air inside
    a Panel (the retired addon refused it there; recorded difference).
  - `Row`/`Column` stack their children on x / y.
    Stacks have NO default gap (D244): children touch unless the file says
    `spacing=` (or a one-off `gap`) — the same law as the blank Panel
    slate. Flow and List rows follow it.
    `Layers` is free placement + z-stack:
    children overlap in declaration order and free-place themselves; it
    reports a min size (element-wise max of visible children's minimums
    plus their positive margins, D242)
    and, like all passive structure, is input-transparent (see Input —
    `interactive=true` declares it a gesture surface). The full type surface — View,
    Flow, Panel, Scroll, Label, Input, Circle, Gradient, Radio, Check and
    friends — is the Vocabulary section below (and REFERENCE.md's table).
  - `gap <epx N | fill>` — the stack-space structure word (Row/Column only):
    a one-off break where `spacing=` is uniform. Lowercase = never named
    (capitalization law). No name, props or children; one size word; errors
    route wrong homes (Flow and Grid → spacing=, Layers → margins).
  - Safe-area insets are a VALUE, not a wrapper (D74): `padding_left=safe` /
    `margin_top=safe` read the host-fed inset for that edge
    (`Runtime::safe`: left, top, right, bottom, in epx); `padding=safe`
    / `margin=safe` are defined sugar for the four per-edge keys, each edge
    its OWN inset. Only edge keys read it — `safe` on an edge-less key is an
    error. Opt-in per edge: name only the edges the element
    reaches. On scroll CONTENT the inset scrolls (rows clip at the physical
    edge); framing the viewport it doesn't.
  - **Buttons size to their content** (WPF parity). A button with
    children derives per axis: `min = max(declared min, content min)`,
    content = the Layers formula (visible children's combined minimums plus
    their positive margins — declared empty space is space the parent
    reserves, so `margin_left=49` widens the derived minimum exactly like
    `padding_left=49`; D242). Negative margins mean "stick out past the
    edge on purpose": the hanging part comes OFF the claim, clamped at
    zero (`100 wide, margin_right=-30` claims 70; `150 tall,
    margin_bottom=-70` claims 80 — WPF/CSS do the same; decided
    2026-08-30. The Godot addon counted them as zero). A margin on the far side of an
    aligned axis moves nothing but still counts (`halign=left, margin_right=49`
    widens the button by 49); it is not a build error. An exact
    `width`/`height` clamps its axis like WPF's explicit Width;
    both-axes-exact buttons are exempt (nothing left for content to grow).
    Live: child min/visibility changes re-derive.
- **Widget body names are per-instance**: nodes named inside a widget body are
  visible to the widget's scoped anims/styles/states, and a widget used twice
  never collides. In the file the instance name means the root; code reaches
  the body through the instance (a build generates one `<Widget>Names`
  struct per widget).

## Scoping for `$refs`

Innermost first: widget parameters shadow theme values. Undefined `$x` = resolver error
listing what *is* in scope.

Code reads theme values by a typed handle (2026-09-27): the build writes a
page's `$name`s as `enum Theme` (`Theme::Accent`), and
`Runtime::theme_color(page, Theme::Accent)` resolves it the way the file
does — active variant first, `$refs` followed. Hidden semantics made
visible: no hex copied into code, no color read off a node that happens to
wear the accent.

## Handler names

An event names a handler. With `on:<event>=Name` (or `emit(Name)`) the
handler is `Name`; with no `on:` line for the event it is the node's own
name (`saveButton`), so a named control is heard with no wiring at all.
One name serves all of a node's events: the event's kind (`Activate`,
`Change`, `Edit`, ...) says which one happened.

A build generates one `enum Handler` per page, with a variant per handler
name (`saveButton` becomes `Handler::SaveButton`, `HandleOk` stays
`Handler::HandleOk`) and `Handler::from_name`. Code that matches on a
handler the file no longer has stops compiling, so a renamed node can
never silently lose its handler. The names that get a variant: every name
an `on:` line writes; every named Button, Check, Toggle or Radio with no
`on:activate`; every named Slider or Input with no `on:change`; every
named `choice` with no `on:change`. A name inside a widget body or a
repeat template gets none; a widget instance whose body marks a
`signals` node gets one under the instance name.

(The Godot addon wired events to host methods named `On<Name><Event>` and
checked at load that they existed. The Rust crate has no such methods.)

## Error contract (diagnostics, D68)

Every compiler error is a **diagnostic**: a severity (error or warning),
a stable code, the line, and a message (`gogo_ui::Diagnostic`). When a
closed set is known (node types, style names, property names, theme
values, parameters, states) the message carries a did-you-mean
suggestion; with no close candidate it lists the closest available
options.

**Message voice** (D84): messages are lowercase sentences. Code fragments
in prose — property spellings (`` `areas=` ``), values (`` `fill` ``), type
words (`` `Panel` ``), keywords, whole example lines — wear `` `backticks` ``
(rustc's convention): backticks say "type this".

Two output shapes:

- from a build (`build_pages`), one line per problem:
  `<path>:<line>: <severity> <code>: <message>`
- a diagnostic printed on its own:
  `<severity> <code> line <line>: <message>`

```text
ui/menu.gogo:3: error G2001: `anim=` is not a gogo word — timing belongs on a `transition` line (`transition any -> held duration=0.1`); a `when` holds standing values
```

**Codes** (`G` + 4 digits) are stable search keys: never renumbered,
never reused (`diag::RETIRED` holds the numbers that must not come back).
Every code has a teaching entry in `docs/gogo-errors.md`, and a test holds
the registry (`diag::REGISTRY`) and that file together both ways.
Banding: G0xxx lex/parse · G1xxx placement · G2xxx props/values · G3xxx
styles/when/transition · G4xxx anims · G5xxx widgets/uses · G6xxx
theme/$refs · G7xxx view root · G8xxx translations · G9xxx repeat and List
(G91xx: a write from code that was refused; the code rides the error the
call returns).

(The Godot addon's diagnostics also carried source spans, notes,
mechanical fixes and a JSON form for its editor panel. The Rust crate has
none of those: a diagnostic is the four fields above.)

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
  Merge order: at rest the node's own props beat the style's props; while
  a state holds, its `when` beats every base prop, and the node's `when`
  for that state beats the style's, whichever file the style is in.
  Between two styles the one worn later on the node's line wins, the
  rule their plain props follow. A plain prop on the node never
  switches a style's response
  off — to opt one node out of a style's `when held opacity=0.5`, write the
  same state on the node: `when held opacity=1` (node beats style at equal
  rank). Opting out is written, never implied by an unrelated `opacity=1`.
- Priority when several states hold: arrangements < focus_within <
  focused < hovered < selected < held < disabled. Re-entry cancels the
  opposite tween and starts from current values.
- `focused` is VISIBLE-FOCUS (D129, semantics unchanged by the D181
  respelling) — focused AND an indicator is warranted: focus arrived by
  key, or a key was pressed while focused; a
  pointer press anywhere turns the indicator off. A tap moves focus
  without showing it, so no ring is stranded (the D122
  press-residue class, generalized). Caret exception: a focused `Input`
  always indicates. One spelling, one meaning — there is no raw-focus
  state; code asks `Input::focused()`. This is the CSS :focus-visible
  model given the short name (a fresh language skips CSS's legacy pair).
  Buttons ship a default response, a ring, as an injected default
  `when focused`; authoring `when focused` replaces it, per the D122
  doctrine.
- `focus_within` (D129) holds while the focus is on the node or any
  DESCENDANT (CSS :focus_within, WPF IsKeyboardFocusWithin) — the
  container highlight for key-driven UIs (a settings row lights
  while its inner slider is focused). Visible-focus flavored like
  `focused`; the one non-participle spelling, kept on purpose — it names
  a relation, not a condition of the node.
- `hovered`/`focused`/`focus_within`/`disabled`/`selected` are UNIVERSAL — legal on any node,
  dormant until a provider flips them (`disabled=` on a passive node is a
  styling flag: the node dims to 0.3 opacity by itself, an authored
  `when disabled opacity=` replaces that dim, `interactive=` stays the
  input switch; `selected` is
  "this one is chosen" — a toggle-mode button holds its own, choice/Radios
  carry it, a `List` flips it on the chosen row's `ListItem` — D118).
  `held` builds on any node too, but only a press surface raises it — the
  Button family (templated
  Radio/Check and toggle mode included), a List row's `ListItem` (the
  List's press tracking flips it, D121), a `Slider` (the drag brackets
  it), or a `control` declaring `state_builtin held` (the runtime's
  gesture tracking maintains it — behaviors never flip it; D181). On a
  `control` instance a `when` may name only the built-in states its
  contract opts into with `state_builtin` (G0036 otherwise). Code
  flips `selected` and `disabled` with `Runtime::set_state`, and the
  restyle lands on the next frame. A mouse
  press CAPTURES hover: slipping off the control mid-press keeps the
  hovered look until release.
- **`disabled` CASCADES** (D133 — the WPF IsEnabled law): effective =
  own flag OR any ancestor's own, and a child cannot re-enable under a
  disabled ancestor — no opt-out. Two layers: re-enabling the ancestor
  restores each descendant's OWN flag (a child disabled in its own right
  stays disabled); rows arriving inside a disabled subtree (a repeat's
  or a List's) inherit on entry. `when disabled`
  fires on the EFFECTIVE value, so gray-out styling cascades with it.
  `disabled` is the only state that flows down the tree (`focus_within`
  and `hovered` look up: each holds on the focused or hovered box and on
  every box above it). `Runtime::disabled(&page, node)` asks
  for the effective value.
- `ListItem` ships DEFAULT `selected`/`focused`/`held` responses (D122,
  defined sugar — doctrine unchanged by the D181 respelling): a bare row
  fills with white at 0.3 when chosen ("Default look"), a
  quarter-strength echo under the cursor, a half-strength fill while the
  press is held — the default is the base look, `when` overrides. An authored `when <state>` on the row replaces that state's
  default entirely, per state; `select=off` rows skip only the selected
  default. `hovered` stays opt-in. Two feel rules (D122 follow-ups): the
  default press shade mirrors "release here would tap" (slide-off clears,
  slide-back re-arms), and the cursor echo draws only for key-driven focus
  — since D129 that IS the `focused` state's one meaning (visible-focus),
  so authored `when focused` behaves identically and the row-local
  `kb_only` gate is gone.
- Friendly response props: `opacity`, `background`, `font_color`; a
  node's own look words (a `Rectangle`'s/`Circle`'s `color`) work too.
  `color` on a node without one is an error routed to
  `font_color=`/`background=`/`opacity=` (known dialect words
  `foreground`/`text_color` route the same way).
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
  property a `when` also holds is a build error. A node line or a style
  may declare the motion words too (`rotate=45`): that is the node's
  resting transform, and a `when` response replaces it while its state
  holds.
- Timing keys on a `when` line are an error: how a change feels is a
  `transition`. The retired spellings (`state` directive, bare `pressed`
  blocks, `enter`/`exit` blocks) are refused as unknown words: G7011 in a
  node body, with no replacement named; G0026 in a style block, which
  says a style holds `when` and `transition` lines.
- A style may respond to a declared arrangement (`when expanded ...`): every
  wearer follows the VIEW's flip, with the view's transition timing.

### Arrangements

`arrangement <name>` (top level) declares one configuration of the view's
EXCLUSIVE set — its enum; `base` is predeclared (the declarations themselves)
and `any` is the transition wildcard (both reserved). Declaration only:
responses live in top-level `when <name>` blocks (dotted `node.prop=value`
pairs from the view root), timing in `transition` rules. Placement keys
(the margin and padding sides, `width`, `height`, `size`, the floors
`min_width`, `min_height`, `min_size`, the caps `max_width`,
`max_height`, and `halign`/`valign`) compile per
arrangement into the target's placement (validated at build); other props
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

One reader for placement words (decided 2026-11-11): a placement word
written in a view-level `when`, from code or on an anim line is read by the
function that reads a node's line. So `side.width = fill(2)`,
`box.max_width = fill`, `box.min_size = 200x100`, `box.width = $panel_w`
and `bar.margin_bottom = safe` mean in a `when` what they mean on the
line. The later word is the whole answer for its side: a number written
over a margin or a padding side that the line declared `safe` drops the
inset (`bar.margin_bottom = 0` puts `bar` on the screen's edge), a number
replaces a `fill` cap, and `fill` replaces a number.

Size keys in a pose (D249): a `width`/`height`/`min_width`/`min_height`
written in a `when` block behaves exactly as if the file had declared it,
for as long as the block applies — when the block stops applying, the
declared value comes back. That includes the minimum size the box reports,
so a pose CAN shrink a node below its declared size. The values compose like declarations too: a
posed `width` still loses to a declared `min_width` floor (the floor
clamps every posed value — WPF's MinWidth-over-Width order), a posed
`min_width`/`min_height` replaces the declared floor, and content still
sets a button's non-exact axis above any floor. On a TIMED flip the box's
edges tween and the boxes around it follow on every frame,
so motion is smooth in both directions. A declared
`min_width` next to an exact `width` on the same line builds only where
a `when` writes that width on the node (otherwise G1019): there the
floor bounds the posed values, and it clamps the declared size by the
same order.
Flipped with `Runtime::go`, `on:<event>=go(name)`, or
`on:<event>=toggle(name)` — toggle is one comparison of sugar (D92):
`go(active == name ? base : name)`, the disclosure button without a handler
(from a DIFFERENT pose it goes to `name`, checkbox-style). `toggle(base)` is
a build error (G0050: base is home — `go(base)` returns there). Keep such
a button momentary: `toggle=true` (button toggle MODE) next to
`toggle(pose)` is two separate on/off states that drift once anything else
flips the pose. The verb set is closed: `play`/`go`/`toggle`/`emit` (cycling through
poses stays code-behind).
The flip tweens with the matching rule; the ACTIVE pose holds across
rotation and hot reload, and hot reload keeps the active arrangement.

### Screen classes and orientation (`when screen.<class>` M15, `when screen.<orientation>` D265)

Two more pose axes, orthogonal to arrangements: the SCREEN's size class
and its orientation. The host program names the active values
(`Runtime::set_screen`, whenever the screen changes size); `when screen.landscapeSmall { node.prop=... }` and
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
from BOTH would be decided by a rank the file never states — build error;
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
`halign`/`valign`: a posed alignment key replaces the declared alignment
word and nothing else — the declared margins and size stay (a box
declared between `margin_left=20` and `margin_right=30`, posed
`halign=right` and `width=50`, sits 30 from the right edge). An
arrangement `when` takes them too, and under a `transition` rule the box
glides to its new place. Container children take size and alignment
poses: numeric
`width`/`height`/`min_width`/`min_height` behave exactly as if declared
for as long as the condition applies (D249 — a posed `width` makes a fill
child stop taking the leftover; the drop restores the declared axis,
instant — nothing to tween), and a Column child posed `halign=right`
moves to the column's right edge, while a posed margin on a container
child stays a build error (G1014: the parent seats its children). The active
class holds across hot reload. The blocks may sit at the top of the file
or in a node's body.

Composed blocks hold only while BOTH their conditions do, for poses
neither axis owns alone. Three shapes: `when <arrangement>
screen.<class>`, `when <arrangement> screen.<orientation>` (arrangement
first — a reversed spelling is a build error naming the fix), and
`when screen.<orientation> screen.<class>` (orientation first, class
second — the sanctioned intersection of the two screen axes). One rule
decides every contested key, and it reads off the file: **a block that
needs more conditions wins** (a pair beats any single), and **at equal
count the later line in the file wins** (CSS's rule — put the block you
want to win lower). So a pair's keys beat its halves on a contested
property, two active pairs settle by file order, and either half dropping
releases the block back to whichever single-axis pose still holds.
Alignment keys work in a pair as in any other `when`: where a
`transition` rule times the arrangement the box glides to its new place;
the screen-first pair has no arrangement half and is always
instant. A pair may sit at the top of the file or in a node's body, and a
named arrangement must be declared in the file.

Seat moves — the one `when` response that REPARENTS. `seat <name>`
declares an empty socket in the tree (lowercase like `slot`/`gap`:
sockets and spacers, never painted controls); `x.seat = <name>` in a
`when` moves the declared node into it, instantly, and
dropping every condition returns it to its declared home — parent AND
position. Legal from every view-level `when` — screen whens, composed
blocks and arrangement whens, with or without a `transition` rule: the
node changes parent at once (it is in one parent or the other, never
partway), and under a timed arrangement its box glides from the old
place to the new one.
(The Godot addon refused a seat move in a timed arrangement at build,
G3003, as it refused the alignment keys above, G3004; the Rust crate
takes both.) Contested seats follow the
placement rule: more conditions win (a composed pair over a single),
equal count → the later line in the file.
`x.seat = home` says the return AS a move: sit at the declared
home while THIS condition holds — for the more specific block that must
pull a node back out of where a less specific one seated it (`home` is
reserved; `seat home` as a declaration is a build error). The socket hugs
to zero alone, to the occupant's minimum when
occupied (occupant fitted to the seat's full rect). That minimum reads
wrap-text by the occupant's own DECLARED width mode: a `width=fill` or
exact-width (`width=N`) occupant measures ORDINARY on both axes (wrapped
— fill asks the socket for size, a number IS the box, there is no hug to
honor), while a hug occupant measures wrap-text UNWRAPPED like every hug
axis (D254 — the crop hint's landscape row). A posed width counts as an
exact width for as long as its block applies. A hug occupant whose
single-line text is wider than the window is the width-feedback trap —
the socket can never fit any pane (the fix is `width=fill` on the
occupant). The rules the compiler checks: a seat has a name and no child
of its own (it may take size and placement words — `seat side width=200`
is a socket 200 wide, empty or occupied); a seat and a resting home
may sit under any parent, a free one included; one occupant per seat
under ONE condition (claims under different conditions build — two
arrangements, or an arrangement and a screen class, may name the same
seat; while both hold, the placement rule picks the occupant: more
conditions win, equal count → the later line, and the other node stays
at its declared home); no moves into a node's own subtree.

The smooth-slide primitive: the node's LAYOUT SPACE as a 0..1 fraction.
`visibility` is a word and words snap; making the space a float puts it on
the same rails as every other response — poses flip it, `transition` rules
tween it, anims may drive it. The
node's box itself shrinks to the fraction, and what the box holds — its
children, its own text — is cut at the part that shows and takes no
touch outside it. The rules:

- Under a container (`Column`/`Row`/`Grid`/...) the node takes only its
  shown fraction of space, and the siblings close up as it shrinks (a
  disclosure collapse).
- A hug axis shows the fraction of its own measure; a `fill` or exact
  axis the fraction of its size. The part shown starts at the edge the
  box is aligned to, whatever its size word: a Rectangle with
  `reveal_x=0.3` in a 100-wide `Layers` is 30 wide at the left, with
  `halign=right` at the right, with `halign=center` in the middle.
- What the box holds keeps its size and travels with the box: it is laid
  out from the corner where the shown part starts. A sheet aligned to
  the bottom shows the top of its content and carries it out through
  the bottom as it closes, with `height=hug` and with `height=200`
  alike; a box with no alignment keeps its content in place and closes
  over it from the far edge.
- `reveal_y=0` is not `visibility=gone`: a concealed node stays ALIVE (anims
  run, states poll), it just earns no space. `visibility=gone` remains the
  "gone" switch and zeroes the space instantly.
- The channel exists only where DECLARED: a `when` or anim track driving
  `x.reveal_y` without `reveal_y=` on `x`'s own line is a build error
  (the resting value is visible in the file).
- Any parent takes it. (The Godot addon wrapped the node in a clipping
  proxy and allowed it under containers only — a recorded difference.)

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
| `activate` | the control's OWN trigger gesture completed — Button: a completed tap (slide-off clears, slide-back re-arms) or Enter/Space; List: tap or Enter/Space on a row. Never "fires on release" as a universal law; never fires disabled | — (List: the event's node is the row; `Runtime::item_key` gives its key) |
| `change` | the committed value is now different — once per commit, genuine transitions only ("changed means CHANGED": re-pick repairs, never re-reports). Events come from input: a write from code reports nothing | the new value: Check/Toggle a bool, Slider a number, Input the text, `choice` the picked Radio's name; for a List read the keys with `Runtime::selected_keys` |
| `edit` | a live, UNCOMMITTED value moved — Slider per drag tick, Input per keystroke that changes the text; discrete controls have no edit | the live value |
| `press` / `release` | the finger landed on a press surface, and that gesture ended — `release` fires exactly once however it ends (lift inside or out, cancel, a Scroll steal, disable/hide under the finger). A Slider brackets its drag with the same pair: `press` on down, `release` on up or cancel, one each. `activate` stays a press surface's completed tap; a Slider never fires it | — |
| `show` / `hide` | visibility choreography (unchanged) | — |
| `pointer` | every touch phase on a box the host draws itself (the node needs `interactive=true`, G0073) | the phase and the position inside the box, epx |

Per control: **Slider** brackets its drag with `press`/`release`
(exactly one each, cancel included), streams `edit` while dragging
(`held`) and commits `change` on release; a key step commits directly —
`change` only, no `edit` (an `edit` landing in the same
instant as its commit adds nothing). **Input** streams `edit` per
keystroke that changes the text; `change` commits on Enter or focus
loss. **Toggle**:
`selected` flips LIVE as the thumb crosses the midpoint (state =
presentation); `change` fires on settle, only when the settled
side differs from the pre-gesture side — wiggling across the middle is
zero changes. **choice**: `change` lives on the GROUP (the value is the
picked Radio's name; items refuse `on:change`). **List**:
`change` on the List, then `activate` on the row; under `select=off`
there is no selection to change, activation still fires.

**The set is closed.** Any word after `on:` that is not one of the events
above is refused (G0049), with the list; a few old spellings are answered
with the new one (`on:pressed` "is spelled `on:activate`"). The Godot addon also took raw
engine signals there (`on:drag_ended`, `on:mouse_entered`); the Rust crate
has no such register, and a `control`'s own declared signals
(`signal drag_started`) reach code as `Signal` events instead of through
`on:`.

**Retired state spellings** are refused with the new word (G0035):
`pressed` and `dragging` are spelled `held` (the drag state of a Slider or
Toggle IS `held`), `hover` is `hovered`, `focus` is `focused`, `checked`
is `selected`. Any other unknown state word is G0036, which lists the
real ones.

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

```kdl partial
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
  `rotate` (degrees) and `opacity` are a transform the renderer applies
  (visual-only; each may also be declared on a node line as its resting
  value). Sizes, margins and paddings are not on this channel: an anim
  that moves one moves the box itself (prop lanes, below). Alignment
  words are not animatable.
  `pivot=` — a 0..1 box
  point (`pivot=0x0`, the Gradient `from=` spelling) — places the fixed
  point `scale` and `rotate` transform about; unset = the center, the
  old behavior verbatim. A look word on every box (`when` responses may
  restyle it), not itself animatable; a full 0..1 pair or the build
  refuses. Hits follow every channel, rotation included — the
  finger un-rotates about the nearest rotating ancestor-or-self, so
  look and touch agree.

- **Prop lanes and the color-mixing law**: any word a `transition` rule can move is an anim target
  too — `b.background to=#FF3B30FF`, `card.corner_radius from=4,
  to=12`, `card.width to=200`, the live `stop_N_color`/`stop_N_offset`
  family. A lane on a size, a margin or a padding moves the box itself,
  as `transition card.width` does: the box and what is around it are
  laid out again on every frame of the play. `scale` and `translate.x/.y`
  move a box with no new layout; `reveal_x/_y` changes the room the box
  takes, so the boxes around it are laid out again as it moves. `.a` on
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
  `color-mix()` made the same default). (The Godot addon lerped per byte —
  a recorded difference.)

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
  carries `and`/`then` — or, on a `group` line, `after=<group>` INSTEAD
  (one placement per step; a connective plus a reference is an error,
  never a precedence rule; `after=` on any other step is G0062: wrap the
  step in a group). `then` waits for every end in the previous `and`-cluster,
  autoreverse's way back included. `after=` is the crossing-edge escape
  hatch: only named `group`s can be referenced, names are anim-flat
  (forward references legal, wrappers reachable-into), unknown names and
  cycles refuse at build. `delay=N` is a PAUSE step, always positive; a
  shift is `offset=`, on any step but a pause (signed; negative overlaps,
  the final start clamps
  at 0). `play <anim>` inlines another anim as one step at bake (speed
  folded in, internal names private, `repeat=forever` refused — it never
  completes). The old `track` block and the delay ATTRIBUTE retired with
  this grammar (big bang, no legacy machinery).
- `each <node|.Style> [stagger_by=N]` names wearers (a `.Style` = every
  wearer in the page, in file order — a wearer before the wearers
  inside it; `stagger_by` shifts
  successive copies); its body is a step chain in BARE paths (the each
  names the target — prefixing is a routed error; dotted paths belong at
  anim/group level). With `stagger_by` > 0 the each's length only exists
  at play time, so nothing may sequence after it — refused, not guessed
  (`and` beside it is fine: that shares its start).
- **Capture rule**: `to=` alone starts from the LIVE value when the step
  begins; `from=` (or a key at t=0) pins the start. On the first step
  that moves a property the pin HOLDS from play start (staggered
  entrances never flash their base); on a later step it is where that
  step starts. Two pinned steps on one property that run at the same
  time = build error. **`from=current`** pins the live value
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
  word or a look property (`background`) while a `transition … anim=` in
  the same scope drives the same one
  on the same target — is refused at build (G4003: "two writers would fight
  on every flip; the loser flickers"). `modulate.a` is refused, naming
  `opacity` (G0075).
- Key line: `t=`/`v=`/`easing=`/`jump=true` (snap, no interpolation).
  `duration=0` sets instantly. A keyframed line's duration IS its last key's
  `t`. Numbers on length properties are epx.
- **Easing names** (WPF-exact math):
  `linear`, `sine`, `quad`, `cubic`, `quart`, `quint`, `circle`, `expo`
  (`exponent=`), `power` (`power=`), `back` (`amplitude=`), `bounce`
  (`bounces=`, `bounciness=`), `elastic` (`oscillations=`, `springiness=`)
  × `_in` / `_out` / `_in_out`. A bare base name means ease-out.
- From code: `Runtime::play` (a newer play of the same lane takes it
  over), `Runtime::stop` (with `restore = true` every property goes back to
  what stands without the anim — a code write or a `when` in force, else
  what the file says — like WPF Storyboard.Stop), `Runtime::is_playing`, and
  `Runtime::take_finished` for the anims that ended on their own. A build
  generates an `enum Anim` per page for these calls; the `_id` forms serve
  a page compiled while the program runs.
- Declarative triggers: `on:show=play(appear)`, `on:hide=play(disappear)` —
  `on:hide` plays first and hides on finish (deferred hide), then restores the
  base look; showing during a hide cancels it. A control that is already
  visible when its page first shows plays its `on:show` once as an
  ENTRANCE (the host asks for it once per page: `Runtime::play_entrances`);
  initial `visibility=gone` stays state, not a trigger (the show plays on
  the flip). Every other event also takes `on:<event>=play(x)`,
  `on:<event>=go(arrangementName)` (flips the view's arrangement;
  `go(base)` returns home) or `on:<event>=toggle(name)` (flips between the
  pose and base, D92); `on:show` and `on:hide` take `play(…)` only
  (G0084). From code, `Runtime::show` goes through the
  `on:show`/`on:hide` pair at once, and `Runtime::set_visible` goes
  through it on the next frame: any change of a node's visibility plays
  the pair (a code write, a `when`, a pose), and a hide waits for its
  `on:hide` anim. A node with no `on:hide` anim hides at once.

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
  Rows are not drawn and not named: in the compiled page each one is a
  plain `row` node between the Grid and its cells (a cell's parent is its
  row), and anim paths and names are unaffected;
  no style/background/name on a row
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
  coordinates, ever. A span past the last column/row is a build error (WPF
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
  is one contiguous rectangle; children bind by name (unknown name = error;
  a nameless child is an error; an UNBOUND area is only a
  warning — sketching the layout first is fine).
- **`cols=`/`areas=` are the Grid's LIST consumers**: entries are ordinary
  values (epx numbers, idents, `fill(n)` calls, `$refs` — theme-driven track
  widths work), newlines free inside the brackets. Besides them only
  Gradient's `stops=`, `border_dash=`, an Arc's `dash=` and an Image's
  `window=` take a list; every other prop refuses one (G0040).
- Uniform flowing mosaics (N thumbnails a row, spans, full_row headers) are
  `Tiles` (below, D94) — NOT a Grid form, because auto-placement would
  contradict this section's core promise. Data-driven repetition (calendars,
  tables) is `repeat`'s job (see Resolver-defined vocabulary).
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
    Panel a { Image source="res://a.jpg", aspect=natural }
    Panel wide span=2 { ... }
}
```

Rules:

- **`cols=N` is required** (the signature every `span=` is checked against —
  same explicitness as Grid's `cols=`), takes a whole number, and is LIVE:
  code may re-set it and the mosaic re-packs (list values error toward Grid).
- **Packing is first-fit wrap, advance-by-actual-width**: unit = width/cols
  (`spacing=N` puts N between the columns and between the rows: the unit
  is what the gutters leave, and a cell of two units covers the gutter
  between them);
  a cell is `span=K` units (default 1), `full_row=true` the whole row
  (exclusive with `span=` — a full row IS the whole span), or — an exact
  `width=`, written or set by code — its own declared size, packed at its own
  pitch. A cell that doesn't fit the row's remainder wraps whole; no
  backfill, the tail stays empty. The wrap test carries a 0.01 epsilon so
  float unit sums never push the last column out.
- **Heights are content truth, not lattice quanta**: a cell is as tall as its
  own minimum, top-aligned; the row is its tallest cell. Bitmap-driven
  tiles say `aspect=natural` on the Image — the layout derives
  height from width by the picture's ratio (a box-`aspect=` prop was
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
- **Width feedback**: minimum height depends on granted width (the
  wrapping-Label idiom). Minimum WIDTH is the widest cell's OWN minimum:
  a zero minimum would deadlock under hugging parents, and a
  lattice-scaled claim (content min × cols/span — the first attempt) exploded
  with the column count and pushed content past the viewport (user-caught on
  device). Oversized cell content never widens the mosaic. The remedy
  belongs to the content: `clip_text=true` on a Label, a
  `clip_contents=true` wrapper otherwise, or a wider cell.
- `span=`/`full_row=` are CELL props of direct Tiles children only (styles
  can't place cells, G1002; widget instance lines may carry them — they
  stamp the widget root).

**Rejected alternatives**: `cols=` on Flow (grid tracks in a Flow costume —
it would silently change Flow's natural-size contract); a Grid auto-flow
form (contradicts *placement is structure*; three placement forms in one
type multiply cross-form validation); strict WP unit HEIGHTS (`span=2x1` —
the shipped catalog's rows are bitmap-ratio heights and code-sized squares,
neither on the height lattice; content-truth heights subsume both);
`aspect=` cell prop (above — affine drift); per-cell auto-clipping (hidden
node levels break tree-is-what-you-write, and silently swallowed overflow
is how parity bugs hide).

## Atlases (designed and built 2026-09-27)

An `atlas` line names one picture and how to cut it into **slices**;
`source=` shows a slice. One keyword, four forms:

```kdl partial
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
  a whole-picture atlas with `edge=` is one slice covering the picture.
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

The type surface is **framework-neutral**: one name per concept, answering
to the goals rather than to muscle memory from WPF or an engine (`Label`,
`Panel`, `Scroll`, `Flow`, `Input`, not `TextBlock`, `Border`,
`ScrollViewer`, `WrapPanel`, `TextBox`). Full table: REFERENCE.md
"Vocabulary". An unknown type word is a build error with a suggestion
(G7011); there is no alias table.

- **`View`** — the SCREEN root (D104): the host program lays it out in the
  rectangle it passes, and its children place FREELY. Its defaults (no
  background, fill both axes) apply only where the file doesn't speak —
  anything spelled wins. Root-only (inside the tree: free placement is
  `Layers`, a styled box is `Panel`). A COMPONENT file — a self-contained
  box like a toast or a progress track — opens with `Panel` instead (D105):
  the root IS the styled box, blank until the file gives it a
  `background=`. Any other root type is a build error naming both
  doors (G7003). NOT hidden by default: `visibility=gone` is state, never
  identity.
- **`Circle`** — discs and rings; radius self-derives from the box.
  `color=`/`border_color=`/`border_thickness=` are its words; transparent
  fill + border = ring. True ellipses stay out of v1.
- **`Gradient`** — generated gradient fills, drawn per pixel:
  `fill=linear|radial|conic` × `repeat=none|repeat|mirror` ×
  `stops=[...]` (REQUIRED — no hidden ramp) × `angle=`/`from=`/`to=` aiming.
  Stop rules: `@positions` are all-or-none, never decrease (equal = hard
  step), and under mirror stop at 0.5. One stop = G2003 (a solid is a
  `Rectangle`). Stops stay live as `stop_N_color`/`stop_N_offset`
  properties — whens/anims drive them.
- **`choice` / `Radio` / `Check`** — one-of-N with a lexical home: `choice`
  is erased structure (children parent flat); every `Radio` descendant joins
  one exclusive group. A
  Radio with a BODY is the TEMPLATED form (D142): the body is the whole
  look on a blank plate (the focus ring kept), `selected` + whens carry
  the selection visual, and `text=` errors (letters live in the body's
  Labels). `value=` pre-presses by name; a named choice reports its pick
  as a `Change` event naming the choice (`on:change=` / `emit()` name
  another handler) — and changed means CHANGED: one report per genuine
  transition (re-tap of the current pick repairs, never re-reports).
  Radios need names (the name IS the reported value);
  duplicates
  error; scopes don't nest; disjoint subtrees can't share a group — by
  design. `Check` is the independent toggle and never joins a group; it
  takes the same BODY spelling (the templated Check is the D142 twin:
  blank plate, no group, `on:change` wiring unchanged).
- **Deliberately NOT types**: Divider (`Rectangle` is honest), Switch/Card/
  Chip (**types name mechanisms, styles name looks, widgets name
  composites** — the capping principle), dialogs
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

A bare control is one picture everywhere. The numbers below ARE the
default look — they live in the crate's draw constants, so a file with
no styling renders the same on every host (decided 2026-08-30).
Anything a file styles (`background=`, a `when held` response, a
`theme { Kind … }` default) replaces the matching piece and only that
piece.

Sizes are epx. `accent` and `gray` are the palette's words.

- **Button** — plate gray 0.1 at alpha 0.6 (square unless the file says
  `corner_radius=`); the label centered. No plate under a mark
  (Check/Radio draw their mark, not a plate). A text-only Button (one
  with no children of its own) puts 12 of air left and right of its
  label and 6 above and below, on each side the file does not write:
  `padding_left=9` moves the text 9 from the left and keeps the other
  three, `padding=0` is 0 all around. A
  Button holding its own content spells its own padding. The default is
  decided once, at build, so layout and draw inset by the same number
  (decided 2026-08-30).
- **Check** — a 20 box, 1.5 white outline; selected fills `accent` and
  draws a white 10 square inside. Mark, then 8 of air, then the text.
- **Radio** — a 20 ring, same outline; selected draws an `accent` 10
  dot. Same mark-then-text run.
- **Toggle** — a 44×22 pill, `gray` off / `accent` on, with a white 18
  knob at the off/on end. A bare Toggle's BOX is at least 44 tall — a
  finger target — with the pill centered in it; the drag reads x only,
  so the tall box changes no math. A tap, a key or a write from code
  glides the knob to the other end in 0.12 s (`cubic_out`); a bare
  `transition` line in the Toggle's block sets another time, and
  `duration=0` is the jump. Under a finger the knob follows at once.
- **Slider** — a 4-tall `#5A5A5A` track, white 0.6 fill to the value, a
  white 16 knob. A bare Slider's strip is 32 tall.
- **Input** — a bare Input is 36 tall; the text sits 8 inside the padding
  on the left and right,
  the caret is a thin bar at the typing point, and `placeholder_text=`
  draws at half the ink's alpha while the field is empty. Its plate is
  drawn as any box's: `border_dash=` and the four `corner_radius_*`
  words work on it.
- **List rows** — a bare `ListItem` shows selection, the press and the
  keyboard cursor out of the box: the selected fill is white at 0.3,
  a held row shades at half that, the cursor echo is a quarter of it.
  An author's own `when` for a state replaces exactly that state's
  default; `select=off` rows drop the selected one. Untouched rows stay
  transparent.
- **Scroll bars** — a `#737373` grabber in a 4-wide lane on an
  overflowing axis (Scroll and List both), as long as the viewport's
  share of the content; `bars=never` drops lane and grabber.
- **Rectangle** — colorless is WHITE; a
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

A widget or control cannot take a built-in's name (G5001): every type word
(`View` … `ListItem`), the lowercase structure words `choice`, `repeat`,
`empty`, `gap`, `row`, `skip`, `slot`, `seat`, and the directive words
`widget`, `control`, `style`, `theme`, `uses`, `allow`, `anim`, `when`,
`transition`, `layout`. `signals` is always read as the flag, and `base`,
`any`, `true`, `false` are never names. The other words gogo reads for
itself — `arrangement`, `atlas`, `slice`, the anim words `each`, `group`,
`play`, `and`, `then`, `delay`, `key`, the state words `held`,
`hovered`, `focused`, `focus_within`, `disabled`, `selected`, and the
rest of the list in REFERENCE "Reserved words" — are refused as a
widget's or a control's name the same way (G5001). The retired state words
`pressed`, `hover`, `focus`, `checked` error after `when`, naming their
replacement (G0035; respelled in D181 — see Input vocabulary). The older
`state`, `enter` and `exit` blocks get no error of their own (an unknown
type, G7011), and `state <names>` lives on as a `control` clause.
Node NAMES stay free — except `base`
and `any`, which are reserved everywhere (decided 2026-08-31): they name
state endpoints, and a node called `base` next to `transition base ->
held` would read as two things. As in Rust, `match` is never a name.
`Button base` is a build error naming the rule (G0046). `true`/`false` are
value words in the name position too (`Check true` errors naming
`selected=true`).

A node name is unique within its page (decided 2026-08-30): a name is an
address — `when x.prop=`, an anim lane, `x.seat =`, the host's lookups —
so two nodes answering to it would be a silent retarget; a duplicate is a
build error. Widget bodies and repeat items are scoped per instance
(`inst/part`, `x/i/item`), so repeated structure never collides.

## Explicitly out of scope for v1 (v2 candidates)

String interpolation (`"Size: $value"`), expressions (`$spacing*2` —
`play()`/`go()`/`toggle()`/`emit()` on an `on:` key, `fill(n)`/`hug(n)` in a
size, `round()` around a number, and `arc()`/`angle()` after
`automatic=` are the only call forms), required
parameters, block comments, general arrays (the list
kind ships but only Grid's `cols=`/`areas=`, Gradient's `stops=`,
`border_dash=`, an Arc's `dash=` and an Image's `window=` accept
one), raw/multiline strings, `Slider orientation=` (no consumer yet),
multiline `Input` (TextEdit), `Gradient resolution=`, true ellipses.
(Style single inheritance shipped as `extends=`; multi-parent composition
stays out. Named slots and `repeat` list templating shipped too: see
Resolver-defined vocabulary.)

## Example

```kdl
// styles/theme.gogo
$accent        = #E51400FF
$swatchDefault = #EB6428FF

theme dark {
    $accent = #FF6B4AFF   // active via Runtime::set_theme_variant
}

style ToolButton min_height=36, tap_pulse=true

widget LabeledSlider $label="", $min=0, $max=100, $value=0 {
    Column spacing=4 {
        Label text=$label
        Slider signals min=$min, max=$max, value=$value
    }
}
```

```kdl partial
// brush_options.gogo
uses "styles/theme.gogo"

Panel visibility=gone, halign=center, valign=bottom, margin_bottom=72 {
    Column spacing=8, min_width=240 {
        LabeledSlider sizeSlider label="Size", min=1, max=300, value=80
        LabeledSlider hardnessSlider label="Hardness", value=30
        Layers preview size=220x110
    }
}
```
