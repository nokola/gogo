Poses the view for the screen — two independent axes, one keyword:

- **Size class** — `when screen.<class> { ... }`: how much room there is.
  Five classes, fixed: `portraitSmall`, `portraitMedium`, `portraitLarge`,
  `landscapeSmall`, `lotsOfSpace`.
- **Orientation** — `when screen.portrait` / `when screen.landscape`
  (D265): which way the screen runs (landscape = wider than tall). Two
  values, fixed. A layout that asks "is it sideways?" keys the
  orientation; enumerating the landscape-ish classes instead silently
  catches big PORTRAIT tablets — `lotsOfSpace` covers both shapes.

The host program names both whenever the screen changes size
(`Runtime::set_screen`); nobody flips these from a button. No declaration
line; an unknown name is a build error listing both vocabularies.

```
when screen.landscape {
    optionsPane.halign = left    // an alignment RE-DECLARES the axis: the
    optionsPane.width = 280      // base's pins drop, these keys stand alone
    optionsPane.margin_left = 60
    hintLabel.opacity = 0.5      // style props flip through the states
}

Layers stage {
    Panel optionsPane margin_left=60, margin_right=60, valign=top, height=200
    Label hintLabel text="rotate me", halign=center, valign=bottom
}
```

**One vocabulary per target (G3005).** A class and an orientation hold AT
THE SAME TIME, so one property, one node's placement, or one seat move
posed from both would be settled by file order alone, silently — that
mix is a build error. Say the intersection with the composed spelling
instead: `when screen.landscape screen.lotsOfSpace { ... }` (orientation
first, class second) — one condition, active only while both hold.

Independent of arrangements: a view can hold its expanded pose on a
rotated screen. Where both re-pin the SAME node, each contested key goes
to the block written LATER in the file (the two rank equal — one
condition each) and the other pose keeps its keys — and closing the
arrangement lands back on the screen pose, not on base. The whole rule:
more conditions win (a composed pair beats any single), equal count →
the later line in the file wins. Flips are instant by design (a rotation is the OS snapping
the window; `transition` rules don't apply).

A composed block holds only while BOTH its conditions do — `when expanded
screen.landscapeSmall { ... }` (arrangement first; the second slot takes a
class or an orientation), for a pose neither axis owns alone. It is more
specific than either half, so its keys win; either half dropping releases
it back to whichever single-axis pose still holds. Alignment keys need an
instant change (G3004): legal here — and in a plain arrangement `when` —
exactly while the arrangement has NO `transition` rule; the screen-first
pair has no arrangement half and is always instant.

A pose may also RESIZE any child — a `width`/`height`/`min_width`/
`min_height` written here behaves exactly as if the file had declared it,
for as long as the condition applies (D249), on flow and free children
alike; a posed `width`/`height` on a fill child stops it taking the
leftover, and the pose dropping restores the declared axis. A declared
`min_width`/`min_height` floor still clamps every posed size, and a posed
`min_*` replaces the declared floor. Margins and alignment on container
children stay build errors: the parent seats its children.

A `when` may also MOVE a node to a declared socket — `x.seat = sideSeat`
(see the seat card) — the one response that reparents. Legal from screen
whens, composed blocks, and arrangement whens no `transition` rule times
(G3003); `x.seat = home` sends it back while a more specific condition
holds. Contested seats follow the same rule as placement keys above.

The active class and orientation hold across a hot reload. `screen.*`
blocks are view-level only — styles can't respond to them.
