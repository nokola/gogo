The grid a slider's value snaps to, counted from `min=`. With `min=0,
max=100, step=5` a drag lands on 0, 5, 10, … 100 — never 7.3. Arrow keys
move by exactly one step. `step=0` turns snapping off (continuous values);
the default is 1.

```
Slider volume value=30, min=0, max=100, step=5
```

The same word appears inside `round($value, step)` — there it is the
unit a shown number is rounded to, not this property.
