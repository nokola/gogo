A keyframe inside a braced property line: `key t=<seconds>, v=<value>`
with optional `easing=` (into this key) and `jump=true` (snap). Times are
absolute from the line's start; the last key's `t=` is the line's
duration; the same `t` twice makes a hard step.

```
anim bounce {
    dot.translate.y {
        key t=0,   v=0
        key t=0.15, v=-14, easing=cubic_out
        key t=0.3,  v=0,   easing=bounce_out
    }
}
Circle dot width=24, height=24, color=accent, on:show=play(bounce)
```
