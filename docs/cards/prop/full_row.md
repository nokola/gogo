A Tiles cell prop: the cell takes the whole row regardless of `cols=` —
section headers, banners and list-style rows inside a tile mosaic. Height
stays the cell's own content. Exclusive with `span=` (a full row IS the
whole span).

```
Tiles cols=2 {
    Label text="section", full_row=true, font_size=20
    Panel a background=teal, height=50
    Panel b background=purple, height=50
    Panel banner full_row=true, background=orange, height=32
}
```
