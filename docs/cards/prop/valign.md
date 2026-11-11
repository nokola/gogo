WHERE the control sits on y: `top` / `center` / `bottom` — position only,
sizing stays with `height=`. The classic Row use: `valign=center` on a
label next to a taller control. `valign=center` on a `height=fill` axis
is an error (`top`/`bottom` there are accepted and move nothing) — unless
the fill is capped (`max_height=`), where the side places the box and no
side means centered (D245, see `max_width`). Don't confuse with `vertical_alignment`,
which places TEXT inside a Label's own box.

```
Row spacing=8, height=44 {
    Label sizeLabel text="Size", font_size=15, valign=center
    Slider sizeSlider min=1, max=100, value=40, width=fill, valign=center
}
```
