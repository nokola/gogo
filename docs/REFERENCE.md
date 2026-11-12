# Gogo Language Reference

Lookup reference for the Gogo UI language (v1).
Docs map: **[examples/tutorial.gogo](examples/tutorial.gogo)** — the language
in one commented file · **[COOKBOOK.md](COOKBOOK.md)** —
task-oriented recipes · **REFERENCE.md** (this file) — everything, organized
for lookup · **[gogo-spec.md](gogo-spec.md)** — formal grammar and design
rules.

Main goals (every language decision is measured against these): **easy to
understand, easy to write, easy to find — ergonomic, readable 6 months
later**; **Rust-inspired**: every construct keyword-announced, one keyword
per concept, hidden semantics made visible, shorthand legal only as defined
sugar.

## Vocabulary

The whole surface, one name per concept (D83 — framework-neutral; an old
WPF spelling is an unknown type: G7011, with the nearest gogo names):

| gogo | Use for |
|------|---------|
| `View` | the file's root: the screen. Defaults: no background + fill both axes (file props win) |
| `Row` / `Column` | stacking on x / y |
| `Grid` | shared tracks and spans across rows (see Grid) |
| `Tiles` | unit-lattice mosaic — cells flow and wrap (see Tiles) |
| `Flow` | wrap-to-next-line (+`orientation=`); hugging, it is as wide as its widest wrapped row |
| `Layers` | z-stack; max-child minimum; input-transparent |
| `Panel` | styled box + padding (blank canvas until told) |
| `Scroll` | touch-pannable scrolling (see Scroll) |
| `gap` | space in a stack: `gap 8`, `gap fill` — structure word, never named |
| `Label` | display text |
| `Input` | single-line text entry |
| `Image` | bitmaps; Uniform fit, centered box (see Image) |
| `Rectangle` | one cheap solid fill — guaranteed |
| `Circle` | discs and rings; radius self-derives from the box |
| `Arc` | ring segments: `thickness=` strokes, `from=`/`sweep=` degrees |
| `Gradient` | generated gradient fills (see Gradient) |
| `Button` | press / toggle |
| `Radio` | one-of-N inside `choice { }`; a body makes it TEMPLATED (see choice) |
| `Check` | independent on/off; a body makes it TEMPLATED (see choice) |
| `Slider` | value drag |
| `Toggle` | the on/off switch; its knob glides to the other end in 0.12 s, or by the bare `transition` line in its block (`duration=0` jumps) |
| `List` | selection over a `repeat`'s items — self-scrolling (see List) |
| `ListItem` | a List row's root — the selectable/focusable/tappable unit (only there, G9013) |
| `choice` | the one-of-N scope around Radios — structure word, erased at build |
| `repeat` | N children from data, reconciled by key (see Repeat) — structure word; its rows lay out as children of the repeat's parent |
| `empty` | a repeat's zero-items content, first in its body |

Structure words (`row`, `skip`, `gap`, `choice`, `repeat`, `empty`, `slot`,
`seat`) are lowercase — **capitalized = an addressable node (nameable,
stylable, animatable); lowercase = structure**. Structure is never named,
with four deliberate exceptions that need an address: `choice` (its name
is the handler its `Change` event names), `repeat` (the `set_items`
target), `slot` (the region a fill block names) and `seat` (where a `when`
moves a node). A type word that is none of these, nor a `widget` or
`control` the file declares, nor a code control the host listed
(`CompileOptions::code_controls`), is a build error (G7011).
Deliberately NOT types: Divider (`Rectangle` is honest), Switch/Card/Chip
(**types name mechanisms, styles name looks, widgets name composites**),
dialogs (patterns).

## Animating (start here)

Gogo animates in two ways, and picking is one question — **does it stay?**

- A property change that **holds** (a new width, color, pose — it should
  survive rotation and hot reload) is standing truth: put the values in a
  `when` response (under an `arrangement` for screen poses) and time the
  change with a `transition`. Any property — the motion words included:
  `when held scale=0.95` is a standing squish that lasts as long as the press.
- A **flourish** (entrance, attention pulse, ambient loop — it ends where it
  began) is an `anim` on the motion channel: `translate.x` / `translate.y` /
  `scale` / `rotate` / `opacity`.
- A property your **code keeps moving** (a bar that follows a selection, an
  underline under the picked tab) slides with a property transition:
  `transition commandBar.{margin_left, margin_top, opacity} duration=0.4` —
  every write then slides there; `set_prop_instant` lands one at once.

See [States](#states) and [Animations](#animations).

## Syntax basics

One line shape covers the whole language:

```text
WidgetType name .StyleName property=value, property=value { children }
```

- `//` comments to end of line. Indentation is never significant; braces are.
- Commas are **required** between properties. A **trailing comma continues the
  line**. A line that holds only the type and the name also continues: the
  next line may open with a size or a `key=value`.
- `;` separates multiple nodes on one line. `{` must be on the node's line,
  with no comma before it.
- Duplicate property on one node = error. Duplicate node name per view = error.
- Property keys are snake_case (`min_height`, `font_size`, `tap_pulse`). `-`
  is not an identifier character.

## Values

| Kind   | Syntax | Notes |
|--------|--------|-------|
| string | `"Paint"`, `"new stroke"/newStroke` | Escapes: `\"` `\\` `\n` `\t`. `/key` marks a translation term: the literal is the English source text, the key indexes the locale tables (see Translations). |
| number | `36`, `0.5`, `-4`, `36epx` | A number on a **length property is epx** (the suffix may be written). There is no device-pixel unit: `36px` is build error G0038, and so is `%` on a property. The compiler does not check where `epx` is written: on a number that is not a length (`opacity=0.5epx`) it reads as the bare number. |
| bool   | `true`, `false` | |
| color  | `#RRGGBB`, `#RRGGBBAA`, `red`, `black/0.5`, `white@0.24` | Alpha **last** (as in CSS). Porting WPF `#AARRGGBB`: move the alpha pair to the end. Bare palette names are legal in any color position, and every color spelling takes `/opacity` — the final alpha as a 0..1 number (see Named colors). Inside a Gradient's `stops=` an entry may also carry `@position` (0..1, after any `/opacity`) — see Gradient. |
| ref    | `$name` | Widget parameter (innermost scope) or theme value. |
| ident  | `space_only` | Enum member / handler / easing / anim name. Enum words ignore letter case (`halign=Center`). |
| size   | `220x110`, `220x`, `x110` | Exact-size shorthand (components epx): `220x110` is `width=220, height=110`. An omitted component leaves that axis alone (the "at least" spelling is `min_size=WxH`). Valid bare (`Button b 44x36`) or as `size=`. `size=` also takes one sizing word for both sides: `size=hug` (an Image at its picture's size), `size=fill`, `size=48`. |
| call   | `play(appear)`, `go(expanded)`, `toggle(expanded)`, `emit(SavePressed)`, `fill(2)` | Callees on `on:` keys: `play` = anim trigger, `go` = arrangement flip, `toggle` = flip between the pose and base, `emit` = report the event under a name you choose. On `width`/`height` and grid tracks: `fill(n)` weighted sizing (n may be fractional). |
| list   | `[80, hug, fill(2)]` | Values in brackets; newlines free inside. Only Grid's `cols=`/`areas=`, Gradient's `stops=`, `border_dash=`, an Arc's `dash=` and an Image's `window=` take one (everything else is fenced, G0040). |
| path   | `listBrush.translate.x` | **Attached** dots fuse into one token: anim line targets and dotted `when` keys. A dot after whitespace is style application (`.ToolButton`). |

## Directives

### Themes

```kdl
$accent = #E51400            // top level: a default token
theme {                      // the DEFAULT theme block
    $chrome = #1F1F1F        //   $token entries — same as the top-level spelling
    font = "res://fonts/Roboto Regular.ttf"   // control defaults — see below
}
theme dark { $accent = #FF6B4A }   // variant overrides, active via Runtime::set_theme_variant
theme fancy { font = "res://fonts/Fancy.ttf" }
```

Tokens (`$name = value`) are referenced anywhere a value goes as `$accent`.
Widget parameters shadow theme values. Undefined/duplicate `$name` =
build error (G6001 / G6002). A number token in a size, margin or padding
(`height=$backBoxHeight`, `min_width=$rowFont`) reads the default entry
only — a `theme dark` block can't change a laid-out size. `spacing=$gap`
is the exception: it follows the active variant.

**From code**: the build writes each page's tokens as `enum Theme`
(`$accent` → `Theme::Accent`), so code that paints with the file's colors
reads them by name: `rt.theme_color(&page, Theme::Accent)` gives the active
variant's color (else the default block's), `$refs` followed like the
file follows them; `None` when the value isn't a color. A renamed token
is then a compile error, not a stale copy of the hex in code. Numbers the
same way: `rt.theme_number(&page, Theme::BackBoxHeight)`.

**Color overrides** (`red = #E51400` entries, no `$`) remap the built-in
named colors — see Named colors below.

**Control defaults** (`prop = value` entries) apply to the whole page:
every node that shows text takes them unless it overrides them — the
analog of WPF's inherited `FontFamily`/`FontSize`.
An active variant's entries win over the default block's, and between two
blocks that set the same default the later one wins: the page's over an
included file's. Supported: `font`
(a font path), `font_size` (epx), `font_color` (the page's text color;
`color` here is refused, G3006) and
`disabled_opacity` (0–1, default 0.3: how far a `disabled`
control's subtree dims — spec "Default look", Disabled). Layout props like `width` can't go on a theme — that's what styles
are for.

**Type defaults** (`Slider .ThinSlider` entries) default every instance of
a CONTROL type. See "Theme type defaults" under Controls.

**Built-in default font_size = 15 epx** (D88). A document with no
`theme { font_size = ... }` behaves exactly as if it declared 15. Declare
the entry to override.

**Subtree override**: a blockless `theme` line inside any node's body themes
that node (and so its subtree); the nearest ancestor theme wins, and
node-local overrides like `font_size` beat any theme:

```kdl
Scroll gallery {
    theme font="res://fonts/Roboto Light.ttf"   // this subtree only
    ...
}
```

Declared themes use blocks; applied overrides are props on one line — braces
declare, lines apply.

### Named colors (built-in palette)

Color names are contextual **keywords**, legal in any color-typed position —
the way `fill`/`hug` are legal on `width=`. They are never global
identifiers: a node or anim named `red` stays fine.

```kdl
Rectangle stripe color=teal
Button del background=silver, font_color=text {
    when held background=danger
}
theme {
    accent = purple        // override — no $ prefix inside theme blocks
}
theme dark {
    surface = #141414      // beats the built-in dark value
}
```

Literal colors (identical in light and dark):

| name | hex | | name | hex |
|------|-----|-|------|-----|
| `black` | `#000000` exact, testing | | `purple` | `#9370DB` medium purple |
| `white` | `#FFFFFF` exact, testing | | `pink` | `#DB7093` pale violet red |
| `red` | `#FF6347` tomato | | `brown` | `#BC8F8F` rosy brown |
| `orange` | `#F4A460` sandy, warm | | `smoke` | `#F5F4F0` warm off-white |
| `yellow` | `#DAA520` goldenrod | | `silver` | `#DCDCDC` gainsboro |
| `green` | `#3CB371` medium sea green | | `gray` | `#708090` slate gray |
| `teal` | `#5F9EA0` cadet blue | | `charcoal` | `#2F4F4F` dark slate |
| `blue` | `#6495ED` cornflower | | `transparent` | `#00000000` fully clear |
| `indigo` | `#6A5ACD` slate blue | | | |

**Opacity suffix** — every color spelling takes `/opacity`, a bare 0..1
number that sets the **final alpha**: `black/0.5`, `danger/0.2`,
`#FF6347/0.25`, `$accent/0.3`. It works in overrides too
(`theme { danger = red/0.5 }`). `/o` replaces the alpha of the value it is
written on: `danger/0.8` over that override draws 0.8, `$flash/0` is
`$flash` made transparent, `#FFFFFF6B/0` fades a pasted color — the suffix
at the slot is the alpha the slot gets. `/` on a non-color value is a
build error.

Semantic tokens — **aliases resolved at theme time**: `accent` = `blue`,
`on_accent` = `white`, `danger` = `red`, `warning` = `yellow`,
`success` = `green`, `info` = `teal`, `text_muted` = `gray`. Remap the
literal (`theme { red = #E51400 }`) and every alias follows; or remap the
token itself (`theme { accent = purple }`).

Mode tokens flip under the **well-known variant name `dark`**
(`Runtime::set_theme_variant`); literal colors never flip:

| token | light | dark |
|-------|-------|------|
| `surface` | `#FBFAF7` | `#1E1F22` |
| `panel` | `#FFFFFF` | `#27282C` |
| `border` | `#E4E2DC` | `#3A3B40` |
| `text` | `#2E3338` | `#E8E6E1` |
| `text_muted` | `#708090` (= `gray`) | `#9AA3AD` |

Any other variant name reads the light column. Overriding `surface` in the
default `theme { }` replaces the built-in pair for **all** modes — add your
own `theme dark { surface = ... }` if you theme both.

Overrides live **only in theme blocks** and take a hex color or another
color name (a `$ref` there is build error G6004). The name set is closed — new
names belong to `$tokens`. Rules of thumb: text on colored swatches — white
on red/green/blue/indigo/purple/teal, charcoal on
yellow/orange/pink/brown/smoke/silver. `black`/`white` ship exact
`#000`/`#FFF` and never mode-flip; tests rely on the defaults — leave them
unoverridden by convention.

### Styles

```kdl
style ToolButton min_height=36, tap_pulse=true {
    when held background=#FFFFFF22      // states: see States section
}
Button undoButton .ToolButton text="Undo"
```

A named property bag applied with `.Name`; local properties override. Style
children may only be `when` responses and `transition` rules. `style=X` as a
property is a build error (G2001: `style=` is not a property).

**Inheritance**: `style Warn extends=ToolButton, background=#AA0000 { ... }` —
single inheritance; the parent's props and when/transition blocks apply first, the
child's override per key (the same last-writer rule as listing two styles on
a node). Chains work across `uses` imports; the parent name resolves in the
current scope (widget-local shadows document-level). Cycles and unknown
parents are build errors.

#### Checked styles: `for=`

`for=<Control>` (first property, always) points a style at one control
type's CONTRACT — its slots and states — and turns on write-time checking
(customization spec §2, D109):

```kdl partial
style ThinFader for=Fader, slot.track.height=4, slot.thumb.size=8 {
    when held    slot.thumb.scale=1.2       // contract state — checked as you type
    when focused slot.track.background=$accent
}
Fader vol .ThinFader value=50
```

- **`slot.<name>.<prop>`** restyles the control's slot regions — placement,
  the box family, `size=` (a bare number is a square). Everything is
  verified at the declaration: unknown slots list the real ones, driven
  properties are the driver's (`slot.thumb.halign` → G3002 "moves by itself"),
  content props route to filling. Slot paths are also legal directly on a
  contract-bearing **instance line** (`Fader f slot.track.height=2`) — a
  one-off tweak needs no named style.
- **`when` states are checked at write time** against the target: a
  built-in kind takes the universal states (`held`, `hovered`, `focused`,
  `focus_within`, `selected`, `disabled`); a `control` takes only the
  states its contract declares (its `state_builtin` and `state` lines).
  Arrangement names and `screen.*` words pass on both. Any other state
  is an error (G0036) that lists the states the target has.
- **When-responses on slots** take the motion vocabulary (`scale`,
  `translate.x/.y`, `rotate`, `opacity`) and two colors, `background` and
  `border_color`. Any other box word in a `when` is an error (G3002: it
  goes on the style line); static motion stays an error (base looks
  declare no transform).
- The target must be reachable **where the style is written** (`uses` the
  declaring file); widgets are not targets (style their nodes by name).
- **`extends=` compatibility**: same-target chains and `for=` over a plain
  base are fine; a plain style can't extend a `for=` base (it would inherit
  slot paths it can't check), and cross-target chains error.
- **Application gate**: wearing a `for=Fader` style on anything but a Fader
  is an error. A built-in kind takes a `for=` style with no ceremony:
  `for=Button` checks the states the style names (a Button has no slots, so
  a slot path in one is an error), and `for=Slider` may also restyle the
  three slot words the built-in Slider takes (see "Restyling the built-in
  Slider" under Controls).
- **Plain styles** may still name any state — contract states are checked
  where the style is *applied* (G0036: "'busy' isn't available on `Button`
  — …"). A plain style whose custom state lands cleanly on exactly one
  control type draws the D2 advisory to add `for=`; landing on two distinct
  types reads as deliberately generic and the advisory stands down.

### Widgets

```kdl partial
widget LabeledSlider $label="", $min=0, $max=100 {
    anim nudge { handle.translate.x from=-2, to=0, duration=0.2 }  // widget-scoped
    style Row min_height=24                                        // widget-scoped
    Column spacing=4 {
        Label text=$label
        Slider signals min=$min, max=$max
        slot
    }
}
LabeledSlider sizeSlider label="Size", min=1, max=300, min_width=240
```

- `$param=default` on the declaration line declares a parameter (defaults
  required); instance overrides use the bare key (`label="Size"`).
- Exactly **one root node** in the body; scoped `anim`/`style` directives may
  sit alongside it. They see the widget's internal names; `self` in a scoped
  style's anim targets the styled node.
- **Body-node names are per-instance**: visible to the widget's own
  anims/styles/whens. In the file, the instance name means the root. Code
  reaches the body through the instance: a build generates one
  `<Widget>Names` struct per widget (`NAMES.size_slider.root`, and a field
  per named body node).
- `signals` — flags the node whose events the instance reports: an `on:`
  line written on an instance lands there.
- Non-parameter instance properties pass through to the root. Widget cycles
  are build errors (G5003).
- A widget inside a widget may hand a param on under the SAME name
  (`NameChip chip text=$text` in a widget that has its own `$text`): the
  instance line is read in the enclosing widget's scope, so the inner
  `$text` is the outer value (D112 — before, the build overflowed its stack
  with no message).

#### Host widgets (`for=`, D276)

A widget that IS a stateful control's look declares its host, and may then
answer the HOST's states at widget scope:

```kdl
widget RadioLook for=Radio, $label="" {
    Row width=fill {
        Circle dot 10x10, color=$accent, opacity=0
        Label text=$label
    }
    when selected dot.opacity=1     // the HOSTING Radio's selected
}
choice save {
    Radio a { RadioLook label="Default album" }
    Radio b { RadioLook label="Custom location" }
}
```

- `for=` names the host type and comes FIRST on the declaration line (the
  style `for=` rule). Hosts: `Button`, `Radio`, `Check` — the button family.
- Widget-scope `when`/`transition` lines (siblings of the body root) answer
  the hosting control's states; their `name.prop=` targets resolve in the
  widget body's own names. Bare props (`when held background=...`) land on
  the widget's body root, not on the host.
- A `for=` widget instantiates only as the DIRECT body of its host type;
  anywhere else the build fails naming the contract.
- The host's own inline `when` for the same state replaces the widget's
  response for that state (nearer scope wins).
- Widget-scope `when` on a state without `for=` is a build error (G5001) —
  no host, no state. One `when` is legal there: `when screen.<class>`,
  which poses the widget's own body
  (`when screen.landscapeSmall { card.width = 420 }`).
- Inside a widget body, `when selected` on a plain node (not a toggle
  surface, not a ListItem) compiles, but nothing in the file raises it
  there — only code can (`Runtime::set_state`). To answer the HOST's
  `selected`, write the `when` at widget scope, as above (D276).

#### Slots

`slot` is where a widget takes content — **one noun, two verbs** (declare in
the body, fill at the instance):

```kdl
widget Card {
    Panel corner_radius=6, padding=12 {
        Column spacing=8 {
            slot header height=24, width=fill { Label text="Untitled" }  // named region + default content
            slot                                                         // the default slot
        }
    }
}
Card stats {
    slot header { Label text="Run stats", font_size=14 }  // named fill
    Label text="I fill the default slot"                  // bare children -> default slot
}
```

- **Bare `slot`** (unnamed, no props) is the default slot — the shipped
  inline marker: content joins the slot's parent directly. It may now carry
  default content in braces (`slot { <default> }`), replaced entirely when
  the instance brings bare children. At most one default slot: a second
  one is a build error (G5004).
- **A fill is checked where it is written** (G5004). `slot <name> { … }`
  at an instance names a slot the widget has, its line takes the name
  and nothing else, and a slot is filled once. Bare children need the
  widget's default slot: on a widget without one they are a build
  error.
- **`slot <name> ...`** (or any slot with props) declares a named **region**:
  a real node the declaration line places and paints. Its line takes the
  placement vocabulary (`width`/`height`/`min_*`/`halign`/`valign`/margins,
  `WxH`) and the **box family** (`background`/`corner_radius`/`border_*`/
  `padding`) — `slot track width=fill, height=4, background=$surface2` needs
  no content at all. Content props (`font_size`, `text`, …) belong to
  content: **restyling content IS filling** — bring the node
  (`slot label { Label font_size=12 }`); the error routes there.
- Region content is clipped to the region; bare content **fills** it (D71);
  `padding` insets every child's frame (centered children shift with
  asymmetric padding, like a padded box's true center). A region reports its
  content's minimum plus padding, so it never collapses in a Column/Panel.
- **Filling replaces the slot's default content entirely** — and only the
  content: `slot` declarations nested inside it are contract structure and
  always survive, drawing **above** the fill (tick marks go under the thumb).
- A second default slot errors at the declaration (G5004); two slots with
  one name error where the widget is used (G0044, below). A fill for a
  slot the widget does not have names the nearest slot, or lists the
  widget's slots when none is near; bare children on a widget with no
  default slot error listing the named fills. Fill blocks take **no**
  props (the declaration places the region; a fill is content).
- A named region is a node of the widget body, named like any other: a
  body node with the same name is a build error where the widget is used
  (G0044, the name is used twice).

### Controls

A `control` is a widget plus an INPUT BEHAVIOR: a `behavior` line names
the behavior that turns the finger and keys into `$value`, beside declared
states and signals, and slots whose geometry the value drives
(control-customization spec §5, D108):

```kdl
control Fader $value=0, $min=0, $max=100, $step=0 {
    behavior slider by $value in $min..$max, step=$step   // the behavior
    // `held` IS the drag state (D181) — the runtime's gesture tracking
    // maintains it; a behavior never writes a state
    state_builtin hovered, focused, disabled, held
    keys enter                                  // claimed key groups (the behavior owns arrows/Home/End)
    signal drag_started, drag_ended             // the behavior's drag edges
    transition duration=0.12, easing=cubic_out  // default FEEL (flips + fraction glide)

    Layers {
        slot track width=fill, height=24 {
            slot progress automatic=length by $value in $min..$max { Rectangle color=$accent }
        }
        slot label { Label text=$value }  // follows the control's value
        slot thumb automatic=position by $value in $min..$max { Circle width=20, height=fill, color=white }
    }
}

Fader gain value=25          // drag_started reaches code as a Signal event
```

`behavior <name> by $value in $min..$max[, step=$step][, key_unit=5%]`
names the INPUT BEHAVIOR that answers every instance of the control. The
line is optional: a control without one takes its input in code
(`on:pointer=` on a node in its body + `keys`). The behaviors gogo ships:

| behavior | what a finger does | keys / wheel | reads |
|---|---|---|---|
| `slider` | a press anywhere on the instance jumps the value to the finger (bare x over the `track` slot's width — no thumb compensation), drags follow it, the release commits | arrows step by `step=` (or `key_unit=` on a continuous slider — `5%` of the range, or a value amount; else 1), Home/End pin the ends, a wheel tick steps | `slot track` |
| `toggle` | a tap glides the value to the other end; a drag past 6 epx puts the thumb under the finger (thumb-width compensated over the track) and settles to the nearer end on release | Enter/Space flip, Right/Up/End = on, Left/Down/Home = off, wheel likewise | `slot track`, `slot thumb` |

A host adds its own behaviors in Rust — see "Your own behavior" below.
An unknown name refuses at build listing every behavior the build knows;
a slot the behavior reads that the body lacks refuses too.

`keys <groups>`: the key groups a focused instance takes for its
code-behind — `arrows`, `home_end`, `enter`, `space` (semantic words,
never raw key names, so a future remap layer leaves claims standing). A
claimed key arrives as a `Key` event on the instance root and never
moves focus; an unclaimed key traverses as
always; Tab is never claimable — the way off a control; a disabled
instance claims nothing. One owner per group: a behavior OWNS the groups
its spec lists (`slider`: arrows, home_end; `toggle`: all four), and a
`keys` claim on one of them refuses at build (G0082) — never a silent
precedence. This is the built-ins' own law (a Slider claims its arrows
unconditionally) extended to user controls.

- **`$value` follows the control's value**: a property in the body
  that reads it (`Label text=$value`, `Rectangle opacity=$value`) starts
  at the control's value and is written again on every write of it — a
  drag, a key step, or code. `Runtime::set_value(&mut page, root, 50.0)`
  writes those properties at once; after any other write of the value
  they follow at the next `tick`. The same holds when the value is
  passed along: `Readout r n=$value` to a widget whose label says
  `text=$n`, or `value=$value` on a control placed in the body. Every
  other `$param` is fixed when the page is built. Theme refs (`$accent`)
  stay static and legal.
  The control is the WRITER of a property that reads `$value` — a `when`
  on the same node holding the same property is a two-writers error
  (G4003), and a `when` response cannot read `$value` at all (G0083: a
  response is one fixed value).
  A shown number rounds to 0.1 by default; `round($value, 1)` picks the
  step, on a `text=` and on a number property alike. (`live($value)`,
  the wrapper the Godot addon needed, is gone: G0081.)
- **The behavior's law** — what the runtime does around ANY behavior, so a
  behavior is only pointer math: a press anywhere on the instance is the
  behavior's (`held` on the root, focus parked there, a second finger
  swallowed); the declared `drag_started` is raised before the press's
  first write and `drag_ended` after the release's commit AND on a
  cancelled or reclaimed gesture (a host arming UI on the start always
  hears the end); every changed write mid-gesture streams `edit`, the
  release commits `change` when the value differs from the one the finger
  found (a number for `slider`, `true`/`false` at the midpoint for
  `toggle`), and a cancel restores that value; `selected` derives from the
  value live when the contract declares it (`state_builtin selected`);
  a property that reads `$value` follows every write. Keys reach the behavior only
  through focus (declare `state_builtin focused`); the wheel reaches the
  instance under the pointer with no focus and parks focus there. A
  disabled instance answers nothing; disabling mid-drag cancels the
  gesture. The FEEL: a write while the finger holds the control lands
  direct (the thumb stays under the finger); a tap's settle, a key step
  and a code write glide by the control's `transition` rule.
- **Your own behavior** (Rust): implement `gogo_ui::Behavior` — `began(cx, x,
  y) -> Claim` (`Take` the gesture or `Pass` the press on), `moved`,
  `ended`, `canceled`, `key(cx, action) -> bool` (`Increment`,
  `Decrement`, `ToMin`, `ToMax`, `Activate`) — reading the value triad and
  the slots as control-local rects from `Cx` (`cx.value()`,
  `cx.fraction()`, `cx.slot(0)`, `cx.key_step()`) and writing ONLY through
  `cx.set_value(v)` / `cx.set_fraction(f)` (snapped to `step=`, clamped).
  It never sees nodes — that is what keeps every checked style safe under
  every behavior. Its `BehaviorSpec` (the name, the slots it reads in
  `cx.slot(i)` order, the key groups it owns, the `change` payload) goes
  to the compiler in `CompileOptions::behaviors`, and the behavior itself
  onto the page's `Input` with `input.register(Box::new(MyMachine))`;
  the `.gogo` file then says `behavior mymachine by $value in $min..$max`.
  Gesture scratch (a press x, a wobble flag) lives in the behavior — one
  gesture is live at a time.
- **Automatic geometry** — the coupling is fully spelled (D134): the kind
  (config in its parens for the radial kinds), the driver after `by`, the
  range after `in`. Nothing defaults invisibly:

  ```kdl
  slot fill  automatic=length by $value in $min..$max
  slot thumb automatic=position by $value in $min..$max
  slot mark  automatic=presence by selected
  slot wedge automatic=arc(from=-135, sweep=270) by $value in $min..$max
  slot dot   automatic=angle(from=-135, sweep=270) by $value in $min..$max
  ```

  `length` = main-axis extent follows the fraction
  `(value-min)/(max-min)`, `position` = main-axis seat, `presence` =
  content exists iff the named state holds (a bool — no `in` range). v1
  drives from exactly `$value in $min..$max`; other params/ranges arrive
  with their first consumer (range slider). The boundary: geometry that
  displays DATA is `automatic=`; presentational motion is an `anim`
  (`repeat=forever`) — there is no clock driver, and a timer is code
  writing a `$param`. The runtime driver is the ONE writer of driven
  geometry: mid-drag writes land
  directly, discrete changes (code, key steps) glide through the one bare
  `transition` rule the instance has: the declaration's default feel, or,
  when the declaration has none, a bare rule in the instance's block or in
  an applied style. A second bare rule on one instance is build error
  G0034. At rest the driver disarms and the written layout HOLDS.
  A `length` or `position` slot must sit in a free host (a slot region or
  a `Layers`) — under a measuring container (Column, Row, Flow, Grid,
  Tiles, Scroll) it is build error G5105, given at the slot's line once
  the control is used. A `position`
  slot's width comes from its content: `width=fill` content is a build
  error unless the slot line sizes itself (`slot thumb width=20`).
- **Radial kinds** (spec §4.6): `arc` clips the slot region to
  the angular WEDGE `[from, from + fraction × sweep]` — content-independent
  (fill the slot with anything, the wedge still reveals it by value), and
  hits agree with the pixels. `angle` SEATS the region on the
  circle centered in its host at `from + fraction × sweep` (angular
  position, never rotation). Both REQUIRE their aperture as kind config
  (`automatic=arc(from=-135, sweep=270) by $value in $min..$max`;
  degrees on the compass, as an `Arc`'s — 0° points up, positive turns
  clockwise) — declared once.
  The typical arc content is a bare full `Arc` ring — the wedge does the
  revealing. The `automatic` card builds a knob this way.
- **States**: customs (`state <names>`) sit between `held` and `disabled`
  in priority; between two customs a `when` on the instance beats one a
  style or the control's own body carries, and of two written in the same
  place the later line wins; `disabled` beats everything. Instance
  and style whens respond to declared states like any built-in
  (`Fader f { when held opacity=0.85 }`); undeclared contract states
  error listing what's available.
- **Scoped anims** may target `slot.<name>.<motion word>`
  (`slot.thumb.translate.y from=-3, to=0`) — the ONLY place anims address
  slots, and only on the motion channel (driven layout keeps its one
  writer, G0063).

### Restyling the built-in Slider

The built-in `Slider` draws its own parts — a `track`, the `fill` that
grows with the value, a `thumb` — and takes three slot words, from a
`for=Slider` style or on the instance line:

```kdl
style ThinSlider for=Slider, slot.track.corner_radius=0
Slider volume .ThinSlider value=50, min=0, max=100, step=5,
    slot.track.background=#333333, slot.fill.background=#E51400
```

| slot word | what it sets |
|---|---|
| `slot.track.background` | the track's color |
| `slot.track.corner_radius` | the track's corners |
| `slot.fill.background` | the color of the part that grows with the value |

Any other slot word on a built-in Slider is build error G3002, which lists
these three. The built-in `Toggle` takes none (G3002 says so) and a Button
has no slots. The two colors answer to states in styles (`when held`, `when
selected`, `when focused`, `when disabled`); `slot.track.corner_radius` in
a `when` is G3002 (it goes on the style line).

A node written between a built-in Slider's braces replaces none of its
parts, whether it sits in a `slot thumb { }` or `slot label { }` block or
bare. It is laid out over the Slider's whole box and drawn on top of the
track, the fill and the thumb, and it does not move with the value.

A slider or switch that needs more than this is a `control` built with
the machinery above (`behavior slider` / `behavior toggle`): its slots
are declared in the file, so every one of them is restylable and
fillable.

### Theme type defaults (`theme { Slider ... }`)

Per-instance styles are right for one-offs and wrong as app policy. A
**type-keyed theme entry** defaults every instance of a control type
(spec §5.8):

```kdl
style ThinSlider for=Slider, slot.track.corner_radius=0
style ThinSliderDark for=Slider, slot.track.background=#333333
style AppButton min_height=36
theme {
    Slider .ThinSlider     // every Slider: the ThinSlider look
    Button .AppButton      // every Button: this style
}
theme dark {
    Slider .ThinSliderDark // a variant overrides the look
}
Column {
    Slider volume value=30
    Button ok text="OK"
}
```

- **Shape: an instance-line prototype** — type, styles, props; no name, no
  children, no `on:` wiring (G3006: "a theme entry has no name"), no
  placement (size/margins belong where the instance sits). Types must be
  controls: the built-in kinds or your `control` declarations.
- **Nearest scope wins**: an instance line beats the entry — two tiers, no
  third. The entry's style merges base-most (theme style → applied styles →
  node props → when overrides); entry props are a defaults tier instance
  props override individually (`theme { Slider step=25 }`).
- **A variant entry** holds what can switch with the variant today: base
  look props. Its `when` responses and restyles of your own controls do
  not switch with the variant yet.

### Animations

See the Animations section. `anim name { ... }` at top level or inside a
widget body.

### Includes

```kdl
uses "styles/theme.gogo"     // relative to the current file
```

Brings theme values, theme blocks (default and variant), styles, widgets and
anims. A later `style` or `widget` of the same name replaces the earlier
one; a `$token` or an `anim` defined a second time is a build error (G6002,
G0030). A file is brought in once, so a cycle of `uses` lines is no error. A
`theme { font = ... }` in a shared include therefore sets the default font of
**every view that uses it** — the usual place for an app-wide typeface.

### Atlases — one picture, cut into slices

An **atlas** is one picture file holding many small pictures: a sheet of
stickers, a strip of toolbar icons, a packer's export. An `atlas` line
names the picture once and says how to cut it; every piece is a
**slice**, and an Image shows one with `source=`. The picture decodes
once and every slice shares it (built 2026-09-27).

Four forms, one keyword:

```kdl partial
// A grid: every square the same size, numbered from 0 left to right,
// then top to bottom. The counts come from the picture: a 480x720
// picture in 80x80 squares is 6 by 9 = cells 0 to 53.
atlas animals source="res://assets/stickers/animals.jpg", cell=80x80

// Named slices: any rectangle, in picture pixels.
atlas toolbar source="res://assets/icons/toolbar.png" {
    slice play  x=0,  y=0,  size=48x48
    slice pause x=48, y=0,  size=48x48
    slice wide  x=0,  y=48, size=96x32
}

// A packer's export (TexturePacker "JSON (Hash)" or "JSON (Array)",
// also Free Texture Packer, ShoeBox, Aseprite). The JSON names its
// own picture; its frames become named slices.
atlas ui from="res://assets/ui/ui.json"

// The whole picture as one slice — here a 9-slice (see `edge=`).
atlas bubble source="res://assets/ui/bubble.png", edge=20

View v {
    Image a 40x40, source=animals[12]      // cell 12: row 2, first column
    Image b 24x24, source=toolbar.play
    Image c 32x32, source=ui.buttons_play  // from "buttons/play.png"
    Image d 200x80, source=bubble
}
```

| Form | Line | `source=` spelling | Rust code |
|------|------|--------------------|-----------|
| grid | `cell=WxH` | `animals[12]` | `atlas::ANIMALS[12]` |
| named slices | a `{ slice … }` block | `toolbar.play` | `atlas::toolbar::PLAY` |
| import | `from="….json"` | `ui.buttons_play` | `atlas::ui::BUTTONS_PLAY` |
| whole picture | neither | `bubble` | `atlas::BUBBLE` |

- **`slice <name> x=, y=, size=WxH`**: a rectangle in picture pixels,
  inside the picture, each name once per atlas.
- **`edge=`** makes a **9-slice**: the outer band (in picture pixels)
  keeps its size while the rest stretches — corners fixed, edges
  stretch one way, the middle both. `edge=12` sets all four sides;
  `edge_left` / `edge_top` / `edge_right` / `edge_bottom` set one; the
  later word on the line wins (the `padding` rule). It goes on a grid
  line (every cell), a `slice` line, a whole-picture line, or — for an
  import — a `slice <name> edge=…` line in the atlas's block (the
  position still comes from the JSON). A corner draws at one epx per
  picture pixel; when the Image is smaller than two corners, every band
  shrinks together. A slice without `edge=` stretches whole, by `fit=`.
- **Import names**: the packer's file name with the extension dropped,
  lowercased, every character other than a letter, digit or `_` turned
  into `_`, and `_` in front of a leading digit — `buttons/Play-Hover.png`
  → `buttons_play_hover`, `01.png` → `_01`. Two files becoming one name
  is a build error naming both.
- **Trimmed imports**: a packer that trimmed empty borders off a sprite
  records its original box; the slice lays out and fits as that box
  (`aspect=natural` included) and draws only its pixels inside it, so
  turning trimming on or off never moves anything. Rotated frames are
  refused — turn rotation off in the packer.
- **Names are one namespace for the whole build** (code reaches them as
  `atlas::…`). Put shared atlases in a `uses` file: the same file under
  two pages is fine; two different `atlas icons` lines are an error.
  An atlas line sits at the top level of a file; its slices can be named
  anywhere below, and in a theme value (`$icon = toolbar.play`) or a
  widget param (`Tile icon=toolbar.play`).
- **Code sets slices like pictures**: `rt.set_prop(img, PropKey::Source,
  Value::VImage(atlas::ANIMALS[3]))` — on a repeat's slots too (a
  repeat item carries no picture; the page's code writes each slot's
  Image).
- **Checked at build time**: the picture is read (its header only) to
  count grid cells and bound every slice, and `from=` reads the JSON; a
  folder behind an `.ignore` marker is refused like any asset.
- **`release=unused`** lets the app free the picture while no page names
  it, and decode it again when a page does — for big pictures a page
  shows now and then (a sticker sheet). Without it a picture stays
  decoded for the whole session once shown, which is right for anything
  shown often (the effects list, icons). It goes on the line that
  declares the picture: an `atlas` line, or an Image naming a file
  (`Image hero source="res://big.jpg", release=unused`). The picture is
  freed only when EVERY line declaring it says so — one line without it
  keeps it, the way one eager line makes a picture eager everywhere. An
  Image showing a slice declares nothing (put the word on the atlas
  line). Hiding or scrolling never frees a picture: the app frees only
  after a page closes, or when a page's code swaps its pictures and asks
  (a category switch); a hidden or parked page still names its pictures.
- **Leave space between slices** (packers call it padding or extrude): a
  scaled, smoothed slice samples one pixel past its edge. A grid whose
  squares carry their own margin (a contact sheet) needs nothing.

Errors G2011–G2020 (docs/gogo-errors.md): the line's shape, a file that
cannot be read, `cell=` not dividing the picture, a slice outside it,
bands with no middle, `source=` naming no such atlas/cell/slice, a JSON
gogo cannot use, `window=`/`fit=` on a slice, one name declared twice,
`release=` it cannot use.
For example:

```text
ui/stickers.gogo:3: error G2013: `res://assets/stickers/animals.jpg` is 480x720 — that does not divide into 80x70 squares
```

## Properties

Every property is a gogo word, snake_case, checked against the node's
kind: a word no kind takes is build error G2001, a word that belongs to
another kind G2002. The words:

| Key | Meaning |
|-----|---------|
| `halign` / `valign` | WHERE the control sits in the available space: `halign=left/center/right`, `valign=top/center/bottom`. One vocabulary under every parent. Position only; sizing is `width`/`height`. See Placement below. |
| `width` / `height` | HOW BIG: a number = that exact size (epx, and don't stretch); `hug` = natural content size; `fill` = take the leftover space; `fill(2)` = a double-weighted share of it; `hug(2)` = a weighted slot around a natural-size control. See Placement below. |
| `size=WxH` / bare `WxH` | Exact-size shorthand for both axes (`50x90`); omit a component to leave that axis alone (`x72`). |
| `min_width` `min_height` `min_size` | Minimum only — the control still stretches with its container. The "at least N, stretchy" spelling. Next to an exact `width=`/`height=` for the same side on one line it is a build error (G1019: an exact size is its own floor and ceiling) — except where a `when` block poses that side's size: there the floor bounds the posed values (D249), in every parent. A floor that a style or a control's template carries stands beside a size written on the line that uses it, and wins. `min_size=44` is both sides; `min_size=x36` leaves the width's floor alone. With a hug cap it forms the min/max band — see Placement. |
| `margin=N` / `margin_left/top/right/bottom` | Margins CARVE the box the other keys produce on a free (`Layers`) child — SIGNED distances from the parent's matching edges: positive sits inside, **negative hangs past the edge**. On a filled/default axis they are insets (the unnamed edge stays flush); on an aligned axis they offset the near edge; on a centered axis the box is centered in the room they leave, so one margin shifts it by half; on an unaligned `hug`/exact axis they pin. `margin=N` alone is breathing room all around. A box that places its children itself (Column, Row, Choice, Flow, Grid, Tiles, a List's rows) refuses margins on them (G1014) — space them with `spacing`/`padding`/`gap`; the one body of a `Scroll` takes them. See Placement below. |
| `spacing=N` | Container separation (epx). Default 0 — children touch; no gap exists unless the file writes one (D244). |
| `background` `corner_radius` `padding` `border_color` `border_thickness` | The box family, each key changing exactly its one thing. Any box takes them — a Label, an Image, a stack, a `Layers` — and the plate draws behind the content, covering the padding. Panels (`Panel`, and `View` roots) start as a **blank canvas** (WPF Border semantics — nothing drawn until a key says so); a Button starts with its default plate (spec "Default look") and `background=` recolors it; an Input starts with its dark plate and takes the same words, `border_dash=` and the four corner radii included. A bare Button shows no pressed look of its own: press feedback is yours to author with `when held background=`. `padding` also comes per side (`padding_left`, `padding_top`, `padding_right`, `padding_bottom`, mirroring `margin_*`) for WPF's per-side Padding thickness — asymmetric content air that still counts into a button's derived minimum. **Text leaves pad too**: `padding` is air between a node's box and its CONTENT — children or text — so `Label` and `Input` inset their own text (WPF's TextBlock/TextBox Padding ports verbatim). Drawing leaves (`Rectangle`/`Circle`/`Arc`/`Gradient`/`Image`) have nothing inside — outer air is `margin`. **`padding` is air INSIDE the border**: on a bordered node the content sits `border_thickness + padding` from the edge (WPF/CSS stacking — XAML Padding ports verbatim; a border alone insets content by its thickness), so a border can never paint over content and retuning one knob never re-derives the other. Fractional borders keep their declared thickness in the layout math and in the painted stroke (`border_thickness=1.5` paints 1.5 epx wide, antialiased). On a `Circle`, `border_color`/`border_thickness` stroke the ring instead (see Circle). On `Layers`, the `View` root and stacks, `padding` insets every child's placement and adds onto the box's minimum. **Corners also come per corner** (D135): `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left` — named keys, so there is no corner order to memorize; the uniform writes all four, a per-corner key writes one, and later on the line wins (the `padding`/`padding_left` layering law). |
| `opacity=0..1` | The whole node's alpha, children included. |
| `pivot=0x0` | The 0..1 box point `scale` and `rotate` transform about; unset = the center (the old behavior verbatim). `0x0` holds the top-left still while a scale grows right and down; `1x1` swings a rotation on the bottom-right hinge. Same spelling as a Gradient's `from=`/`to=` box points; anything but a full 0..1 pair refuses at build. A look word (`when` responses restyle it), not itself animatable. Hits follow every motion channel — translate/scale/pivot through the moved rects, rotation by un-rotating the finger — so look and touch always agree. |
| `visibility=visible/space_only/gone` | The three-state triad (D133; the bool `visible=` is retired). `gone` hides and releases the space; `space_only` hides but keeps holding its layout slot — toggling it never moves a sibling (WPF Hidden). Hidden is hidden either way: hit-inert and unfocusable; `on:show`/`on:hide` track the pixels. `space_only` holds its slot under any parent. Code flips visible/gone with `Runtime::set_visible`. |
| `reveal_y=0..1` / `reveal_x=0..1` | The node's LAYOUT SPACE as a fraction — the smooth-slide primitive. `0` conceals (the node earns no room on that axis and its own box shrinks toward the edge it is aligned to: the left or top edge with no alignment, the right with `halign=right`, the middle with `halign=center`. What is INSIDE the box is cut at the part that shows: its text and children draw there only and take no touch outside it. The content keeps its size and travels with the box — it is laid out from the corner where the shown part starts, so a sheet with `valign=bottom` slides out through the bottom), `1` grants it all. The factor applies to the extent the node would otherwise have — natural size when hugged, the **stretched extent when filled** (a `width=fill` label in a 100-wide column at `reveal_x=0.5` gets a 50-wide box; a label with no width word gets half of its own text's width). It's a float, so poses flip it, `transition` rules tween it, anims drive it — and the whole document reflows smoothly. A concealed node is still **alive** (anims run, conditions poll) — `visibility=gone` stays the instant switch and zeroes the space. Any parent: in a container the siblings close up as the box shrinks; under `Layers` the box shows its fraction from the edge its alignment names. The channel exists only where DECLARED: a `when`/anim driving `x.reveal_y` without `reveal_y=` on `x`'s line is a build error. See the COOKBOOK smooth-slide recipe. |
| `font_size=N` | Font size (epx) of the node's own text and of the Labels inside it that write none of their own — see the font note below the table. |
| `font_color=#...` | Text color of the node's own text and of the Labels inside it that write none of their own — the text half of `background=` (what WPF calls `Foreground` and CSS calls `color`). Pairs with `font_size`. |
| `min` `max` | A Slider's lower and upper bound. |
| `toggle` | `toggle=true` makes a Button latch: it holds `selected` while on. |
| `touch_target=off` | Marks a deliberately small interactive control. Accepted and read by nothing yet: the Rust crate has no touch-target lint. |
| `keyboard_target=off` | Takes a control out of keyboard focus (the WP app-bar pattern): Tab and the arrows skip it and it never shows a focus indicator. Code may write it (`set_prop` with the word `off`) to take a control out of the Tab order without disabling it; `clear_prop` gives the file's word back. A control switched off while it has the focus keeps its place — Tab goes on from it — and answers no key. |
| `interactive=true/false` | THE input switch: `true` claims input for this control, `false` makes it hit-invisible (unlike `disabled`, no restyle). Per control — never the subtree. Interactive controls answer input by default, everything else is transparent; see Input. |
| `overlay=true` | THE layout-negotiation switch (D139), completing the node-role trio: `interactive=` is a control's relationship to input routing, `overlay=` to layout negotiation, `visibility=` to rendering presence. It cuts exactly one thing — the box's upward size contribution: no ancestor hug/minimum (and no scroll range) counts it. Everything else is untouched: placement (signed margins may hang it past the parent), draw order (declaration order, as always), clipping (a `Scroll` viewport still cuts it — a box that must beat a scroller is a popup, a different concept), hit-testing (hit order is draw order, last declared wins), focus, event routing. For boxes that sit on a parent without sizing it: touch-slop surfaces, finger pucks, drag markers, badges. Free parents only — permanent law, not a v1 gap: a flowed child holds a SEAT it can't refuse, and `Column { Layers { content ... overlay } }` already spells that case. The overlay's own inside still checks: its children validate against ITS box; a nested overlay is exempt only at its own parent boundary. Refused: with `visibility=space_only` (a seat-holder is not an overlay — a hidden overlay is `gone`), and `fill` on an axis the parent hugs with no non-overlay child (nothing determines the size). |
| `color` | The fill of nodes that OWN a plain color (`Rectangle`, `Circle`) — their fill *is* their color; it is also an `Arc`'s stroke and the filled part of a `Slider`'s track. On anything else it's an error that points to the right channel: text is `font_color=`, fill is `background=`, a `Gradient` paints its `stops=`, whole-control fade is `opacity=`. |
| `source` | The picture: `res://path` (an asset embedded at build time) or `<host>://name` (a host picture the app's code registers at run time under that exact name — a live preview, a canvas tile). See "Resource paths" below. |
| `on:<event>=...` | Wiring — see Events and triggers. |

Resource paths. Resource-typed properties take path strings that always
carry a scheme (`source="res://icons/undo.png"`; a bare `undo.png` is
refused, G0071 — a bare word could be a label or a path). `res://` is an
asset under the build's resource root, checked and embedded at build
time. Any other scheme on an `Image`'s `source=` — `preview://brush`,
`canvas://tile`, `crop://source` — is a **host picture**: the app's code
hands the renderer pixels under that exact name at run time, and the
box shows nothing until it does. The name in the file is the contract:
the build bakes it as the picture's id, code looks the id up by the same
string, and an `Image` with no `source=` is a
picture of nothing however much the code registers. An **empty string
means null** — no load attempt (safe widget `$icon=""` defaults). A
non-empty `res://` path that fails to load is a build error.

Type names — one friendly name per concept: see the **Vocabulary** table at
the top. An unknown type word is a build error with a suggestion (G7011);
the host may add node types it draws itself
(`CompileOptions::code_controls`). A property spelled in another dialect
is refused toward the gogo word (G2001, G0041): there is no alias table.

**`font_size` and `font_color` flow down to the text inside.**
`font_size`/`font_color` on a node style that node's own text and every
Label under it that writes none of its own (a `font_size` on a Button
with a body sizes the Labels in the body; a nearer one wins). Only the
kinds that show text take the words on their line (Label, Button, Check,
Toggle, Radio, Input); any other box sets them for what is under it with
a `theme` line in its body (`Panel { theme font_size=20 ... }`). The
page-wide default is a theme entry —
`theme { font_size = 20 }` in a shared include gives every Label in every
view that default, exactly like the font FAMILY (`theme { font = ... }`) — with
a nearer `font_size` still winning where set. Without one, the built-in
**15 epx default** applies (D88).

### Placement

Two questions, two props, one vocabulary everywhere: **where** (`halign` /
`valign` — position only) and **how big** (`width` / `height` — the sizing
triad). The same words mean the same thing under every parent, so a node
moved from a `Column` to a `Layers` keeps meaning what it says.

```kdl
Column spacing=8 {
    Label title text="settings", font_size=15
    gap 12                                              // one-off break (see gap)
    Button okButton halign=center, text="OK", min_width=120
}

Row spacing=8 {
    Label label text="Size", font_size=15, valign=center
    Slider sizeSlider width=fill, min=1, max=100        // takes the leftover
}

Layers screen {
    Rectangle scrim color=#000000B0, width=fill, height=fill
    Panel toast halign=center, valign=top, margin_top=20 { ... }
}
```

- **The default: a child takes the space its parent gives it.** An axis with
  NO placement key (no size, no alignment, no margin) takes the parent's
  extent: a `Column` gives a full-width slot (the cross axis fills), a
  `Layers` gives its whole rect (layers stack), a Button
  gives its content rect. Sizing, alignment and margins carve that down —
  per axis, so `width=fill` with nothing vertical fills both ways.
  One exception: content
  of the bare default `slot` (the inline marker — its children join the
  widget's own layout with explicit placement only, the shipped rule) keeps
  explicit placement only; content of a NAMED slot region fills the region
  like any free child. The document ROOT is no exception: it fills the
  rectangle the host lays it out in. A box that must keep its natural
  size opts out with `width=hug, height=hug`.
- **The triad**: `width=200` is exactly 200 epx (its content neither grows nor shrinks it);
  `width=hug` is natural content size; `width=fill` takes the leftover space.
  (`width=host` / `height=host`, the Godot version's word for an axis the
  host's code sizes, are gone — build error G0055; the host program sizes a
  page by the rectangle it lays it out in.) `min_width` is the "at least 200, still stretchy" spelling. Hug means
  natural size for wrap-text too (D254): a hugged wrapping `Label` measures
  at its UNWRAPPED single-line width, capped or not — wrapping happens only
  where a width binds it (`max_width=N/fill`, `fill`, a sized parent). A
  wrap that provably never binds is linted (G1004) instead of silently
  collapsing the box to a one-char-per-line sliver (the pre-D254 behavior).
  A `wrap=true, width=fill` Label INSIDE a hug parent has no frame to wrap
  at — the parent is asking the children how wide to be — and answers
  with its longest word only, so the box collapses to a one-word sliver;
  the compiler warns (G1004). Fill wraps where a real frame
  exists: a sized parent, a cap, or a seat's socket (D263).
  The own text of a Button, a Check, a Radio and a Toggle folds by the
  same rule with `wrap=true` (`Button b text="Save a copy", wrap=true,
  width=120`), and the control is as tall as its lines; beside a Check's,
  a Radio's or a Toggle's mark the text has the width the mark leaves.
- **Hug is LIVE** (D141): a content-sized box re-hugs whenever its content's
  size changes while the program runs — a child hidden or re-shown from
  code, text swapped, a picture landing. This covers every measured axis:
  explicit `hug` AND an aligned axis with no size key
  (`Panel valign=top { Column ... }` — the WPF StackPanel shape, which
  shrinks when rows hide).
- **The hug caps** (D132): `width=hug, max_width=336` (and the height twins)
  = natural size, at most the cap — wrap-text hugs its UNWRAPPED extent
  (every hug axis does, D254) and wraps only when the cap binds, which is
  how you spell a hugging label that folds on narrow screens
  (`max_width=fill` = wrap at the live frame). The box re-fits live when
  code-behind swaps its text. The cap holds for any box under any parent:
  a box never reports more than its cap, and content past the cap is CUT
  at the box, as at any written size (`width=200`) — the same as WPF,
  which clips when ink exceeds a `Max*` constraint (a third text line
  under `max_height` comes out sheared mid-glyph). Two children are let
  through: one that hangs out by a negative margin, and an
  `overlay=true` one. Two refusals remain (G1002): a cap that reads the
  parent's space on a stack's OWN stacking axis (`max_width=fill` in a
  `Row`, or any cap on a `fill` slot there) is circular — cap the stack
  itself; and `max_width=fill` / `max_height=fill` on a child of a `Flow`,
  which has no frame to read — cap at a number. Text that wraps is a
  `Label`'s, or the own text of a Button, a Check, a Radio or a Toggle
  with `wrap=true`.
- **The min/max band** (D135): `width=hug, min_width=100, max_width=300`
  clamps the natural size into the pair — below 100 the floor holds, above
  300 the cap binds (wrap-text wraps there). The floor beats the cap (WPF
  clamp order): a runtime `$param` floor that crosses above the cap wins.
  The contradictions are refused at build, for the width and for the
  height alike: a literal floor above a literal cap (G1013); a floor or
  a cap next to an exact size for the same side on one line (G1019: an
  exact size IS its own floor and ceiling — unless a `when` poses that
  size, which the floor and the cap then bound); and a floor on the side
  `aspect=` works out (G1017: the ratio sets that side). A floor under
  `fill` stays legal: "at least N, still stretchy".
- **`max_width=fill`** caps at what a `fill` axis would GET — parent frame
  minus region padding minus margins, live across resizes. Margins bind the
  measuring frame like they inset fill, whatever the alignment, and a
  centered box sits in the middle of the room they leave: `width=hug,
  max_width=fill, halign=center,
  margin_left=12, margin_right=12` is the toast shape — hug the text,
  center it, wrap at live width − 24.
- **The capped fill** (D245): `width=fill, max_width=380, halign=center`
  (and `max_height=` on `height=fill` the same way) = the available
  space, never more than the cap.
  Narrower screens fill edge to edge like plain `fill`; when the cap binds,
  the stated side places the box (`halign=left`, `right` or `center`), and
  with no side written the box is centered, as WPF's Stretch + `MaxWidth`
  does. The
  centered-column-with-a-maximum shape: the effects catalog is
  `width=fill, max_width=380, halign=center` where shipped wrote
  `VertWrapPanel MaxWidth="380"`. On a stack's own stacking axis it is
  refused (G1002: the slot is the frame there) — cap the stack. Margins inset the
  measuring space; the floor beats the cap (D135). A `Scroll` takes the
  cap like any box, and `max_width=fill` on a fill axis is accepted and
  changes nothing (with `halign=center` it is refused, G1011).
- **`aspect=`** (D133): the shape of the node's OWN rectangle (the space
  layout gives the node itself, not its parent) as a ratio — a number
  (width/height: `aspect=1` square, `aspect=1.78` 16:9) or `natural` (an
  `Image` bitmap's own). The axis you spelled drives, the axis you didn't
  is derived; both `fill` + aspect = the largest ratio rectangle that fits
  the space the parent offers, centered (WPF Uniform). Both spelled is an
  error; `hug`/`max_*=` don't combine (v1). Numbers solve under free
  parents; `natural` follows the picture's own ratio everywhere (the
  bitmap-exact Tiles cells). Nothing spelled: a number fits like both
  `fill`; `natural` takes the width its parent offers and derives the height.
- **`fit=contain/cover/distort`** (D133, `Image` only): how the bitmap maps
  into the Image's own rectangle, orthogonal to how that rectangle is sized
  — `contain` letterboxes (default), `cover` fills and clips, `distort`
  stretches. Replaces the retired `stretch_mode=`/`expand_mode=` consts.
- **Weighted fills**: siblings with `width=fill(2)` and `width=fill` split
  the **leftover** space (what remains after fixed/hug siblings take their
  minimums) two-to-one — weights are ratios, not fractions of the parent.
  `hug(2)` is the rare "weighted slot around a natural-size control" combo.
- **Alignment is position only**: `halign=center, width=fill` is a build-time
  error (fill leaves nothing to position — use `hug`). In a `Row`/`Column` the
  cross axis is where alignment shows; on the main axis a child takes its
  own size unless it fills.
- **Margins carve — they never pick the sizing mode.** A margin places the
  node's edge at a signed distance from the parent's matching edge —
  positive inside, **negative past it** — and what it does follows from
  what the axis's box already is:
  - **default / `fill` axis** → an INSET of the filled box; the unnamed edge
    stays bound at inset 0 (the WPF Margin-under-Stretch reading — signed,
    so a node can exceed its parent). `Rectangle strip margin_bottom=-10`
    on a free child spans its parent and hangs 10 epx below it.
  - **aligned axis** → an offset of the near edge (`valign=bottom,
    margin_bottom=90` sits 90 above the bottom). A margin on the far edge
    of an aligned axis is accepted and does not move the box; it still
    counts into what a hugging parent holds and narrows a
    `max_width=fill` cap. On a centered axis the box is centered in the
    room the margins leave: one margin shifts it by half of itself, two
    by half their difference.
  - **`hug`/exact axis with no alignment** → a PIN: the pinned edge moves,
    the other follows at size distance (declared size, else the live
    minimum), and grow points away from the pin.
  The `margin=N` shorthand is breathing room: insets all around on
  default-filled axes, the touched edges under alignment (`halign=right,
  valign=bottom, margin=16` keeps meaning "a fab, 16 off the corner").
  **Two per-edge keys bind both edges** — the box spans them (a declared
  size there over-constrains and errors). A document ROOT follows the
  same rules against the rectangle the host gives it: a lone margin
  there insets the filled root, it does not pin it. Under a box that
  places its children itself (a Column, a Row, a `choice`, a Flow, a
  Grid, Tiles, a List) margins are a build error (G1014) — space those
  children with `spacing` / `gap`, or `padding` on the parent. The one
  body of a `Scroll` takes margins.
- **`anchors=` / `offset_*` were removed** — placement says it all now. The
  old escape hatches translate mechanically: overhang = a negative margin
  (plus an explicit size for an exact box); off-screen parking = a negative
  pin past the whole box (`valign=bottom, height=50, margin_bottom=-90`);
  "offsets an anim drives" = the motion channel (`translate`, below) or an
  `arrangement` when the pose stands.
- (`inset=N` and `size_flags_*` are gone — G2001 and G0041: a bare child
  already fills, and `margin=N` carves that box; a document root fills
  the same way, with nothing spelled.)
- An anim line may move a size, a margin or a padding (see Motion: the box
  is laid out again on every frame of the play); `halign`/`valign` are no
  anim targets. Arrangement `when`
  responses may overlay `margin_*`/`width`/`height`/`min_width`/
  `min_height` per node, and that pose holds for as long as the
  arrangement does.
- Don't confuse `halign` with `horizontal_alignment`: `halign` places the
  control; `horizontal_alignment`/`vertical_alignment` place *text* inside a
  Label's own box.

### View — the screen root

The root keyword set is **{`View`, `Panel`}**: a file announces itself as a
SCREEN or a COMPONENT in its first word (grammar, not convention). `View` is
the screen: the host program lays this node out in the rectangle it
passes, and its children place **freely**
(`halign`/`valign` + margins), so the natural first draft of a dialog
compiles with no wrapper:

```kdl
View settingsSheet {
    Panel card interactive=true, margin_left=40, margin_right=40,
            valign=center, background=#1F1F1FFF, padding=12 { ... }
    Button closeButton 44x44, halign=right, valign=top, margin_top=12 { ... }
}
```

- **Defaults**: transparent background + `width=fill, height=fill` — the
  incantation every view used to open with, baked in. Anything the file
  spells wins (an explicit `background=`, a `halign=`/`width=` on that axis,
  a bare size); the defaults only fill silence. The root is also
  input-transparent like every passive node (see Input) — `interactive=true`
  on the root declares a full-screen surface.
- **Free placement + content minimum**: root children take the Layers
  treatment (later = on top, margins pin), and the root reports its biggest
  child as its minimum — a hug-placed root (a corner chevron) sizes to its
  content, live.
- **The root plate**: `View background=..., corner_radius=...,
  border_*=...` draws on the root itself, as on any box (a Column, a Row
  and a `Layers` take the same words). `padding` on the root
  insets every child's placement (D114) and counts into the derived
  minimum.
- **One position**: `View` inside the tree errors (inner free placement is
  `Layers`, a styled box is `Panel`); a root that is neither `View` nor
  `Panel` errors (G7003, naming both doors).
- NOT hidden by default: `visibility=gone` is state the file declares, never
  an identity default.

### Panel — the component root

A file whose root is `Panel` is a COMPONENT: the root IS the styled box — a
toast banner, a progress track. No wrapper node, no extra nesting; the
file's children place freely inside it (`halign`/`valign` + margins), as
under a `View` root; `padding` and the border inset them the same way in
both:

```kdl
Panel tipPanel background=#000000FF, padding=12, valign=top, width=fill,
        visibility=gone, margin_left=12, margin_right=12, margin_top=180 {
    Label tipText text="", font_size=$fontMediumLarge
}
```

The host program shows a component like any page: it lays it out in the
rectangle it chose for it — the whole window, or a box inside another
page. Pick by concept: *is this file a screen the app shows, or a box the
app embeds?* The first word answers it for every future reader. (Inside a
tree, `Panel` stays the ordinary styled box it always was — root position
is what makes it a component.)

Like every gogo Panel, the root starts on the blank slate: where the file
doesn't speak `background=` (node line or style), nothing is drawn.

**The root places itself in the rectangle the host gives it**: `width=fill`
/ `valign=top` / margins on the root are read against that rectangle,
exactly as a child's are read against its parent.

### gap — space in a stack

`gap` is the one-off break between stack children — a structure word
(lowercase, like `row` and `skip`: never named):

```kdl
Column spacing=8 {
    Label title text="Brushes"
    gap 12        // fixed: 12 epx along the stack's axis
    Row { ... }
}
Row {
    Label caption text="Size"
    gap fill      // elastic: pushes what follows to the far edge
    Button close text="X"
}
```

- `spacing=` is the uniform air between EVERY pair of children; `gap` is the
  one-off — a bigger break between groups, or the classic push-apart.
- Row/Column only. Elsewhere the error routes to the owner of spacing there:
  Flow and Grid → `spacing=` (a Grid's gutters; `row_height` sizes a
  row, it is not a gap), Layers → margins.
- One size word, nothing else — no name, no props, no children. Space you
  must ADDRESS (name, style, animate — e.g. a when-collapsed spacer) is a
  layout element: spell it as a real node.

### Layers

`Layers` is **free placement + z-stack**: `Column` stacks children on y,
`Row` on x, `Layers` on z. Children overlap in declaration order (later = on
top) and place themselves with the normal placement vocabulary — nothing
arranges them for you:

```kdl
Layers screen {
    Rectangle bg color=$chrome, width=fill, height=fill   // backdrop = a layer
    Image photo width=fill, height=fill
    Panel toast halign=center, valign=top, margin_top=20 { ... }
}
```

What it does:

- **It has a minimum size**: the element-wise max of its visible children's
  minimums, so a `Layers` inside a `Column`/`Row` takes its biggest child's
  space instead of collapsing to zero. `min_size=` on the Layers itself still
  combines via max; `fill` children typically contribute 0 — they adapt to
  the Layers, not the other way around.
- **It takes `padding_*`** (D114): the declared air folds into every child's
  placement — `Layers padding_left=10, padding_right=10 { ... }` insets its
  content with no `Panel` wrapper — and adds onto the derived minimum.
  Asymmetric padding shifts a centered child to the padded box's true
  center, and an axis with nothing declared starts AT the padding instead of
  parked at 0, exactly like a slot region (§3.4).

Like all passive structure it is input-transparent (see Input); a Layers
that IS the gesture surface — the whole-row drag idiom — declares
`interactive=true`.

A Layers takes the plate words like any box: `background=`, a border and
`corner_radius=` draw behind its children, with no `Panel` wrapper. The
backdrop can also be the first layer (tint or fade it like any other
node).

### Grid

Shared tracks across rows — the layout `Row`/`Column` can't say (label
columns that line up, spans, weighted splits). No coordinates: one Grid uses
exactly ONE of two placement forms, and errors route between them.
`spacing=` puts the same gutter between every pair of tracks AND every
pair of rows — the word Column/Row/Flow use (decided 2026-08-30); no
spacer tracks or `skip`s needed for air.

**Row form** — placement IS structure. `cols=` is the signature every row is
checked against; cells walk left to right:

```kdl
Grid cols=[hug, fill, 60] {                 // tracks: epx / hug / fill(n)
    row { Label lbl text="Size"; Slider s; Label val }
    row height=fill {
        Image preview col_span=3            // spans are plain cell props
    }
}
```

- Row heights come from their `row` block (`height=` in the same triad, default
  `hug`; `row_height=` on the Grid line sets a uniform default). There is no
  `rows=` list — nothing to desync when you reorder.
- `skip` spells every interior hole, one per empty track; trailing tracks
  may simply be omitted. A `row_span` OWNS the tracks it covers below — each
  covered track is written `skip` there (the error walks you through it).
- Spans past the last row/column are build errors (WPF silently clamps).
  Overlapping spans are unrepresentable: a whole-grid overlay (WPF's
  `RowSpan=99` scrim idiom) is a `Layers` sibling instead, and crossing
  guides decompose into one grid per axis.
- Cells place inside their cell with the normal vocabulary (halign/valign;
  the default fills the cell). Weights belong to TRACKS: `width=fill(2)` on
  a cell is an error routing to `cols=`.

**Areas form** — placement by NAME, for page shells (CSS grid-template-areas
with heights riding each row):

```kdl
Grid cols=[hug, fill],
     areas=["header header : 60",
            "nav    body   : fill",
            "footer footer : 40"] {
    Panel header { ... }                    // children bind by node name
    Column nav { ... }
    Rectangle footer color=$chrome
}
```

One quoted string per row (CSS's own spelling); every row ends with its
`: height` — REQUIRED (a missing one is a build error; `row_height=` belongs
to the row form; D89: a silent `hug` default made dead space under the grid
hard to diagnose); `.` marks an empty region; every area must be one
contiguous rectangle; an area with no child is only a warning. Heights are
literals here ($refs need the row form).

Track behavior: a hug track sizes to its single-span
cells' max minimum, spanned deficits spread into hug tracks; the grid's
minimum is the track sums (fill content included); fill tracks split the
leftover purely by weight — WPF star parity: squeezing a grid below its
minimum crushes fill content proportionally rather than lying about ratios.
An advisory lint (D89) flags a `height=fill` / `width=fill` grid none of
whose tracks is `fill`: the leftover space after the last row/column is
dead — give one track `fill`, or size the grid `hug`.
An invisible cell keeps its cell but stops contributing size (WPF
Collapsed). Uniform flowing mosaics (N thumbnails a row) are `Tiles`;
generated repetition (calendars, tables) waits for `repeat`.

### Tiles

The unit-lattice wrap panel (D94) — the phone-launcher mosaic. Where Grid
promises *placement is structure*, Tiles promises the opposite: cells flow in
declaration order over `cols=` equal-width units and placement EMERGES —
first-fit wrap, no backfill, so a span that can't fit heads the next row and
the vacated tail stays empty.

```kdl
Tiles gallery cols=2 {
    Label title text="top picks", full_row=true    // the whole row
    Panel a { Image source="res://a.jpg", aspect=natural }
    Panel b { Image source="res://b.jpg", aspect=natural }
    Panel wide span=2 { ... }                      // two units of width
}
```

- `cols=N` (required — the count every `span=` is checked against) splits the
  Tiles' width into N equal units; it is a LIVE property, so code can
  re-pack the mosaic (`rt.set_prop(tiles, PropKey::Cols, Value::VNum(3.0))`).
- `spacing=N` puts N between the columns and between the rows, as on a
  Column or a Grid; with none written the cells touch. A cell of two
  units covers the gutter between them.
- `cells=lattice` (the default) or `cells=natural` (D112): with `natural`
  every cell is as wide as its own content and the row wraps where the next
  one would not fit — a names-only catalog, a row of tags. `span=` says
  nothing then (there is no unit to count); `full_row=true` still takes the
  row and an exact width still packs at that width. A cell wider than the
  Tiles is capped at the Tiles' width — give a chip's label `wrap=true` and
  a long name folds inside that row instead of running off the edge. Live
  like `cols=`, and a pose may
  flip it (`when names { panel.cells = natural }`) — one list, two packings:

  ```kdl partial
  Tiles catalog cols=2, cells=lattice {
      Label title text="top effects", full_row=true
      Panel colorize { Image .PictureOnly source="colorize.jpg", aspect=natural; NameChip text="Colorize" }
      Panel darkDream { Image .PictureOnly source="dark.jpg", aspect=natural; NameChip text="Dark Dream" }
  }
  when names { catalog.cells = natural }   // chips pack at their own width
  ```
- Cell props: `span=K` takes K units (over `cols=` is a build error);
  `full_row=true` takes the whole row at any column count (exclusive with
  `span=` — a full row IS the whole span).
- Cell HEIGHT is the cell's own content, top-aligned in its row; the row is
  as tall as its tallest cell. There is no lattice quantum for heights: an
  `Image` with `aspect=natural` makes the bitmap's ratio
  size the tile (square bitmaps → square tiles) — WPF's measure-with-
  constraint answer without the constraint protocol.
- An invisible cell REFLOWS away entirely (wrap semantics — Grid keeps the
  cell, Tiles doesn't have one to keep).
- Width feedback: like a wrapping Label, the minimum HEIGHT depends on the
  granted width. The minimum WIDTH is the widest cell's
  OWN minimum — never zero, so a hugging parent still
  grants a real width instead of deadlocking the pack at 0, and never
  lattice-scaled: oversized content never widens the mosaic.
- Content too big for its cell is the content's to fix:
  `clip_text=true` on a Label (a
  clipping Label demands no width — how tile captions truncate), a
  `clip_contents=true` wrapper for anything else, or a wider cell (`span=`).

Use `Flow` when items should keep their natural sizes; `Grid` when rows and
columns are addressed structure; `Tiles` when equal columns rule and content
flows through them.

**How wide a Flow is.** `width=fill` (or no alignment): the Flow takes the
width it is given and every row starts at its left edge. A hugging Flow
(`halign=center`, no width) packs its rows into the width it is offered,
then is as wide as its widest wrapped row. So
`Flow halign=center` is a centred block of full rows, and a short last row
starts at the block's left:

```kdl
Column width=fill {                 // 350 wide
    Flow cells halign=center {
        Rectangle a 100x10
        Rectangle b 100x10
        Rectangle c 100x10
        Rectangle d 100x10
        Rectangle e 100x10
    }                               // rows of 3 and 2: the block is 300 wide at x=25
}
```

What a Flow asks its parent for (its minimum) stays the widest single
item: a Flow can always wrap down to one item per row.

### Safe-area insets — the `safe` value

`safe` is a length value for the padding/margin families: the screen's
safe-area inset for that edge — camera cutout, rounded corners, home bar —
written by the host program whenever it changes:

```rust
    // The screen's insets in epx: left, top, right, bottom.
    host.rt.safe = [0.0, 24.0, 0.0, 0.0];
    host.rt.dirty = true; // a plain field: say that the page must lay out again
```

```kdl
Layers screen {
    Image photo width=fill, height=fill      // full-bleed — under the cutout
    Layers content margin=safe {             // carved clear of every inset
        Column links halign=left, valign=top, margin_left=30, margin_top=20 { ... }
        Button back halign=left, valign=bottom, text="back"
    }
}
```

- **Opt-in, per edge**: write only the edges the element actually REACHES —
  a left-docked sidebar says `padding_left=safe` and never names the right
  (an inset for an edge the element can't meet would only add dead space;
  user-caught, D73/D74). `padding=safe` / `margin=safe` are defined sugar
  for the four per-edge keys, each edge reading its OWN inset.
- **Both families, one value**: `padding_*=safe` counts into the hug and
  sits under the background/hit rect (bars, scroll content);
  `margin_*=safe` pins/carves a free child (a close button riding the top
  inset; a full-screen `margin=safe` layer is the classic safe frame).
  Any other key errors — `safe` is an edge inset, and only edge keys have
  an edge to read (`spacing=safe` routes there).
- Design offsets and insets compose STRUCTURALLY, same as ever — by
  nesting, never arithmetic: the outer element carries `safe`, the child
  its design value (`margin_top=20` inside a `margin=safe` layer reads
  "20 epx below whatever the cutout needs"). No code-behind re-deriving
  offsets on rotation.
- Content that must **scroll under** a cutout pads the scroll CONTENT:
  `Scroll { Panel padding_left=safe, padding_top=safe { ... } }` —
  at rest content starts clear of the cutout, scrolled content glides
  under it and clips at the physical screen edge (COOKBOOK "scroll under
  the notch"). Insets that frame the viewport instead would cut scrolled
  rows at the safe line in mid-air.
- Full-bleed content (backdrops, canvases, scrims) simply names no safe
  edges.
- Insets are epx, like every length: the OS reports device pixels, and the
  host divides by the display's scale before it writes them. Every `safe`
  padding and margin follows at the next frame (orientation flip, inset
  simulation).

### Scroll

`Scroll` owns touch panning itself (WPF-ScrollViewer style), so it works
with **no input tuning**:

- A drag starting on any child — including plain buttons — pans the list once
  it travels past the deadzone (16 epx), and the child's press is
  cancelled: `activate` only
  ever fires for genuine taps. **The direction the finger commits to
  decides who gets it** (decided 2026-08-30): once past the deadzone the
  drag's dominant direction is read once. A Slider keeps a sideways drag
  (its own axis) and a Toggle keeps a drag it already started; otherwise
  the innermost Scroll or List that scrolls THAT way and has somewhere to
  go takes the gesture and the child's press is cancelled (a Slider's
  value goes back to where the finger found it). If nothing around the
  child scrolls that way, the child keeps the finger for good — a
  sideways drag on a Button in a vertical list steals nothing.
- Releasing mid-pan glides with velocity (WP's flicks outrun the finger,
  then decay); touching a gliding list stops it without pressing what's
  under the finger (a glide slower than 60 epx a second stops and the
  press goes on to what is under the finger).
- Edges are springy (Windows-Phone feel): content pulls past its limit with
  a diminishing give and springs back on release; a glide that hits an edge
  is absorbed and settles back.
- The glide and spring numbers are fixed in the crate; a file cannot tune
  them (`fling_boost=`, `spring_stiffness=` and their kin are not gogo
  words). Gesture thresholds are epx (design units, D95), so one flick
  feels the same in a desktop window and on a 3-px/epx phone.
- The mouse wheel does not pan a Scroll yet (it steps a Slider or Toggle
  under the pointer).
- **Content takes the viewport** (D94): an unkeyed axis on the Scroll's child
  sizes to the viewport and scrolls past it when its minimum is larger — the
  WPF ScrollViewer reading, and the one CONTAINER case of the free-fill
  default. A declared
  `width=`/`halign=` keeps its meaning, per axis.
- **An axis that does not pan holds its content to the viewport** (D109):
  there the viewport counts like a width you wrote, so `fill` content stays
  on screen and wrap text wraps at the viewport. One line too long to wrap
  is cut at the edge on its own; it no longer widens every full-width row
  with it. The Scroll asks its parent for no more than it is offered on
  that axis, so one long line never grows the page past the window:

  ```kdl
  Scroll allow=vertical {
      Column width=fill {
          Label text="one line far wider than the phone"   // cut at the edge
          Label text="a paragraph", wrap=true, width=fill  // wraps on screen
      }
  }
  ```

  Heights are measured without a budget, so a sideways Scroll holds its
  content's height when its own height is a number or comes from one
  (`height=60`, `height=hug`, inside a box with a written height).
- **`allow=` names the pan axes; the sizing triad infers them** (D138;
  D125 grown up): `allow=vertical|horizontal|both` owns both axes when
  present — listed axes scroll, the rest are off (the fill-width page that
  must not pan sideways, in one word). Absent, the triad infers: a `hug`
  axis is content-sized, and a content-sized axis has nothing to scroll;
  a number,
  `fill` or an unkeyed axis scrolls. `Scroll height=100, width=hug` is a
  menu: as wide as its widest row, exactly 100 tall, panning vertically —
  the WPF default ScrollViewer, spelled out.
- **`bars=` paints the scrolling axes' bars** (D138):
  `auto|never|always|reserve` — `never` hides the bar, the axis still pans
  (the handheld look); `reserve` shows like auto but always holds the
  bar's space, so content doesn't shift when it appears; `always` holds
  the space and shows the bar with nothing to scroll too. Inert on a
  non-scrolling axis. A `List` reads the word the same way.
- **A capped hug axis scrolls PAST its cap** (D138 — the List D124
  doctrine on Scroll): `height=hug, max_height=300` follows the content up
  to 300 epx, then the axis scrolls. The contradictions are build errors
  (G1020): a hug axis capped with `max_*=fill` (it would grow with its
  content and never scroll — write `fill` for an axis that scrolls past
  its frame, or cap at a number); a bare hug axis that pans, listed in
  `allow=` or aligned with no size word (it would be 0 long: a panning
  axis asks its parent for no room); and a capped axis left out of
  `allow=` (cut at its cap with no way to scroll). Both axes hugged
  bare = nothing left to scroll: a warning (G1021).

### Input

One sentence covers the model: **interactive controls (`Button`, `Slider`,
`Input`, `Scroll`, …) answer input by default, everything else is
transparent, and `interactive=true/false` overrides that — per control, like
any property.**

```kdl
Panel card interactive=true, background=#1F1F1FFF { ... }  // a plate that blocks
Button demo text="OK", interactive=false                   // hit-invisible, undimmed
Rectangle shield interactive=true, color=#00000000         // the catcher idiom
```

- `interactive=true` claims input for THIS control (its rect becomes a hit
  target: a tap on it stops there and presses nothing behind or around
  it, not even a Button it sits in; a drag on it still pans the Scroll it
  sits in). `interactive=false`
  makes an interactive control hit-invisible — unlike `disabled`, it never
  restyles. Neither touches the subtree: children keep their own behavior.
- Passive means every display/layout built-in (`Panel`, `Rectangle`, `Image`,
  `Label`, `Layers`, `Row`/`Column`, `Grid`, `Tiles`, `Circle`, `Gradient`,
  the `View` root) — none of them ever claims a press. Button content can't
  steal the button's press *by construction* — WPF's null-background
  hit-invisibility, as the whole model rather than a special case.
- **Region silencing is a catcher**: input is per-control, so "block
  everything over there" is an invisible `interactive=true` plate drawn over
  the region (the `shield` above). `interactive=false` on a passive container
  is a no-op.
- Legal in `when` blocks like any bool (`when held shield.interactive=true`)
  — applies instantly, restores on release.
- **Finding a dead surface**: `gogo_ui::hit(&page, frame, x, y, false)`
  answers with the node a touch at that point lands on, or none (the
  cookbook: "How do I debug a tap that misses?").
- A box the host draws itself takes the raw touch with `on:pointer=<name>`
  — every phase with its position inside the box — and needs
  `interactive=true` on the same node (G0073: the wire alone claims
  nothing).

### Button content sizing

A Button sizes to its content, as a WPF Button does: a button with
children derives its minimum per axis as `max(declared min, content min)`,
where content is the Layers formula — the element-wise max of visible
children's combined minimums plus their margins (D242; a negative margin
takes off). It stays
live: a child's text, font or visibility change re-derives the button.

The rules, per axis:

- `min_width`/`min_height`/`min_size` merge with content via max (they stay
  minimums).
- An exact `width`/`height`/`WxH` **clamps** its axis — content never grows
  it, like WPF's explicit `Width` (a button exact on both axes skips the
  machinery entirely).
- Pin-placed buttons participate too: their content minimum is what the pin
  math sizes an un-declared axis with.
- Positive placement margins on the content count into it (D242, same rule
  as Layers): the empty space you declared is space the button reserves,
  so `margin_left=49` widens the minimum exactly like `padding_left=49`.
  Negative margins mean "stick out past the edge on purpose" (labels that
  overflow their slot) and come OFF the minimum (a 100-wide child with
  `margin_left=-30` asks for 70; never below zero). A margin on an edge
  the axis' alignment does not use (the right edge under `halign=left`)
  is accepted and counts the same way. `Panel
  padding=6 { Row {...} }` remains the WPF `Padding` analog.

### Image

`Image` shows a picture, with WPF Image semantics baked in:

- Defaults to `fit=contain` (WPF `Stretch=Uniform`): the texture scales to
  fit the Image's OWN rectangle — the space layout gives the Image node
  itself, not the Panel/Column holding it — centered, and never dictates a
  minimum size, no boilerplate needed. Note: strict WPF draws the scaled
  bitmap top-LEFT within its rectangle; gogo centers it (the friendlier
  default). A non-square glyph that must not float in a slack rectangle:
  make the Image's rectangle the glyph's ratio instead — `width=16,
  aspect=natural`.
- `fit=` (D133) is the content-mapping knob — bitmap into the Image's own
  rectangle: `contain` letterboxes (the default), `cover` fills the
  rectangle and clips the overhang, `distort` stretches (named for what it
  does — the CSS/WPF word `fill` is gogo's sizing word). When the Image's
  rectangle and the bitmap share a shape, all three draw the same pixels.
  `aspect=natural` is the other direction: the RECTANGLE follows the
  bitmap's ratio from the driving axis (see the sizing triad).
- Sizing BOTH axes (`Image icon.png 16x16`, or `width=` + `height=`) makes it
  an exact box, like a XAML Image with `Width`/`Height` — containers
  won't stretch it. A Panel, a Grid cell or a Scroll centers that box, a
  Column, Row or Flow centers it across its line; under a `Layers`, the
  `View` root, a Button or a `Tiles` it sits at the top-left like any
  sized box (write `halign=center, valign=center` there). Without this, a
  width-stretched box silently scales the glyph up to fit.
- Sizing ONE axis (`min_height=32`) keeps normal container fill on the other.
- Sizing NOTHING (no size word, no `aspect=`): the Image fills the space
  its parent gives it, and the picture fits inside (`Button { Image
  photo.jpg }` — a big picture never grows the Button). In a Column the
  height is not given, so it comes from the picture: the Column's width,
  and the height the picture's shape makes at that width (`Column
  width=300 { Image photo.jpg }` with a 600x400 photo = 300x200; WPF
  Uniform in a vertical StackPanel). In a Row a bare Image
  is still 0 wide — write `height=` + `aspect=natural` there.
- The picture's own size: `size=hug` (= `width=hug, height=hug`; an Image's content is
  its picture; one picture pixel per layout unit, so on a phone that
  draws 3 screen pixels per unit it shows 3x enlarged — a crisp picture
  needs a bigger file). One side `hug` sizes that side only. Only the
  written word counts: an alignment alone does not read the picture.
- `loading=lazy` (default `eager`) is a word for the host program: gogo's
  own crates check that the file exists at build time and read nothing
  else from it. A host that honors it decodes the picture off the frame,
  starting the first frame that shows it, and the node draws empty until
  the pixels land. Made for image-heavy catalogs. For the layout not to
  move when the pixels arrive, the host answers the picture's size up
  front (`TextMeasure::image_size`; `gogo_ui::atlas::picture_size` reads
  it from a PNG or JPEG header). Code that must see settled pixels
  (screenshots, reveal gates) asks the host whether a decode is still out.
- `source=` also takes an **atlas slice** — `animals[12]`, `toolbar.play`,
  `bubble` (see Directives → Atlases). A slice lays out, fits and
  `loading=lazy`-decodes as its own picture would; the file behind it
  decodes once for every slice.
- `window=[left, top, width, height]` shows only that
  rectangle of the picture, in its pixels; the fit maps the rectangle,
  not the whole picture. For pieces the file names more than once,
  prefer an atlas — `window=` on an Image showing a slice is an error
  (G2018). `sampling=pixel` draws the picture unsmoothed (hard pixels
  when scaled up); the default is `smooth`.
- `release=unused`: the app may free this picture while no
  page names it and decode it again when one does; default: kept for the
  session. See Directives → Atlases for the rule (every declaring line
  must say it).

### Circle

`Circle` is the disc/ring type — an antialiased disc drawn in a plain box. The
radius self-derives: `min(width, height) / 2`. Size the box, get the circle.

```kdl
Circle dot 24x24, color=$accent                       // a filled disc
Circle ring width=50, height=50,                      // a ring: clear fill +
    border_color=$accent, border_thickness=1.5        // a stroked rim
```

- Like every gogo panel, a bare Circle draws NOTHING (the blank slate):
  `color=` fills the disc; `border_color=`/`border_thickness=` stroke the
  rim (inward — the ring never overflows the box). All three are real
  properties: statics, whens and anims drive them
  (`when hovered color=#FF0000FF`).
- Box words route away — a circle has no box to pad or round
  (`corner_radius`/`padding` error; `background=` routes to `color=`).
- A non-square box still draws a CIRCLE (radius from the short side) —
  a true ellipse stays out of v1.

### Gradient

`Gradient` is the generated-gradient fill, drawn per pixel over the node's
box. Three fill patterns × three
repeat modes × arbitrary stops × from/to geometry.

```kdl
Gradient veil  fill=radial, stops=[accent, accent/0]        // fade to clear
Gradient sky   angle=90, stops=[blue, indigo], repeat=mirror // linear, aimed
Gradient wheel fill=conic, stops=[red, yellow, green, blue, red]
Gradient flash fill=radial, stops=[#FFFFFF6B@0, #FFFFFF6B@0.94, #FFFFFF6B/0@1]
```

- `stops=[...]` — REQUIRED (no hidden default ramp). Entries are any color
  spelling (hex, palette names, `$refs`; the `/opacity` suffix is allowed), optionally
  pinned with **`@position`** (0..1, always after any `/`): no positions =
  even spacing; **if one entry is pinned, all must be** (all-or-none);
  positions never decrease (equal = a hard step); under `repeat=mirror` they
  stop at 0.5 (the second half is the reflection). One stop = error `G2003`
  with a behavior fix — a solid is a `Rectangle`. **Fading out**: stops
  mix with their alpha already multiplied in (premultiplied Oklab), so a
  stop at alpha 0 adds no color of its own — `white/0` and `#00000000`
  fade the same, clean, with no dark edge. Bare `transparent` is still
  refused in `stops=` (G2006): write `white/0` or `black/0`. (WPF's
  default stop color is transparent black — a XAML `GradientStop` with no
  `Color=` fades dark there; in gogo `#00000000` fades clean, so a dark
  edge that is the wanted look needs a dark stop of its own.)
- `fill=linear|radial|conic` (default `linear`). `repeat=
  none|repeat|mirror`.
- **Aiming**: `angle=` (linear only; the CSS compass — 0 = to top, 90 = to
  right) or `from=`/`to=` box points (`0.5x0.5` = center; 0..1 of the box) —
  spelling both is an error. `from=`/`to=` aim a linear fill only: on
  `radial` and `conic` the compiler accepts them and the draw ignores
  them. Defaults: linear runs left→right; the round
  fills center in the box and reach the right edge.
- **Stops stay live**: `stop_1_color`, `stop_1_offset`, ... are real
  per-stop properties — whens and anims drive them like anything else
  (`flash.stop_1_color.a from=0, to=0.42`). Numbered from **1** in
  declaration order: `stop_1` is the FIRST stop in `stops=[...]`, not an
  array index. A driven offset never goes below the stop before it, and
  it pushes the stops after it along when it passes them: the
  declared order stays law.
- `opacity` stays free for whole-node fades. The old `Radial` type retired into this:
  `Radial color=C, solid_until=S` → `Gradient fill=radial,
  stops=[C@0, C@S, C/0@1]`.

### choice — one of N

`choice` scopes an exclusive set of `Radio`s. It is a structure word
(lowercase): the scope is erased at build — children parent flat, in order —
and every `Radio` inside joins ONE group, however deep the layout nests:

```kdl
choice size value=medium {
    Radio small text="Small"
    Radio medium text="Medium"
    Radio large text="Large"
}
```

The same set, laid out in columns:

```kdl
choice size value=medium {
    Row {
        Column { Radio small text="Small";  Radio medium text="Medium" }
        Column { Radio large text="Large" }
    }
}
```

- Every `Radio` in the scope joins one exclusive group. Radios NEED names — the
  name is the value the choice reports. `Check` is the independent on/off
  and never joins a group (inside a choice it errors, routing to Radio).
- **Templated radios** (D142): a Radio with a BODY draws none of the default
  radio visuals — the body is the whole look, on a blank plate
  (`background=`/styles color it; the FOCUS ring stays, so keyboard focus
  still indicates). `selected` + whens
  carry the selection visual. `text=` on a bodied Radio errors — letters
  live in the body's own Labels:

  ```kdl
  choice paintMode value=color {
      Radio color 52x50 {
          Circle colorRing 35x35, border_color=#FFFFFFFF, border_thickness=1.5
          Label text="color", font_size=11, valign=top, margin_top=36
          when selected colorRing.color=$accent
      }
      Radio rainbow 52x50 { ... }
  }
  ```

  The same body spelling works on `Check`: a bodied `Check name { ... }`
  is the independent twin — the same blank plate and `selected`/whens, NO
  group, and a `change` carrying a bool like the plain form.

  A repeated look factors into a **host widget** (`widget RadioLook
  for=Radio` — see Widgets § Host widgets, D276): the widget is the whole
  body, and its widget-scope `when selected` binds to the hosting Radio.
- `value=<name>` pre-presses that Radio (optional — omit for none-selected).
- **Wiring**: a named choice reports its pick as a `Change` event whose
  value is the picked Radio's name; the handler it names is the choice's
  own name (`paintMode`), or the one written with `on:change=<handler>` /
  `on:change=emit(Name)`. Items refuse
  `on:change` (the value is "which one" and lives on the group; rows
  expose `when selected`).
- **changed means CHANGED** (D142): one report per genuine transition, both
  radio kinds. Re-tapping the current pick keeps it pressed and does NOT
  re-report.
- Exclusivity has a **lexical home**: radios in disjoint subtrees can't
  share a group — if two far-apart controls must be exclusive, restructure
  so one `choice` encloses both (a feature: the reader sees the whole set).
  Scopes don't nest.

## Repeat (lists from data)

`repeat` renders N children from a code-supplied collection, reconciled by
key (design: `docs/design/2026-07-15-spec-repeat-and-list.md`; codes G90xx
build / G91xx runtime refusals).

```kdl partial
Column spacing=8 {
    Label text="Today"
    repeat tasks as $task by $task.id {     // as optional: default $item
        empty { Label text="Nothing due today" }
        TaskRow title=$task.title, done=$task.done
    }
    Label text="end"
}
```

Code hands the repeat its rows through the repeat's node, every time the
collection changes (here `host.rt` is the page's `Runtime`):

```rust
fn task(id: i64, title: &str, done: bool) -> Item {
    Item::Record(vec![("id".to_string(), ItemValue::Int(id)), ("title".to_string(), ItemValue::Str(title.to_string())), ("done".to_string(), ItemValue::Bool(done))])
}
```

```rust
    let tasks: NodeId = host.page.find("tasks").expect("repeat tasks");
    let picker: NodeId = host.page.find("picker").expect("List picker");

    // The whole collection, every time it changes. Rows are matched by
    // key: a kept key keeps its row.
    host.rt.set_items(&mut host.page, tasks, &[task(1, "Buy milk", false), task(2, "Call mom", true)]).expect("two rows with different keys");
```

- **The line**: `repeat <name> [as $binding] by <key-path> { ... }` — the
  name is required (it's the `set_items` address, unique per view; a repeat
  without one is a build error, G9002); clauses
  are keyword-anchored and COMMA-FREE (the arrow-endpoint precedent), `as`
  before `by`; looks belong on the parent or the template, not on the
  line. Omitted `as` is defined sugar for `as $item`;
  since `by` must key the binding (`by $item.id` names one field: an item
  is a flat record, so there are no nested fields; bare
  `by $item` = the item IS its key — G9003 otherwise), every repeat line
  displays its item variable. A binding that would shadow a theme value or
  widget param in scope errors with the rename fix-it (G9004).
- **Erased structure**: the repeat itself draws nothing and takes no room.
  Its rows lay out as children of the repeat's parent, so a `Column`'s
  `spacing` spaces them and static siblings mix freely, in source order.
  Multiple repeats per container are fine.
- **The counted form**: `repeat marks capacity=8 { <template> }` has no
  `by`. Code shows the first N rows with `Runtime::set_count` and fills
  them itself (`set_text`, `set_prop`). `capacity=` reserves rows up front
  and is never a ceiling: a bigger count grows past it.
- **`$binding.field` reads the item's data** — a STATIC value re-applied at
  each assignment (it does not change between two assignments).
  Dots on any other `$ref` are an error: reading one is G6001 (no such
  value), naming a value or a parameter with a dot is G9008. A field may
  feed any property: text, a number, true or false, a color, a word
  (`visibility=$item.vis`) or a size (`width=$item.w`), also through a
  widget's parameter (`TaskRow done=$item.done`). The field must hold
  what the property takes; structures don't bind whole.
- **Reconcile by key** on every `set_items`: a kept key with equal data
  keeps its row untouched (focus, states, anim positions survive), changed
  data re-binds its row in place, an absent key frees its row for reuse,
  order follows the collection. A row follows its KEY: after a re-sort
  or an edit, a row's pressed, selected or focused look stays with its
  item.
- **Refusals, never halts**: a duplicate key, a missing `by` field, a key
  that is neither text nor a whole number, mixed key types, and data the
  template cannot take (a missing field, or a field of another type
  than its property takes: a whole number in `text=`, text in `color=`,
  a word the property does not have) each refuse the WHOLE assignment — `set_items`
  returns an error carrying the code (G9101/G9102/G9103) and the last
  good rows stay. An empty list clears legitimately.
- **`empty { ... }`** — zero-items content (initial state included):
  nothing on its line, one child (G9007). Write at most one, first in the
  body: a second one, one after the row, and an `empty { }` outside a
  repeat or a List are build errors (G9007). A
  literal `empty { }` says "considered: show nothing". A plain repeat may
  leave it out; a `List` must have one (on the List's body, G9016).
  Replaces only the repeat's own expansion; statics stay.
- **Inside the body**: names are per-row local. An event from inside a
  row carries that row's node, and `Runtime::item_key(&page, ev.node)` is
  the row's key.
- **Not supported**: a repeat inside another repeat's template. It is
  a build error (G9018): code fills a repeat by its one name, and every
  outer row would hold one. A repeat inside a widget or
  control body works: every instance has its own, found as
  `<instance>/<repeat>` (`page.find("toolbar/marks")`). Selection,
  keyboard and activation belong to the `List` control (next section).

## List (selection over items)

`List` is the one control that owns selection (design: the repeat spec §5,
D118; self-scrolling + `ListItem` rows: D121). Its body is statics plus
**exactly one** direct child `repeat` (G9010) whose row root is a
**`ListItem`** (G9011), stacked top-to-bottom (`spacing=` like a Column)
inside the List's own scroll viewport. Zero new state names: the chosen
row's `ListItem` holds the universal **`selected`**, the keyboard cursor
is focus (the **`focused`** state), a press in flight is **`held`**.

```kdl partial
List picker select=many, spacing=4 {
    Label .SectionHeader text="Today"
    empty { Label .Muted text="Nothing due today" }
    repeat tasks by $item.id {
        ListItem .TaskLook { TaskRow title=$item.title }
    }
}
```

```rust
    // A tap on a row: `Change` on the List when the selection changed,
    // then `Activate` on the row. The row's key says which item it was.
    let first_row: NodeId = host.rt.items(&host.page, tasks)[0];
    for ev in host.tap(first_row, 1.0) {
        match ev.kind {
            EventKind::Change(_) => assert_eq!(handler_name(&host.page, &ev), Some("picker")),
            EventKind::Activate => assert_eq!(host.rt.item_key(&host.page, ev.node), Some(&ItemKey::Int(1))),
            _ => {}
        }
    }
    assert_eq!(host.rt.selected_keys(&host.page, picker), vec![ItemKey::Int(1)]);

    // Code writes the selection too: a list of keys, empty to clear.
    host.rt.set_selected(&host.page, picker, &[ItemKey::Int(2)]).expect("key 2 is a row");
    assert_eq!(host.rt.selected_keys(&host.page, picker), vec![ItemKey::Int(2)]);
```

- **It scrolls itself** (D121 — a list that doesn't scroll is a surprise,
  and virtualization needs the List to own its viewport). The internal
  scroll pans like a `Scroll` (touch pan, fling, spring edges) and follows
  focus (the cursor stays in view). Statics
  scroll with the rows. Never wrap a List in a `Scroll`. **`layout`**
  (D128) picks the items panel — a closed set, each owning its trio:
  `Column` (default, no block) scrolls vertically with an up/down cursor;
  `Row` is the strip (horizontal pan, left/right cursor, `width=hug` +
  `max_width=` transposed); `Flow`/`Tiles` wrap (vertical scroll, arrows
  move to the nearest row in their direction). The block holds exactly the one
  `repeat`; its props style the panel (`layout Flow spacing=8`). G9017
  fences misuse; `Grid` excluded (tracks vs dynamic rows underdefined).
  **`empty` lives on the LIST body** (D128, G9016) — required; shown
  beside the statics at zero items, hidden when data arrives; a List's
  repeat carries no inner empty (plain repeats keep G9007's form). **The box is the
  viewport**: an unplaced List EXPANDS to the space its parent offers (its
  own minimum is the viewport's, near zero — WPF ListBox); any authored
  placement (`height=200`, margins, `halign=`...) keeps full authority.
  **`height=hug`** (D123) flips the minimum to the CONTENT's — the
  menu-sized list for hug parents (a centered menu Column), where the
  viewport doctrine starves to zero height; a bare hug does not scroll
  (content past the screen overflows). **`max_height=`** (D124) bounds the
  hug: `height=hug, max_height=400` follows the items up to 400 epx, then
  the internal scroll takes over — the centered-menu arc (grows while
  there's room, scrolls when there isn't; WPF's MaxHeight). It requires
  `height=hug` (a plain viewport List already takes exactly the parent's
  offer — cap that with `height=`). **`bars=`** (D138) paints the internal
  scroll's bar — `never` (hidden, still pans — the handheld look),
  `always`, `reserve` (auto visibility, space always held); the same word
  `Scroll` takes. The AXIS stays the layout's (`allow=` is Scroll
  vocabulary — a List redirects to `layout`). **Fixed content counts into the
  minimum** (D127): statics and the `empty` block size the box like any
  authored content — a bare list shows its header and "none yet" text
  even in a hug parent; only ROWS are viewport-only. Starvation reports
  itself at BUILD time where the chain proves it (G9015 — walking up from
  the List to the first ancestor with a written `height=` or
  `min_height=`, the compiler warns at a `Scroll`, and at a Column or Row
  that is `valign=`-anchored or `height=hug`; Layers, Panel, Flow and Grid
  never warn themselves, conservative by design). A content-sized
  DEFAULT was rejected: it would un-scroll any list taller than its
  screen.
- **`ListItem` is the row root, always** (G9011; anywhere else: G9013).
  It is the unit selection (`selected`), the focus cursor and taps attach
  to, and a drawn box like `Panel` (`background=`/`padding=`/`corner_radius=`
  /`border_*=`, blank until told) — so the row's looks live ON it. One
  child fills it; multi-part rows lay out with a `Row`/`Column` inside.
  `when focused` styles the cursor.
- **Default looks** (D122, doctrine unchanged by the D181 respelling): a
  bare row shows selection, the cursor and the
  press OUT OF THE BOX — `selected` fills the row with white at 0.3 (spec
  "Default look"), `focused` with a quarter-strength echo, `held` with
  a half-strength fill while the press is held (the ladder orders them:
  held > selected > focused — an unselected row shades up, a selected one
  dims). The default is the base look, `when` overrides it —
  an authored `when selected`/`when focused`/`when held` (node or style)
  replaces that state's default entirely, PER STATE. `hovered` stays
  opt-in;
  `select=off` rows skip only the selected default (menus still flash on
  press). Two feel rules come with the defaults (user-caught): the press shade
  mirrors "release here would tap" — sliding off the row clears it, sliding
  back re-arms it — and the cursor echo draws only for key-driven focus (a
  tap parks focus silently). Since D129 that gating IS the `focused`
  state's
  one meaning — visible-focus — so authored `when focused` responses
  behave
  the same way; the row-local `kb_only` special case is gone.
- **`select=`** — `one` (default), `many`, or `off` (activation/focus
  without selection — menus; gogo's shipped opt-out word). Closed set
  (G0057). Code may write the mode
  (`rt.set_prop(list, PropKey::Select, …)`): the next tap follows it at
  once, and at the next `tick` the selection does too: `off` lets every
  row go, `one` keeps the first selected row in the List's order.
- **One handler on the List**: `List picker on:activate=pick` — a row
  that names no `on:activate=` of its own reports `pick`, and the event
  still carries the row (`item_key` gives its key).
- **Gestures** (user call, 2026-07-15): tap or Enter/Space on an item
  ALWAYS fires `activate` on the row; selection is the mode's side effect
  — `one` selects it (tapping the selected item never deselects — radio
  doctrine; clearing comes from code), `many` toggles it, `off` nothing.
  `change` fires on the List only when the selection actually changed
  (changed means CHANGED). A write from code fires no event:
  `set_selected` answers whether the selection changed. The
  cursor follows the tap; an arrow moves it to the nearest box that takes
  the keyboard in that direction: the next row, or a `Button` among the
  statics or outside the List (a `Label` static and `empty` text take no
  keyboard, and neither is ever selected). A `Button`
  inside a row takes its own presses — tapping it fires the button, not
  the row (WP semantics).
- **The selection** — `Runtime::selected_keys` reads it: no key or one
  under `one`, any number under `many`, present keys in collection order,
  then inert keys in the order they entered. **Inert keys** (no present
  item) render nothing, error nothing, and read back until reassigned or
  the item arrives — select-then-load needs no author code. Invalid writes
  (wrong shape, duplicate key, any write under `off`) are **refused whole**
  (G9104) and the last good selection stays — under `off` the write is
  refused but `activate` still fires per row.
- **Row styling** lives on the `ListItem`: `when selected` (selection),
  `when held` (press feedback — legal on rows like on buttons; the
  List's press tracking flips it, a pan-cancel or off-row release disarms),
  `when focused` (the cursor). A `when selected` on a node INSIDE the row
  never fires — the List drives the ROOT (linted, G9014; toggle buttons
  drive their own selected and stay quiet).
- **Reconcile keeps selection by key**: reorders move it, an item whose
  data changed keeps its row and its selection, and an empty collection
  leaves the selection inert. Hot reload carries it: a List's selection and a
  repeat's rows are on the reloaded page (`Runtime::carried_to`).

## States

**The UI is a function of its states.** While a state holds, its
`when` responses overlay the declarations; when it ends, the base look
restores. One concept at three scopes — *responses live at the state's
scope, and dotted names descend from that scope*:

| scope | state | who flips it | overlays |
|-------|-----------|--------------|----------|
| app | `theme dark` variant | `Runtime::set_theme_variant` | `$values` |
| app | `when screen.landscapeSmall` class | `Runtime::set_screen` (when the screen changes size) | props + placement — closed five-class set, composes with arrangements (equal rank: the later line wins) |
| view | `arrangement expanded` | `Runtime::go` / `go()` / `toggle()` | props + placement — **exclusive**, one active |
| control | built-in `held` `hovered` `focused` `focus_within` `selected` `disabled` | input | props — they **stack** |

Timing is a separate concern: [`transition` rules](#transitions). A press
highlight sits on top of whatever arrangement the screen is in.

```kdl
style GalleryItem background=$chrome {
    transition any -> selected duration=0.1, easing=cubic_out
    when held background=#FFFFFF22          // momentary, while the press is held
    when selected background=$accent        // persists while toggled on
    when disabled opacity=0.4
}
Button dangerRow .GalleryItem { when held background=#B00020 }  // node-level override
```

- Built-ins: `held`, `hovered`, `focused`, `focus_within`, `disabled`,
  `selected` — always available, no declaration (an unknown state word is
  build error G0036, which names them; the retired `pressed`/`hover`/
  `focus`/`checked` are answered with the new spelling, G0035).
  `hovered`/`focused`/`focus_within`/`disabled`/`selected` are
  **universal** — legal on any node, dormant until a provider flips them.
  `disabled=true` on a node that takes input blocks it — a control, or a
  box with `interactive=true` (the touch stops there and reports nothing);
  on a node that takes none it's a styling flag (greys via
  `when disabled`). `selected` is "this
  one is chosen": a toggle-mode button holds its own, a `choice`'s Radios
  carry it, a `List` flips it on the chosen item's root. `held` needs a
  press surface — the Button family, a `List` row's `ListItem` (D121), a
  `Slider` (the drag brackets it), or a `control` declaring
  `state_builtin held`. On any other node a `when held` builds and never
  fires (the Godot addon refused it; the Rust compiler only refuses it on
  a `control` that does not declare it, G0036). From code,
  `rt.set_state(node, DISABLED, true)`
  and `rt.set_state(node, SELECTED, true)` flip a state at once and
  `rt.has(node, SELECTED)` reads one. A write of the file's word does the
  same: `rt.set_prop(node, PropKey::Disabled, Value::VBool(true))` sets
  the disabled state, and `PropKey::SelectedProp` the selected one.
- **`disabled` cascades** (D133 — the WPF IsEnabled law): effective = own
  OR any ancestor's own, no opt-out; re-enabling the ancestor restores each
  descendant's OWN flag, and `when disabled` fires on the effective value,
  so gray-out styling flows with it. The only state that flows down
  (`focus_within` and `hovered` look up: every ancestor of the focused or
  hovered box holds them too). Code: `rt.set_state` on
  the group root; `rt.disabled(&page, node)` asks for the effective value.
- `hovered` answers a **real mouse only** — a finger isn't a pointer. The
  host feeds hover from a mouse (`Input::hover`) and never from a finger,
  so no look is stranded where a finger lifted (drag-scroll a list and no
  row stays lit). Hover styles are safe to author as
  pointer-device niceties — phones simply never show them. **Hover follows
  capture**: a mouse press pins the hovered look through a mid-drag slip
  off the control; the release re-checks the pointer and settles it.
- `focused` is **visible-focus** (D129, semantics unchanged by the D181
  respelling) — focused AND an indicator is
  warranted: focus arrived by keyboard, or a key was pressed while
  focused. A tap/click moves focus **without showing it**, and a later
  pointer press turns the indicator off. Text fields are the caret
  exception — a clicked `Input` always indicates. There is no raw-focus
  spelling; code asks `Input::focused()` which node holds focus. Buttons
  ship a default `when focused` response, a ring (key-driven only);
  authoring `when focused` replaces it — same replacement doctrine
  as the `ListItem` defaults.
- `focus_within` (D129) holds while the focus is on the node **or anywhere
  inside it** (CSS `:focus_within`, WPF `IsKeyboardFocusWithin`) — the
  container highlight for keyboard UIs: the settings row lights
  while its inner slider is focused, the active pane brightens its border.
  Visible-focus flavored like `focused` (and the one non-participle
  spelling: it names a relation, not a condition of the node).
- `when` props layer over the base look and restore when the state ends.
  Merge order: in the base look a node prop wins over the style's; the
  responses sit over the base look, and for one state and property a node
  `when` wins over the style's (between two styles, the one worn later on
  the node's line). A plain prop on the node never switches a
  style's response
  off; opt a node out by writing the same state on it (`when held opacity=1`
  over a style's `when held opacity=0.5`). Priority when several states hold:
  arrangements < focus_within < focused < hovered < selected < held <
  disabled.
- Friendly response props: `opacity`, `background`, `font_color`; a node's
  own look words (a `Rectangle`'s or `Circle`'s `color`, a `Gradient`'s
  `stop_1_color`) work too. Floats and colors tween; bools/enums apply
  instantly. A `background` or `font_color` response on a control that
  never declared one of its own starts from the control's default look and
  hands the channel back to it when the state ends.
- **Motion words are responses too**: `when held scale=0.95` holds the
  scale while the press is held and hands it back on release, timed by the same
  `transition` rules as every other response — the WP press-squish in one
  line. `scale=` is uniform, pops from the center, and scales the whole
  picture — glyphs, corner radii and borders shrink with the box, and the
  layout around it never moves; `scale.x=`/`scale.y=`
  take one axis, `translate.x=`/`translate.y=` are epx offsets, `rotate=` is
  degrees. One writer per property: a
  transition `anim=` in the same scope that drives a property a `when` also
  holds is a build error — pick the held value or the choreography, not
  both. (A motion word on the node's own line is its resting transform:
  `scale=0.5` draws the node at half size about its center and the layout
  keeps the full box; a `when` overrides it. Size a look that never
  changes with `width`/`height`.)
- **Child targets**: a dotted key whose head is a NODE NAME drives that node
  instead of the styled control — `when held flash.visibility=visible` shows
  a named child while held (the WPF template-part pattern). Names resolve
  widget-local first, so a widget used six times flips each instance's own
  `flash`. The friendly vocabulary applies to the target (`flash.opacity=1`);
  values are whatever the target's property takes — numbers (epx-aware),
  booleans, colors and words such as `visible`. A control property of the
  same name wins over a node name; an unknown name is a build error (G0060).
- A style may also respond to a **declared arrangement** — `when expanded
  visibility=visible` inside a style follows the *view's* flip on every wearer,
  with the view's transition timing (one flip, one feel).

### Arrangements

```kdl partial
arrangement expanded            // declares one configuration of the view
arrangement fullscreen          // (base is predeclared: the declarations)

transition duration=0.3, easing=cubic_out   // how every flip feels

when expanded {                 // the pose, in one place — dots descend
    bar.margin_bottom = 0       // from the view root
    bar.height = 90
    scrim.opacity = 0.5
}
```

An `arrangement` declares one configuration of the view's **exclusive set**
(drawer open/closed, toolbar expanded/collapsed) — its enum. `base` is
predeclared and needs no `when`: it IS the declarations. Flip from code with
`rt.go(Arrangement::Expanded)` (the page's generated enum; `rt.go_id` on a
page compiled while the program runs) or declaratively with
`on:activate=go(expanded)` — and a disclosure button (expand on press,
collapse on the next) is `on:activate=toggle(expanded)`: it goes home when
its arrangement is active, to the arrangement from anywhere else. Style the
button itself through the arrangement
(`when expanded { moreButton.rotate = 180 }`) rather than button toggle
MODE — `toggle=true` plus `toggle(expanded)` is two separate on/off states
that drift apart. (The Godot addon warned about the pair; the Rust
compiler builds it without a word.)

**"Pose"** in these docs is defined shorthand, not a keyword: the set of
values one `when` condition holds — an arrangement's, a screen class's, or
a composed pair's. Wherever a rule says "pose", it applies to all three
condition kinds alike; rules that differ between them name the kind.

For a pose that GROWS the layout — a panel sliding open to include more
content — give the appearing node `reveal_y=0` and let the pose reveal it
(`when expanded { hello.reveal_y = 1 }`): the space fraction is a float, so
the flip slides smoothly under the transition rule, siblings and the border
reflowing with it (see the `reveal_*` props row and the COOKBOOK
smooth-slide recipe).

- Responses are top-level `when <name>` blocks: dotted `node.prop=value`
  pairs. **Placement keys** (the margin and padding sides, `width`,
  `height`, `size`, the floors `min_width`, `min_height`, `min_size`, the
  caps `max_width`, `max_height`, and `halign`/`valign`) overlay the
  target's placement and compile per arrangement at build time;
  everything else goes through the normal state machinery.
- The flip tweens edge offsets and style props with the matching
  `transition` rule.
- **A standing pose**: the ACTIVE arrangement's pose holds across rotation
  and hot reload — no two-writers, no teleporting panels after a rotation.
  Hot reload keeps the active arrangement (it's user state, like slider
  values).
- **A posed size behaves exactly as if declared** (D249), for as long as
  the block applies — including the minimum the box reports, so a pose CAN
  shrink a node below its declared size. Composition matches declarations:
  a posed `width` still loses to a declared `min_width` floor (the floor
  clamps every posed value — WPF's MinWidth-over-Width order), a posed
  `min_width`/`min_height` replaces the declared floor, and content still
  sets a button's non-exact axis above any floor. On a timed flip the
  box glides to its new size, and a parent that hugs it follows the glide,
  both ways.
- Reserved names: `base`, `any`, the built-in state names.
  Arrangements rank below every built-in state when both touch a
  property.

### Screen classes and orientation (`when screen.*`)

```kdl partial
when screen.landscapeSmall {
    middlePanel.width = 280
    middlePanel.margin_left = 0
    hintLabel.opacity = 0.5
}
when screen.landscape {          // the ORIENTATION axis (D265)
    bodyStack.visibility = gone
    wideRow.visibility = visible
}
```

Two more independent pose axes, both named by the host app — never by
user actions (`Runtime::set_screen`, whenever the screen changes size).
The size CLASS says how much room there is; the ORIENTATION says which
way it runs (landscape = wider than tall). Both vocabularies are closed: classes
`screen.portraitSmall` / `screen.portraitMedium` / `screen.portraitLarge`
/ `screen.landscapeSmall` / `screen.lotsOfSpace`; orientations
`screen.portrait` / `screen.landscape` (an unknown name refuses at build,
listing both). No declaration line — they exist whether or not a file
poses for them; with nothing set (or none posed) the declarations stand.
Key the axis the design actually means: "is it sideways?" is the
orientation — enumerating the landscape-ish classes instead silently
catches big PORTRAIT tablets, because `lotsOfSpace` covers both shapes.

- **One vocabulary per target (G3005)** — a class and an orientation hold
  at the same time, so one style prop, one node's placement, or one seat
  move posed from BOTH would be decided by a rank the file never states:
  build error. Say the intersection as one condition instead —
  `when screen.landscape screen.lotsOfSpace { ... }` (orientation first,
  class second). Arrangements are exempt (a user pose, not a screen
  fact) and keep the ladder below.
- **Composes with arrangements** — the axes are independent (a dialog can
  hold its expanded pose on a rotated screen). Where BOTH re-pin the same
  node, each contested key goes to the block written LATER in the file
  (an arrangement `when` and a screen `when` rank equal — one condition
  each) and the other pose keeps the rest; leaving the arrangement lands
  back on the screen pose, not on base. Ranking for style props: base <
  single poses (arrangement / screen class / orientation, later line wins)
  < composed pairs (below) < built-in states.
- **Composed blocks** — three shapes, each a view-level block holding only
  while BOTH its conditions do, for poses neither axis owns alone:
  `when <arrangement> screen.<class> { ... }` and
  `when <arrangement> screen.<orientation> { ... }` (arrangement first),
  and `when screen.<orientation> screen.<class> { ... }`: `when
  largeColor screen.landscapeSmall { recentPanel.margin_left = 300 }`
  lays the panel out that way only when the dialog's largeColor
  arrangement is open ON that screen. One rule for every contested key:
  more conditions win (a pair beats either half), equal count → the later
  line in the file wins (two active pairs settle by file order). Either
  half dropping releases the block (back to whichever single-axis pose
  still holds).
- **A placement word means in a pose what it means on a line**:
  `fill(2)`, `hug`, `max_width = fill`, `min_size = 200x100`, a theme's
  `$number` and `safe` are read the same way in a view-level `when`, from
  code and on a node's line. The later word is the whole answer for its
  side: `bar.margin_bottom = 0`, over a line that says
  `margin_bottom=safe`, puts `bar` on the screen's edge with no inset.
- **A seat written from code is checked like a file's**:
  `rt.set_prop(node, PropKey::SeatOf, Value::VIdent(name))` moves `node`
  into the seat called `name` (`home` = back where its line put it). A
  name that is no declared seat, or a seat inside `node`, stops a debug
  build with a message; a release build leaves `node` at home.
- **Any pose may re-anchor and re-seat**: alignment keys and seat moves
  are legal in every view-level `when` — screen whens and arrangement
  whens (plain or composed) alike. A screen flip lands at once. Under an
  arrangement a `transition` rule times, the box glides from where it sat
  to where the new alignment or seat puts it. (The Godot addon refused
  the timed form at build, G3003/G3004; the Rust crate glides it.)
- **Seat moves** — the one response that REPARENTS: `seat sideSeat`
  declares an empty socket in the tree (lowercase like `slot`/`gap` —
  sockets, not painted controls), and `x.seat = sideSeat` in a `when`
  moves the declared node into it. The seat hugs to zero alone and
  to the occupant's minimum when occupied — a `width=fill` or exact-width
  occupant measures wrap-text ORDINARY on both axes (wrapped; fill asks
  the socket for size, a number IS the box, and a posed width counts as
  exact while its block applies), a hug occupant UNWRAPPED like every hug
  axis (D254). A hug occupant whose single-line wrap-text is wider than
  the window can never fit any pane (the width-feedback trap); the fix is
  `width=fill` on the occupant.
  Dropping every condition
  returns the node to its declared home, parent and position, and
  `x.seat = home` says the same thing AS a move — for the more specific
  block that wants the node back where a less specific one seated it
  (`home` is reserved; declaring `seat home` is a build error). Contested
  seats follow the rule above: more conditions win, equal count → the
  later line in the file. A seat and a resting home may sit under any
  parent, a container or a free one (`Layers`, the `View`). One occupant
  per seat is checked per condition: two occupants claiming one seat in
  the same condition refuse (G1016); claims under different conditions
  are accepted (two arrangements never hold together; an arrangement
  claim and a screen claim can, and then the rule above picks the one
  occupant — the other stays at its declared home);
  a bare class claim and a bare orientation claim on one occupant refuse
  outright (G3005). Full rules on the seat card.
- **Flips are instant** by design — a rotation is the OS snapping the
  window, and a tween here would show panels sliding to their landscape
  seats after the screen already turned. `transition` rules don't apply
  (a rule that names a class or an orientation as an endpoint is a build
  error, G0034).
- Placement keys are the arrangement set (above), `halign`/`valign`
  among them — a posed alignment key replaces the declared alignment
  word and nothing else: the declared margins and size stay unless the
  pose writes them too (`middlePanel.halign=left,
  middlePanel.width=280, middlePanel.margin_left=60` flips a
  stretch-with-margins panel into a left-docked one). An arrangement pose
  takes the alignment keys too — see the bullet above. Everything else
  goes through the state machinery. Container rule: a pose may RESIZE a
  Container-seated child (numeric `width`/`height`/`min_width`/
  `min_height` behave exactly as if declared while the condition applies,
  and a posed `width`/`height` makes a fill child stop taking the
  leftover; instant, nothing to tween) and re-align it across its parent
  (`halign` in a Column, `valign` in a Row), but never move it with a
  margin — a posed margin on a container child is a build error (G1014).
- The active class holds across hot reload.
- A style or a node block may answer the screen too: `when
  screen.landscape opacity=0.5` inside a style restyles every wearer while
  the screen is landscape.

### Transitions

How changes between standing truths feel — a directive line, never a block:

```kdl partial
transition duration=0.3, easing=cubic_out            // the default: any -> any
transition base -> expanded duration=0.25, easing=cubic_out
transition expanded -> base duration=0.2,  easing=cubic_in
transition any -> settings duration=0.25             // one-sided wildcard
```

- Payload: `duration=`, `easing=` (+ params), `anim=<name>` (choreography
  played with the flip; the state ending cancels it and puts the
  displaced properties back at once).
- **Specificity, most specific wins**: exact pair → entering one-sided
  (`any -> X`) → leaving one-sided (`X -> any`) → bare default. The same
  endpoints declared twice in one scope is a build error, not a silent
  pick. Going home (`X -> base`) with no rule of its own **mirrors the rule
  that brought you out** (timing only, never its anim). Untimed = snap.
- **At control scope** (inside a style/node block) the endpoints are built-in
  states, one side at a time (`transition any -> held
  duration=0.05` enters, `transition held -> any duration=0.2` leaves) —
  built-ins stack, so exact pairs between two of them have no single flip to
  time (G0034). `base` may stand for the rest side
  (`transition base -> held`), and a rule with no endpoints
  (`transition duration=0.15`) times every state change of that control.
- **A node's share of a pose flip** (2026-08-16): a node block may carry an
  exact POSE pair (`base` counts as a pose) — it times THAT node's tween in
  the flip, overriding the view-level rule; `duration=0` snaps the node to
  its new seat while the rest glides. Timing only — `anim=` and one-sided
  arrangement rules stay at the view's top level.

  ```kdl partial
  Rectangle plate 60x120 {
      transition option2 -> option1 duration=0.6, easing=cubic_out
      transition option1 -> option2 duration=0.4, easing=cubic_out
  }
  ```
- **A rule alone subscribes its endpoint state** (D182): a press pulse needs
  no standing `when held` response —

  ```kdl
  anim nudge { self.scale to=0.95, duration=0.1, autoreverse=true }
  Button saveButton text="Save" {
      transition any -> held anim=nudge
  }
  ```

  The enter plays the anim, the exit reverts what it displaced. Endpoints
  carry the same availability gate as `when` (`held` needs a press surface).
- Re-entering cancels the opposite tween and starts from current values.
- **A property's own timing** (2026-09-27): name `node.prop` targets
  instead of an arrow, at the top level —

  ```kdl partial
  transition commandBar.{margin_left, margin_top, opacity} duration=0.4, easing=expo_in_out
  transition underline.margin_left duration=0.4, easing=expo_in_out
  ```

  Any change of the property's value slides there from what shows: a code
  write (`Runtime::set_prop`), a `when` response, a pose. A change
  mid-slide turns around from where the slide is. `.{a, b}` names several
  properties of one node; `margin` / `padding` = the four edges; a comma
  lists more targets. Lands at once instead: the first value, a node that
  doesn't show, `Runtime::set_prop_instant` (a `set_prop` right after it
  slides from that value), and a `go_instant` flip's own `when` values.
  `visibility` never slides — fade `opacity` (G3007). One timing per
  property: a second rule for it, or a timed `A -> B` over a `when` that
  sets it, is a build error naming both lines (G3008).

## Animations

One model (D136): *an anim is a CHAIN of steps, and every step says when it
runs.* A step is a property line, a `group { }`, a `play <anim>`, an
`each ... { }` or a `delay=` pause; every step after the first carries a
connective — `and` (runs WITH the previous step) or `then` (runs after it
and everything `and`-joined to it) — or an `after=<group>` reference
instead. Sequencing is always written, never defaulted.

```kdl partial
anim disappear duration_each=0.25, easing=cubic_in { // header defaults:
    brushPanel.translate.x to=-60                    // duration_each= is EACH
    and brushPanel.opacity to=0, duration=0.4        // step's default; a
}                                                    // line's own wins

anim openMenu {                                  // groups share timing and
    group dim { backdrop.opacity to=0.6, duration=0.15 }   // carry the name
    then group drop duration_each=0.25 {         // after= references
        menu.translate.y from=-40, to=0, easing=back_out
        and menu.opacity from=0, to=1
    }
}

anim appear duration_each=0.2 {                  // each: who / when / what
    each .MenuButton stagger_by=0.1 {            // every wearer, file order
        translate.y from=40, to=0, easing=back_out
        and opacity from=0, to=1
        then scale.x to=1.05, duration=0.08, autoreverse=true
    }
}
```

- **Property lines** are `target.property[.component]` paths — the target
  is a named node (or `self` inside widget/style scope); components are
  `.x`/`.y` (on `translate` and `scale`) or `.a` (a color's alpha; a
  color has no `.r`/`.g`/`.b` lane). Step words on a line:
  `duration=`, `easing=`, `autoreverse=`, and the signed `offset=` shift.
  Inside an `each`, lines are bare (the each names the target; prefixing
  them there is an error).
- **Prop lanes**: any word a `transition` rule can move is
  an anim target — `b.background to=#FF3B30FF`, `card.corner_radius
  from=4, to=12`, `card.width to=200`, `g.stop_1_color.a to=0.42` (the
  live stop family included). `.a` on any color word moves alpha alone;
  the hue keeps
  following its ladder, so a `when` can restyle the color mid-fade. One
  ownership law with the motion channels: a playing anim owns its lane —
  `when`/code writes yield, a `transition` on an owned lane spawns no
  tween, a newer player takes over, `stop` restores, a finished anim
  holds its end pose. `key` children stay numeric — a color lane spells
  its segments as chained `then` steps. Colors mix under gogo's ONE
  mixing law: premultiplied Oklab — gradient stops, `transition` tweens
  and anim lanes all mix through the same code, so a fade to transparent
  never darkens and cross-hue midpoints never go muddy.
- **The connective law**: the first step stands alone; a second step with
  no connective is an error. `and` shares the previous step's start;
  `then` waits for every end in the previous `and`-cluster (autoreverse's
  way back included). A step carrying `after=<group>` takes NO connective —
  one placement per step, and the reference is it.
- **`group [name] duration_each=, easing=, autoreverse=, offset=, after=`**
  distributes step words to its own chain — a line resolves each word from
  the NEAREST enclosing scope that sets it (groups at any depth, then the
  anim header) — and carries the name `after=` references. Names are
  opt-in: only referenced groups need one.
- **The anim header** carries playback (`repeat=forever`, `speed=`) and the
  same distributing defaults, honestly named: `duration_each=` is per-step
  by NAME (a `then`-chain can't make it read as the anim's total), plus
  `easing=` and `autoreverse=`. Plain `duration=` on any header refuses,
  teaching `duration_each=`.
- **`after=<group>`** goes on a `group` line only (on a property line it
  is a build error, G0062) and starts that group when the named group's
  chain completes —
  the escape hatch for dependencies that CROSS the flow. Names are
  anim-flat: forward references are legal, wrappers are reachable-into;
  unknown names and cycles refuse at build. `offset=` shifts any placed
  start (negative overlaps the anchor's tail; the final start clamps at 0).
- **`delay=N`** is a PAUSE — its own step in the chain, always positive
  (starting a step early is `offset=`, signed). The old delay ATTRIBUTE is
  retired.
- **`play <anim>`** runs another anim as one step, inlined at bake — its
  `speed=` folds in, its internal names stay private, and a
  `repeat=forever` anim never completes so it can't be a step.
- **`each <node|.Style> [stagger_by=N]`** is a step whose chain every
  wearer copies (file order: a wearer before the wearers inside it),
  shifted by `stagger_by`. With `stagger_by` >
  0 its length only exists at play time — nothing may sequence after it
  (`and` beside it is fine: that shares its start).
- **The capture rule**: `to=` alone starts from the LIVE value when the
  step begins (WPF "To" animations — perfect for interruptible fades).
  Adding `from=` pins the start. On the first step that moves a
  property the pin **holds from play start**, so staggered entrances
  never flash their base; on a later step it is where that step starts
  (a blink is two steps with the same `from=`). Two from-pinned steps on
  one property that run at the same time = build error.
- **`from=current`** pins the live value read at PLAY start — the open
  start made spellable, and HELD through a delay window (an open start
  would flash the base there). `from=`-only. Pose-flip motion doesn't ride
  this: arrangement flips glide via `transition` pair rules (per-node
  timing included), which retarget from live values by construction.
- **Handoff on play (WPF Begin)**: a new play OWNS the properties it
  animates — in-flight players stop writing them (their clocks and other
  tracks run on) and hand over their restore base, so `stop` after a
  re-play chain restores the true pre-anim value. A FINISHED player's held
  end pose is a standing value; a new play just captures it.
- Key lines: `t=` (seconds), `v=`, `easing=` (into this key; default linear),
  `jump=true`. Same `t` twice = discontinuity. A keyframed line's duration
  **is** its last key's `t`.
- Numbers on length properties are epx; values may be colors.

### Motion — layout is declared, motion is transformed

The motion words move a **visual-only channel** — layout never budges, containers
can't stomp it, and hover/press state survives. The vocabulary:

| Word | Notes |
|------|-------|
| `translate.x` / `translate.y` | epx |
| `scale` (uniform) / `scale.x`/`.y` | about the box's center unless `pivot=` says otherwise — the WP "pop from the center" |
| `rotate` | degrees, about the same point |
| `opacity` | the one dual citizen — also a static style prop |

Two things write this channel: anim steps (transient flourishes) and `when`
responses (standing values held while a state holds). A `when` that holds
a property and a `transition … anim=` of the same scope whose anim drives
it are a build error (G4003); an anim that code plays over a standing
`when` value wins while it plays. Hits follow the motion:
a moved, scaled or rotated box takes touches where it is drawn. Because
motion works on any node — container children included — never
restructure a tree to animate it.

**A size, a margin or a padding moves the box itself.** An anim line on
`width`, `height`, `min_width`, `min_height`, a `margin_*` or a
`padding_*` (`card.width to=200`) changes the box, as `transition
card.width` does, so the box and what is around it are laid out again on
every frame of the play. Where only the picture has to move, a motion
word (`scale`, `translate.x/.y`) does it with no new layout (`reveal_x/_y`
changes the room the box takes, so it is laid out again as it moves):
transient pulse → `scale.x`/`scale.y`; changing and staying →
`arrangement` + `when` + `transition`. A side written `fill` or `hug`
starts from the size the box has. Engine words (`position`, `offset_*`,
`rotation`) are not gogo words and are refused (G0063). A finished anim
holds its end pose, so an anim meant as a pulse is written to end where
it began.

### Easing

WPF-compatible math. Names take
an optional `_in` / `_out` / `_in_out` suffix; a bare base name means
ease-out. Parameters are written as sibling props next to `easing=`:

| Base | Parameters (default) |
|------|----------------------|
| `linear`, `sine`, `quad`, `cubic`, `quart`, `quint`, `circle` | — |
| `expo` | `exponent` (2) |
| `power` | `power` (2) |
| `back` | `amplitude` (1) |
| `bounce` | `bounces` (3), `bounciness` (2) |
| `elastic` | `oscillations` (3), `springiness` (3) |

```kdl partial
line.scale.y to=1.5, duration=0.5, easing=bounce_out, bounces=1, bounciness=3
```

### Playing

On a page compiled while the program runs, an anim is found by its name
(`host.rt` is the page's `Runtime`):

```rust
    // An anim's id on a page compiled while the program runs.
    let appear: StrId = host.page.intern_lookup("appear");
    host.rt.play_id(&host.page, appear, Finish::Nothing); // restarts if already running
    assert!(host.rt.is_playing_id(appear));
    host.frame(0.5); // the anim lasts 0.3 s
    assert!(!host.rt.is_playing_id(appear));
    assert!(host.rt.take_finished().contains(&appear)); // the finished anims, once each

    host.rt.play_id(&host.page, appear, Finish::Nothing);
    host.rt.stop_id(appear, true); // true: every property goes back to what stands without the anim
    assert!(!host.rt.is_playing_id(appear));
```

A build generates an `enum Anim` per page, and the calls are then
`rt.play(&page, Anim::Appear, Finish::Nothing)`, `rt.stop(Anim::Appear,
true)` and `rt.is_playing(Anim::Appear)`: a misspelled anim does not
compile.

**What `stop` puts back** — every property the anim wrote returns to what
the file says without the anim: the declared value; the `when` value
holding at the time; or a FINISHED anim's end pose the play started from
(a standing value the author chose — it stays). A stopped anim is not in
the list `take_finished` hands out. Example:

```kdl
anim pulse repeat=forever {
    dot.scale from=1, to=1.2, duration=0.4, autoreverse=true
}
anim vanish {
    dot.opacity to=0, duration=0.2
}
style DotLook {
    when held dot.scale=0.9
}
Button dotButton .DotLook width=40, height=40 {
    Circle dot width=24, height=24, color=accent, halign=center, valign=center
}
```

- Play `pulse`, then stop it: `dot.scale` is 1, the declared value.
- The same with `dotButton` held: `dot.scale` is 0.9, because the `when
  held` value is what the file says now.
- Play `vanish` to its end: `dot.opacity` is 0, and stays 0. Play it again
  and stop it: still 0 — the replay started from the held end pose, not
  from 1.

`stop` reads the file's answer at stop time, so a `when` that flipped
while the anim ran counts too.

## Translations

Any string a control DISPLAYS can carry a translation term:

```kdl
Label tip text="new stroke"/newStroke, font_size=15
```

- **The literal is the English source text** — the design-time value, the
  fallback for any locale that lacks the key, and the string translators
  translate. Your `.gogo` files ARE the English table; no separate English
  strings file exists to drift out of date.
- **The key indexes the language table the host program hands the page**:
  `Locale::build(KEYS, pairs)` makes one language from (key, text) pairs,
  and `Runtime::set_locale(Some(locale))` shows it — every keyed string on
  the page is measured and drawn in that language at the next frame.
  `set_locale(None)` returns to the literals. A key the language lacks
  shows its English.

  ```rust
      // One language: (key, translated text). A key the list leaves out
      // shows its English.
      let title_key: KeyId = keys.get("appTitle").expect("the page names /appTitle");
      let german: Locale = Locale::build(MENU_KEYS, &[(title_key, "Malen")]);
      host.rt.set_locale(Some(Arc::new(german)));
  ```

  (`MENU_KEYS` is the list of every key with its English, in id order; a
  build generates it as `KEYS`, with an `enum Key` whose `from_name` turns
  a translation file's key into its id.)
- **One key, one English string** (error `G8001`): the same key may repeat
  with the SAME literal (shared strings — that's how views share a term),
  but a different literal under an existing key fails the build, pointing at
  both places. Editing a literal in its own file is always legal.
- Keys work wherever the string lands: node props, style props, widget
  params (`Chip label="keep changes"/keepChanges`). They are rejected where
  nothing is displayed — theme `$values`
  (key the literal where it shows), resource paths, `uses` paths.
- **Code-side strings** (toasts, formatted messages):
  `rt.translate("PaintingSaved", "Your painting has been saved")` — same
  key + English-fallback rule as `/key`. Format params stay in your code.
- **A node's text from code**: `Runtime::set_key(node, Key::PaintingSaved)`
  shows another key's text (translated), and `Runtime::set_text` writes a
  literal that is never translated — two writes, two meanings.
- **Text that stays English on purpose**: `"test only"/_` — the
  `_` is no key, so the text is never translated and never counts as a
  key (two `/_` strings may differ; G8001 does not apply). For a whole
  file, write `allow untranslated` as a line at the top (dev pages, a
  release-notes page): the check below skips every string written in that
  file. Each file answers for its own text — a widget's body follows the
  widget's file, an instance line's `caption="…"` the page's.
- **The missing-key check**: a build that ships in several
  languages sets `CompileOptions::require_keys`, and every displayed string
  with no key is an error (`G8002`) naming the file and line. Not checked:
  paths and picture addresses (`res://`, `collage://before`), `areas=`
  rows, and text with no letter in it (`"100%"`, `"1/1"`). `allow` takes
  exactly `untranslated`, at the top of a file (`G8003`).

```kdl
allow untranslated              // this whole file stays English

View devPage {
    Label text="probe"                      // fine: the file allows it
}
```

```kdl
View settings {
    Label text="Privacy"/settingsPrivacy    // translated
    Label text="Leon Shaw"/_                // a name: English on purpose
    Label text="100%"                       // no letters: never checked
}
```

## Events and triggers

`on:<event>=` answers a moment. The events are a closed set of bare stems
(D181); any other word after `on:` is build error G0049.

| event | fires | value |
|---|---|---|
| `activate` | the control's OWN trigger gesture completed — Button: a completed tap or Enter/Space; List: tap or Enter/Space on a row. Never fires disabled | — (List: the event's node is the row; `Runtime::item_key` gives its key) |
| `press` / `release` | a finger landed on a press surface or a Slider, and that gesture ended — `release` fires exactly once per `press`, however the gesture ends (lift inside or out, cancel, a Scroll taking it, the node disabling under the finger) | — |
| `change` | the committed value is now different — once per commit, genuine transitions only | Check/Toggle a bool, Slider a number, Input the text, `choice` the picked Radio's name; for a List read the keys with `Runtime::selected_keys` |
| `edit` | a live, UNCOMMITTED value moved — Slider per drag tick, Input per keystroke that changes the text; discrete controls have no edit | the live value |
| `show` / `hide` | the node was shown or hidden — takes `play(anim)` | — |
| `pointer` | every touch phase on a box the host draws itself (the node needs `interactive=true`, G0073) | the phase and the position inside the box, epx |

Per control: **Slider** — `edit` streams while dragging (`held`),
`change` commits on release; a key step commits directly (`change`
only, no `edit`). **Input** — `edit` per keystroke that changes the text; `change` on Enter or
focus loss. **Toggle** — `change` on settle, only if the settled side
differs from the pre-gesture side. **choice** — `change` on the GROUP
(the value is the picked Radio's name; items refuse it). **List** —
`change` on the List when the selection changed, then `activate` on the
row.

Events come from input. A write from code (`Runtime::set_value`,
`set_prop`, `set_selected`) reports nothing: the code that wrote already
knows.

What `on:` takes:

- **A handler name**: `on:activate=HandleOk`. The host program reads it
  off the event; a build also generates an `enum Handler` per page with
  one variant per name (`Handler::HandleOk`), so code naming a handler
  the file no longer has does not compile.
- **Nothing**: with no `on:` line for an event, the handler it names is
  the node's own name (`saveButton`). One name serves all of a node's
  events; the event's kind says which one happened. `press`, `release`
  and `pointer` are the exception: gogo reports them only on a node
  whose line has that `on:`.
- **`emit(Name)`**: the event is reported under `Name`. Use it to give
  several controls one name (`emit(BackPressed)` on a button and on a
  key).
- **Anim triggers**: `on:<event>=play(animName)`.
- **Arrangement triggers**: `on:<event>=go(arrangementName)` flips the
  view's arrangement declaratively — `go(base)` returns home. No host
  handler needed. `on:<event>=toggle(arrangementName)` flips BETWEEN the
  pose and base (active == it ? base : it — the disclosure button; from a
  different pose it goes to the named one). `toggle(base)` is a build
  error (G0050). Keep such a button momentary: a button's `toggle=true`
  and the pose are two separate on/off states — style it with
  `when <pose> { ... }`.
- **Visibility pair**: `on:show=play(appear)`,
  `on:hide=play(disappear)`. The two hooks take `play(…)` only: any
  other action or a handler name is a build error (G0084), since nothing
  would carry it out. The hide plays first and hides on finish
  (deferred hide), then restores the base look; showing during a hide cancels
  it and plays the show anim from current values. A control that is
  already visible when the page first shows plays its show anim once as an
  **entrance** — to a user, "appeared" and "was shown" are the same
  event; the host asks for it once per page (`Runtime::play_entrances`).
  An initial `visibility=gone` stays state, not a trigger (the show plays
  on the flip). From code, `Runtime::show(&mut page, node, visible)` goes
  through the pair at once and `Runtime::set_visible` on the next frame;
  either way a hide waits for the node's `on:hide` anim, and a node with
  none hides at once.

How the host program reads the handler name off an event is in the
cookbook: "How do I hear a button in my code?".

## epx units

Every number on a length property is an **effective pixel**: sizes,
spacing, insets, paddings, margins, `translate`, corner radius, border
thickness, `font_size`, and anim/when values on those.
gogo lays out in epx only and never sees a device pixel: the host program
passes the screen's size in epx as the frame's viewport, and its renderer
multiplies by the display's scale when it paints.

Text is the one place where the device pixel shows. The text engine
(`gogo-text`) draws a font at a whole number of device pixels (the size in
epx times the scale, cut down), and it sets the lines of a wrapped label
one font height plus 1 epx apart, that gap rounded to a whole device pixel
and never under one. Both come from `TextPx::from_epx`, which the layout's
measure (`EpxMeasure`) and the renderer share.

**One writer per property.** Base declarations own layout, arrangements
own the alternative standing poses, and anims own the motion channel
(identity at rest) and the prop lanes they play. A code write
(`Runtime::set_prop`) wins over the file
until `Runtime::clear_prop` takes it back, so don't have code keep
adjusting a property the file also poses (the classic: adding a safe-area
inset onto a declared margin — that's the `safe` value's job); either
declare the value in gogo, make it an `arrangement`, or leave the
property entirely to code.

## Host API (the `gogo-ui` crate)

gogo does the middle of a UI. The program around it — the **host** —
compiles the pages, feeds them touches and time, and paints the draw list.
The Rust examples here are cut from `docs/examples/host_tour.rs`, a
program the tests compile and run.

**Compile.** A shipping app compiles its pages at build time:
`gogo_ui::build_pages(dir, out, BuildOptions::new(res_root))`, called from
`build.rs`, turns every `.gogo` page in a folder into one Rust file. Per
page it holds a module with `PAGE`, `NAMES` (every named node), the enum
`Handler`, and the enums `Arrangement`, `Anim` and `Theme` where the page
declares arrangements, anims or theme values; for the whole build,
`KEYS` with `enum Key`, the embedded `IMAGES` and `FONTS`, and the
atlases. A page with an error fails the build. While a program runs
(tests, tools, hot reload) `compile` does the same from text:

```rust
pub fn compile_page(source: &str) -> Option<(Page, Keys)> {
    // The second argument reads the files that `uses "..."` lines name.
    // This page names none.
    let compiled: Compiled = compile(source, &|_| None);
    for d in &compiled.diags {
        eprintln!("{d}"); // error G2001 line 3: unknown property ...
    }
    if has_errors(&compiled.diags) {
        return None;
    }
    Some((compiled.page, compiled.keys))
}
```

**Keep a page on screen.** These travel together:

```rust
pub struct Hosted {
    pub page: Page,
    /// Everything that changes while the page lives: states, scroll
    /// positions, running animations, what code wrote.
    pub rt: Runtime,
    /// The touch and key machine: it turns raw touches into events.
    pub input: Input,
    /// Where every node sat in the last frame, and how it looked.
    pub kept: KeptFrame,
    /// The last frame's draw commands, in paint order.
    pub draws: Vec<Draw>,
    /// The screen in epx.
    pub screen: Rect,
}
```

**A frame.** Time moves, then the page is laid out and its draw list
filled; only what changed is redone:

```rust
    pub fn frame(&mut self, dt: f32) -> bool {
        let mut moving: bool = self.rt.tick(&mut self.page, dt);
        if self.kept.is_laid_out() {
            moving |= self.input.tick(&self.page, &mut self.rt, self.kept.frame(), dt);
        }
        // Lays the page out and fills `draws`. `FixedAdvance` measures
        // text as if every letter had one width; a real host passes its
        // own `TextMeasure`.
        self.kept.frame_retained(&mut self.rt, &self.page, self.screen, &FixedAdvance, &mut self.draws, None);
        moving | self.rt.is_moving()
    }
```

It returns whether anything still moves. A host that stops its frame loop
when nothing can change (to save battery) asks this before it parks, or a
glide freezes mid-flight until the next input.

**A touch** goes in, events come out:

```rust
    pub fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        self.input.touch(&self.page, &mut self.rt, self.kept.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 })
    }
```

**The calls**, by what they are for (`rt` is the page's `Runtime`):

| Call | Purpose |
|------|---------|
| `rt.play` / `stop` / `is_playing` / `take_finished` (and the `_id` forms) | Animations (see Animations, Playing). |
| `rt.go` / `toggle` / `go_instant` / `arrangement_is` / `arrangement_settled` | Arrangements — flip the standing pose (`Arrangement::Base` = the declarations). |
| `rt.set_screen(&page, orientation, class)` | Names the screen's orientation and size class; every `when screen.*` re-poses at once. |
| `rt.safe = [l, t, r, b]` (then `rt.dirty = true`) | Safe-area insets in epx; every `safe` padding and margin follows. |
| `rt.set_theme_variant(&page, name)`, `rt.theme_color`, `rt.theme_number` | Switch the theme variant; read a theme value from code. |
| `rt.set_locale`, `rt.translate`, `rt.set_key`, `rt.set_text`, `rt.text_of` | Languages and texts (see Translations). |
| `rt.set_prop` / `set_prop_instant` / `clear_prop` | Write any property from code: sliding by its `transition`, at once, and taking the write back. |
| `rt.set_visible`, `rt.show` | Show or hide a node. Both play its `on:show`/`on:hide` anims: `show` at once, `set_visible` on the next frame. |
| `rt.set_state(node, DISABLED, on)`, `rt.has(node, SELECTED)` | Flip or read a state bit. |
| `rt.set_value(&mut page, root, v)`, `rt.set_custom` / `custom` | A control's value; a control's own declared states. |
| `rt.set_items` / `items` / `item_key`, `rt.set_selected` / `selected_keys` | A repeat's rows and a List's selection (see Repeat, List). |
| `rt.set_scroll` / `scroll_of` / `scroll_into_view` | A Scroll's position. |
| `rt.play_entrances(&page)` | Plays each visible node's `on:show` once, when the page first shows. |
| `input.touch` / `key` / `wheel` / `hover` | Feed input; `touch`, `key` and `wheel` answer with the events they caused, `hover` only sets `hovered`. |
| `input.register(behavior)`, `input.focused()`, `input.focus_on` | A host's own control behavior; who has keyboard focus; move it. |
| `gogo_ui::hit(&page, frame, x, y, any)` | The node under a point of the last frame. |
| `reload::Reloader` (`open`, `refresh`), `reload::Sources` | Hot reload (below). |

Options the compiler takes from the host (`CompileOptions`):
`code_controls` (node types the host draws itself), `behaviors` (its own
control behaviors), `require_keys` (every displayed string needs a
translation key).

**Hot reload.** With the cargo feature `hot-reload` on, a page follows
its `.gogo` file while the program runs: save the file, and the page on
screen changes within a quarter of a second, its state kept. A host
makes one `Reloader` and calls it in three places:

```rust partial
// Cargo.toml, for development builds only:
//     gogo-ui = { version = "0.5", features = ["hot-reload"] }
use gogo_ui::reload::{Reloader, Sources};

// Once, at start. `ui::BUILD` is generated by `build_pages`.
let mut reload = Reloader::new(&ui::BUILD, Sources::dir("ui"));
reload.on_change(move || wake_the_event_loop());

// When a page opens: the page as its file says now.
let mut page: Page = reload.open(&ui::menu::PAGE);
let mut rt = Runtime::new(&page);

// Before the page's frame.
match reload.refresh(&mut page, &mut rt) {
    Ok(true) => { /* the page shows its file again: lay it out anew */ }
    Ok(false) => {}
    Err(why) => eprintln!("{why}"), // menu.gogo:12 G2001 ...
}
```

(Nothing compiles this example: it needs the `ui` module a build script
generates.)

- **Off by default.** Without the feature every call above is empty:
  `open` hands the page as built, `refresh` returns at once, and the
  program never reaches the compiler, so the linker leaves it out. The
  host writes the same code for a development build and for the build it
  ships.
- **The promise.** While reload is on, a page shows its file: a page
  that is open, and a page opened after the edit. A save that cannot be
  shown changes nothing on screen; `refresh` returns why, with the file
  and the line, and does not repeat it until another file is saved.
- **Pages made elsewhere.** A host that makes its pages with
  `Page::from_static`, in many places, may leave them so: `refresh`
  takes such a page over the first time it sees it, and brings it in
  step with its file then.
- **`refresh` is cheap.** With nothing saved it compares one number, so
  a host may call it for every page on every frame. A saved file is
  compiled once, and only the pages that read it (the file itself, or a
  file it `uses`) change.
- **`Sources::dir(folder)`** reads the files from a folder and watches
  it on a thread of its own: on a PC the project's `ui` folder, on a
  phone a folder a script copies the files into. Empty that copy when
  you install a new build: a page shows its file, so an old copy would
  show in place of the new build's pages. `on_change` is called
  from that thread when a file was saved, for a host that sleeps while
  nothing moves. **`Sources::read_with(f)`** reads through a function
  (a socket, an editor); the host calls `reload.mark_changed()` when
  text arrived.
- **What a reload keeps:** every scroll position, state, the
  arrangement, the theme variant, what code wrote, and the rows of every
  repeat with a List's selection. Each row keeps its node, so a row
  node your code remembered is still that row. A keyed row is filled
  again from the item it held, through the file's new row, so an edit
  of a row's bindings shows on the rows already on screen. Motion in
  flight ends, and a focused Input loses its caret.
- **What a reload refuses** (nothing changes, and `refresh` says why):
  a file that does not compile; a changed list of nodes, in a row or
  outside one (`NAMES` in your code are numbers into that list); a
  translation key, a picture or an `atlas` line the program was not
  built with (G8004, G2021, G2022), and another English text for a key
  it was built with (G8001); a row binding the items on screen cannot
  fill (the message `set_items` gives); another `by` on a repeat.
  Rebuild once and the page reloads again.

A program with no build step, which compiles its page when it starts and
finds its nodes by name (`page.find`), has no numbers fixed in its code.
It makes the reloader with `Reloader::files` and hands its page over
with `follow`; a reload may then change anything, and a page whose node
list changed starts fresh. A program that compiles its pages with code
controls or behaviors of its own makes the reloader with
`Reloader::files_with(sources, options)`, the options it compiles with,
so a reload knows them too. The key table, the picture table and the
asset reader of those options are lent for one compile and are not
kept, so a page with an `atlas` line is refused at its first edit:

```rust
    // Where the program reads its page files while it runs. A real host
    // names a folder (`Sources::dir("ui")`), which is watched for saves.
    let file: Arc<Mutex<String>> = Arc::new(Mutex::new(MENU.to_string()));
    let read: Arc<Mutex<String>> = file.clone();
    let sources = Sources::read_with(move |name: &str| (name == "menu.gogo").then(|| read.lock().unwrap().clone()));
    // This host compiled its page itself: the reloader takes it over.
    let mut reload = Reloader::files(sources);
    reload.follow(&mut host.page, "menu.gogo");

    // The file is saved with another spacing.
    *file.lock().unwrap() = MENU.replace("spacing=12", "spacing=20");
    reload.mark_changed(); // a watched folder marks its own saves

    // Before the page's next frame:
    match reload.refresh(&mut host.page, &mut host.rt) {
        // The page shows its file again: lay it out anew.
        Ok(true) => {
            host.frame(0.0);
        }
        // Nothing was saved, or the `hot-reload` feature is off.
        Ok(false) => {}
        // The save cannot be shown: the page on screen stays.
        Err(why) => eprintln!("{why}"),
    }
```

A host with a loop of its own can call the pieces the `Reloader` is made
of, which need no feature: `reload::recompile` (a file compiled against
a build's tables), `reload::same_shape`, `reload::swap` and
`rt.carried_to`.

**Painting.** The draw list is plain data: quads, text runs, pictures,
gradients, clips, in paint order (`gogo_ui::Draw`). `gogo-ui` draws
nothing itself and measures no text: the host passes a `TextMeasure` and
paints the list with its own renderer. A `TextMeasure` answers two
questions about a text: its size at a wrap width (`measure`), and the
narrowest it folds to, which is the width of its longest word
(`narrowest_wrap`; a wrapping `fill` label never gets less where its room
is no written number). The second
has a default that measures every word alone, so a measure of your own
needs only `measure`; `gogo-text`'s measure answers both from its line
breaker.

## Errors (reading a diagnostic)

```text
ui/menu.gogo:3: error G2001: `anim=` is not a gogo word — timing belongs on a `transition` line (`transition any -> held duration=0.1`); a `when` holds standing values
```

- The build prints one line per problem: the file, the line, `error` or
  `warning`, the code, and the message, which usually names the fix. A
  page with an error fails the build; warnings do not.
- In code a diagnostic is a `gogo_ui::Diagnostic` (`severity`, `code`,
  `line`, `message`); printed, it reads `error G2001 line 3: ...`.
- The **code** is a stable search key: `docs/gogo-errors.md` holds one
  entry per code, saying what the message means and how to fix it. A code
  is never renumbered and never reused.
- For known sets (types, properties, styles, states, `$values`) the
  message carries a did-you-mean suggestion or lists the valid words; an
  unknown easing or anim target is only named.
- With hot reload, a file with an error is refused and the page on screen
  stays (`Reloader::refresh` hands back the errors).

## Reserved words

No widget and no control can be named one of gogo's own words (G5001).
The parser and the compiler share one list of them:

- lines that are not nodes: `when`, `transition`, `anim`, `repeat`,
  `uses`, `widget`, `control`, `style`, `theme`, `atlas`, `slice`,
  `arrangement`, `allow`, `layout`, `choice`, `empty`, `key`;
- lines of a widget or control body: `slot`, `seat`, `gap`, `row`,
  `skip`, `script`, `behavior`, `keys`, `signal`, `state`,
  `state_builtin`;
- clause and step words: `as`, `by`, `in`, `and`, `then`, `play`,
  `each`, `group`, `delay`;
- states, and the words that stand for one: `held`, `hovered`,
  `focused`, `focus_within`, `disabled`, `selected`, `base`, `any`,
  `screen`;
- values and flags: `true`, `false`, `fill`, `hug`, `signals`,
  `interactive`;
- actions: `go`, `toggle`, `emit`.

The retired state words `pressed`, `hover`, `focus`, `checked` are
refused after `when`, each naming its replacement (G0035; respelled in
D181). Node names stay free — except `base` and `any`, reserved
everywhere (they name state endpoints; a child called `base` next to
`transition base -> held` would read as two things — decided
2026-08-31), `true`/`false` (value words), and
`interactive`, which errors as a name (a bare `interactive` on a node line
is always the typo for `interactive=true`).

## Not in v1

String interpolation, expressions (the call forms gogo reads are `play()`,
`go()`, `toggle()`, `emit()` on `on:` lines, sizing `fill(n)` and `hug(n)`,
`round()` around a number, and a slot's `automatic=arc(...)` /
`angle(...)`), required parameters, block comments, general
arrays (the list kind exists but only Grid's `cols=`/`areas=`, Gradient's
`stops=`, `border_dash=`, an Arc's `dash=` and an Image's `window=` accept
one), multiline strings, a `%` unit for sizes (a `%` is read in two places
only: an anim's `translate.x`/`translate.y` ends, as a share of the node's
own width or height, and a behavior's `key_unit=`), `Slider orientation=`,
multiline `Input`, true ellipses (Circle draws from the short side).
Repeat's own fences: nested `repeat`s (addressability — the future form
sources its collection from the outer item), a declarative `items=` binding
(collections are code-set until a data layer exists), dictionary
iteration. `List`'s own: virtualization (it owns its viewport since D121 —
rows still render eagerly), declarative initial selection (keys come from
data code assigns anyway), a by-key `ensure_visible(key)` (code scrolls a
row into view by its node: `Runtime::scroll_into_view`).

### No WPF-style ControlTemplates (by design)

Gogo deliberately has no ControlTemplate feature, and there are **no plans
until further discussed, as and if needed**. WPF templates work because its
controls are lookless — behavior classes with zero intrinsic visuals; gogo's
built-in controls draw themselves, and a `control` declares its own body,
so there is no hidden tree to replace. The useful parts of the
template idea already exist under other names:

| WPF | Gogo |
|-----|------|
| ControlTemplate visual tree | `widget` body |
| ContentPresenter | `slot` |
| VisualStateManager driving named parts | `when` responses + child-target props (`when held flash.visibility=visible`) |
| `PART_*` named-parts contract | per-instance widget names + `signals` surface |
| TemplateBinding | `$params` (build-time; mutate the named node from code for live changes) |
| Control chrome (Background/Border) | box props (`background`, `corner_radius`, ...) |
| Implicit styles (restyle a whole type) | theme type entries (`theme { Button .AppButton }`) |
| DataTemplate / ItemsControl | `repeat` (see Repeat) — `set_items` hands the collection whole |
| ListBox / ListBoxItem | `List` / `ListItem` (explicit — gogo never generates the item container) |

Known semantic traps when porting a template by hand: input surfaces must be
declared (see Input above) and WPF's top-left image alignment (see Image).
