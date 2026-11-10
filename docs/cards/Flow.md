Wrap-to-next-line: children flow like words in a paragraph. `spacing=`
covers both directions (default 0 — no gap you didn't write);
`orientation=vertical` flows in columns.

```
Flow spacing=6 {
    Button tag1 text="red"
    Button tag2 text="green"
    Button tag3 text="blue"    // wraps when the row runs out of width
}
```
