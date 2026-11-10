Value drag (horizontal). `min=`/`max=`/`value=` set the range and the
start; `step=` snaps. `held` holds while the thumb is dragged (the drag IS
the press — style it like a button's). Each drag tick fires `edit` (the
live stream); release commits and fires `change` — once, only if the value
genuinely differs. Arrow-key steps are complete gestures and commit
directly (`change` per step). `on:press=` / `on:release=` bracket the drag
when you need the gesture itself.

```
style Scrubby { when held scale=1.02 }
Slider sizeSlider .Scrubby min=1, max=300, value=80, width=fill
// a Change event carries the committed value (handler: `sizeSlider`);
// Edit events are the live stream, only when you need it
```
