Press / toggle. `activate` is the button's trigger gesture — a completed
tap (slide-off clears, slide-back re-arms) or Enter/Space while focused.
It reaches code as an `Activate` event; the handler it names is the
button's own name (`okButton`) unless `on:activate=` names another.
`toggle=true` gives it an on/off value: it holds `selected` while ON and
fires `change(bool)`.
Content minimums derive from free children, positive margins included
(an exact size clamps; a negative margin takes that much off the child's
claim, never below zero).

```
style ToolButton min_height=36 {
    when held background=white/0.13
    when selected background=accent    // toggle=true holds it while ON
}
Row spacing=10 {
    Button okButton .ToolButton text="OK"
    Button paintButton .ToolButton text="Paint", toggle=true
    Button colorButton .ToolButton min_width=44 {
        Rectangle swatch color=#EB6428, margin=8
    }
}
```
