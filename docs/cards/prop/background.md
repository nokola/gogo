The box's fill color — it RECOLORS, never reshapes. Panels (and `View`
roots) start as a blank canvas: nothing draws until a key says so.
Buttons start as a copy of their theme chrome, so `background=` keeps the
corner radius and borders and changes only the color. On a button it is
the PIGMENT TAKEOVER (D128): the one recolored box fills every state slot,
so press feedback is yours to author with `when held background=` —
shape keys alone (`padding`/`corner_radius`/`border-*`) leave the theme's
own state feedback alive. Inside `when held` the tween starts from the
theme's resting value and hands back cleanly (a padded button gets its
per-state shape copies back). Text color is `font_color=`; a
`Rectangle`/`Circle` fill is `color=`.

```
Panel plate background=charcoal, corner_radius=6, padding=10 {
    Button okButton text="OK" {
        when held background=white/0.13
    }
}
```
