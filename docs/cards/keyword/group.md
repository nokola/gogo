An anim step block with two jobs, neither of them sequencing: it DISTRIBUTES step words (`duration_each=`, `easing=`, `autoreverse=`) to the chain inside — a line resolves each word from the nearest enclosing scope that sets it (anim header included), a line's own always winning — and it CARRIES the name `after=` references. `duration_each` says what it means: each step's default, never the group's total. Names are opt-in: only a referenced group needs one. A group contains its own `and`/`then` chain; several properties running together is a chain of `and`s, never a bare block.

What a property line can drive:

| target | drives |
|---|---|
| `translate.x` / `translate.y` | visual offset in epx, one axis per line |
| `scale` | uniform scale from the center |
| `scale.x` / `scale.y` | one-axis scale |
| `rotate` | degrees, around the center |
| `opacity` | whole-node fade, 0..1 |
| `background`, `font_color`, … | any numeric/color property nothing else writes |

Layout geometry (`width`, `margin`, …) is fenced — an anim would fight the layout engine; a pose that changes and STAYS is arrangement + `when` + `transition` territory.

```
anim open {
    group dim { veil.opacity to=0.5, duration=0.15 }
    then card.translate.y from=-30, to=0, duration=0.25, easing=back_out
}
Layers stage width=200, height=120, on:show=play(open) {
    Rectangle veil color=charcoal, width=fill, height=fill
    Panel card background=panel, padding=10, halign=center, valign=center {
        Label cardText text="hi", font_size=15
    }
}
```
