Holds while the control has focus AND an indicator is warranted (D129):
focus arrived by keyboard/gamepad, or a key was pressed while focused — a
tap/click parks engine focus *silently*, so no ring or shade is left
standing after every touch. Text fields are the exception: a clicked
`Input` always indicates (the caret is coming). This is the CSS
`:focus-visible` model with the short name — there is no separate raw
spelling; code-behind reads `has_focus()` for the raw fact.

Buttons ship a default: gogo retires Godot's always-on engine ring and
shows the theme's own ring only while `focused` holds. Authoring
`when focused` replaces that default — your response IS the focus look.

```
// gamepad menu: the focused entry pops, WP-style — replaces the ring
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
