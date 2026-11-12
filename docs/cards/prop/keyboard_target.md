Makes a deliberately TOUCH-ONLY button: with `keyboard_target=off` it takes
taps but never takes keyboard focus — Tab and the arrows skip it, and no
focus indicator ever shows. Right for WP-style app bars (phone surfaces
with no keyboard). The `touch_target=off` sibling marks the other
deliberate exception (a small target).

```
Button dotsButton 50x90, keyboard_target=off, text="…"
```

Code may write the word too (`set_prop` with `off`): Tab, the arrows and
a tap's focus then skip the control, which keeps its look and still
takes taps — the way to take a control out of the Tab order without
disabling it.
`clear_prop` gives the file's word back. A control switched off while it
has the focus keeps its place, so Tab goes on from it, and answers no
key meanwhile.
