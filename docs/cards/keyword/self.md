In an animation played FOR a node — a style's `transition ... anim=x`,
a widget-scoped anim — `self` targets that node: the style can't know
its wearers' names, so `self` always means "the one this is happening
to". In plain view-level anims, target the node's real name instead.

```
style Pressy min_height=36, min_width=120 {
    transition any -> held duration=0.15, easing=cubic_out, anim=squash
}
anim squash {
    self.scale to=0.95, duration=0.15
}
Column spacing=8 {
    Button go .Pressy text="Press me"
}
```
