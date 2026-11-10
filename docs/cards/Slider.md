Value drag (horizontal). `min=`/`max=`/`value=` map to the engine range.
`held` holds while the grabber is dragged (the drag IS the press — style
it like a button's). Each drag tick fires `edit` (the live stream);
release commits and fires `change` — once, only if the value genuinely
differs. Keyboard/gamepad steps are complete gestures and commit directly
(`change` per step). The raw `drag_started`/`drag_ended` stay legal
(`on:drag_ended=`) when you need the gesture bracket itself.

```
style Scrubby { when held scale=1.02 }
Slider sizeSlider .Scrubby min=1, max=300, value=80, width=fill
// OnSizeSliderChange(value) auto-wires — the committed value;
// OnSizeSliderEdit(value) is the live stream, only when you need it
```
