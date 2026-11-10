A Grid cell prop: cover N tracks to the right (plain WPF ColumnSpan, no
coordinates). Spans past the last column are load errors (WPF silently
clamps). Overlapping spans are unrepresentable by design — a whole-grid
overlay is a `Layers` sibling instead.

```
Grid cols=[60, fill, 60] {
    row height=24 {
        Rectangle a color=teal
        Rectangle b color=purple
        Rectangle c color=orange
    }
    row height=30 {
        Rectangle wide color=accent, col_span=3
    }
}
```
