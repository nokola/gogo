The predeclared resting arrangement: the declarations THEMSELVES. It needs
no `when` — every `when <pose>` override tweens home when the view returns
to base (`go(base)`, or `toggle(pose)` coming back). As a `transition`
endpoint it names one direction of a flip: `base -> expanded` times
leaving rest, and the way home takes the same timing unless a rule of its
own (`expanded -> base`) times it.

```
arrangement expanded
transition base -> expanded duration=0.25, easing=cubic_out
transition expanded -> base duration=0.15, easing=cubic_out
when expanded {
    bar.height = 120
}
Layers stage width=220, height=160 {
    Rectangle bar color=accent, width=200, height=50, halign=center,
        valign=top
    Button moreButton text="More", min_width=88, halign=center,
        valign=bottom, on:activate=toggle(expanded)
}
```
