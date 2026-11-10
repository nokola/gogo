Visual offset on the motion channel — moves the node's RENDERING in epx;
layout never sees it, so nothing reflows. One axis per line:
`translate.x` / `translate.y` (positive x right, positive y down). In an
anim it tweens (`from=`/`to=`); in a `when` response it holds while the
condition does.

```
anim slidein {
    card.translate.x from=-60, to=0, duration=0.3, easing=cubic_out
}
Panel card background=panel, padding=10, on:show=play(slidein) {
    Label cardText text="hi", font_size=15
}
```
