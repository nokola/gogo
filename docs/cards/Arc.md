A ring SEGMENT drawn in the box — the radial cousin of `Circle`'s ring.
The radius self-derives from the box (min side / 2); `thickness=` strokes
inward from the rim (`width=` stays the sizing key). Angles are degrees in
the screen's convention: 0° = right, clockwise positive; defaults `from=0,
sweep=360` draw a full ring.

```
Arc gauge 48x48, from=-90, sweep=180, thickness=3, color=red
```

Inside a `control`, an arc slot's default content is typically a bare full
ring — the slot's WEDGE (`automatic=arc(from=..., sweep=...) by $value in $min..$max`)
does the revealing, so the aperture is written once, as the kind's config.
