Wiring: `on:<event>=` binds a discrete moment. The events are bare stems —
`activate` (the control's trigger gesture: a Button's completed tap or
Enter/Space, a List row's tap), `press` / `release` (a finger landed on a
press surface, and that gesture ended), `change` (the committed value is
now different — once per genuine change a finger or key makes; a write
from code raises no event), `edit`
(the live uncommitted stream: Slider drag ticks, Input keystrokes that change the text), the
visibility pair `show`/`hide` (show also plays when a control lands
visible; hide plays first, then hides), and `pointer` (every touch phase
with its position, for a box the host draws itself). Any other word after
`on:` is build error G0049.

The value is a handler name (`on:activate=SaveBrush`), or a call (`on:show` and `on:hide` take `play(anim)` only, G0084):
`play(anim)` runs a flourish, `go(arrangement)` flips the view's pose,
`toggle(arrangement)` flips between the pose and base (press expands,
press again collapses), `emit(Name)` reports the event under a name you
choose.

A named control needs no `on:` line to be heard: with none, the handler an
event names is the node's own name (`okButton`, `brushSize`). So `on:` is
for another name and for the declarative calls.

```
anim appear {
    panel.opacity from=0, to=1, duration=0.3, easing=cubic_out
}
Panel panel background=panel, padding=10, on:show=play(appear) {
    Button okButton text="OK", on:activate=OnConfirm
    Slider brushSize min=1, max=300, value=80, width=fill
    // no on: line needed: its Change and Edit events name `brushSize`
}
```
