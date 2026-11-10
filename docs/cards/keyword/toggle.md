`on:<signal>=toggle(arrangementName)` flips between the pose and base —
the disclosure button: press expands, press again collapses (from a
different pose it goes to the named one). Keep the button MOMENTARY and
style it through the pose; button `toggle=true` next to `toggle(pose)` is
two separate on/off states that drift apart (gogo lints it).
`toggle(base)` is a load-time error — `go(base)` returns home.

```
arrangement expanded
transition duration=0.2, easing=cubic_out
when expanded {
    bar.height = 120
    moreButton.rotate = 180   // the chevron points back up
}
Layers stage width=240, height=200 {
    Rectangle bar color=accent, width=200, height=50, halign=center,
        valign=top
    Button moreButton text="v", min_width=88, halign=center,
        valign=bottom, on:activate=toggle(expanded)
}
```
