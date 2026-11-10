The scrollbar look on a `Scroll`'s (or `List`'s) scrolling axes — `auto` (default: the bar shows while there is something to scroll), `never` (hidden; the axis still pans — the handheld look), `always`, or `reserve` (shows like auto, but the bar's space is always held, so content doesn't shift when the bar appears).

```
Scroll page height=120, bars=never {
    Column spacing=8 {
        Label text="glides with no bar"
    }
}
```

`bars=` never picks WHICH axes scroll — that is `allow=` or the sizing triad on a `Scroll`, `layout` on a `List` (`List picker bars=never { ... }` paints the internal scroll's bar). On a non-scrolling axis it is inert.

Per-axis bar policy (auto vertical, hidden horizontal) is the one thing the word can't say — the raw `horizontal_scroll_mode=`/`vertical_scroll_mode=` escape hatch keeps engine meaning for that, never mixed with `bars=` on the same Scroll.
