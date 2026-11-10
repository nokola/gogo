## States and events

Two slots, two axes: `when <state>` styles a CONDITION while it holds and
restores when it ends; `on:<event>=` answers a MOMENT. States are past
participles (`held`, `selected`), events are bare verbs (`activate`,
`change`) — the spelling tells you the axis from across the room.

**States (`when`)**

| state | meaning | provided by |
|---|---|---|
| `hovered` | the pointer is over the control; a press CAPTURES it (slipping off mid-press keeps the look until release) | universal, dormant |
| `held` | input is down on this control; IS the drag state on Slider and Toggle | press surfaces only: Button family, `ListItem` row, Slider, `state_builtin held` controls |
| `focused` | VISIBLE focus — the indicator is warranted (key/gamepad-driven; a text field always indicates) | universal, dormant |
| `focus_within` | the focus is on this node or any descendant | universal, dormant |
| `selected` | "this one is chosen" — persists until the next flip | toggle-mode Button, Radio, Check, Toggle, List row root |
| `disabled` | unavailable; CASCADES down the tree, a child can't re-enable | `disabled=` |

Priority when several hold (higher wins per property): arrangements <
`focus_within` < `focused` < `hovered` < `selected` < `held` < `disabled`.

**Events (`on:`)**

| event | fires | payload |
|---|---|---|
| `activate` | the control's own trigger gesture completed — a Button's tap or Enter/Space, a List row's tap | — (List: the row key + item) |
| `change` | the COMMITTED value is now different — once per genuine transition, programmatic writes included | the new value |
| `edit` | a live, uncommitted value moved — per drag tick, per keystroke | the live value |
| `show` / `hide` | visibility choreography — takes `play(anim)` | — |

**Value-bearing controls**

| control | value state (`when`) | live (`on:`) | commit (`on:`) | notes |
|---|---|---|---|---|
| `Check` | `selected` | — | `change` | `bool`; every flip commits |
| `Toggle` | `selected` (flips live mid-drag) | — | `change` | `bool`; on settle, only if the side differs from before the gesture |
| `choice` | `selected` per Radio | — | `change` on the GROUP line | the picked Radio's name; items style `when selected`, never wire `change` |
| `Slider` | — (`held` is the drag) | `edit` | `change` | `float`; release commits; a keyboard step commits directly |
| `Input` | — | `edit` | `change` | `String`; Enter or focus loss commits; programmatic `text=` reports too |
| `List` | `selected` on the row's `ListItem` root | — | `change` | the selected key(s); `activate(key, item)` fires on every row tap |

Values on an `on:` key: a handler name (`on:activate=SaveBrush`), or a
call — `play(anim)`, `go(pose)`, `toggle(pose)`, `emit(HostSignal)`.
Named nodes auto-wire mechanically: `On<Name><Event>` — `saveButton` →
`OnSaveButtonActivate()`, `brushSize` → `OnBrushSizeChange(value)`. Raw
Godot signals stay wireable for anything gogo has no word for
(`on:drag_ended=`, `on:mouse_entered=`) and auto-wire the same way.

```
Column spacing=8 {
    Slider brushSize min=1, max=300, value=80, width=fill,
        on:edit=PreviewBrush,      // live stream while held
        on:change=CommitBrush      // once, on release — the undo boundary
    Button saveButton text="Save"  // auto-wires OnSaveButtonActivate()
}
```

Rules of thumb: listen to `activate` on buttons, `change` on everything
else (it also catches programmatic updates); reach for `edit` only when
you genuinely need the live stream.
