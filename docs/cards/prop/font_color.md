Text color — the text half of `background=` (what
WPF calls Foreground and CSS calls color). Pairs with `font_size`. A
`font_color` response inside `when` slides under a `transition` rule and
hands back when the condition ends. With no `font_color=` on the node it
starts from the color the text shows: the one a box above it or the
theme gives, else white.

```
Column spacing=6 {
    Label title text="Brush size", font_size=18, font_color=text
    Label hint  text="drag to adjust", font_size=13, font_color=text_muted
}
```
