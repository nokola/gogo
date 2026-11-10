How the bitmap maps into the Image's OWN rectangle when their shapes
disagree. "Rectangle" = the space layout gives the Image node itself —
NOT the Panel or Column holding it: in `Panel { Image width=96,
height=48 }` it is that 96x48 slot inside the panel. An `Image` prop
only (children are laid out, never scaled). `contain` letterboxes —
the whole bitmap stays visible, bars fill the slack (the default every
Image is built with); `cover` fills the rectangle and clips the
overhang; `distort` stretches the bitmap to the rectangle, named for
what it does (the CSS/WPF word for it is `fill`, which is gogo's
sizing word). Slack in `contain` centers (v1 — no alignment knob).

When the Image's rectangle already matches the bitmap's shape, all
three draw the same pixels — `fit=` only acts once you've forced a
mismatched rectangle (an exact `WxH`, or an `aspect=` that differs
from the asset). Pair with `aspect=natural` when you want the
rectangle ITSELF to take the bitmap's shape instead.

```
Row spacing=8 {
    Image a source="res://icon.svg", width=96, height=48
    Image b source="res://icon.svg", width=96, height=48, fit=cover
    Image c source="res://icon.svg", width=96, height=48, fit=distort
}
```
