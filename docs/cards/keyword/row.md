One Grid row — a lowercase structure word (never named; in the built page it is a plain `row` node between the Grid and its cells).
Cells walk the `cols=` signature left to right; `height=` goes on the row.

```
Grid cols=[hug, fill] {
    row height=30 { Label k text="Name", font_size=14; Input v }
    row { skip; Label hint text="left track empty", font_size=12 }
}
```
