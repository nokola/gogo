Holds while the control has focus AND an indicator is warranted (D129):
focus arrived by keyboard, or a key was pressed while focused — a
tap/click moves focus *without showing it*, so no ring or shade is left
standing after every touch. Text fields are the exception: a clicked
`Input` always indicates (the caret is coming). This is the CSS
`:focus-visible` model with the short name — there is no separate raw
spelling; code asks `Input::focused()` which node holds focus.

Buttons ship a default: a ring that shows only while `focused` holds.
Authoring `when focused` replaces that default — your response IS the
focus look.

```
// keyboard menu: the focused entry pops, WP-style — replaces the ring
style MenuEntry min_height=44 {
    transition duration=0.1, easing=cubic_out
    when focused background=accent/0.3, scale=1.05
}
Column spacing=8 {
    Button startButton .MenuEntry text="Start"
    Button optionsButton .MenuEntry text="Options"
    Button quitButton .MenuEntry text="Quit"
}
```

The container flavor is `focus_within` — the row/pane lights while the
focus is anywhere inside it.
