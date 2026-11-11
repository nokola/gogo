A uniform default height for every `row { }` block: an epx number or
`fill(n)` (the bare words `hug` and `fill` are refused here, G0058 — write
them on a row's own `height=`). A `row height=` on one block overrides it for that
row. Row-form only: areas rows each carry their own `: height` instead.

```
Grid cols=[hug, fill], row_height=28 {
    row {
        Label aLabel text="Size", font_size=14
        Rectangle aBar color=accent
    }
    row {
        Label bLabel text="Flow", font_size=14
        Rectangle bBar color=teal
    }
}
```
