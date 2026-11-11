The shape of this node's OWN rectangle — the space layout gives the
node itself, not the Panel or Column holding it — as a relation between
its two axes: width/height as one number (`aspect=1` square,
`aspect=1.78` 16:9), or `natural` — an `Image` bitmap's own ratio. One
rule: **the axis you spelled drives, the axis you didn't is derived.**
`width=fill, aspect=1.78` — height follows whatever width turns out to
be, live across resizes. Both axes `fill` + aspect = the largest
ratio rectangle that fits the space the parent offers, centered (WPF
Uniform — the color-wheel shape). Both axes spelled + aspect is an
error (drop one); `hug` and `max_*=` don't combine with it (v1).

Numbers solve under free parents (Layers / slot).
`aspect=natural` works by the picture instead: the Image's MINIMUM follows
the bitmap's ratio from the driving axis (the WPF UniformToFill
measure) — works under containers and free parents alike. The layout
asks the host for the picture's size (`TextMeasure::image_size`): until
the host knows it (a `loading=lazy` picture it has not measured yet) the
Image has no ratio to follow and gets no height from it. Nothing
spelled: width drives.

```
Layers {
    Rectangle wide color=#4488CCFF, width=fill, aspect=1.78,
        margin_left=12, margin_right=12
    Rectangle square color=#CC8844FF, width=fill, height=fill, aspect=1
}
```

```
Tiles cols=2 {
    Panel a background=#222222FF { Image source="res://icon.svg", aspect=natural }
    Panel b background=#444444FF { Image source="res://icon.svg", aspect=natural }
}
```
