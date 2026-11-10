The label/button/input literal. A `/key` suffix marks a translation term:
the literal is the English source text, the key indexes the locale tables
(`text="new stroke"/newStroke`). Escapes: `\"` `\\` `\n` `\t`. Text
inside a Label's own box aligns with `horizontal_alignment=` /
`vertical_alignment=` (not `halign=`, which places the control).

```
Column spacing=6 {
    Label greeting text="Hello, Gogo", font_size=16
    Button okButton text="OK", min_width=100
}
```
