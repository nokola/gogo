Text color across all state slots — the text half of `background=` (what
WPF calls Foreground and CSS calls color). Pairs with `font_size`. A
`font_color` response inside `when` works against the theme: the tween
starts from the theme's resting value and hands back when the condition
ends.

```
Column spacing=6 {
    Label title text="Brush size", font_size=18, font_color=text
    Label hint  text="drag to adjust", font_size=13, font_color=text_muted
}
```
