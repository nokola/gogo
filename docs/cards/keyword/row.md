One Grid row — a lowercase structure word (never named, erased at build).
Cells walk the `cols=` signature left to right; `height=` goes on the row.

```
Grid cols=[hug, fill] {
    row height=30 { Label k text="Name", font_size=14; Input v }
    row { skip; Label hint text="left track empty", font_size=12 }
}
```
