The box's border stroke color — pairs with `border_thickness` (neither
draws alone on a blank panel until both say so; the border can never
paint over content because padding stacks inside it). On a `Circle` the
pair strokes the RING instead (inward — it never overflows the box).

```
Row spacing=10 {
    Panel framed background=panel, border_color=accent, border_thickness=2,
        padding=10 {
        Label framedText text="framed", font_size=14
    }
    Circle ring width=44, height=44, border_color=accent, border_thickness=2
}
```
