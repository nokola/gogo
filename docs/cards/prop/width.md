HOW BIG on x — the sizing triad: a number = exactly that many epx (and
don't stretch); `hug` = natural content size; `fill` = take the leftover
space; `fill(2)` = a double-weighted share of it. Weights split what
remains AFTER fixed/hug siblings take their minimums. "At least N but
stretchy" is `min_width=`, not a number here. A number smaller than the
content can't hold — content wins and the build warns; scroll it or let
it `hug`. On a `Scroll`, `hug` fits the content and turns that axis'
scrolling off — unless capped: `width=hug, max_width=300` follows the
content to 300, then the axis scrolls (see `max_width`, `allow`).
`fill` takes a cap too (D245): `width=fill, max_width=380, halign=center`
fills up to 380, then the stated side places the box — see `max_width`.

```
Row spacing=8, height=40 {
    Rectangle fixed color=teal,   width=90
    Rectangle rest  color=purple, width=fill
    Rectangle twice color=orange, width=fill(2)
}
```
