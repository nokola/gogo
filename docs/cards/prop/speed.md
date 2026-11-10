A rate multiplier on the anim envelope: `speed=2` plays the whole anim
twice as fast without retiming every lane (durations, delays and `after=`
anchors all scale together). Tuning one journey is its own `duration=`.

```
anim zip speed=2 {
    chip.translate.x from=-40, to=0, duration=0.4
}
Rectangle chip color=teal, size=80x28, on:show=play(zip)
```
