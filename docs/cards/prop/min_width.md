Minimum width only (epx) — the control still stretches with its container: the "at least N, stretchy" spelling. An exact `width=N` means N and no stretching; `min_width` sets the floor and keeps the flex. On a button it merges with the derived content minimum via max.

```
Column spacing=8 {
    Button okButton     text="OK",     min_width=120
    Button cancelButton text="Cancel", min_width=120
}
```

Next to an exact `width=` on the same axis the compiler accepts it and the floor wins: `width=50, min_width=100` is 100 wide. The exception is a child of a Row, of a Flow that runs across, or of a Tiles: there the exact width holds. The floor also clamps a width that a `when` block poses on the node (`min_width=30, width=50` with `when small { n.width=20 }` renders 30; the floor beats a posed width, WPF's MinWidth-over-Width order). With a hug cap it forms the min/max band (D135): `width=hug, min_width=100, max_width=300` clamps the natural size into the pair, and the floor beats the cap (WPF clamp order) — see `max_width`.
