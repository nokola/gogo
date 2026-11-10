THE root keyword — a view file opens with it: this node is the screen.
Children place freely (`halign`/`valign` + margins, no wrapper);
injects `width=fill`, `height=fill` and a transparent background unless the
file says otherwise (per axis). `background=`/`corner_radius=` draw the root
plate; `padding` insets every declared child's placement and counts into the
derived minimum.

```
View settingsSheet {
    Panel card interactive=true, margin_left=40, margin_right=40,
            valign=center, background=#1F1F1FFF, padding=12 {
        Label text="Settings", font_size=24
    }
    Button closeButton text="X", 44x44, halign=right, valign=top
}
```
