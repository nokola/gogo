A `key` line's value at its time `t=` — a number (epx on length
properties) or a color, matching the property the keyed line drives.
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
