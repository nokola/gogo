Three homes. On an anim line, `from=` pins the journey's start AND holds it
from play start — staggered entrances never flash their base look; `to=`
alone starts from the live value read when its STEP begins (the
interruptible-fade capture rule — a `then` leg glides from wherever the
previous leg left the property). On a `Gradient`, `from=`/`to=` are box points (0..1 of the box,
`0.5x0.5` = center) aiming the pattern. On an `Arc` — or inside a radial
kind's config, `automatic=arc(from=-225, sweep=270)` — `from=` is the
start ANGLE in degrees (0° points right, positive turns clockwise;
`sweep=` says how far it goes).

```
anim slideIn { card.translate.x from=-40, to=0, duration=0.25 }
Panel card background=panel, padding=10, on:show=play(slideIn) {
    Label cardText text="slides in", font_size=15
}
```

```
Gradient beam fill=radial, from=0x0.5, to=1x0.5,
    stops=[accent@0, accent/0@1], width=120, height=40
```
