Seconds — a property line's OWN length. Plain `duration=` lives on lines only: on an anim or `group` header it would read as the total ("this anim is 1 second") and a `then`-chain would make that reading false, so headers refuse it and take `duration_each=` instead — the distributing default every step falls back to, nearest scope first, a line's own always winning. On a `transition` it times the flip between standing truths. A keyframed line refuses it — the last key's `t=` IS the duration.

```
anim disappear duration_each=0.25, easing=cubic_in {
    card.translate.x to=-60
    and card.opacity to=0, duration=0.4
}
Panel card background=panel, padding=10, on:hide=play(disappear) {
    Label cardText text="fades out", font_size=15
}
```
