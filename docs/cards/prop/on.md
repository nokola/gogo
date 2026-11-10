Wiring: `on:<event>=` binds a discrete moment. Gogo's own events are bare
stems — `activate` (the control's trigger gesture: a Button's completed
tap or key/gamepad accept, a List row's double-tap/Enter), `change` (the
committed value is now different — once per genuine change, programmatic
writes included), `edit` (the live uncommitted stream: Slider drag ticks,
Input keystrokes), and the visibility pair `show`/`hide` (show also plays
when a control lands visible; hide plays first, then hides). The value is
a handler name (`on:activate=SaveBrush` — a missing host method is a
load-time error, typos can't unwire), or a call: `play(anim)` runs a
flourish, `go(arrangement)` flips the view's pose, `toggle(arrangement)`
flips between the pose and base (press expands, press again collapses),
`emit(HostSignal)` relays to a host-declared signal.

`on:` stays OPEN: raw Godot signals are legal for anything gogo has no
word for (`on:drag_ended=`, `on:mouse_entered=`) — snake_case past tense
IS the raw register, visible at a glance. Raw spellings that duplicate a
gogo concept error with a redirect: `on:pressed` → `on:activate`,
`on:toggled`/`on:changed` → `on:change`, `on:value_changed` → `on:edit`
(live) or `on:change` (committed).

Named controls also AUTO-wire, mechanically — `On<Name><Event>`:
`okButton` + activate → `OnOkButtonActivate()`, `brushSize` + change →
`OnBrushSizeChange(value)`; raw signals follow the same rule
(`OnFooDragEnded`). A stale `On<Name>Pressed`/`Toggled`/`Changed`/
`ValueChanged`/`Activated` host method is a load-time error naming the
new spelling — a rename must never silently unwire. So `on:` is for
overrides and the declarative calls.

```
anim appear {
    panel.opacity from=0, to=1, duration=0.3, easing=cubic_out
}
Panel panel background=panel, padding=10, on:show=play(appear) {
    Button okButton text="OK", on:activate=OnConfirm
    Slider brushSize min=1, max=300, value=80, width=fill
    // auto-wired: OnBrushSizeChange(v) commits, OnBrushSizeEdit(v) live
}
```
