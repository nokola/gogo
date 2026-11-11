The sequencing connective: `then` starts this anim step AFTER the previous step — and everything `and`-joined to it — completes (autoreverse's way back included). A group placed by reference (`after=<group>`) stands outside this order: a `then` written after it waits for the step before that group, not for the group. Shift the start with `offset=` (negative overlaps the previous step's tail); a bare timed wait in the chain is the `delay=` step.

```
anim toast {
    plate.translate.y from=40, to=0, duration=0.25, easing=back_out
    then delay=2
    then plate.opacity to=0, duration=0.2
}
Panel plate background=panel, padding=10, on:show=play(toast) {
    Label plateText text="saved", font_size=14
}
```
