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
| `focused` | VISIBLE focus — the indicator is warranted (key-driven; a text field always indicates) | universal, dormant |
| `focus_within` | the focus is on this node or any descendant | universal, dormant |
| `selected` | "this one is chosen" — persists until the next flip | toggle-mode Button, Radio, Check, Toggle, List row root |
| `disabled` | unavailable; CASCADES down the tree, a child can't re-enable | `disabled=` |

Priority when several hold (higher wins per property): arrangements <
`focus_within` < `focused` < `hovered` < `selected` < `held` < `disabled`.

**Events (`on:`)**

| event | fires | payload |
|---|---|---|
| `activate` | the control's own trigger gesture completed — a Button's tap or Enter/Space, a List row's tap | — (List: the event's node is the row; `Runtime::item_key` gives its key) |
| `press` / `release` | a finger landed on a press surface or a Slider, and that gesture ended (exactly one `release` per `press`) | — |
| `change` | the COMMITTED value is now different — once per genuine transition a tap, a drag or a key makes; a write from code reports nothing | the new value |
| `edit` | a live, uncommitted value moved — per drag tick, per keystroke that changes the text | the live value |
| `show` / `hide` | visibility choreography — takes `play(anim)` | — |
| `pointer` | every touch phase on a box the host draws itself (needs `interactive=true`) | the phase and the position inside the box |

**Value-bearing controls**

| control | value state (`when`) | live (`on:`) | commit (`on:`) | notes |
|---|---|---|---|---|
| `Check` | `selected` | — | `change` | `bool`; every flip commits |
| `Toggle` | `selected` (flips live mid-drag) | — | `change` | `bool`; on settle, only if the side differs from before the gesture |
| `choice` | `selected` per Radio | — | `change` on the GROUP line | the picked Radio's name; items style `when selected`, never wire `change` |
| `Slider` | — (`held` is the drag) | `edit` | `change` | `float`; release commits; a keyboard step commits directly |
| `Input` | — | `edit` | `change` | `String`; Enter or focus loss commits; a text written from code reports nothing |
| `List` | `selected` on the row's `ListItem` root | — | `change` | read the keys with `Runtime::selected_keys`; `activate` fires on every row tap |

Values on an `on:` key: a handler name (`on:activate=SaveBrush`), or a
call — `play(anim)`, `go(pose)`, `toggle(pose)`, `emit(Name)`; `on:show`
and `on:hide` take `play(anim)` only (G0084).
A named node needs no `on:` line to be heard: the handler an event names
is then the node's own name (`saveButton`, `brushSize`). Any word after
`on:` that is not one of the events above is build error G0049.

```
Column spacing=8 {
    Slider brushSize min=1, max=300, value=80, width=fill,
        on:edit=PreviewBrush,      // live stream while held
        on:change=CommitBrush      // once, on release — the undo boundary
    Button saveButton text="Save"  // its tap names the handler `saveButton`
}
```

Rules of thumb: listen to `activate` on buttons, `change` on everything
else (a write from code brings no event back: the code that wrote
already knows); reach for `edit` only when you genuinely need the live
stream.
