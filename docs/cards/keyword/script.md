RETIRED — the Godot addon's word: a control's behavior was a GDScript file
there. A control names its input behavior with `behavior` now:

```kdl partial
control Knob $value=0, $min=0, $max=1 {
    behavior slider by $value in $min..$max   // or a behavior of your own
    ...
}
```

A `script` line in a control is a load error pointing here (G0082); a
control that takes its input in code writes `on:pointer=` on a body node
(plus `keys` for the groups it wants).
