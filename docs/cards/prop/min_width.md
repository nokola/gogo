Minimum width only (epx) — the control still stretches with its container: the "at least N, stretchy" spelling. An exact `width=N` means N and no stretching; `min_width` sets the floor and keeps the flex. On a button it merges with the derived content minimum via max.

```
Column spacing=8 {
    Button okButton     text="OK",     min_width=120
    Button cancelButton text="Cancel", min_width=120
}
```

Next to an exact `width=` on the same line the floor is a build error (G1019): an exact width is the width, so the floor says nothing or contradicts it. Two forms build, because there the floor has a job, and in both the floor wins in every parent (WPF's MinWidth-over-Width order). One: a `when` block poses the width on the node (`min_width=30, width=50` with `when small { n.width=20 }` renders 30). Two: the floor and the width come from different lines — a style or a control's template carries `min_width=48` and the line that uses it writes `width=20`: the box is 48 wide. With a hug cap it forms the min/max band (D135): `width=hug, min_width=100, max_width=300` clamps the natural size into the pair, and the floor beats the cap (WPF clamp order) — see `max_width`.
