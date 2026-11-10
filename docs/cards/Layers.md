The depth container: children stack in z, each placed with
`halign`/`valign` + margins. Sized to its biggest child;
input-transparent by default. `padding-*` insets every child's
placement (and grows the minimum) — inner air with no `Panel`
wrapper; paint (`background`/borders) stays on `Panel`.

```
Layers padding_left=10, padding_right=10 {
    Image photo source="res://icon.svg"
    Label caption text="on top", halign=center, valign=bottom, font_size=13
}
```
