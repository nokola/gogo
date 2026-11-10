The sequencing connective: `then` starts this anim step AFTER the previous step — and everything `and`-joined to it — completes (autoreverse's way back included). Following a referenced step (`after=`), `then` means "after that step", resolved in graph time. Shift the start with `offset=` (negative overlaps the previous step's tail); a bare timed wait in the chain is the `delay=` step.

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
