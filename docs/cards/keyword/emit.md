`on:<event>=emit(Name)` reports the event to the host program under the
name you chose instead of the node's own: one name can then stand for
"save was asked for" whichever control asked. The event still carries its
kind and value.

```
Row spacing=8 {
    Button saveButton text="Save", min_width=88, min_height=36,
        on:activate=emit(SaveRequested)
    Button undoButton text="Undo", min_width=88, min_height=36
    // undoButton's tap names the handler `undoButton`
}
```
