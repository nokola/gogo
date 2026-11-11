Minimum size on both axes at once — `min_size=WxH` is the two-axis spelling of `min_width` + `min_height` (epx; the control still stretches). Omit a component to leave that axis alone: `min_size=44x` / `min_size=x36`. The bare `WxH` shorthand (`Button b 44x36`) is the EXACT size instead — min_size keeps the flex. Each declared component follows the same laws as `min_width`/`min_height` (D135): beating an exact size on that axis, and forming the min/max band under a hug cap.

```
Row spacing=8 {
    Button aButton text="A", min_size=44x44
    Button bButton text="B", min_size=44x44
}
```
