Minimum height only (epx) — the control still stretches with its container: the "at least N, stretchy" spelling. An exact `height=N` means N and no stretching; `min_height` sets the floor and keeps the flex. The usual home for touch-friendly control heights (36–44 epx), often in a style every button wears.

```
style ToolButton min_height=36
Row spacing=10 {
    Button paintButton .ToolButton text="Paint"
    Button undoButton  .ToolButton text="Undo"
}
```

Next to an exact `height=` on the same line the floor is a build error (G1019): an exact height is the height. Two forms build, and in both the floor wins in every parent (WPF's MinHeight-over-Height order): a `when` block poses the height on the node, or the floor comes from another line (the style above beside `height=30` on a Button that wears it: the Button is 36 tall). With a hug cap it forms the min/max band (D135); the floor beats the cap (WPF clamp order) — see `min_width`/`max_width`.
