HOW BIG on y — the sizing triad: a number = exactly that many epx (and
don't stretch); `hug` = natural content size; `fill` = take the leftover
space; `fill(2)` = a double-weighted share. An axis with NO placement key
takes what the parent gives (a Column's cross axis fills). "At least N but
stretchy" is `min_height=`. A number smaller than the content can't hold —
content wins and the build warns; scroll it (`Scroll height=100 { ... }`)
or let it `hug`. On a `Scroll`, `hug` fits the content and turns that
axis' scrolling off — unless capped: `height=hug, max_height=300` follows
the content to 300, then the axis scrolls (see `max_height`, `allow`).

```
Column spacing=8, width=120, height=200 {
    Rectangle top    color=teal,   height=40
    Rectangle spring color=purple, height=fill
    Label     tag text="hugged", font_size=14, height=hug
}
```
