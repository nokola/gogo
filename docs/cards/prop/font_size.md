Font size in epx. Only a box that shows text takes it (Label, Button,
Check, Toggle, Radio, Input), and text inside that box follows it: a
Label in a `Button font_size=18` draws at 18. For a whole page the
spelling is a theme entry (`theme { font_size = 15 }` in a
shared include), with a node's own value still winning where set. Without
either, text is 15 epx.

```
theme { font_size = 14 }
Column spacing=6 {
    Label title text="Brush size", font_size=18
    Label body  text="themed default size"
}
```
