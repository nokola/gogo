An anim step block with two jobs, neither of them sequencing: it DISTRIBUTES step words (`duration_each=`, `easing=`, `autoreverse=`) to the chain inside — a line resolves each word from the nearest enclosing scope that sets it (anim header included), a line's own always winning — and it CARRIES the name `after=` references. `duration_each` says what it means: each step's default, never the group's total. Names are opt-in: only a referenced group needs one. A group contains its own `and`/`then` chain; several properties running together is a chain of `and`s, never a bare block.

What a property line can drive:

| target | drives |
|---|---|
| `translate.x` / `translate.y` | visual offset in epx, one axis per line |
| `scale` | uniform scale from the center |
| `scale.x` / `scale.y` | one-axis scale |
| `rotate` | degrees, around the center |
| `opacity` | whole-node fade, 0..1 |
| `background`, `font_color`, … | the words a `transition` rule can time: the color words (whole, or `.a` for the alpha alone), `corner_radius`, `border_thickness`, `font_size`, a Gradient's `angle` and its live stops, and `width`, `height`, `min_width`, `min_height`, the `margin_*` and the `padding_*` sides (these move the box, which is laid out again on every frame). Not every number: `spacing=` is refused (G0063) |

An anim line on a size, a margin or a padding (`card.width to=200`) moves the box itself, and the box and what is around it are laid out again on every frame of the play. Where only the picture has to move, use `scale` or `translate`. A pose that changes and STAYS is arrangement + `when` + `transition` territory.

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
