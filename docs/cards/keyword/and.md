The joining connective: `and` runs this anim step TOGETHER with the previous one — same start. A run of `and`-joined steps is a CLUSTER; a following `then` waits for every end in it. The first step of a chain stands alone (nothing before it to join), and a second step with no connective is an error — sequencing is always written, never defaulted.

```
anim appear {
    card.translate.y from=40, to=0, duration=0.2
    and  card.opacity from=0, to=1, duration=0.3
    then card.scale.x to=1.05, duration=0.08, autoreverse=true
}
Panel card background=panel, padding=10, on:show=play(appear) {
    Label cardText text="slide and fade, then pop", font_size=14
}
```
