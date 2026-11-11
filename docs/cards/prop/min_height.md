Minimum height only (epx) — the control still stretches with its container: the "at least N, stretchy" spelling. An exact `height=N` means N and no stretching; `min_height` sets the floor and keeps the flex. The usual home for touch-friendly control heights (36–44 epx), often in a style every button wears.

```
style ToolButton min_height=36
Row spacing=10 {
    Button paintButton .ToolButton text="Paint"
    Button undoButton  .ToolButton text="Undo"
}
```

Next to an exact `height=` on the same axis the compiler accepts it and the floor wins: `height=10, min_height=30` is 30 tall. The exception is a child of a Column or of a Tiles: there the exact height holds. The floor also clamps a height that a `when` block poses on the node (the floor beats a posed height, WPF's MinHeight-over-Height order). With a hug cap it forms the min/max band (D135); the floor beats the cap (WPF clamp order) — see `min_width`/`max_width`.
