# D83 — the vocabulary round (2026-07-11)

Historic decision record: why the type surface looks the way it does. The
normative rules live in gogo-spec.md ("Vocabulary (D83)") and REFERENCE.md;
this file keeps the reasoning that produced them, so it never has to be
re-derived (or re-litigated) from memory.

## What happened

Gogo's control vocabulary went **framework-neutral** in one round: the
surface stopped being a WPF dialect. `TextBlock`→`Label`, `TextBox`→`Input`,
`Border`→`Panel`, `ScrollViewer`→`Scroll`, `WrapPanel`→`Flow`; `View`, `gap`,
`Circle`, `Gradient`, `choice`/`Radio`/`Check` joined; `Radial` (and its
`solid-until=`) retired into `Gradient`; `Ellipse` redirects to `Circle`.
All shipped app files migrated in the same round — zero visual change was
the acceptance bar (parity states must match baselines exactly).

## The trade that unlocked it

The WPF spellings existed for **searchability** — a porter greps XAML for
`TextBlock` and finds the gogo word. The round moved that value into
machinery instead of the surface: every old spelling errors with a
one-keystroke redirect (`RETIRED_TYPES`), and the porting skill carries the
XAML→gogo table. The surface itself now answers only to the goals (easy to
understand / write / find, readable 6 months later) — a reader who never saw
WPF loses nothing.

The census that triggered the round (shipped-XAML + app-file grep,
2026-07-11): `Border`(22) and `PanelContainer`(12) were two spellings of ONE
engine class — the 12 existed only because the root contract forced the raw
name; raw `Control`(20) wore three hats (body host / spacer / draw surface);
every view opened with the same `width=fill, height=fill,
background=#00000000` incantation; and the shipped app needs 30 Ellipses
(M12 pickers/pucks) and 30 RadioButtons (NewPaintingDialog, M12/M13) that
had no gogo words yet — the Toolbar hand-rolls radio exclusivity in C# today.

## The two laws this round wrote into the spec Goals

**Node-type stability** — *a property edit must never change what a node
IS.* Gogo compiles to scenes users script against (`get_node("bg") as
ColorRect`, tweens on typed properties); a `fill=` that silently swapped
ColorRect↔TextureRect would break casts at a distance. This is why
`Rectangle` and `Gradient` are two types rather than one type with a mode
prop, and why `Circle` is its own type. The Rust analogy that settled it:
`&str`/`String` — when the cost or character differs, the type differs, and
hiding the difference is the disservice. It also settles the conic fallback:
a build without native FILL_CONIC CPU-paints into an ImageTexture — the node
TYPE never changes with a property.

**The capitalization law** — *capitalized = an addressable node (nameable,
stylable, animatable); lowercase = structure, never named.* Found mid-round:
the first draft had a capitalized `Gap` type that refused a name — the only
type-rule exception, a memorized special case. Lowercasing it made
namelessness automatic and put it in the register it belongs to (`row`,
`skip`, `gap`, `choice`). The one impurity, documented: `gap` leaves an
anonymous mouse-ignored Control behind (Godot stacks make space with child
slots — `add_spacer()`'s own trick); being unnameable, that node is
unobservable from the language surface. Space you must ADDRESS is a layout
element, not a gap.

## Per-decision notes

- **View**: the boilerplate WAS the design — every transparent-shell view
  hand-blanked its host because the root node isn't gogo-created (the D61
  blank slate never reaches it). View bakes the three defaults in, injected
  only where the file doesn't speak (props, a bare size arg, or a style —
  per axis: `halign=` silences only the width default). Deliberately NOT
  `visible=false` by default — hidden-by-default would be hidden semantics
  (identity vs choreography). Root-only; and the reverse holds: a root that
  IS a styled box (toast banner, progress track, brush_options' plate —
  which is the host theme's panel look) stays `Panel` at root, because
  View's transparent default would erase the plate. Both are PanelContainer
  underneath; G7002 hosts stay compatible.
- **Duplicate-raw redirects**: raw engine names that exactly duplicate a
  gogo name (`VBoxContainer`, `ColorRect`, `LineEdit`, ...) redirect instead
  of building — two spellings of one node type would split every grep. The
  escape register stays for classes gogo has NO name for. Godot's raw
  `Panel` class is shadowed by the alias (user-accepted: a bare gray Panel
  was never reachable under the blank slate anyway).
- **Gradient**: four fills × three repeats × arbitrary stops × from/to
  geometry — the general mechanism `Radial` special-cased. `stops=` is
  REQUIRED (keyword-announced, no hidden default ramp). The stop rules are
  the user's, verbatim: positions optional; if one is present, all must be
  (all-or-none); increasing order with equal allowed (a hard step); under
  `repeat=mirror` positions stop at 0.5 (the reflected half). `@position`
  rides after `/opacity` (`blue/0.5@0.24`) as the second value-suffix
  qualifier. `angle=` is linear-only sugar on the CSS compass (0 = to top,
  90 = to right — the most-searched gradient-angle convention). Stops stay
  LIVE as `stop_N_color`/`stop_N_offset` real properties so whens/anims
  drive them through the standard machinery (no anim-system changes) — the
  D14 press flashes migrated as
  `Gradient fill=radial, stops=[C@0, C@0.94, C/0@1]`, exact to the old
  `color=C, solid-until=0.94`.
- **G2003 (one-stop gradient)**: machine-applicable fix to
  `Rectangle color=X` — the honesty diagnostic that falls out of having two
  types. The REVERSE nudge (two stacked Rectangles faking a fade) has no
  reliable lint signal — it lives in the COOKBOOK instead (user call).
- **Circle**: blank until told (D61 slate), so the ring spelling is exactly
  `border-color= + border-thickness=` over a clear fill — the D45 FileOps
  ring becomes one thickness number instead of three coupled stylebox
  values. Radius self-derives from the box's short side; true ellipses
  deliberately out (no shipped consumer draws one that isn't a circle).
- **choice**: exclusivity gets a **lexical home** — the erased scope (the
  `row` precedent: children parent flat, structure exists only at build)
  wires every Radio descendant into ONE ButtonGroup, however deep the
  Row/Column nesting. Radios in disjoint subtrees can't share a group — a
  feature: the reader sees the whole exclusive set in one block, where
  `group="size"` strings would be spooky action. The group IS the wiring
  surface (`On<Name>Changed(radio_name)` / `on:changed=`) — no runtime node
  exists for the choice. `Check` never joins a group; inside a choice it
  errors, routing to Radio.
- **Capping principle** (why the table ends where it does): **types name
  mechanisms, styles name looks, widgets name composites.** Divider is a
  look (`Rectangle` is honest); Switch is an M12 composite; Card/Chip are
  styles; lists are `repeat`; dialogs are patterns.

## Deferred, deliberately

- `repeat` list templating — its own design round; the expressions on-ramp.
- `Slider orientation=` — no consumer until one appears.
- Multiline `Input` (TextEdit) — no consumer.
- `Gradient resolution=` — 128×128 has no visible banding in the shipped
  uses; a knob without a consumer is noise.
- Shared size groups (`hug(name)`) — grammar room reserved (D82), waiting on
  a second consumer.
- True ellipses on Circle.

## Consumers queued when this landed

NewPaintingDialog (RadioButton ×8), M12 brush/color pages (Ellipse pucks,
radio rows), M13 effect pages. The Toolbar's C# radio logic becomes
deletable once its rows re-land on `choice` — deliberately NOT refactored in
this round.
