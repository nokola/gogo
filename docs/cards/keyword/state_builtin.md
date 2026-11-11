Inside a `control` block: opts into RUNTIME-MAINTAINED states from the
built-in set — `hovered`, `focused`, `focus_within`, `disabled`, `held`,
`selected`. The runtime maintains them (hover's touch rules, focus order,
input blocking); nothing but the runtime writes them.

```kdl partial
control Knob $value=0, $min=0, $max=1 {
    behavior slider by $value in $min..$max
    state_builtin hovered, focused, disabled   // focused: keyboard needs it
    ...
}
```

A `when` on a control can name only what this line lists: any of the
six the line leaves out is a build error (G0036), on an instance and in
a style it wears — `hovered`, `focused`, `focus_within` and `disabled`
included. Listing `focused` also makes the control reachable by Tab and
the arrows. `held` is maintained by the
runtime from the gesture bracket on the root (a behavior never flips it);
`selected` on a control with a `behavior` line DERIVES from the value —
live, as a drag crosses the midpoint.
