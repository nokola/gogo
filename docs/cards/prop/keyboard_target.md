Accepts a deliberately TOUCH-ONLY button. A `focus_mode=none` button takes
taps but can never take keyboard or gamepad focus — desktop and controller
users can't reach it, and no focus indicator will ever show — so the
advisory lint asks whether that's deliberate. It is on WP-style app bars
(phone surfaces with no keyboard); say so and the lint stands down. The
`touch_target=off` sibling accepts the other deliberate exception (a
small target).

```
Button dotsButton 50x90, keyboard_target=off, text="…"
```

If the button should simply work with a keyboard, drop `focus_mode=none`
instead — focus, the D129 visible-focus ring and gamepad navigation all
return on their own.
