The box's border stroke width (epx) — pairs with `border_color`. Content
sits `border_thickness + padding` from the edge, so a border never paints
over content. Fractional values keep their declared thickness, in layout
math and in the painted stroke alike. On a `Circle` it
strokes the ring (inward).

```
Panel outlined background=panel, border_color=border, border_thickness=1.5,
    padding=10 {
    Label outlinedText text="outlined", font_size=14
}
```
