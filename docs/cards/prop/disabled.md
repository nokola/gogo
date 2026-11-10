The universal off-switch condition: `disabled=true` flips the `disabled`
state the node's `when disabled` responses read. On a control that takes
input (Button, Slider, Input, ...) it blocks the input and dims the
control; on passive nodes it is styling-only — the look greys via `when`,
input is `interactive=`'s job.

```
style ToolButton {
    when disabled opacity=0.3
}
Button saveButton .ToolButton text="Save", disabled=true
```

Code flips it while the program runs with
`rt.set_state(node, DISABLED, true)`; the `when disabled` responses follow
either way. `interactive=false` is the hit-invisible switch that never
restyles — `disabled` is the one that shows.
