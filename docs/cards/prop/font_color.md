Text color — the text half of `background=` (what
WPF calls Foreground and CSS calls color). Pairs with `font_size`. A
`font_color` response inside `when` tweens from the node's own
`font_color=` and hands back when the condition ends; with none written
on the node the text switches color at once.

```
Column spacing=6 {
    Label title text="Brush size", font_size=18, font_color=text
    Label hint  text="drag to adjust", font_size=13, font_color=text_muted
}
```
