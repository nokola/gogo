Font size in epx — NODE-LOCAL: it styles this node's own text only and
never flows to child Labels the way WPF's inherited FontSize does. The
propagating spelling is a theme entry (`theme { font_size = 15 }` in a
shared include), with node-local values still winning where set. Without
either, text is 15 epx.

```
theme { font_size = 14 }
Column spacing=6 {
    Label title text="Brush size", font_size=18
    Label body  text="themed default size"
}
```
