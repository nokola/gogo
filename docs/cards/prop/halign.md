WHERE the control sits on x: `left` / `center` / `right` — position only,
sizing stays with `width=`. `halign=center, width=fill` is an error (fill
leaves nothing to position — use hug), UNLESS the fill is capped: on a
`width=fill, max_width=380` axis the side says where the box sits when
the cap leaves space over, and with no side it is centered (D245, see
`max_width`). Don't confuse with
`horizontal_alignment`, which places TEXT inside a Label's own box.

```
Column spacing=8, width=220 {
    Button leftButton   text="left",   halign=left
    Button centerButton text="center", halign=center
    Button rightButton  text="right",  halign=right
}
```
