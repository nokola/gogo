# Toolbar over canvas

A full-bleed content layer with a floating bottom toolbar — `Layers` is
the depth container, so the bar's margins pin it freely over the canvas.

```
style ToolButton min_size=64x44, font_size=15 {
    when held background=white/0.13
}

Layers stage width=fill, height=fill {
    Rectangle canvas color=#101418
    Panel bar halign=center, valign=bottom, margin_bottom=14,
            background=#2E4053, corner_radius=8, padding=6 {
        Row spacing=8 {
            Button paintButton .ToolButton text="Paint", toggle=true
            Button colorButton .ToolButton text="Color"
            Button undoButton .ToolButton text="Undo"
        }
    }
}
```
