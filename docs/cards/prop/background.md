The box's fill color — it RECOLORS, never reshapes. Panels (and `View`
roots) start as a blank canvas: nothing draws until a key says so.
A Button starts with a default plate (dark gray, 60% opaque);
`background=` changes only its color and leaves the corner radius and
borders as written. A Button has no press feedback of its own, with or
without `background=`: it is yours to author with `when held background=`.
Inside `when held` the tween starts from the button's resting color and
hands back when the press ends. Text color is `font_color=`; a
`Rectangle`/`Circle` fill is `color=`.

```
Panel plate background=charcoal, corner_radius=6, padding=10 {
    Button okButton text="OK" {
        when held background=white/0.13
    }
}
```
