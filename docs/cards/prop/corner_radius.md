Rounds the box's corners (epx) — one knob of the box family (`background` / `corner_radius` / `padding` / `border-*`), each changing exactly its one thing. A `Circle` has no box to round — its error routes back to sizing the disc.

```
Panel card background=panel, corner_radius=10, padding=12 {
    Label cardText text="rounded card", font_size=15
}
```

Each corner also has its own key — named, so there is no corner order to memorize: `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`. The uniform writes all four corners, a per-corner key writes one, and later on the line wins — exactly how `padding` and `padding_left` layer. A sheet docked to the screen's bottom edge rounds only its top:

```
Panel sheet background=panel, corner_radius=12,
        corner_radius_bottom_right=0, corner_radius_bottom_left=0,
        padding=12 {
    Label sheetText text="bottom sheet", font_size=15
}
```
