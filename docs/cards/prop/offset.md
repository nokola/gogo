A SIGNED shift on an anim step (a property line, with or without `key` children, a `group`, an `each` or a `play` step): moves the step's start relative to wherever it was placed — by a connective or by `after=`. Positive is later, negative is earlier; negative creates overlap (`then card.scale ..., offset=-0.05` bites into the previous step's tail; `after=b, offset=-0.05` into b's). One sign convention, every placement; a shifted start never goes before the anim's own start (it clamps at 0). Use negative offsets sparingly — they reintroduce read-the-numbers timing. A pause is a different word: `delay=` is a step, always positive.

```
anim landing {
    card.translate.y from=30, to=0, duration=0.3, easing=back_out
    then card.scale to=1.03, duration=0.08, autoreverse=true, offset=-0.05
}
Panel card background=panel, padding=10, on:show=play(landing) {
    Label cardText text="lands, pops early", font_size=14
}
```
