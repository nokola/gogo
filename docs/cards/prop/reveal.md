The node's LAYOUT SPACE as a fraction, 0..1 — the smooth-slide primitive.
`reveal_y=0` conceals (no height earned, trailing edge clips, still alive);
a pose reveals it and the `transition` rule tweens the slide, parent
reflowing included. `reveal_x=` is the horizontal twin. Needs a container
parent; pair with `opacity` for the fade. `visibility=gone` stays the
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
