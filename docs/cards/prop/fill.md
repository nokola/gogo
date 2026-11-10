The Gradient's pattern: `linear` (default), `radial`, `square`, `conic`.
Round fills center in the box and reach the right edge by default; aim
with `from=`/`to=` (or `angle=` for linear). Not to be confused with the
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
