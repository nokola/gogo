Makes a deliberately TOUCH-ONLY button: with `keyboard_target=off` it takes
taps but never takes keyboard focus — Tab and the arrows skip it, and no
focus indicator ever shows. Right for WP-style app bars (phone surfaces
with no keyboard). The `touch_target=off` sibling marks the other
deliberate exception (a small target).

```
Button dotsButton 50x90, keyboard_target=off, text="…"
```
