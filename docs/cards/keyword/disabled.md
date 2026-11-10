Holds while the control is disabled (`disabled=true` — the engine
property): it stops receiving input, and the `when disabled` response
styles the grayed-out look. Flip it from code to gate actions.

```
style Dimmy min_width=88, min_height=36 {
    when disabled opacity=0.4
}
Row spacing=8 {
    Button okButton .Dimmy text="OK"
    Button lockedButton .Dimmy text="Locked", disabled=true
}
```
