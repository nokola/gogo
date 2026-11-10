The bare word after the type is the node's NAME — the handle everything
else targets: anim lines and top-level `when` responses (`bar.height`),
Grid `areas=` binding, and the handler its events name (a tap on
`paintButton` names `paintButton`; a build generates `Handler::PaintButton`
and `NAMES.paint_button`, so code naming a node the file no longer has does
not compile). One name per node, lowerCamel by
convention. Unnamed nodes are fine — they just can't be targeted.

```
Column spacing=8 {
    Label title text="Named nodes", font_size=15
    Rectangle bar color=teal, width=200, height=24
}
anim slide {
    bar.translate.x from=-40, to=0, duration=0.4, easing=cubic_out
}
```
