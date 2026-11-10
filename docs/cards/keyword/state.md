Inside a `control` block: declares the control's CUSTOM states — semantics
defined by the code-behind, which raises and lowers them
(`Runtime::set_custom`).

```
control Fader $value=0, $min=0, $max=100 {
    behavior slider by $value in $min..$max
    state clipping              // code: rt.set_custom(&page, fader, "clipping", true)
    ...
}

Fader vol { when clipping opacity=0.85 }   // styled like any state
```

Built-in names (`hovered`, `held`, ...) are reserved — opting into those is
`state_builtin`. Priority when several states hold: custom states sit between
`held` and `disabled`, declaration order among themselves (later wins).

At TOP level, `state` is the retired pose directive — declare an
`arrangement` instead.
