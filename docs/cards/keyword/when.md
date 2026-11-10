Standing values for as long as a condition applies. In a style or node
block the condition is a control state (`held`/`hovered`/`focused`/
`disabled`/`selected`); at the top level it's a declared arrangement (a
view pose — margin overlays there need free `Layers` children; size keys
pose any child and behave exactly as if declared, D249), a screen size
class or orientation (`when screen.landscapeSmall { ... }`,
`when screen.landscape { ... }` — see the screen card), or two composed
(`when expanded screen.landscapeSmall { ... }` arrangement first;
`when screen.landscape screen.lotsOfSpace { ... }` orientation first —
each applies only while BOTH do).

```
style ToolButton {
    when held background=white/0.13, scale=0.97
}
arrangement expanded
when expanded {
    bar.height = 90     // top level: node.prop pairs, dots from the root
}
Layers stage width=220, height=160 {
    Rectangle bar color=teal, width=200, height=50, halign=center,
        valign=top
    Button goButton .ToolButton text="expand", min_height=36,
        halign=center, valign=bottom, on:activate=go(expanded)
}
```
