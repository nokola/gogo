How a condition flip FEELS — duration/easing (and optionally `anim=`)
for the standing values a `when` declares. Control scope times one side
at a time; arrangements are timed at the top level.

```
style ToolButton {
    transition any -> held duration=0.05, easing=cubic_out
    transition held -> any duration=0.2,  easing=cubic_out
    when held background=white/0.13
}
arrangement expanded
transition base -> expanded duration=0.25, easing=expo_out
Button toolButton .ToolButton text="press me", min_height=36,
    on:activate=go(expanded)
```

A rule alone subscribes its endpoint state (D182) — a press pulse needs
no standing `when held` response:

```
anim nudge { self.scale to=0.95, duration=0.1, autoreverse=true }
Button saveButton text="Save" {
    transition any -> held anim=nudge
}
```
