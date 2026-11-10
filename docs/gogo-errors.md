# gogo error codes

Every diagnostic the compiler and runtime can emit, by code. The same
list lives in code as `diag::REGISTRY` (diag.rs) — a test keeps this
file and the registry matched both ways, so a code you see on screen is
always findable here.

How to read an entry: the first line says what the message means, the
rest says how to fix it. `Wrong`/`Fix` pairs show the shortest file that
trips the error and the repaired form.

Codes are grouped in bands: G0xxx language and parsing, G1xxx placement,
G2xxx color and gradients, G3xxx styles and theme shape, G4xxx
animation, G5xxx widgets and controls, G6xxx theme values, G7xxx file
structure, G8xxx translation, G9xxx repeat and List. Retired numbers
(`diag::RETIRED`) are never reused.

## G0001 — unexpected character

A character the language has no meaning for, at the caret. Usually a
stray symbol from a paste; delete it or quote it if it was meant as
text.

## G0002 — unclosed block

A `{` was opened and the file ended before its `}`. The caret points at
the `{` that was never closed — the missing brace belongs to whoever
opened it. Indentation finds it fast: the `}` almost always goes at the
first line that dedents past the opener.

## G0003 — unterminated string

A `"` was opened and the line ended before the closing `"`. Strings
never span lines; close it or split the text.

## G0004 — unknown escape

Inside a string, `\` starts an escape and the letter after it is not
one gogo knows (`\n`, `\t`, `\"`, `\\`). For a literal backslash write
`\\`.

## G0005 — one hex color form

Colors are `#RRGGBB` or `#RRGGBBAA` — six or eight hex digits, alpha
LAST. A four-digit or `#AARRGGBB` form is refused rather than guessed
(see G2004 for why alpha-first pastes are dangerous).

## G0006 — `$` must be followed by a name

A `$` alone or before a non-name character. Theme values and widget
parameters are `$word`.

## G0007 — `/opacity` is 0..1

The `/` suffix scales a color's alpha and takes a fraction:
`black/0.5`, not `black/50`.

## G0008 — `@pos` is 0..1

The `@` suffix places a gradient stop and takes a fraction along the
run: `white@0.24`.

## G0009 — bad number

Digits that do not form a number (two dots, a lone `-`, …). Rewrite the
number.

## G0011 — unexpected `}`

A `}` with no open block to close. Count braces upward — an extra one
usually rides a deleted block.

## G0012 — `$name needs = value` here

At top level a `$name` line is a definition and must assign:
`$accent = #FF6A00FF`. To USE a value, write `$name` in a property
position instead.

## G0013 — expected a node type or `$value`

A line starts with something that is neither a node kind (`Button`,
`Column`…), a directive (`theme`, `style`…) nor a `$definition`. Most
often a typo in the kind word — the message suggests the nearest one.

## G0014 — expected end of line

The line's form is complete but more tokens follow. Usually a missing
comma earlier made two properties run together.

## G0015 — a comma before `{`

The property list ends before the body: `Button b text="x" { … }`, not
`Button b text="x", { … }`. Drop the trailing comma.

## G0016 — `->` needs a state name

A transition arrow with a missing end: `transition base -> held …`.
Both ends are state or arrangement words (`any` matches all).

## G0017 — missing `,` between names

Two properties or names ran together without a comma. The message names
the token where the list broke. (A reserved flag like `signals` is its
own delimiter — a comma after it is legal but not required.)

## G0018 — duplicate parameter / property

The same key appears twice on one line or one widget head. The second
would silently win or lose — say it once.

## G0019 — missing `)`

A call `name(…` reached the end of the line unclosed.

## G0020 — missing `]`

A list `[…` reached the end of the file unclosed.

## G0021 — expected a value

A property position holds something that is not a value (a stray
symbol, a keyword). Inside calls, named arguments are legal grammar:
`arc(from=-225, sweep=270)`.

## G0022 — `key = …` is only legal inside a `when` block

A bare `name = value` line at top level or in a node body. Assignments
pose properties in `when` blocks; a definition needs the `$` sigil; a
node property is `key=value` on the node's own line.

## G0023 — `uses` path rules

`uses` takes one quoted path, and a path never carries a translation
key. Write `uses "widgets.gogo"`. A nested `uses` resolves against the
INCLUDING file's directory (`sub/widgets.gogo` writing
`uses "inner.gogo"` loads `sub/inner.gogo`; `..` climbs).

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
(`fill/0.5`). Move the suffix to the color it belongs to.

## G0040 — `@pos` / a list where none belongs

A stop position or a `[list]` outside `stops=`. Positions mean nothing
off a gradient.

## G0041 — retired placement words

Placement words from older drafts (the message names today's word).

## G0043 — unknown flag

A bare word in a node's property list that is not a known flag
(`signals` is one; most words need `=value`).

## G0044 — node name used twice

Two nodes share a name in one scope; `find`, responses and anims could
target either. Rename one.

## G0045 — `gap` takes one size word

`gap 12` or `gap fill` — nothing else rides the line, and a gap IS the
space: it holds no children.

## G0046 — a reserved word cannot name a node

`base`, `any`, value words and property words cannot be node names —
`transition base -> held` next to a child called `base` would read as
two things.

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
bare handler name for the host.

## G0054 — a `when` block sets properties

Inside `when`: `opacity=0.5` or `node.prop = value` — never `$theme`
definitions, never nested nodes. An empty top-level `when s { }` is
refused too (it can never do anything), and two blocks on ONE condition
may not both set the same property — the loser would silently vanish.

## G0055 — `=host` is gone

The old host-axis words; a pure layout needs no host axis (§7 of the
spec).

## G0056 — a size is a number, `fill`, `hug`, `fill(n)`

A size key got a word outside the size vocabulary, or a `$value` that is
not a number. (A numeric `$value` is fine: `height=$backBoxHeight`.)

## G0057 — cross-axis alignment word

An alignment word that does not exist on this axis (`halign=top`).

## G0058 — a number key given a non-number

`spacing=big` — the key wants epx. (A numeric `$value` is fine:
`min_width=$rowFont`.)

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

## G0062 — `after=` is not supported yet

The addon's `after=<group>` anchors have no Rust counterpart yet.

## G0063 — anim lane shape

An anim line drives `<node>.<motion word>` — translate.x, translate.y,
scale, scale.x, scale.y, rotate, opacity. `slot.<name>.<channel>` lanes
belong to a control's OWN scoped anims. And an anim holds journeys,
never assignments: `box.opacity from=1, to=0, duration=0.2` — a bare
`box.opacity = 0` would snap with no track to play.

## G0064 — an anim lane needs `to=`

A journey without a destination; `from=` is optional (the current value
fills in), `to=` is not.

## G0065 — `easing=` takes a name

An easing word (`cubic_out`, `spring`, …), not a number.

## G0066 — `repeat=` takes `forever`

A one-shot anim just omits it; there is no repeat count.

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

Image and font paths are explicit: `src="res://icons/save.png"`. A bare
file name could be a label or a path — the scheme says which.

## G0072 — `on:<event>` fires only on a press surface

`activate`/`press`/`release` live on Button, Check, Toggle, Radio,
ListItem (and the List that owns them; a Slider brackets its drag with
press/release). `change` on a Radio belongs on the `choice` line.
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

`script`, `state`, `state_builtin`, `signal` describe a `control`'s
code-behind; a `widget` has none. Under `state`, a built-in's name is
also refused — `state hover` claims the engine's state; declare
built-ins with `state_builtin`, your own words with `state`.

## G0078 — a bool key / overlay rules

A true/false key got something else, or an `overlay` box's `fill` axis
sits under a hug parent that nothing sizes.

## G0079 — unknown unit on a number

Numbers take `epx` (the default — just write the number) or `px` /`%`
in the retired-forms sense of G0038; anything else is a typo.

## G0080 — `round()` goes inside live()

`round($value, step)` picks a live number's display step:
`text=live(round($value, 1))`. Outside `live()` it has nothing to
format.

## G0081 — `bind()` is gone

The old binding call; write `live($value)`.

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
is refused here too, naming `behavior` (a dropped line left a control
deaf, todo-bugs.md #4).

## G1002 — grid track bookkeeping

The row form must square with `cols=`: each `row { }` fills the track
count (a `skip` covers one), spans reach only declared tracks, weights
ride tracks not cells, and structure words never come from a style.

## G1003 — areas-form rules

`areas=` strings draw the lattice: every row string lists one word per
track, a name marks a RECTANGLE (no L-shapes), `.` is a hole, `:
height` after a row string sizes it, and each named area binds one
child by name. The areas and row forms never mix in one Grid.

## G1004 — `wrap=true` with no width to wrap at (lint)

A wrapping label whose width nothing determines wraps at infinity —
give it a width or put it in a sized box.

## G1006 — both margins and an exact size over-constrain the axis

`width=200, margin_left=10, margin_right=10` says three things about
one axis; the box cannot honor them all. Drop one.

## G1010 — `height=fill` but no row takes the leftover (lint)

A Grid told to fill whose rows are all content-sized leaves dead space;
give a row the leftover (`: fill`) or hug the Grid.

## G1011 — an aligned axis with `fill` leaves nothing to position

`halign=left, width=fill` — a filling box has no room to be left OF.
Pick one.

## G1013 — `min_width` above `max_width`

The floor is above the ceiling; no size satisfies both.

## G1014 — margins under a Column/Row/Flow/Grid

A stack parent seats its children — margins there never bind, posed or
declared; space children with `spacing=` on the parent or a `gap`
between them. The same rule refuses `margin=` in a pose (a pose moves
one edge at a time — write `margin_left=`/…) and placement words under
an input state's `when` (placement answers the view's conditions —
`when <arrangement>`, `when screen.*`; a held box re-anchoring would
fight the finger).

## G1015 — a seat declaration's shape

`seat name` declares a named, empty socket inside a container; content
and props ride the occupant, not the socket.

## G1016 — a seat move's rules

`when <state> { node.seat = seatName }` at view level moves a node into
a DECLARED seat: the target must be a seat, a node cannot move into its
own subtree, one occupant per seat, one writer per node, and the
declared home is where it rests.

## G1017 — `aspect=` contract

`aspect=` relates the two axes, so exactly ONE spelled axis drives:
both sizes spelled leaves nothing to derive (the ratio IS the second
size), a hug axis is content-sized, a `max_*` cap would break the
ratio, and a stack/grid parent owns both axes (aspect needs a free
parent — Layers, View, Panel). The ratio is positive width/height;
`aspect=natural` reads a bitmap's own ratio and lives on Image only.

## G1018 — a List's cap rides its hug

`max_height=` on a List bounds `height=hug` (the box grows with its
rows to the cap, then scrolls); on any other height the cap would fight
the viewport. A `layout Row` strip runs sideways: its cap is
`max_width=` on `width=hug`.

## G2001 — unknown property

No kind takes this word. The message suggests the nearest real one.

## G2002 — another dialect's or another kind's word

The word exists but not HERE: a Grid cell word off its lattice, a
XAML/Godot spelling, a `Scroll`-only word on a box. The message names
where it belongs.

## G2003 — a one-stop gradient renders as a solid

One color is a `Rectangle color=…` — the message spells the rewrite.

## G2004 — invisible pigment

A hex color whose ALPHA channel reads zero because an `#AARRGGBB` paste
put alpha first (gogo is `#RRGGBBAA`, alpha LAST). The box would be
invisible with no other symptom.

## G2005 — `stops=` takes the gradient's color list

`stops=[white, black]` spaces evenly; `@positions` pin entries and are
all-or-none, never decreasing.

## G2006 — `transparent` in `stops=`

Straight-alpha blending fades THROUGH a color's hidden RGB; spell which
one: `white/0` or `black/0`.

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
(one of the two), and its words are `cell=`, `edge=` and the `edge_left`
/ `edge_top` / `edge_right` / `edge_bottom` family. Its block holds only
`slice` lines; a grid (`cell=`) has no block.

Wrong: `atlas icons cell=48x48` · Fix: `atlas icons source="res://icons.png", cell=48x48`

## G2012 — an atlas picture or file the build cannot read

An atlas reads its picture's size from the file header (and `from=`
reads the JSON), so the file must exist, sit outside any `.ignore`
fence, and be a PNG or JPEG. A standalone compile (a test, hot reload)
needs `CompileOptions::assets` to read files; `build_pages` passes one.

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

```
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
`CompileOptions::refuse_new_keys_and_pictures` (`gogo::reload`).

## G3002 — an automatic slot property assigned

`automatic=` couples the slot to the control's value — the control
drives that geometry; writing it by hand would fight the driver.

## G3005 — one screen vocabulary per target

A node posed from both a `screen.<class>` and a `screen.<orientation>`
block: both can hold at once and would fight. Compose them in one
condition or pick one vocabulary.

## G3006 — style targeting / theme-entry shape

The style/theme bookkeeping family: `for=` must come first and name a
control kind, `extends=` names another style (never itself), a subtree
`theme` line goes inside a node's body (top level uses a block), and a
`theme { Kind … }` entry carries props only — no name, no `on:` wiring,
no placement, no duplicates.

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

A `when` holds a property while a `transition … anim=` (or `live()`)
drives the same one on the same node — they would fight on every flip
and the loser flickers. The message names both writers; keep one.

## G4004 — unknown anim

`play(x)` / `anim=x` names no declared `anim x`.

## G5001 — widget naming / `for=` host contract

A widget or control cannot wear a built-in word's name (`widget gap`,
`control Slider` — the built-in would be unreachable or shadowed). A
`for=Radio` widget dresses a Radio: used outside one, inside the wrong
control, or with widget-scope `when`/`transition` and no `for=`, the
contract breaks — the message names which way.

## G5002 — unknown style

`.Name` names no declared style. The message suggests the nearest one.

## G5003 — widget cycle

`A` contains `B` contains `A` — the expansion would never end. The
message spells the cycle; break one link. (A widget MAY fill its own
slot from an instance site — that pops one level and ends.)

## G5105 — an automatic slot in a host that measures children

A driven slot inside a Column/Row would re-lay out the whole stack on
every value change; driven layout stays relayout-local — put the slot
in a free parent (Layers).

## G5106 — a position slot takes its width from its content

`width=fill` content inside an unsized `automatic=position` slot has
nothing to fill and vanishes; size the slot line (`slot thumb
width=20`) or the content.

## G5107 — `automatic=` driver coupling

The driver set is closed (`position`, `length`, `presence`); `by`
couples it to a value or state and `in a..b` gives the value's range —
each form's message names what is missing or extra.

## G6001 — undefined `$ref`

No theme value or widget parameter of that name is in scope. The
message lists or suggests what is.

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

## G7011 — unknown node type

No kind, widget or registered code-control wears this word. The message
suggests the nearest.

## G7012 — `View` is the screen root only

Inside the tree — and as a widget's body — use `Layers` (free
placement) or `Panel` (a styled box).

## G7013 — a code-drawn control (warning)

The host declared this word as code-drawn; gogo lays out an empty box
of its declared size and the host paints it.

## G8001 — a translation key carries different English

One key, one English string, across the whole build — the message
names the file that used it first. Also refused: a key on a theme
value (`$title = "…"/appTitle`) — keys live on displayed text, where
the translation actually shows. `/_` is not a key and never counts
(two `/_` strings may differ; a theme value may say `/_`).

## G8002 — displayed text with no translation key

Only when the host asks for it (`CompileOptions::require_keys`; the
app's build does). Every string a page shows needs a key, so no
English-only text ships in a translated app:

```
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
language. Rebuild once to add the key; editing the English of an existing
key reloads fine.

## G9002 — repeat/slot clause shape

`as $binding` names the item binding (sigil and all, no dots), and a
keyed repeat with no `capacity=` requires `by` (there is nothing to
count).

## G9003 — `by` keys the item binding

`by $item.<field>` (or `by $item` for plain values) — a field path on
the binding, nothing else.

## G9004 — the item binding shadows a `$` value

`as $accent` next to a theme `$accent` would make every use ambiguous;
rename the binding.

## G9007 — `empty { }` block rules

The zero-items view holds ONE child (wrap several in a Column or
Layers) and takes nothing on its own line.

## G9008 — a dotted `$ref` outside a repeat item binding

`$x.field` reads a repeat item; theme values and params are flat.

## G9010 — a List holds exactly one `repeat`

None: statics alone are a `Column`'s job. Two: the List's selection and
rows are one roster.

## G9011 — a List row's root is a `ListItem`

The one tap-and-select surface per row; wrap other content in one
(`ListItem { Label … }`), and give the row ONE root.

## G9013 — `ListItem` belongs at a List repeat's root

Anywhere else — a static child, a plain repeat, nested in another
row, inside `empty { }` — a `Panel` is the box you want. A List also
has no slots.

## G9014 — a `when selected` below the row's root never fires (warning)

The List checks the `ListItem`; style the root, or watch it
(`the row's own state`). A Toggle's own `when selected` inside a row is
its own business and stays quiet.

## G9015 — a viewport List its ancestors provably starve (warning)

A Scroll, or an anchored/hug stack chain, hands the viewport List its
minimum — near zero — and the rows vanish. Write `height=` on the List
(a sized box) or `height=hug` (a content-sized menu; the page then
scrolls as one).

## G9016 — a List declares one `empty { }`

The zero-items view lives at the LIST level, once — beside the repeat,
not inside it. Bare is fine: `empty { }`.

## G9017 — `layout` block rules

`layout <Kind> { … }` picks the rows' container inside a `List`: the
kind set is closed (Row, Flow, Tiles, Column), the block holds the
rows' `repeat` only (statics stay on the List body), and a Row strip
caps with `max_width=`, not `max_height=`.

## G9101 — `set_items` refused whole (runtime)

The repeat is not keyed, or one item broke the template (a missing
bound field) — the assignment is refused WHOLE and the last good rows
stand. The message names the item and the field.

## G9102 — item key shape (runtime)

Keys are strings or whole numbers, never floats, never mixed types in
one assignment.

## G9103 — duplicate item key (runtime)

Two items carry one key; every row keys uniquely. The message names
both positions.

## G9104 — `selected` write refused (runtime)

`select=off` has no selection to write; `select=one` takes at most one
key; duplicate keys are refused. The last good selection stands.
