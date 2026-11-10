Holds while the mouse is OVER the control — desktop-only feedback: touch
has no hover, so never hide functionality behind it; treat it as polish on
top of a look that works without it.

```
style Hovery min_width=88, min_height=36 {
    transition duration=0.15, easing=cubic_out
    when hovered background=white/0.08
}
Button overButton .Hovery text="Hover me"
```
