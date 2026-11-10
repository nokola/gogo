`jump=true` on a `key` line snaps to that key's value instead of easing
into it — the discrete step inside an otherwise smooth journey (WPF's
DiscreteKeyFrame).

```
anim tick {
    dot.translate.x {
        key t=0,   v=0
        key t=0.2, v=20, jump=true
        key t=0.4, v=0,  easing=cubic_out
    }
}
Circle dot width=24, height=24, color=teal, on:show=play(tick)
```
