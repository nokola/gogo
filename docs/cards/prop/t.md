A `key` line's time in seconds — ABSOLUTE from the line's start, not
relative to the previous key. A keyframed line's duration IS its last
key's `t=` (a `duration=` alongside keys is an error — keys own the
timeline). The same `t` twice makes a hard step.

```
anim nod {
    dot.translate.y {
        key t=0,   v=0
        key t=0.1, v=6, easing=cubic_out
        key t=0.2, v=0, easing=cubic_in
    }
}
Circle dot width=24, height=24, color=accent, on:show=play(nod)
```
