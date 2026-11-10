Visual scale on the motion channel, from the node's center (pivot 0.5) —
rendering only, layout never moves. `scale=0.97` is uniform (the classic
press dip); `scale.x` / `scale.y` take one axis. In a `when` response it
holds; in an anim it tweens.

```
style Pressy {
    transition any -> held duration=0.05
    transition held -> any duration=0.2
    when held scale=0.97
}
Button okButton .Pressy text="OK"
```
