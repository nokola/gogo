Margins CARVE the box the other keys produce, on a free (`Layers`)
child — SIGNED distances from the parent's matching edge:
positive sits inside, negative hangs past it. On a filled axis they inset;
on an aligned axis they offset the near edge; on a hug/exact axis they
pin. `margin=N` alone is breathing room all around; per-edge keys are
`margin_left/-top/-right/-bottom`; `margin=safe` reads the safe-area
insets. Containers refuse margins — space their children with `spacing`,
`gap` or `padding`.

```
Layers screen width=240, height=160 {
    Rectangle bg color=charcoal, width=fill, height=fill
    Panel toast background=panel, padding=8, halign=center, valign=top,
        margin_top=16 {
        Label toastText text="Saved", font_size=14
    }
}
```
