`.Style` after the type (and optional name) dresses the node in that
style's property bag. Local properties win over the style's; several
(`.A .B`) apply left to right. A style carries `when` responses and
`transition` rules along with plain props — the wearer gets those too.
Ctrl+click the token jumps to the definition.

```
style Chip background=#2E4053, corner_radius=12, min_height=36 {
    when held background=#3A5268
}
Row spacing=8 {
    Button one .Chip text="One"
    Button two .Chip text="Two"
}
```
