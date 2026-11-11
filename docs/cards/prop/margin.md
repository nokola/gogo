Margins CARVE the box the other keys produce, on a free (`Layers`)
child — SIGNED distances from the parent's matching edge:
positive sits inside, negative hangs past it. On a filled axis they inset;
on an aligned axis they offset the near edge; on a centered axis the box
is centered in the room the margins leave, so one margin shifts it by
half (`halign=center, margin_left=20` sits 10 right of center); on a
hug/exact axis they pin. `margin=N` alone is breathing room all around;
per-edge keys are `margin_left/-top/-right/-bottom`; `margin=safe` reads
the safe-area insets. A box that places its children itself (Column, Row,
Choice, Flow, Grid, Tiles, a List's rows) refuses margins on them (G1014)
— space them with `spacing`, `gap` or `padding`. The one body of a
`Scroll` takes margins: they inset it and scroll with it.

```
Layers screen width=240, height=160 {
    Rectangle bg color=charcoal, width=fill, height=fill
    Panel toast background=panel, padding=8, halign=center, valign=top,
        margin_top=16 {
        Label toastText text="Saved", font_size=14
    }
}
```
