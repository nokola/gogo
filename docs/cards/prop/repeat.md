Two homes, one word. On a `Gradient`: `repeat=none|repeat|mirror` — how
the ramp tiles past its end (mirrored stops declare only the first half,
positions up to 0.5). On an `anim` header: `repeat=forever` loops the
whole animation (pair with a line's `autoreverse=true` for breathing).

```
Gradient stripes fill=linear, angle=90, repeat=mirror,
    stops=[charcoal@0, silver@0.5], width=120, height=40
```

```
anim breathe repeat=forever {
    dot.opacity to=0.3, duration=0.8, autoreverse=true
}
Circle dot width=28, height=28, color=accent, on:show=play(breathe)
```
