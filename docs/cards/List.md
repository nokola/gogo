Selection over repeated items — the one control that owns "which one is
chosen". Its body: statics, the required `empty` view, and exactly ONE
`repeat` whose row root is a `ListItem` — arranged by the optional
`layout` panel (no block = a vertical column, `spacing=` like a Column's)
inside the List's own scroll viewport.

```
style TaskLook padding=8 {
    when selected background=accent
    when held opacity=0.7
}
List picker select=many, spacing=4 {
    Label text="Today"
    empty { Layers { Label text="Nothing due today" } }
    repeat tasks by $item.id {
        ListItem .TaskLook { Label text=$item.title }
    }
}
```

The same surface arranges any way the `layout` panel says — an inventory
bag wrapping into a grid:

```
List bag select=one {
    Label text="Inventory"
    empty { Label text="your bag is empty" }
    layout Flow spacing=6 {
        repeat loot by $item.id {
            ListItem padding=6 {
                Column spacing=2 {
                    Rectangle 48x48
                    Label text=$item.name
                }
            }
        }
    }
}
```

- **It scrolls itself** (WP feel: touch pan, fling, spring edges; the mouse
  wheel does not pan it yet, and the bar only shows where you are). The
  List's box is the VIEWPORT: unplaced, it takes the space its parent
  offers; `height=` pins it. Statics scroll with the
  rows, and the keyboard cursor stays in view. No `Scroll` wrapper — ever.
  `bars=never|always|reserve` paints the internal scroll's bar (D138 — the
  same word `Scroll` takes; `never` is the handheld look, the axis still
  pans). The axis itself is the layout's — `allow=` is Scroll vocabulary.
- **`layout` picks the items panel** (D128): `layout Row|Flow|Tiles { repeat
  ... }` — strips (horizontal pan, left/right cursor, `width=hug` +
  `max_width=`), wrap galleries and tile grids (vertical scroll, geometric
  2D arrows). No block = today's vertical column. Panel props live on the
  block (`layout Flow spacing=8` — row spacing, separate from the statics').
  See the `layout` keyword card.
- **`empty` lives on the List body** (D128): the zero-items view, REQUIRED
  (G9016) — shown beside the statics while no items are present, hidden
  when data arrives. Free placement inside (`Layers` for centered art).
  A List's repeat carries no inner `empty`.
- **Fixed content sizes the box; rows never do** (D127). Statics and the
  `empty` block count into the List's minimum like any authored content —
  a bare list in a centered menu shows its header and "none yet" text out
  of the box. The ROWS stay the viewport's business: in a hug parent they
  starve — provable cases warn at build time (G9015: `valign=`-anchored
  Column chains, `Scroll` pages); for the rest nothing warns at run time,
  and a host can ask `gogo_ui::layout::starved` which Lists got less than
  a row's worth of room.
- **`height=hug` sizes the box to the items** — the menu-sized list.
  `height=hug` flips the List's minimum to its whole content's, so the
  menu grows with its items; the explicit trade: a bare hug List does not
  scroll — content past the screen overflows.
- **`max_height=` bounds the hug** — `height=hug, max_height=400`: the box
  follows the items UP TO 400 epx, then stops and the internal scroll
  takes over. The centered-menu arc in one line: grows while there's room,
  scrolls when there isn't (WPF's `MaxHeight` answer — infinite-measure
  parents give a list no bound of its own, there either).
- **`ListItem` is the row root, always.** It is the unit selection
  (`selected`), the focus cursor and taps attach to — and a drawn box like
  `Panel` (`background=`/`padding=`), so the row's looks live ON it:
  `when selected` for selection, `when held` for press feedback (legal
  here, like on buttons), `when focused` for the keyboard cursor.
  Multi-part rows lay out with a `Row`/`Column` inside.
- **A bare list looks like a list.** Unstyled rows show selection, the
  cursor and the press out of the box — a white fill at 30% for the
  selection, a half-strength shade while held (it clears when the finger
  slides off the row), and a faint echo for the cursor that draws only
  when the KEYBOARD moved it — a tap parks the cursor silently (see the
  ListItem card). Your `when selected` / `when focused` / `when held`
  replaces that state's default entirely, per state.
- `select=` — `one` (default), `many`, or `off` (activation/focus only —
  menus).
- **A tap selects AND activates** — every completed tap on a row, and
  Enter or Space on the focused row, gives an `Activate` event whose node
  is the row; `Runtime::item_key(&page, ev.node)` is its key. Selection
  is the mode's business — `one` selects (tapping the selected item never
  deselects; code clears it with `Runtime::set_selected` and no keys),
  `many` toggles, `off` nothing. When the selection actually changed, a
  `Change` event on the List comes first (`on:change=` on the List line
  names its handler; `on:activate=` goes on the `ListItem` line). A
  `Button` inside a row takes its own presses — tapping it fires the
  button, not the row.
- **The selection** is read and written from code as a list of keys:
  `Runtime::selected_keys` reads it (empty = none, at most one key under
  `one`) and `Runtime::set_selected` writes it. Selected keys with no
  present item are **inert** — they render nothing, error nothing, and read
  back until the item arrives (select-then-load) or you reassign. Invalid
  writes are refused whole (G9104); the last good selection stays.
- **Screen readers hear nothing yet**: gogo hands nothing to the
  platform's accessibility tree, so a reader gets no list, no item count
  and no row selection. See the Accessibility card.

Items arrive through the repeat's node, as always:
`rt.set_items(&mut page, tasks, &items)` — reconcile keeps selection by key
across reorders and rebuilds. A hot reload does not carry the rows or the
selection yet: code sets them again.
