`on:<signal>=emit(HostSignalName)` re-emits a node's signal as a signal on
the HOST (your GogoView / code-behind) — a pure relay, no handler method.
The host signal must exist (a missing one is a load-time error); source
arguments are dropped, so use a named handler when the payload matters.

```
Row spacing=8 {
    Button saveButton text="Save", min_width=88, min_height=36,
        on:activate=emit(SaveRequested)
    Button undoButton text="Undo", min_width=88, min_height=36
    // undoButton auto-wires OnUndoButtonActivate() by convention
}
```
