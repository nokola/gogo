Wearer fan-out inside an `anim`: `each <node|.Style|self>` names who its
step chain drives — every wearer of a style gets its own copy (in file
order: a wearer before the wearers inside it), shifted by `stagger_by=N`.
Lines inside an each are BARE (the
each names the target; prefixing errors). With `stagger_by` > 0 the
each's length depends on how many wearers it finds at play time, so
nothing may sequence AFTER it — keep it the tail of its chain. The Main
Menu example (the `_example-mainmenu` card) adds a `then` pop for a
settle-then-pop entrance.

```
style Chip min_height=28
anim wave {
    each .Chip stagger_by=0.08 {
        group duration_each=0.2 {
            translate.y from=16, to=0, easing=back_out
            and opacity from=0,  to=1
        }
    }
}
Row spacing=6, on:show=play(wave) {
    Button aButton .Chip text="one"
    Button bButton .Chip text="two"
    Button cButton .Chip text="three"
}
```
