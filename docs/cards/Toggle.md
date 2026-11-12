The on/off switch. Its states (`selected`, `held`, plus the universal
hovered/focused/disabled) respond in styles.

```
Toggle mute value=1, on:change=OnMuteChange
```

Under the hood one number is the truth: `value` runs 0..1, the thumb follows
it, and `selected` flips as it crosses the middle — live while a finger
drags (`held` holds through the drag, off the Toggle's box too). A tap
glides the knob to the other end in 0.12 s; a
drag settles on the side it lands. `change(bool)` fires on settle, and
only if the value differs from before the gesture — wiggling across the
middle is zero changes; state is live presentation, `change` is
commitment. The `Change` event carries the new side as a bool; code
writes the value with `Runtime::set_value`.

```
style DimWhenOff for=Toggle {
    when selected opacity=1
    when disabled opacity=0.3
}
Toggle wifi .DimWhenOff opacity=0.7
```

A `transition` line in the Toggle's block sets the knob's time
(`duration=0` makes it jump); a key and a write from code move the knob
the same way a tap does:

```
Toggle slowSwitch text="Slow" {
    transition duration=0.3, easing=cubic_out
}
```

Its text takes `wrap=true`, in the width the pill leaves.

The pill and the knob take no restyle today: a `slot.` path on a Toggle is
build error G3002. A switch with its own look is a `control` with
`behavior toggle` over a `slot track` holding a `slot thumb`.
