The universal off-switch condition: `disabled=true` flips the `disabled`
state the node's `when disabled` responses read. On a button it is the
engine property (input blocked, engine styling applies); on editable
controls (Slider, Input) it maps to not-editable; on passive nodes it is
styling-only — the look greys via `when`, input is `interactive=`'s job.

```
style ToolButton {
    when disabled opacity=0.3
}
Button saveButton .ToolButton text="Save", disabled=true
```

Code-behind flips it at runtime through the node property
(`button.Disabled = ...`); the `when disabled` responses follow either
way. `interactive=false` is the hit-invisible switch that never
restyles — `disabled` is the one that shows.
