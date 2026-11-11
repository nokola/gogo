THE layout-negotiation switch, completing the node-role trio: `interactive=`
is a control's relationship to input routing, `overlay=` to layout
negotiation, `visibility=` to rendering presence. `overlay=true` cuts
exactly one thing — the box's upward size contribution: no ancestor
hug/minimum (and no scroll range) counts it. Everything else is untouched:
placement works as declared (signed margins may hang the box past the
parent's edges), draw order stays declaration order, a `Scroll` viewport
still clips it (a box that must beat a scroller is a popup — a different
concept), hit order stays draw order, focus and event routing don't change.
For boxes that sit on a parent without sizing it: a touch-slop surface wider
than the strip it serves, a finger puck, a drag marker, a corner badge.

```
Panel chip width=120, height=48, background=#1F1F1FFF {
    Layers {
        Label text="inbox", halign=center, valign=center
        Circle badge overlay=true, 16x16, color=#E51400FF,
            halign=right, valign=top, margin_right=-6, margin_top=-6
    }
}
```

The badge hangs past the chip's corner and the chip never grows for it.
A box with a written size cuts a child that pokes past it; an overlay
child is never cut. Free parents only (permanent law): under `Column`/`Row`/`Grid`
a child holds a seat it can't refuse — wrap the flowed content in `Layers`
and declare the overlay beside it. The overlay's own inside is a box like
any other: its written size cuts its own children, and a nested overlay
is left alone only by its own parent. Two contradictions refuse at build:
`overlay` with `visibility=space_only` (a seat-holder is not an overlay — a
hidden overlay is `visibility=gone`), and `overlay` with `fill` on an axis
the parent hugs with no non-overlay child (nothing determines the size).
