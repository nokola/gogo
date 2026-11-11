Declares one named configuration of the view (drawer open, toolbar
expanded). Responses live in top-level `when <name> { }` blocks — dotted
`node.prop` pairs. Margin overlays need free (`Layers`) children; size
keys (`width`/`height`/`min_width`/`min_height`) pose ANY child and
behave exactly as if the file had declared them, for as long as the
block applies — a declared `min_width` floor still clamps every posed
width (D249). Timing comes from `transition` rules; flip with `go(name)`,
`toggle(name)` (between the arrangement and base — the disclosure button)
or `Runtime::go` from code. `base` is the predeclared declarations.

An arrangement NO `transition` rule times changes instantly. Its `when`
blocks may also re-anchor (`halign`/`valign`) and move seats
(`x.seat = <seatName>`), like a screen `when` — timed or not: under a
timed arrangement the box glides to its new anchor or its seat over the
rule's duration (the Godot addon refused both; the Rust crate accepts
them on purpose).

```
arrangement expanded
transition any -> expanded duration=0.2, easing=cubic_out
when expanded {
    bar.height = 90
}
Layers stage width=220, height=160 {
    Rectangle bar color=accent, width=200, height=50, halign=center,
        valign=top
    Button dotsButton text="⋯", halign=center, valign=bottom,
        on:activate=toggle(expanded)
}
```
