Independent on/off — never joins an exclusive group (inside `choice`,
use `Radio`). Holds `selected` while ON; `change(bool)` fires once per
flip a tap or a key makes. A write from code flips it and reports nothing.
Its text takes `wrap=true`, in the width the box leaves.

```
Check gridToggle text="Show grid", selected=true
// a Change event carrying the bool; the handler is `gridToggle`, or on:change=Handler
```
