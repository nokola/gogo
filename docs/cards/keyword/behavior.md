Inside a `control` block: names the INPUT BEHAVIOR that answers every
instance — the runtime routes the finger, the keys and the wheel to it and
takes back one value write per event.

```
control Switch $value=0, $min=0, $max=1 {
    behavior toggle by $value in $min..$max
    ...
}
control Vol $value=5, $min=0, $max=10, $step=0 {
    behavior slider by $value in $min..$max, step=$step, key_unit=5%
    ...
}
```

`by $value in $min..$max` is spelled in full; `step=` (0 = continuous)
and `key_unit=` (a continuous slider's key/wheel step — `5%` of the range
or a value amount) are the words. The behaviors gogo ships: `slider`
(press-jump, drag over the `track` slot, arrows/Home/End/wheel step) and
`toggle` (a tap glides to the other end, a drag settles nearest, Enter and
Space flip; reads `slot track` and `slot thumb`). A host adds its own in
Rust: a type implementing `gogo::Behavior` with a `BehaviorSpec` (the
name, the slots it reads, the key groups it owns), named to the compiler
(`CompileOptions::behaviors`) and seated on the page's `Input`
(`input.register`).

The runtime owns the law around any behavior — `held`, `edit` per changed
write, `change` on a changed release, restore on cancel, the declared
`drag_started`/`drag_ended` edges, `selected` derived from the value when
declared, direct writes while held and a glide by the `transition` rule
otherwise — so a behavior is only pointer math and never sees nodes. One
`behavior` line per control; an unknown name lists the behaviors the build
knows; a `keys` claim on a group the behavior owns refuses (G0082).
