The Gradient's pattern: `linear` (default), `radial`, `conic`.
Round fills center in the box and reach its edges; `from=`/`to=` are
accepted on them and move nothing yet. A linear fill aims with `angle=`
or `from=`/`to=`. Not to be confused with the
sizing word `fill` on `width=`/`height=` — that one is a value, not this
key.

```
Row spacing=8 {
    Gradient lin fill=linear, stops=[teal, purple], width=70, height=40
    Gradient rad fill=radial, stops=[accent, accent/0], width=70, height=40
    Gradient con fill=conic,  stops=[red, yellow, green, blue, red],
        width=70, height=40
}
```
