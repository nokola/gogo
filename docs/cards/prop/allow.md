Which axes a `Scroll` pans — `vertical`, `horizontal`, or `both`. Without it the sizing triad infers the axes: a `hug` axis is content-sized (scrolling off on that axis), numbers, `fill` and unkeyed axes scroll. With it, `allow=` owns BOTH axes — the listed ones scroll, the rest are off. That is the word for the case the triad can't say: a fill-width page that must not pan sideways.

```
Scroll page height=120, allow=vertical, bars=never {
    Column spacing=8 {
        Label text="row one"
        Label text="row two"
    }
}
```

The axis left out holds the content to the Scroll's size: with `allow=vertical` a `fill` column is exactly the Scroll's width, so wrap text wraps on screen and one line too long to wrap is cut at the edge on its own.

Naming a bare `hug` axis is a build error (G1020): a side that pans asks its parent for no room, so the Scroll would be 0 long there. A capped hug axis (`width=hug, max_width=300`) is fine to name: it scrolls past its cap (see `max_width`). Leaving a capped axis OUT of `allow=` is the same error: what the cap cuts could not be reached.

`Scroll` only. A `List` picks its axis with `layout` (`layout Row` pans sideways); other nodes don't scroll.
