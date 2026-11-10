Minimum width only (epx) — the control still stretches with its container: the "at least N, stretchy" spelling. An exact `width=N` means N and no stretching; `min_width` sets the floor and keeps the flex. On a button it merges with the derived content minimum via max.

```
Column spacing=8 {
    Button okButton     text="OK",     min_width=120
    Button cancelButton text="Cancel", min_width=120
}
```

Next to an exact `width=` on the same axis it is refused — an exact size IS its own floor and ceiling, so the pair reads as a contradiction, not a precedence puzzle. The one exception (D249): when some `when` block poses `width=` on the node, the floor is live again — it clamps every posed value (`min_width=30, width=50` with `when small { n.width=20 }` renders 30; the floor beats a posed width, WPF's MinWidth-over-Width order). With a hug cap it forms the min/max band (D135): `width=hug, min_width=100, max_width=300` clamps the natural size into the pair, and the floor beats the cap (WPF clamp order) — see `max_width`.
