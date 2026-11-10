Plays the journey out and straight back (WPF AutoReverse) — pulse, breathe, pop. A step word: on a property line it doubles that line; on a `group` it distributes to every line inside (a line's own setting wins). `then` and `after=` wait for the return leg too. Pair with the header's `repeat=forever` for ambient loops.

```
anim pulse repeat=forever {
    dot.scale to=1.25, duration=0.5, autoreverse=true
}
Circle dot width=24, height=24, color=accent, on:show=play(pulse)
```
