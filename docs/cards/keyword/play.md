One word, two registers. As a trigger value, `on:<event>=play(animName)` runs an anim when the event fires — no host handler needed; `on:show`/`on:hide` are the visibility pair (show also plays when the control LANDS visible, hide plays first and hides on finish); from code it's `Runtime::play`. As an anim STEP, `play <name>` runs another anim inline — a parallel arm gets internal sequence without nesting blocks. The played anim's internal names stay its own (never reachable from the caller), a `repeat=forever` anim can't be a step (it never completes), and its `speed=` is folded in.

```
anim badgeIntro {
    badge.translate.y from=-10, to=0, duration=0.15
    then badge.scale to=1.2, duration=0.3, autoreverse=true
}
anim panelOpen {
    panel.translate.x from=-200, to=0, duration=0.3
    and  play badgeIntro
    then panel.scale to=1.0, duration=0.1
}
Layers stage width=240, height=100, on:show=play(panelOpen) {
    Panel panel background=panel, padding=10 {
        Label panelText text="hello", font_size=14
    }
    Circle badge width=16, height=16, color=danger, halign=right, valign=top
}
```

**Stopping from code.** `Runtime::stop` ends the anim without playing it
out — it is not reported as finished (`Runtime::take_finished`) — and,
with `restore = true`, puts every property it animated back to what the
file says without the anim: the declared value, or the `when` value
holding at the time, or a finished anim's end pose the play started from
(that is a standing value the author chose, so it stays). With
`restore = false` the values stay where the stop found them.
`Runtime::is_playing`
asks. One property has one writer: while an anim plays it owns what it
animates; when it ends its end pose stands until the next writer.

```
anim pulse repeat=forever {
    dot.scale from=1, to=1.2, duration=0.4, autoreverse=true
}
style DotLook {
    when held scale=0.9
}
Layers stage2 width=120, height=60 {
    Button dotButton .DotLook width=40, height=40, halign=center, valign=center {
        Circle dot width=24, height=24, color=accent, halign=center, valign=center
    }
}
// code:  play `pulse`   the dot breathes 1 -> 1.2 -> 1 ...
//        stop `pulse`   back to scale 1 — declared, nothing held
//   (with the button held, `when held scale=0.9` is what the file says:
//   the dot lands on 0.9, not on 1)
```
