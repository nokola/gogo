Independent on/off — never joins an exclusive group (inside `choice`,
use `Radio`). Holds `selected` while ON; `change(bool)` fires once per
genuine flip, programmatic writes included.

```
Check gridToggle text="Show grid", button_pressed=true
// OnGridToggleChange(on: bool) auto-wires / on:change=Handler
```
