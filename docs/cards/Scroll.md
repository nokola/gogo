Touch-pannable scrolling (a pan cancels the child's press). One child; size the Scroll, let the content grow. Which axes pan: `allow=vertical|horizontal|both` says it outright (listed axes scroll, the rest are off); without it the sizing triad infers — a `hug` axis fits the content and turns scrolling OFF on that axis (a content-sized axis has nothing to scroll), every other axis scrolls. So a menu hugs its width and pans vertically:

```
Scroll height=100, width=hug {
    Column spacing=8 {
        Label text="a long list..."
        // ...
    }
}
```

On an axis that does not pan, the content is held to the Scroll: a `fill` column is the Scroll's width, wrap text wraps there, and one line too long to wrap is cut at the edge on its own instead of widening every row with it.

`bars=auto|never|always|reserve` picks the scrollbar look on the scrolling axes — `never` hides the bar but the axis still pans (the handheld look), `reserve` shows like auto but always holds the bar's space so content doesn't shift.

A capped hug axis scrolls PAST its cap: `height=hug, max_height=300` follows the content up to 300 epx, then the axis scrolls — the `List` doctrine, on Scroll. The Scroll's own minimum stops at the cap, so it works under containers too:

```
Scroll menu width=hug, height=hug, max_height=300, bars=never {
    Column spacing=4 {
        Button newGame text="New Game"
        Button cont text="Continue"
    }
}
```
