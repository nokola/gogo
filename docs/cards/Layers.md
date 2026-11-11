The depth container: children stack in z, each placed with
`halign`/`valign` + margins. Sized to its biggest child;
input-transparent by default. `padding=` (or `padding_left=` and its
three siblings) insets every child's placement (and grows the minimum)
— inner air with no `Panel` wrapper. A `background=` or a border draws
behind the children here too, as on any box.

```
Layers padding_left=10, padding_right=10 {
    Image photo source="res://icon.svg"
    Label caption text="on top", halign=center, valign=bottom, font_size=13
}
```
