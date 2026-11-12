On an `each` block: shifts successive wearers' copies of its step chain
by N seconds (in file order) — the cascading-entrance one-liner. Pair with
a pinned `from=` so waiting wearers hold their start pose instead of
flashing the base look.

```
style MenuItem min_height=32
anim cascade {
    each .MenuItem stagger_by=0.1 {
        group duration_each=0.2 {
            translate.y from=24, to=0, easing=back_out
            and opacity from=0,  to=1
        }
    }
}
Column menu spacing=6, on:show=play(cascade) {
    Button aButton .MenuItem text="New"
    Button bButton .MenuItem text="Open"
    Button cButton .MenuItem text="Save"
}
```
