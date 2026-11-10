A Tiles cell prop: take N lattice units of width (default 1). A span wider
than the remaining row wraps whole to the next row — first-fit, the vacated
tail stays empty. `span=` past the Tiles' `cols=` is a build error. The whole
row is `full_row=true`, not a maximal span (it survives `cols=` changes).

```
Tiles cols=3 {
    Panel a background=teal, height=40
    Panel b background=purple, height=40
    Panel wide span=2, background=orange, height=40
    Panel c background=yellow, height=40
}
```
