The Grid's track signature — every row is checked against it. Tracks are
the sizing triad: epx numbers, `hug` (max cell minimum), `fill(n)`
(weighted share of the leftover). There is no `rows=` list: row heights
come from each `row` block. Weights belong to TRACKS — `width=fill(2)` on a
cell errors and routes here.

```
Grid cols=[hug, fill, 60] {
    row height=30 {
        Label nameLabel text="Size", font_size=14
        Rectangle bar color=accent
        Label valLabel text="80", font_size=14
    }
}
```
