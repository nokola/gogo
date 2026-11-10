Visual rotation on the motion channel — degrees, clockwise, around the
node's center; rendering only. Takes no component (`rotate to=90`, never
`rotate.x`). A motion word, so an anim and the layout never share a
writer.

```
anim spin repeat=forever {
    dial.rotate from=0, to=360, duration=1, easing=linear
}
Circle dial width=40, height=40, color=accent, on:show=play(spin)
```
