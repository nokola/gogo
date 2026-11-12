# gogo error codes

Every diagnostic the compiler and runtime can emit, by code. The same
list lives in code as `diag::REGISTRY` (diag.rs) — a test keeps this
file and the registry matched both ways, so a code you see on screen is
always findable here.

How to read an entry: the first line says what the message means, the
rest says how to fix it. Where an entry shows a `Wrong` example, the
tests compile it and check that the compiler refuses it with exactly that
code; a `Fix` example must compile.

Codes are grouped in bands: G0xxx language and parsing, G1xxx placement,
G2xxx properties and their values (unknown words, colors, gradients,
pictures and atlases), G3xxx styles and theme shape, G4xxx animation,
G5xxx widgets and controls, G6xxx theme values, G7xxx file structure,
G8xxx translation, G9xxx repeat and List. Retired numbers
(`diag::RETIRED`) are never reused.

## G0001 — unexpected character

A character the language has no meaning for; the message names it and
its line. Usually a stray symbol from a paste; delete it or quote it if
it was meant as text.

## G0002 — unclosed block

A `{` was opened and the file ended before its `}`. The message gives
the line where the file ends, not the line of the `{` that was never
closed. Indentation finds it fast: the `}` almost always goes at the
first line that dedents past the opener.

Wrong:

```kdl error=G0002
Column menu {
    Button a text="One"
    Button b text="Two"
```

Fix:

```kdl
Column menu {
    Button a text="One"
    Button b text="Two"
}
```

## G0003 — unterminated string

A `"` was opened and the line ended before the closing `"`. Strings
never span lines; close it or split the text.

## G0004 — unknown escape

Inside a string, `\` starts an escape and the letter after it is not
one gogo knows (`\n`, `\t`, `\"`, `\\`). For a literal backslash write
`\\`.

## G0005 — one hex color form

Colors are `#RRGGBB` or `#RRGGBBAA` — six or eight hex digits, alpha
LAST. A three- or four-digit form (`#F00`) is refused rather than
guessed. An eight-digit `#AARRGGBB` paste cannot be told apart: it is
read alpha LAST like any other (G2004 catches the one case where that
leaves the alpha at 00).

## G0006 — `$` must be followed by a name

A `$` alone or before a non-name character. Theme values and widget
parameters are `$word`.

## G0007 — `/opacity` is 0..1

The `/` suffix sets a color's alpha and takes a fraction:
`black/0.5`, not `black/50`.

## G0008 — `@pos` is 0..1

The `@` suffix places a gradient stop and takes a fraction along the
run: `white@0.24`.

## G0009 — bad number

Digits that do not form a number (two dots as in `1.2.3`, a leading dot
as in `.5`, …). Rewrite the number.

## G0011 — unexpected `}`

A `}` with no open block to close. Count braces upward — an extra one
usually rides a deleted block.

## G0012 — `$name needs = value` here

At top level a `$name` line is a definition and must assign:
`$accent = #FF6A00FF`. To USE a value, write `$name` in a property
position instead.

## G0013 — expected a node type or `$value`

A line starts with something that is not a word at all — a number, a
quoted string, a stray symbol — where a node kind (`Button`,
`Column`…), a directive (`theme`, `style`…) or a `$definition` goes.
The message names what it found. (A misspelled kind word is G7011.)

## G0014 — expected end of line

An assignment line (`$name = value`, or `node.prop = value` in a `when`
block) holds one value, and more tokens follow it. Put the second
assignment on its own line. (Two properties run together on a node
line are G0017.)

## G0015 — a comma before `{`

The property list ends before the body: `Button b text="x" { … }`, not
`Button b text="x", { … }`. Drop the trailing comma. It is refused with
the `{` on the same line and on the next.

## G0016 — `->` needs a state name

A transition arrow with a missing end: `transition base -> held …`.
Both ends are state or arrangement words (`any` matches all).

## G0017 — missing `,` between names

Two properties or names ran together without a comma. The message names
the token where the list broke. (A reserved flag like `signals` is its
own delimiter — a comma after it is legal but not required.)

A node whose first word is `x` and a number (`Rectangle x0 10x10`) gets
this error too, with a note: `x0` is the size shorthand for "0 tall", so
it is read as a size and the word after it looks like a member with no
comma before it. Give the node another name.

## G0018 — duplicate parameter / property

The same key appears twice on one line or one widget head. The second
would silently win or lose — say it once.

## G0019 — missing `)`

A call `name(…` reached the end of the line unclosed.

## G0020 — missing `]`

A list `[…` reached the end of the file unclosed.

## G0021 — expected a value

A property position holds something that is not a value (a stray
symbol, or nothing: the line ends after `=`). Inside calls, named
arguments are legal grammar: `arc(from=-135, sweep=270)`.

## G0022 — `key = …` is only legal inside a `when` block

A bare `name = value` line at top level or in a node body. Assignments
pose properties in `when` blocks; a definition needs the `$` sigil; a
node property is `key=value` on the node's own line.

## G0023 — `uses` path rules

`uses` takes one quoted path, and a path never carries a translation
key. Write `uses "widgets.gogo"`. Two files are two lines: a second path
on one line (`uses "a.gogo", "b.gogo"`) is refused. A nested `uses`
resolves against the INCLUDING file's directory (`sub/widgets.gogo`
writing `uses "inner.gogo"` loads `sub/inner.gogo`; `..` climbs).

The same code answers a translation key on a picture's address:
`source="preview://brush"/k` and `source="res://a.png"/k`. An address is
never translated; the key goes on text.

## G0024 — `uses` file not found

The loader has no file at the resolved path. When the spelled and the
resolved path differ, the message shows both, so a wrong base directory
is visible.

## G0025 — `style` needs a name

`style Tool { … }` — the name is how nodes wear it (`.Tool`).

## G0026 — a style block holds `when` and `transition` lines

A style's body responds to states; static looks go on the style's own
line: `style Tool background=black { when held opacity=0.5 }`. A bare
assignment inside the block belongs on the head line.

## G0027 — `widget` needs a name

`widget LabeledSlider { … }` — the name is the node type instances use.

## G0028 — a widget body has one root

A widget is a fragment with exactly one root node; wrap several parts
in a `Column` or `Layers`.

## G0029 — `anim` needs a name

`anim pop { … }` — `play(pop)` and `transition … anim=pop` call it by
name.

## G0030 — anim defined twice

Two `anim` blocks share a name in one scope; rename one.

## G0031 — `arrangement` needs a name

`arrangement wide` — the name `when wide { … }` blocks pose against.

## G0032 — reserved arrangement name

`base` and `any` are the transition grammar's own words, and a state
word (`held`, `selected`, …) cannot name an arrangement — `when held`
must mean one thing. An arrangement line also declares a NAME and
nothing else; poses go in a `when <name> { … }` block.

## G0033 — arrangement declared twice

The same arrangement name on two lines; delete one.

## G0034 — a transition rule is `A -> B`

A view or node transition names its endpoints (`transition base -> held
duration=0.1`); only the listed keys ride it (duration, easing and its
parameters, anim). A `control` declaration's own `transition` line is
its default feel and takes NO endpoints.

Three more forms are refused, because each would build and time nothing:

- a rule written as a block (`transition base -> edit { duration=0.25 }`):
  a rule is one line, and its words go on the line;
- a `screen.` word as an endpoint (`transition any -> screen.landscape
  duration=0.3`): a screen change lands at once;
- `anim=` on a node's rule between two arrangements: an arrangement
  change plays the anim of a top-level rule only (`transition base -> edit
  anim=open`).

## G0035 — misspelled word, with the spelling

A word gogo knows under another spelling (`pressed` → `held`, `max=` →
`capacity=`…). The message says the one to write.

## G0036 — unknown screen word / state

A `when` condition that is no built-in state, declared arrangement,
`screen.` word, or (inside a control) declared custom state. The
message lists what IS in scope. On a widget instance the block is
parked as a warning so copied files keep building.

## G0037 — `when` needs a state

A `when` head with no condition word.

## G0038 — retired spellings

Forms gogo v1 dropped on purpose: `px` (lengths are epx), `%` (shares
are `fill(n)`), and over-composed `when` conditions. The message names
the replacement.

## G0039 — `/` and `@` suffixes are for colors and strings

An opacity or position suffix on a word that takes neither
(`fill/0.5`), or on a `$value` that holds a number (`spacing=$gap/0.5`).
Move the suffix to the color it belongs to.

## G0040 — `@pos` / a list where none belongs

A stop position (`white@0.24`) outside `stops=`, or a `[list]` on a
property that takes none. Positions mean nothing off a gradient; lists
go on `cols=`, `areas=`, `stops=`, `border_dash=`, `dash=` and
`window=`.

## G0041 — retired placement words

Godot property names that gogo spells another way: `anchors`,
`offset_*` and `size_flags_*` (placement says it: halign/valign,
width/height, margins), `stretch_mode`/`expand_mode` (`fit=`),
`autowrap_mode` (`wrap=true`) and `texture` (`source=`). The message
names today's word.

## G0044 — node name used twice

Two nodes share a name in one scope; `find`, responses and anims could
target either. Rename one.

## G0045 — `gap` takes one size word

`gap 12` or `gap fill` — nothing else rides the line, and a gap IS the
space: it holds no children.

## G0046 — a reserved word cannot name a node

`base`, `any`, value words and property words cannot be node names —
`transition base -> held` next to a child called `base` would read as
two things. Any other stray word, number or string after the name gets
this code too: a property is `key=value`.

## G0047 — `gap` lives in a Column/Row

A gap is a stack's spacer; in free parents position the box instead.

## G0048 — `gap` needs a size

`gap` alone says nothing — `gap 12` (epx) or `gap fill` (leftover).

## G0049 — raw signals are gone

`on:pressed=`, `on:value_changed=` and other engine signal names are
not events. The events are `activate`, `press`, `release`, `change`,
`edit`, `show`, `hide`, `pointer`; the message maps the old name.

## G0050 — action call form

Action calls take exactly one argument (`go(wide)`, `emit(saved)`) —
extras would carry nothing. `toggle(base)` is spelled `go(base)`: base
is home, there is nothing to toggle to.

## G0051 — unknown arrangement in an action

`go(x)` / `toggle(x)` where no `arrangement x` exists. Declare it or
fix the spelling.

## G0052 — not an action

`on:` takes `play(name)`, `go(name)`, `toggle(name)`, `emit(name)` or a
bare handler name for the host. `emit()` with nothing in it is refused:
it names what it raises.

## G0054 — a `when` block sets properties

Inside `when`: `opacity=0.5` or `node.prop = value` — never `$theme`
definitions, never nested nodes. An empty top-level `when s { }` is
refused too (it can never do anything), and two blocks on ONE condition
may not both set the same property — the loser would silently vanish.

A `when` answers a state and sets none. `selected=` in a `when`, and
`value=` in a `when` on a Toggle, are refused: `when held selected=true`
would leave the Button unselected, and `when held value=1` would move a
Toggle's knob while the Toggle stays off. A state changes by a tap, a
key or code; `selected=true` or `value=1` on the node's own line is
where it starts.

## G0055 — `=host` is gone

`width=host` / `height=host` were the Godot addon's words for an axis
the host's code sizes. The host program now sizes a page by the
rectangle it lays it out in, so a size is a number, `fill` or `hug`.

## G0056 — a size is a number, `fill`, `hug`, `fill(n)`

A size key got a word outside the size vocabulary, or a `$value` that is
not a number. (A numeric `$value` is fine: `height=$backBoxHeight`.)

## G0057 — cross-axis alignment word

A property that takes one of a fixed list of words got another: an
alignment word from the other axis (`halign=top`), `visibility=maybe`,
`fit=stretch`. The message lists the words the property takes.

## G0058 — a number key given a non-number

`spacing=big` — the key wants epx. (A numeric `$value` is fine:
`min_width=$rowFont`.) A `$value` that holds a color or a text is
refused like the word itself: `$c = #ff0000` then `spacing=$c`.

## G0059 — a top-level `when` writes `node.prop = value`

View-level poses target nodes by name; a bare `opacity=0.5` there names
nobody.

## G0060 — no slot/node of that name

A response, anim lane or pose targets a name the page does not have.
Inside a widget, body names carry the instance prefix (`inst/label`),
`self` is the widget's root and the body root's own name is an alias
for the instance (`a/root` = `a`). At page scope `self` is the file's
root.

## G0061 — an anim step says when it runs

Every step after the first starts with `and` (alongside the previous)
or `then` (after the whole cluster); the first stands alone.

## G0062 — anim step edges: `after=`, `play`, `each`, `group` misuse

A step of an anim placed or named in a way that cannot play. `after=`
goes on a `group` line and names another named `group` of the same anim
(`group late after=intro { … }`); it stands instead of `and`/`then`,
never beside one, and two groups never wait on each other. A `group`
holds at least one step and its name is used once. `play <name>` names
a declared anim that ends (not `repeat=forever`) and does not play
itself. `each <node>` or `each .Style` names who it drives, never sits
inside another `each`, and its lines are bare (`opacity to=0`). A
`then` cannot wait on an `each … stagger_by=`, a pause (`delay=`) is
never negative, and `key` lines sit inside a property line. `after=` is
a `group` word only: an `each` or a `play` step that should wait on a
group is wrapped in one (`group late after=intro { play pop }`).

## G0063 — anim lane shape

An anim line drives `<node>.<motion word>` — translate.x, translate.y,
scale, scale.x, scale.y, rotate, opacity — or `<node>.<property>` for a
look property (`box.background`, `box.corner_radius`, `box.font_size`)
or a size, a margin or a padding (`box.width`, `box.margin_left`,
`box.padding_top`).
`slot.<name>.<channel>` lanes
belong to a control's OWN scoped anims. And an anim holds journeys,
never assignments: `box.opacity from=1, to=0, duration=0.2` — a bare
`box.opacity = 0` would snap with no track to play.

## G0064 — an anim lane needs `to=`

A journey without a destination; `from=` is optional (the current value
fills in), `to=` is not.

Two steps that write `from=` on one property and run at the same time are
refused too: the property would start at two values at once. One after
the other is fine, each step starts at its own `from=` (a blink).

## G0065 — `easing=` takes a name

An easing word (`cubic_out`, `bounce_out`, …), not a number.

## G0066 — the words of an anim line

An anim's header line takes `repeat=forever` (a one-shot anim just omits
it; there is no repeat count), `speed=` above 0, `duration_each=`,
`easing=` and `autoreverse=`.

A step line takes its own words and nothing else. A word no line reads
would be dropped without a sign, so it is refused:

| Line | Takes |
|------|-------|
| a property line (`box.opacity …`) | `to=`, `from=`, `duration=`, `offset=`, `easing=`, `autoreverse=` |
| a property line with `key` children | `offset=`, `autoreverse=` |
| a `key` line | `t=`, `v=`, `easing=`, `jump=` |
| a `group` | `duration_each=`, `easing=`, `offset=`, `after=`, `autoreverse=` |
| an `each` | `stagger_by=`, `duration_each=`, `easing=`, `offset=`, `autoreverse=` |
| a `play` step | `offset=` |
| a pause | `delay=` |

`repeat=` on a step line gets a pointer to the header: a step plays once
each time the anim runs.

Wrong:

```kdl error=G0066
anim flash {
    badge.opacity from=1, to=0, duration=0.3, repeat=forever
}
View page { Panel badge width=20, height=20 }
```

Fix:

```kdl
anim flash repeat=forever {
    badge.opacity from=1, to=0, duration=0.3
}
View page { Panel badge width=20, height=20 }
```

## G0067 — `a..b` only after `in` on a slot line

Ranges couple a driven slot to its value:
`slot thumb automatic=position by $value in $min..$max`.

## G0068 — `choice` rules

A `Check` never sits inside `choice` (exclusivity is Radio's), and a
bodied Radio/Check draws its own content — `text=` beside a body would
be two labels.

## G0069 — Radio naming / `choice value=` rules

Every Radio in a `choice` has a name (the value is "which one"), a
Radio outside any `choice` has nothing to be exclusive with, and
`value=` on the choice line names one of its Radios.

## G0070 — retired event spellings / nested `choice`

Old event words, and a `choice` inside a `choice` (one exclusivity
scope per group).

## G0071 — an asset path carries `res://`

Image and font paths are explicit: `source="res://icons/save.png"`. A
bare file name could be a label or a path — the scheme says which. The
names it is asked of end in `.png`, `.jpg`, `.jpeg`, `.svg`, `.ttf` or
`.otf`.

## G0072 — `on:<event>` fires only on a press surface

`activate`/`press`/`release` live on Button, Check, Toggle, Radio,
ListItem. A List takes `on:activate` only (for the rows it owns); a
Slider takes `on:press`/`on:release` (they bracket its drag) but not
`on:activate`. `change` on a Radio belongs on the `choice` line.
`on:pointer` goes on a passive box — on a control it would fight the
control's own gestures.

## G0073 — `on:pointer` needs `interactive=true`

Passive boxes are transparent to input, always; the wire alone claims
nothing. Declare the surface: `interactive=true`.

## G0074 — a 1x1 box clips its children (warning)

A written 1x1 size on a box with content is almost always a stray size
arg; the content would be invisible.

## G0075 — `modulate.a` is an engine word

Write `opacity` — the gogo word for the same channel.

## G0076 — a name-list head needs at least one name

`state`, `state_builtin` and `signal` lines declare comma-separated
names; an empty head declares nothing.

## G0077 — control-body lines in a widget body

`state`, `state_builtin`, `signal`, `keys` and `behavior` lines say how
a `control` behaves; a `widget` has no behavior, so its body refuses
them (and the Godot addon's `script` line). Under `state`, a built-in's
name is also refused — `state held` claims a built-in state; declare
built-ins with `state_builtin`, your own words with `state`. Another
dialect's word for a built-in (`hover`, `pressed`, `focus`, `checked`)
is answered with the spelling that builds: `state_builtin hovered`.

## G0078 — a bool key / overlay rules

A true/false key got something else; `reveal_x=` or `reveal_y=` got a
number outside 0..1; `overlay=true` sits beside `visibility=space_only`;
or an `overlay` box's `fill` axis sits under a hug parent that nothing
sizes.

## G0079 — unknown unit on a number

Numbers take `epx` (the default — just write the number). `px` and `%`
are read as units too: `px` is retired (G0038); `%` is retired as a
length (G0038) and lives on in `key_unit=5%` and in an anim's
`translate.x`/`translate.y` values (`to=100%`). Anything else is a typo.

## G0081 — `live()` and `bind()` are gone

Both wrapped a control's value so that a label would follow it. A
`$value` follows by itself now, on any property: write
`Label text=$value`, not `Label text=live($value)`. To pick how a shown
number is rounded, write `text=round($value, 1)`. The message names the
spelling to write.

## G0082 — the `behavior` line

`behavior <name> by $value in $min..$max, step=$step, key_unit=5%`
names the input behavior that answers a `control` (GP52/GP53): gogo's
`slider` and `toggle`, or a behavior the host registered
(`CompileOptions::behaviors` + `Input::register`) — an unknown name
lists every behavior the build knows. The wiring is spelled in full;
`step=` and `key_unit=` are positive (`key_unit` in value units or `%`
of the range); every slot the behavior reads must be a `slot` in the
body (`slider`: track; `toggle`: track and thumb); one `behavior` line
per control; and the behavior OWNS the key groups its spec lists
(`slider`: arrows, home_end; `toggle`: all four) — a `keys` claim on
one is a second owner, refused. The Godot addon's `script "x.gd"` line
is refused here too, naming `behavior` (a line dropped without a word
would leave the control deaf to taps).

## G0083 — a `when` response reads the control's `$value`

Inside a `control`, `$value` moves while the page runs: a drag, a key
or code writes it, and a property that reads it on a node line follows
(`Rectangle opacity=$value`). A `when` response is different: it is one
fixed value, held while its state is on, so it cannot follow.
`when hovered opacity=$value` would keep the number the control started
with and look right until the value moved, so it is refused.

Write the property on the node line and it follows. A `$param` that is
not the control's value never changes after the build, and stays legal
in a `when`.

Wrong:

```kdl error=G0083
control Dimmer $value=0.5, $min=0, $max=1 {
    behavior slider by $value in $min..$max
    state_builtin focused, held
    Layers width=100, height=20 {
        slot track width=fill, height=fill { Rectangle color=#333333FF }
        Rectangle glow color=white {
            when held opacity=$value
        }
    }
}

View v { Dimmer d }
```

Right:

```kdl
control Dimmer $value=0.5, $min=0, $max=1 {
    behavior slider by $value in $min..$max
    state_builtin focused, held
    Layers width=100, height=20 {
        slot track width=fill, height=fill { Rectangle color=#333333FF }
        Rectangle glow color=white, opacity=$value
    }
}

View v { Dimmer d }
```

## G0084 — `on:show` and `on:hide` take `play(…)` only

`on:show` and `on:hide` are hooks, not events: when a node shows or
hides, gogo plays the anim the hook names, and that is all it does. No
event leaves for the host and no arrangement flips. So
`on:show=go(edit)`, `on:show=toggle(edit)`, `on:show=emit(Shown)` and a
handler name (`on:show=Shown`) would sit on the line and never run, and
they are refused.

The code that shows a node already knows it did: run the handler's work
where the host calls `rt.show(…)`, or where it sets the state that shows
the node.

Wrong:

```kdl error=G0084
View page {
    Panel sheet width=200, height=100, on:show=Opened
}
```

Right:

```kdl
anim appear { sheet.opacity from=0, to=1, duration=0.2 }
View page {
    Panel sheet width=200, height=100, on:show=play(appear)
}
```

## G0085 — a call gogo does not have

The calls a value takes are few, and each has its place:

- `round(number, step)` on a number.
- `fill(n)` and `hug(n)` where a size is written: `width=`, `height=`,
  the floors and caps, an entry of `cols=`, `row_height=`.
- `arc(…)` and `angle(…)` on a slot's `automatic=`.
- The actions on an `on:` word: `play(name)`, `go(name)`,
  `toggle(name)`, `emit(name)` (a wrong one there is G0052).

Any other name would be stored and read by nothing: the line would
build and draw nothing. A size call where no size goes
(`opacity=fill(2)`) is refused too.

Wrong:

```kdl error=G0085
View page {
    Label title text=upper("hello")
}
```

Fix:

```kdl
View page {
    Label title text="HELLO"
}
```

## G1002 — grid track bookkeeping

The row form must square with `cols=`: a `row { }` covers no more
tracks than `cols=` declares (a `skip` covers one; a short row leaves
its last tracks empty), spans reach only declared tracks, weights ride
tracks not cells, and structure words never come from a style.

Wrong:

```kdl error=G1002
Grid form cols=[hug, fill] {
    row { Label a text="Size"; Slider b; Label c text="px" }   // three cells, two tracks
}
```

Fix:

```kdl
Grid form cols=[hug, fill, hug] {
    row { Label a text="Size"; Slider b; Label c text="px" }
}
```

## G1003 — areas-form rules

`areas=` strings draw the lattice: every row string lists one word per
track, a name marks a RECTANGLE (no L-shapes), `.` is a hole, `:
height` after a row string sizes it, and each named area binds one
child by name. The areas and row forms never mix in one Grid.

Wrong:

```kdl error=G1003
Grid shell cols=[80, fill],
     areas=["header header : 60",
            "nav    header : fill"] {      // `header` is an L, not a rectangle
    Panel header { }
    Column nav { }
}
```

## G1004 — `wrap=true` with no width to wrap at (lint)

A wrapping label whose width nothing determines never wraps
(`width=hug` measures the text on one line), or wraps at about one word
when the boxes above it size to their content. Give it `width=fill`, a
number or a `max_width`; in the second case give the boxes above it a
width too. A sized box around a `width=hug` label does not help.

Warns:

```kdl warning=G1004
Layers {
    Label note text="a long line of text that should wrap", wrap=true, width=hug
}
```

Fix:

```kdl
Layers {
    Label note text="a long line of text that should wrap", wrap=true, width=hug, max_width=200
}
```

## G1006 — both margins and an exact size over-constrain the axis

`width=200, margin_left=10, margin_right=10` says three things about
one axis; the box cannot honor them all. Drop one. The same holds for
`height=` with `margin_top=` and `margin_bottom=`, and for `margin=N`
beside an exact size on a side with no alignment (the shorthand writes
both margins there).

## G1010 — `height=fill` but no row takes the leftover (lint)

A Grid told to fill whose rows are all content-sized leaves dead space;
give a row the leftover (`: fill`) or hug the Grid.

## G1011 — an aligned axis with `fill` leaves nothing to position

`halign=center, width=fill` (or `valign=center, height=fill`) — a
filling box has no room to be centered in. Use `hug`, or cap the fill
(`max_width=`, `max_height=`). `left`/`right` with `fill` are not
refused.

## G1013 — a floor above its cap

`min_width` above `max_width`, or `min_height` above `max_height`: the
floor is above the ceiling; no size satisfies both.

## G1014 — a margin on a child of a box that places its children

A Column, Row, Choice, Flow, Grid or Tiles deals each child its place,
and so does a List with its rows: a margin on such a child is taken and
moves nothing, posed or declared. Space the children with `spacing=` on
the parent or a `gap` line between them, or give the parent `padding=`.
The one body of a `Scroll` is not such a child: a margin insets it and
scrolls with it. The same rule refuses `margin=` in a pose (a pose moves
one edge at a time — write `margin_left=`/…) and placement words under
an input state's `when` (placement answers the view's conditions —
`when <arrangement>`, `when screen.*`; a held box re-anchoring would
fight the finger).

## G1015 — a seat declaration's shape

`seat name` declares a named, empty socket: it needs a name (`home` is
taken — in a seat move it means the node's declared home) and holds no
children, only what a `when` moves into it. Size words on the seat line
size the socket (`seat socket width=200`).

## G1016 — a seat move's rules

`when <state> { node.seat = seatName }` at view level moves a node into
a DECLARED seat: the target must be a seat, a node cannot move into its
own subtree, one occupant per seat, one writer per node, and the
declared home is where it rests.

## G1017 — `aspect=` contract

`aspect=` relates the two axes, so one axis drives and the other is
derived (both `fill` is fine: the box is the largest of that ratio that
fits): two written numbers leave nothing to derive (the ratio IS the
second size), a hug axis is content-sized, a `max_*` cap would break the
ratio, and a stack/grid parent owns both axes (aspect needs a free
parent — Layers, View, Panel). A floor on the side the ratio works out
(`width=100, aspect=2, min_height=80`) is never read: put it on the side
that drives the ratio. The ratio is positive width/height;
`aspect=natural` reads a bitmap's own ratio and lives on Image only.

## G1018 — a List's cap rides its hug

`max_height=` on a List bounds `height=hug` (the box grows with its
rows to the cap, then scrolls); on any other height the cap would fight
the viewport. A `layout Row` strip runs sideways: its cap is
`max_width=` on `width=hug`.

## G1019 — a floor or a cap beside an exact size for the same side

`width=100, min_width=200` on one line: an exact width is the width, so
the floor says nothing or contradicts it. `width=100, max_width=40`: the
cap is never read. Drop one of the two; a box that takes the room it is
given up to 40 is `width=fill, max_width=40`. The same holds for the
height, for `size=` and a bare `100x40`, and for `min_size=`.

Wrong:

```kdl error=G1019
Rectangle bar width=100, min_width=200
```

Two forms build, because there the second word has a job. A `when` that
writes that size on the node: the floor and the cap bound what the
`when` asks for. And words from different lines: a style or a control's
template that carries `min_size=48x48`, used on a line that writes
`20x20` — the floor holds, in every parent.

```kdl
arrangement small
when small { bar.width = 20 }
Rectangle bar width=100, min_width=40
```

## G1020 — a Scroll that is 0 long on a side it pans, or that can never reach what it cuts

Three Scrolls that build nothing usable. A side that pans and hugs with
no cap (`allow=vertical, height=hug`, or an aligned side with no size
word): a side that pans asks its parent for no room, so the Scroll is 0
long there — write `height=fill`, a number, or `height=hug,
max_height=300`. A side that a cap cuts while `allow=` leaves it out
(`allow=horizontal, height=hug, max_height=300`): what lies past the cap
cannot be reached — name the side in `allow=`, or drop `allow=`. And
`height=hug, max_height=fill`: that side grows with its content, past
its parent, and never scrolls — write `height=fill`, or cap at a number.

Wrong:

```kdl error=G1020
Scroll page allow=vertical, height=hug {
    Column { Label text="row" }
}
```

## G1021 — a Scroll that hugs both sides never scrolls (warning)

`Scroll width=hug, height=hug`: as big as its content on both sides, it
is a plain box and nothing scrolls. Give a side a size: `height=fill`, a
number, or `height=hug, max_height=300`.

## G2001 — unknown property

No kind takes this word. The message suggests the nearest real one, or
names the gogo word for a XAML or Godot spelling (`foreground=`,
`visible=`, `mouse_filter=`).

Wrong:

```kdl error=G2001
Label title text="Hello", no_such_word=1
```

## G2002 — a gogo word on a kind that does not take it

The word exists but not HERE: a Grid cell word off its lattice, a
`Scroll`-only word on a box, `color=` on a Label. The message names
where it belongs. (A XAML or Godot spelling that is no gogo word at
all, such as `foreground=` or `visible=`, is G2001.)

Some words belong to a kind's neighbours and not to the kind:

- `clip_text=` on a Button, a Check, a Radio or a Toggle. Their own text
  folds with `wrap=true`; text that is cut is a Label's
  (`Button { Label text="…", clip_text=true }`).
- `wrap=`, `clip_text=`, `horizontal_alignment=` and
  `vertical_alignment=` on an Input. An Input shows one line of text
  that starts at its left edge.
- `value=` on an Input. The field's text is `text=`.
- `valign=`, `aspect=` and `overlay=` on a ListItem. A List places its
  rows itself (a margin on a row is G1014).
- `disabled_opacity=` on any node's line. It is page-wide and has one
  home, the top `theme { }` block.
- `background=` or `color=` on a seat, a Grid's `row` or a `choice`.
  They draw nothing of their own.
- `toggle=` on anything but a Button.

Wrong:

```kdl error=G2002
Label title text="Hello", color=red        // a Label's text color is font_color=
```

Fix:

```kdl
Label title text="Hello", font_color=red
```

## G2003 — a one-stop gradient renders as a solid

One color is a `Rectangle color=…` — the message spells the rewrite.

Wrong:

```kdl error=G2003
Gradient veil stops=[blue]
```

Fix:

```kdl
Rectangle veil color=blue
```

## G2004 — invisible pigment

A hex color whose ALPHA channel reads zero because an `#AARRGGBB` paste
put alpha first (gogo is `#RRGGBBAA`, alpha LAST). The box would be
invisible with no other symptom. The compiler checks `background=` and
`border_color=` outside a `when`; other color words are not checked.

Wrong:

```kdl error=G2004
Panel scrim background=#FFFFFF00 { Label a text="x" }   // white, alpha 00: nothing shows
```

Fix:

```kdl
Panel scrim background=#FFFF00FF { Label a text="x" }   // the XAML paste #FFFFFF00, alpha moved LAST
```

## G2005 — `stops=` takes the gradient's color list

`stops=[white, black]` spaces evenly; `@positions` pin entries and are
all-or-none, never decreasing. A `when` or an anim line that writes a
stop (`g.stop_3_color`) names a stop the list declares: stop 3 of a
two-stop gradient is refused.

Two words are written LIVE (in a `when`, an anim, or from code) and
never on a node's line: a Gradient's `stop_<n>_color` / `stop_<n>_offset`
(the line says `stops=`), and a fill's `color.a` (the line puts the
alpha in the color: `color=black/0.5`, `color=#00000080`).

## G2006 — `transparent` in `stops=`

A stop is written as a color with its alpha: `white/0`, or the next
stop's color (`red/0`). Stops mix premultiplied, so a stop at alpha 0
adds no color of its own and each of these fades the same. The word
`transparent` stays refused all the same: it names no color, and a stop
that names one says the same thing however stops are mixed.

## G2007 — the word is not a color

A color position got a non-color word (or a `$ref@pos` whose ref is
not a color). The message suggests the nearest palette word.

## G2008 — a variant-flipping color inside `stops=`

`surface`, `text` and the other mode words carry a light and a dark
half; a gradient stop bakes ONE color. Write the hex you mean (the
light half still lands so the shape survives while you fix it).


## G2009 — `border_dash=` / `dash=` takes dash/gap lengths

`border_dash=[4, 1]`: a non-empty list of positive numbers in
`border_thickness` units (WPF StrokeDashArray — `4 1` at a 1-epx border is
4-epx dashes and 1-epx gaps; an odd list repeats to even out). A bare
number, an empty list, or a non-positive entry is this error. On a Circle
the lengths run along the ring, clockwise from the top. An Arc's `dash=`
takes the same list in `thickness` units along its sweep.

## G2010 — `window=` takes the texture rectangle

`window=[left, top, width, height]` on an Image: four numbers in the
texture's own pixels, the corner at or past 0 and a positive size. A
bare number, a short list, a negative corner or a zero size is this
error — such a window would draw nothing and say nothing.

## G2011 — an `atlas` line's shape

An `atlas` line names one picture and says how to cut it. It sits at the
top of a file (never inside a node), has one plain name, takes the picture
by `source="res://….png"` or a packer's export by `from="res://….json"`
(one of the two), and its words are `cell=`, `edge=`, the `edge_left`
/ `edge_top` / `edge_right` / `edge_bottom` family and `release=unused`
(G2020). Its block holds only
`slice` lines; a grid (`cell=`) has no block.

Wrong: `atlas icons cell=48x48` · Fix: `atlas icons source="res://icons.png", cell=48x48`

## G2012 — an atlas picture or file the build cannot read

An atlas reads its picture's size from the file header (and `from=`
reads the JSON), so the file must exist, sit outside any `.ignore`
fence, and be a PNG or JPEG. A standalone compile (a test, a tool)
needs `CompileOptions::assets` to read files; `build_pages` passes one.
A hot reload of a built program reads no picture: it takes each atlas
as the program was built with it (G2022).

## G2013 — `cell=` does not divide the picture

A grid's squares must tile the picture exactly: a 480x720 picture cuts
into 80x80 squares (6 by 9), not 80x70. `cell=` is one square's size in
whole picture pixels.

## G2014 — a `slice` line's place

`slice play x=0, y=0, size=48x48`: the slice's top-left corner and size
in whole picture pixels, inside the picture. Each slice name appears
once in its atlas.

## G2015 — `edge=` bands

`edge=12` keeps the outer 12 picture pixels of each side at their own
size while the rest stretches (a 9-slice). The numbers are whole pixels,
and left + right (and top + bottom) must leave a middle to stretch. On a
named atlas `edge=` goes on the `slice` lines; on an import it goes on a
`slice <name> edge=…` line in the block; a slice the packer trimmed
cannot take it (turn trimming off for that sprite).

## G2016 — `source=` names no such atlas, slice or cell

`source=animals[12]` picks cell 12 of the grid atlas `animals`,
`source=toolbar.play` the slice `play`, `source=bubble` a whole-picture
atlas. The atlas must be declared (in this file or a `uses` file), the
cell number inside the grid, the slice name one the atlas has. A quoted
`"res://…"` path shows a whole picture without any atlas.

## G2017 — a packer's JSON import that gogo cannot use

`from=` reads the TexturePacker "JSON (Hash)" or "JSON (Array)" layout
(also written by Free Texture Packer, ShoeBox and Aseprite). The message
says what is wrong: the file does not parse, has no `frames` or
`meta.image`, a frame is rotated (turn rotation off in the packer), two
file names become the same gogo name, a frame lies outside the picture,
or the picture changed size since the export (export again).

## G2018 — `window=` / `fit=` on an Image showing an atlas slice

A slice is already a part of its picture, so `window=` has nothing left
to choose — declare a smaller slice instead. A 9-slice (a slice with
`edge=`) always fills its Image's rectangle, so `fit=` does nothing on
it.

## G2019 — one atlas name declared twice with different slices

Atlas names are one namespace for the whole build (like translation
keys), because code reaches them as `atlas::NAME`. The same `uses` file
under two pages is fine; two different `atlas icons` lines are not —
rename one.

## G2020 — `release=` it cannot use

`release=unused` lets the app free a built-in picture while no page shows
it, and decode it again when one does. It takes only the word `unused`,
and it goes where the picture is declared:

```kdl partial
atlas stars source="res://assets/Images/Stickers/Stars.jpg", cell=80x80, release=unused
Image hero source="res://assets/big.jpg", release=unused
```

An Image showing an atlas slice (`source=stars[3]`) declares nothing —
put the word on the `atlas` line. A picture the app draws itself
(`source="collage://preview"`) has no file to decode again; its owner
frees it.

## G2021 — a picture not in this build (hot reload)

A dev build reloads a `.gogo` file while the app runs, but the pictures it
can show were embedded when the app was built. A `source=` naming a file
the build never saw has no pixels to show, so the reload is refused at
that line. Rebuild the app once; from then on the picture reloads like
any other value. The compile-time switch is
`CompileOptions::refuse_new_keys_and_pictures` (`gogo_ui::reload`).

## G2022 — an `atlas` line that is not the build's (hot reload)

An atlas is cut when the app is built: the build reads the picture's
size, numbers the slices and writes those numbers into the code
(`atlas::STARS[3]`). A reload cannot number them again, so it takes every
atlas as built. An `atlas` line that says something else than it did at
build time, or one the build never saw, is refused at its line. Rebuild
once; from then on the page reloads again. The rest of a page with
atlases reloads like any other.

## G3002 — an automatic slot property assigned

`automatic=` couples the slot to the control's value — the control
drives that geometry; writing it by hand would fight the driver.

Wrong:

```kdl error=G3002
control Progress $value=0, $min=0, $max=100 {
    Layers {
        slot track width=fill, height=8 {
            slot progress automatic=length by $value in $min..$max { Rectangle color=blue }
        }
    }
}
style Half for=Progress, slot.progress.width=50     // the value drives this width
Progress download .Half 200x8
```

## G3005 — one screen vocabulary per target

A node posed from both a `screen.<class>` and a `screen.<orientation>`
block: both can hold at once and would fight. Compose them in one
condition or pick one vocabulary.

Wrong:

```kdl error=G3005
when screen.landscape { info.opacity = 0.5 }
when screen.lotsOfSpace { info.opacity = 1 }       // both can hold at once
Label info text="rotate me"
```

Fix:

```kdl
when screen.landscape screen.lotsOfSpace { info.opacity = 0.5 }
Label info text="rotate me"
```

## G3006 — style targeting / theme-entry shape

The style/theme bookkeeping family: `for=` must come first and name a
control kind, `extends=` names another style (never itself), a subtree
`theme` line goes inside a node's body (top level uses a block), and a
`theme { Kind … }` entry carries props only — no name, no `on:` wiring,
no placement, no duplicates.

Two more, each a line that used to build and do nothing:

- `color = …` in a `theme` block. The page's text color is
  `font_color = …`, the word text color has everywhere else; `color` is
  a shape's fill.
- A style on a `widget` or `control` line (`widget Card .Big { … }`). The
  line declares a name and its `$params`; it is not a node, so nothing
  on it can wear a style. Write `.Big` on the first node of the body
  (every instance wears it) or on an instance line (that one does).

And two more:

- A block under a `theme { Kind … }` entry. An entry is one line of
  defaults for EVERY node of its kind; nothing reads a child, a `when`
  or a `transition` written under it. A look that answers a state goes
  in a style the entry wears (`theme { Button .Look }`).
- A `for=<control>` style written above the `control` it names. The
  style reads the control's slots, so it goes after the declaration; the
  message names the control's line.

## G3007 — property transition shape

A property transition (`transition commandBar.opacity duration=0.4`)
sits at the file's top level and names `node.prop` targets
(`commandBar.{margin, opacity}` names several). It needs `duration=`
above 0, takes only `duration=` and `easing=`, and names properties
that slide. `visibility` never slides: fade `opacity` instead.

## G3008 — one timing per property

A property is timed twice: by two property transitions, or by a
property transition plus a timed `transition A -> B` over a `when` that
sets it. The message names both lines. Keep one: drop the property from
the property rule, or move its value out of the `when`.

## G4003 — two writers on one property

A `when` holds a property while a `transition … anim=` (or the
control's `$value`) drives the same one on the same node — they would fight on every flip
and the loser flickers. The message names both writers; keep one. It
holds for a motion word (`scale`) and for a look property (`background`)
alike.

Wrong:

```kdl error=G4003
anim press { newGame.scale to=0.9, duration=0.1 }
style MenuButton {
    transition any -> held anim=press
    when held scale=0.95                 // and the anim drives scale too
}
Button newGame .MenuButton text="New Game"
```

Fix:

```kdl
style MenuButton {
    transition any -> held duration=0.1
    when held scale=0.95
}
Button newGame .MenuButton text="New Game"
```

## G4004 — unknown anim

`play(x)` / `anim=x` names no declared `anim x`. The name is looked up
in the enclosing widget first, then among the page's anims, and the anim
may be written above or below the line that names it.

A widget's own anim is not visible from outside the widget: `play(pop)`
on a page cannot start the `pop` a widget declares.

## G5001 — widget naming / `for=` host contract

A widget or control cannot wear a built-in kind's name
(`control Slider` — the built-in would be shadowed) or a gogo keyword
(`widget each`, `widget state`, `widget held`, `widget fill`): the name
would be read as the keyword wherever an instance is written. A
`for=Radio` widget dresses a Radio: used outside one, inside the wrong
control, or with widget-scope `when`/`transition` and no `for=`, the
contract breaks — the message names which way. On the `widget` line
`for=` comes first, before the `$params`.

## G5002 — unknown style

`.Name` names no declared style. The message suggests the nearest one.

Wrong:

```kdl error=G5002
Button ok .NoSuchStyle text="OK"
```

## G5003 — widget cycle

`A` contains `B` contains `A` — the expansion would never end. The
message spells the cycle; break one link. (A widget MAY fill its own
slot from an instance site — that pops one level and ends.)

## G5004 — a slot fill that would never show

An instance fills a widget's slots: `slot header { … }` fills the slot
named `header`, and bare children go to the widget's default slot (the
bare `slot` line in its body). Without the check each of these builds
and shows nothing, or shows twice:

- A fill for a slot the widget does not have (`slot heder { … }`).
- Bare children on a widget with no default slot.
- A word on the fill's line (`slot header spacing=4 { … }`). A fill is a
  name and a block; the slot's size and look are written on the slot's
  own line in the widget.
- One slot filled twice.
- A second default `slot` in the widget's body. Bare children go to one
  place.

Wrong:

```kdl error=G5004
widget Card {
    Panel {
        Column {
            slot header { Label text="Untitled" }
            slot
        }
    }
}
View page {
    Card stats {
        slot heder { Label text="Run stats" }
    }
}
```

Fix:

```kdl
widget Card {
    Panel {
        Column {
            slot header { Label text="Untitled" }
            slot
        }
    }
}
View page {
    Card stats {
        slot header { Label text="Run stats" }
    }
}
```

## G5105 — an automatic slot in a host that measures children

A driven slot inside a Column/Row would re-lay out the whole stack on
every value change; driven layout stays relayout-local — put the slot
in a free parent (Layers).

Wrong:

```kdl error=G5105
control Meter $value=0, $min=0, $max=100 {
    Column { slot progress automatic=length by $value in $min..$max { Rectangle color=blue } }
}
Meter m 200x8
```

Fix:

```kdl
control Meter $value=0, $min=0, $max=100 {
    Layers { slot progress automatic=length by $value in $min..$max { Rectangle color=blue } }
}
Meter m 200x8
```

## G5106 — a position slot takes its width from its content

`width=fill` content inside an unsized `automatic=position` slot has
nothing to fill and vanishes; size the slot line (`slot thumb
width=20`) or the content.

Wrong:

```kdl error=G5106
control Dot $value=0, $min=0, $max=100 {
    Layers {
        slot track width=fill, height=8 {
            slot thumb automatic=position by $value in $min..$max { Rectangle width=fill, color=white }
        }
    }
}
Dot d 200x8
```

## G5107 — `automatic=` driver coupling

The driver set is closed (`position`, `length`, `presence`, `arc`,
`angle`; the last two take their aperture in parentheses:
`automatic=arc(from=-135, sweep=270)`); `by` couples it to a value or
state and `in a..b` gives the value's range — each form's message names
what is missing or extra.

## G6001 — undefined `$ref`

No theme value or widget parameter of that name is in scope. The
message lists or suggests what is.

Wrong:

```kdl error=G6001
Label title text="Hello", font_color=$acent
```

## G6002 — `$value` defined twice

One name, one definition per scope (per variant in theme blocks).

## G6003 — not a palette color

A palette override's target is not a palette word.

## G6004 — a palette override needs a color

`theme { red = … }` takes a color — hex, another palette word (a
link: `red = green` moves `red` AND its aliases), but never a theme
`$value`.

## G6005 — a circular palette override chain

`red = green` and `green = red` chase each other; break the loop.

## G6006 — a palette override under a variant other than `dark`

Only `dark` has a second color column (`theme dark { surface = … }`
beats the dark half). A variant-scoped color is a `$value` — those are
already variant-gated.

## G7003 — a file has one root

One `View` (a screen) or `Panel` (a fragment) at top level; everything
else lives inside it.

Wrong:

```kdl error=G7003
Column menu {
    Button a text="One"
}
```

Fix:

```kdl
View menuPage {
    Column menu {
        Button a text="One"
    }
}
```

## G7011 — unknown node type

No kind, widget or registered code-control wears this word. The message
suggests the nearest.

## G7012 — `View` is the screen root only

Inside the tree — and as a widget's body — use `Layers` (free
placement) or `Panel` (a styled box).

## G7013 — a code-drawn control (warning)

The host declared this word as code-drawn; gogo lays out an empty box
of its declared size and the host paints it.

## G7014 — a child inside a shape

A Rectangle, a Circle, an Arc, a Gradient and an Image draw themselves
and hold nothing. A child written in a shape's block would be placed
over the shape, and a shape with no size of its own would not grow to
hold it. Put the two side by side in a `Layers` (they stack), or use a
`Panel`, which draws a plate and holds content. A shape's block still
takes its `when` and `transition` lines.

Wrong:

```kdl error=G7014
View page {
    Rectangle badge 80x24, color=red {
        Label text="New"
    }
}
```

Fix:

```kdl
View page {
    Panel badge 80x24, background=red {
        Label text="New"
    }
}
```

## G8001 — a translation key carries different English

One key, one English string, across the whole build — the message
names the file that used it first. Also refused: a key on a theme
value (`$title = "…"/appTitle`) — keys live on displayed text, where
the translation actually shows. `/_` is not a key and never counts
(two `/_` strings may differ; a theme value may say `/_`).

Wrong:

```kdl error=G8001
Label a text="Save"/save
Label b text="Save it"/save
```

## G8002 — displayed text with no translation key

Only when the host asks for it (`CompileOptions::require_keys`, off by
default). Every string a page shows needs a key, so no English-only
text ships in a translated app:

```kdl partial
Label text="Privacy"                    // error[G8002]
Label text="Privacy"/settingsPrivacy    // a term
Label text="Leon Shaw"/_                // stays English on purpose
```

A whole file that stays English (a dev page, release notes) starts with
the line `allow untranslated`. Not checked: paths and picture addresses
(`res://…`, `collage://before`), `areas=` rows, and text with no letter
(`"100%"`, `"1/1"`). A string in a `uses` file is reported with that
file's name, and only that file's `allow untranslated` covers it.

## G8003 — `allow` takes one word

`allow untranslated` is the only form, and it is a line at the top of a
file — not a node inside a body.

## G8004 — a translation key not in this build (hot reload)

Same rule as G2021 for `/key`: a reload shows text through the key table
the app was built with, so a key it never baked has no string in any
language. Rebuild once to add the key. Changing the English of an
existing key needs a rebuild too: the reload refuses it as G8001, because
the table still holds the old string.

## G9002 — repeat/slot clause shape

`as $binding` names the item binding (sigil and all, no dots), `as`
comes before `by`, and each clause is written once. `by` is optional: a
repeat without it is a counted one, filled by the host's `set_count`.
A repeat has a name (`repeat tasks …`): the name is how code hands it
its rows, so one without a name could never show any.

Wrong:

```kdl error=G9002
Column { repeat tasks as task by $task.id { Label text="x" } }   // `as $task`
```

## G9003 — `by` keys the item binding

`by $item.<field>` (or `by $item` for plain values) — a field path on
the binding, nothing else.

Wrong:

```kdl error=G9003
Column { repeat tasks as $task by $other.id { Label text="x" } }
```

## G9004 — the item binding shadows a `$` value

`as $accent` next to a theme `$accent` would make every use ambiguous;
rename the binding.

Wrong:

```kdl error=G9004
$item = 3
Column { repeat tasks by $item.id { Label text="x" } }
```

Fix:

```kdl
$item = 3
Column { repeat tasks as $task by $task.id { Label text="x" } }
```

## G9007 — `empty { }` block rules

The zero-items view holds ONE child (wrap several in a Column or
Layers) and takes nothing on its own line. A repeat has one, written
first, before the row. Outside a repeat or a List an `empty { }` is
refused: nothing would ever hide it.

Wrong:

```kdl error=G9007
Column {
    repeat tasks by $item.id {
        empty { Label text="none"; Label text="yet" }
        Label text="x"
    }
}
```

Fix:

```kdl
Column {
    repeat tasks by $item.id {
        empty { Column { Label text="none"; Label text="yet" } }
        Label text="x"
    }
}
```

## G9008 — a dotted `$name` in a definition

A `$` definition or a widget parameter whose name holds a dot
(`$brush.size = 3`, `widget W $a.b=1`). `$x.field` reads a repeat item;
theme values and params are flat. (Reading a dotted `$ref` outside a
repeat is G6001.)

Wrong:

```kdl error=G9008
$brush.size = 3
Label t text="x"
```

## G9010 — a List holds exactly one `repeat`

None: statics alone are a `Column`'s job. Two: the List's selection and
rows are one roster.

Wrong:

```kdl error=G9010
List picker {
    empty { }
    repeat today by $item.id { ListItem { Label text="x" } }
    repeat later by $item.id { ListItem { Label text="y" } }
}
```

## G9011 — a List row's root is a `ListItem`

The one tap-and-select surface per row; wrap other content in one
(`ListItem { Label … }`), and give the row ONE root.

Wrong:

```kdl error=G9011
List picker {
    empty { }
    repeat tasks by $item.id { Label text=$item.title }
}
```

Fix:

```kdl
List picker {
    empty { }
    repeat tasks by $item.id { ListItem { Label text=$item.title } }
}
```

## G9013 — `ListItem` belongs at a List repeat's root

Anywhere else — a static child, a plain repeat, nested in another
row, inside `empty { }` — a `Panel` is the box you want. A List also
has no slots.

Wrong:

```kdl error=G9013
Column { ListItem { Label text="not in a list" } }
```

## G9014 — a `when selected` below the row's root never fires (warning)

The List checks the `ListItem`; style the root, or watch it
(`the row's own state`). A Toggle's own `when selected` inside a row is
its own business and stays quiet.

Warns:

```kdl warning=G9014
List picker {
    empty { }
    repeat tasks by $item.id {
        ListItem { Label title text=$item.title { when selected font_color=red } }
    }
}
```

Fix:

```kdl
List picker {
    empty { }
    repeat tasks by $item.id {
        ListItem { when selected background=accent; Label title text=$item.title }
    }
}
```

## G9015 — a viewport List its ancestors provably starve (warning)

A Scroll, or an anchored/hug stack chain, hands the viewport List its
minimum — near zero — and the rows vanish. Write `height=` on the List
(a sized box) or `height=hug` (a content-sized menu; the page then
scrolls as one).

Warns:

```kdl warning=G9015
Column menu valign=center {
    List picker { empty { }; repeat tasks by $item.id { ListItem { Label text=$item.title } } }
}
```

Fix:

```kdl
Column menu valign=center {
    List picker height=hug { empty { }; repeat tasks by $item.id { ListItem { Label text=$item.title } } }
}
```

## G9016 — a List declares one `empty { }`

The zero-items view lives at the LIST level, once — beside the repeat,
not inside it. Bare is fine: `empty { }`.

Wrong:

```kdl error=G9016
List picker { repeat tasks by $item.id { ListItem { Label text=$item.title } } }
```

## G9017 — `layout` block rules

`layout <Kind> { … }` picks the rows' container inside a `List`: the
kind set is closed (Row, Flow, Tiles, Column), the block holds the
rows' `repeat` only (statics stay on the List body), and a Row strip
caps with `max_width=`, not `max_height=`.

Wrong:

```kdl error=G9017
Column { layout Flow { Label a text="x" } }
```

## G9018 — a `repeat` inside another repeat's row

Code fills a repeat by its name, once for the page. Inside another
repeat's row every outer row would hold a repeat with that one name, and
none of them could be given rows. Put the data into one repeat: a header
item and its member items in one list, each kind of row drawn by what
its fields say.

Wrong:

```kdl error=G9018
Column {
    repeat groups by $item.id {
        Column {
            Label text=$item.title
            repeat members by $item.id { Label text=$item.name }
        }
    }
}
```

## G9101 — `set_items` refused whole (runtime)

The repeat is not keyed, or one item cannot fill the row: a field the
row reads is missing, or holds another type than its property takes (a
whole number in `text=`, text in `color=`, a word the property does not
have) — the assignment is refused WHOLE and the last good rows stand.
The message names the item, the field and the property.

## G9102 — item key shape (runtime)

Keys are strings or whole numbers, never floats, never mixed types in
one assignment.

## G9103 — duplicate item key (runtime)

Two items carry one key; every row keys uniquely. The message names
both positions.

## G9104 — `selected` write refused (runtime)

`select=off` has no selection to write; `select=one` takes at most one
key; duplicate keys are refused. The last good selection stands.
