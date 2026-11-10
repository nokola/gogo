A wrap panel over a unit lattice (the phone-launcher mosaic): `cols=` splits
the width into equal units; cells flow in declaration order and wrap —
first-fit, no backfill. A cell is one unit unless `span=N` widens it or
`full_row=true` takes the whole row. Cell HEIGHT is the cell's own content:
give a tile an `Image` with `aspect=natural` and the bitmap's ratio sizes
it (square bitmaps make square tiles).

Cells never shrink content below its minimum — an oversized child OVERLAPS
its neighbors (Tiles warns at sort time). Clip it: `clip_text=true` on a
Label caption, a `clip_contents=true` wrapper otherwise, or widen the cell
(`span=`).

```
Tiles cols=2 {
    Label text="top picks", full_row=true, font_size=24
    Panel a background=teal { Image source="res://icon.svg", aspect=natural }
    Panel b background=purple { Image source="res://icon.svg", aspect=natural }
    Panel wide span=2, background=orange, height=60
}
```
