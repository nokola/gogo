Minimum height only (epx) — the control still stretches with its container: the "at least N, stretchy" spelling. An exact `height=N` means N and no stretching; `min_height` sets the floor and keeps the flex. The usual home for touch-friendly control heights (36–44 epx), often in a style every button wears.

```
style ToolButton min_height=36
Row spacing=10 {
    Button paintButton .ToolButton text="Paint"
    Button undoButton  .ToolButton text="Undo"
}
```

Next to an exact `height=` on the same axis it is refused — an exact size IS its own floor and ceiling. The one exception (D249): when some `when` block poses `height=` on the node, the floor is live again — it clamps every posed value (the floor beats a posed height, WPF's MinHeight-over-Height order). With a hug cap it forms the min/max band (D135); the floor beats the cap (WPF clamp order) — see `min_width`/`max_width`.
