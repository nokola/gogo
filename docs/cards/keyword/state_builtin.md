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

`hovered`/`focused`/`focus_within`/`disabled` are universal (always
styleable, everywhere); declaring them lists them on the contract.
`held`/`selected` are capabilities — declaring them is what makes `when
held`/`when selected` legal on the control. `held` is maintained by the
runtime from the gesture bracket on the root (a behavior never flips it);
`selected` on a control with a `behavior` line DERIVES from the value —
live, as a drag crosses the midpoint.
