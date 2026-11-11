A `key` line's value at its time `t=` — a number (epx on length
properties). A color line takes no keys: it spells its legs as chained
steps (`then box.background to=red, duration=0.2`).
The journey eases INTO each key with that key's `easing=`.

```
anim blink {
    dot.opacity {
        key t=0,   v=1
        key t=0.2, v=0.3, easing=cubic_out
        key t=0.4, v=1,   easing=cubic_in
    }
}
Circle dot width=24, height=24, color=accent, on:show=play(blink)
```
