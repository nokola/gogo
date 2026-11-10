The distributing duration — on an anim or `group` header, `duration_each=N` is the default length of EACH step inside; a line's own `duration=` wins, nearest scope first. The word carries the law: it is never the container's total, so `anim x duration_each=0.2 { a then b then c }` honestly runs 0.6 seconds. Plain `duration=` on a header refuses and points here.

```
anim appear duration_each=0.3, easing=cubic_out {
    card.opacity from=0, to=1
    and card.translate.y from=12, to=0
    then card.scale to=1.02, duration=0.08, autoreverse=true
}
Panel card background=panel, padding=10, on:show=play(appear) {
    Label cardText text="three steps, one default", font_size=14
}
```
