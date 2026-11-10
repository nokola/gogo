The fill of nodes that OWN a plain color — `Rectangle` and `Circle`,
whose fill IS their color. On anything else it errors and points to the
right channel: text is `font_color=`, a box fill is `background=`, a
`Gradient` paints its `stops=`, a whole-control fade is `opacity=`.

```
Row spacing=10 {
    Rectangle swatch color=teal, size=60x28
    Circle    dot    color=accent, width=28, height=28
}
```
