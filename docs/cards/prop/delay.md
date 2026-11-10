A PAUSE in an anim chain — `delay=N` is its own step, placed like any other (`then delay=0.5` waits, then waits half a second more). Always positive: a pause that starts something EARLY isn't a pause — that's `offset=` on the step you want shifted. The old delay ATTRIBUTE (on anim headers and track lines) retired with the connective grammar; a shift relative to placement is `offset=`.

```
anim reveal {
    veil.opacity to=0.4, duration=0.15
    then delay=0.5
    then card.translate.y from=-30, to=0, duration=0.25, easing=back_out
}
Layers stage width=200, height=120, on:show=play(reveal) {
    Rectangle veil color=charcoal, width=fill, height=fill
    Panel card background=panel, padding=10, halign=center, valign=center {
        Label cardText text="hello", font_size=15
    }
}
```
