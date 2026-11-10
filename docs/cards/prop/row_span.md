A Grid cell prop: cover N rows downward. A row_span OWNS the tracks it
covers below — each covered track is written `skip` in those rows (the
error walks you through it). Spans past the last row are load errors.

```
Grid cols=[60, fill] {
    row height=24 {
        Rectangle side color=teal, row_span=2
        Rectangle top  color=purple
    }
    row height=24 {
        skip
        Rectangle bottom color=orange
    }
}
```
