WHERE the control sits on y: `top` / `center` / `bottom` — position only,
sizing stays with `height=`. The classic Row use: `valign=center` on a
label next to a taller control. On a `height=fill` axis it is an error —
unless the fill is capped (`max_height=`), where the side is REQUIRED
(D245, see `max_width`). Don't confuse with `vertical_alignment`,
which places TEXT inside a Label's own box.

```
Row spacing=8, height=44 {
    Label sizeLabel text="Size", font_size=15, valign=center
    Slider sizeSlider min=1, max=100, value=40, width=fill, valign=center
}
```
