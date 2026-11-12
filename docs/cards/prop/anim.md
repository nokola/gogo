On a `transition` rule: choreography played WITH the flip (`anim=pop`).
The condition ending cancels it and puts what it moved back at once.
One writer per property: an `anim=` here may not drive a property a
`when` in the same scope also holds — pick the held value or the
choreography.

```
arrangement expanded
transition base -> expanded duration=0.25, anim=pop
anim pop { card.scale to=1.05, duration=0.2 }
when expanded {
    card.height = 90
}
Layers stage width=220, height=140 {
    Panel card background=panel, padding=10, width=200, height=60,
        halign=center, valign=top {
        Label cardText text="poses", font_size=15
    }
}
```
