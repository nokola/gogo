# Gogo Language Reference

Lookup reference for the Gogo UI language (v1).
Docs map: **[examples/tutorial.gogo](examples/tutorial.gogo)** — the language
in one commented, hot-reloadable file · **[COOKBOOK.md](COOKBOOK.md)** —
task-oriented recipes · **REFERENCE.md** (this file) — everything, organized
for lookup · **[gogo-spec.md](gogo-spec.md)** — formal grammar and design
rules.

Main goals (every language decision is measured against these): **easy to
understand, easy to write, easy to find — ergonomic, readable 6 months
later**; **Rust-inspired**: every construct keyword-announced, one keyword
per concept, hidden semantics made visible, shorthand legal only as defined
sugar.

## Vocabulary

The whole surface, one name per concept (D83 — framework-neutral; the old
WPF spellings error with a redirect):

| gogo | Maps to | Use for |
|------|---------|---------|
| `View` | PanelContainer (root contract) | the file's root; the host binds it. Defaults: transparent bg + fill both axes (file props win) |
| `Row` / `Column` | HBox/VBoxContainer | stacking on x / y |
| `Grid` | GogoGrid | shared tracks and spans across rows (see Grid) |
| `Tiles` | GogoTiles | unit-lattice mosaic — cells flow and wrap (see Tiles) |
| `Flow` | FlowContainer | wrap-to-next-line (+`orientation=`); hugging, it is as wide as its widest wrapped row |
| `Layers` | GogoLayers | z-stack; max-child minimum; input-transparent |
| `Panel` | PanelContainer | styled box + padding (blank canvas until told) |
| `Scroll` | GogoScroll | touch-pannable scrolling (see Scroll) |
| `gap` | Control (anonymous) | space in a stack: `gap 8`, `gap fill` — structure word, never named |
| `Label` | Label | display text |
| `Input` | LineEdit | single-line text entry |
| `Image` | TextureRect | bitmaps; Uniform fit, centered box (see Image) |
| `Rectangle` | ColorRect | one cheap solid fill — guaranteed |
| `Circle` | GogoCircle | discs and rings; radius self-derives from the box |
| `Arc` | GogoArc | ring segments: `thickness=` strokes, `from=`/`sweep=` degrees |
| `Gradient` | GogoGradient | generated gradient fills (see Gradient) |
| `Button` | Button | press / toggle |
| `Radio` | CheckBox + group (bodied: blank toggle Button) | one-of-N inside `choice { }`; a body makes it TEMPLATED (see choice) |
| `Check` | CheckBox (bodied: blank toggle Button) | independent on/off; a body makes it TEMPLATED (see choice) |
| `Slider` | HSlider (or the Gogo composite behind `render=gogo` — see Controls) | value drag |
| `Toggle` | the Gogo composite always (no native class — see Controls) | the on/off switch |
| `List` | the Gogo composite always (no native class — see List) | selection over a `repeat`'s items — self-scrolling |
| `ListItem` | PanelContainer (script-backed: list_item.gd) | a List row's root — the selectable/focusable/tappable unit (only there, G9013) |
| `choice` | — (erased; wires a ButtonGroup) | the one-of-N scope around Radios — structure word |
| `repeat` | — (erased; a hidden marker anchors the items) | N children from data, reconciled by key (see Repeat) — structure word |
| `empty` | — (structure inside `repeat`) | a repeat's zero-items content, first in its body |

Structure words (`row`, `skip`, `gap`, `choice`, `repeat`, `empty`) are
lowercase — **capitalized = an addressable node (nameable, stylable,
animatable); lowercase = structure**. Structure is never named, with two
deliberate exceptions that need a code-facing address: `choice` (auto-wires
`On<Name>Change`) and `repeat` (the `set_items` target). Raw Godot
`Control` classes stay available as the escape register (`Control`,
`TextureRect`, `ScrollContainer`, ...) — except exact duplicates of a gogo
name (`VBoxContainer`, `ColorRect`, `LineEdit`, ...), which redirect to it,
and Godot's raw `Panel` class, which the gogo `Panel` alias shadows.
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
  began) is an `anim` on the motion channel: `translate` / `scale` /
  `rotate` / `opacity`.
- A property your **code keeps moving** (a bar that follows a selection, an
  underline under the picked tab) slides with a property transition:
  `transition commandBar.{margin_left, margin_top, opacity} duration=0.4` —
  every write then slides there; `set_prop_instant` lands one at once.

See [States](#states) and [Animations](#animations).

## Syntax basics

One line shape covers the whole language:

```kdl
WidgetType name .StyleName property=value, property=value { children }
```

- `//` comments to end of line. Indentation is never significant; braces are.
- Commas are **required** between properties. A **trailing comma continues the
  line** (the only continuation mechanism).
- `;` separates multiple nodes on one line. `{` must be on the node's line.
- Duplicate property on one node = error. Duplicate node name per view = error.
- Property keys are snake_case (`min_height`, `icon_size`, `tap_pulse`) — the
  same convention as raw Godot engine properties (`autowrap_mode=word_smart`),
  so engine names copy-paste unchanged. `-` is not an identifier character.

## Values

| Kind   | Syntax | Notes |
|--------|--------|-------|
| string | `"Paint"`, `"new stroke"/newStroke` | Escapes: `\"` `\\` `\n` `\t`. `/key` marks a translation term: the literal is the English source text, the key indexes the locale tables (see Translations). |
| number | `36`, `0.5`, `-4`, `36px`, `36epx` | Bare number on a **length property = epx**; `px` = device pixels. Units on scalar props are an error. |
| bool   | `true`, `false` | |
| color  | `#RRGGBB`, `#RRGGBBAA`, `red`, `black/0.5`, `white@0.24` | Alpha **last** (Godot/CSS). Porting WPF `#AARRGGBB`: move the alpha pair to the end. Bare palette names are legal in any color position, and every color spelling takes `/opacity` — the final alpha as a 0..1 number (see Named colors). Inside a Gradient's `stops=` an entry may also carry `@position` (0..1, after any `/opacity`) — see Gradient. |
| ref    | `$name` | Widget parameter (innermost scope) or theme value. |
| ident  | `center_bottom` | Enum member / handler / easing / anim name. Enum matching ignores case and punctuation (`word_smart` matches "Word (Smart)"). |
| size   | `220x110`, `220x`, `x110` | Min-size shorthand (components epx). Omitted component = unset. Valid bare (`Button b 44x36`) or as `size=`. `size=` also takes one sizing word for both sides: `size=hug` (an Image at its picture's size), `size=fill`, `size=48` (Rust app). |
| call   | `play(appear)`, `go(expanded)`, `toggle(expanded)`, `emit(SavePressed)`, `fill(2)` | Callees on `on:` keys: `play` = anim trigger, `go` = arrangement flip, `toggle` = flip between the pose and base, `emit` = host-signal relay. On `width`/`height` and grid tracks: `fill(n)` weighted sizing (n may be fractional). |
| list   | `[80, hug, fill(2)]` | Values in brackets; newlines free inside. Only Grid's `cols=`/`areas=` and Gradient's `stops=` take one (everything else is fenced). |
| path   | `listBrush.translate.x` | **Attached** dots fuse into one token: anim line targets and dotted `when` keys. A dot after whitespace is style application (`.ToolButton`). |

## Directives

### Themes

```kdl
$accent = #E51400            // top level: a default token
theme {                      // the DEFAULT theme block
    $chrome = #1F1F1F        //   $token entries — same as the top-level spelling
    font = "res://fonts/Roboto Regular.ttf"   // control defaults — see below
}
theme dark { $accent = #FF6B4A }   // variant overrides, active via set_theme_variant("dark")
theme fancy { font = "res://fonts/Fancy.ttf" }
```

Tokens (`$name = value`) are referenced anywhere a value goes as `$accent`.
Widget parameters shadow theme values. Undefined/duplicate `$name` =
load-time error. A number token in a size, spacing or padding
(`height=$backBoxHeight`, `min_width=$rowFont`) reads the default entry
only — a `theme dark` block can't change a laid-out size.

**From code**: the build writes each page's tokens as `enum Theme`
(`$accent` → `Theme::Accent`), so code that paints with the file's colors
reads them by name: `rt.theme_color(&page, Theme::Accent)` gives the active
variant's color (else the default block's), `$refs` followed like the
file follows them; `None` when the value isn't a color. A renamed token
is then a compile error, not a stale copy of the hex in code. Numbers the
same way: `rt.theme_number(&page, Theme::BackBoxHeight)`.

**Color overrides** (`red = #E51400` entries, no `$`) remap the built-in
named colors — see Named colors below.

**Control defaults** (`prop = value` entries) build a Godot `Theme` resource
applied to the **view root**, so they PROPAGATE to every descendant that
doesn't override them — the analog of WPF's inherited `FontFamily`/`FontSize`.
An active variant's entries win over the default block's. Supported: `font`
(a font path), `font_size` (epx — re-applied on unit changes like any
length) and `disabled_opacity` (0–1, default 0.3: how far a `disabled`
control's subtree dims — spec "Default look", Disabled). Layout props like `width` can't go on a theme — that's what styles
are for.

**Type defaults** (`Slider .ThinSlider render=gogo` entries) default every
instance of a CONTROL type — style and backend as one prototype. See
"Theme type defaults" under Controls.

**Built-in default font_size = 15 epx** (D88). A document with no
`theme { font_size = ... }` behaves exactly as if it declared 15 — text never
silently keeps Godot's raw, density-blind 16 px. Declare the entry to
override; a `default_font_size` the HOST already put on the root wins (gogo
never clobbers a user theme).

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
load-time error.

Semantic tokens — **aliases resolved at theme time**: `accent` = `blue`,
`on_accent` = `white`, `danger` = `red`, `warning` = `yellow`,
`success` = `green`, `info` = `teal`, `text_muted` = `gray`. Remap the
literal (`theme { red = #E51400 }`) and every alias follows; or remap the
token itself (`theme { accent = purple }`).

Mode tokens flip under the **well-known variant name `dark`**
(`set_theme_variant("dark")`); literal colors never flip:

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

Overrides live **only in theme blocks** and take a hex color, another color
name, or a `$ref` (`theme { info = $brand }`). The name set is closed — new
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
property errors with "use .X to apply a style".

**Inheritance**: `style Warn extends=ToolButton, background=#AA0000 { ... }` —
single inheritance; the parent's props and when/transition blocks apply first, the
child's override per key (the same last-writer rule as listing two styles on
a node). Chains work across `uses` imports; the parent name resolves in the
current scope (widget-local shadows document-level). Cycles and unknown
parents are load-time errors.

#### Checked styles: `for=`

`for=<Control>` (first property, always) points a style at one control
type's CONTRACT — its slots and states — and turns on write-time checking
(customization spec §2, D109):

```kdl
style ThinFader for=Fader, slot.track.height=4, slot.thumb.size=8 {
    when held    slot.thumb.scale=1.2       // contract state — checked as you type
    when focused slot.track.background=$accent
}
Fader vol .ThinFader value=50
```

- **`slot.<name>.<prop>`** restyles the control's slot regions — placement,
  the box family, `size=` (a bare number is a square). Everything is
  verified at the declaration: unknown slots list the real ones, driven
  properties are the driver's (`slot.thumb.x` → G3002 "moves by itself"),
  content props route to filling. Slot paths are also legal directly on a
  contract-bearing **instance line** (`Fader f slot.track.height=2`) — a
  one-off tweak needs no named style.
- **`when` states are checked at write time** against the target: universal
  states always pass (an undeclared one draws an advisory to add
  `state_builtin`, so the contract lists it); unknown or un-opted contract
  states error with the control's real state list and fix-its.
- **When-responses on slots** take the motion vocabulary (`scale`,
  `translate.x/.y`, `rotate`, `opacity`) and the box family; static motion
  stays an error (base looks declare no transform).
- The target must be reachable **where the style is written** (`uses` the
  declaring file); widgets are not targets (style their nodes by name).
- **`extends=` compatibility**: same-target chains and `for=` over a plain
  base are fine; a plain style can't extend a `for=` base (it would inherit
  slot paths it can't check), and cross-target chains error.
- **Application gate**: wearing a `for=Fader` style on anything but a Fader
  is an error; on a NATIVE built-in, contract features error G5106 with the
  `render=gogo` opt-in as the fix (see "Built-in backends" under Controls).
  When the style needs nothing from the contract (no slot paths, only
  natively-raised states), the cheaper exit is offered first: drop the
  `for=` and it applies natively. Button is the settled case of that rule
  (D112): its contract has no slots, so every `for=Button` style is
  write-time state checking that applies natively — no gate, no backend.
- **Plain styles** may still name any state — contract states are checked
  where the style is *applied* ("Button has no state 'busy' — states
  here: …"). A plain style whose custom state lands cleanly on exactly one
  control type draws the D2 advisory to add `for=`; landing on two distinct
  types reads as deliberately generic and the advisory stands down.

### Widgets

```kdl
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
  anims/styles/whens, not to the view's named map. The instance name maps to
  the root.
- `signals` — flags the node that exposes signals: instance-level wiring and
  auto-wiring bind there.
- Non-parameter instance properties pass through to the root. Widget cycles
  are load-time errors.
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
- Widget-scope `when`/`transition` lines (siblings of the body root) bind to
  the hosting control, exactly as if written inline as the host's children;
  their `name.prop=` targets resolve in the widget body's own names. Bare
  props (`when held background=...`) land on the host itself.
- A `for=` widget instantiates only as the DIRECT body of its host type;
  anywhere else the build fails naming the contract.
- The host's own inline `when` for the same state replaces the widget's
  response for that state (nearer scope wins).
- Widget-scope `when` without `for=` is a build error — no host, no state.
- Inside any widget body, `when selected` on a plain node (not a toggle
  surface, not a ListItem) is a build error: nothing raises it there — the
  dead-when trap that shipped unfilled radio dots (D276).

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
  the instance brings bare children. At most one default slot.
- **`slot <name> ...`** (or any slot with props) declares a named **region**:
  a real node the declaration line places and paints. Its line takes the
  placement vocabulary (`width`/`height`/`min-*`/`halign`/`valign`/margins,
  `WxH`) and the **box family** (`background`/`corner_radius`/`border-*`/
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
- Duplicate slot names and a second default slot error at the declaration.
  Unknown fill names error with the widget's slot list and a rename
  suggestion; bare children on a widget with no default slot error listing
  the named fills. Fill blocks take **no** props (the declaration places the
  region; a fill is content).
- Slot names live in their own namespace (`slot header {` at fills,
  keyword-anchored) — body nodes may freely share them.

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
        slot label { Label text=live($value) }  // live read of a contract prop
        slot thumb automatic=position by $value in $min..$max { Circle width=20, height=fill, color=white }
    }
}

Fader gain value=25          // drag_started auto-wires OnGainDragStarted
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
An unknown name refuses at load listing every behavior the build knows;
a slot the behavior reads that the body lacks refuses too.

`keys <groups>`: the key groups a focused instance takes for its
code-behind — `arrows`, `home_end`, `enter`, `space` (semantic words,
never raw key names, so a future remap layer leaves claims standing). A
claimed key arrives as a `Key` event on the instance root (`on:key=` or
auto-wiring) and never moves focus; an unclaimed key traverses as
always; Tab is never claimable — the way off a control; a disabled
instance claims nothing. One owner per group: a behavior OWNS the groups
its spec lists (`slider`: arrows, home_end; `toggle`: all four), and a
`keys` claim on one of them refuses at load (G0082) — never a silent
precedence. This is the built-ins' own law (a Slider claims its arrows
unconditionally) extended to user controls.

- **`$params` are LIVE props**, exposed as real properties on the root —
  `GetGogoNode("gain").Set("value", 50)` re-fires everything. In the body,
  a bare `$param` read is an error with the wrap fix-it (`live($value)`,
  G5103): a pasted read would freeze at the initial value. Theme refs
  (`$accent`) stay static and legal. `live()` targets engine properties and
  is a WRITER — a `when` holding the same property is a two-writers error.
  Float displays round to 0.1 by default; `live(round($value, 1))` picks
  the step (it also quantizes numeric targets).
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
  a `live($value)` label follows every write. Keys reach the behavior only
  through focus (declare `state_builtin focused`); the wheel reaches the
  instance under the pointer with no focus and parks focus there. A
  disabled instance answers nothing; disabling mid-drag cancels the
  gesture. The FEEL: a write while the finger holds the control lands
  direct (the thumb stays under the finger); a tap's settle, a key step
  and a code write glide by the control's `transition` rule.
- **Your own behavior** (Rust): implement `gogo::Behavior` — `began(cx, x,
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
  slot wedge automatic=arc(from=-225, sweep=270) by $value in $min..$max
  slot dot   automatic=angle(from=-225, sweep=270) by $value in $min..$max
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
  directly, discrete changes (code, key steps) glide through the nearest
  bare `transition` rule (node block > applied style > the declaration's
  default feel); at rest the driver disarms and the written layout HOLDS.
  Automatic slots must sit in a free host (a slot region, Layers, Circle) —
  a measuring container is a declaration-time error (G5105). A `position`
  slot's width comes from its content: `width=fill` content is a build
  error unless the slot line sizes itself (`slot thumb width=20`).
- **Radial kinds** (spec §4.6): `arc` clips the slot region to
  the angular WEDGE `[from, from + fraction × sweep]` — content-independent
  (fill the slot with anything, the wedge still reveals it by value), and
  hits agree with the pixels. `angle` SEATS the region on the
  circle centered in its host at `from + fraction × sweep` (angular
  position, never rotation). Both REQUIRE their aperture as kind config
  (`automatic=arc(from=-225, sweep=270) by $value in $min..$max`;
  degrees — 0° points right, positive turns clockwise) — declared once;
  the behavior reads the same truth via `slot_aperture(name)`
  (`Vector2(from_deg, sweep_deg)`). The typical arc content is a bare full
  `Arc` ring — the wedge does the revealing. See
  `docs/design/examples/control_customization/knob.gogo` + `knob.gd` for
  the reference radial control.
- **States**: customs (`state <names>`) sit between `held` and `disabled`
  in priority, declaration order among themselves; `disabled` beats
  everything. Instance
  and style whens respond to declared states like any built-in
  (`Fader f { when held opacity=0.85 }`); undeclared contract states
  error listing what's available.
- **Scoped anims** may target `slot.<name>.<motion word>`
  (`slot.thumb.translate.y from=-3, to=0`) — the ONLY place anims address
  slots, and only on the motion channel (driven layout keeps its one
  writer, G4001).
- Debug: `gogo.set_contract_overlay(root, true)` (or the preview's
  Contract button) badges every control with `type · fraction · states`.

### Built-in backends (`render=`)

Shipped built-ins are raw Godot nodes (`Slider` **is** an `HSlider`) — no
slots, no contract states, byte-identical for existing projects. An
instance gains its Gogo contract only when it opts in (customization spec
§5.7, D111):

```kdl
Slider volume render=gogo, value=$vol, min=0, max=100, step=5
```

- **`render=gogo`** swaps in the Gogo composite implementation — a
  `control` built with the machinery above (addon-shipped:
  `addons/gogo/controls/slider.gogo`). You rarely write it by hand: it is
  what the G5106/D15 error offers when a `for=Slider` style, a slot fill,
  or an inline `slot.` path lands on
  a native instance (`when held` needs no opt-in — the native grabber
  drag flips it too, D181). The fix-it is **confirm-level, never auto-applied**:
  it changes the node's class, so `as HSlider` casts on that instance stop
  working (address it through `value`/states/signals instead).
- **Visual + behavior parity**: an unstyled composite draws the SAME theme
  resources the native node draws (groove/area styleboxes, the grabber
  icon) and answers input identically — the conformance tests drive both
  backends with the same scripted input and require identical value/signal
  traces, native as the oracle. The gogo events (`on:edit`/`on:change`)
  are synthesized identically on both backends, and the Godot-compat raw
  signals (`drag_started`, `drag_ended`, `value_changed`) stay real —
  explicit raw wiring and `OnFooDragEnded` auto-wires bind across
  backends (`on:value_changed` itself is a shadowed spelling: it
  redirects to the gogo events and never auto-wires, D181). Styles then override
  piecemeal: painting a slot's box (background/border) replaces that native
  part; a fill replaces its content.
- **`render=native`** is the spelled opt-out against a theme entry (below)
  — and legal, silent, anywhere on a contract built-in: the *defensive pin*
  for instances whose code-behind casts the node class. On a user control
  it errors (no native backend exists); `render=gogo` on one is silently
  redundant. Composite set today: **Slider** (both backends) and
  **Toggle** (gogo-only: a new control with no native class — unmarked
  instances take the composite, `render=native` on one errors loudly, and
  D15 can never fire on it; `$value` 0..1 is the thumb's truth, `selected`
  derives from it crossing the midpoint — live mid-drag, state is
  presentation — taps glide to
  the other end, drags settle by side, and the root exposes a `selected`
  bool plus `change(bool)` on settle, fired only when the settled side
  differs from the pre-gesture side — wiggling across the middle is zero
  changes).
  **Button deliberately has none** (D112): its children are free-form
  content already, plain styles cover `held`/`selected`, and named fill
  points are a widget's job (`widget IconButton { Button { Row { slot
  leading; slot } } }`). A `for=Button` style is write-time state checking
  and applies natively; a slot path in one errors at the declaration.
- Composite params mirror the native defaults (`min=0, max=100, step=1`)
  and quantize like `Range` (snap to `$step` anchored at `$min`, then
  clamp) — flipping a backend never moves a value. Native-only extras
  (`tick_count`...) are not on the composite; styles are the composite way.
- **Hot reload carries value and focus across backends**: editing `render=`
  rebuilds the node, and the value lands on the other implementation (the
  same carry that preserves slider values across any rebuild).

### Theme type defaults (`theme { Slider ... }`)

Per-instance `render=`/styles are right for one-offs and wrong as app
policy. A **type-keyed theme entry** defaults every instance of a control
type (spec §5.8):

```kdl
theme {
    Slider .ThinSlider render=gogo   // every Slider: ThinSlider look, Gogo backend
    Button .AppButton                // plain default style — native Buttons keep working
}
theme dark {
    Slider .ThinSliderDark           // a variant overrides the LOOK half only —
}                                    // render= is variant-invariant (default block)
```

- **Shape: an instance-line prototype** — type, styles, props; no name, no
  children, no `on:` wiring ("a theme entry has no name to wire"), no
  placement (size/margins belong where the instance sits). Types must be
  controls: the contract built-ins or your `control` declarations.
- **Nearest scope wins**: an instance line beats the entry — two tiers, no
  third. The entry's style merges base-most (theme style → applied styles →
  node props → when overrides); entry props are a defaults tier instance
  props override individually.
- **`render=native` at an instance discards the entry WHOLE** — style
  included: the prototype is taken whole or not at all. That instance stays
  native and unstyled-by-default; the cast-protecting pin may be written
  *before* any entry exists.
- A `for=X` default style **implies the backend**: an entry pairing one
  without `render=gogo` errors once, at the entry, with the add-it fix-it
  (D15's theme-entry shape).

### Animations

See the Animations section. `anim name { ... }` at top level or inside a
widget body.

### Includes

```kdl
uses "styles/theme.gogo"     // relative to the current file
```

Brings theme values, theme blocks (default and variant), styles, widgets and
anims. Later definitions override. Include cycles are load-time errors. A
`theme { font = ... }` in a shared include therefore sets the default font of
**every view that uses it** — the usual place for an app-wide typeface.

### Atlases — one picture, cut into slices

An **atlas** is one picture file holding many small pictures: a sheet of
stickers, a strip of toolbar icons, a packer's export. An `atlas` line
names the picture once and says how to cut it; every piece is a
**slice**, and an Image shows one with `source=`. The picture decodes
once and every slice shares it (Rust gogo; built 2026-09-27).

Four forms, one keyword:

```kdl
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
  Image, the way the effect presets do).
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

Any Godot engine property of the control works, snake_case, with typed
validation. Additions and aliases:

| Key | Meaning |
|-----|---------|
| `halign` / `valign` | WHERE the control sits in the available space: `halign=left/center/right`, `valign=top/center/bottom`. One vocabulary for both regimes — container children get size flags, children of a plain `Control`/`Layers` get anchors + offsets. Position only; sizing is `width`/`height`. See Placement below. |
| `width` / `height` | HOW BIG: a number = that exact size (epx, and don't stretch); `hug` = natural content size; `fill` = take the leftover space; `fill(2)` = a double-weighted share of it; `hug(2)` = a weighted slot around a natural-size control. See Placement below. |
| `size=WxH` / bare `WxH` | Exact-size shorthand for both axes (`50x90`); omit a component to leave that axis alone (`x72`). |
| `min_width` `min_height` `min_size` | Minimum only — the control still stretches with its container. The "at least N, stretchy" spelling. Next to an exact `width=`/`height=` on the same axis it's refused (an exact size is its own floor and ceiling) — unless a `when` block poses that axis' size, where the floor bounds the posed values (D249); with a hug cap it forms the min/max band — see Placement. |
| `margin=N` / `margin_left/top/right/bottom` | Margins CARVE the box the other keys produce on a free (`Layers`/plain-`Control`) child — SIGNED distances from the parent's matching edges: positive sits inside, **negative hangs past the edge**. On a filled/default axis they are insets (the unnamed edge stays flush); on an aligned axis they offset the near edge; on an unaligned `hug`/exact axis they pin. `margin=N` alone is breathing room all around. Containers refuse margins — space their children with `spacing`/`padding`/`gap`. See Placement below. |
| `spacing=N` | Container separation (epx). Default 0 — children touch; no gap exists unless the file writes one (D244). |
| `background` `corner_radius` `padding` `border_color` `border_thickness` | One StyleBoxFlat per control, each key changing exactly its one thing. Panels (`Panel`, and `View` roots) start as a **blank canvas** (WPF Border semantics — nothing drawn until a key says so); buttons start as a **copy of their theme chrome**. On stateful controls (buttons, `Input`) **shape is not pigment** (D128): the shape keys (`padding`/`corner_radius`/`border-*`) copy every state slot's theme box and reshape them all — each state keeps its own theme color, so press/hover/read-only feedback stays alive. `background=` is the **pigment takeover**: one shared box across all state slots (state tweens own the fill — press feedback is yours to author with `when held background=` from there; a `when` response hands the slots back when it ends, shape copies included). `background=` recolors, it never reshapes — corners/borders/margins come from the theme copy or your explicit keys. `padding` also comes per side (`padding_left/-top/-right/-bottom`, mirroring `margin-*`) for WPF's per-side Padding thickness — asymmetric content air that still counts into a button's derived minimum. **Text leaves pad too** (D118 follow-up): `padding` is air between a node's box and its CONTENT — children or text — so `Label` and `Input` inset their own text through the same stylebox mechanism buttons use (WPF's TextBlock/TextBox Padding ports verbatim; their OTHER box keys stay refused — a chip's drawn background/border is a `Panel`'s job, though `when` responses may still recolor a label's generated box). Drawing leaves (`Rectangle`/`Circle`/`Arc`/`Gradient`/`Image`) have nothing inside — outer air is `margin`. **`padding` is air INSIDE the border**: on a bordered node the content sits `border_thickness + padding` from the edge (WPF/CSS stacking — XAML Padding ports verbatim; a border alone insets content by its thickness). Raw Godot styleboxes overlap the two — gogo compiles the stack, so a border can never paint over content and retuning one knob never re-derives the other. Fractional borders keep their declared thickness in the layout math even though the painted stroke rounds to whole px. On a `Circle`, `border_color`/`border_thickness` stroke the ring instead (no stylebox — see Circle). **`padding` (alone of the family) also applies to the free-placement regions** — `Layers` and the plain `View` root: no stylebox is involved; the declared air folds into every DECLARED child's placement (the slot-region mechanism, D114) and adds onto the region's minimum. Paint stays on boxes — `background` on a `Layers` remains an error. On a Godot-sorted container (`Column`/`Row`/`Grid`/...) `padding` is refused loudly: flow-axis air is a `gap N` line, inset air wraps the content in `Layers`/`Panel`. **Corners also come per corner** (D135): `corner_radius_top_left`/`-top-right`/`-bottom-right`/`-bottom-left` — named keys, so there is no corner order to memorize; the uniform writes all four, a per-corner key writes one, and later on the line wins (the `padding`/`padding_left` layering law). |
| `opacity=0..1` | `modulate.a`. |
| `pivot=0x0` | *(FanRust, 2026-09-02 — the addon predates it.)* The 0..1 box point `scale` and `rotate` transform about; unset = the center (the old behavior verbatim). `0x0` holds the top-left still while a scale grows right and down; `1x1` swings a rotation on the bottom-right hinge. Same spelling as a Gradient's `from=`/`to=` box points; anything but a full 0..1 pair refuses at load. A look word (`when` responses restyle it), not itself animatable. Hits follow every motion channel — translate/scale/pivot through the moved rects, rotation by un-rotating the finger — so look and touch always agree. |
| `visibility=visible/space_only/gone` | The three-state triad (D133; the bool `visible=` is retired). `gone` hides and releases the space; `space_only` hides but keeps holding its layout slot — toggling it never moves a sibling (WPF Hidden). Hidden is hidden either way: hit-inert, unfocusable, a11y-skipped; `on:show`/`on:hide` track the pixels. `space_only` binds under FREE parents only (engine containers skip hidden children in their own sorting — v1). Code-behind: `gogo.set_visibility(node, mode)`; plain `.visible` engine writes keep meaning visible/gone. |
| `reveal_y=0..1` / `reveal_x=0..1` | The node's LAYOUT SPACE as a fraction — the smooth-slide primitive. `0` conceals (the node earns no room on that axis, the trailing edge clips — drawing and hits; the leading edge stays pinned), `1` grants it all; declaring either wraps the node in a clipping proxy the parent sorts instead. The factor applies to the extent the node would otherwise have — natural size when hugged, the **stretched extent when filled** (a label stretched to a 100-wide column at `reveal_x=0.5` shows its first 50 pixels). It's a float, so poses flip it, `transition` rules tween it, anims drive it — and the whole document reflows smoothly. A concealed node is still **alive** (anims run, conditions poll) — `visibility=gone` stays the instant switch and zeroes the space. Container parents only (`Column`/`Row`/`Grid`/`Panel`/`Scroll`/`Flow`); under `Layers`/free parents nothing reflows — overlay looks animate `opacity`/`translate` instead. The channel exists only where DECLARED: a `when`/anim driving `x.reveal_y` without `reveal_y=` on `x`'s line is a load-time error. See the COOKBOOK smooth-slide recipe. |
| `font_size=N` | Font size theme override (epx). **Node-local** — see the font note below the table. |
| `font_color=#...` | Text color across all state slots — the text half of `background=` (what WPF calls `Foreground` and CSS calls `color`). Pairs with `font_size`. |
| `icon_size=N` | Button icon max width (epx). |
| `min` `max` | `min_value` / `max_value` (Range). |
| `toggle` | `toggle_mode`. |
| `touch_target=off` | Accept a deliberately small interactive control — silences the advisory 44-epx touch_target lint. |
| `keyboard_target=off` | Accept a deliberately touch-only button (`focus_mode=none` — the WP app-bar pattern) — silences the advisory keyboard-reachability lint (D129: a focusless button is invisible to keyboard/gamepad users and never shows a focus indicator). |
| `interactive=true/false` | THE input switch: `true` claims input for this control, `false` makes it hit-invisible (unlike `disabled`, no restyle). Per control — never the subtree. Interactive controls answer input by default, everything else is transparent; see Input. Replaces the retired `mouse_filter`. |
| `overlay=true` | THE layout-negotiation switch (D139), completing the node-role trio: `interactive=` is a control's relationship to input routing, `overlay=` to layout negotiation, `visibility=` to rendering presence. It cuts exactly one thing — the box's upward size contribution: no ancestor hug/minimum (and no scroll range) counts it. Everything else is untouched: placement (signed margins may hang it past the parent), draw order (declaration order, as always), clipping (a `Scroll` viewport still cuts it — a box that must beat a scroller is a popup, a different concept), hit-testing (hit order is draw order, last declared wins), focus, event routing. For boxes that sit on a parent without sizing it: touch-slop surfaces, finger pucks, drag markers, badges. Free parents only — permanent law, not a v1 gap: a flowed child holds a SEAT it can't refuse, and `Column { Layers { content ... overlay } }` already spells that case. The overlay's own inside still checks: its children validate against ITS box; a nested overlay is exempt only at its own parent boundary. Refused: with `visibility=space_only` (a seat-holder is not an overlay — a hidden overlay is `gone`), and `fill` on an axis the parent hugs with no non-overlay child (nothing determines the size). |
| `color` | The fill of nodes that OWN a plain color (`Rectangle`, `Circle`) — their fill *is* their color. On anything else it's an error that points to the right channel: text is `font_color=`, fill is `background=`, a `Gradient` paints its `stops=`, whole-control fade is `opacity=`. (A deliberate whole-control tint is the engine's `modulate=` — never a silent fallback.) |
| `source` | The picture: `res://path` (an asset embedded at build time) or `<host>://name` (a host picture the app's code registers at run time under that exact name — a live preview, a canvas tile). See "Resource paths" below. |
| `on:<signal>=...` | Wiring — see Signals & triggers. |

Resource paths. Resource-typed properties take path strings that always
carry a scheme (`icon="res://icons/undo.png"`; a bare `undo.png` is
refused, G0071 — a bare word could be a label or a path). `res://` is an
asset under the build's resource root, checked and embedded at build
time. Any other scheme on an `Image`'s `source=` — `preview://brush`,
`canvas://tile`, `crop://source` — is a **host picture**: the app's code
hands the renderer pixels under that exact name at run time, and the
box shows nothing until it does. The name in the file is the contract:
the build bakes it as the picture's id, code looks the id up by the same
string (Rust `Images::get`), and an `Image` with no `source=` is a
picture of nothing however much the code registers. An **empty string
means null** — no load attempt (safe widget `$icon=""` defaults). A
non-empty `res://` path that fails to load is a build error.

Type names — one friendly name per concept: see the **Vocabulary** table at
the top. Any other Godot `Control` class works directly (the escape
register), plus any factory registered with `register_control`. Retired
spellings error with a one-keystroke redirect: the pre-release synonyms
(`VertPanel`/`HorizPanel`/`StackPanel`), the old WPF dialect (`TextBlock`→
`Label`, `TextBox`→`Input`, `Border`→`Panel`, `ScrollViewer`→`Scroll`,
`WrapPanel`→`Flow`, `Ellipse`→`Circle`, `Radial`→`Gradient`), and raw engine
classes that exactly duplicate a gogo name (`VBoxContainer`→`Column`,
`ColorRect`→`Rectangle`, ...). Retired PROPERTY spellings do the same:
`mouse_filter` errors per value toward `interactive=` (see Input),
`inset`/`anchors`/`offset_*`/`SafeArea` toward the placement register.

**Node-local `font_size` doesn't inherit — set the default in a theme.**
`font_size`/`font_color` on a node are Godot theme *overrides*: they style
that node's own text only and never flow to child Labels the way WPF's
inherited `FontSize` does (a `font_size` in a button's style is dead weight
unless the button has `text=` itself). The propagating spelling is a theme
entry — `theme { font_size = 20 }` in a shared include gives every Label
in every view an epx-scaled default, exactly like the font FAMILY (`theme
font=...`) — with node-local `font_size` still winning where set. Without
one, the built-in **15 epx default** applies (D88), so text is always
epx-scaled — never Godot's raw, density-blind stock pixels. The editor
panel still flags the one remaining trap as it previews: a `font_size`
sitting on a text-less node above Labels (a rust-style warning block in the
Info pane).

### Placement

Two questions, two props, one vocabulary everywhere: **where** (`halign` /
`valign` — position only) and **how big** (`width` / `height` — the sizing
triad). The same words compile to size flags for container children and to
anchors + edge offsets for children of a plain `Control`/`Layers`; each
parent kind reads its own encoding, so a node moved between regimes keeps
meaning what it says.

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
  `Layers`/plain `Control` gives its whole rect (layers stack), a Button
  gives its content rect. Sizing, alignment and margins carve that down —
  per axis, so `width=fill` with nothing vertical fills both ways.
  Exceptions: the document ROOT (the host owns root placement) and content
  of the bare default `slot` (the inline marker — its children join the
  widget's own layout with explicit placement only, the shipped rule) keep
  explicit placement only; content of a NAMED slot region fills the region
  like any free child. A code-behind-owned rect opts out with
  `width=hug, height=hug` — natural size until the code moves it.
- **The triad**: `width=200` is exactly 200 epx (min_size + don't stretch);
  `width=hug` is natural content size; `width=fill` takes the leftover space.
  On the DOCUMENT ROOT only, `width=host` / `height=host` says the host's
  CODE owns the axis (D283): the root's minimum ignores its children there
  (declared `min-*` floors still count), gogo writes no flag and no anchor,
  and alignment/margins/caps on the axis are refused — so a child's
  one-frame measure spike (a wrap label at a stale width) can never inflate
  the code's box for good (Godot bakes a clamped `set_size` into the
  offsets and never shrinks it back; the D282 whole-screen ad plate).
  Root-only (G7004); on a `Panel` root it needs the GogoPanelView host
  (G7005). `min_width` is the "at least 200, still stretchy" spelling. Hug means
  natural size for wrap-text too (D254): a hugged autowrap `Label` measures
  at its UNWRAPPED single-line width, capped or not — wrapping happens only
  where a width binds it (`max_width=N/fill`, `fill`, a sized parent). A
  wrap that provably never binds is linted (G1004) instead of silently
  collapsing the box to a one-char-per-line sliver (the pre-D254 behavior).
  A `width=fill` child INSIDE a hug parent still measures unwrapped for
  that parent's hug: the fill has no frame yet — the parent is asking the
  children how wide to be — so the text is what sizes the box (wrapped
  there would collapse to the sliver). Fill wraps where a real frame
  exists: a sized parent, a cap, or a seat's socket (D263).
- **Hug is LIVE** (D141): a content-sized box re-hugs whenever its content's
  minimum changes at runtime — a child hidden or re-shown from code (a plain
  engine `.visible` write is enough), text swapped, a texture landing. This
  covers every measured axis: explicit `hug` AND an aligned axis with no
  size key (`Panel valign=top { Column ... }` — the WPF StackPanel shape,
  which shrinks when rows hide). It's the D132 capped-hug re-assert
  generalized: same deferred convergence, and axes that size from pins
  (`fill`, both-margins span) or a declared number never re-measure.
- **The hug caps** (D132): `width=hug, max_width=336` (and the height twins)
  = natural size, at most the cap — wrap-text hugs its UNWRAPPED extent
  (every hug axis does, D254) and wraps only when the cap binds, which is
  how you spell a hugging label that folds on narrow screens
  (`max_width=fill` = wrap at the live frame). The box re-fits live when
  code-behind swaps its text. Under a CONTAINER parent (D144) the cap works on the boxes that
  REPORT their own minimum — `Layers` (and `Scroll`/`List` by their
  scroll-past-cap doctrine): the reported minimum clamps at the cap, the
  container grants it honestly, and content past the cap DRAWS past the box
  — no clip. NOTE this is a declared DIVERGENCE from WPF, not an
  equivalence (corrected 2026-07-27): WPF LAYOUT-CLIPS when ink exceeds a
  `Max*` constraint (ArrangeCore's needsClipBounds — a third text line
  under `MaxHeight` comes out sheared mid-glyph), while gogo lets the
  overflow draw whole into the neighboring air. Same box geometry, softer
  overflow; where a shipped design RELIED on the shear, say so at the
  site.
  Any other box under a container refuses the cap toward Layers. Hard
  content minimums win under free parents — a cap never shears there.
  Wrap-capable text is `Label` and, since D135, a raw-register
  `RichTextLabel` — give the latter `fit_content=true` (its engine minimum
  counts no content otherwise; the shaped minimum then lands the box on
  tree entry — a detached RTL measures 0 by engine law).
- **The min/max band** (D135): `width=hug, min_width=100, max_width=300`
  clamps the natural size into the pair — below 100 the floor holds, above
  300 the cap binds (wrap-text wraps there). The floor beats the cap (WPF
  clamp order): a runtime `$param` floor that crosses above the cap wins.
  Statically visible contradictions refuse at load instead of resolving by
  precedence: a literal floor above a literal cap, `min-*` next to an exact
  `width=`/`height=` on the same axis (an exact size IS its own floor and
  ceiling), and `min-*` with `aspect=` (a forced axis would silently break
  the ratio — the cap refusal's twin). A floor under `fill` stays legal:
  "at least N, still stretchy" is engine-native everywhere.
- **`max_width=fill`** caps at what a `fill` axis would GET — parent frame
  minus region padding minus margins, live across resizes. Margins bind the
  measuring frame like they inset fill, whatever the alignment, so a
  centered axis takes them here (here and on a capped fill — the "pins no
  edge" fence holds otherwise): `width=hug, max_width=fill, halign=center,
  margin_left=12, margin_right=12` is the toast shape — hug the text,
  center it, wrap at live width − 24.
- **The capped fill** (D245): `width=fill, max_width=380, halign=center`
  (and `max_height=` on `height=fill` the same way) = the available
  space, never more than the cap.
  Narrower screens fill edge to edge like plain `fill`; when the cap binds,
  the stated side places the box — and the side is REQUIRED, because the
  leftover space has to go somewhere and the file must say where (WPF's
  Stretch + `MaxWidth` silently centers; gogo writes it out). The
  centered-column-with-a-maximum shape: the effects catalog is
  `width=fill, max_width=380, halign=center` where shipped wrote
  `VertWrapPanel MaxWidth="380"`. Free parents only — under a container,
  wrap the box in a `Layers` to give it a frame to read. Margins inset the
  measuring space; the floor beats the cap (D135); not on a `Scroll`
  (a filling Scroll axis already scrolls past its frame), and
  `max_width=fill` on a fill axis refuses (it changes nothing).
- **`aspect=`** (D133): the shape of the node's OWN rectangle (the space
  layout gives the node itself, not its parent) as a ratio — a number
  (width/height: `aspect=1` square, `aspect=1.78` 16:9) or `natural` (an
  `Image` bitmap's own). The axis you spelled drives, the axis you didn't
  is derived; both `fill` + aspect = the largest ratio rectangle that fits
  the space the parent offers, centered (WPF Uniform). Both spelled is an
  error; `hug`/`max-*=` don't combine (v1). Numbers solve under free
  parents; `natural` uses the engine minimum everywhere (the bitmap-exact
  Tiles cells). Nothing spelled: width drives.
- **`fit=contain/cover/distort`** (D133, `Image` only): how the bitmap maps
  into the Image's own rectangle, orthogonal to how that rectangle is sized
  — `contain` letterboxes (default), `cover` fills and clips, `distort`
  stretches. Replaces the retired `stretch_mode=`/`expand_mode=` consts.
- **Weighted fills**: siblings with `width=fill(2)` and `width=fill` split
  the **leftover** space (what remains after fixed/hug siblings take their
  minimums) two-to-one — weights are ratios, not fractions of the parent.
  `hug(2)` is the rare "weighted slot around a natural-size control" combo.
  One weight per control (Godot shares `stretch_ratio` across axes).
- **Alignment is position only**: `halign=center, width=fill` is a load-time
  error (fill leaves nothing to position — use `hug`). In a Box container the
  cross axis is where alignment shows; on the main axis a slot is min_size
  unless it fills, same as the underlying flags.
- **Margins carve — they never pick the sizing mode.** A margin places the
  node's edge at a signed distance from the parent's matching edge —
  positive inside, **negative past it** — and what it does follows from
  what the axis's box already is:
  - **default / `fill` axis** → an INSET of the filled box; the unnamed edge
    stays bound at inset 0 (the WPF Margin-under-Stretch reading — signed,
    so a node can exceed its parent). `Rectangle strip margin_bottom=-10`
    on a free child spans its parent and hangs 10 epx below it.
  - **aligned axis** → an offset of the near edge (`valign=bottom,
    margin_bottom=90` sits 90 above the bottom). The far edge of an aligned
    axis and any edge of a centered one can't bind — explicit keys there
    are load-time errors.
  - **`hug`/exact axis with no alignment** → a PIN: the pinned edge moves,
    the other follows at size distance (declared size, else the live
    minimum), and grow points away from the pin.
  The `margin=N` shorthand is breathing room: insets all around on
  default-filled axes, the touched edges under alignment (`halign=right,
  valign=bottom, margin=16` keeps meaning "a fab, 16 off the corner").
  **Two per-edge keys bind both edges** — the box spans them (a declared
  size there over-constrains and errors). Document ROOTS are exempt from
  the fill default, so a lone margin there is still a pin (the classic
  matrix). Under a container margins are a load-time error — space
  container children with `spacing` / `gap` (or `padding` on a
  `Panel`/`Layers`).
- **`anchors=` / `offset_*` were removed** — placement says it all now. The
  old escape hatches translate mechanically: overhang = a negative margin
  (plus an explicit size for an exact box); off-screen parking = a negative
  pin past the whole box (`valign=bottom, height=50, margin_bottom=-90`);
  "offsets an anim drives" = the motion channel (`translate`, below) or an
  `arrangement` when the pose stands.
- **One system per node**: placement props don't mix with raw `size_flags_*`
  on the same node (load-time errors say which to keep). (`inset=N` was
  retired: a bare child already fills, and `margin=N` carves that box —
  document roots spell the fill: `width=fill, height=fill`.)
- Not anim targets (layout is declared — see Motion). Arrangement `when`
  responses may overlay `margin-*`/`width`/`height`/`min_width`/
  `min_height` per node, and that pose re-asserts itself on every
  `units_changed`.
- Don't confuse `halign` with `horizontal_alignment`: `halign` places the
  control; `horizontal_alignment`/`vertical_alignment` place *text* inside a
  Label's own box.

### View — the screen root

The root keyword set is **{`View`, `Panel`}**: a file announces itself as a
SCREEN or a COMPONENT in its first word (grammar, not convention). `View` is
the screen: the host class (a `GogoView` / any non-Container `Control`) IS
this node, WPF-UserControl-style, and its children place **freely**
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
  content, live. The answer is computed AT QUERY TIME (the `GogoView` host
  bases delegate `_get_minimum_size` to the one view formula), so
  `custom_minimum_size` stays purely the declared floor — nothing derived
  is ever stamped into it, and no build-time transient can stick. A custom
  host class extending neither base keeps the old stamped wiring (best
  effort).
- **The root plate**: `View background=..., corner_radius=...,
  border-*=...` draws on the root itself — the ONLY non-Panel home for
  `background` (never a precedent for Column/Row). `padding` on the root
  insets every DECLARED child's placement (D114 — the slot-region fold, no
  stylebox) and counts into the derived minimum; children the code-behind
  adds aren't governed — gogo insets only what it places.
- **One position**: `View` inside the tree errors (inner free placement is
  `Layers`, a styled box is `Panel`); a root that is neither `View` nor
  `Panel` errors (G7003, naming both doors). The host class must match the
  root kind (G7002): `View` → a non-Container `Control` (`GogoView`),
  `Panel` → a `PanelContainer` (`GogoPanelView`).
- NOT hidden by default: `visibility=gone` is state the file declares, never
  an identity default.

### Panel — the component root

A file whose root is `Panel` is a COMPONENT: the root IS the styled box — a
toast banner, a progress track — and the host node is the widget itself,
embedded anywhere like any Control. No wrapper node, no extra nesting;
container regime for the file's children; `padding`/border stacking works
natively (a free-placement `View` root folds padding into child placement
instead — same word, its own mechanism):

```kdl
Panel tipPanel background=#000000FF, padding=12, valign=top, width=fill,
        visibility=gone, margin_left=12, margin_right=12, margin_top=180 {
    Label tipText text="", font_size=$fontMediumLarge
}
```

The host extends `GogoPanelView` (GDScript or C#) — same lifecycle, named
map, anims and arrangements as `GogoView`. Since D283 that base is a plain
`Container` that plays PanelContainer through gogo's one panel-host formula
(minimum, sort, plate — `GogoMin.panel_host_*`), byte-equal in look and
layout; it exists so a root axis the host's code sizes (`width=host`) can
really ignore the children, which the engine's PanelContainer cannot (its
C++ minimum ignores scripts). A raw `PanelContainer` host still builds —
minus host axes (G7005). The hosts also carry a runtime net for the
declaration being FORGOTTEN (D284): a free-standing visible component
that stays bigger than the window across a frame warns once, naming its
.gogo source and the `width=host` fix — container-seated, hidden, and
one-frame-transient hosts are exempt. Pick by concept: *is this file a
screen the app shows, or a box the app embeds?* The first word answers it
for every future reader. (Inside a tree, `Panel` stays the ordinary styled
box it always was — root position is what makes it a component.)

Like every gogo Panel, the root starts on the blank slate: where the file
doesn't speak `background=` (node line or style), `transparent` is injected —
the ambient theme's gray plate never leaks into a component (the `View`
defaults' twin; anything spelled wins).

**Root placement follows the host's seat** (D189): the same placement props
mean the same thing wherever the host lands, through two mechanisms. Under
a FREE parent (a plain Control, a `View` root's surface) the root's
`width=fill` / `valign=top` / margins go through anchors — the component places
itself. Under an engine **Container** (a code-behind `AddChild` into a
Column/Row cell, a list) they compile to size flags only and the container
seats the box — the D164 law applied to the document root. The seat is read
from the host's live parent at build time (the host builds in `_Ready`, so
it is already parented). Two caveats: a hug cap (`max_*`) on the root binds
only free-parented in v1 (a container grants the honest content minimum),
and re-parenting a live host across regimes needs a `Rebuild()`.

### gap — space in a stack

`gap` is the one-off break between stack children — a structure word
(lowercase, like `row`/`skip`/`choice`: never named):

```kdl
Column spacing=8 {
    Label title text="Brushes"
    gap 12        // fixed: 12 epx along the stack's axis (units-rebound)
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
- It leaves an anonymous mouse-ignored `Control` behind (Godot stacks make
  space with child slots) — the one structure word that isn't fully erased;
  being unnameable, that node is unobservable from the language.

### Layers

`Layers` is **free placement + z-stack**: `Column` stacks children on y,
`Row` on x, `Layers` on z. Children overlap in declaration order (later = on
top) and place themselves with the normal placement vocabulary — it is NOT a
godot `Container`, so nothing gets stomped:

```kdl
Layers screen {
    Rectangle bg color=$chrome, width=fill, height=fill   // backdrop = a layer
    Image photo width=fill, height=fill
    Panel toast halign=center, valign=top, margin_top=20 { ... }
}
```

Deliberate deviations from the plain `Control` it wraps:

- **It has a minimum size**: the element-wise max of its visible children's
  minimums, so a `Layers` inside a `Column`/`Row` takes its biggest child's
  space instead of collapsing to zero. `min_size=` on the Layers itself still
  combines via max; `fill` children typically contribute 0 — they adapt to
  the Layers, not the other way around.
- **It takes `padding-*`** (D114): the declared air folds into every child's
  placement — `Layers padding_left=10, padding_right=10 { ... }` insets its
  content with no `Panel` wrapper — and adds onto the derived minimum.
  Asymmetric padding shifts a centered child to the padded box's true
  center, and an axis with nothing declared starts AT the padding instead of
  parked at 0, exactly like a slot region (§3.4).

Like all passive structure it is input-transparent (see Input); a Layers
that IS the gesture surface — the whole-row drag idiom — declares
`interactive=true`.

A raw `Control` remains available by name for the unopinionated version.
There's no "background" on a Layers by design — padding follows placement
ownership, pigment stays on boxes: the backdrop is just the first layer
(tint or fade it like any other node), or wrap the Layers in a `Panel` when
you want the styled-box look.

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
- Spans past the last row/column are load errors (WPF silently clamps).
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
`: height` — REQUIRED (a missing one is a load error; `row_height=` belongs
to the row form; D89: a silent `hug` default made dead space under the grid
hard to diagnose); `.` marks an empty region; every area must be one
contiguous rectangle; an area with no child is only a warning. Heights are
literals here ($refs need the row form).

Container behavior (gogo_grid.gd): a hug track sizes to its single-span
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
    Panel a { Image source="a.jpg", aspect=natural }
    Panel b { Image source="b.jpg", aspect=natural }
    Panel wide span=2 { ... }                      // two units of width
}
```

- `cols=N` (required — the count every `span=` is checked against) splits the
  Tiles' width into N equal units; it is a LIVE property, so a code-behind
  can re-pack the mosaic (`tiles.Set("cols", 3)`).
- `cells=lattice` (the default) or `cells=natural` (D112): with `natural`
  every cell is as wide as its own content and the row wraps where the next
  one would not fit — a names-only catalog, a row of tags. `span=` says
  nothing then (there is no unit to count); `full_row=true` still takes the
  row and an exact width still packs at that width. A cell wider than the
  Tiles is capped at the Tiles' width — give a chip's label `wrap=true` and
  a long name folds inside that row instead of running off the edge. Live
  like `cols=`, and a pose may
  flip it (`when names { panel.cells = natural }`) — one list, two packings:

  ```kdl
  Tiles catalog cols=2, cells=lattice {
      Label title text="top effects", full_row=true
      Panel colorize { Image .PictureOnly source="colorize.jpg", aspect=natural; NameChip text="Colorize" }
      Panel darkDream { Image .PictureOnly source="dark.jpg", aspect=natural; NameChip text="Dark Dream" }
  }
  when names { catalog.cells = natural }   // chips pack at their own width
  ```
- Cell props: `span=K` takes K units (over `cols=` is a load error);
  `full_row=true` takes the whole row at any column count (exclusive with
  `span=` — a full row IS the whole span).
- Cell HEIGHT is the cell's own content, top-aligned in its row; the row is
  as tall as its tallest cell. There is no lattice quantum for heights: an
  `Image` with `aspect=natural` makes the bitmap's ratio
  size the tile (square bitmaps → square tiles) — WPF's measure-with-
  constraint answer without the constraint protocol.
- An invisible cell REFLOWS away entirely (wrap semantics — Grid keeps the
  cell, Tiles doesn't have one to keep).
- Width feedback: like an autowrap Label, the minimum HEIGHT depends on the
  granted width, so a fresh build settles over a frame or two; tests await
  ~3 frames before asserting rects. The minimum WIDTH is the widest cell's
  OWN minimum (FlowContainer's rule) — never zero, so a hugging parent still
  grants a real width instead of deadlocking the pack at 0, and never
  lattice-scaled: oversized content never widens the mosaic.
- Cells never SHRINK content either — the engine never renders a control
  below its minimum, so a cell child whose minimum exceeds its cell OVERLAPS
  the neighboring cells (it does not clip). Tiles warns at sort time when
  that happens (one `push_warning` per offender; `overflow_warnings` for
  tests). The fix belongs to the content: `clip_text=true` on a Label (a
  clipping Label demands no width — how tile captions truncate), a
  `clip_contents=true` wrapper for anything else, or a wider cell (`span=`).
- Runtime cell API (code-behind): `set_cell(child, {span, full_row, fixed_w,
  fixed_h})` — `fixed_*` (px, pre-scaled) packs a cell at its own size,
  advance-by-actual: how "explicit Width" children pixel-pack independent of
  the lattice; `-1.0` clears back to lattice sizing.

Use `Flow` when items should keep their natural sizes; `Grid` when rows and
columns are addressed structure; `Tiles` when equal columns rule and content
flows through them.

**How wide a Flow is.** `width=fill` (or no alignment): the Flow takes the
width it is given and every row starts at its left edge. A hugging Flow
(`halign=center`, no width) packs its rows into the width it is offered,
then is as wide as its widest wrapped row — like a wrapping Label. So
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
fed by the host in pixels and re-applied live:

```gdscript
gogo.set_safe_insets(left, top, right, bottom)   # px; call again on change
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
- **Both families, one value**: `padding-*=safe` counts into the hug and
  sits under the background/hit rect (bars, scroll content);
  `margin-*=safe` pins/carves a free child (a close button riding the top
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
- Insets are PIXELS, not epx: they come from the OS (e.g.
  `DisplayServer.get_display_safe_area()`), not from the design grid — the
  host converts however it measures. Pushing new values re-applies every
  `safe` binding on tracked views (orientation flip, inset simulation).
- `SafeArea` was retired (D74): the wrapper applied every inset unless
  switched off, and that opt-out padded docked panels on edges they never
  reach. The build error routes here.

### Scroll

`Scroll` is a `ScrollContainer` subclass that owns touch panning itself
(WPF-ScrollViewer style), so it works with **no input tuning**:

- A drag starting on any child — including plain buttons — pans the list once
  it travels past the deadzone (16 epx; a raw `scroll_deadzone=` override
  stays engine px), and the child's press is cancelled: `activate` only
  ever fires for genuine taps. **The direction the finger commits to
  decides who gets it** (decided 2026-08-30): once past the deadzone the
  drag's dominant direction is read once. A Slider keeps a sideways drag
  (its own axis) and a Toggle keeps a drag it already started; otherwise
  the innermost Scroll or List that scrolls THAT way and has somewhere to
  go takes the gesture and the child's press is cancelled (a Slider's
  value goes back to where the finger found it). If nothing around the
  child scrolls that way, the child keeps the finger for good — a
  sideways drag on a Button in a vertical list steals nothing.
- Releasing mid-pan glides with velocity (`fling_boost=2` release-velocity
  amplifier — WP's flicks outrun the finger; `fling_friction=1.5`
  exponential decay, `fling_min_speed=40` epx/s — settable as properties);
  touching a gliding list stops it without pressing what's under the finger.
- Edges are springy (Windows-Phone feel): content pulls past its limit with
  a diminishing give and springs back on release; a glide that hits an edge
  is absorbed and settles back. Tune with `spring_stiffness=200`,
  `spring_damping=24` (lower = bouncier), `overscroll_give=0.5`.
- Gesture thresholds are epx (design units, D95): the deadzone and the
  catch/min/max fling speeds use the host's epx provider, so one flick
  feels the same on the 1:1 desktop preview and a 3-px/epx phone. Friction
  and the spring rates are per-second — density-free already.
- Mouse wheel and scrollbar drags keep native Godot behavior. Use the raw
  `ScrollContainer` type if you want the engine's stock touch handling.
- **Content takes the viewport** (D94): an unkeyed axis on the Scroll's child
  sizes to the viewport and scrolls past it when its minimum is larger — the
  WPF ScrollViewer reading, and the one CONTAINER case of the free-fill
  default. (The engine stretches only `SIZE_EXPAND` children; without the
  default, unkeyed content quietly sat at its own minimum width.) A declared
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
  axis is content-sized, and a content-sized axis has nothing to scroll —
  it compiles to `SCROLL_MODE_DISABLED`, and the engine then counts
  content into that axis' minimum (which is what hug means); a number,
  `fill` or an unkeyed axis scrolls. `Scroll height=100, width=hug` is a
  menu: as wide as its widest row, exactly 100 tall, panning vertically —
  the WPF default ScrollViewer, spelled out.
- **`bars=` paints the scrolling axes' bars** (D138):
  `auto|never|always|reserve` — `never` hides the bar, the axis still pans
  (the handheld look); `reserve` shows like auto but always holds the
  bar's space, so content doesn't shift when it appears. Inert on a
  non-scrolling axis.
- **A capped hug axis scrolls PAST its cap** (D138 — the List D124
  doctrine on Scroll): `height=hug, max_height=300` follows the content up
  to 300 epx, then the axis scrolls. The Scroll's minimum stops at the
  cap, so engine containers grant it honestly (the D132 free-parent rule
  excepts Scroll); `max_*=fill` refuses — `fill` already scrolls past the
  frame. Contradictions refuse at load naming both facts: `allow=` listing
  a bare hug axis, a capped axis left out of `allow=`. Both axes hugged
  bare = nothing left to scroll (build warning). Raw
  `horizontal_scroll_mode=`/`vertical_scroll_mode=` props keep engine
  meaning and own their axis (the per-axis-bar escape hatch) — never mixed
  with `allow=`/`bars=` on one Scroll.

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
  target; unhandled events still bubble to ancestors). `interactive=false`
  makes an interactive control hit-invisible — unlike `disabled`, it never
  restyles. Neither touches the subtree: children keep their own behavior.
- Passive means every display/layout built-in (`Panel`, `Rectangle`, `Image`,
  `Label`, `Layers`, `Row`/`Column`, `Grid`, `Tiles`, `Circle`, `Gradient`,
  a plain `Control`, the `View` root) — none of them ever claims a press.
  Godot's per-class defaults (plain Control/ColorRect/PanelContainer stop,
  Containers/TextureRect pass) never leak into a view. Button content can't
  steal the button's press *by construction* — WPF's null-background
  hit-invisibility, as the whole model rather than a special case.
- **Region silencing is a catcher**: input is per-control, so "block
  everything over there" is an invisible `interactive=true` plate drawn over
  the region (the `shield` above). `interactive=false` on a passive container
  is a no-op and draws an advisory lint saying exactly that.
- Legal in `when` blocks like any bool (`when held shield.interactive=true`)
  — applies instantly, restores on release.
- Declared surfaces join the 44-epx touch lint like any shipped control
  (`touch_target=off` accepts a deliberately small one).
- `mouse_filter` is **retired** (a second spelling of the same model): each
  value's build error names its replacement — `stop` → `interactive=true`,
  `ignore`/`pass` → delete (transparency and bubbling are the defaults).
- Wiring `gui_input` (auto-wire or `on:gui_input=`) onto a transparent node
  is provably dead code-behind and warns: mark the node `interactive=true`.
- **Runtime discovery**: in debug builds, a press that lands on gogo-owned
  paint with no one claiming it logs a one-time hint naming the topmost node,
  its file:line and the fix. `gogo.set_input_overlay(root, true)` (or the
  editor preview's "Input" toggle) tints every currently-capturing region —
  a dead surface shows no tint, and a view with ZERO capturing regions shows
  a banner saying so (naming the `interactive=true` fix) instead of nothing.
- Gestures today attach as raw `gui_input` handlers from code-behind; when a
  gogo gesture API ships, attachment will be legal only on interactive nodes
  (a standing obligation, recorded in the host project's D103 entry).

**Background boundary doctrine**: `background=` on `View` is the root plate
and is the ONLY non-Panel home for `background` — it does not generalize to
other containers (no `background` on `Column`/`Row`, ever, citing this).

### Button content sizing

A WPF Button sizes to its content; a Godot BaseButton IGNORES free Control
children in its minimum — and since canvas items don't clip, `Button { Row
{...} }` *draws* perfectly while the button's hit rect stays stylebox-sized:
the view looks right and taps miss. Gogo closes the trap: a button with gogo
children derives its minimum per axis as `max(declared min, content min)`,
where content is the Layers formula — the element-wise max of visible
children's combined minimums plus their positive margins (D242). It stays
live: a child's text, font, epx re-apply or visibility change re-derives
the button.

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
  overflow their slot) and count as zero; a margin that doesn't bind for
  the axis' alignment is a load error and never gets this far. `Panel
  padding=6 { Row {...} }` remains the WPF `Padding` analog.
- Escape hatch caveat: writing the raw `custom_minimum_size` property (or
  setting it from code) on a content-sized button gets overwritten by the
  next re-derive — declare sizes through the vocabulary instead.

### Image

`Image` is a `TextureRect` with WPF Image semantics baked in:

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
  bitmap's ratio from the driving axis (see the sizing triad). The raw
  `stretch_mode=`/`expand_mode=` consts are retired spellings.
- Sizing BOTH axes (`Image icon.png 16x16`, or `width=` + `height=`) makes it
  an exact centered box, like a XAML Image with `Width`/`Height` — containers
  won't stretch it (size flags default to `shrink_center`). Without this, a
  width-stretched box silently scales the glyph up to fit.
- Sizing ONE axis (`min_height=32`) keeps normal container fill on the other.
- Sizing NOTHING (no size word, no `aspect=`): the Image fills the space
  its parent gives it, and the picture fits inside (`Button { Image
  photo.jpg }` — a big picture never grows the Button). In a Column the
  height is not given, so it comes from the picture: the Column's width,
  and the height the picture's shape makes at that width (`Column
  width=300 { Image photo.jpg }` with a 600x400 photo = 300x200; WPF
  Uniform in a vertical StackPanel). Rust app only; in a Row a bare Image
  is still 0 wide — write `height=` + `aspect=natural` there.
- The picture's own size: `size=hug` (= `width=hug, height=hug`; an Image's content is
  its picture; one picture pixel per layout unit, so on a phone that
  draws 3 screen pixels per unit it shows 3x enlarged — a crisp picture
  needs a bigger file). One side `hug` sizes that side only. Only the
  written word counts: an alignment alone does not read the picture.
  Rust app only.
- `loading=lazy` (default `eager`): the picture never holds a frame — it
  decodes on a background thread, starting the first frame that shows it,
  and the node draws empty until the pixels land (requests go in
  declaration order, so the top of the file fills first). Made for
  image-heavy catalogs. The file's existence is still checked at build
  time, and its size is read from the file header up front, so the layout
  does not move when the pixels arrive. Code that must see settled pixels
  (screenshots, reveal gates) polls `gogo.lazy_pending()` until it reads 0.
  Pair it with `texture=` on the node line.
- `source=` also takes an **atlas slice** — `animals[12]`, `toolbar.play`,
  `bubble` (see Directives → Atlases). A slice lays out, fits and
  `loading=lazy`-decodes as its own picture would; the file behind it
  decodes once for every slice.
- `window=[left, top, width, height]` (Rust gogo) shows only that
  rectangle of the picture, in its pixels; the fit maps the rectangle,
  not the whole picture. For pieces the file names more than once,
  prefer an atlas — `window=` on an Image showing a slice is an error
  (G2018). `sampling=pixel` draws the picture unsmoothed (hard pixels
  when scaled up); the default is `smooth`.
- `release=unused` (Rust gogo): the app may free this picture while no
  page names it and decode it again when one does; default: kept for the
  session. See Directives → Atlases for the rule (every declaring line
  must say it).

### Circle

`Circle` is the disc/ring type — an antialiased `_draw` on a plain box. The
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

`Gradient` is the generated-gradient fill: a `TextureRect` with a
`GradientTexture2D` stretched to the node's box. Four fill patterns × three
repeat modes × arbitrary stops × from/to geometry.

```kdl
Gradient veil  fill=radial, stops=[accent, accent/0]        // fade to clear
Gradient sky   angle=90, stops=[bg0, bg1], repeat=mirror    // linear, aimed
Gradient wheel fill=conic, stops=[red, yellow, green, blue, red]
Gradient flash fill=radial, stops=[#FFFFFF6B@0, #FFFFFF6B@0.94, #FFFFFF6B/0@1]
```

- `stops=[...]` — REQUIRED (no hidden default ramp). Entries are any color
  spelling (hex, palette names, `$refs`; the `/opacity` suffix is allowed), optionally
  pinned with **`@position`** (0..1, always after any `/`): no positions =
  even spacing; **if one entry is pinned, all must be** (all-or-none);
  positions never decrease (equal = a hard step); under `repeat=mirror` they
  stop at 0.5 (the second half is the reflection). One stop = error `G2003`
  with a behavior fix — a solid is a `Rectangle`. **Fading out**: the RGB
  under alpha 0 steers the blend even though it never shows on its own
  (interpolation is straight RGBA) — `white/0` fades clean, `#00000000`
  fades through dark gray; bare `transparent` is refused in `stops=`
  because it's #00000000 without saying so. (WPF's default stop color is
  transparent black — a XAML `GradientStop` with no `Color=` fades dark;
  port it as `#00000000` only when the dark edging is the shipped look.)
- `fill=linear|radial|square|conic` (default `linear`). `repeat=
  none|repeat|mirror`.
- **Aiming**: `angle=` (linear only; the CSS compass — 0 = to top, 90 = to
  right) or `from=`/`to=` box points (`0.5x0.5` = center; 0..1 of the box) —
  spelling both is an error. Defaults: linear runs left→right; the round
  fills center in the box and reach the right edge.
- **Stops stay live**: `stop_1_color`, `stop_1_offset`, ... are real
  per-stop properties — whens and anims drive them like anything else
  (`flash.stop_1_color.a from=0, to=0.42`). Numbered from **1** in
  declaration order: `stop_1` is the FIRST stop in `stops=[...]`, not an
  array index — code calling `Set(...)` from GDScript/C# habits misreads
  this silently. An animated offset that crosses its neighbour re-indexes
  (the engine keeps stops sorted).
- Each `Gradient` owns its texture; `modulate`/`opacity` stay free for
  whole-node fades. The old `Radial` type retired into this:
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
choice size value=medium {           // same set, laid out in columns
    Row {
        Column { Radio small text="Small";  Radio medium text="Medium" }
        Column { Radio large text="Large" }
    }
}
```

- `Radio` is a CheckBox joined to the scope's ButtonGroup (grouping is what
  draws radio visuals and enforces exclusivity). Radios NEED names — the
  name is the value the choice reports. `Check` is the independent on/off
  and never joins a group (inside a choice it errors, routing to Radio).
- **Templated radios** (D142): a Radio with a BODY draws none of the native
  visuals — the body is the whole look. The node builds as a toggle Button
  in the same group with its chrome STRIPPED (D144): the normal/hover/
  pressed plates are a transparent slate, so nothing shows but the body
  (`background=`/styles recolor the slate; the FOCUS ring stays with the
  D129 machinery, so keyboard focus still indicates). `selected` + whens
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
  is the independent twin — the same blank toggle Button and
  `selected`/whens, the checkbox reader role, NO group, and `change(bool)`
  wiring like the native form.

  A repeated look factors into a **host widget** (`widget RadioLook
  for=Radio` — see Widgets § Host widgets, D276): the widget is the whole
  body, and its widget-scope `when selected` binds to the hosting Radio.
- `value=<name>` pre-presses that Radio (optional — omit for none-selected).
- **Wiring**: a named choice auto-wires `On<Name>Change(radio_name)`
  (`choice paintMode ...` → `OnPaintModeChange("medium")`);
  `on:change=<handler>` / `on:change=emit(HostSignal)` are the explicit
  forms — the radio's name is passed as the argument. Items refuse
  `on:change` (the value is "which one" and lives on the group; rows
  expose `when selected`).
- **changed means CHANGED** (D142): one report per genuine transition, both
  radio kinds. Re-tapping the current pick keeps it pressed (the group's
  no-unpress repair) and does NOT re-report; a programmatic
  `button_pressed = true` reports like a tap (WPF's IsChecked=true fires
  Checked). `set_pressed_no_signal` seats silently — the choice never sees
  it, and it also skips the `when selected` restyle (push-based since E1) —
  so prefer `button_pressed` when the report should fire and
  `gogo.set_selected` when it shouldn't.
- Exclusivity has a **lexical home**: radios in disjoint subtrees can't
  share a group — if two far-apart controls must be exclusive, restructure
  so one `choice` encloses both (a feature: the reader sees the whole set).
  Scopes don't nest.

## Repeat (lists from data)

`repeat` renders N children from a code-supplied collection, reconciled by
key (design: `docs/design/2026-07-15-spec-repeat-and-list.md`; codes G90xx
build / G91xx runtime refusals).

```kdl
Column spacing=8 {
    Label text="Today"
    repeat tasks as $task by $task.id {     // as optional: default $item
        empty { Label text="Nothing due today" }
        TaskRow title=$task.title, done=$task.done
    }
    Label text="end"
}
```

```gdscript
set_items("tasks", tasks)              # in a GogoView/GogoPanelView host
Gogo.set_items(view, "tasks", tasks)   # the underlying API, any caller
# assignment IS the change event — re-assign to update
```

- **The line**: `repeat <name> [as $binding] by <key-path> { ... }` — the
  name is required (it's the `set_items` address, unique per view); clauses
  are keyword-anchored and COMMA-FREE (the arrow-endpoint precedent), `as`
  before `by`; nothing else is legal on the line (G9009 — looks belong on
  the parent or the template). Omitted `as` is defined sugar for `as $item`;
  since `by` must key the binding (`by $item.id`, nested fields fine, bare
  `by $item` = the item IS its key — G9003 otherwise), every repeat line
  displays its item variable. A binding that would shadow a theme value or
  widget param in scope errors with the rename fix-it (G9004).
- **Erased structure**: no runtime node — a hidden zero-size marker anchors
  the spot and items parent FLAT into the repeat's parent, so a `Column`'s
  `spacing` spaces them and static siblings mix freely (source order = tree
  order). Multiple repeats per container are fine, each spanning from its
  own marker.
- **`$binding.field` reads the item's data** — a STATIC value re-applied at
  each assignment (no `live()` inside repeat; bare-`$`-is-static holds).
  Dots on any other `$ref` are an error (G9008). Fields must hold what the
  property can take (String, number, bool, Color) — structures don't bind
  whole.
- **Reconcile by key** on every `set_items`: kept keys with equal data keep
  their NODES (focus, states, anim positions survive; Dictionaries compare
  by value against a frozen copy, Objects are assumed changed), changed
  data rebuilds its item, absent keys free (their epx/locale re-apply
  bindings scrubbed), order follows the collection. The last assignment
  re-applies across theme switches and hot reloads.
- **Refusals, never halts**: duplicate key, missing `by` field, non-String/
  int or mixed key types, `null`/non-Array, and data-dependent template
  errors each refuse the WHOLE assignment with a rich printerr block
  (G9101/G9102/G9103) — the last good items stay live. It's the "last good
  tree stays live" philosophy applied to data; `[]` clears legitimately.
- **`empty { ... }`** — zero-items content (initial state included):
  **required**, exactly one, FIRST in the body, nothing on its line
  (G9007). Required even with nothing to show — a literal `empty { }`
  says "considered: show nothing"; every list answers the zero-items
  question visibly, which is also what keeps the block discoverable.
  Replaces only the repeat's own expansion; statics stay.
- **Inside the body**: names are per-item local (the widget-body rule —
  never in the view's named map); `on:` handlers and auto-wires get the
  item's key appended as their TRAILING argument (`OnRowActivate(key)` for
  an argless event — Godot `Callable.bind` shape).
- **Fences (v1, both for addressability)**: no nesting (G9005) and no
  repeats inside widget/control bodies (G9006) — fill content is fine (a
  widget's slot filled with a repeat at the instance site works, names are
  view-scope there). Selection/keyboard/activation belong to the `List`
  control (next section).
- **Editor preview**: the static preview truthfully shows the ZERO-ITEMS
  state (the `empty` block). To design the items state: (a) the **Test
  data** dropdown above the preview (next to Arrangement) lists every
  repeat as `name: 0 (off) / 3 / 10` — synthesized placeholders (field
  values guessed from the prop that reads them), editor-only, re-asserted
  across preview rebuilds; (b) for real data, run the scene and edit the
  .gogo live — hot reload re-applies the last `set_items` collection
  across every rebuild.

## List (selection over items)

`List` is the one control that owns selection (design: the repeat spec §5,
D118; self-scrolling + `ListItem` rows: D121). Its body is statics plus
**exactly one** direct child `repeat` (G9010) whose row root is a
**`ListItem`** (G9011), stacked top-to-bottom (`spacing=` like a Column)
inside the List's own scroll viewport. Zero new state names: the chosen
row's `ListItem` holds the universal **`selected`**, the keyboard cursor
is engine focus (the **`focused`** state), a press in flight is
**`held`**. Gogo-implemented always —
`render=native` errors, like Toggle.

```kdl
List picker select=many, spacing=4 {
    Label .SectionHeader text="Today"
    empty { Label .Muted text="Nothing due today" }
    repeat tasks by $item.id {
        ListItem .TaskLook { TaskRow title=$item.title }
    }
}
```

```gdscript
picker.selected = ["t1", "t9"]   # live property; a key or null under one
func OnPickerChange(selected) -> void: ...      # the selection changed
func OnPickerActivate(key, item) -> void: ...   # an item was invoked
```

- **It scrolls itself** (D121 — a list that doesn't scroll is a surprise,
  and virtualization needs the List to own its viewport). The internal
  scroll is a GogoScroll: WP touch pan/fling/spring edges, native wheel and
  scrollbar, and follow-focus (the cursor and taps stay in view). Statics
  scroll with the rows. Never wrap a List in a `Scroll`. **`layout`**
  (D128) picks the items panel — a closed set, each owning its trio:
  `Column` (default, no block) scrolls vertically with an up/down cursor;
  `Row` is the strip (horizontal pan, left/right cursor, `width=hug` +
  `max_width=` transposed); `Flow`/`Tiles` wrap (vertical scroll, arrows
  by the engine's geometric navigation). The block holds exactly the one
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
  itself twice over: at BUILD time where the chain proves it (G9015 —
  `valign=`-anchored Column chains and `Scroll` pages; Rows/Layers/Grid/
  widget seams stay quiet, conservative by design), and at RUNTIME when
  items are present but less than one row's worth of room remains beyond
  the fixed content (the half-starved nudge). A content-sized DEFAULT was
  rejected: Godot minimums are hard floors with no "available size"
  input, so it would overrule authored `height=` and un-scroll any list
  taller than its screen.
- **`ListItem` is the row root, always** (G9011; anywhere else: G9013).
  It is the unit selection (`selected`), the focus cursor and taps attach
  to, and a drawn box like `Panel` (`background=`/`padding=`/`corner_radius=`
  /`border-*=`, blank until told) — so the row's looks live ON it. One
  child fills it; multi-part rows lay out with a `Row`/`Column` inside.
  Being gogo-owned, it draws **no engine focus chrome** — `when focused`
  is the only cursor styling.
- **Default looks** (D122, doctrine unchanged by the D181 respelling): a
  bare row shows selection, the cursor and the
  press OUT OF THE BOX — `selected` fills the row with the theme's list-
  selection color (the ItemList selection box; an app theme restyles it,
  resolved
  live at flip time), `focused` with a quarter-strength echo, `held` with
  a half-strength fill while the press is held (the ladder orders them:
  held > selected > focused — an unselected row shades up, a selected one
  dims). Button doctrine: the theme is the base look, `when` overrides it —
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
  (G9012).
- **Gestures** (user call, 2026-07-15): tap or Enter/Space on an item
  ALWAYS fires `activate(key, item)`; selection is the mode's side effect
  — `one` selects it (tapping the selected item never deselects — radio
  doctrine, `null` comes from code), `many` toggles it, `off` nothing.
  `change(selected)` fires only when `selected` actually changed
  (changed means CHANGED — programmatic writes report the same way, a
  same-value write is no event). The
  cursor follows the tap; arrows walk the items (statics and `empty`
  content are skipped — never selectable, never navigated). A `Button`
  inside a row takes its own presses — tapping it fires the button, not
  the row (WP semantics).
- **`selected`** — a key or `null` under `one`; an Array of keys under
  `many` (`[]` = none, read-back order: present keys in collection order,
  then inert keys in the order they entered). **Inert keys** (no present
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
- **Reconcile keeps selection by key**: reorders move it, changed-data
  rebuilds re-check the fresh node, `[]` leaves the selection inert, and
  hot reload carries it — except edits that invalidate identity (a changed
  `by` path or `select=` mode), which reset honestly to `null`/`[]`.

## States

**The UI is a function of its states.** While a state holds, its
`when` responses overlay the declarations; when it ends, the base look
restores. One concept at three scopes — *responses live at the state's
scope, and dotted names descend from that scope*:

| scope | state | who flips it | overlays |
|-------|-----------|--------------|----------|
| app | `theme dark` variant | `set_theme_variant` | `$values` |
| app | `when screen.landscapeSmall` class | `set_screen_class` (the host's resize handler) | props + placement — closed five-class set, composes under arrangements |
| view | `arrangement expanded` | `set_arrangement` / `go()` / `toggle()` | props + placement — **exclusive**, one active |
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
  `selected` — always available, no declaration (respelled in the D181
  input-vocabulary round; the retired `pressed`/`hover`/`focus`/`checked`
  error with one-keystroke redirects).
  `hovered`/`focused`/`focus_within`/`disabled`/`selected` are
  **universal** — legal on any node, dormant until a provider flips them.
  `disabled=true` on a button blocks input as always; on any other node
  it's a styling flag (greys via `when disabled`; pair with
  `interactive=false` if it must also stop answering) — the flag lives in
  shared storage code-behind reaches through `GogoStates.set_disabled`, and
  a Slider/Input spells it as not-`editable`. `selected` (universal since
  D118, as `checked`) is "this one is chosen": a toggle-mode button holds
  its own, a
  `choice`'s Radios carry it, a `List` flips it on the chosen item's root
  — code-behind reaches it through `GogoStates.set_selected`/`is_selected`.
  `held` alone still needs a press surface — the Button family, a `List`
  row's `ListItem` (D121), the native `Slider` (the grabber's drag
  brackets it), or a composite declaring `state_builtin held` (the root's
  gesture tracking maintains it, D181) — checked where the style is
  APPLIED, and the
  load-time error lists what's available. `selected`/`disabled` restyle by
  PUSH from their write paths (`GogoStates.set_selected`/`set_disabled` —
  a toggle button's own clicks and `button_pressed =` writes arrive through
  `toggled`). Raw engine writes that bypass both (`set_pressed_no_signal`,
  `disabled = true` from code) are caught by a shared 0.1 s sweep instead
  of the old always-on per-control poll: the restyle lands a beat late and
  a one-time `gogo:` warning names the instant spelling (the write paths;
  C#: the `GogoSetSelected`/`GogoSetDisabled` extensions — the prefix
  dodges the bindings' native `SetDisabled`) — so a raw write is never
  silently wrong, just loud and slightly slow. The warning also shows as
  an on-screen amber banner (tap to dismiss), RELEASE builds included — a
  raw writer in a device-only path has no console to confess to, so end
  users see and report it instead; `gogo.set_raw_write_banner(false)` at
  startup keeps it console-only.
- **`disabled` cascades** (D133 — the WPF IsEnabled law): effective = own
  OR any ancestor's own, no opt-out; re-enabling the ancestor restores each
  descendant's OWN flag, dynamic children inherit on entry, and `when
  disabled` fires on the effective value, so gray-out styling flows with
  it. The only state that flows down (`focus_within` is the only one that
  looks up). Code-behind: `gogo.set_disabled(node, true)` on the group
  root; `explain()` prints `disabled: yes (inherited from '...')`.
- `hovered` answers a **real mouse only** — a finger isn't a pointer. Touch
  screens emulate a mouse that parks at the last touch point, which would
  strand the hover look on whatever the finger lifted over (drag-scroll a
  list and one row stays lit); gogo ignores finger-driven enters instead,
  matching every touch platform. Hover styles are safe to author as
  pointer-device niceties — phones simply never show them. **Hover follows
  capture**: a mouse press pins the hovered look through a mid-drag slip
  off the control; the release re-checks the pointer and settles it.
- `focused` is **visible-focus** (D129, semantics unchanged by the D181
  respelling) — focused AND an indicator is
  warranted: focus arrived by keyboard/gamepad, or a key was pressed while
  focused. A tap/click parks engine focus **silently** (Godot's click-focus
  would otherwise leave every tapped control indicating), and a later
  pointer press turns the indicator off. Text fields are the caret
  exception — a clicked `Input` always indicates. There is no raw-focus
  spelling; code-behind reads `has_focus()` for the fact. Buttons ship a
  default: the engine's always-on focus ring is retired and re-shown as the
  injected default `when focused` response (the theme's own ring,
  key-driven
  only); authoring `when focused` replaces it — same replacement doctrine
  as the `ListItem` defaults.
- `focus_within` (D129) holds while the focus is on the node **or anywhere
  inside it** (CSS `:focus_within`, WPF `IsKeyboardFocusWithin`) — the
  container highlight for controller/keyboard UIs: the settings row lights
  while its inner slider is focused, the active pane brightens its border.
  Visible-focus flavored like `focused` (and the one non-participle
  spelling: it names a relation, not a condition of the node).
- `when` props layer over the base look and restore when the state ends.
  Merge order, most local wins: style base → style when → node props → node
  when override. A plain prop on the node never switches a style's response
  off; opt a node out by writing the same state on it (`when held opacity=1`
  over a style's `when held opacity=0.5`). Priority when several states hold:
  arrangements < focus_within < focused < hovered < selected < held <
  disabled.
- Friendly response props: `opacity`, `background`, `font_color`; dotted
  paths (`modulate.a`) and plain engine props (a `Rectangle`'s or `Circle`'s
  own `color`, a `Gradient`'s `stop_1_color`) also work. Floats and
  colors tween; bools/enums apply instantly. A `background` or `font_color`
  response on a control that never declared one of its own works against the
  THEME: the tween starts from the theme's resting value, the generated
  background box copies the theme's shape (corner radius, borders, content
  margins — only the color moves), and the state ending hands the
  channel back to the theme — the default look, including its per-state
  styling, survives the round trip.
- **Motion words are responses too**: `when held scale=0.95` holds the
  scale while the press is held and hands it back on release, timed by the same
  `transition` rules as every other response — the WP press-squish in one
  line. `scale=` is uniform, pops from the center, and scales the whole
  picture — glyphs, corner radii and borders shrink with the box, and the
  layout around it never moves; `scale.x=`/`scale.y=`
  take one axis, `translate.x=`/`translate.y=` are epx offsets, `rotate=` is
  degrees. Gogo arms the offset transform while a state holds one off
  identity and releases it when it lands back. One writer per property: a
  transition `anim=` in the same scope that drives a property a `when` also
  holds is a load-time error — pick the held value or the choreography, not
  both. (The base look declares no transform — static `scale=` stays an
  error; a look that never changes is sized with `width`/`height`.)
- **Child targets**: a dotted key whose head is a NODE NAME drives that node
  instead of the styled control — `when held flash.visible=true` shows a
  named child while held (the WPF template-part pattern). Names resolve
  widget-local first, so a widget used six times flips each instance's own
  `flash`. The friendly vocabulary applies to the target (`flash.opacity=1`);
  values are numbers (epx-aware), booleans and colors — enum names stay with
  the plain `prop=` form. A control property of the same name wins over a
  node name; an unknown name reports when the view loads and the prop is
  dropped.
- A style may also respond to a **declared arrangement** — `when expanded
  visible=true` inside a style follows the *view's* flip on every wearer,
  with the view's transition timing (one flip, one feel).

### Arrangements

```kdl
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
`gogo.set_arrangement(root, "expanded")` (`GogoView.SetArrangement` /
awaitable `SetArrangementAsync` in C#) or declaratively with
`on:activate=go(expanded)` — and a disclosure button (expand on press,
collapse on the next) is `on:activate=toggle(expanded)`: it goes home when
its arrangement is active, to the arrangement from anywhere else. Style the
button itself through the arrangement
(`when expanded { moreButton.rotate = 180 }`) rather than button toggle
MODE — `toggle=true` plus `toggle(expanded)` is two separate on/off states
and draws an advisory lint.

**"Pose"** in these docs is defined shorthand, not a keyword: the set of
values one `when` condition holds — an arrangement's, a screen class's, or
a composed pair's. Wherever a rule says "pose", it applies to all three
condition kinds alike; rules that differ between them name the kind.

For a pose that GROWS the layout — a panel sliding open to include more
content — give the appearing node `reveal_y=0` and let the pose reveal it
(`when expanded { hello.reveal_y = 1 }`): the space fraction is a float, so
the flip slides smoothly under the transition rule, siblings and the border
reflowing with it (see the `reveal-*` props row and the COOKBOOK
smooth-slide recipe).

- Responses are top-level `when <name>` blocks: dotted `node.prop=value`
  pairs. **Placement keys** (`margin_left/-top/-right/-bottom`, `width`,
  `height`, `min_width`, `min_height`) overlay the target's placement and
  compile per arrangement at load time; everything else goes through the
  normal state machinery.
- The flip tweens edge offsets and style props with the matching
  `transition` rule.
- **Re-assertable by construction**: on every `units_changed` and hot reload
  the ACTIVE arrangement's pose re-applies — no two-writers, no teleporting
  panels after a rotation. Hot reload keeps the active arrangement (it's
  user state, like slider values).
- **A posed size behaves exactly as if declared** (D249), for as long as
  the block applies — including the minimum the box reports, so a pose CAN
  shrink a node below its declared size. Composition matches declarations:
  a posed `width` still loses to a declared `min_width` floor (the floor
  clamps every posed value — WPF's MinWidth-over-Width order), a posed
  `min_width`/`min_height` replaces the declared floor, and content still
  sets a button's non-exact axis above any floor. On a timed flip the
  edges tween; the minimum drops up front and raises when the tween lands,
  so motion is smooth both ways.
- Reserved names: `base`, `any`, the built-in state names.
  Arrangements rank below every built-in state when both touch a
  property.

### Screen classes and orientation (`when screen.*`)

```kdl
when screen.landscapeSmall {
    middlePanel.width = 280
    middlePanel.margin_left = 0
    hintLabel.opacity = 0.5
}
when screen.landscape {          // the ORIENTATION axis (D265)
    bodyStack.visible = false
    wideRow.visible = true
}
```

Two more independent pose axes, both named by the host app — never by
user actions. The size CLASS (`gogo.set_screen_class("landscapeSmall")`
from its resize handler) says how much room there is; the ORIENTATION
(`gogo.set_screen_orientation("landscape")`) says which way it runs
(landscape = wider than tall). Both vocabularies are closed: classes
`screen.portraitSmall` / `screen.portraitMedium` / `screen.portraitLarge`
/ `screen.landscapeSmall` / `screen.lotsOfSpace`; orientations
`screen.portrait` / `screen.landscape` (an unknown name refuses at load,
listing both). No declaration line — they exist whether or not a file
poses for them; with nothing set (or none posed) the declarations stand.
Key the axis the design actually means: "is it sideways?" is the
orientation — enumerating the landscape-ish classes instead silently
catches big PORTRAIT tablets, because `lotsOfSpace` covers both shapes.

- **One vocabulary per target (G3005)** — a class and an orientation hold
  at the same time, so one style prop, one node's placement, or one seat
  move posed from BOTH would be decided by a rank the file never states:
  load error. Say the intersection as one condition instead —
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
- **Instant changes may re-anchor and re-seat** (G3003/G3004): alignment
  keys and seat moves are legal in any `when` whose change nothing can
  animate — screen whens always, and arrangement whens (plain or
  composed) exactly while NO `transition` rule times the arrangement (no
  rule naming it, no bare rule, no `any` endpoint). An anchor is one
  alignment or the other, and a node is in one parent or the other —
  neither has a partway for an animation to show, so a timed arrangement
  carrying either is a load error pointing at both lines; adding a
  `transition` rule later turns an existing alignment key or seat move
  into that same error.
- **Seat moves** — the one response that REPARENTS: `seat sideSeat`
  declares an empty socket in the tree (lowercase like `slot`/`gap` —
  sockets, not painted controls), and `x.seat = sideSeat` in a `when`
  moves the declared node into it. The seat hugs to zero alone and
  to the occupant's minimum when occupied — a `width=fill` or exact-width
  occupant measures wrap-text ORDINARY on both axes (wrapped; fill asks
  the socket for size, a number IS the box, and a posed width counts as
  exact while its block applies), a hug occupant UNWRAPPED like every hug
  axis (D254). A hug occupant whose single-line wrap-text is wider than
  the window can never fit any pane (the width-feedback trap) and warns
  at runtime — G1005 names the fix: `width=fill` on the occupant.
  Dropping every condition
  returns the node to its declared home, parent and position, and
  `x.seat = home` says the same thing AS a move — for the more specific
  block that wants the node back where a less specific one seated it
  (`home` is reserved; declaring `seat home` is a load error). Contested
  seats follow the rule above: more conditions win, equal count → the
  later line in the file. Seats and resting homes must both be containers;
  one occupant per seat under conditions that can hold together (two
  different arrangements are exclusive, so their claims may share a seat;
  an arrangement claim and a screen claim can co-hold, so theirs may
  not; a bare class claim and a bare orientation claim on one occupant
  refuse outright — G3005). Full rules on the seat card.
- **Flips are instant** by design — a rotation is the OS snapping the
  window, and a tween here would show panels sliding to their landscape
  seats after the screen already turned. `transition` rules don't apply
  (and can't name a class as an endpoint).
- Placement keys are the arrangement set (`margin_*`, `width`, `height`,
  `min_width`, `min_height`) PLUS `halign`/`valign` — an alignment key
  RE-DECLARES that axis: the base's pins and size mode drop and the pose's
  own margins/size stand alone (`middlePanel.halign=left,
  middlePanel.width=280, middlePanel.margin_left=60` flips a
  stretch-with-margins panel into a left-docked one). Alignment keys need
  an instant change — see the G3003/G3004 bullet above. Everything else
  goes through the state machinery. Container rule: a pose may RESIZE a
  Container-seated child (numeric `width`/`height`/`min_width`/
  `min_height` behave exactly as if declared while the condition applies,
  and a posed `width`/`height` makes a fill child stop taking the
  leftover; instant, nothing to tween) but never re-PIN one —
  margins/alignment on container children are load errors.
- The active class re-asserts on every `units_changed` and hot reload, and
  a view built AFTER the class was named starts in it.
- View-level `when screen.*` blocks only — no style-scoped screen
  responses (yet).

### Transitions

How changes between standing truths feel — a directive line, never a block:

```kdl
transition duration=0.3, easing=cubic_out            // the default: any -> any
transition base -> expanded duration=0.25, easing=cubic_out
transition expanded -> base duration=0.2,  easing=cubic_in
transition any -> settings duration=0.25             // one-sided wildcard
```

- Payload: `duration=`, `easing=` (+ params), `anim=<name>` (choreography
  played with the flip; the state ending cancels it and tweens the
  displaced properties back).
- **Specificity, most specific wins**: exact pair → entering one-sided
  (`any -> X`) → leaving one-sided (`X -> any`) → bare default. The same
  endpoints declared twice in one scope is a load-time error, not a silent
  pick. Going home (`X -> base`) with no rule of its own **mirrors the rule
  that brought you out** (timing only, never its anim). Untimed = snap.
- **At control scope** (inside a style/node block) the endpoints are built-in
  states, and exactly one side is `any` (`transition any -> held
  duration=0.05` enters, `transition held -> any duration=0.2` leaves) —
  built-ins stack, so exact pairs between two of them have no single flip to
  time.
- **A node's share of a pose flip** (2026-08-16): a node block may carry an
  exact POSE pair (`base` counts as a pose) — it times THAT node's tween in
  the flip, overriding the view-level rule; `duration=0` snaps the node to
  its new seat while the rest glides. Timing only — `anim=` and one-sided
  arrangement rules stay at the view's top level.

  ```kdl
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

  ```kdl
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
  sets it, is a load error naming both lines (G3008).

## Animations

One model (D136): *an anim is a CHAIN of steps, and every step says when it
runs.* A step is a property line, a `group { }`, a `play <anim>`, an
`each ... { }` or a `delay=` pause; every step after the first carries a
connective — `and` (runs WITH the previous step) or `then` (runs after it
and everything `and`-joined to it) — or an `after=<group>` reference
instead. Sequencing is always written, never defaulted.

```kdl
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
    each .MenuButton stagger_by=0.1 {            // every wearer, tree order
        translate.y from=40, to=0, easing=back_out
        and opacity from=0, to=1
        then scale.x to=1.05, duration=0.08, autoreverse=true
    }
}
```

- **Property lines** are `target.property[.component]` paths — the target
  is a named node (or `self` inside widget/style scope); components are
  `.x`/`.y` (Vector2) or `.r/.g/.b/.a` (Color). Step words on a line:
  `duration=`, `easing=`, `autoreverse=`, and the signed `offset=` shift.
  Inside an `each`, lines are bare (the each names the target; prefixing
  them there is an error).
- **Prop lanes** *(FanRust, 2026-09-02 — where the addon offered
  `.r/.g/.b/.a` color components, the Rust runtime replaces them with
  whole-value lanes plus `.a`)*: any word a `transition` rule can move is
  an anim target — `b.background to=#FF3B30FF`, `card.corner_radius
  from=4, to=12`, `g.stop_1_color.a to=0.42` (the live stop family
  included). `.a` on any color word moves alpha alone; the hue keeps
  following its ladder, so a `when` can restyle the color mid-fade. One
  ownership law with the motion channels: a playing anim owns its lane —
  `when`/code writes yield, a `transition` on an owned lane spawns no
  tween, a newer player takes over, `stop` restores, a finished anim
  holds its end pose. `key` children stay numeric — a color lane spells
  its segments as chained `then` steps. Colors mix under gogo's ONE
  mixing law: premultiplied Oklab — gradient stops, `transition` tweens
  and anim lanes all mix through the same code, so a fade to transparent
  never darkens and cross-hue midpoints never go muddy (the addon lerped
  per byte — a recorded divergence).
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
- **`after=<group>`** starts the step when that group's chain completes —
  the escape hatch for dependencies that CROSS the flow. Names are
  anim-flat: forward references are legal, wrappers are reachable-into;
  unknown names and cycles refuse at load. `offset=` shifts any placed
  start (negative overlaps the anchor's tail; the final start clamps at 0).
- **`delay=N`** is a PAUSE — its own step in the chain, always positive
  (starting a step early is `offset=`, signed). The old delay ATTRIBUTE is
  retired.
- **`play <anim>`** runs another anim as one step, inlined at bake — its
  `speed=` folds in, its internal names stay private, and a
  `repeat=forever` anim never completes so it can't be a step.
- **`each <node|.Style> [stagger_by=N]`** is a step whose chain every
  wearer copies (tree order), shifted by `stagger_by`. With `stagger_by` >
  0 its length only exists at play time — nothing may sequence after it
  (`and` beside it is fine: that shares its start).
- **The capture rule**: `to=` alone starts from the LIVE value when the
  step begins (WPF "To" animations — perfect for interruptible fades).
  Adding `from=` pins the start **and holds it from play start**, so
  staggered entrances never flash their base. Two from-pinned steps on one
  property = load-time error.
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
- Numbers on length properties are epx (`px` opts out); values may be colors.

### Motion — layout is declared, motion is transformed

Anims move a **visual-only channel** (Godot 4.7 offset transforms) — layout
never budges, containers can't stomp it, and hover/press state survives
(`offset_transform_visual_only` stays true). The vocabulary:

| Word | Drives | Notes |
|------|--------|-------|
| `translate.x` / `translate.y` | `offset_transform_position` | epx |
| `scale` (uniform) / `scale.x`/`.y` | `offset_transform_scale` | centered — the engine pivot-ratio default is (0.5, 0.5), the WP "pop from the center" |
| `rotate` | `offset_transform_rotation` | degrees |
| `opacity` | `modulate.a` | the one dual citizen — also a static style prop |

Two things write this channel: anim steps (transient flourishes) and `when`
responses (standing values held while a state holds) — one property gets
ONE of them per scope, enforced at load. Gogo owns
`offset_transform_enabled`: armed while motion is live (an anim running, a
state holding a value off identity), released when the node lands back
**at identity** or **hidden** (reset clean for the next entrance). A
parked-enabled transform renders a black backing rectangle (a 4.7
compositing artifact) — that is why raw `offset_transform_*` props are
fenced off everywhere in gogo, and why an anim should end at identity or
hide its target. Because motion works on any node — container children
included — never restructure a tree to animate it.

**Layout geometry is not animatable** (`position`, `offset_*`, `size`,
`rotation`, minimums, `width`/`height` — load-time errors point back here):
the layout engine re-writes those on rotate/reload, so an anim would fight
it. Transient pulse → `scale.x`/`scale.y`; changing and staying →
`arrangement` + `when` + `transition`. Anims are transient and end where
they began.

### Easing

WPF-compatible math (Godot's TransitionType curves are never used). Names take
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

```kdl
line.size.y to=52, duration=0.5, easing=bounce_out, bounces=1, bounciness=3
```

### Playing

```gdscript
var player := gogo.play(view_root, "appear")   # restarts if already running
await player.finished
gogo.stop(view_root, "appear")                 # WPF Stop: back to what the file says without the anim
gogo.is_playing(view_root, "appear")
```

**What `stop` puts back** — every property the anim wrote returns to what
the file says without the anim, read where the play found it: the
declared value; the `when` value holding at the time; or a FINISHED
anim's end pose the play started from (a standing value the author chose
— it stays). `stop` does not emit `finished`. Example:

```kdl
anim pulse repeat=forever {
    dot.scale from=1, to=1.2, duration=0.4, autoreverse=true
}
anim vanish {
    dot.opacity to=0, duration=0.2
}
style DotLook {
    when held scale=0.9
}
Button dotButton .DotLook width=40, height=40 {
    Circle dot width=24, height=24, color=accent, halign=center, valign=center
}
```

```gdscript
gogo.play(view_root, "pulse")
gogo.stop(view_root, "pulse")    # dot.scale = 1: the declared value
# with dotButton held:
gogo.play(view_root, "pulse")
gogo.stop(view_root, "pulse")    # dot.scale = 0.9: the `when held` value is what the file says now
# a finished anim's end pose counts as a standing value:
var v := gogo.play(view_root, "vanish")
await v.finished                 # dot.opacity = 0, and stays 0
gogo.play(view_root, "vanish")
gogo.stop(view_root, "vanish")   # dot.opacity = 0 — the replay started from the held end pose, not from 1
```

(The Rust port reads the ladder again at stop time, so a `when` that
flipped while the anim ran counts too; the addon returns the value the
play captured at its start — the same answer unless a state flipped
mid-anim.)

`GogoView` wraps these as `play_anim` / `stop_anim` / `is_anim_playing`
(C# host wrappers: `PlayAnim` / `PlayAnimAsync` / `StopAnim` / `IsAnimPlaying`;
arrangements: `SetArrangement` / `SetArrangementAsync` / `CurrentArrangement`).

`gogo.set_motion_speed(multiplier)` scales ALL motion — anims and state
tweens. `1.0` default; fractions are slow motion (feel tuning); `INF` is
**instant-complete, not skip**: final values apply, `finished` fires, show/
hide sequencing and the transform disarm still run, and `repeat=forever`
anims hold their base pose. A host-fed environment fact (OS reduced-motion,
test determinism) — deliberately not a theme prop. `<= 0` is rejected.

## Translations

Any string a control DISPLAYS can carry a translation term:

```
Label tip text="new stroke"/newStroke, font_size=$fontAppBar
```

- **The literal is the English source text** — the design-time value, the
  fallback for any locale that lacks the key, and the string translators
  translate. Your `.gogo` files ARE the English table; no separate English
  strings file exists to drift out of date.
- **The key indexes host-fed locale tables**: register with
  `gogo.set_translations("de", {"newStroke": "neuer Strich", ...})` (replaces
  that locale's table; merge layered sources host-side), then flip with
  `gogo.set_locale("de")` — every keyed string on every live view updates IN
  PLACE, instantly; new builds pick the active locale up at build.
  `set_locale("")` returns to the literals.
- **gogo owns the re-apply** — one writer per property. Godot's built-in
  auto-translate (TranslationServer) is a hidden second writer on `text` and
  is not used; the resolver sets the final string and re-runs the same
  appliers on a locale flip.
- **One key, one English string** (error `G8001`): the same key may repeat
  with the SAME literal (shared strings — that's how views share a term),
  but a different literal under an existing key fails the build, pointing at
  both places. Editing a literal in its own file is always legal.
- Keys work wherever the string lands: node props, style props, widget
  params (`Chip label="keep changes"/keepChanges`), registered custom
  properties. They are rejected where nothing is displayed — theme `$values`
  (key the literal where it shows), resource paths, `uses` paths.
- **Code-side strings** (toasts, formatted messages):
  `gogo.translate("PaintingSaved", "Your painting has been saved")` — same
  key + English-fallback rule as `/key`. Format params stay host-side:
  `String.Format(translate(key, en), album)`.
- **Text that stays English on purpose** (Rust app): `"test only"/_` — the
  `_` is no key, so the text is never translated and never counts as a
  key (two `/_` strings may differ; G8001 does not apply). For a whole
  file, write `allow untranslated` as a line at the top (dev pages, a
  release-notes page): the check below skips every string written in that
  file. Each file answers for its own text — a widget's body follows the
  widget's file, an instance line's `caption="…"` the page's.
- **The missing-key check** (Rust app): a build that ships in several
  languages sets `CompileOptions::require_keys`, and every displayed string
  with no key is an error (`G8002`) naming the file and line. Not checked:
  paths and picture addresses (`res://`, `collage://before`), `areas=`
  rows, and text with no letter in it (`"100%"`, `"1/1"`). `allow` takes
  exactly `untranslated`, at the top of a file (`G8003`).

```
allow untranslated              // this whole file stays English

View devPage {
    Label text="probe"                      // fine: the file allows it
}
```

```
View settings {
    Label text="Privacy"/settingsPrivacy    // translated
    Label text="Leon Shaw"/_                // a name: English on purpose
    Label text="100%"                       // no letters: never checked
}
```

## Signals & triggers

`on:` speaks two visible registers (D181): **bare stems are gogo events**
— `activate`, `change`, `edit`, plus the visibility pair `show`/`hide` —
and **snake_case past-tense names are raw Godot signals** (`on:drag_ended`,
`on:resized`, `on:mouse_entered`; the raw register stays open for
anything gogo has no word for).

| event | fires | payload |
|---|---|---|
| `activate` | the control's OWN trigger gesture completed — Button: a completed tap or key/gamepad accept; List: tap or Enter/Space on a row. Never fires disabled | — (List: key + item) |
| `change` | the committed value is now different — once per commit, genuine transitions only: re-pick repairs (no re-report), programmatic writes report like interaction, a same-value write is no event (which breaks handler→write→event loops) | Check/Toggle `bool`, Slider `float`, Input `String`, `choice` the picked Radio's name, List the key / Array of keys / `null` |
| `edit` | a live, UNCOMMITTED value moved — Slider per drag tick, Input per keystroke; discrete controls have no edit | the live value |

Per control: **Slider** — `edit` streams while dragging (`held`),
`change` commits on release; a keyboard/gamepad step or programmatic
write commits directly (`change` only, no `edit`). **Input** — `edit`
per keystroke; `change` on Enter or focus loss; programmatic `text=` is
synthesized (Godot emits nothing on set). **Toggle** — `change(bool)` on
settle, only if the settled side differs from the pre-gesture side.
**choice** — `change` on the GROUP (payload = the picked Radio's name;
items refuse it). **List** — `change(selected)` / `activate(key, item)`.

Raw spellings that duplicate a gogo concept are SHADOWED — a
one-keystroke redirect, never a silent alias: `on:pressed`→`on:activate`
(or the raw `on:button_down`/`on:button_up`), `on:toggled`→`on:change`,
`on:changed`/`on:changing`→`on:change`/`on:edit`,
`on:activated`→`on:activate`, `on:value_changed`→`on:edit`/`on:change`,
`on:text_changed`→`on:edit`, `on:text_submitted`→`on:change`,
`on:input`→`on:edit`.

- **Auto-wiring**: a named node's events connect to host methods by one
  mechanical rule — `name + event → On<Name><Event>` (`saveButton` +
  `activate` → `OnSaveButtonActivate()`; `brushSize` + `change` →
  `OnBrushSizeChange(value)`); raw signals follow the same rule
  (`OnFooDragEnded`). A missing explicit handler is a load-time error.
  Shadowed raw names never auto-wire, and a stale pre-D181 host method
  (`On<Name>Pressed`, `On<Name>ValueChanged`, ...) on a node that speaks
  (or spoke) that signal is a load-time error naming the new spelling —
  it would otherwise silently unwire.
- **Explicit**: `on:activate=HandleOk`.
- **Anim triggers**: `on:<signal>=play(animName)` on any real signal.
- **Arrangement triggers**: `on:<signal>=go(arrangementName)` flips the
  view's arrangement declaratively — `go(base)` returns home. No host
  handler needed. `on:<signal>=toggle(arrangementName)` flips BETWEEN the
  pose and base (active == it ? base : it — the disclosure button; from a
  different pose it goes to the named one). `toggle(base)` is a load-time
  error, and pairing it with button `toggle=true` draws an advisory lint:
  the button's selected state and the pose are two separate on/off states —
  keep the button momentary and style it with `when <pose> { ... }`.
- **Host relays**: `on:<event>=emit(HostSignalName)` — re-emits as a signal
  on the host, no code-behind handler (the WPF relay pattern without the
  method). The host signal must exist (load-time error otherwise). Source
  args are dropped — a `change(float)` relays as a bare notification;
  use a named handler when the payload matters.
- **Visibility pseudo-events**: `on:show=play(appear)`,
  `on:hide=play(disappear)`. The hide plays first and hides on finish
  (deferred hide), then restores the base look; showing during a hide cancels
  it and plays the show anim from current values. A control that LANDS on
  screen already visible (first build, hot reload) plays its show anim once
  as an **entrance** — to a user, "appeared" and "was shown" are the same
  event; an initial `visibility=gone` stays state, not a trigger (the show
  plays on the flip). The editor preview defaults entrances OFF (it rebuilds
  per keystroke) — its "Entrance anims" toolbar toggle re-enables them;
  hosts can switch with `gogo.set_entrance_anims(false)`. Caveat: during a
  deferred hide the control is technically still visible, so a raw
  `visible = true` write in that window fires no signal — interrupt a
  running hide from code via `play` instead.

## epx units

Bare numbers on length properties are **effective pixels** — plug in the
conversion with `set_epx_provider(callable)` and call `units_changed()` on
resize/dpi change; every epx-typed binding re-applies. Length properties:
sizes, spacing, insets, paddings, margins, `translate`, corner radius,
border thickness, `font_size`, `icon_size`, and anim/when values on those.

**One writer per property — enforced by the structure.** An epx-typed
declaration is a live binding: every `units_changed()` re-applies the declared
value, silently overwriting whatever code wrote to the same property in
between. gogo makes the conflict unrepresentable instead of merely forbidden:
**base declarations** own layout (bindings re-apply it), **arrangements** own
the alternative standing poses (the ACTIVE one re-applies), and **anims** own
only the writer-free motion channel (identity at rest). Don't have
code-behind adjust a property the file declares (the classic: adding a
safe-area inset onto a declared offset — that's the `safe` value's job);
either declare the value in gogo, make it an `arrangement`, or leave the
property entirely to code.

## Runtime API (gogo.gd)

| Call | Purpose |
|------|---------|
| `rebuild_into(root, path, host, on_built)` | (Re)build a file into a live root; on failure the previous tree stays. |
| `track(root, path, rebuild)` / `untrack(root)` | Hot reload (debug desktop only). |
| `play / stop / is_playing (root, anim)` | Animations (see above). |
| `set_arrangement(root, name)` / `current_arrangement(root)` | Arrangements — flip the standing pose ("base" = the declarations). |
| `set_screen_class(name)` / `current_screen_class()` | Screen classes — name the app-wide size class the `when screen.*` blocks respond to ("" = none). Every tracked view re-poses instantly; later builds start in the class. |
| `set_motion_speed(multiplier)` | Global motion speed: 1.0 / fractions (slow-mo) / INF (instant-complete). |
| `set_epx_provider(callable)` / `units_changed()` | epx conversion / re-apply. |
| `set_safe_insets(l, t, r, b)` | Safe-area insets in px; every `safe`-valued padding/margin re-applies live. |
| `set_theme_variant(name)` / `theme_variant()` | Live re-skin of tracked views. |
| `set_translations(locale, table)` / `set_locale(name)` / `locale()` | `"literal"/key` locale tables; flipping re-applies every keyed string live (see Translations). |
| `translate(key, fallback)` | Code-side term lookup — pass the English text as the fallback. |
| `explain(control)` | Debugging readout: declared placement → compiled anchors/offsets (px + epx), regime, input provenance (`interactive=`/passive/engine), minimums, live rect. The editor panel shows it for the node at the caret. |
| `resolve_named(named, name)` (static) | Named lookup over a build's map. A `/` reaches a widget's body nodes: `"sizeSlider/valueLabel"` (one level; per instance). Backs `GogoView.get_gogo_node` and the C# `GetGogoNode`. |
| `set_input_overlay(root, on)` / `input_overlay_active(root)` (static) | The interactive-region debug overlay: tints every currently-capturing region — a dead surface shows no tint; zero capturing regions shows a "No input targets" banner naming the `interactive=true` fix. Also on the editor preview toolbar ("Input"). |
| `stranded_conditions(root, [conditions])` | Idle-state invariant probe: transient input conditions (`held`, `hovered` by default) still active under `root`, as `"name: condition"` entries. Call at a "no finger down" moment and expect empty — a hit is a stranded condition (a gesture ended without releasing it). `focused` and selected/disabled are standing states and stay out; on a desktop with a live mouse pass `["held"]` (a hover under the cursor is legitimate). |
| `register_control(name, factory)` | Custom widget types from a `Callable() -> Control` factory. |
| `register_control_script(name, script_path)` | Custom widget type from a GDScript file (`new()` is the factory). Script-backed types can ALSO preview live in the editor — see the custom-controls note below. |
| `create_control(name)` | Builds one registered control by name in code — the same registry the resolver uses for that name in a `.gogo` file. Null + console error for unknown names. |
| `apply_style(control, path, style_name)` | Puts a style declared in `path` (or its `uses` includes) onto a control built in code — the same standing look and `when` responses `.StyleName` gives a declared node. The file is named at the call site because styles are file-scoped. Self props and self `when` responses only: responses reaching other nodes, `slot.` paths and `anim=` transition rules are refused (they need declared structure), as are classes outside Button/Label/Input/Image/Rectangle/Scroll/Panel/Slider. Epx values re-apply on `units_changed`, keyed strings follow locale flips. True on success; false + console error leaves the control untouched. |
| `register_property(name, handler)` | Custom properties (e.g. `tap_pulse`). |
| `check_source(source)` / `parse_error_check(source)` | Tooling/tests. |
| `add_diagnostics_listener(cb)` / `remove_diagnostics_listener(cb)` | JSON diagnostics per (re)build: `cb(path, Array[Dictionary])`, empty = clean. |

Code-behind: extend `GogoView` (GDScript) — the view class IS the file's root
node; named nodes land in `named`, `_on_view_loaded()` runs per (re)build.
C# hosts load `gogo.gd` once and `Call` through it.

**Is gogo animating right now?** Poll the scene-tree group `"gogo_anims"`
(`get_tree().get_nodes_in_group(&"gogo_anims")`, C# `GetNodesInGroup`):
empty means nothing gogo-driven moves this frame. Every motion pump keeps
its membership current — anim players for their whole lifetime, the
`when`/arrangement tween behavior, the slot-glide driver and the scroll
fling exactly while armed. Built for hosts that stop their frame loop when
nothing on screen can change (a battery idle gate): poll the group before
parking, or a glide freezes mid-flight until the next input. Value polling
(text fields) and the debug overlays deliberately stay out — they only
matter on frames the host runs anyway.

**Custom controls in the editor preview.** Runtime registrations don't exist
in the editor, so the panel reads the `gogo/editor/custom_controls` project
setting (a string array) to preview files that use them:

- `"MyWidget"` — builds a tooltip'd placeholder Control (right for
  C#/runtime-only factories the editor can't run).
- `"MyWidget=res://ui/my_widget.gd"` — builds the REAL control from the
  script (GDScript loads in the editor), fully interactive in the preview.
  Pair it with `register_control_script("MyWidget", "res://ui/my_widget.gd")`
  at runtime so both sides build the same thing. A path that fails to load
  falls back to a placeholder naming the failure in its tooltip.

Names NOT in the list stay errors — typos are never stubbed.

## Gogo internals (nodes the runtime puts in your tree)

Inspecting a built view (the remote scene tree, `explain()`, breakpoints)
shows nodes you never declared. They are gogo's machinery: address your UI
through the `named` map and the gogo surface, never by reaching for these —
their names, placement and existence are implementation details that shift
between versions. None of them claim input for themselves.

| Node | What it is |
|------|------------|
| `GogoStates` | Per-node child managing `when` responses, transitions and state priorities; appears wherever something is styled by state. |
| `GogoDriver` | A control's automatic-geometry writer — the ONE writer of `automatic=` slot geometry, mapping the fraction to anchors/offsets/wedges and timing the glide. |
| `GogoSlot` (named after the slot) | The region node behind a named/propped `slot` declaration: box drawing, clipping, padding, the wedge mask, hit truth for the control's router. |
| `GogoControlRoot` (and per-composite subclasses, e.g. the Slider's) | The node behind a `control` instance (and behind `render=gogo` built-ins): live `$params`, contract signals, the behavior host, slot registry, input routing. |
| `<Name>Reveal` / `GogoReveal` | The `reveal_x/-y` proxy the container sorts — reports `child_min × factor`, clips the rest; your node lives inside it and `named` still points at your node. |
| `NativeLook` | The composites' native-drawing default content — see below. |
| `GogoInputHint` | Debug builds only: prints the press-miss hint (topmost node + the `interactive=` fix) when a press lands on paint nothing claims. |
| `GogoInputOverlay` / `GogoContractOverlay` | The debug overlays behind `set_input_overlay` / `set_contract_overlay` and the preview toolbar toggles. |

The vocabulary aliases (`Scroll`, `Layers`, `Circle`, `Arc`, `Gradient`,
`Grid`, `Tiles`) are script-backed Controls rather than injected extras —
they ARE the node you declared, listed in Vocabulary.

### NativeLook

`NativeLook` is how `render=gogo` avoids a visual cliff (spec §5.7 "Visual
continuity"): the Slider composite's slots ship with it as their default
content, and each one draws the **same theme resources the native node
draws** — `part=groove` the HSlider "slider" stylebox, `part=area` the
"grabber_area"/"grabber_area_highlight" styleboxes, `part=grabber` the
grabber icon (including its highlight and disabled variants, picked live
from the control's states). An unstyled composite is therefore
near-pixel-identical to the native control under ANY Godot theme — the
pixels come from the theme itself, not from a copied look.

It also carries the native *minimums* (groove height, grabber size), which
is why the composite measures exactly like the native node in layouts.

It yields piecemeal, which is what makes styling a composite predictable:

- a **fill** (`slot thumb { ... }`) replaces the NativeLook node outright —
  your content, your minimum;
- a **style painting the slot's box** (`slot.track.background=`,
  `border-*`) mutes it — draw and minimum both — so your box *replaces*
  the native part instead of layering under it. Geometry-only tweaks
  (`corner_radius`, `padding`, sizes) leave the native drawing in place.

You never write `NativeLook` yourself: it resolves only inside the addon's
own composite declarations (`addons/gogo/controls/`), and the name is
reserved — a user file naming it gets the shadows-a-built-in error. Want a
native-looking part in your own control? There is deliberately no public
spelling for it — a user control's default look is its declared body
(colors and shapes you own), and a restyled built-in slot is yours the
moment you paint it.

## Errors (reading a diagnostic)

```
error[G3001]: `anim=` is not a state response — `when` holds values, it doesn't play
  --> res://ui/menu.gogo:3:37            ← click target: file, line, column
   |
 3 |     when held background=#FFFFFFff, anim=test
   |                                     ^^^^^^^^^ belongs on a `transition`
   |
   = note: ...                            ← why the rule exists
help: move it to a transition into `held`
   |                                      ← the fixed source, when the fix
 3 |     when held background=#FFFFFFff         is mechanical
 4 |     transition any -> held anim=test
   |
```

- `^^^` = the error; `---` (after a `:::` header) = the other place involved
  (e.g. the `when` that holds a property an anim also drives).
- `error[G0000]`-style **codes** are stable search keys; the editor panel's
  *📖 Why?* link opens the code's teaching page (`docs/errors/<code>.md`).
  Most errors are uncoded (`error:`) — codes mark the teachable ones.
- *⚡ Apply fix* in the editor panel applies the suggested edits as one
  undoable operation (only offered when the fix is mechanically safe).
- For known sets (types, properties, styles, states, easings, `$values`,
  anim targets) the help carries a did-you-mean suggestion.
- While a file has an error, the last good tree stays live: the game overlay
  shows the one-liner + help, the Output panel gets the full block, and the
  editor preview keeps the last good build with a banner.

## Reserved words

`widget`, `style`, `theme`, `uses`, `slot`, `signals`, `anim`, `arrangement`,
`atlas`, `slice` (inside an atlas block),
`when`, `transition`, `each`, `track`, `key`, `row`, `skip`, `gap`, `choice`,
`repeat`, `empty`, `as`, `by` (the last two on repeat lines),
`held`, `hovered`, `focused`, `focus_within`, `disabled`, `selected`,
`base`, `any`, `true`,
`false`, `NativeLook` (composite-internal — see Gogo internals) — plus the
retired `state`, `enter`, `exit`, `pressed`, `hover`, `focus`, `checked`
(load-time errors point at their
replacements; the last four respelled in D181). Reserved in TYPE
position; node names stay free — except `base` and `any`, reserved
everywhere (they name state endpoints; a child called `base` next to
`transition base -> held` would read as two things — decided
2026-08-31), `true`/`false` (value words), and
`interactive`, which errors as a name (a bare `interactive` on a node line
is always the typo for `interactive=true`).

## Not in v1

String interpolation, expressions (`play()`, `go()`, `toggle()`, `emit()`
and sizing `fill(n)` are the only call forms), required parameters, named/multiple
slots, style composition, block comments, general
arrays (the list kind exists but only Grid's `cols=`/`areas=` and Gradient's
`stops=` accept one), multiline strings, `%` unit, `Slider orientation=`,
multiline `Input`, true ellipses (Circle draws from the short side).
Repeat's own fences: nested `repeat`s, `repeat` inside widget/control
bodies (both: addressability — the future forms source collections from
the outer item / a widget param), a declarative `items=` binding
(collections are code-set until a data layer exists), dictionary
iteration. `List`'s own: virtualization (it owns its viewport since D121 —
rows still render eagerly), declarative initial selection (keys come from
data code assigns anyway), `ensure_visible(key)` from code.

### No WPF-style ControlTemplates (by design)

Gogo deliberately has no ControlTemplate feature, and there are **no plans
until further discussed, as and if needed**. WPF templates work because its
controls are lookless — behavior classes with zero intrinsic visuals; Godot
controls draw themselves through the theme system, so "replace the visual
tree" would be a permanently leaky abstraction. The useful parts of the
template idea already exist under other names:

| WPF | Gogo |
|-----|------|
| ControlTemplate visual tree | `widget` body |
| ContentPresenter | `slot` |
| VisualStateManager driving named parts | `when` responses + child-target props (`when held flash.visible=true`) |
| `PART_*` named-parts contract | per-instance widget names + `signals` surface |
| TemplateBinding | `$params` (build-time; mutate the named node from code for live changes) |
| Control chrome (Background/Border) | stylebox props (`background`, `corner_radius`, ...) |
| Implicit styles (retemplate a whole type) | — (use Godot Theme for stock chrome) |
| DataTemplate / ItemsControl | `repeat` (see Repeat) — `set_items` hands the collection whole |
| ListBox / ListBoxItem | `List` / `ListItem` (explicit — gogo never generates the item container) |

Known semantic traps when porting a template by hand: input surfaces must be
declared (see Input above) and WPF's top-left image alignment (see Image).
