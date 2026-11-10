The on/off switch. Gogo-drawn always (there is no native Godot toggle
switch), so it is fully restylable from day one: `for=Toggle` styles reach
its `track` and `thumb` slots, and its states (`selected`, `held`, plus
the universal hovered/focused/disabled) respond in styles.

```
Toggle mute value=1, on:change=OnMuteChange
```

Under the hood one number is the truth: `value` runs 0..1, the thumb follows
it, and `selected` flips as it crosses the middle — live while a finger
drags (`held` holds through the drag). A tap glides to the other end; a
drag settles on the side it lands. `change(bool)` fires on settle, and
only if the value differs from before the gesture — wiggling across the
middle is zero changes; state is live presentation, `change` is
commitment. Code-behind reads/writes the `selected` bool (or `value`);
`change` auto-wires as `On<Name>Change`.

```
style BigSwitch for=Toggle, slot.track.corner_radius=4 {
    when selected slot.track.background=green
}
Toggle wifi .BigSwitch
```

`render=` means nothing here: `render=gogo` is redundant (and silent),
`render=native` errors — there is no native backend to opt back into.
