The node's LAYOUT SPACE as a fraction, 0..1 — the smooth-slide primitive.
`reveal_y=0` conceals (no height earned, still alive);
a pose reveals it and the `transition` rule tweens the slide, parent
reflowing included. `reveal_x=` is the horizontal twin. It works under any
parent: in a stack the siblings close up, under `Layers` the box shows its
fraction from the edge its alignment names (`halign=right` keeps the
right end, `halign=center` the middle). What the box holds is cut at the
part it shows, and takes no touch outside it; it keeps its size and
travels with the box, so a sheet with `valign=bottom` slides out through
the bottom. Pair with `opacity` for the
fade. `visibility=gone` stays the
instant switch.

```
arrangement open
transition duration=0.2, easing=cubic_out
when open {
    hello.reveal_y = 1
    hello.opacity = 1
}
Panel box padding=8 {
    Column {
        Button more text="v", 50x50, on:activate=toggle(open)
        Label hello text="Hello, Gogo", reveal_y=0, opacity=0
    }
}
```
