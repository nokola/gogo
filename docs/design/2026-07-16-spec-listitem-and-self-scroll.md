# List: self-scrolling + the `ListItem` row root (D121)

Designed and built 2026-07-16 (user calls, same day). Amends §5 of
`2026-07-15-spec-repeat-and-list.md` — everything else there stands.

## Why (the user-caught bug cluster, D121)

Three reports against the D118 List, one root system:

1. **"Focus styling overrides selected item style."** Godot 4.7's default
   theme ships a `focus` stylebox for `Label` (a 2 px light frame) — a
   focused Label row drew ENGINE chrome over the authored `when checked`
   look. Gogo's own ladder was correct (checked outranks focus); the frame
   was a second, unauthored styling channel.
2. **"List does not scroll data out of view."** v1 said "compose with
   `Scroll`" — but every real list scrolls, the WP author's mental model of
   a list INCLUDES the viewport, and the planned virtualization needs List
   to own its viewport anyway. Composition was going to break at that point.
3. **"No selection visible with a `Column` row."** The List drives
   `checked`/focus/taps on the row ROOT; a style worn by a node INSIDE the
   row never fires — a silent no-op. And a `Column` can't take
   `background=`, so there was no legal seat for the selection look.

## The calls

- **List scrolls itself.** The content stack rides an internal GogoScroll
  (WP pan/fling/spring + native wheel/scrollbar) with `follow_focus` on.
  Statics scroll with the rows. Wrapping a List in a `Scroll` is an
  anti-pattern now. No `scroll=off` opt-out until a real consumer asks.
- **The box is the viewport**: an UNPLACED List expands to the space its
  parent offers (`size_flags_vertical = EXPAND_FILL`; its own minimum is
  the scroll's, near zero — WPF ListBox). Any authored placement keeps
  full authority (`height=200` means 200).
- **`ListItem` is the required row root — always** (G9011 names the wrap
  fix-it; misplacement is G9013, lexical at build). It is WPF's
  ListBoxItem made explicit — the spec's "no implicit wrappers" doctrine
  applied to the item container. Names considered: `Focus` (user's first
  sketch — rejected: collides with the `focus` state keyword, and the node
  owns more than focus), `Item` (rejected: "item" would mean both the node
  and the `$item` binding). `ListItem` says its scope, starts the
  `TabItem`/`MenuItem` family convention, and is one letter-swap from the
  WPF concept.
- **What ListItem is**: a script-backed PanelContainer
  (`controls/list_item.gd`) — a drawn box like `Panel` (blank slate,
  `background=`/`padding=`/`corner-radius=`/`border-*=`), STOP mouse
  filter (ONE input channel for rows — before: Label rows IGNORE +
  root-level hit-test fall-through, Column rows PASS, Button rows STOP),
  no engine focus chrome (gogo-owned class ⇒ `when focus` is the only
  cursor styling — the Metro answer), and a legal `pressed` surface
  (list_root's press tracking flips it through the row's condition
  machine; release-anywhere and pan-cancel disarm). Multi-part rows lay
  out with a `Row`/`Column` inside — Panel semantics, one story.
- The old fall-through hit-test channel in list_root is DELETED — with
  every row a STOP ListItem it was dead code.

## Diagnostics

| Condition | Code |
|---|---|
| row template root isn't a `ListItem` (or ≠ 1 root) | G9011 (reworded) |
| `ListItem` outside a List's repeat: statics, plain containers, non-List repeats, nested in a row, `empty` content | G9013 (new; lexical at build + `_li_seat` runtime belt in `_instantiate`) |

## Traps folded into the design

- `when checked` on a node INSIDE the row still never fires (the List
  drives the root) — but now the row root legally takes the look, docs say
  so, and the G9011 fix-it points there. A dedicated lint stays open as
  future work.
- A List inside a scrolling page (`Scroll { Column { List } }`) gives the
  List unbounded height → it collapses to its minimum; give it `height=`
  there. (The WPF ListBox-in-StackPanel gotcha, inverted.) D123 softened
  this whole trap class: the collapse now reports itself at runtime, and
  `height=hug` is the sanctioned menu-sized answer.
- Gesture arbitration between a List's internal pan and an outer GogoScroll
  is the pre-existing "two containers panning" not-in-v1 edge — unchanged.

## Amendment: default looks (D122, user call 2026-07-16)

User-caught, same day: "list does not have check background by default and
looks broken on first use." An unstyled row had no condition machine at all
— selection, cursor and press were all invisible until the author wrote
`when checked` themselves. Options discussed (default look / rustc-style
lint / opt-in shipped look); the user picked the default look as scoped:

- **`checked` + `focus` get theme-derived defaults**; `hover` stays
  opt-in. Palette: the theme's ItemList `selected` fill for checked
  (the engine's own list-selection color — an app theme restyles gogo
  lists for free), a quarter-strength echo for the cursor (the engine's
  `focus` box is a border frame — not expressible on the row's one `@bg`
  channel — and its `hovered` fill sits at ~¼ of `selected`). Statics on
  `GogoListItem`; fallback = the 4.7 default values (probe-verified).
- **`pressed` joined the default set the same day** (follow-up user call):
  with press feedback opt-in, the first shade an author ever saw was the
  selection fill AT RELEASE — user-caught as "the pressed shade still
  stays" (the probe showed nothing stuck: a release inside the full-width
  row completes as a tap, and the fill was select=many's toggle). The
  default is a half-strength fill while the press rides — press beats
  checked on the ladder, so an unselected row shades up and a selected one
  dims; release/pan-cancel disarm through the already-pinned D121 paths.
  All select modes take it — WP menus flash on press too.
- **Per-state replacement**: an authored `when checked` (node or style)
  kills only the checked default; `when focus` only the focus one. No
  merging. `select=off` rows skip only the checked default — menus still
  flash on press.
- **Mechanism**: the resolver injects ordinary condition-machine defs
  (`_push_li_default_looks`, pushed after `_collect_states`' applier —
  same batch, so `has_condition()` is the whole per-state test; all
  pre-`_ready`, so wiring/polls see the merged defs). Values are Callables
  resolved at flip time — rows build detached, and only a live read
  honors ambient themes (`GogoStates._final_value` learned the branch).
  Everything rides the normal PRIORITY ladder: pressed still beats
  checked, checked beats focus (the selected row keeps the full fill).
- **Doctrine**: not new magic — Button already looks alive by default
  because the engine theme styles it; ListItem only missed out for being
  gogo-scripted with no theme entry. Documented on the ListItem card,
  spec §States, REFERENCE.

## Amendment: press/cursor feel + `height=hug` + the starve nudge (D123, user calls 2026-07-16)

Two more user-caught rounds the same day, both hands-on in test5.

**The wander residue.** Press a bare row, move away, release: a shade
stayed. Nothing was stuck — the press had parked ENGINE focus on the row
(the viewport's click-focus), and D122's always-on cursor echo made the
parked cursor visible for the first time. Two feel rules fix the read:

- The default press shade mirrors "release here would tap": a held-move
  event routes to the pressed row (mouse focus), and the shade follows the
  point being inside/outside the row rect — slide off clears, slide back
  re-arms, pan-cancel's away-motion clears the moment a pan wins.
- The default cursor echo obeys FOCUS-VISIBLE (the UWP/web answer): defs
  carrying `kb_only` skip `_retarget` unless the cursor's last mover was
  the keyboard. The flavor is a GogoStates static fed by list_root — it
  sees every row press and every arrow BEFORE the viewport navigates, and
  every retarget re-reads the flag, so a press on a stale flag corrects
  itself within the same input delivery. Authored `when focus` is never
  gated — styling it is the author explicitly asking for every focus.

**The starved menu List.** `Column menu valign=center { ... List }` showed
nothing: a centered Column is hug-sized, hug parents ask children for
MINIMUMS, and a viewport List's minimum is near zero by doctrine. "Fill
the available space" is circular there — the column's size derives from
the list's answer. Options discussed; user picked A+C:

- **A — `height=hug`**: the box IS the content. Consumed by the List arm
  (the `select=` shape — the word never reaches placement); the root
  reports the content stack's minimum, the D121 expand default stays off.
  Menu grows with its items; explicitly non-scrolling past the screen (the
  WPF ListBox-in-StackPanel shape, chosen instead of stumbled into).
  Reuses the Grid-track word — one keyword per concept.
- **C — the runtime nudge**: a starved viewport List (unplaced, items
  present, laid out to <1 px two frames after first in-tree data) prints
  one `gogo:`-prefixed warning naming all three fixes. Runtime sees what
  build-time linting can't (sizing is a layout fact); the strict suites
  treat `gogo:` lines as failures, so a starved list in a shipped screen
  is test-visible rot, not silence.
- **B — content-sized by DEFAULT (WPF semantics) — rejected**: Godot
  minimums are hard floors with no "available size" input (WPF's Measure
  gets one). A content minimum would overrule authored `height=` and
  un-scroll every list taller than its screen — it kills self-scrolling
  exactly where lists matter most.

## Amendment: `max-height=` bounds the hug (D124, user call 2026-07-16)

The centered-menu arc, asked for directly: "the whole structure centered;
as I add items and New Game reaches the top, the list just starts showing
as a scroll." Neither existing word covers the full arc — `height=hug`
grows forever (overflows past the screen), numeric `height=` never grows.
WPF has the same hole (infinite-measure parents give a ListBox no bound;
the canonical fix is `MaxHeight`), so the word is parity, not patchwork.

- `height=hug, max-height=400`: minimum = min(content, cap). The box
  follows the items to the cap, then stops — and content > box already
  scrolls (D121's internal viewport), so the scroll phase costs nothing.
- The cap converts epx→px LIVE in `_get_minimum_size` (the root carries
  the authored epx value + the resolver's converter) — units changes need
  no re-apply binding.
- `max-height=` REQUIRES `height=hug` (build error with the fix spelled
  out): a plain viewport List already takes exactly what its parent
  offers — capping the parent's offer is `height=`'s job, and a silent
  implied-hug would surprise ("why did my list shrink to its items?").
  An error can become sugar later; sugar can't become an error.
- The screen-relative form (`max-height=fill` — cap from the ancestors'
  actual space) was discussed and parked: it retrofits WPF's bounded
  measure onto one control, with stale-minimum-cache and oscillation
  hazards. The epx constant covers the menu case; revisit only if real
  screens make the constant annoying.

## Amendment: fixed-content minimum + the lint pair (D127, user calls 2026-07-16)

User-caught ("scenario 4, may be a bug"): a starved List with an `empty`
block hid the "Nothing due today" text too — the whole box was minimum-
zero, so the parts that are FIXED AUTHORED CONTENT (the header static, the
empty text) vanished with the rows. The user's framing: "half-starved —
the values are starved, but at least the empty text should display, and we
get a lint for starved values." Approved with the full lint set.

- **Fixed content sizes the box; rows never do.** The List's minimum (the
  plain viewport form) = visible non-row children of the stack, stacked
  with separations — rows are identifiable by class (statics and empty
  content can never be `ListItem`, G9013). A bare list in a centered menu
  shows its header and "none yet" line out of the box; the viewport
  doctrine (and D124's rejection of content minimums) still governs the
  unbounded part. An authored `height=` smaller than the fixed content is
  floored by it — standard layout, same as an over-full Column.
- **The nudge learned "half-starved"**: trigger = items present and less
  than ONE ROW's height of room beyond the fixed-content minimum (a hug
  parent holds the old empty-text height, which the EXPAND flag backfills
  as a sliver — a +1px threshold read that sliver as "room").
- **G9014** (lint, advisory): `when checked` on a node INSIDE the row —
  directly or via a worn style — never fires; the List checks the ROOT.
  Toggle-family buttons (BaseButton-backed) and widget/control/composite
  wearers stay quiet (they may drive their own checked).
- **G9015** (lint, advisory): build-time starvation where the chain PROVES
  it — the parse-ancestry walk (a `_node_stack` bracketing `_build_node`)
  warns on `valign=`-anchored Column chains and `Scroll` pages; a sized
  Column ends the walk quiet; `Row` (siblings can give it height),
  `Layers`/`Panel`/`Grid`/widget seams bail QUIET. Conservative because
  ui-lint FAILS on warnings — a lint that cries wolf rots unheard. The
  no-data editor preview is where this earns its keep: the runtime nudge
  needs items, the lint doesn't.
- Both lints ride `_warn_diag` (advisory diags with carets/fix-it help;
  `_warn_diag` learned an optional code param); codes registered with
  docs/errors cards. ui/test.gogo — the conversation's own menu — tripped
  G9015 on the first run and was migrated to the D124 arc
  (`height=hug, max-height=400`): the lint's first real catch.

## Amendment: `layout` panels + the List-level `empty` (D128, user calls 2026-07-17)

The user's sketch (a required `layer` wrapper naming the panel + a
list-level empty view) landed as:

- **`layout <Panel> { repeat ... }`** — the items panel, a CLOSED set:
  Column (default — block optional, the user's required-always was traded
  away: a mandatory `layout Column` carries no information a default
  doesn't) | Row (strip: horizontal pan, left/right cursor, width=hug +
  max-width transposed) | Flow | Tiles (wrap/mosaic: vertical scroll,
  arrows via the engine's geometric focus navigation — 2D for free).
  Named `layout` over the sketch's `layer` (z-collision with `Layers`) and
  over `items`/`panel`/`rows` (user call after options). The block form
  beats a `layout=` enum in one real way: panel props have a home
  (`layout Flow spacing=8` — row spacing separate from the statics').
  Grid excluded (authored tracks vs dynamic rows underdefined); the set is
  closed so hug/caps/starve/virtualization multiply against four known
  modes. G9017 fences: closed set, one layout, exactly the one repeat
  inside, nothing else.
- **`empty` moved to the List body** (G9016, required — the G9007 doctrine
  one seat up): the zero-items view shows BESIDE the statics (headers
  stay) while no items are present, hidden on data; free placement inside
  (Layers-centered art). A List's repeat carries NO inner empty — one
  concept, one seat per context; the repeat parser routes it up with a
  fix-it, plain repeats keep G9007. Runtime: the empty children build once
  as ordinary stack children and list_root toggles visibility per commit —
  simpler than the repeat's rebuild-on-crossing machinery.
- Mechanics: list_root.list_layout (renamed off the native
  Control.layout_mode — a String assignment to the engine enum failed the
  whole script's compile, caught by the first run) drives the scroll axis,
  the D94 EXPAND flag's fitted axis, the focus chain's neighbor pair, the
  fixed-content minimum (the panel node is skipped whole; a row strip's
  cross axis IS content-sized), the starve probe's axis, and the hug/cap
  words' axis. G9015's chain lint skips row layouts (they starve on width
  — runtime-only).
- Migration: every List moved its empty up one seat (script-assisted;
  plain repeats keep the inner form — the first sweep over-reached and the
  repeat fixtures caught it). The d120 slot-math pin re-anchored (the
  fixture's lines moved); the panel preview's lens helpers learned that
  hidden empty content isn't "shown".
