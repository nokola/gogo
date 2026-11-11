Transient choreography on the motion channel (`translate`/`scale`/`rotate`/`opacity` + animatable props). An anim is a CHAIN of steps: property lines, `group { }`, `play <anim>`, `each ... { }` and `delay=` pauses. Every step after the first says when it runs — `and` (with the previous step) or `then` (after it) — or carries `after=<group>` instead. Play with `Runtime::play` from code, `on:show=play(x)` or a transition's `anim=`.

```
// sidebar floats in and content fades in, then the title pops
anim appear {
    sidebar.translate.x from=-60, to=0, duration=0.5, easing=cubic_out
    and  content.opacity from=0,   to=1, duration=0.5
    then contentText.scale to=1.03, duration=0.08, autoreverse=true
}
Row strip spacing=8, on:show=play(appear) {
    Rectangle sidebar color=charcoal, width=60, height=80
    Panel content background=panel, padding=10, width=fill {
        Label contentText text="content", font_size=14
    }
}
```

```
// staggered menu entrance: slide in, springy settle, then a little pop
style MenuButton min_width=160, min_height=48
anim enter duration_each=0.4 {
    each .MenuButton stagger_by=0.1 {
        translate.x from=140, to=0, easing=back_out   // settle = overshoot
        and opacity from=0,   to=1
        then scale.x to=1.05, duration=0.08, autoreverse=true
    }
}
Column menu spacing=12, on:show=play(enter) {
    Button newGameButton .MenuButton text="New Game"
    Button settingsButton .MenuButton text="Settings"
    Button quitButton     .MenuButton text="Quit"
}
```

```
// attention pulse — autoreverse comes back to rest
anim nudge {
    saveButton.scale to=1.1, duration=0.12, autoreverse=true
}
Button saveButton text="Save", on:activate=play(nudge)
```

The anim header carries playback (`repeat=forever`, `speed=`) and honestly-named defaults: `duration_each=` is EVERY step's default duration (the word says per-step — a `then`-chain can't make it lie), `easing=` and `autoreverse=true` distribute the same way, and a line's own words always win, through any depth of `group`s. The Main Menu example (the `_example-mainmenu` card) is the staggered entrance as a complete screen.
