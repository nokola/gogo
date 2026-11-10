`touch_target=off` accepts a deliberately small interactive control —
it silences the advisory 44-epx touch_target lint for that node. Use it
when small is the design (a dense desktop toolbar), not to hush a phone
UI that should simply be bigger.

```
Button tinyButton text="×", size=20x20, touch_target=off
```
